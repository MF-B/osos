use core::sync::atomic::Ordering;
use core::time::Duration;

use axerrno::LinuxResult;
use axtask::current;
use axtask::TaskExtRef;
use linux_raw_sys::general::__kernel_old_timeval;
use linux_raw_sys::general::rusage;

use crate::ptr::UserPtr;
use crate::time::TimeValueLike;
// use crate::rusage::Rusage;
// use crate::rusage::RUSAGE_BOTH;
// use crate::rusage::RUSAGE_CHILDREN;
// use crate::rusage::RUSAGE_SELF;
// use crate::rusage::RUSAGE_THREAD;
const RUSAGE_SELF: i32 = 0; // 当前进程的资源使用情况

pub fn sys_getrusage(
    who: isize,
    rusage: UserPtr<rusage>,
) -> LinuxResult<isize> {
    let curr = current();
    let task = curr.task_ext();

    let result:rusage = match who as i32 {
        // TODO!
        // RUSAGE_THREAD => {
        //     // 获取当前线程的资源使用情况
        //     // let usage = task.thread_data().rusage();
        //     // if usage.is_none() {
        //     //     return Err(axerrno::LinuxError::EINVAL);
        //     // }
        //     // let usage = usage.unwrap();
        //     // if usage.utime.is_zero() && usage.stime.is_zero() {
        //     //     return Err(axerrno::LinuxError::EINVAL);
        //     // }
        // },
        // RUSAGE_BOTH => {
        //     // 获取当前进程的资源使用情况
        //     // let usage = process_data.rusage();
        //     // if usage.is_none() {
        //     //     return Err(axerrno::LinuxError::EINVAL);
        //     // }
        //     // let usage = usage.unwrap();
        //     // if usage.utime.is_zero() && usage.stime.is_zero() {
        //     //     return Err(axerrno::LinuxError::EINVAL);
        //     // }
        // },
        // RUSAGE_CHILDREN => {
        //     // 获取当前进程的所有子进程的资源使用情况
        //     // let usage = process_data.children_rusage();
        //     // if usage.is_none() {
        //     //     return Err(axerrno::LinuxError::EINVAL);
        //     // }
        //     // let usage = usage.unwrap();
        //     // if usage.utime.is_zero() && usage.stime.is_zero() {
        //     //     return Err(axerrno::LinuxError::EINVAL);
        //     // }
        // },
        RUSAGE_SELF => {
            // 获取当前进程的资源使用情况
            let timestat = task.time.borrow().output();
            let minflt = task.minflt.load(Ordering::Relaxed);
            let majflt = task.majflt.load(Ordering::Relaxed);
            let res = rusage {
                ru_utime: __kernel_old_timeval::from_time_value(Duration::from_nanos(timestat.0 as _)),
                ru_stime: __kernel_old_timeval::from_time_value(Duration::from_nanos(timestat.1 as _)),
                ru_maxrss: 0, // TODO
                ru_ixrss: 0, // TODO
                ru_idrss: 0, // TODO
                ru_isrss: 0, // TODO
                ru_minflt: minflt as _,
                ru_majflt: majflt as _,
                ru_nswap: 0, // TODO
                ru_inblock: 0, // TODO
                ru_oublock: 0, // TODO
                ru_msgsnd: 0, // TODO
                ru_msgrcv: 0, // TODO
                ru_nsignals: 0, // TODO
                ru_nvcsw: 0,
                ru_nivcsw: 0,
            };
            res
        },
        _ => {
            // 无效的参数
            return Err(axerrno::LinuxError::EINVAL);
        }
    };

    // 将 rusage 数据写入用户空间
    let rusage = rusage.get_as_mut()?;
    (*rusage) = result;

    Ok(0)
}