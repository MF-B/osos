use crate::{
    file::{FileLike, get_file_like},
    ptr::UserPtr,
};
use alloc::vec::Vec;
use axerrno::{LinuxError, LinuxResult};
use bitflags::bitflags;
use linux_raw_sys::general::*;
use axtask::current;
use axhal::time::Duration;
use axtask::TaskExtRef;

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
    let ready_count = poll_files(&mut poll_fds, timeout)?;

    // 将结果写回用户空间
    for (i, pfd) in poll_fds.iter().enumerate() {
        user_fds[i] = *pfd;
    }

    Ok(ready_count as isize)
}

fn poll_files(poll_fds: &mut [Pollfd], timeout: i32) -> LinuxResult<usize> {  
    let mut ready_count = 0;  
      
    // 第一次轮询检查  
    ready_count = check_poll_fds(poll_fds);  
    if ready_count > 0 || timeout == 0 {  
        return Ok(ready_count);  
    }  
      
    // 处理阻塞等待  
    if timeout > 0 {  
        // 使用等待队列实现真正的超时  
        let curr = current();  
        let wq = &curr.task_ext().process_data().child_exit_wq; // 或创建专门的poll等待队列  
          
        // 转换超时时间  
        let timeout_duration = Duration::from_millis(timeout as u64);  
          
        // 带超时的等待  
        let result = wq.wait_timeout(timeout_duration);  
          
        // 等待后重新检查  
        ready_count = check_poll_fds(poll_fds);  
          
        if !result && ready_count == 0 {  
            // 超时且没有就绪的文件描述符  
            return Ok(0);  
        }  
    } else if timeout < 0 {  
        // 无限等待  
        loop {  
            ready_count = check_poll_fds(poll_fds);  
            if ready_count > 0 {  
                break;  
            }  
            axtask::yield_now();  
        }  
    }  
      
    Ok(ready_count)  
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

// fn poll_files(poll_fds: &mut [Pollfd], timeout: i32) -> LinuxResult<usize> {
//     let mut ready_count = 0;

//     // 第一次轮询：检查所有文件描述符状态
//     for pfd in poll_fds.iter_mut() {
//         pfd.revents = 0; // 清空返回事件  

//         if pfd.fd < 0 {
//             continue; // 忽略无效fd  
//         }

//         // 获取文件对象并调用poll方法
//         match get_file_like(pfd.fd) {
//             Ok(file) => {
//                 match file.poll() {
//                     Ok(state) => {
//                         // 将PollState转换为poll事件
//                         if state.readable && (pfd.events & POLLIN as i16) != 0 {
//                             pfd.revents |= POLLIN as i16;
//                         }
//                         if state.writable && (pfd.events & POLLOUT as i16) != 0 {
//                             pfd.revents |= POLLOUT as i16;
//                         }

//                         if pfd.revents != 0 {
//                             ready_count += 1;
//                         }
//                     }
//                     Err(_) => {
//                         pfd.revents |= POLLERR as i16;
//                         ready_count += 1;
//                     }
//                 }
//             }
//             Err(_) => {
//                 pfd.revents |= POLLNVAL as i16;
//                 ready_count += 1;
//             }
//         }
//     }

//     // 如果有就绪的文件描述符或超时为0，直接返回
//     if ready_count > 0 || timeout == 0 {
//         return Ok(ready_count);
//     }

//     // 处理阻塞等待（简化实现）
//     if timeout > 0 {
//         // TODO: 实现真正的超时等待机制
//         // 目前简化为yield一次后重新检查
//         axtask::yield_now();

//         // 重新检查一次
//         ready_count = 0;
//         for pfd in poll_fds.iter_mut() {
//             if pfd.fd < 0 {
//                 continue;
//             }

//             if let Ok(file) = get_file_like(pfd.fd) {
//                 if let Ok(state) = file.poll() {
//                     pfd.revents = 0;
//                     if state.readable && (pfd.events & POLLIN as i16) != 0 {
//                         pfd.revents |= POLLIN as i16;
//                     }
//                     if state.writable && (pfd.events & POLLOUT as i16) != 0 {
//                         pfd.revents |= POLLOUT as i16;
//                     }

//                     if pfd.revents != 0 {
//                         ready_count += 1;
//                     }
//                 }
//             }
//         }
//     }

//     Ok(ready_count)
// }
