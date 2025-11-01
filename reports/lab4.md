# 一 实现功能
## 1.
迁移上一章的 sys_get_time sys_mmap sys_munmap 以适应新的进程结构
## 2.
实现系统调用sys_linkat 
功能：创建一个文件的一个硬链接
实现系统调用sys_unlinkat 
功能：取消一个文件路径到文件的链接
实现系统调用sys_fstat
功能：获取文件状态
## 3.
在easy-fs里加了log依赖 只要os那边启动info 就可以在easy-fs里调试了
修改了inode的数据结构 维护inode_id  由于find函数逻辑总是返回新的inode
from_elf是解析elf文件生成应用地址空间   exec是加载进来并替换当前的地址空间
修改了from_elf函数的返回值 新增返回 elf 映射的最高地址  这样使用exec加载的时候可以inner.heap_bottom = elf_end;
        inner.program_brk = elf_end;以防有前面测例残留导致 有旧的Maparea
# 二 问答题
## 1.
在  easy-fs  里，root inode 是整个文件系统「唯一入口」。它不靠路径名查找，而是被硬编码在磁盘布局的固定位置（超级块里记录其 ino）。因此：
作用：
1. 
所有  open("/...")  都从它开始解析路径；
挂接文件系统时（ mount / open_root ）第一件事就是把它读进来；
只要它在，就能逐级找到任何文件和目录。
2. 
如果 root inode 的内容（即它的  DiskInode  记录）被写坏，典型后果：
文件系统驱动仍能读出超级块，但 无法得到有效的 root inode， open_root()  直接返回  Err ；
## 2.（ch7）
1. 
cat log.txt | wc -l
做了啥：shell 主进程先调用  pipe()  得到 fd3→fd4，然后  fork()  两次。
子进程 1 把 fd3 重定向到 stdout，再  exec cat log.txt ，于是 cat 的输出写进管道；
子进程 2 把 fd4 重定向到 stdin，再  exec wc -l ，于是 wc 从管道读数据并输出行数。
效果：一条命令完成“文件内容 → 行数”的流水线，无需中间临时文件。
2. 
若有 N 个进程需要两两通信，必须建立 N×(N−1)/2 条匿名管道，描述符爆炸、管理困难。
可以内核里用循环缓冲区 + 引用计数实现；无读者时写者阻塞，无写者时读者 EOF。
int w = open("/chan/log", O_WRONLY);   // 任何进程
int r = open("/chan/log", O_RDONLY);   // 任何进程
write(w, "hello\n", 6);                // 立即被 read(r,buf,6) 拿到
 
# 三 荣誉准则
1.在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

 无

2.此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

rCore-Tutorial-Guide-2025S： https://learningos.cn/rCore-Tutorial-Guide-2025S/ 

rCore-Tutorial-Book-v3： https://rcore-os.cn/rCore-Tutorial-Book-v3/ 

3. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

4. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计