//!Implementation of [`TaskManager`]
use super::TaskControlBlock;
use crate::sync::UPSafeCell;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use lazy_static::*;
///A big constant number that define previously
pub const BIG_STRIDE:usize=100;
///A array of `TaskControlBlock` that is thread-safe
pub struct TaskManager {
    ready_queue: VecDeque<Arc<TaskControlBlock>>,
}

/// A simple FIFO scheduler.
impl TaskManager {
    ///Creat an empty TaskManager
    pub fn new() -> Self {
        Self {
            ready_queue: VecDeque::new(),
        }
    }
    /// Add process back to ready queue
    pub fn add(&mut self, task: Arc<TaskControlBlock>) {
        self.ready_queue.push_back(task);
    }
    /// Take a process out of the ready queue
    pub fn fetch(&mut self) -> Option<Arc<TaskControlBlock>> {
        // self.ready_queue.pop_front() //简单地从队列弹出 FIFO算法
        //现在修改为stride算法
        if self.ready_queue.is_empty() {return None}
        let min_stride_idx={
            let queue=&mut self.ready_queue;
         //let mut a = &b  是“引用本身可变”——你能让  a  再去指向别的  &b2 。
// let a = &mut b  是“引用指向的数据可变”——你不能把  a  改成指向别处，但能通过  a  修改  b  的内容
        let mut min_stride=usize::MAX;
        let mut min_stride_idx=0;
        for (idx,tcb) in queue.iter().enumerate() {//遍历得到最小stride tcb
            let inner=tcb.inner_exclusive_access();
            if inner.stride<min_stride {
                min_stride=inner.stride;
                min_stride_idx=idx;
            }
        }
        min_stride_idx
    };
        let task={
            let queue=&mut self.ready_queue;
       queue.remove(min_stride_idx).unwrap()
        };
       {
         let mut inner=task.inner_exclusive_access();
        inner.stride+=inner.pass;
       }
        Some(task)
    }
}

lazy_static! {
    /// TASK_MANAGER instance through lazy_static!
    pub static ref TASK_MANAGER: UPSafeCell<TaskManager> =
        unsafe { UPSafeCell::new(TaskManager::new()) };
}

/// Add process to ready queue
pub fn add_task(task: Arc<TaskControlBlock>) {
    //trace!("kernel: TaskManager::add_task");
    TASK_MANAGER.exclusive_access().add(task);
}

/// Take a process out of the ready queue
pub fn fetch_task() -> Option<Arc<TaskControlBlock>> {
    //trace!("kernel: TaskManager::fetch_task");
    TASK_MANAGER.exclusive_access().fetch()
}
