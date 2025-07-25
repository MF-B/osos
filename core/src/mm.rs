//! User address space management.

use alloc::vec;
use alloc::{
    string::String,
    vec::Vec,
};
use axerrno::{AxError, AxResult};
use axhal::{mem::virt_to_phys, paging::MappingFlags};
use axmm::{AddrSpace, kernel_aspace};
use core::ffi::CStr;
use kernel_elf_parser::{AuxvEntry, ELFParser, app_stack_region};
use memory_addr::{MemoryAddr, PAGE_SIZE_4K, VirtAddr};
use xmas_elf::{ElfFile, program::SegmentData};
use crate::alloc::string::ToString;

/// Creates a new empty user address space.
pub fn new_user_aspace_empty() -> AxResult<AddrSpace> {
    AddrSpace::new_empty(
        VirtAddr::from_usize(axconfig::plat::USER_SPACE_BASE),
        axconfig::plat::USER_SPACE_SIZE,
    )
}

/// If the target architecture requires it, the kernel portion of the address
/// space will be copied to the user address space.
pub fn copy_from_kernel(aspace: &mut AddrSpace) -> AxResult {
    if !cfg!(target_arch = "aarch64") && !cfg!(target_arch = "loongarch64") {
        // ARMv8 (aarch64) and LoongArch64 use separate page tables for user space
        // (aarch64: TTBR0_EL1, LoongArch64: PGDL), so there is no need to copy the
        // kernel portion to the user page table.
        aspace.copy_mappings_from(&kernel_aspace().lock())?;
    }
    Ok(())
}

/// Map the signal trampoline to the user address space.
pub fn map_trampoline(aspace: &mut AddrSpace) -> AxResult {
    let signal_trampoline_paddr = virt_to_phys(axsignal::arch::signal_trampoline_address().into());
    aspace.map_linear(
        axconfig::plat::SIGNAL_TRAMPOLINE.into(),
        signal_trampoline_paddr,
        PAGE_SIZE_4K,
        MappingFlags::READ | MappingFlags::EXECUTE | MappingFlags::USER,
        axhal::paging::PageSize::Size4K,
    )?;
    Ok(())
}

/// Map the elf file to the user address space.
///
/// # Arguments
/// - `uspace`: The address space of the user app.
/// - `elf`: The elf file.
///
/// # Returns
/// - The entry point of the user app.
fn map_elf(uspace: &mut AddrSpace, elf: &ElfFile) -> AxResult<(VirtAddr, [AuxvEntry; 17])> {
    let uspace_base = uspace.base().as_usize();
    let elf_parser = ELFParser::new(
        elf,
        axconfig::plat::USER_INTERP_BASE,
        Some(uspace_base as isize),
        uspace_base,
    )
    .map_err(|_| AxError::InvalidData)?;

    for segement in elf_parser.ph_load() {
        debug!(
            "Mapping ELF segment: [{:#x?}, {:#x?}) flags: {:#x?}",
            segement.vaddr,
            segement.vaddr + segement.memsz as usize,
            segement.flags
        );
        let seg_pad = segement.vaddr.align_offset_4k();
        assert_eq!(seg_pad, segement.offset % PAGE_SIZE_4K);

        let seg_align_size =
            (segement.memsz as usize + seg_pad + PAGE_SIZE_4K - 1) & !(PAGE_SIZE_4K - 1);
        uspace.map_alloc(
            segement.vaddr.align_down_4k(),
            seg_align_size,
            segement.flags,
            true,
            axhal::paging::PageSize::Size4K,
        )?;
        let seg_data = elf
            .input
            .get(segement.offset..segement.offset + segement.filesz as usize)
            .ok_or(AxError::InvalidData)?;
        uspace.write(segement.vaddr, axhal::paging::PageSize::Size4K, seg_data)?;
        // TDOO: flush the I-cache
    }

    Ok((
        elf_parser.entry().into(),
        elf_parser.auxv_vector(PAGE_SIZE_4K),
    ))
}

/// Load the user app to the user address space.
///
/// # Arguments
/// - `uspace`: The address space of the user app.
/// - `path`: The path of the executable file to load.
/// - `args`: The arguments of the user app. The first argument should be the program name.
/// - `envs`: The environment variables of the user app.
///
/// # Returns
/// - The entry point of the user app.
/// - The stack pointer of the user app.
pub fn load_user_app(
    uspace: &mut AddrSpace,
    path: &str,
    args: &[String],
    envs: &[String],
) -> AxResult<(VirtAddr, VirtAddr)> {
    if args.is_empty() {
        return Err(AxError::InvalidInput);
    }
    
    let file_data = axfs::api::read(path)?;
    
    // Check if the file is a script (e.g., shell script).
    if file_data.starts_with(b"#!") {
        return load_script(uspace, path, args, envs, &file_data);
    }
    
    // For ELF files, use the dedicated load_elf function
    load_elf(uspace, &file_data, args, envs)
}

/// Load an ELF file to the user address space.
///
/// # Arguments
/// - `uspace`: The address space of the user app.
/// - `elf_data`: The content of the ELF file.
/// - `args`: The arguments of the user app. The first argument should be the program name.
/// - `envs`: The environment variables of the user app.
///
/// # Returns
/// - The entry point of the user app.
/// - The stack pointer of the user app.
pub fn load_elf(
    uspace: &mut AddrSpace,
    elf_data: &[u8],
    args: &[String],
    envs: &[String],
) -> AxResult<(VirtAddr, VirtAddr)> {
    if args.is_empty() {
        return Err(AxError::InvalidInput);
    }
    
    // Check if the data is an ELF binary.
    if elf_data.len() < 4 || &elf_data[0..4] != b"\x7fELF" {
        return Err(AxError::InvalidData);
    }
    
    let elf = ElfFile::new(elf_data).map_err(|_| AxError::InvalidData)?;

    if let Some(interp) = elf
        .program_iter()
        .find(|ph| ph.get_type() == Ok(xmas_elf::program::Type::Interp))
    {
        let interp = match interp.get_data(&elf) {
            Ok(SegmentData::Undefined(data)) => data,
            _ => panic!("Invalid data in Interp Elf Program Header"),
        };

        let mut interp_path = axfs::api::canonicalize(
            CStr::from_bytes_with_nul(interp)
                .map_err(|_| AxError::InvalidData)?
                .to_str()
                .map_err(|_| AxError::InvalidData)?,
        )?;

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

        // Set the first argument to the interpreter name, then add original args
        let interp_name = interp_path
            .rsplit_once('/')
            .map_or(interp_path.as_str(), |(_, name)| name);
        let mut new_args = vec![interp_name.to_string()];
        new_args.extend_from_slice(args);
        return load_user_app(uspace, &interp_path, &new_args, envs);
    }

    let (entry, mut auxv) = map_elf(uspace, &elf)?;
    
    // The user stack is divided into two parts:
    // `ustack_start` -> `ustack_pointer`: It is the stack space that users actually read and write.
    // `ustack_pointer` -> `ustack_end`: It is the space that contains the arguments, environment variables and auxv passed to the app.
    //  When the app starts running, the stack pointer points to `ustack_pointer`.
    let ustack_end = VirtAddr::from_usize(axconfig::plat::USER_STACK_TOP);
    let ustack_size = axconfig::plat::USER_STACK_SIZE;
    let ustack_start = ustack_end - ustack_size;
    debug!(
        "Mapping user stack: {:#x?} -> {:#x?}",
        ustack_start, ustack_end
    );

    let stack_data = app_stack_region(args, envs, &mut auxv, ustack_start, ustack_size);
    uspace.map_alloc(
        ustack_start,
        ustack_size,
        MappingFlags::READ | MappingFlags::WRITE | MappingFlags::USER,
        true,
        axhal::paging::PageSize::Size4K,
    )?;

    let heap_start = VirtAddr::from_usize(axconfig::plat::USER_HEAP_BASE);
    let heap_size = axconfig::plat::USER_HEAP_SIZE;
    uspace.map_alloc(
        heap_start,
        heap_size,
        MappingFlags::READ | MappingFlags::WRITE | MappingFlags::USER,
        true,
        axhal::paging::PageSize::Size4K,
    )?;

    let user_sp = ustack_end - stack_data.len();

    uspace.write(
        user_sp,
        axhal::paging::PageSize::Size4K,
        stack_data.as_slice(),
    )?;

    Ok((entry, user_sp))
}

/// Load a script file to the user address space.
///
/// This function handles script files that start with shebang (#!).
/// It parses the shebang line and recursively loads the interpreter.
///
/// # Arguments
/// - `uspace`: The address space of the user app.
/// - `script_path`: The path of the script file.
/// - `args`: The original arguments.
/// - `envs`: The environment variables.
/// - `file_data`: The content of the script file.
///
/// # Returns
/// - The entry point of the interpreter.
/// - The stack pointer of the user app.
fn load_script(
    uspace: &mut AddrSpace,
    script_path: &str,
    args: &[String],
    envs: &[String],
    file_data: &[u8],
) -> AxResult<(VirtAddr, VirtAddr)> {
    // Parse the shebang line (first line starting with #!)
    let head = &file_data[2..file_data.len().min(256)];
    let pos = head.iter().position(|c| *c == b'\n').unwrap_or(head.len());
    let shebang_line = core::str::from_utf8(&head[..pos])
        .map_err(|_| AxError::InvalidData)?
        .trim();

    if shebang_line.is_empty() {
        return Err(AxError::InvalidData);
    }

    // Parse interpreter and its arguments
    let parts: Vec<&str> = shebang_line.split_whitespace().collect();
    if parts.is_empty() {
        return Err(AxError::InvalidData);
    }

    let interpreter_path = parts[0];
    
    // Build new arguments for the interpreter
    let mut new_args = Vec::new();
    
    // First argument: interpreter name (basename)
    let interpreter_name = interpreter_path
        .rsplit_once('/')
        .map_or(interpreter_path, |(_, name)| name);
    new_args.push(interpreter_name.to_string());
    
    // Add interpreter arguments from shebang (if any)
    for &arg in &parts[1..] {
        new_args.push(arg.to_string());
    }
    
    // Add the script path as an argument
    new_args.push(script_path.to_string());
    
    // Add remaining user arguments (skip the original script name)
    for arg in &args[1..] {
        new_args.push(arg.clone());
    }

    debug!(
        "Loading script: interpreter={}, args={:?}",
        interpreter_path, new_args
    );

    // Recursively load the interpreter
    load_user_app(uspace, interpreter_path, &new_args, envs)
}

#[percpu::def_percpu]
static mut ACCESSING_USER_MEM: bool = false;

/// Enables scoped access into user memory, allowing page faults to occur inside
/// kernel.
pub fn access_user_memory<R>(f: impl FnOnce() -> R) -> R {
    ACCESSING_USER_MEM.with_current(|v| {
        *v = true;
        let result = f();
        *v = false;
        result
    })
}

/// Check if the current thread is accessing user memory.
pub fn is_accessing_user_memory() -> bool {
    ACCESSING_USER_MEM.read_current()
}
