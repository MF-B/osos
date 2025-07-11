use core::ffi::{c_char, CStr};
use xmas_elf::ElfFile;

use alloc::{string::{String, ToString}, vec::Vec};
use axerrno::{LinuxError, LinuxResult};
use axhal::arch::TrapFrame;
use axtask::{TaskExtRef, current};
use starry_core::mm::{load_user_app, load_elf, map_trampoline};

use crate::{
    path::{handle_symlink_path},
    ptr::UserConstPtr,
};

/// Validate if a script file can be executed
fn validate_script(file_data: &[u8], script_path: &str) -> bool {
    // Parse the shebang line (first line starting with #!)
    let head = &file_data[2..file_data.len().min(256)];
    let pos = head.iter().position(|c| *c == b'\n').unwrap_or(head.len());
    let shebang_line = match core::str::from_utf8(&head[..pos]) {
        Ok(line) => line.trim(),
        Err(_) => {
            error!("Failed to parse shebang line as UTF-8 in {}", script_path);
            return false;
        }
    };

    if shebang_line.is_empty() {
        error!("Empty shebang line in {}", script_path);
        return false;
    }

    // Parse interpreter and check if it exists
    let parts: Vec<&str> = shebang_line.split_whitespace().collect();
    if parts.is_empty() {
        error!("No interpreter found in shebang line in {}", script_path);
        return false;
    }

    let interpreter_path = parts[0];
    info!("Checking interpreter path: {} for script {}", interpreter_path, script_path);
    
    // Check if interpreter exists
    let exists = axfs::api::absolute_path_exists(interpreter_path);
    if !exists {
        error!("Interpreter {} not found for script {}", interpreter_path, script_path);
    }
    exists
}

/// Validate if an ELF file can be executed
fn validate_elf(file_data: &[u8]) -> bool {
    // Check ELF magic
    if file_data.len() < 4 || &file_data[0..4] != b"\x7fELF" {
        return false;
    }
    
    // Try to parse ELF file
    match ElfFile::new(file_data) {
        Ok(elf) => {
            // Check if it has a valid entry point
            if elf.header.pt2.entry_point() == 0 {
                return false;
            }
            
            // If it has an interpreter, check if the interpreter exists
            if let Some(interp) = elf
                .program_iter()
                .find(|ph| ph.get_type() == Ok(xmas_elf::program::Type::Interp))
            {
                if let Ok(xmas_elf::program::SegmentData::Undefined(data)) = interp.get_data(&elf) {
                    if let Ok(interp_cstr) = CStr::from_bytes_with_nul(data) {
                        if let Ok(interp_str) = interp_cstr.to_str() {
                            let mut interp_path = match axfs::api::canonicalize(interp_str) {
                                Ok(path) => path,
                                Err(_) => return false,
                            };
                            
                            // Handle standard interpreter paths
                            if interp_path == "/lib/ld-linux-riscv64-lp64.so.1"
                                || interp_path == "/lib64/ld-linux-loongarch-lp64d.so.1"
                                || interp_path == "/lib64/ld-linux-x86-64.so.2"
                                || interp_path == "/lib/ld-linux-aarch64.so.1"
                                || interp_path == "/lib/ld-linux-riscv64-lp64d.so.1"
                                || interp_path == "/lib/ld-musl-riscv64-sf.so.1"
                                || interp_path == "/lib/ld-musl-riscv64.so.1"
                                || interp_path == "/lib64/ld-musl-loongarch-lp64d.so.1"
                            {
                                interp_path = String::from("/musl/lib/libc.so");
                            }
                            
                            return axfs::api::absolute_path_exists(&interp_path);
                        }
                    }
                }
                return false;
            }
            
            true
        }
        Err(_) => false,
    }
}

pub fn sys_execve(
    tf: &mut TrapFrame,
    path: UserConstPtr<c_char>,
    argv: UserConstPtr<UserConstPtr<c_char>>,
    envp: UserConstPtr<UserConstPtr<c_char>>,
) -> LinuxResult<isize> {
    // 路径处理
    let path = path.get_as_str()?.to_string();
    let mut absolute_path = handle_symlink_path(-100, path.as_str())?;

    let args = argv
        .get_as_null_terminated()?
        .iter()
        .map(|arg| arg.get_as_str().map(Into::into))
        .collect::<Result<Vec<_>, _>>()?;

    let envs = envp
        .get_as_null_terminated()?
        .iter()
        .map(|env| env.get_as_str().map(Into::into))
        .collect::<Result<Vec<_>, _>>()?;

    info!(
        "sys_execve: path: {:?}, args: {:?}, envs: {:?}",
        path, args, envs
    );

    let curr = current();
    let curr_ext = curr.task_ext();

    if curr_ext.thread.process().threads().len() > 1 {
        // TODO: handle multi-thread case
        error!("sys_execve: multi-thread not supported");
        return Err(LinuxError::EAGAIN);
    }

    // Read file data first, before modifying address space
    if absolute_path == "/proc/self/exe" {
        // Special case for /proc/self/exe
        let exec_path = "/bin/sh".to_string();
        absolute_path = handle_symlink_path(-100, exec_path.as_str())?;
    }
    let file_data = axfs::api::read(absolute_path.as_str()).map_err(|_| {
        error!("Failed to read file {}", absolute_path);
        LinuxError::ENOENT
    })?;

    // Validate file format before proceeding
    if !file_data.starts_with(b"#!") && 
       (file_data.len() < 4 || &file_data[0..4] != b"\x7fELF") {
        error!("Unsupported file format for {}", absolute_path);
        return Err(LinuxError::ENOEXEC);
    }

    // Validate that the file can be loaded before clearing address space
    let can_load = if file_data.starts_with(b"#!") {
        // For script files, validate shebang format
        validate_script(&file_data, absolute_path.as_str())
    } else {
        // For ELF files, validate ELF format
        validate_elf(&file_data)
    };
    
    if !can_load {
        error!("File validation failed for {}", absolute_path);
        return Err(LinuxError::ENOEXEC);
    }

    // Only clear address space after we've validated the file can be loaded
    let mut aspace = curr_ext.process_data().aspace.lock();
    aspace.unmap_user_areas()?;
    map_trampoline(&mut aspace)?;
    axhal::arch::flush_tlb(None);

    let (entry_point, user_stack_base) = if file_data.starts_with(b"#!") {
        // Handle script files
        load_user_app(&mut aspace, absolute_path.as_str(), &args, &envs).map_err(|_| {
            error!("Failed to load script {}", absolute_path);
            LinuxError::ENOEXEC
        })?
    } else {
        // Handle ELF files directly with load_elf
        load_elf(&mut aspace, &file_data, &args, &envs).map_err(|_| {
            error!("Failed to load ELF {}", absolute_path);
            LinuxError::ENOEXEC
        })?
    };
    drop(aspace);

    let name = path
        .rsplit_once('/')
        .map_or(path.as_str(), |(_, name)| name);
    curr.set_name(name);
    *curr_ext.process_data().exe_path.write() = path;

    // TODO: fd close-on-exec

    tf.set_ip(entry_point.as_usize());
    tf.set_sp(user_stack_base.as_usize());
    Ok(0)
}
