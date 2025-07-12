use core::ffi::c_char;

use axerrno::LinuxResult;
use linux_raw_sys::system::{sysinfo, new_utsname, __IncompleteArrayField};

use crate::ptr::UserPtr;

pub fn sys_getuid() -> LinuxResult<isize> {
    Ok(0)
}

pub fn sys_geteuid() -> LinuxResult<isize> {
    Ok(1)
}

pub fn sys_getgid() -> LinuxResult<isize> {
    Ok(0)
}

pub fn sys_getegid() -> LinuxResult<isize> {
    Ok(1)
}

pub fn sys_setresuid(ruid: i32, euid: i32, suid: i32) -> LinuxResult<isize> {
    debug!("sys_setresuid: ruid={}, euid={}, suid={}", ruid, euid, suid);
    // For simplified implementation, just return success
    // In a real system, this would check permissions and set the UIDs
    Ok(0)
}

pub fn sys_setresgid(rgid: i32, egid: i32, sgid: i32) -> LinuxResult<isize> {
    debug!("sys_setresgid: rgid={}, egid={}, sgid={}", rgid, egid, sgid);
    // For simplified implementation, just return success
    // In a real system, this would check permissions and set the GIDs
    Ok(0)
}

pub fn sys_socket(domain: i32, socket_type: i32, protocol: i32) -> LinuxResult<isize> {
    debug!("sys_socket: domain={}, type={}, protocol={}", domain, socket_type, protocol);
    // For simplified implementation, return error - socket not supported
    // This prevents bash from trying to use network features
    Err(axerrno::LinuxError::EAFNOSUPPORT)
}

const fn pad_str(info: &str) -> [c_char; 65] {
    let mut data: [c_char; 65] = [0; 65];
    // this needs #![feature(const_copy_from_slice)]
    // data[..info.len()].copy_from_slice(info.as_bytes());
    unsafe {
        core::ptr::copy_nonoverlapping(info.as_ptr().cast(), data.as_mut_ptr(), info.len());
    }
    data
}

const UTSNAME: new_utsname = new_utsname {
    sysname: pad_str("Starry"),
    nodename: pad_str("Starry - machine[0]"),
    release: pad_str("10.0.0"),
    version: pad_str("10.0.0"),
    machine: pad_str("10.0.0"),
    domainname: pad_str("https://github.com/oscomp/starry-next"),
};

pub fn sys_uname(name: UserPtr<new_utsname>) -> LinuxResult<isize> {
    *name.get_as_mut()? = UTSNAME;
    Ok(0)
}

pub fn sys_sysinfo(info: UserPtr<sysinfo>) -> LinuxResult<isize> {
    debug!("sys_sysinfo");
    
    let sysinfo_data = sysinfo {
        uptime: 60,                    // 系统运行时间（秒）
        loads: [0, 0, 0],             // 1, 5, 15分钟负载平均值
        totalram: 134217728,          // 总内存（128MB）
        freeram: 67108864,            // 空闲内存（64MB）
        sharedram: 0,                 // 共享内存
        bufferram: 0,                 // 缓冲区内存
        totalswap: 0,                 // 总交换空间
        freeswap: 0,                  // 空闲交换空间
        procs: 1,                     // 进程数
        pad: 0,                       // 填充字段
        totalhigh: 0,                 // 高端内存总量
        freehigh: 0,                  // 高端内存空闲量
        mem_unit: 1,                  // 内存单位大小
        _f: __IncompleteArrayField::new(),  // 不完整数组字段
    };
    
    *info.get_as_mut()? = sysinfo_data;
    Ok(0)
}
