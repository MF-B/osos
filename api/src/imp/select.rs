use core::time::Duration;

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

/// Poll file descriptors for events
pub fn sys_poll(
    fds: UserPtr<pollfd>,
    nfds: u32,
    timeout: UserConstPtr<i32>,
) -> LinuxResult<isize> {
    if nfds > 1024 {
        return Err(LinuxError::EINVAL);
    }

    // 计算超时时间
    // 计算超时时间
    let timeout = *timeout.get_as_ref().unwrap_or(&0);
    let deadline = if timeout >= 0 {
        Some(wall_time() + Duration::from_millis(timeout as u64))
    } else {
        None
    };

    // 获取用户提供的pollfd数组
    let poll_fds = fds.get_as_mut_slice(nfds as usize).unwrap();

    loop {
        axnet::poll_interfaces();
        
        let mut ready_count = 0;
        
        // 检查每个文件描述符的状态
        for fd_entry in &mut *poll_fds {
            // 初始化revents为0
            fd_entry.revents = 0;
            
            // 如果没有请求任何事件，则跳过
            if fd_entry.events == 0 {
                continue;
            }
            
            // 获取文件描述符对应的文件对象
            match get_file_like(fd_entry.fd as _) {
                Ok(file) => {
                    // 获取文件的轮询状态
                    match file.poll() {
                        Ok(state) => {
                            // 检查是否可读
                            if (fd_entry.events & POLLIN as i16) != 0 && state.readable {
                                fd_entry.revents |= POLLIN as i16;
                            }
                            
                            // 检查是否可写
                            if (fd_entry.events & POLLOUT as i16) != 0 && state.writable {
                                fd_entry.revents |= POLLOUT as i16;
                            }
                            
                            // 如果有任何事件就绪，增加计数
                            if fd_entry.revents != 0 {
                                ready_count += 1;
                            }
                        }
                        Err(_) => {
                            // 错误状态
                            fd_entry.revents |= POLLERR as i16;
                            ready_count += 1;
                        }
                    }
                }
                Err(_) => {
                    // 无效的文件描述符
                    fd_entry.revents |= POLLNVAL as i16;
                    ready_count += 1;
                }
            }
        }
        
        // 如果有就绪的文件描述符，立即返回
        if ready_count > 0 {
            return Ok(ready_count as isize);
        }
        
        // 检查是否超时
        match deadline {
            Some(ddl) if wall_time() >= ddl => {
                debug!("    poll timeout!");
                return Ok(0);
            }
            None => {}, // 无限期等待
            _ => {}    // 继续等待直到超时
        }
        
        // 让出CPU时间片
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
