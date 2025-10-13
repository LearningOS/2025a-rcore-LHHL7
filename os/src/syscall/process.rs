//! Process management syscalls
use crate::{
    task::{exit_current_and_run_next, suspend_current_and_run_next,TASK_MANAGER},
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// get time with second and microsecond
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    println!("[KERNEL] sys_get_time called, ts ptr: {:p}", ts);
    let us = get_time_us();
    println!("[KERNEL] get_time_us() = {}", us);
    unsafe {
        *ts = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
        println!("[KERNEL] Wrote: sec={}, usec={}", (*ts).sec, (*ts).usec);
    }
    0
}

// TODO: implement the syscall
pub fn sys_trace(_trace_request: usize, _id: usize, _data: usize) -> isize {
    trace!("kernel: sys_trace");//isize是指针大小的有符号整数32 位机器上它是  i32 ，64 位机器上它是  i64
    match _trace_request {
        0=>{
            let ptr=_id as *const u8;//*代表裸指针 
            // 将id整数硬转化为指向一个字节（u8）的裸指针
            unsafe{
                *ptr as isize
            }
        }
        1=>{
            let ptr=_id as *mut u8;
            let data_u8=_data as u8;
            unsafe {
                *ptr=data_u8;
                //编译器看到  *ptr  在赋值号左边，就生成** Store 指令**
                //此时可以写值
            }
            0
        }
        2=>{
            //当前任务的对应的args[1]系统调用次数
            if _id>=512 {return -1}//越界
            let inner=TASK_MANAGER.inner.exclusive_access();
            let current_task_id=inner.current_task;
            inner.tasks[current_task_id].syscall_cnt[_id]
        }
        _=>{
            -1
        }
    }
}
