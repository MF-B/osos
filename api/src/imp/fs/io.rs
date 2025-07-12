use core::ffi::c_int;

use alloc::vec;
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

pub fn sys_pwrite64(fd: i32, buf: UserConstPtr<u8>, len: usize, offset: i64) -> LinuxResult<isize> {  
    let buf = buf.get_as_slice(len)?;  
    debug!(  
        "sys_pwrite64 <= fd: {}, buf: {:p}, len: {}, offset: {}",  
        fd, buf.as_ptr(), buf.len(), offset  
    );  
      
    if offset < 0 {  
        return Err(LinuxError::EINVAL);  
    }  
      
    let file = File::from_fd(fd)?;  
    let written = file.get_inner().write_at(offset as u64, buf)?;  
    Ok(written as isize)  
}

pub fn sys_pread64(fd: i32, buf: UserPtr<u8>, len: usize, offset: i64) -> LinuxResult<isize> {  
    let buf = buf.get_as_mut_slice(len)?;  
    debug!(  
        "sys_pread64 <= fd: {}, buf: {:p}, len: {}, offset: {}",  
        fd, buf.as_ptr(), buf.len(), offset  
    );  
      
    if offset < 0 {  
        return Err(LinuxError::EINVAL);  
    }  
      
    let file = File::from_fd(fd)?;  
    let read = file.get_inner().read_at(offset as u64, buf)?;  
    Ok(read as isize)  
}

pub fn sys_lseek(fd: c_int, offset: __kernel_off_t, whence: c_int) -> LinuxResult<isize> {
    debug!("sys_lseek <= {} {} {}", fd, offset, whence);
    let pos = match whence {
        0 => SeekFrom::Start(offset as _),
        1 => SeekFrom::Current(offset as _),
        2 => SeekFrom::End(offset as _),
        _ => return Err(LinuxError::EINVAL),
    };
    let off = File::from_fd(fd)?.get_inner().seek(pos)?;
    Ok(off as _)
}

pub fn sys_ftruncate(fd: c_int, length: __kernel_off_t) -> LinuxResult<isize> {
    debug!("sys_ftruncate <= {} {}", fd, length);
    let file = File::from_fd(fd)?;
    if length < 0 {
        return Err(LinuxError::EINVAL);
    }
    file.get_inner().truncate(length as _)?;
    Ok(0)
}

/// Synchronize a file's in-core state with storage device
/// 
/// fsync() transfers ("flushes") all modified in-core data of the file 
/// referred to by the file descriptor fd to the disk device
pub fn sys_fsync(fd: c_int) -> LinuxResult<isize> {
    warn!("sys_fsync <= fd: {}", fd);
    Ok(0)
}

/// Transfer data between file descriptors
///
/// sendfile() copies data between one file descriptor and another.
/// This is more efficient than using read() and write() separately.
pub fn sys_sendfile(
    out_fd: c_int,
    in_fd: c_int,
    offset: UserPtr<__kernel_off_t>,
    count: usize,
) -> LinuxResult<isize> {
    debug!("sys_sendfile <= out_fd: {}, in_fd: {}, count: {}", out_fd, in_fd, count);
    
    // 简单实现：从 in_fd 读取数据并写入 out_fd
    let mut buffer = vec![0u8; count.min(8192)]; // 限制缓冲区大小
    let mut total_copied = 0;
    let mut remaining = count;
    
    // 如果有偏移量，先处理偏移
    if !offset.is_null() {
        let offset_val = *offset.get_as_mut()?;
        // 注意：这里简化处理，实际应该支持 seek
        debug!("sendfile with offset: {}", offset_val);
    }
    
    while remaining > 0 && total_copied < count {
        let to_read = remaining.min(buffer.len());
        let read_size = get_file_like(in_fd)?.read(&mut buffer[..to_read])?;
        
        if read_size == 0 {
            break; // EOF reached
        }
        
        let written = get_file_like(out_fd)?.write(&buffer[..read_size])?;
        total_copied += written;
        remaining -= written;
        
        if written < read_size {
            break; // Output blocked
        }
    }
    
    // 更新偏移量（简化实现）
    if !offset.is_null() {
        *offset.get_as_mut()? += total_copied as __kernel_off_t;
    }
    
    Ok(total_copied as isize)
}