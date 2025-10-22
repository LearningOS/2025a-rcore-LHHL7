//! Process management syscalls
//!
use alloc::sync::Arc;
use alloc::vec::Vec;
use crate::{
    loader::get_app_data_by_name,
    fs::{open_file, OpenFlags},
    mm::{translated_refmut, translated_str,translated_byte_buffer,VirtAddr,VPNRange,FrameTracker,frame_alloc,PTEFlags},
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next,TaskControlBlock,BIG_STRIDE
    },
    timer::get_time_us,
    config::PAGE_SIZE,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}
///由内核向token代表的用户空间 user_ptr指向的位置写入data
fn write_user_data<T>(token:usize,user_ptr:*mut T,data: &T)->Result<(),()>{
        //返回Result<(),()> 代表只关心是ok还是err  对变体具体值不关心 相当于bool 且还有一些bool没有的优点
    //将数据转化为字节数组
    //此时最好将time_val实例数据变成一个字节切片
    //这会让数据复制时方便 并且处理跨页情况轻松
    //core::slice::from_raw_parts 内部实际上会解引用裸指针来创建切片，这是不安全的操作。
    let data_bytes=unsafe{
        core::slice::from_raw_parts(data as *const T as *const u8,
        core::mem::size_of::<T>()
        )//接受ptr和len
    };
    //总结：此时我想将数据转化为字节数组 那么ptr就需要为字节的指针类型  
    // 而data此时是T的引用 不能直接转化为字节指针 那么先转化为裸指针 再转化
    //因为普通的类型引用 记录着内存地址 类型大小等信息 不能直接转化为另一个类型引用
    //裸指针只记录着内存地址
    //此时思路借助translated_byte_buffer  
    // 将用户空间的ts指向的长度为timeval结构体长度的那块区域 变成内核可访问的
    //函数返回一个字节数组  向它写数据就相当于向用户空间写
    let user_buffers=translated_byte_buffer(
        token,
        user_ptr as *const u8,
        core::mem::size_of::<T>()
    );//将用户空间这块区域变成字节切片
     if user_buffers.is_empty() {
        return Err(())
    }
    let mut total_copied=0;
    for buffer in user_buffers {
        let to_copied=data_bytes.len()-total_copied;
        if to_copied ==0{
            break;
        }
        let copy_len=to_copied.min(buffer.len());
        //循环将数据写入字节切片
        for i in 0..copy_len {//索引区间可以省  0 ，for 循环区间不能省。
            buffer[i]=data_bytes[total_copied+i];
        }
        total_copied+=copy_len;
    }
    let len=core::mem::size_of::<T>();
    if len==total_copied {
         Ok(())
    }else{
        Err(())
    }
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    //trace!("kernel: sys_waitpid");
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    // let token={
    // let inner=TASK_MANAGER.inner.exclusive_access();
    // let cur_task_id=inner.current_task;
    // inner.tasks[cur_task_id].memory_set.token()
    // };
    let token=current_user_token();
    //获取时间
    let us=get_time_us();
    let time_val=TimeVal{
        sec:us/1_000_000,
        usec:us%1_000_000,
    };
    let data=&time_val;
    match write_user_data(token,_ts,data){
        Ok(())=>0,
        Err(())=>-1,
    }
}

/// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
     if _start & (PAGE_SIZE-1) !=0 || _port & !0x7 != 0 ||_port & 0x7 == 0 {

        return -1
    }//按页大小对齐 即 需要地址页内偏移为0
    if _len ==0 {return 0}
    
    // let mut inner=TASK_MANAGER.inner.exclusive_access();
    // let cur_task_id=inner.current_task;
    // let memory_set=&mut inner.tasks[cur_task_id].memory_set;
    //拿地址空间 查/插映射都要用
    let task=current_task().unwrap();
    let mut inner=task.inner_exclusive_access();
    let memory_set=&mut inner.memory_set;
    //构造虚拟地址 并拿到虚拟页号
    let start_va=VirtAddr(_start);
    let  mut end_va=VirtAddr(_start+_len-1);
    if (_start+_len-1)%PAGE_SIZE==0{
         end_va=VirtAddr(_start+_len);
    }
    let start_vpn=start_va.floor();
    let end_vpn=end_va.ceil();
    //计算vpn_range迭代器长度
    let vpn_range_len=end_vpn.0-start_vpn.0;
    //查看是否有映射
    {
    let vpn_range=VPNRange::new(start_vpn,end_vpn);
    for vpn in vpn_range {
        match memory_set.translate(vpn){
            Some(_)=>{return -1}
            None=>{continue;}
        }
    }
    }
    //分配物理内存  frames存物理页帧 预分配容量
    let mut frames:Vec<FrameTracker>=Vec::with_capacity(vpn_range_len);
    //虽然申请长度为len字节 但是物理内存是整页整页分配的
    // 调试
    // println!("[mmap] need {} frames, capacity={}", vpn_range_len, frames.capacity());
    for _ in 0..vpn_range_len {
        match frame_alloc(){//分配物理页帧 返回一个option
            Some(f)=>{ frames.push(f);}
            None=>{
                drop(frames);
                return -1}//物理内存不足
        }
    }
    //构造权限
    // let perm=MapPermission::U;//用户态
    // //&1 就代表跟1按位与 得到最低1位
    let mut flags=PTEFlags::V;//硬件页表项标志 先置有效位
    flags|=PTEFlags::U;
    if _port & 1 !=0 {flags|=PTEFlags::R;}
    if _port & 2 !=0 {flags|=PTEFlags::W;}
    if _port & 4 !=0 {flags|=PTEFlags::X;}
    //将映射写入页表
    {
    let page_table = &mut memory_set.page_table;
    let vpn_range = VPNRange::new(start_vpn, end_vpn);
    //调试
    println!("[mmap] mapping vpn range: {:?}..{:?}", start_vpn, end_vpn);
    for (i,vpn) in vpn_range.into_iter().enumerate() {
        // enumerate()  就是在原有迭代器的基础上，
        // 再配一个从 0 开始递增的索引，
        // 把每个元素包装成  (索引, 元素)  的元组。
        let ppn=frames[i].ppn;//得到每个页帧的物理页号
        println!("[mmap] mapping vpn:{:?}",vpn.0);
        page_table.map(vpn,ppn,flags);
    }
    drop(frames);
    }
    0
}

/// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    if _start & (PAGE_SIZE-1) !=0{return -1}
    //拿地址空间 查/插映射都要用
    let task=current_task().unwrap();
    let mut inner=task.inner_exclusive_access();
    let memory_set=&mut inner.memory_set;
    //构造虚拟地址 并拿到虚拟页号
    let start_va=VirtAddr(_start);
    let mut  end_va=VirtAddr(_start+_len-1);
    if (_start+_len-1)%PAGE_SIZE==0{
        end_va=VirtAddr(_start+_len);
    }
    let start_vpn=start_va.floor();
    let end_vpn=end_va.ceil();
    // let vpn_range_len=end_vpn.0-start_vpn.0;
    {
    //查看是否有映射
    let vpn_range=VPNRange::new(start_vpn,end_vpn);
    for vpn in vpn_range {
       match memory_set.translate(vpn){
            Some(_)=>{continue;}
            None=>{return -1}//若无映射 那么失败
        }
        // }else{
        //     page_table.unmap(vpn);//从页表里取消映射  //边判断映射边取消 
        // }
    }
    }
    {
    let page_table=&mut memory_set.page_table;
    let vpn_range=VPNRange::new(start_vpn,end_vpn);
     //调试
    for vpn in vpn_range{
        page_table.unmap(vpn);//从页表里取消映射
    }
    }
    0
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    //无效的文件名
    let data={
        let token=current_user_token();
        let path=translated_str(token,_path);
        get_app_data_by_name(path.as_str()).unwrap()
        
    };
    //新建子进程
    let children=Arc::new(TaskControlBlock::new_empty());
     //执行目标程序
    children.exec(data);
    //维护父子关系
    let parent_task=current_task().unwrap();
    {
        let mut parent_inner=parent_task.inner_exclusive_access();
    //current_task()  返回  Option<Arc<TaskControlBlock>> 要加个unwrap 
    parent_inner.children.push(children.clone());
    }
    {
        let mut children_inner=children.inner_exclusive_access();
    children_inner.parent=Some(Arc::downgrade(&parent_task));
    }
   
    // 关键：把子进程扔进就绪队列
    add_task(children.clone());
    return children.pid.0 as isize;
}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(_prio: isize) -> isize {
    //进程优先级大于等于2
    if _prio<=1 {
        println!("[Error] priority:{} set fault!",_prio);
        return -1;
    }
    let task=current_task().unwrap();//先绑定再借用
    let mut inner=task.inner_exclusive_access();
    //  如果连起来current_task().unwrap()产生一个临时值 借用完就销毁
    //  那么inner_exclusive_access()变悬空指针
    inner.prio=_prio as usize;
    inner.pass=BIG_STRIDE/inner.prio;
    _prio
}
// pub fn sys_linkat(oldpath:*const u8,newpath:*const u8)->isize{
//     //将裸指针转化为字符串
//     let token=current_user_token();
//     let oldpath_str=translated_str(token,oldpath);
//     let newpath_str=translated_str(token,newpath);
//     //若重名则错误
//     if oldpath_str==newpath_str {
//         return -1;
//     }
//     //先找原inode
//     let old_inode=ROOT_INODE.find(oldpath_str).unwrap();
//     //然后在根目录下插入目录项
//     //先获取disk inode可变引用
//     ROOT_INODE.modify_disk_inode(|root_dinode| {
//         //先对根节点扩容
//         //计算根节点有多少个目录项
//         let file_count=root_dinode.size as usize /DIRENT_SZ;
//         let new_size=(file_count+1)*DIRENT_SZ;//这里分两步计算是为了得到filecount 后面要用
//         let fs=ROOT_INODE.fs.lock();//fs外面包一层mutex 要用lock拆
//         ROOT_INODE.increase_size(new_size as u32,root_dinode,&mut fs);
//         //用old_inode 新建一个目录项
//         let old_inode_number=old_inode.read_disk_inode(|old_diskinode|{
//             old_inode.find_inode_id(oldpath_str,old_diskinode).unwrap()});
//         let new_dir=DirEntry::new(newpath_str,old_inode_number);
//         //往目录写目录项 (若是写文件内容 用内存索引节点write_at)
//         root_dinode.write_at(file_count*DIRENT_SZ,new_dir.as_bytes(),&BLOCK_DEVICE);
//     })
//     //维护硬链接数
//     //同步回diskinode
//     old_inode.modify_disk_inode(|old_diskinode|{
//         old_diskinode.nlink+=1
//         // 闭包里我们已独占整个 block cache，可以安全地改内存
//         unsafe{
//             Arc::get_mut_unchecked(&mut old_inode).nlink=old_diskinode.nlink;
//     });
//     0
// }
// pub fn sys_unlinkat(path:*const u8)->isize{
//     let token=current_user_token();
//     let file=translated_str(token,path);
//     if ROOT_INODE.find(file).is_none() {
//         //文件不存在
//         return -1;
//     }
//     //删除目录项
//         //先找目录项的idx
//     let need_clear=ROOT_INODE.modify_disk_inode(|root_dinode|{
//         let file_count=root_dinode.size as u32 /DIRENT_SZ;
//         let mut idx=0;
//         for i in 0..file_count {
//             //每次循环读一个目录项到缓冲区de
//             let mut de=DirEntry::new();
//             root_dinode.read_at(i*DIRENT_SZ,de.as_bytes_mut(),&BLOCK_DEVICE);
//             if de.name==file{
//                 idx=i;
//                 break;
//             }
//         }
//         if idx != file_count-1{
//         //读最后一个目录项
//         let last_de=DirEntry::new();
//         root_dinode.read_at((file_count-1)*DIRENT_SZ,last_de.as_bytes_mut(),&BLOCK_DEVICE);
//         //然后覆盖掉idx那条目录项
//         root_dinode.write_at(idx*DIRENT_SZ,last_de.as_bytes(),&BLOCK_DEVICE);
//         }
//         //修改size
//         let new_size=(file_count-1)*DIRENT_SZ;
//         root_dinode.size=new_size as u32;
//         //inode--
//         let inode=ROOT_INODE.find(file).unwrap();
//         inode.modify_disk_inode(|dinode| {
//             dinode.nlink-=1;
//             unsafe{
//             Arc::get_mut_unchecked(&mut inode).nlink=dinode.nlink;
//             dinode.nlink==0
//     }
//         })
//     });
//     //  若发现inode现在为0 则需要回收
//     if need_clear {
//         //清空文件内容 回收数据块
//         let inode=ROOT_INODE.find(file).unwrap();
//         inode.modify_disk_inode(|dinode|{
//             //此时清空文件内容
//             let data_blocks=dinode.clear_size(&BLOCK_DEVICE);
//             let fs=inode.fs.lock();
//             //在位图上对应位置置0 
//             for b in data_blocks {
//                 fs.dealloc_data(b);
//             }
//         })
//     }
// }
// pub fn sys_fstat(fd:i32,st:*mut Stat)->isize{
//     //通过fd来找文件
//     let token=current_user_token();
//     let task=current_task().unwrap();
//     let inner=task.acquire_inner_lock()

// }