use core::{
    ffi::{c_char, c_int, c_void},
    mem::offset_of,
};

use alloc::ffi::CString;
use axerrno::{LinuxError, LinuxResult};
use axfs::fops::DirEntry;
use linux_raw_sys::general::{
    linux_dirent64, AT_FDCWD, AT_REMOVEDIR, DT_BLK, DT_CHR, DT_DIR, DT_FIFO, DT_LNK, DT_REG, DT_SOCK, DT_UNKNOWN, RENAME_EXCHANGE, RENAME_NOREPLACE, RENAME_WHITEOUT
};

use crate::{
    file::{Directory, FileLike},
    path::{handle_file_path, HARDLINK_MANAGER},
    ptr::{nullable, UserConstPtr, UserPtr},
};

use axtask::current;
use axtask::TaskExtRef;

// Terminal control constants
const TCGETS: usize = 0x5401;
const TCSETS: usize = 0x5402;
const TCSETSW: usize = 0x5403;
const TCSETSF: usize = 0x5404;
const TIOCGPGRP: usize = 0x540F;
const TIOCSPGRP: usize = 0x5410;
const TIOCGWINSZ: usize = 0x5413;

/// The ioctl() system call manipulates the underlying device parameters
/// of special files.
///
/// # Arguments
/// * `fd` - The file descriptor
/// * `op` - The request code. It is of type unsigned long in glibc and BSD,
///   and of type int in musl and other UNIX systems.
/// * `argp` - The argument to the request. It is a pointer to a memory location
pub fn sys_ioctl(fd: i32, op: usize, argp: UserPtr<c_void>) -> LinuxResult<isize> {
    debug!("sys_ioctl <= fd: {}, op: 0x{:x}", fd, op);
    
    // 获取文件描述符
    let current = current();
    // let file = get_file_like(fd as _)
    //     .map_err(|_| LinuxError::EBADF)?;
    
    // 检查是否是 tty 设备
    match op {
        TCGETS | TCSETS | TCSETSW | TCSETSF => {
            // 对于 tty 设备的终端控制操作，我们简单返回成功
            // 这里可以实现更复杂的终端属性设置
            debug!("Terminal control ioctl: 0x{:x}", op);
            Ok(0)
        }
        TIOCGPGRP => {
            // 获取前台进程组ID
            debug!("Get foreground process group");
            // 对于简化实现，返回当前进程的进程组ID
            // 这样可以避免 SIGTTIN 信号的发送
            let pgid = current.task_ext().thread.process().group().pgid();
            if !argp.is_null() {
                let argp_mut = argp.address().as_mut_ptr() as *mut i32;
                unsafe { *argp_mut = pgid as i32 };
            }
            Ok(0)
        }
        TIOCSPGRP => {
            // 设置前台进程组ID
            debug!("Set foreground process group");
            // 对于简化实现，我们忽略这个操作
            Ok(0)
        }
        TIOCGWINSZ => {
            // 获取终端窗口大小
            debug!("Get window size");
            // 返回默认的窗口大小
            #[repr(C)]
            struct Winsize {
                ws_row: u16,
                ws_col: u16,
                ws_xpixel: u16,
                ws_ypixel: u16,
            }
            if !argp.is_null() {
                // let winsize = argp.cast::<Winsize>().get_as_mut()?;
                let winsize_ptr = argp.address().as_mut_ptr() as *mut Winsize;
                let winsize = unsafe { &mut *winsize_ptr };
                winsize.ws_row = 24;    // 默认24行
                winsize.ws_col = 80;    // 默认80列
                winsize.ws_xpixel = 0;
                winsize.ws_ypixel = 0;
            }
            Ok(0)
        }
        _ => {
            debug!("Unsupported ioctl operation: 0x{:x}", op);
            Ok(0)  // 对于不支持的操作，返回成功以避免程序崩溃
        }
    }
}

pub fn sys_chdir(path: UserConstPtr<c_char>) -> LinuxResult<isize> {
    let path = path.get_as_str()?;
    debug!("sys_chdir <= {:?}", path);

    axfs::api::set_current_dir(path)?;
    Ok(0)
}

pub fn sys_mkdirat(dirfd: i32, path: UserConstPtr<c_char>, mode: u32) -> LinuxResult<isize> {
    let path = path.get_as_str()?;
    debug!(
        "sys_mkdirat <= dirfd: {}, path: {}, mode: {}",
        dirfd, path, mode
    );

    if mode != 0 {
        warn!("directory mode not supported.");
    }

    let path = handle_file_path(dirfd, path)?;
    axfs::api::create_dir(path.as_str())?;

    Ok(0)
}

#[allow(dead_code)]
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum FileType {
    Unknown = DT_UNKNOWN as u8,
    Fifo = DT_FIFO as u8,
    Chr = DT_CHR as u8,
    Dir = DT_DIR as u8,
    Blk = DT_BLK as u8,
    Reg = DT_REG as u8,
    Lnk = DT_LNK as u8,
    Socket = DT_SOCK as u8,
}

impl From<axfs::api::FileType> for FileType {
    fn from(ft: axfs::api::FileType) -> Self {
        match ft {
            ft if ft.is_dir() => FileType::Dir,
            ft if ft.is_file() => FileType::Reg,
            _ => FileType::Unknown,
        }
    }
}

// Directory buffer for getdents64 syscall
struct DirBuffer<'a> {
    buf: &'a mut [u8],
    offset: usize,
}

impl<'a> DirBuffer<'a> {
    fn new(buf: &'a mut [u8]) -> Self {
        Self { buf, offset: 0 }
    }

    fn remaining_space(&self) -> usize {
        self.buf.len().saturating_sub(self.offset)
    }

    fn write_entry(&mut self, d_type: FileType, name: &[u8]) -> bool {
        const NAME_OFFSET: usize = offset_of!(linux_dirent64, d_name);

        let len = NAME_OFFSET + name.len() + 1;
        // alignment
        let len = len.next_multiple_of(align_of::<linux_dirent64>());
        if self.remaining_space() < len {
            return false;
        }

        unsafe {
            let entry_ptr = self.buf.as_mut_ptr().add(self.offset);
            entry_ptr.cast::<linux_dirent64>().write(linux_dirent64 {
                // FIXME: real inode number
                d_ino: 1,
                d_off: 0,
                d_reclen: len as _,
                d_type: d_type as _,
                d_name: Default::default(),
            });

            let name_ptr = entry_ptr.add(NAME_OFFSET);
            name_ptr.copy_from_nonoverlapping(name.as_ptr(), name.len());
            name_ptr.add(name.len()).write(0);
        }

        self.offset += len;
        true
    }
}

pub fn sys_getdents64(fd: i32, buf: UserPtr<u8>, len: usize) -> LinuxResult<isize> {
    let buf = buf.get_as_mut_slice(len)?;
    debug!(
        "sys_getdents64 <= fd: {}, buf: {:p}, len: {}",
        fd,
        buf.as_ptr(),
        buf.len()
    );

    let mut buffer = DirBuffer::new(buf);

    let dir = Directory::from_fd(fd)?;

    let mut last_dirent = dir.last_dirent();
    if let Some(ent) = last_dirent.take() {
        if !buffer.write_entry(ent.entry_type().into(), ent.name_as_bytes()) {
            *last_dirent = Some(ent);
            return Err(LinuxError::EINVAL);
        }
    }

    let mut inner = dir.get_inner();
    loop {
        let mut dirents = [DirEntry::default()];
        let cnt = inner.read_dir(&mut dirents)?;
        if cnt == 0 {
            break;
        }

        let [ent] = dirents;
        if !buffer.write_entry(ent.entry_type().into(), ent.name_as_bytes()) {
            *last_dirent = Some(ent);
            break;
        }
    }

    if last_dirent.is_some() && buffer.offset == 0 {
        return Err(LinuxError::EINVAL);
    }
    Ok(buffer.offset as _)
}

/// create a link from new_path to old_path
/// old_path: old file path
/// new_path: new file path
/// flags: link flags
/// return value: return 0 when success, else return -1.
pub fn sys_linkat(
    old_dirfd: c_int,
    old_path: UserConstPtr<c_char>,
    new_dirfd: c_int,
    new_path: UserConstPtr<c_char>,
    flags: i32,
) -> LinuxResult<isize> {
    let old_path = old_path.get_as_str()?;
    let new_path = new_path.get_as_str()?;
    debug!(
        "sys_linkat <= old_dirfd: {}, old_path: {}, new_dirfd: {}, new_path: {}, flags: {}",
        old_dirfd, old_path, new_dirfd, new_path, flags
    );

    if flags != 0 {
        warn!("Unsupported flags: {flags}");
    }

    // handle old path
    let old_path = handle_file_path(old_dirfd, old_path)?;
    // handle new path
    let new_path = handle_file_path(new_dirfd, new_path)?;

    HARDLINK_MANAGER.create_link(&new_path, &old_path)?;

    Ok(0)
}

pub fn sys_link(
    old_path: UserConstPtr<c_char>,
    new_path: UserConstPtr<c_char>,
) -> LinuxResult<isize> {
    sys_linkat(AT_FDCWD, old_path, AT_FDCWD, new_path, 0)
}

/// remove link of specific file (can be used to delete file)
/// dir_fd: the directory of link to be removed
/// path: the name of link to be removed
/// flags: can be 0 or AT_REMOVEDIR
/// return 0 when success, else return -1
pub fn sys_unlinkat(dirfd: c_int, path: UserConstPtr<c_char>, flags: u32) -> LinuxResult<isize> {
    let path = path.get_as_str()?;
    debug!(
        "sys_unlinkat <= dirfd: {}, path: {}, flags: {}",
        dirfd, path, flags
    );

    let path = handle_file_path(dirfd, path)?;

    if flags == AT_REMOVEDIR {
        axfs::api::remove_dir(path.as_str())?;
    } else {
        let metadata = axfs::api::metadata(path.as_str())?;
        if metadata.is_dir() {
            return Err(LinuxError::EISDIR);
        } else {
            debug!("unlink file: {:?}", path);
            HARDLINK_MANAGER
                .remove_link(&path)
                .ok_or(LinuxError::ENOENT)?;
        }
    }
    Ok(0)
}

pub fn sys_unlink(path: UserConstPtr<c_char>) -> LinuxResult<isize> {
    sys_unlinkat(AT_FDCWD, path, 0)
}

pub fn sys_getcwd(buf: UserPtr<u8>, size: usize) -> LinuxResult<isize> {
    let buf = nullable!(buf.get_as_mut_slice(size))?;

    let Some(buf) = buf else {
        return Ok(0);
    };

    let cwd = CString::new(axfs::api::current_dir()?).map_err(|_| LinuxError::EINVAL)?;
    let cwd = cwd.as_bytes_with_nul();

    if cwd.len() <= buf.len() {
        buf[..cwd.len()].copy_from_slice(cwd);
        Ok(buf.as_ptr() as _)
    } else {
        Err(LinuxError::ERANGE)
    }
}

pub fn sys_symlinkat(
    old_path: UserConstPtr<c_char>,
    new_dirfd: c_int,
    new_path: UserConstPtr<c_char>,
) -> LinuxResult<isize> {
    let old_path = old_path.get_as_str()?;
    let new_path = new_path.get_as_str()?;
    debug!(
        "sys_symlinkat <= old_path: {}, new_dirfd: {}, new_path: {}",
        old_path, new_dirfd, new_path
    );

    // 处理新路径，支持相对于 dirfd 的路径
    let new_path = handle_file_path(new_dirfd, new_path)?;
    
    // 创建符号链接
    axfs::api::create_symlink(old_path, &new_path)?;

    Ok(0)
}

pub fn sys_readlinkat(
    dirfd: c_int,
    path: UserConstPtr<c_char>,
    buf: UserPtr<u8>,
    bufsiz: usize,
) -> LinuxResult<isize> {
    let path = path.get_as_str()?;
    debug!(
        "sys_readlinkat <= dirfd: {}, path: {}, bufsiz: {}",
        dirfd, path, bufsiz
    );

    if bufsiz == 0 {
        return Err(LinuxError::EINVAL);
    }

    let buf = buf.get_as_mut_slice(bufsiz)?;
    
    // 处理路径，支持相对于 dirfd 的路径
    let path = handle_file_path(dirfd, path)?;
    
    // 读取符号链接的目标
    let copy_len = axfs::api::read_link(&path, buf)?;
    
    // 返回实际读取的字节数
    Ok(copy_len as isize)
}
