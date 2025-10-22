//! `Arc<Inode>` -> `OSInodeInner`: In order to open files concurrently
//! we need to wrap `Inode` into `Arc`,but `Mutex` in `Inode` prevents
//! file systems from being accessed simultaneously
//!
//! `UPSafeCell<OSInodeInner>` -> `OSInode`: for static `ROOT_INODE`,we
//! need to wrap `OSInodeInner` into `UPSafeCell`
use super::File;
use crate::drivers::BLOCK_DEVICE;
use crate::mm::UserBuffer;
use crate::sync::UPSafeCell;
use alloc::sync::Arc;
use alloc::vec::Vec;
use bitflags::*;
use easy_fs::{EasyFileSystem, Inode};
use lazy_static::*;
use core::any::Any;
// use crate::syscall::dbg_find;
/// inode in memory
/// A wrapper around a filesystem inode
/// to implement File trait atop
pub struct OSInode {
    readable: bool,
    writable: bool,
    ///
    pub inner: UPSafeCell<OSInodeInner>,
}
/// The OS inode inner in 'UPSafeCell'
pub struct OSInodeInner {
    offset: usize,
    pub inode: Arc<Inode>,
}

impl OSInode {
    /// create a new inode in memory
    pub fn new(readable: bool, writable: bool, inode: Arc<Inode>) -> Self {
        Self {
            readable,
            writable,
            inner: unsafe { UPSafeCell::new(OSInodeInner { offset: 0, inode }) },
        }
    }
    ///
    pub fn ino(&self)->u32{
        let inner=self.inner.exclusive_access();
        inner.inode.ino()
    }
    /// read all data from the inode
    pub fn read_all(&self) -> Vec<u8> {
        let mut inner = self.inner.exclusive_access();
        let mut buffer: Vec<u8> = Vec::with_capacity(512);
        buffer.resize(512, 0);
        let mut v: Vec<u8> = Vec::new();
        loop {
            let len = inner.inode.read_at(inner.offset, &mut buffer);
            if len == 0 {
                break;
            }
            inner.offset += len;
            v.extend_from_slice(&buffer[..len]);
        }
        v
    }
}
lazy_static! {
    ///全局根目录
    pub static ref ROOT_INODE: Arc<Inode> = {
        let efs = EasyFileSystem::open(BLOCK_DEVICE.clone());
        Arc::new(EasyFileSystem::root_inode(&efs))
    };
}

/// List all apps in the root directory
pub fn list_apps() {
    println!("/**** APPS ****");
    for app in ROOT_INODE.ls() {
        println!("{}", app);
    }
    println!("**************/");
}

bitflags! {
    ///  The flags argument to the open() system call is constructed by ORing together zero or more of the following values:
    pub struct OpenFlags: u32 {
        /// readyonly
        const RDONLY = 0;
        /// writeonly
        const WRONLY = 1 << 0;
        /// read and write
        const RDWR = 1 << 1;
        /// create new file
        const CREATE = 1 << 9;
        /// truncate file size to 0
        const TRUNC = 1 << 10;
    }
}

impl OpenFlags {
    /// Do not check validity for simplicity
    /// Return (readable, writable)
    pub fn read_write(&self) -> (bool, bool) {
        if self.is_empty() {
            (true, false)
        } else if self.contains(Self::WRONLY) {
            (false, true)
        } else {
            (true, true)
        }
    }
}

/// Open a file
pub fn open_file(name: &str, flags: OpenFlags) -> Option<Arc<OSInode>> {
    let (readable, writable) = flags.read_write();
    if flags.contains(OpenFlags::CREATE) {
        if let Some(inode) = ROOT_INODE.find(name) {
            // clear size
            inode.clear();
            Some(Arc::new(OSInode::new(readable, writable, inode)))
        } else {
            // create file
            println!("[open_file] CREATE {} -> new inode", name);
            let new_inode=ROOT_INODE
                .create(name)
                .map(|inode| Arc::new(OSInode::new(readable, writable, inode)))?;
            println!("[open_file] CREATE got inode_id={}", new_inode.inner.exclusive_access().inode.ino());
            Some(new_inode)
            
        }
    } else {
        ROOT_INODE.find(name).map(|inode| {
            if flags.contains(OpenFlags::TRUNC) {
                inode.clear();
            }
            println!("[open_file] found inode:{} " , inode.ino());
            Arc::new(OSInode::new(readable, writable, inode))
        })
    }
}

impl File for OSInode {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn readable(&self) -> bool {
        self.readable
    }
    fn writable(&self) -> bool {
        self.writable
    }
    fn read(&self, mut buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();

        println!("[OSInode::read] START - inode: {}, current offset: {}, total buffer len: {}", 
             inner.inode.ino(), inner.offset, buf.len());

        let mut total_read_size = 0usize;
        for (i, slice) in buf.buffers.iter_mut().enumerate(){
            println!("[OSInode::read] Processing slice {}: len {}", i, slice.len());

            let read_size = inner.inode.read_at(inner.offset, *slice);
             println!("[OSInode::read] read_at returned: {} bytes", read_size);
            if read_size == 0 {
                println!("[OSInode::read] read_size is 0, breaking");
                break;
            }
            // 打印实际读取到的内容
        println!("[OSInode::read] Read content: {:?}", &slice[..read_size.min(20)]);
            inner.offset += read_size;
            total_read_size += read_size;

            println!("[OSInode::read] After slice {}: offset now {}, total read {}", 
                 i, inner.offset, total_read_size);
        }
        println!("[OSInode::read] END - total read: {}, final offset: {}", total_read_size, inner.offset);
        total_read_size
    }
    fn write(&self, buf: UserBuffer) -> usize {
        let mut inner = self.inner.exclusive_access();
        println!("[OSInode::write] START - inode: {}, current offset: {}, total buffer len: {}", 
             inner.inode.ino(), inner.offset, buf.len());
        let mut total_write_size = 0usize;
        for (i,slice) in buf.buffers.iter().enumerate() {
            println!("[OSInode::write] Processing slice {}: len {}", i, slice.len());
        println!("[OSInode::write] Content preview: {:?}", &slice[..slice.len().min(10)]);
            let write_size = inner.inode.write_at(inner.offset, *slice);
            println!("[OSInode::write] write_at returned: {} bytes", write_size);

            assert_eq!(write_size, slice.len());
            inner.offset += write_size;
            total_write_size += write_size;

            println!("[OSInode::write] After slice {}: offset now {}, total written {}", 
                 i, inner.offset, total_write_size);
        }
        println!("[OSInode::write] END - total written: {}, final offset: {}", total_write_size, inner.offset);

        total_write_size
    }
}
