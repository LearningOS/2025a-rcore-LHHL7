//! Types related to task management

use super::TaskContext;
/// 系统调用计数器数组长度（覆盖 0..511 号调用）
pub const SYSCALL_ID_CNT:usize=512; 
/// The task control block (TCB) of a task.
#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    /// The task status in it's lifecycle
    pub task_status: TaskStatus,
    /// The task context
    pub task_cx: TaskContext,
    ///每个任务维护自己的系统调用次数
    pub syscall_cnt:[isize;SYSCALL_ID_CNT],
}

/// The status of a task
#[derive(Copy, Clone, PartialEq)]
pub enum TaskStatus {
    /// uninitialized
    UnInit,
    /// ready to run
    Ready,
    /// running
    Running,
    /// exited
    Exited,
}
