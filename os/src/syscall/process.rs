//! Process management syscalls
use crate::task::{change_program_brk, exit_current_and_run_next, suspend_current_and_run_next,TASK_MANAGER};
use crate::mm::{translated_byte_buffer,VirtAddr,VPNRange,FrameTracker,frame_alloc,PTEFlags};
use crate::config::PAGE_SIZE;
use crate::timer::get_time_us;
use alloc::vec::Vec;//Vec是标准库的

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}
///封装函数
///内核从token代表的用户空间里面 user_ptr所指的位置 T类型长度的字节 个数的数据
//  读出数据存入vec
fn read_user_data<T>(token:usize,user_ptr:*const T)->Result<Vec<u8>,()>{
    let user_buffers=translated_byte_buffer(
        token,
        user_ptr as *const u8,
        core::mem::size_of::<T>()
    );
    if user_buffers.is_empty() {
        return Err(())
    }
    let mut res=Vec::new();
    for buffer in user_buffers {
        //extend_from_slice() 是 Rust 中 Vec 的方法，
        // 它的作用是将一个切片的所有元素追加到向量的末尾。
        res.extend_from_slice(buffer);
    }
    let len=core::mem::size_of::<T>();
    if res.len()==len {
        Ok(res)
    }else{
        Err(())
    }

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
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(_ts: *mut TimeVal, _tz: usize) -> isize {
    //用户传递裸指针相当于虚拟地址进来  之前可以直接解引用写入值
    //现在在内核虚拟空间 页表是内核的 则通过全局实例TASK_MANAGER得到任务控制块 
    // 再由get_user_token(&self)  得到用户token 即页表基址
    //或者由任务控制块得到用户地址空间 再直接拿页表
    //直接想拿物理地址写入是错误的 引入虚存后 cpu只能通过虚存访问
    trace!("kernel: sys_get_time");
    let token={
    let inner=TASK_MANAGER.inner.exclusive_access();
    let cur_task_id=inner.current_task;
    inner.tasks[cur_task_id].memory_set.token()
    };
    // let memory_set=TASK_MANAGER.tasks[cur_task_id].memory_set;
    // let page_table=memory_set.page_table;
    // let token=inner.tasks[cur_task_id].get_user_token();
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

/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(_trace_request: usize, _id: usize, _data: usize) -> isize {
    trace!("kernel: sys_trace");
    println!("[trace] req={} id={}", _trace_request, _id);
    //_id  必须落在 用户态能看见的虚拟地址区间；
    const USER_TOP: usize = 0x8000_0000;
    if _id >= USER_TOP {
        return -1
    }
    let (token,readable,writable,cur_task_id)={
    let inner=TASK_MANAGER.inner.exclusive_access();
    let cur_task_id=inner.current_task;
    let memory_set=&inner.tasks[cur_task_id].memory_set;
    let token=memory_set.token();
    let page_table=&memory_set.page_table;
    let va=VirtAddr::from(_id);//将usize转换为虚拟地址
    let vpn=va.floor();//转化为虚拟页号
    let Some(entry)=page_table.find_pte(vpn) 
        else{
            return -1  //地址不可见  页表中没有这一项
        };
    let mut readable=true;
    let mut writable=true;
    if !entry.readable(){
                //不可读
                readable=false;
            }
    if !entry.writable(){
                //不可写
                writable=false;
            }
    (token,readable,writable,cur_task_id)
    };
    //获取页表项entry  它可以帮忙检查用户地址是否可读/可写
    match _trace_request{
        0=>{
            if !readable{
                //不可读
                return -1
            }
            if let Ok(v)=read_user_data(token,_id as *const u8){
                return v[0] as isize;
            }
            -1
        }
        1=>{
             if !writable{
                //不可写
                return -1
            }
            let byte=_data as u8;//将data按位截断成一个字节  截最低8位
            match write_user_data(token,_id as *mut u8,&byte){
                Ok(())=>{0}
                Err(())=>{-1}
            }
        }
        2=>{
            let inner=TASK_MANAGER.inner.exclusive_access();
            inner.tasks[cur_task_id].syscall_cnt[_id]
        }
        _=>-1
    }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    if _start & (PAGE_SIZE-1) !=0 || _port & !0x7 != 0 ||_port & 0x7 == 0 {
        return -1
    }//按页大小对齐 即 需要地址页内偏移为0
    if _len ==0 {return 0}
    // let (start_vpn,end_vpn,vpn_range_len)={
    let mut inner=TASK_MANAGER.inner.exclusive_access();
    let cur_task_id=inner.current_task;
    let memory_set=&mut inner.tasks[cur_task_id].memory_set;
    //构造虚拟地址 并拿到虚拟页号
    let start_va=VirtAddr(_start);
    let  mut end_va=VirtAddr(_start+_len-1);
    if (_start+_len-1)%PAGE_SIZE==0{
         end_va=VirtAddr(_start+_len);
    }
    let start_vpn=start_va.floor();
    let end_vpn=end_va.ceil();
    // let end_vpn=VirtAddr(_start.saturating_add(_len)).ceil();
    //计算vpn_range迭代器长度
    let vpn_range_len=end_vpn.0-start_vpn.0;
    //查看是否有映射
    {
    let vpn_range=VPNRange::new(start_vpn,end_vpn);
    for vpn in vpn_range {
        let mapped=memory_set.mapped(vpn);
        println!("[mmap DEBUG] checking vpn {:?}: mapped = {}", vpn, mapped);
            if mapped {
                println!("[mmap DEBUG] VA range: {:#x} - {:#x}", start_va.0, end_va.0);
                println!("[mmap DEBUG] VPN range: {:?} - {:?}, pages={}", start_vpn.0, end_vpn.0, vpn_range_len);
                println!("[mmap] vpn {:?} already mapped!", vpn);
                return -1;
            }
    }
}
    // (start_vpn,end_vpn,vpn_range_len)
    // };
    //分配物理内存  frames存物理页帧 预分配容量
    let mut frames:Vec<FrameTracker>=Vec::with_capacity(vpn_range_len);
    //虽然申请长度为len字节 但是物理内存是整页整页分配的
    // //调试
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
   
    // let mut inner = TASK_MANAGER.inner.exclusive_access();
    // let cur_task_id=inner.current_task;
    // let memory_set = &mut inner.tasks[cur_task_id].memory_set;
    let page_table = &mut memory_set.page_table;
    {
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
    // println!("[DEBUG] mmapOK!");
    0
}
// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    // let (start_vpn,end_vpn)={
    if _start & (PAGE_SIZE-1) !=0{return -1}
    let mut inner=TASK_MANAGER.inner.exclusive_access();
    let cur_task_id=inner.current_task;
    let memory_set=&mut inner.tasks[cur_task_id].memory_set;
    //构造虚拟地址 并拿到虚拟页号
    let start_va=VirtAddr(_start);
    let mut  end_va=VirtAddr(_start+_len-1);
    if (_start+_len-1)%PAGE_SIZE==0{
        end_va=VirtAddr(_start+_len);
    }
    let start_vpn=start_va.floor();
    // let end_vpn=VirtAddr(_start.saturating_add(_len)).ceil();
    let end_vpn=end_va.ceil();
    
    let vpn_range_len=end_vpn.0-start_vpn.0;
    
    {
    
    //查看是否有映射
    let vpn_range=VPNRange::new(start_vpn,end_vpn);
    for vpn in vpn_range {
        if !memory_set.mapped(vpn){
            //调试
            println!("[munmap] VA range: {:#x} - {:#x}", start_va.0, end_va.0);
            println!("[munmap] VPN range: {:?} - {:?}, pages={}", start_vpn.0, end_vpn.0, vpn_range_len);
            println!("[munmap] vpn {:?} not already mapped!", vpn);
            return -1
        }
        // }else{
        //     page_table.unmap(vpn);//从页表里取消映射  //边判断映射边取消 
        // }
    }
}
    // (start_vpn,end_vpn)
    // };
    
    
    // let mut inner = TASK_MANAGER.inner.exclusive_access();
    // let cur_task_id=inner.current_task;
    // let memory_set = &mut inner.tasks[cur_task_id].memory_set;
    
    {
    let page_table=&mut memory_set.page_table;
    let vpn_range=VPNRange::new(start_vpn,end_vpn);
     //调试
    println!("[mumap] mumapping vpn range: {:?}..{:?}", start_vpn, end_vpn);
    for vpn in vpn_range{
        page_table.unmap(vpn);//从页表里取消映射
    }
}

    0
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
