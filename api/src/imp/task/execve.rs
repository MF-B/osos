use core::ffi::{c_char, CStr};
use xmas_elf::ElfFile;

use alloc::{string::{String, ToString}, vec::Vec};
use axerrno::{LinuxError, LinuxResult};
use axhal::arch::TrapFrame;
use axtask::{TaskExtRef, current};
use starry_core::mm::{load_user_app, load_elf, map_trampoline};

use crate::{
    path::{resolve_path_with_flags, PathFlags},
    ptr::UserConstPtr,
};

/// Supported interpreter paths that map to musl libc
const SUPPORTED_INTERPRETERS: &[&str] = &[
    "/lib/ld-linux-riscv64-lp64.so.1",
    "/lib64/ld-linux-loongarch-lp64d.so.1",
    "/lib64/ld-linux-x86-64.so.2",
    "/lib/ld-linux-aarch64.so.1",
    "/lib/ld-linux-riscv64-lp64d.so.1",
    "/lib/ld-musl-riscv64-sf.so.1",
    "/lib/ld-musl-riscv64.so.1",
    "/lib64/ld-musl-loongarch-lp64d.so.1",
];

const MUSL_LIBC_PATH: &str = "/musl/lib/libc.so";

/// File format validation result
#[derive(Debug, PartialEq)]
enum FileFormat {
    Script,
    Elf,
    Invalid,
}

/// Validation module for executable files
mod validation {
    use super::*;
    
    /// Parse shebang line and extract interpreter path
    fn parse_shebang(file_data: &[u8]) -> LinuxResult<String> {
        if file_data.len() < 2 || !file_data.starts_with(b"#!") {
            return Err(LinuxError::ENOEXEC);
        }
        
        let head = &file_data[2..file_data.len().min(256)];
        let pos = head.iter().position(|c| *c == b'\n').unwrap_or(head.len());
        
        let shebang_line = core::str::from_utf8(&head[..pos])
            .map_err(|_| LinuxError::ENOEXEC)?
            .trim();
            
        if shebang_line.is_empty() {
            return Err(LinuxError::ENOEXEC);
        }
        
        let interpreter_path = shebang_line
            .split_whitespace()
            .next()
            .ok_or(LinuxError::ENOEXEC)?;
            
        Ok(interpreter_path.to_string())
    }
    
    /// Validate if a script file can be executed
    pub fn validate_script(file_data: &[u8], script_path: &str) -> LinuxResult<()> {
        let interpreter_path = parse_shebang(file_data)?;
        
        info!("Checking interpreter path: {} for script {}", interpreter_path, script_path);
        
        if !axfs::api::absolute_path_exists(&interpreter_path) {
            error!("Interpreter {} not found for script {}", interpreter_path, script_path);
            return Err(LinuxError::ENOENT);
        }
        
        Ok(())
    }
    
    /// Resolve interpreter path to actual location
    fn resolve_interpreter_path(interp_path: &str) -> String {
        if SUPPORTED_INTERPRETERS.contains(&interp_path) {
            MUSL_LIBC_PATH.to_string()
        } else {
            interp_path.to_string()
        }
    }
    
    /// Extract and validate ELF interpreter
    fn validate_elf_interpreter(elf: &ElfFile) -> LinuxResult<()> {
        if let Some(interp) = elf
            .program_iter()
            .find(|ph| ph.get_type() == Ok(xmas_elf::program::Type::Interp))
        {
            let data = interp.get_data(elf)
                .map_err(|_| LinuxError::ENOEXEC)?;
                
            if let xmas_elf::program::SegmentData::Undefined(data) = data {
                let interp_cstr = CStr::from_bytes_with_nul(data)
                    .map_err(|_| LinuxError::ENOEXEC)?;
                let interp_str = interp_cstr.to_str()
                    .map_err(|_| LinuxError::ENOEXEC)?;
                    
                let canonical_path = axfs::api::canonicalize(interp_str)
                    .map_err(|_| LinuxError::ENOENT)?;
                let resolved_path = resolve_interpreter_path(&canonical_path);
                
                if !axfs::api::absolute_path_exists(&resolved_path) {
                    error!("ELF interpreter {} not found", resolved_path);
                    return Err(LinuxError::ENOENT);
                }
            }
        }
        Ok(())
    }
    
    /// Validate if an ELF file can be executed
    pub fn validate_elf(file_data: &[u8]) -> LinuxResult<()> {
        if file_data.len() < 4 || &file_data[0..4] != b"\x7fELF" {
            return Err(LinuxError::ENOEXEC);
        }
        
        let elf = ElfFile::new(file_data)
            .map_err(|_| LinuxError::ENOEXEC)?;
            
        if elf.header.pt2.entry_point() == 0 {
            return Err(LinuxError::ENOEXEC);
        }
        
        validate_elf_interpreter(&elf)?;
        Ok(())
    }
}

/// Determine file format from file data
fn detect_file_format(file_data: &[u8]) -> FileFormat {
    if file_data.starts_with(b"#!") {
        FileFormat::Script
    } else if file_data.len() >= 4 && &file_data[0..4] == b"\x7fELF" {
        FileFormat::Elf
    } else {
        FileFormat::Invalid
    }
}

/// Parse command line arguments from user space
fn parse_user_args(argv: UserConstPtr<UserConstPtr<c_char>>) -> LinuxResult<Vec<String>> {
    argv.get_as_null_terminated()?
        .iter()
        .map(|arg| arg.get_as_str().map(Into::into))
        .collect::<Result<Vec<_>, _>>()
}

/// Parse environment variables from user space
fn parse_user_envs(envp: UserConstPtr<UserConstPtr<c_char>>) -> LinuxResult<Vec<String>> {
    envp.get_as_null_terminated()?
        .iter()
        .map(|env| env.get_as_str().map(Into::into))
        .collect::<Result<Vec<_>, _>>()
}

/// Handle special path cases like /proc/self/exe
fn resolve_executable_path(path: &str) -> LinuxResult<String> {
    let resolved_path = if path == "/proc/self/exe" {
        "/bin/sh".to_string()
    } else {
        path.to_string()
    };
    
    resolve_path_with_flags(-100, &resolved_path, PathFlags::new())
        .map(|path| path.to_string())
}

/// Load executable into address space
fn load_executable(
    aspace: &mut axmm::AddrSpace,
    file_data: &[u8],
    absolute_path: &str,
    args: &[String],
    envs: &[String],
) -> LinuxResult<(memory_addr::VirtAddr, memory_addr::VirtAddr)> {
    match detect_file_format(file_data) {
        FileFormat::Script => {
            load_user_app(aspace, absolute_path, args, envs)
                .map_err(|_| LinuxError::ENOEXEC)
        }
        FileFormat::Elf => {
            load_elf(aspace, file_data, args, envs)
                .map_err(|_| LinuxError::ENOEXEC)
        }
        FileFormat::Invalid => Err(LinuxError::ENOEXEC),
    }
}

pub fn sys_execve(
    tf: &mut TrapFrame,
    path: UserConstPtr<c_char>,
    argv: UserConstPtr<UserConstPtr<c_char>>,
    envp: UserConstPtr<UserConstPtr<c_char>>,
) -> LinuxResult<isize> {
    let path = path.get_as_str()?.to_string();
    let args = parse_user_args(argv)?;
    let envs = parse_user_envs(envp)?;

    info!("sys_execve: path: {:?}, args: {:?}, envs: {:?}", path, args, envs);

    let curr = current();
    let curr_ext = curr.task_ext();

    // Check for multi-threaded process
    if curr_ext.thread.process().threads().len() > 1 {
        error!("sys_execve: multi-thread not supported");
        return Err(LinuxError::EAGAIN);
    }

    // Resolve executable path and read file data
    let absolute_path = resolve_executable_path(&path)?;
    let file_data = axfs::api::read(&absolute_path)
        .map_err(|_| {
            error!("Failed to read file {}", absolute_path);
            LinuxError::ENOENT
        })?;

    // Validate file format and executability
    let file_format = detect_file_format(&file_data);
    if file_format == FileFormat::Invalid {
        error!("Unsupported file format for {}", absolute_path);
        return Err(LinuxError::ENOEXEC);
    }

    // Validate that the file can be executed before clearing address space
    match file_format {
        FileFormat::Script => validation::validate_script(&file_data, &absolute_path)?,
        FileFormat::Elf => validation::validate_elf(&file_data)?,
        FileFormat::Invalid => return Err(LinuxError::ENOEXEC),
    }

    // Clear address space and set up new memory layout
    let mut aspace = curr_ext.process_data().aspace.lock();
    aspace.unmap_user_areas()?;
    map_trampoline(&mut aspace)?;
    axhal::arch::flush_tlb(None);

    // Load the new executable
    let (entry_point, user_stack_base) = load_executable(
        &mut aspace,
        &file_data,
        &absolute_path,
        &args,
        &envs,
    )?;
    drop(aspace);

    // Update process metadata
    let name = path.rsplit_once('/').map_or(path.as_str(), |(_, name)| name);
    curr.set_name(name);
    *curr_ext.process_data().exe_path.write() = path;

    // TODO: Handle file descriptor close-on-exec flags

    // Set up execution context
    tf.set_ip(entry_point.as_usize());
    tf.set_sp(user_stack_base.as_usize());
    
    Ok(0)
}
