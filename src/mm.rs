use core::sync::atomic::Ordering;

use axhal::{
    mem::VirtAddr,
    paging::MappingFlags,
    trap::{PAGE_FAULT, register_trap_handler},
};
use axtask::{TaskExtRef, current};
use linux_raw_sys::general::SIGSEGV;
use starry_api::do_exit;
use starry_core::mm::is_accessing_user_memory;

#[register_trap_handler(PAGE_FAULT)]
fn handle_page_fault(vaddr: VirtAddr, access_flags: MappingFlags, is_user: bool) -> bool {
    trace!(
        "Page fault at {:#x}, access_flags: {:#x?}",
        vaddr, access_flags
    );
    if !is_user && !is_accessing_user_memory() {
        return false;
    }

    let curr = current();
    let result = curr
        .task_ext()
        .process_data()
        .aspace
        .lock()
        .handle_page_fault(vaddr, access_flags);

    if result {
        // 页面错误处理成功，判断是 minor 还是 major fault  
        if is_minor_fault(vaddr, access_flags) {  
            curr.task_ext().minflt.fetch_add(1, Ordering::Relaxed);  
        } else {  
            curr.task_ext().majflt.fetch_add(1, Ordering::Relaxed);  
        }  
    } else {
        warn!(
            "{} ({:?}): segmentation fault at {:#x}, exit!",
            curr.id_name(),
            curr.task_ext().thread,
            vaddr
        );
        do_exit(SIGSEGV as _, true);
    }

    true
}

fn is_minor_fault(_vaddr: VirtAddr, _access_flags: MappingFlags) -> bool {
    // 简化实现：当前系统主要是内存分配相关的页面错误
    // 都视为 minor fault，因为不涉及磁盘I/O
    true
}
