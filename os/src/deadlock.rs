//! Deadlock detection module using Banker's algorithm.

use alloc::vec;
use alloc::vec::Vec;
use crate::task::current_process;
use crate::sync::UPSafeCell;
//lazy_static!宏是将初始化推迟到第一次运行时
//因为函数若不主动标明const fn 那么编译器将其当成运行时函数 运行时才求值
//套一层upsafecell可以保证互斥使用全局实例
lazy_static::lazy_static! {
    static ref DL: UPSafeCell<DLData> = unsafe{UPSafeCell::new(DLData::new())};
}
//定义资源和线程个数
const MAX_MUTEX:usize=16;//定义最大锁数量
const M:usize=32;
const N:usize=128;
//定义死锁检测时用到的三个数据结构 糅合进一个结构体里
struct DLData {
    init:Vec<bool>,//m*1  表示是否初始化过信号量和互斥锁avail
    avail:Vec<i32>, //m*1
    alloc:Vec<Vec<i32>>,//n*m
    need:Vec<Vec<i32>>,//n*m
}
/// 映射锁和信号量id
pub fn mutex_res_id(mutex_id:usize)->usize{
    mutex_id
}
///
pub fn sem_res_id(sem_id:usize)->usize{
    MAX_MUTEX+sem_id
}
//Available 初始化时 互斥锁值为1 信号量值为count    
// 对于Allocation  初值均0   
//对于Need 一开始不知道线程需求多少 初值也均为0
impl DLData{
    ///
    pub fn new()->Self{
        Self {
            init:vec![false;M],//当第一次申请semaphore时 根据init来更新avail的值
            avail:vec![1;M],//先全部初始化为1 之后信号量要变成count
            alloc:vec![vec![0;M];N],
            need:vec![vec![0;M];N],
        }
    }
    ///死锁检测 查看是否安全
    fn safe(&self)->bool{
        let mut work=self.avail.clone();
        let mut finish:Vec<bool>=vec![false;N];
        loop {
            let mut found:bool=false;//代表每次能不能找到满足条件的线程
        for i in 0..N {
            if finish[i]==true {continue;}
            let ok=(0..M).all(|j| self.need[i][j]<=work[j]);
            //迭代器方法：对迭代器里的每一个元素依次调用闭包  f只有当闭包对 所有  j  都返回  true  时， .all()  才返回  true
            if ok==true {
                for j in 0..M {
                    work[j]=work[j]+self.alloc[i][j];
                    finish[i]=true;
                    found=true;
                }
            } 
        }
        if found==false{//这一轮没找到
                let finish_res=(0..N).all(|i| finish[i]==true);
                if finish_res{
                    return true;
                }else{
                    return false;
                }
            }
    }
    }
}
///初始化avail 主要针对信号量
    pub fn init_avail(res_id:usize,num:i32){
        let mut dl=DL.exclusive_access();
        if dl.init[res_id]==true {return;}
        dl.avail[res_id]=num;
        dl.init[res_id]=true;
    }
///请求资源
    pub fn request(res_id:usize,req:i32){
        let mut dl=DL.exclusive_access();
        let pid=current_process().pid.0 as usize;
        dl.need[pid][res_id]+=req;
        }
    ///分配req个资源时check
    pub fn check(res_id:usize,req:i32)->bool{
        let mut dl=DL.exclusive_access();
        let pid=current_process().pid.0 as usize;
        if dl.avail[res_id]<req {return false;}
        //尝试分配
        dl.avail[res_id]-=req;
        dl.alloc[pid][res_id]+=req;
        dl.need[pid][res_id]-=req;
        let ok=dl.safe();
        //回滚资源
        dl.avail[res_id]+=req;
        dl.alloc[pid][res_id]-=req;
        dl.need[pid][res_id]+=req;
        ok
    }

 /// 真正分配
pub fn alloc(res_id: usize, amount: i32) {
    let mut dl = DL.exclusive_access();
    let pid = current_process().pid.0 as usize;
    dl.avail[res_id]      -= amount;
    dl.alloc[pid][res_id] += amount;
    dl.need[pid][res_id]  -= amount;
}
/// 释放
pub fn dealloc(res_id: usize, amount: i32) {
    let mut dl = DL.exclusive_access();
    let pid = current_process().pid.0 as usize;
    dl.avail[res_id]      += amount;
    dl.alloc[pid][res_id] -= amount;
    dl.need[pid][res_id]  += amount;
}
