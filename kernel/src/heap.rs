use core::mem::MaybeUninit;

use linked_list_allocator::LockedHeap;
use uefi_kernel::{KERNEL_HEAP_SIZE, KERNEL_HEAP_VIRT, frame_alloc::FrameUsageType};
use x86_64::{
    VirtAddr,
    structures::paging::{Mapper, OffsetPageTable, Page, PageTableFlags, Size2MiB},
};

use crate::frame_alloc::KernelFrameAllocator;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub fn init(frame_alloc: &mut KernelFrameAllocator, mapper: &mut OffsetPageTable) {
    let heap_bottom = VirtAddr::new(KERNEL_HEAP_VIRT);
    let heap_size = KERNEL_HEAP_SIZE;
    assert_eq!(heap_size % (16 * 1024 * 1024), 0);

    let page_range = Page::<Size2MiB>::containing_address(heap_bottom)
        ..=Page::containing_address(heap_bottom + heap_size - 1);

    for pages in page_range.array_chunks::<8>() {
        let mut frames = [MaybeUninit::uninit(); 8];
        let allocated = frame_alloc.allocate_frames_ty(&mut frames, FrameUsageType::KernelHeap);
        assert_eq!(allocated, frames.len());
        let frames = unsafe { MaybeUninit::array_assume_init(frames) };

        for (page, frame) in pages.into_iter().zip(frames) {
            unsafe {
                mapper.map_to(
                    page,
                    frame,
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE,
                    frame_alloc,
                )
            }
            .unwrap()
            .flush();
        }
        frame_alloc.frame_tracker.merge_all();
    }

    unsafe {
        ALLOCATOR
            .lock()
            .init(heap_bottom.as_mut_ptr(), heap_size as usize);
    }
}
