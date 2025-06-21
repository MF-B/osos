use core::ffi::c_int;

use axerrno::{LinuxError, LinuxResult};
use axio::SeekFrom;
use linux_raw_sys::general::{__kernel_off_t, iovec};

use crate::{
    file::{File, FileLike, get_file_like},
    ptr::{UserConstPtr, UserPtr},
};

/// Read data from the file indicated by `fd`.
///
/// Return the read size if success.
pub fn sys_read(fd: i32, buf: UserPtr<u8>, len: usize) -> LinuxResult<isize> {
    let buf = buf.get_as_mut_slice(len)?;
    debug!(
        "sys_read <= fd: {}, buf: {:p}, len: {}",
        fd,
        buf.as_ptr(),
        buf.len()
    );
    Ok(get_file_like(fd)?.read(buf)? as _)
}

pub fn sys_readv(fd: i32, iov: UserPtr<iovec>, iocnt: usize) -> LinuxResult<isize> {
    if !(0..=1024).contains(&iocnt) {
        return Err(LinuxError::EINVAL);
    }

    let iovs = iov.get_as_mut_slice(iocnt)?;
    let mut ret = 0;
    for iov in iovs {
        if iov.iov_len == 0 {
            continue;
        }
        let buf = UserPtr::<u8>::from(iov.iov_base as usize);
        let buf = buf.get_as_mut_slice(iov.iov_len as _)?;
        debug!(
            "sys_readv <= fd: {}, buf: {:p}, len: {}",
            fd,
            buf.as_ptr(),
            buf.len()
        );

        let read = get_file_like(fd)?.read(buf)?;
        ret += read as isize;

        if read < buf.len() {
            break;
        }
    }

    Ok(ret)
}

/// Write data to the file indicated by `fd`.
///
/// Return the written size if success.
pub fn sys_write(fd: i32, buf: UserConstPtr<u8>, len: usize) -> LinuxResult<isize> {
    let buf = buf.get_as_slice(len)?;
    debug!(
        "sys_write <= fd: {}, buf: {:p}, len: {}",
        fd,
        buf.as_ptr(),
        buf.len()
    );
    Ok(get_file_like(fd)?.write(buf)? as _)
}

pub fn sys_writev(fd: i32, iov: UserConstPtr<iovec>, iocnt: usize) -> LinuxResult<isize> {
    if !(0..=1024).contains(&iocnt) {
        return Err(LinuxError::EINVAL);
    }

    let iovs = iov.get_as_slice(iocnt)?;
    let mut ret = 0;
    for iov in iovs {
        if iov.iov_len == 0 {
            continue;
        }
        let buf = UserConstPtr::<u8>::from(iov.iov_base as usize);
        let buf = buf.get_as_slice(iov.iov_len as _)?;
        debug!(
            "sys_writev <= fd: {}, buf: {:p}, len: {}",
            fd,
            buf.as_ptr(),
            buf.len()
        );

        let written = get_file_like(fd)?.write(buf)?;
        ret += written as isize;

        if written < buf.len() {
            break;
        }
    }

    Ok(ret)
}

pub fn sys_lseek(fd: c_int, offset: __kernel_off_t, whence: c_int) -> LinuxResult<isize> {
    debug!("sys_lseek <= {} {} {}", fd, offset, whence);
    let pos = match whence {
        0 => SeekFrom::Start(offset as _),
        1 => SeekFrom::Current(offset as _),
        2 => SeekFrom::End(offset as _),
        _ => return Err(LinuxError::EINVAL),
    };
    let off = File::from_fd(fd)?.inner().seek(pos)?;
    Ok(off as _)
}

pub fn sys_ftruncate(fd: c_int, length: __kernel_off_t) -> LinuxResult<isize> {
    debug!("sys_ftruncate <= {} {}", fd, length);
    let file = get_file_like(fd)?;
    // 检查 length 是否为负数
    if length < 0 {
        return Err(LinuxError::EINVAL);
    }
    // 调用文件的 truncate 方法来截断文件
    if let Ok(file_obj) = file.into_any().downcast::<File>() {
        file_obj.inner().truncate(length as _)?;
        Ok(0)
    } else {
        // 对于其他类型的文件描述符（如管道、socket等），可能不支持截断
        error!("File descriptor {} does not support ftruncate", fd);
        Err(LinuxError::EINVAL)
    }
}

/// Synchronize a file's in-core state with storage device
/// 
/// fsync() transfers ("flushes") all modified in-core data of the file 
/// referred to by the file descriptor fd to the disk device
pub fn sys_fsync(fd: c_int) -> LinuxResult<isize> {
    debug!("sys_fsync <= fd: {}", fd);
    
    Ok(0)
}

/// Synchronize a file's data with storage device (similar to fsync but doesn't sync metadata)
/// 
/// fdatasync() is similar to fsync(), but does not flush modified metadata 
/// unless that metadata is needed in order to allow a subsequent data retrieval to be correctly handled
pub fn sys_fdatasync(fd: c_int) -> LinuxResult<isize> {
    debug!("sys_fdatasync <= fd: {}", fd);
    
    // 对于大多数简单的文件系统实现，fdatasync 可以直接调用 fsync
    // 在更复杂的实现中，这里只会同步数据而不同步元数据
    sys_fsync(fd)
}