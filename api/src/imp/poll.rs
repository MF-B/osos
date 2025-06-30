use core::mem;

use crate::{
    file::get_file_like,
    ptr::{UserConstPtr, UserPtr},
    time::TimeValueLike,
};
use alloc::vec::Vec;
use axerrno::LinuxResult;
use axhal::time::Duration;
use axsignal::SignalSet;
use axtask::TaskExtRef;
use axtask::current;
use bitflags::bitflags;
use linux_raw_sys::general::*;

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct PollEvents: i16 {
        const POLLIN = POLLIN as i16;      // 数据可读
        const POLLOUT = POLLOUT as i16;    // 数据可写
        const POLLERR = POLLERR as i16;    // 错误条件
        const POLLHUP = POLLHUP as i16;    // 挂起
        const POLLNVAL = POLLNVAL as i16;  // 无效请求
        const POLLPRI = POLLPRI as i16;    // 紧急数据
        const POLLRDNORM = POLLRDNORM as i16; // 正常数据可读
        const POLLWRNORM = POLLWRNORM as i16; // 正常数据可写
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Pollfd {
    pub fd: i32,      // 文件描述符
    pub events: i16,  // 请求的事件
    pub revents: i16, // 返回的事件
}

pub fn sys_poll(fds: UserPtr<Pollfd>, nfds: usize, timeout: i32) -> LinuxResult<isize> {
    // 参数验证
    if nfds == 0 {
        return Ok(0);
    }

    // 使用 get_as_mut_slice 获取整个 Pollfd 数组
    let user_fds = fds.get_as_mut_slice(nfds)?;

    // 复制到本地数组以便操作
    let mut poll_fds = Vec::with_capacity(nfds);
    for pfd in user_fds.iter() {
        poll_fds.push(*pfd);
    }

    // 主要轮询逻辑
    let ready_count = poll_files(&mut poll_fds, timeout, None)?;

    // 将结果写回用户空间
    for (i, pfd) in poll_fds.iter().enumerate() {
        user_fds[i] = *pfd;
    }

    Ok(ready_count as isize)
}

pub fn sys_ppoll(
    fds: UserPtr<Pollfd>,
    nfds: usize,
    timeout: UserConstPtr<timespec>,
    _sigmask: UserConstPtr<SignalSet>,
) -> LinuxResult<isize> {
    // 参数验证
    if nfds == 0 {
        return Ok(0);
    }

    // 使用 get_as_mut_slice 获取整个 Pollfd 数组
    let user_fds = fds.get_as_mut_slice(nfds)?;

    // 复制到本地数组以便操作
    let mut poll_fds = Vec::with_capacity(nfds);
    for pfd in user_fds.iter() {
        poll_fds.push(*pfd);
    }

    // 处理超时时间
    let timeout_ms = if timeout.is_null() {
        0 // 如果 timeout 为空，则设置为 0，表示不等待
    } else {
        let ts = timeout.get_as_ref()?;
        let duration = ts.to_time_value();
        duration.as_millis() as i32
    };

    // 主要轮询逻辑
    let ready_count = poll_files(&mut poll_fds, timeout_ms, None)?;

    // 将结果写回用户空间
    for (i, pfd) in poll_fds.iter().enumerate() {
        user_fds[i] = *pfd;
    }

    Ok(ready_count as isize)
}

fn poll_files(poll_fds: &mut [Pollfd], timeout: i32, _old_sigmask: Option<SignalSet>) -> LinuxResult<usize> {
    // 第一次检查
    let ready_count = check_poll_fds(poll_fds);
    if ready_count > 0 || timeout == 0 {
        return Ok(ready_count);
    }
    
    if timeout > 0 {
        // 带超时的等待
        let start_time = axhal::time::monotonic_time();
        let timeout_duration = Duration::from_millis(timeout as u64);
        
        loop {
            // 检查文件状态
            let ready_count = check_poll_fds(poll_fds);
            if ready_count > 0 {
                return Ok(ready_count);
            }
            
            // 检查信号中断
            if check_signal_interrupt()? {
                return Err(axerrno::LinuxError::EINTR);
            }
            
            // 检查超时
            let elapsed = axhal::time::monotonic_time() - start_time;
            if elapsed >= timeout_duration {
                return Ok(0);
            }
            
            // 短暂休眠后重试
            axtask::yield_now();
        }
    } else {
        // 无限等待 (timeout < 0)
        loop {
            let ready_count = check_poll_fds(poll_fds);
            if ready_count > 0 {
                return Ok(ready_count);
            }
            
            // 检查信号中断
            if check_signal_interrupt()? {
                return Err(axerrno::LinuxError::EINTR);
            }
            
            axtask::yield_now();
        }
    }
}

fn check_signal_interrupt() -> LinuxResult<bool> {
    let curr = current();
    let thr_data = curr.task_ext().thread_data();
    
    // 获取当前待处理信号和被阻塞的信号
    let pending = thr_data.signal.pending();
    let blocked = thr_data.signal.with_blocked_mut(|blocked| *blocked);
    
    // 检查是否有未被阻塞的待处理信号
    let unblocked_pending = pending & !blocked;
    
    // 如果有未被阻塞的待处理信号，则应该被中断
    let bits: u64 = unsafe { mem::transmute(unblocked_pending) };
    Ok(bits != 0)
}

fn check_poll_fds(poll_fds: &mut [Pollfd]) -> usize {
    let mut ready_count = 0;

    for pfd in poll_fds.iter_mut() {
        pfd.revents = 0; // 清空返回事件  

        if pfd.fd < 0 {
            continue; // 忽略无效fd  
        }

        // 获取文件对象并调用poll方法
        match get_file_like(pfd.fd) {
            Ok(file) => {
                match file.poll() {
                    Ok(state) => {
                        // 将PollState转换为poll事件
                        if state.readable && (pfd.events & POLLIN as i16) != 0 {
                            pfd.revents |= POLLIN as i16;
                        }
                        if state.writable && (pfd.events & POLLOUT as i16) != 0 {
                            pfd.revents |= POLLOUT as i16;
                        }

                        if pfd.revents != 0 {
                            ready_count += 1;
                        }
                    }
                    Err(_) => {
                        pfd.revents |= POLLERR as i16;
                        ready_count += 1;
                    }
                }
            }
            Err(_) => {
                pfd.revents |= POLLNVAL as i16;
                ready_count += 1;
            }
        }
    }

    ready_count
}
