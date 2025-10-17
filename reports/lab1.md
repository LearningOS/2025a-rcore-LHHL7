# 一 实现功能
修改了TaskControlBlock 的数据结构 让每个任务维护自己的系统调用次数
并新增了系统调用sys_trace

这个系统调用有三种功能，根据 trace_request 的值不同，执行不同的操作：

如果 trace_request 为 0，则 id 应被视作 *const u8 ，表示读取当前任务 id 地址处一个字节的无符号整数值。此时应忽略 data 参数。返回值为 id 地址处的值。

如果 trace_request 为 1，则 id 应被视作 *mut u8 ，表示写入 data （作为 u8，即只考虑最低位的一个字节）到该用户程序 id 地址处。返回值应为0。

如果 trace_request 为 2，表示查询当前任务调用编号为 id 的系统调用的次数，返回值为这个调用次数。本次调用也计入统计 。

否则，忽略其他参数，返回值为 -1。
# 二 问答题
## 1.程序出错行为：
bad_address:访问空指针  程序运行后提示 PageFault in application, bad addr = 0x0,

bad_instrcution:使用了sret这种S特权级的指令   则程序终止 打印  IllegalInstruction in application,

bad_register:在U态下读取S态的CSR 则程序终止打印IllegalInstruction in application,

sbi 及其版本：RustSBI 0.3.0-alpha.2（适配 QEMU 0.2.0-alpha.2）

## 2.
### 1.
刚进__restore时候 sp代表着内核栈栈顶指针
__restore两种使用场景：1.当处理完trap_handler时 使用__restore做trap上下文的恢复  2  当启动应用程序时，可以压入特殊的trap上下文到内核栈里  再使用__restore 就可以得到应用程序所需的上下文状态
### 2.
特殊处理了sstatus sepc sscratch这些寄存器
sstatus保存了进入s态前的特权级  确保能正确返回用户态
sepc保存了发生异常的指令地址 sret后返回sepc地址  继续用户程序
sscratch保存用户栈指针 确保恢复正确的栈环境
### 3.
在 RISC-V 调用约定里：
 x2  固定作为 栈指针寄存器 sp
 x4  固定作为 线程指针寄存器 tp
此时若恢复x2 那么sp值会被覆盖  后面的寄存器都无法恢复了  
而x4 tp对于内核没什么作用
### 4.
指令之后sp的值为内核栈指针
sscratch的值为用户栈指针
### 5.
sret指令发生了状态切换  它将sepc送pc并且恢复了sstatus 这样进入了用户态
### 6.
指令之后sp的值为用户栈指针
sscratch的值为内核栈指针
### 7.
用户程序中的sys_call是由ecall这条硬件指令内嵌实现的  执行时会trap进S态  

# 三 荣誉准则
1.在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

 无

2.此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

rCore-Tutorial-Guide-2025S： https://learningos.cn/rCore-Tutorial-Guide-2025S/ 

rCore-Tutorial-Book-v3： https://rcore-os.cn/rCore-Tutorial-Book-v3/ 

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计
# 四 备注
当我进行make test时  timer.rs中的get_time_us()中的两个常量不知为何变成060这种8进制形式(make run的时候还是正常的),查阅了资料后仍没解决,可能时测试框架的问题,之后修改为字面量才通过测试。
