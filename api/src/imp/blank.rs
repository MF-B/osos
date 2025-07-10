use core::ffi::c_char;

use axerrno::LinuxResult;
use memory_addr::VirtAddr;
use crate::ptr::UserConstPtr;


/// Set the robust futex list for the current thread
/// 对于简单的操作系统，可以返回成功但不做实际操作
pub fn sys_set_robust_list(_head: VirtAddr, _len: usize) -> LinuxResult<isize> {
    // 在简单的OS中，我们可以忽略 robust futex 列表
    // 直接返回成功
    Ok(0)
}

/// Get/set resource limits for a process
/// 对于简单的操作系统，可以返回一些默认值
pub fn sys_prlimit64(
    _pid: i32,
    _resource: i32,
    _new_limit: VirtAddr,
    _old_limit: VirtAddr,
) -> LinuxResult<isize> {
    // 在简单的OS中，我们可以：
    // 1. 忽略设置新限制的请求
    // 2. 如果请求获取旧限制，返回一些合理的默认值
    // 3. 或者简单地返回成功
    Ok(0)
}


pub fn sys_faccessat(
    _dirfd: isize,
    pathname: UserConstPtr<c_char>,
    _mode: isize,
    _flags: isize,
) -> LinuxResult<isize> {
    let path = pathname.get_as_str()?;
    debug!("sys_faccessat <= path: {}", path);
    // 目前文件存在即返回Ok
    if axfs::api::absolute_path_exists(path) {
        Ok(0) // 文件存在
    } else {
        Err(axerrno::LinuxError::ENOENT) // 文件不存在
    }
}
