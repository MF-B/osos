use axerrno::LinuxError;
use axhal::{
    arch::TrapFrame,
    trap::{SYSCALL, register_trap_handler},
};
use starry_api::*;
use starry_core::task::{time_stat_from_kernel_to_user, time_stat_from_user_to_kernel};
use syscalls::Sysno;

#[register_trap_handler(SYSCALL)]
fn handle_syscall(tf: &mut TrapFrame, syscall_num: usize) -> isize {
    let sysno = Sysno::from(syscall_num as u32);
    info!("Syscall {}", sysno);
    time_stat_from_user_to_kernel();
    let result = match sysno {
        // fs ctl
        Sysno::ioctl => sys_ioctl(tf.arg0() as _, tf.arg1() as _, tf.arg2().into()),
        Sysno::chdir => sys_chdir(tf.arg0().into()),
        Sysno::mkdirat => sys_mkdirat(tf.arg0() as _, tf.arg1().into(), tf.arg2() as _),
        Sysno::getdents64 => sys_getdents64(tf.arg0() as _, tf.arg1().into(), tf.arg2() as _),
        Sysno::linkat => sys_linkat(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2() as _,
            tf.arg3().into(),
            tf.arg4() as _,
        ),
        #[cfg(target_arch = "x86_64")]
        Sysno::link => sys_link(tf.arg0().into(), tf.arg1().into()),
        Sysno::unlinkat => sys_unlinkat(tf.arg0() as _, tf.arg1().into(), tf.arg2() as _),
        #[cfg(target_arch = "x86_64")]
        Sysno::unlink => sys_unlink(tf.arg0().into()),
        Sysno::getcwd => sys_getcwd(tf.arg0().into(), tf.arg1() as _),

        // fd ops
        Sysno::openat => sys_openat(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2() as _,
            tf.arg3() as _,
        ),
        #[cfg(target_arch = "x86_64")]
        Sysno::open => sys_open(tf.arg0().into(), tf.arg1() as _, tf.arg2() as _),
        Sysno::close => sys_close(tf.arg0() as _),
        Sysno::dup => sys_dup(tf.arg0() as _),
        #[cfg(target_arch = "x86_64")]
        Sysno::dup2 => sys_dup2(tf.arg0() as _, tf.arg1() as _),
        Sysno::dup3 => sys_dup2(tf.arg0() as _, tf.arg1() as _),
        Sysno::fcntl => sys_fcntl(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        Sysno::renameat2 => sys_renameat2(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2() as _,
            tf.arg3().into(),
            tf.arg4() as _,
        ),

        // io
        Sysno::read => sys_read(tf.arg0() as _, tf.arg1().into(), tf.arg2() as _),
        Sysno::readv => sys_readv(tf.arg0() as _, tf.arg1().into(), tf.arg2() as _),
        Sysno::write => sys_write(tf.arg0() as _, tf.arg1().into(), tf.arg2() as _),
        Sysno::writev => sys_writev(tf.arg0() as _, tf.arg1().into(), tf.arg2() as _),
        Sysno::pwrite64 => sys_pwrite64(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2() as _,
            tf.arg3() as _,
        ),
        Sysno::pread64 => sys_pread64(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2() as _,
            tf.arg3() as _,
        ),
        Sysno::lseek => sys_lseek(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        Sysno::ftruncate => sys_ftruncate(tf.arg0() as _, tf.arg1() as _),
        Sysno::fsync => sys_fsync(tf.arg0() as _),
        Sysno::sync => sys_fsync(tf.arg0() as _),

        // fs mount
        Sysno::mount => sys_mount(
            tf.arg0().into(),
            tf.arg1().into(),
            tf.arg2().into(),
            tf.arg3() as _,
            tf.arg4().into(),
        ) as _,
        Sysno::umount2 => sys_umount2(tf.arg0().into(), tf.arg1() as _) as _,

        // pipe
        Sysno::pipe2 => sys_pipe2(tf.arg0().into(), tf.arg1() as _),
        #[cfg(target_arch = "x86_64")]
        Sysno::pipe => sys_pipe2(tf.arg0().into(), 0),

        // fs stat
        #[cfg(target_arch = "x86_64")]
        Sysno::stat => sys_stat(tf.arg0().into(), tf.arg1().into()),
        Sysno::fstat => sys_fstat(tf.arg0() as _, tf.arg1().into()),
        #[cfg(target_arch = "x86_64")]
        Sysno::lstat => sys_lstat(tf.arg0().into(), tf.arg1().into()),
        #[cfg(target_arch = "x86_64")]
        Sysno::newfstatat => sys_fstatat(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2().into(),
            tf.arg3() as _,
        ),
        #[cfg(not(target_arch = "x86_64"))]
        Sysno::fstatat => sys_fstatat(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2().into(),
            tf.arg3() as _,
        ),
        Sysno::statx => sys_statx(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2() as _,
            tf.arg3() as _,
            tf.arg4().into(),
        ),

        // mm
        Sysno::brk => sys_brk(tf.arg0() as _),
        Sysno::mmap => sys_mmap(
            tf.arg0(),
            tf.arg1() as _,
            tf.arg2() as _,
            tf.arg3() as _,
            tf.arg4() as _,
            tf.arg5() as _,
        ),
        Sysno::munmap => sys_munmap(tf.arg0(), tf.arg1() as _),
        Sysno::mprotect => sys_mprotect(tf.arg0(), tf.arg1() as _, tf.arg2() as _),

        // task info
        Sysno::getpid => sys_getpid(),
        Sysno::getppid => sys_getppid(),
        Sysno::gettid => sys_gettid(),
        Sysno::getpgid => sys_getpgid(tf.arg0() as _),

        // task sched
        Sysno::sched_yield => sys_sched_yield(),
        Sysno::nanosleep => sys_nanosleep(tf.arg0().into(), tf.arg1().into()),

        // task ops
        Sysno::execve => sys_execve(tf, tf.arg0().into(), tf.arg1().into(), tf.arg2().into()),
        Sysno::set_tid_address => sys_set_tid_address(tf.arg0()),
        #[cfg(target_arch = "x86_64")]
        Sysno::arch_prctl => sys_arch_prctl(tf, tf.arg0() as _, tf.arg1() as _),

        // task management
        Sysno::clone => sys_clone(
            tf,
            tf.arg0() as _,
            tf.arg1() as _,
            tf.arg2(),
            tf.arg3(),
            tf.arg4(),
        ),
        #[cfg(target_arch = "x86_64")]
        Sysno::fork => sys_fork(tf),
        Sysno::exit => sys_exit(tf.arg0() as _),
        Sysno::exit_group => sys_exit_group(tf.arg0() as _),
        Sysno::wait4 => sys_waitpid(tf.arg0() as _, tf.arg1().into(), tf.arg2() as _),

        // signal
        Sysno::rt_sigprocmask => sys_rt_sigprocmask(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2().into(),
            tf.arg3() as _,
        ),
        Sysno::rt_sigaction => sys_rt_sigaction(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2().into(),
            tf.arg3() as _,
        ),
        Sysno::rt_sigpending => sys_rt_sigpending(tf.arg0().into(), tf.arg1() as _),
        Sysno::rt_sigreturn => sys_rt_sigreturn(tf),
        Sysno::rt_sigtimedwait => sys_rt_sigtimedwait(
            tf.arg0().into(),
            tf.arg1().into(),
            tf.arg2().into(),
            tf.arg3() as _,
        ),
        Sysno::rt_sigsuspend => sys_rt_sigsuspend(tf, tf.arg0().into(), tf.arg1() as _),
        Sysno::kill => sys_kill(tf.arg0() as _, tf.arg1() as _),
        Sysno::tkill => sys_tkill(tf.arg0() as _, tf.arg1() as _),
        Sysno::tgkill => sys_tgkill(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        Sysno::rt_sigqueueinfo => sys_rt_sigqueueinfo(
            tf.arg0() as _,
            tf.arg1() as _,
            tf.arg2().into(),
            tf.arg3() as _,
        ),
        Sysno::rt_tgsigqueueinfo => sys_rt_tgsigqueueinfo(
            tf.arg0() as _,
            tf.arg1() as _,
            tf.arg2() as _,
            tf.arg3().into(),
            tf.arg4() as _,
        ),
        Sysno::sigaltstack => sys_sigaltstack(tf.arg0().into(), tf.arg1().into()),
        Sysno::futex => sys_futex(
            tf.arg0().into(),
            tf.arg1() as _,
            tf.arg2() as _,
            tf.arg3().into(),
            tf.arg4().into(),
            tf.arg5() as _,
        ),

        // sys
        Sysno::getuid => sys_getuid(),
        Sysno::geteuid => sys_geteuid(),
        Sysno::getgid => sys_getgid(),
        Sysno::getegid => sys_getegid(),
        Sysno::setresuid => sys_setresuid(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        Sysno::setresgid => sys_setresgid(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        Sysno::socket => sys_socket(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        Sysno::uname => sys_uname(tf.arg0().into()),

        // time
        Sysno::gettimeofday => sys_gettimeofday(tf.arg0().into()),
        Sysno::times => sys_times(tf.arg0().into()),
        Sysno::clock_gettime => sys_clock_gettime(tf.arg0() as _, tf.arg1().into()),

        // I/O multiplexing
        #[cfg(target_arch = "x86_64")]
        Sysno::poll => sys_poll(tf.arg0().into(), tf.arg1() as _, tf.arg2().into()),
        Sysno::ppoll => sys_poll(tf.arg0().into(), tf.arg1() as _, tf.arg2().into()),
        #[cfg(target_arch = "x86_64")]
        Sysno::select => sys_select(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2().into(),
            tf.arg3().into(),
            tf.arg4().into(),
        ),
        Sysno::pselect6 => sys_select(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2().into(),
            tf.arg3().into(),
            tf.arg4().into(),
        ),

        // shm
        Sysno::shmget => sys_shmget(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        Sysno::shmat => sys_shmat(tf.arg0() as _, tf.arg1() as _, tf.arg2() as _),
        Sysno::shmctl => sys_shmctl(tf.arg0() as _, tf.arg1() as _, tf.arg2().into()),
        Sysno::shmdt => sys_shmdt(tf.arg0() as _),

        // symlink
        Sysno::symlinkat => sys_symlinkat(tf.arg0().into(), tf.arg1() as _, tf.arg2().into()),
        Sysno::readlinkat => sys_readlinkat(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2().into(),
            tf.arg3() as _,
        ),

        // random
        Sysno::getrandom => sys_getrandom(tf.arg0().into(), tf.arg1() as _, tf.arg2() as _),

        // blank
        Sysno::faccessat => sys_faccessat(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2() as _,
            tf.arg3() as _,
        ),
        Sysno::prlimit64 => sys_prlimit64(
            tf.arg0() as _,
            tf.arg1() as _,
            tf.arg2().into(),
            tf.arg3().into(),
        ),
        Sysno::set_robust_list => sys_set_robust_list(tf.arg0().into(), tf.arg1() as _),

        Sysno::fchmodat => sys_fchmodat(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2() as _,
            tf.arg3() as _,
        ),

        Sysno::utimensat => sys_utimensat(
            tf.arg0() as _,
            tf.arg1().into(),
            tf.arg2().into(),
            tf.arg3() as _,
        ),

        Sysno::sysinfo => sys_sysinfo(tf.arg0().into()),

        Sysno::sendfile => sys_sendfile(
            tf.arg0() as _,
            tf.arg1() as _,
            tf.arg2().into(),
            tf.arg3() as _,
        ),

        _ => {
            error!("Unimplemented syscall: {}", sysno);
            Err(LinuxError::ENOSYS)
        }
    };
    let ans = result.unwrap_or_else(|err| -err.code() as _);
    time_stat_from_kernel_to_user();
    info!("Syscall {:?} return {}", sysno, ans);
    ans
}
