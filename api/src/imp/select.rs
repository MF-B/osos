// use crate::{
//     file::get_file_like,
//     ptr::{UserConstPtr, UserPtr},
//     time::TimeValueLike,
// };
// use axerrno::LinuxResult;
// use axhal::time::Duration;
// use axsignal::SignalSet;
// use axtask::TaskExtRef;
// use axtask::current;
// use core::mem;
// use linux_raw_sys::general::timespec;

// // fd_set结构体大小，通常为1024位
// const FD_SETSIZE: usize = 1024;
// const NFDBITS: usize = 8 * mem::size_of::<usize>();
// const FD_SET_SIZE: usize = FD_SETSIZE / NFDBITS;

// #[repr(C)]
// #[derive(Debug, Clone, Copy)]
// pub struct FdSet {
//     fds_bits: [usize; FD_SET_SIZE],
// }

// impl FdSet {
//     pub fn new() -> Self {
//         Self {
//             fds_bits: [0; FD_SET_SIZE],
//         }
//     }

//     pub fn zero(&mut self) {
//         self.fds_bits.fill(0);
//     }

//     pub fn set(&mut self, fd: usize) {
//         if fd < FD_SETSIZE {
//             let word_idx = fd / NFDBITS;
//             let bit_idx = fd % NFDBITS;
//             self.fds_bits[word_idx] |= 1 << bit_idx;
//         }
//     }

//     pub fn clear(&mut self, fd: usize) {
//         if fd < FD_SETSIZE {
//             let word_idx = fd / NFDBITS;
//             let bit_idx = fd % NFDBITS;
//             self.fds_bits[word_idx] &= !(1 << bit_idx);
//         }
//     }

//     pub fn is_set(&self, fd: usize) -> bool {
//         if fd < FD_SETSIZE {
//             let word_idx = fd / NFDBITS;
//             let bit_idx = fd % NFDBITS;
//             (self.fds_bits[word_idx] & (1 << bit_idx)) != 0
//         } else {
//             false
//         }
//     }

//     pub fn count(&self) -> usize {
//         self.fds_bits
//             .iter()
//             .map(|word| word.count_ones() as usize)
//             .sum()
//     }
// }

// #[repr(C)]
// #[derive(Debug, Clone, Copy)]
// pub struct TimeVal {
//     pub tv_sec: i64,  // 秒
//     pub tv_usec: i64, // 微秒
// }

// fn check_select_fds(
//     nfds: usize,
//     readfds: &mut FdSet,
//     writefds: &mut FdSet,
//     exceptfds: &mut FdSet,
// ) -> usize {
//     let mut ready_count = 0;
//     let mut new_readfds = FdSet::new();
//     let mut new_writefds = FdSet::new();
//     let mut new_exceptfds = FdSet::new();

//     for fd in 0..nfds {
//         let check_read = readfds.is_set(fd);
//         let check_write = writefds.is_set(fd);
//         let check_except = exceptfds.is_set(fd);

//         if !check_read && !check_write && !check_except {
//             continue;
//         }

//         match get_file_like(fd as i32) {
//             Ok(file) => {
//                 match file.poll() {
//                     Ok(state) => {
//                         // 标准输入 (fd=0) 特殊处理
//                         if check_read && state.readable {
//                             if fd == 0 {
//                                 // STDIN_FILENO
//                                 // 对于标准输入，除非确实有数据，否则不标记为就绪
//                                 // 这可以避免iozone等程序的无限循环
//                                 // 只有在交互式环境下才检查标准输入
//                                 if false && has_stdin_data() {
//                                     new_readfds.set(fd);
//                                     ready_count += 1;
//                                 }
//                             } else {
//                                 new_readfds.set(fd);
//                                 ready_count += 1;
//                             }
//                         }
//                         if check_write && state.writable {
//                             new_writefds.set(fd);
//                             ready_count += 1;
//                         }
//                     }
//                     Err(_) => {
//                         if check_except {
//                             new_exceptfds.set(fd);
//                             ready_count += 1;
//                         }
//                     }
//                 }
//             }
//             Err(_) => {
//                 if check_except {
//                     new_exceptfds.set(fd);
//                     ready_count += 1;
//                 }
//             }
//         }
//     }

//     // 更新fd_set
//     *readfds = new_readfds;
//     *writefds = new_writefds;
//     *exceptfds = new_exceptfds;

//     ready_count
// }

// fn has_stdin_data() -> bool {
//     if let Ok(file) = get_file_like(0) {
//         // 尝试检查文件是否有数据可读
//         match file.poll() {
//             Ok(state) => {
//                 // 对于标准输入，我们需要更智能的检测
//                 // 如果是终端设备，检查是否有输入缓冲
//                 if state.readable {
//                     // 可以尝试非阻塞读取来检测是否真的有数据
//                     // 这里先简化处理，假设如果poll返回readable就有数据
//                     true
//                 } else {
//                     false
//                 }
//             }
//             Err(_) => false,
//         }
//     } else {
//         false
//     }
// }

// // 辅助宏，用于兼容C库的fd_set操作
// #[macro_export]
// macro_rules! FD_ZERO {
//     ($fdset:expr) => {
//         $fdset.zero()
//     };
// }

// #[macro_export]
// macro_rules! FD_SET {
//     ($fd:expr, $fdset:expr) => {
//         $fdset.set($fd)
//     };
// }

// #[macro_export]
// macro_rules! FD_CLR {
//     ($fd:expr, $fdset:expr) => {
//         $fdset.clear($fd)
//     };
// }

// #[macro_export]
// macro_rules! FD_ISSET {
//     ($fd:expr, $fdset:expr) => {
//         $fdset.is_set($fd)
//     };
// }

// pub fn sys_pselect6(
//     nfds: i32,
//     readfds: UserPtr<FdSet>,
//     writefds: UserPtr<FdSet>,
//     exceptfds: UserPtr<FdSet>,
//     timeout: UserConstPtr<timespec>,
//     sigmask: UserConstPtr<SignalSet>,
// ) -> LinuxResult<isize> {
//     //    error!("sys_pselect6 called with nfds: {}", nfds);
//     // 参数验证
//     if nfds < 0 || nfds as usize > FD_SETSIZE {
//         error!("参数验证失败");
//         return Err(axerrno::LinuxError::EINVAL);
//     }

//     // 获取用户空间的fd_set
//     let mut readfds_local = FdSet::new();
//     let mut writefds_local = FdSet::new();
//     let mut exceptfds_local = FdSet::new();

//     if !readfds.is_null() {
//         readfds_local = *readfds.get_as_mut()?;
//     }
//     if !writefds.is_null() {
//         writefds_local = *writefds.get_as_mut()?;
//     }
//     if !exceptfds.is_null() {
//         exceptfds_local = *exceptfds.get_as_mut()?;
//     }

//     // 解析超时
//     let timeout_ms = if !timeout.is_null() {
//         let ts = timeout.get_as_ref()?;
//         let duration = ts.to_time_value();
//         Some(duration.as_millis() as i64)
//     } else {
//         None // 无限等待
//     };

//     // 处理信号屏蔽
//     let old_sigmask = if sigmask.is_null() {
//         None
//     } else {
//         let new_mask = *sigmask.get_as_ref()?;
//         let curr = current();
//         let old_mask = curr
//             .task_ext()
//             .thread_data()
//             .signal
//             .with_blocked_mut(|blocked| {
//                 let old = *blocked;
//                 *blocked = new_mask;
//                 old
//             });
//         Some(old_mask)
//     };

//     // 执行pselect逻辑
//     let ready_count = pselect_files(
//         nfds as usize,
//         &mut readfds_local,
//         &mut writefds_local,
//         &mut exceptfds_local,
//         timeout_ms,
//         old_sigmask,
//     );

//     // 恢复原始信号屏蔽
//     if let Some(old_mask) = old_sigmask {
//         let curr = current();
//         curr.task_ext()
//             .thread_data()
//             .signal
//             .with_blocked_mut(|blocked| {
//                 *blocked = old_mask;
//             });
//     }

//     // 处理结果
//     let result = match ready_count {
//         Ok(count) => {
//             // 将结果写回用户空间
//             if !readfds.is_null() {
//                 *readfds.get_as_mut()? = readfds_local;
//             }
//             if !writefds.is_null() {
//                 *writefds.get_as_mut()? = writefds_local;
//             }
//             if !exceptfds.is_null() {
//                 *exceptfds.get_as_mut()? = exceptfds_local;
//             }
//             Ok(count as isize)
//         }
//         Err(e) => Err(e),
//     };

//     result
// }

// fn pselect_files(
//     nfds: usize,
//     readfds: &mut FdSet,
//     writefds: &mut FdSet,
//     exceptfds: &mut FdSet,
//     timeout_ms: Option<i64>,
//     _old_sigmask: Option<SignalSet>,
// ) -> LinuxResult<usize> {
//     // 第一次检查
//     let ready_count = check_select_fds(nfds, readfds, writefds, exceptfds);
//     if ready_count > 0 {
//         return Ok(ready_count);
//     }

//     // 如果超时为0，直接返回
//     if let Some(0) = timeout_ms {
//         return Ok(0);
//     }

//     // 获取当前任务的等待队列 - 参考 futex 模式
//     let curr = current();
//     let wq = &curr.task_ext().process_data().select_wq;

//     // 使用真正的阻塞等待，而不是忙等待
//     match timeout_ms {
//         Some(ms) if ms > 0 => {
//             let timeout_duration = Duration::from_millis(ms as u64);
//             // 参考 futex 的 wait_timeout 模式
//             wq.wait_timeout(timeout_duration);

//             // 等待结束后重新检查一次
//             let ready_count = check_select_fds(nfds, readfds, writefds, exceptfds);
//             Ok(ready_count)
//         }
//         None => {
//             // 无限等待 - 参考 futex 的 wait 模式
//             wq.wait();

//             let ready_count = check_select_fds(nfds, readfds, writefds, exceptfds);
//             Ok(ready_count)
//         }
//         _ => Ok(0),
//     }
// }

use crate::file::get_file_like;
use crate::ptr::UserConstPtr;
use crate::ptr::UserPtr;
use crate::time::TimeValueLike;
use axerrno::LinuxError;
use axerrno::LinuxResult;
use axhal::time::wall_time;
use linux_raw_sys::general::*;

const FD_SETSIZE: usize = 1024;
const BITS_PER_USIZE: usize = usize::BITS as usize;
const FD_SETSIZE_USIZES: usize = FD_SETSIZE.div_ceil(BITS_PER_USIZE);

struct FdSets {
    nfds: usize,
    bits: [usize; FD_SETSIZE_USIZES * 3],
}

impl FdSets {
    fn from(
        nfds: usize,
        read_fds: UserPtr<__kernel_fd_set>,
        write_fds: UserPtr<__kernel_fd_set>,
        except_fds: UserPtr<__kernel_fd_set>,
    ) -> Self {
        let nfds = nfds.min(FD_SETSIZE);
        let nfds_usizes = nfds.div_ceil(BITS_PER_USIZE);
        let mut bits = core::mem::MaybeUninit::<[usize; FD_SETSIZE_USIZES * 3]>::uninit();
        let bits_ptr: *mut usize = unsafe { core::mem::transmute(bits.as_mut_ptr()) };

        let copy_from_fd_set = |bits_ptr: *mut usize, fds: UserPtr<__kernel_fd_set>| unsafe {
            let dst = core::slice::from_raw_parts_mut(bits_ptr, nfds_usizes);
            if fds.is_null() {
                dst.fill(0);
            } else {
                // let fds_ptr = (*fds).fds_bits.as_ptr() as *const usize;
                let fds_ptr = fds.get_as_mut().unwrap().fds_bits.as_ptr() as *const usize;
                let src = core::slice::from_raw_parts(fds_ptr, nfds_usizes);
                dst.copy_from_slice(src);
            }
        };

        let bits = unsafe {
            copy_from_fd_set(bits_ptr, read_fds);
            copy_from_fd_set(bits_ptr.add(FD_SETSIZE_USIZES), write_fds);
            copy_from_fd_set(bits_ptr.add(FD_SETSIZE_USIZES * 2), except_fds);
            bits.assume_init()
        };
        Self { nfds, bits }
    }

    fn poll_all(
        &self,
        res_read_fds: UserPtr<__kernel_fd_set>,
        res_write_fds: UserPtr<__kernel_fd_set>,
        res_except_fds: UserPtr<__kernel_fd_set>,
    ) -> LinuxResult<usize> {
        let mut read_bits_ptr = self.bits.as_ptr();
        let mut write_bits_ptr = unsafe { read_bits_ptr.add(FD_SETSIZE_USIZES) };
        let mut execpt_bits_ptr = unsafe { read_bits_ptr.add(FD_SETSIZE_USIZES * 2) };
        let mut i = 0;
        let mut res_num = 0;
        while i < self.nfds {
            let read_bits = unsafe { *read_bits_ptr };
            let write_bits = unsafe { *write_bits_ptr };
            let except_bits = unsafe { *execpt_bits_ptr };
            unsafe {
                read_bits_ptr = read_bits_ptr.add(1);
                write_bits_ptr = write_bits_ptr.add(1);
                execpt_bits_ptr = execpt_bits_ptr.add(1);
            }

            let all_bits = read_bits | write_bits | except_bits;
            if all_bits == 0 {
                i += BITS_PER_USIZE;
                continue;
            }
            let mut j = 0;
            while j < BITS_PER_USIZE && i + j < self.nfds {
                let bit = 1 << j;
                if all_bits & bit == 0 {
                    j += 1;
                    continue;
                }
                let fd = i + j;
                match get_file_like(fd as _)?.poll() {
                    Ok(state) => {
                        if state.readable && read_bits & bit != 0 {
                            unsafe { set_fd_set(res_read_fds, fd) };
                            res_num += 1;
                        }
                        if state.writable && write_bits & bit != 0 {
                            unsafe { set_fd_set(res_write_fds, fd) };
                            res_num += 1;
                        }
                    }
                    Err(e) => {
                        debug!("    except: {} {:?}", fd, e);
                        if except_bits & bit != 0 {
                            unsafe { set_fd_set(res_except_fds, fd) };
                            res_num += 1;
                        }
                    }
                }
                j += 1;
            }
            i += BITS_PER_USIZE;
        }
        Ok(res_num)
    }
}

/// Monitor multiple file descriptors, waiting until one or more of the file descriptors become "ready" for some class of I/O operation
pub fn sys_select(
    nfds: isize,
    readfds: UserPtr<__kernel_fd_set>,
    writefds: UserPtr<__kernel_fd_set>,
    exceptfds: UserPtr<__kernel_fd_set>,
    timeout: UserConstPtr<timeval>,
) -> LinuxResult<isize> {
    if nfds < 0 {
        return Err(LinuxError::EINVAL);
    }
    let nfds = (nfds as usize).min(FD_SETSIZE);
    let deadline = timeout
        .get_as_ref()
        .map(|t| wall_time() + (*t).to_time_value());
    let fd_sets = FdSets::from(nfds, readfds, writefds, exceptfds);

    unsafe {
        zero_fd_set(readfds, nfds);
        zero_fd_set(writefds, nfds);
        zero_fd_set(exceptfds, nfds);
    }

    loop {
        axnet::poll_interfaces();
        let res = fd_sets.poll_all(readfds, writefds, exceptfds)?;
        if res > 0 {
            return Ok(res as isize);
        }

        if deadline.is_ok_and(|ddl| wall_time() >= ddl) {
            debug!("    timeout!");
            return Ok(0);
        }
        axtask::yield_now();
    }
}

unsafe fn zero_fd_set(fds: UserPtr<__kernel_fd_set>, nfds: usize) {
    if !fds.is_null() {
        let nfds_usizes = nfds.div_ceil(BITS_PER_USIZE);
        // let dst = &mut unsafe { *fds }.fds_bits[..nfds_usizes];
        let dst = &mut fds.get_as_mut().unwrap().fds_bits[..nfds_usizes];
        dst.fill(0);
    }
}

unsafe fn set_fd_set(fds: UserPtr<__kernel_fd_set>, fd: usize) {
    if !fds.is_null() {
        // unsafe { *fds }.fds_bits[fd / BITS_PER_USIZE] |= 1 << (fd % BITS_PER_USIZE);
        fds.get_as_mut().unwrap().fds_bits[fd / BITS_PER_USIZE] |= 1 << (fd % BITS_PER_USIZE);
    }
}
