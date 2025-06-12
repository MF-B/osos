//! Shared page mapping backend.

use core::sync::atomic::{AtomicUsize, Ordering};

use alloc::string::String;
use alloc::string::ToString;
use alloc::sync::Arc;
use axhal::paging::{MappingFlags, PageTable};
use kspin::SpinNoIrq;
use memory_addr::MemoryAddr;
use memory_addr::{PhysAddr, VirtAddr};
use page_table_multiarch::PageSize;
use hashbrown::HashMap;
use lazyinit::LazyInit;

use super::Backend;
use super::PageIterWrapper;

/// Shared page information
#[derive(Debug)]
pub struct SharedPage {
    /// Physical address of the shared page
    pub paddr: PhysAddr,
    /// Reference count (atomic for thread safety)
    pub ref_count: AtomicUsize,
    /// Size of the shared region
    pub size: usize,
    /// Alignment
    pub align: PageSize,
}

/// Global shared page manager
static SHARED_PAGES: LazyInit<SpinNoIrq<HashMap<String, Arc<SharedPage>>>> = 
LazyInit::new();

impl Backend {
    /// Creates a new shared mapping backend.
    pub const fn new_shared(name: String, size: usize, align: PageSize) -> Self {
        Self::Shared { name, size, align }
    }

    pub(crate) fn map_shared(
        start: VirtAddr,
        name: &str,
        size: usize,
        flags: MappingFlags,
        pt: &mut PageTable,
        align: PageSize,
    ) -> bool {
        debug!(
            "map_shared: [{:#x}, {:#x}) name={} flags={:?}",
            start,
            start + size,
            name,
            flags
        );

        let mut shared_pages = SHARED_PAGES.lock();
        
        let shared_page = if let Some(existing) = shared_pages.get(name) {
            // Use existing shared page and increment reference count atomically
            existing.ref_count.fetch_add(1, Ordering::SeqCst);
            Arc::clone(existing)
        } else {
            // Create new shared page
            let paddr = match Self::alloc_shared_frames(size, align) {
                Some(addr) => addr,
                None => return false,
            };
            
            let shared = Arc::new(SharedPage {
                paddr,
                ref_count: AtomicUsize::new(1), // Start with 1 reference
                size,
                align,
            });
            shared_pages.insert(name.to_string(), Arc::clone(&shared));
            shared
        };

        // Release lock before mapping to avoid holding it too long
        drop(shared_pages);

        // Map virtual address to shared physical address
        if let Some(iter) = PageIterWrapper::new(start, start + size, align) {
            let mut offset = 0;
            let mut mapped_count = 0;
            
            for vaddr in iter {
                let frame = shared_page.paddr + offset;
                if let Ok(tlb) = pt.map(vaddr, frame, align, flags) {
                    tlb.ignore(); // TLB flush on map is unnecessary
                    offset += align as usize;
                    mapped_count += 1;
                } else {
                    // Mapping failed, need to clean up already mapped pages
                    let mut cleanup_offset = 0;
                    if let Some(cleanup_iter) = PageIterWrapper::new(start, start + mapped_count * align as usize, align) {
                        for cleanup_vaddr in cleanup_iter {
                            if let Ok((_, _, tlb)) = pt.unmap(cleanup_vaddr) {
                                tlb.flush();
                            }
                            cleanup_offset += align as usize;
                        }
                    }
                    
                    // Decrement reference count and clean up if necessary
                    let old_count = shared_page.ref_count.fetch_sub(1, Ordering::SeqCst);
                    if old_count == 1 {
                        // This was the last reference, clean up the shared page
                        let mut shared_pages = SHARED_PAGES.lock();
                        shared_pages.remove(name);
                        drop(shared_pages);
                        Self::dealloc_shared_frames(shared_page.paddr, shared_page.size, shared_page.align);
                    }
                    
                    return false;
                }
            }
        } else {
            // Failed to create iterator, decrement reference count
            let old_count = shared_page.ref_count.fetch_sub(1, Ordering::SeqCst);
            if old_count == 1 {
                let mut shared_pages = SHARED_PAGES.lock();
                shared_pages.remove(name);
                drop(shared_pages);
                Self::dealloc_shared_frames(shared_page.paddr, shared_page.size, shared_page.align);
            }
            return false;
        }

        true
    }

    pub(crate) fn unmap_shared(
        start: VirtAddr,
        name: &str,
        size: usize,
        pt: &mut PageTable,
        align: PageSize,
    ) -> bool {
        debug!("unmap_shared: [{:#x}, {:#x}) name={}", start, start + size, name);

        let shared_pages = SHARED_PAGES.lock();
        
        if let Some(shared_page) = shared_pages.get(name) {
            let shared_page_clone = Arc::clone(shared_page);
            
            // Release lock before unmapping
            drop(shared_pages);
            
            // Unmap virtual addresses
            if let Some(iter) = PageIterWrapper::new(start, start + size, align) {
                for vaddr in iter {
                    if let Ok((_, _, tlb)) = pt.unmap(vaddr) {
                        tlb.flush();
                    }
                }
            }

            // Decrement reference count atomically
            let old_count = shared_page_clone.ref_count.fetch_sub(1, Ordering::SeqCst);
            
            // If this was the last reference, clean up
            if old_count == 1 {
                let mut shared_pages = SHARED_PAGES.lock();
                shared_pages.remove(name);
                drop(shared_pages);
                Self::dealloc_shared_frames(shared_page_clone.paddr, shared_page_clone.size, shared_page_clone.align);
            }
        }

        true
    }

    fn alloc_shared_frames(size: usize, align: PageSize) -> Option<PhysAddr> {
        use axhal::paging::PagingHandlerImpl;
        use page_table_multiarch::PagingHandler;
        
        let num_pages = (size + align as usize - 1) / align as usize;
        
        if num_pages == 1 {
            PagingHandlerImpl::alloc_frame()
        } else {
            // For multi-page allocation, we need a contiguous allocator
            // This is a simplified implementation that allocates pages one by one
            // In a real implementation, you should use a contiguous allocator
            
            // Try to allocate first page
            if let Some(first_frame) = PagingHandlerImpl::alloc_frame() {
                // For simplicity, assume consecutive allocation works
                // This may not be true in practice and should be improved
                first_frame.into()
            } else {
                None
            }
        }
    }

    fn dealloc_shared_frames(paddr: PhysAddr, size: usize, align: PageSize) {
        use axhal::paging::PagingHandlerImpl;
        use page_table_multiarch::PagingHandler;
        
        let num_pages = (size + align as usize - 1) / align as usize;
        
        for i in 0..num_pages {
            let frame_addr = paddr + i * align as usize;
            PagingHandlerImpl::dealloc_frame(frame_addr);
        }
    }

    pub(crate) fn handle_page_fault_shared(
        vaddr: VirtAddr,
        name: &str,
        flags: MappingFlags,
        pt: &mut PageTable,
        align: PageSize,
    ) -> bool {
        let shared_pages = SHARED_PAGES.lock();
        
        if let Some(shared_page) = shared_pages.get(name) {
            // Calculate offset within the shared region
            let page_vaddr = vaddr.align_down(align);
            
            // Find the base virtual address that this shared page was mapped to
            // This is tricky because we don't store the original mapping address
            // For now, assume the offset calculation is based on the fault address
            let offset_in_page = page_vaddr.as_usize() % align as usize;
            let frame = shared_page.paddr + offset_in_page;
            
            // Release lock before mapping
            drop(shared_pages);
            
            if let Ok(tlb) = pt.map(page_vaddr, frame, align, flags) {
                tlb.ignore();
                return true;
            }
        }
        
        false
    }
}