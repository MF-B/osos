use crate::{
    file::get_file_like,
    ptr::{UserPtr, UserConstPtr},
    time::TimeValueLike,
};
use axerrno::LinuxResult;
use axtask::current;
use axhal::time::Duration;
use axtask::TaskExtRef;
use axsignal::SignalSet;
use core::mem;
use linux_raw_sys::general::timespec;

// fd_set结构体大小，通常为1024位
const FD_SETSIZE: usize = 1024;
const NFDBITS: usize = 8 * mem::size_of::<usize>();
const FD_SET_SIZE: usize = FD_SETSIZE / NFDBITS;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FdSet {
    fds_bits: [usize; FD_SET_SIZE],
}

impl FdSet {
    pub fn new() -> Self {
        Self {
            fds_bits: [0; FD_SET_SIZE],
        }
    }

    pub fn zero(&mut self) {
        self.fds_bits.fill(0);
    }

    pub fn set(&mut self, fd: usize) {
        if fd < FD_SETSIZE {
            let word_idx = fd / NFDBITS;
            let bit_idx = fd % NFDBITS;
            self.fds_bits[word_idx] |= 1 << bit_idx;
        }
    }

    pub fn clear(&mut self, fd: usize) {
        if fd < FD_SETSIZE {
            let word_idx = fd / NFDBITS;
            let bit_idx = fd % NFDBITS;
            self.fds_bits[word_idx] &= !(1 << bit_idx);
        }
    }

    pub fn is_set(&self, fd: usize) -> bool {
        if fd < FD_SETSIZE {
            let word_idx = fd / NFDBITS;
            let bit_idx = fd % NFDBITS;
            (self.fds_bits[word_idx] & (1 << bit_idx)) != 0
        } else {
            false
        }
    }

    pub fn count(&self) -> usize {
        self.fds_bits.iter().map(|word| word.count_ones() as usize).sum()
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct TimeVal {
    pub tv_sec: i64,   // 秒
    pub tv_usec: i64,  // 微秒
}

pub fn sys_select(
    nfds: i32,
    readfds: UserPtr<FdSet>,
    writefds: UserPtr<FdSet>,
    exceptfds: UserPtr<FdSet>,
    timeout: UserPtr<TimeVal>,
) -> LinuxResult<isize> {
    // 参数验证
    if nfds < 0 || nfds as usize > FD_SETSIZE {
        return Err(axerrno::LinuxError::EINVAL);
    }

    if nfds == 0 {
        // 如果没有文件描述符，只是延时
        if !timeout.is_null() {
            let tv = timeout.get_as_mut()?;
            if tv.tv_sec >= 0 && tv.tv_usec >= 0 {
                let timeout_ms = tv.tv_sec * 1000 + tv.tv_usec / 1000;
                if timeout_ms > 0 {
                    let timeout_duration = Duration::from_millis(timeout_ms as u64);
                    let curr = current();
                    let wq = &curr.task_ext().process_data().child_exit_wq;
                    wq.wait_timeout(timeout_duration);
                }
            }
        }
        return Ok(0);
    }

    // 获取用户空间的fd_set
    let mut readfds_local = FdSet::new();
    let mut writefds_local = FdSet::new();
    let mut exceptfds_local = FdSet::new();

    if !readfds.is_null() {
        readfds_local = *readfds.get_as_mut()?;
    }
    if !writefds.is_null() {
        writefds_local = *writefds.get_as_mut()?;
    }
    if !exceptfds.is_null() {
        exceptfds_local = *exceptfds.get_as_mut()?;
    }

    // 解析超时
    let timeout_ms = if !timeout.is_null() {
        let tv = timeout.get_as_mut()?;
        if tv.tv_sec < 0 || tv.tv_usec < 0 {
            return Err(axerrno::LinuxError::EINVAL);
        }
        Some(tv.tv_sec * 1000 + tv.tv_usec / 1000)
    } else {
        None // 无限等待
    };

    // 执行select逻辑
    let ready_count = select_files(
        nfds as usize,
        &mut readfds_local,
        &mut writefds_local,
        &mut exceptfds_local,
        timeout_ms,
    )?;

    // 将结果写回用户空间
    if !readfds.is_null() {
        *readfds.get_as_mut()? = readfds_local;
    }
    if !writefds.is_null() {
        *writefds.get_as_mut()? = writefds_local;
    }
    if !exceptfds.is_null() {
        *exceptfds.get_as_mut()? = exceptfds_local;
    }

    Ok(ready_count as isize)
}

fn select_files(
    nfds: usize,
    readfds: &mut FdSet,
    writefds: &mut FdSet,
    exceptfds: &mut FdSet,
    timeout_ms: Option<i64>,
) -> LinuxResult<usize> {
    let mut _ready_count = 0;

    // 第一次检查
    _ready_count = check_select_fds(nfds, readfds, writefds, exceptfds);
    
    if _ready_count > 0 {
        return Ok(_ready_count);
    }

    // 如果超时为0，直接返回
    if let Some(0) = timeout_ms {
        return Ok(0);
    }

    // 处理阻塞等待
    match timeout_ms {
        Some(ms) if ms > 0 => {
            // 带超时的等待
            let curr = current();
            let wq = &curr.task_ext().process_data().child_exit_wq;
            let timeout_duration = Duration::from_millis(ms as u64);
            
            let result = wq.wait_timeout(timeout_duration);
            
            // 等待后重新检查
            _ready_count = check_select_fds(nfds, readfds, writefds, exceptfds);
            
            if !result && _ready_count == 0 {
                // 超时且没有就绪的文件描述符
                return Ok(0);
            }
        }
        None => {
            // 无限等待
            loop {
                _ready_count = check_select_fds(nfds, readfds, writefds, exceptfds);
                if _ready_count > 0 {
                    break;
                }
                axtask::yield_now();
            }
        }
        _ => {}
    }

    Ok(_ready_count)
}

fn check_select_fds(
    nfds: usize,
    readfds: &mut FdSet,
    writefds: &mut FdSet,
    exceptfds: &mut FdSet,
) -> usize {
    let mut ready_count = 0;
    let mut new_readfds = FdSet::new();
    let mut new_writefds = FdSet::new();
    let mut new_exceptfds = FdSet::new();

    for fd in 0..nfds {
        let check_read = readfds.is_set(fd);
        let check_write = writefds.is_set(fd);
        let check_except = exceptfds.is_set(fd);

        if !check_read && !check_write && !check_except {
            continue;
        }

        match get_file_like(fd as i32) {
            Ok(file) => {
                match file.poll() {
                    Ok(state) => {
                        // 标准输入 (fd=0) 特殊处理
                        if check_read && state.readable {
                            // 对于标准输入，添加更严格的检查
                            if fd == 0 {  // STDIN_FILENO
                                // 检查是否确实有数据可读
                                // 这里可以通过调用底层的文件系统函数来检查
                                // 例如检查缓冲区中是否有数据待读取
                                if has_stdin_data() {
                                    new_readfds.set(fd);
                                    ready_count += 1;
                                }
                            } else {
                                // 其他文件保持原有逻辑
                                new_readfds.set(fd);
                                ready_count += 1;
                            }
                        }
                        if check_write && state.writable {
                            new_writefds.set(fd);
                            ready_count += 1;
                        }
                    }
                    Err(_) => {
                        if check_except {
                            new_exceptfds.set(fd);
                            ready_count += 1;
                        }
                    }
                }
            }
            Err(_) => {
                if check_except {
                    new_exceptfds.set(fd);
                    ready_count += 1;
                }
            }
        }
    }

    // 更新fd_set
    *readfds = new_readfds;
    *writefds = new_writefds;
    *exceptfds = new_exceptfds;

    ready_count
}

// 辅助函数：检查标准输入是否真的有数据可读
fn has_stdin_data() -> bool {
    // 需要实现一个检查标准输入缓冲区的函数
    // 如果你有内核中的终端/控制台驱动，应该查询它是否有待处理的输入
    // 这是一个示例实现，你需要根据你的系统架构进行调整
    if let Ok(_file) = get_file_like(0) {
        // 可以尝试从内部获取缓冲区状态，或使用其他方法检查
        // 例如，如果你的终端驱动有 peek 方法或缓冲区状态查询
        // return file.has_pending_input();
        false  // 默认假设没有数据，除非确认有
    } else {
        false
    }
}

// 辅助宏，用于兼容C库的fd_set操作
#[macro_export]
macro_rules! FD_ZERO {
    ($fdset:expr) => {
        $fdset.zero()
    };
}

#[macro_export]
macro_rules! FD_SET {
    ($fd:expr, $fdset:expr) => {
        $fdset.set($fd)
    };
}

#[macro_export]
macro_rules! FD_CLR {
    ($fd:expr, $fdset:expr) => {
        $fdset.clear($fd)
    };
}

#[macro_export]
macro_rules! FD_ISSET {
    ($fd:expr, $fdset:expr) => {
        $fdset.is_set($fd)
    };
}

pub fn sys_pselect6(
    nfds: i32,
    readfds: UserPtr<FdSet>,
    writefds: UserPtr<FdSet>,
    exceptfds: UserPtr<FdSet>,
    timeout: UserConstPtr<timespec>,
    sigmask: UserConstPtr<SignalSet>,
) -> LinuxResult<isize> {
    // 参数验证
    if nfds < 0 || nfds as usize > FD_SETSIZE {
        return Err(axerrno::LinuxError::EINVAL);
    }

    if nfds == 0 {
        // 如果没有文件描述符，只是延时
        if !timeout.is_null() {
            let ts = timeout.get_as_ref()?;
            let duration = ts.to_time_value();
            let timeout_ms = duration.as_millis() as i64;
            if timeout_ms >= 0 {
                let timeout_duration = Duration::from_millis(timeout_ms as u64);
                let curr = current();
                let wq = &curr.task_ext().process_data().child_exit_wq;
                wq.wait_timeout(timeout_duration);
            }
        }
        return Ok(0);
    }

    // 获取用户空间的fd_set
    let mut readfds_local = FdSet::new();
    let mut writefds_local = FdSet::new();
    let mut exceptfds_local = FdSet::new();

    if !readfds.is_null() {
        readfds_local = *readfds.get_as_mut()?;
    }
    if !writefds.is_null() {
        writefds_local = *writefds.get_as_mut()?;
    }
    if !exceptfds.is_null() {
        exceptfds_local = *exceptfds.get_as_mut()?;
    }

    // 解析超时
    let timeout_ms = if !timeout.is_null() {
        let ts = timeout.get_as_ref()?;
        let duration = ts.to_time_value();
        Some(duration.as_millis() as i64)
    } else {
        None // 无限等待
    };

    // 处理信号屏蔽
    let old_sigmask = if sigmask.is_null() {
        None
    } else {
        let new_mask = *sigmask.get_as_ref()?;
        let curr = current();
        let old_mask = curr
            .task_ext()
            .thread_data()
            .signal
            .with_blocked_mut(|blocked| {
                let old = *blocked;
                *blocked = new_mask;
                old
            });
        Some(old_mask)
    };

    // 执行pselect逻辑
    let ready_count = pselect_files(
        nfds as usize,
        &mut readfds_local,
        &mut writefds_local,
        &mut exceptfds_local,
        timeout_ms,
        old_sigmask,
    );

    // 恢复原始信号屏蔽
    if let Some(old_mask) = old_sigmask {
        let curr = current();
        curr.task_ext()
            .thread_data()
            .signal
            .with_blocked_mut(|blocked| {
                *blocked = old_mask;
            });
    }

    // 处理结果
    let result = match ready_count {
        Ok(count) => {
            // 将结果写回用户空间
            if !readfds.is_null() {
                *readfds.get_as_mut()? = readfds_local;
            }
            if !writefds.is_null() {
                *writefds.get_as_mut()? = writefds_local;
            }
            if !exceptfds.is_null() {
                *exceptfds.get_as_mut()? = exceptfds_local;
            }
            Ok(count as isize)
        }
        Err(e) => Err(e)
    };

    result
}

fn pselect_files(
    nfds: usize,
    readfds: &mut FdSet,
    writefds: &mut FdSet,
    exceptfds: &mut FdSet,
    timeout_ms: Option<i64>,
    _old_sigmask: Option<SignalSet>,
) -> LinuxResult<usize> {
    // 第一次检查
    let ready_count = check_select_fds(nfds, readfds, writefds, exceptfds);
    
    if ready_count > 0 {
        return Ok(ready_count);
    }

    // 如果超时为0，直接返回
    if let Some(0) = timeout_ms {
        return Ok(0);
    }

    // 处理阻塞等待
    match timeout_ms {
        Some(ms) if ms > 0 => {
            // 带超时的等待
            let start_time = axhal::time::monotonic_time();
            let timeout_duration = Duration::from_millis(ms as u64);
            
            loop {
                // 检查文件状态
                let ready_count = check_select_fds(nfds, readfds, writefds, exceptfds);
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
        }
        None => {
            // 无限等待
            loop {
                let ready_count = check_select_fds(nfds, readfds, writefds, exceptfds);
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
        _ => return Ok(0)
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