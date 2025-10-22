//! File and filesystem-related syscalls
use crate::fs::{open_file, OpenFlags, Stat,ROOT_INODE,DIRENT_SZ,OSInode,StatMode};
use crate::mm::{translated_byte_buffer, translated_str, UserBuffer,translated_refmut};
use crate::task::{current_task, current_user_token};
use crate::drivers::BLOCK_DEVICE;
use easy_fs::DirEntry;
// use alloc::sync::Arc;
// use easy_fs::Inode;
use easy_fs::{DiskInode,BLOCK_SZ};
///
pub fn sys_write(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_write", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        if !file.writable() {
            println!("[sys_write] ERROR: fd {} not writable", fd);
            return -1;
        }
        let file = file.clone();
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        let res=file.write(UserBuffer::new(translated_byte_buffer(token, buf, len)));
        res as isize
    } else {
        println!("[sys_write] ERROR: fd {} is None", fd);
        -1
    }
}
///
pub fn sys_read(fd: usize, buf: *const u8, len: usize) -> isize {
    trace!("kernel:pid[{}] sys_read", current_task().unwrap().pid.0);
    let token = current_user_token();
    let task = current_task().unwrap();
    let inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if let Some(file) = &inner.fd_table[fd] {
        let file = file.clone();
        if !file.readable() {
            return -1;
        }
        // release current task TCB manually to avoid multi-borrow
        drop(inner);
        trace!("kernel: sys_read .. file.read");
        file.read(UserBuffer::new(translated_byte_buffer(token, buf, len))) as isize
    } else {
        -1
    }
}
///
pub fn sys_open(path: *const u8, flags: u32) -> isize {
    trace!("kernel:pid[{}] sys_open", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let token = current_user_token();
    let path = translated_str(token, path);
    println!("[sys_open] look for {}, flags={:?}", path, flags);
    if let Some(inode) = open_file(path.as_str(), OpenFlags::from_bits(flags).unwrap()) {
        {
            let inner=inode.inner.exclusive_access();
        let mem_inode=&inner.inode;
        mem_inode.read_disk_inode(|d| {
        println!("[sys_open] found inode:{} nlink={} size={}", mem_inode.ino(), d.nlink, d.size);
    });
    }
        let mut inner = task.inner_exclusive_access();
        let fd = inner.alloc_fd();
        inner.fd_table[fd] = Some(inode);
        fd as isize
    } else {
        -1
    }
}
///
pub fn sys_close(fd: usize) -> isize {
    trace!("kernel:pid[{}] sys_close", current_task().unwrap().pid.0);
    let task = current_task().unwrap();
    let mut inner = task.inner_exclusive_access();
    if fd >= inner.fd_table.len() {
        return -1;
    }
    if inner.fd_table[fd].is_none() {
        return -1;
    }
    inner.fd_table[fd].take();
    0
}

/// YOUR JOB: Implement fstat.
pub fn sys_fstat(_fd: usize, _st: *mut Stat) -> isize {
    //通过fd来找文件
    let token=current_user_token();
    let task=current_task().unwrap();
    let inner=task.inner_exclusive_access();
    if _fd >= inner.fd_table.len() {
        return -1;
    }
    let st=translated_refmut(token,_st);
    // let file=inner.fd_table[_fd].unwrap();//此时得到trait object 编译时无法确定类型
    //则需要向下转换类型
    //Any  trait 才提供了 运行时类型检查 的能力（ downcast_ref ）。   
    //所以必须先  as_any()  拿到  &dyn Any ，才能继续  downcast_ref::<OSInode>()；
    // 这是 Any 提供的：
    //fn downcast_ref<T: Any>(&self) -> Option<&T>;
    if let Some(file)=&inner.fd_table[_fd]{
        if let Some(osinode) = file.as_any().downcast_ref::<OSInode>(){
    //取出inode
        let inode={
            let inner=osinode.inner.exclusive_access();//引用
            inner.inode.clone()
        };
    //读取diskinode信息到_st里面
    inode.read_disk_inode(|dinode|{
        st.dev=0;
        st.ino=inode.ino() as u64;
        st.nlink=dinode.nlink;
        st.mode=if dinode.is_dir() {
            StatMode::DIR
        }else{
            StatMode::FILE
        }
    });
    }
    }else{
        //找不到
        return -1;
    }
    0
}
// ///调试函数
// pub fn dbg_find(name: &str) -> Option<Arc<Inode>> {
//     let entries=ROOT_INODE.read_disk_inode(|d| {
//         let entries = d.size as usize / DIRENT_SZ;
//         println!("[dbg_find] root size={}, entries={}", d.size, entries);
//         entries
//     });
//     for i in 0..entries {
//          println!("[dbg_find] i={}, entries={}", i, entries);
//         let mut de = DirEntry::empty();
//         let bytes = ROOT_INODE.read_at(i * DIRENT_SZ, de.as_bytes_mut());
//         if bytes != DIRENT_SZ { break; }
//         println!("[dbg_find] entry[{}] = {},inode_id={}", i, de.name(),de.inode_id());
//         if de.name() == name {
//             let res=ROOT_INODE.find(name).unwrap();
//             println!("[dbg_find] find! entry[{}] = {},de.ino={},res.ino={}", i, de.name(),de.inode_id(),res.ino());
//             return Some(res);
//         }
//     }
//     println!("[dbg_find] scan finished, now leave find.");
//     None
// }

/// YOUR JOB: Implement linkat.
pub fn sys_linkat(_old_name: *const u8, _new_name: *const u8) -> isize {
    println!("[sys_linkat] start");
    //将裸指针转化为字符串
    let token=current_user_token();
    let oldpath_str=translated_str(token,_old_name);
    let newpath_str=translated_str(token,_new_name);
    println!("[sys_linkat] old: {}, new: {}", oldpath_str, newpath_str);
    //若重名则错误
    if oldpath_str==newpath_str {
        println!("[sys_linkat] same path, return -1");
        return -1;
    }
    println!("[sys_linkat] finding old inode");
    //先找原inode
    let old_inode=match ROOT_INODE.find(&oldpath_str){
        Some(inode) => {
            println!("[sys_linkat] found old inode: {}", inode.ino());
            inode
        },
        None => {
            println!("[sys_linkat] old inode not found");
            return -1;
        },
    };
    println!("[sys_linkat] checking if new name exists");
    if ROOT_INODE.find(&newpath_str).is_some() {
        println!("[sys_linkat] new name already exists");
        return -1;
    }
    println!("[sys_linkat] before modify_disk_inode");
    // let old_inode=ROOT_INODE.find(&oldpath_str).unwrap();
    //然后在根目录下插入目录项
    //先获取disk inode可变引用
    let old_inode_number=old_inode.ino();
    println!("[sys_linkat] get old_inode_number!");
    ROOT_INODE.modify_disk_inode(|root_dinode| {
        println!("[sys_linkat] inside modify_disk_inode");
        //先对根节点扩容
        //计算根节点有多少个目录项
        let file_count=root_dinode.size as usize /DIRENT_SZ;
        let new_size=(file_count+1)*DIRENT_SZ;//这里分两步计算是为了得到filecount 后面要用
        let mut fs=ROOT_INODE.fs.lock();//fs外面包一层mutex 要用lock拆
        println!("[sys_linkat] Got both locks");
        ROOT_INODE.increase_size(new_size as u32,root_dinode,&mut fs);
        root_dinode.size = new_size as u32;
        println!("[sys_linkat] increase size success!");
        //用old_inode 新建一个目录项
        let new_dir=DirEntry::new(&newpath_str,old_inode_number);
        //往目录写目录项 (若是写文件内容 用内存索引节点write_at)
        root_dinode.write_at(file_count*DIRENT_SZ,new_dir.as_bytes(),&ROOT_INODE.block_device);
        println!("[sys_linkat] new_dir name:{} inode_id:{}",new_dir.name(),new_dir.inode_id());
    });
    println!("[sys_linkat] modify_disk_inode success!");
    //维护硬链接数
    //同步回diskinode
    old_inode.modify_disk_inode(|old_diskinode|{
        old_diskinode.nlink+=1;
    });
    //调试
    ROOT_INODE.modify_disk_inode(|d| {
    let cnt = d.size as usize / DIRENT_SZ;
    for i in 0..cnt {
        let mut de = DirEntry::empty();
        d.read_at(i * DIRENT_SZ, de.as_bytes_mut(), &ROOT_INODE.block_device);
        if de.name() == "linkname0" {
            println!("[after link] idx={} inode_id={}", i, de.inode_id());
        }
    }
});
    //
    println!("[sys_linkat] completed successfully");
    0
}
///调试
pub fn debug_inode_calculation() {
    let fs = ROOT_INODE.fs.lock();
    let inode_size = core::mem::size_of::<DiskInode>();
    let inodes_per_block = BLOCK_SZ / inode_size;
    
    println!("=== INODE CALCULATION DEBUG ===");
    println!("BLOCK_SZ: {}", BLOCK_SZ);
    println!("DiskInode size: {}", inode_size);
    println!("inodes_per_block: {}", inodes_per_block);
    println!("inode_area_start_block: {}", fs.inode_area_start_block);
    
    // 测试几个 inode_id
    for inode_id in [60, 41] {
        let (block_id, offset) = fs.get_disk_inode_pos(inode_id);
        println!("inode_id {} -> block_id: {}, offset: {}", inode_id, block_id, offset);
        
        // 反向计算应该得到原值
        let calculated_back = (block_id - fs.inode_area_start_block as u32) * inodes_per_block as u32 
                            + (offset / inode_size) as u32;
        println!("Reverse calculation: {} -> {}", inode_id, calculated_back);
    }
}
/// YOUR JOB: Implement unlinkat.
pub fn sys_unlinkat(_name: *const u8) -> isize {
    println!("[DEBUG]--------------------------------------- ");
    debug_inode_calculation();
    println!("[DEBUG]--------------------------------------- ");
    println!("[DEBUG] Global BLOCK_DEVICE: {:?}", &*BLOCK_DEVICE as *const _);
    println!("[DEBUG] ROOT_INODE block_device: {:?}", &*ROOT_INODE.block_device as *const _);
    println!("[sys_unlinkat] start");
    let token=current_user_token();
    let file=translated_str(token,_name);
    if ROOT_INODE.find(&file).is_none() {
        //文件不存在
        return -1;
    }
    let inode=ROOT_INODE.find(&file).unwrap();
    println!("[sys_unlinkat] find ok!");
    //删除目录项
        //先找目录项的idx
    let need_clear=ROOT_INODE.modify_disk_inode(|root_dinode|{
        println!("[sys_unlinkat] enter modify");
        let file_count=root_dinode.size /DIRENT_SZ as u32;
        let mut idx=0;
        for i in 0..file_count {
            //每次循环读一个目录项到缓冲区de
            let mut de=DirEntry::empty();
            root_dinode.read_at(i as usize *DIRENT_SZ,de.as_bytes_mut(),&ROOT_INODE.block_device);
            if de.name()==file{
                idx=i;
                break;
            }
        }
        if idx != file_count-1{
        //读最后一个目录项
        let mut last_de=DirEntry::empty();
        root_dinode.read_at((file_count-1) as usize *DIRENT_SZ,last_de.as_bytes_mut(),&ROOT_INODE.block_device);
        println!("[sys_unlinkat] last_de inode_id = {}", last_de.inode_id());
        //然后覆盖掉idx那条目录项
        root_dinode.write_at(idx as usize *DIRENT_SZ,last_de.as_bytes(),&ROOT_INODE.block_device);
        }
        //修改size
        let new_size=(file_count-1) as usize *DIRENT_SZ;
        println!("[sys_unlinkat] file_count={}, new_size={}", file_count, new_size);
        root_dinode.size=new_size as u32;
        println!("[sys_unlinkat] set size = {}", root_dinode.size);
        //inode--
        inode.modify_disk_inode(|dinode| {
            dinode.nlink-=1;
            dinode.nlink==0
        })
    });
    //  若发现inode现在为0 则需要回收
    if need_clear {
        println!("[sys_unlinkat] clearing");
        //清空文件内容 回收数据块
        inode.modify_disk_inode(|dinode|{
            //此时清空文件内容
            let data_blocks=dinode.clear_size(&ROOT_INODE.block_device);
            let mut fs=inode.fs.lock();
            //在位图上对应位置置0 
            for b in data_blocks {
                fs.dealloc_data(b);
            }
        })
    }
    //调试
    ROOT_INODE.modify_disk_inode(|d| {
    let cnt = d.size as usize / DIRENT_SZ;
    println!("[after unlink] size={}, entries={}", d.size, cnt);
    for i in 0..cnt {
        let mut de = DirEntry::empty();
        d.read_at(i * DIRENT_SZ, de.as_bytes_mut(), &ROOT_INODE.block_device);
        if de.name() == "linkname0" {
            println!("[after unlink] lname0 at idx={} inode_id={}", i, de.inode_id());
        }
    }
});
    //
    println!("[sys_unlinkat] completed successfully");
    0
}
