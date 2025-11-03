use crate::{
    fs::{open_file, OpenFlags},
    mm::{translated_ref, translated_refmut, translated_str,translated_byte_buffer},
    task::{
        current_process, current_task, current_user_token, exit_current_and_run_next, pid2process,
        suspend_current_and_run_next, SignalFlags,
    },
    timer::get_time_us,
};
use alloc::{string::String, sync::Arc, vec::Vec};

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
/// exit syscall
///
/// exit the current task and run the next task in task list
pub fn sys_exit(exit_code: i32) -> ! {
    trace!(
        "kernel:pid[{}] sys_exit",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}
/// yield syscall
pub fn sys_yield() -> isize {
    //trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}
/// getpid syscall
pub fn sys_getpid() -> isize {
    trace!(
        "kernel: sys_getpid pid:{}",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    current_task().unwrap().process.upgrade().unwrap().getpid() as isize
}
/// fork child process syscall
pub fn sys_fork() -> isize {
    trace!(
        "kernel:pid[{}] sys_fork",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    let current_process = current_process();
    let new_process = current_process.fork();
    let new_pid = new_process.getpid();
    // modify trap context of new_task, because it returns immediately after switching
    let new_process_inner = new_process.inner_exclusive_access();
    let task = new_process_inner.tasks[0].as_ref().unwrap();
    let trap_cx = task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    new_pid as isize
}
/// exec syscall
pub fn sys_exec(path: *const u8, mut args: *const usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_exec",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    let token = current_user_token();
    let path = translated_str(token, path);
    let mut args_vec: Vec<String> = Vec::new();
    loop {
        let arg_str_ptr = *translated_ref(token, args);
        if arg_str_ptr == 0 {
            break;
        }
        args_vec.push(translated_str(token, arg_str_ptr as *const u8));
        unsafe {
            args = args.add(1);
        }
    }
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let process = current_process();
        let argc = args_vec.len();
        process.exec(all_data.as_slice(), args_vec);
        // return argc because cx.x[10] will be covered with it later
        argc as isize
    } else {
        -1
    }
}

/// waitpid syscall
///
/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    //trace!("kernel: sys_waitpid");
    let process = current_process();
    // find a child process

    let mut inner = process.inner_exclusive_access();
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
        p.inner_exclusive_access().is_zombie && (pid == -1 || pid as usize == p.getpid())
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

/// kill syscall
pub fn sys_kill(pid: usize, signal: u32) -> isize {
    trace!(
        "kernel:pid[{}] sys_kill",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    if let Some(process) = pid2process(pid) {
        if let Some(flag) = SignalFlags::from_bits(signal) {
            process.inner_exclusive_access().signals |= flag;
            0
        } else {
            -1
        }
    } else {
        -1
    }
}

/// get_time syscall
///
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

/// mmap syscall
///
/// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_mmap NOT IMPLEMENTED",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    -1
}

/// munmap syscall
///
/// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!(
        "kernel:pid[{}] sys_munmap NOT IMPLEMENTED",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    -1
}

/// change data segment size
// pub fn sys_sbrk(size: i32) -> isize {
//     trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().process.upgrade().unwrap().getpid());
//     if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
//         old_brk as isize
//     } else {
//     -1
// }

/// spawn syscall
/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(_path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn NOT IMPLEMENTED",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    -1
}

/// set priority syscall
///
/// YOUR JOB: Set task priority
pub fn sys_set_priority(_prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority NOT IMPLEMENTED",
        current_task().unwrap().process.upgrade().unwrap().getpid()
    );
    -1
}
