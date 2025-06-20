use uefi::boot::{MemoryDescriptor, MemoryType};
use x86_64::{
    PhysAddr, VirtAddr,
    structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB},
};

pub struct BootFrameAllocator {
    mmap: &'static [MemoryDescriptor],
    next_frame: usize,
}
impl BootFrameAllocator {
    pub fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        let usable_regions = self
            .mmap
            .iter()
            .filter(|x| x.ty == MemoryType::CONVENTIONAL);
        let usable_ranges =
            usable_regions.map(|x| x.phys_start..(x.phys_start + x.page_count * 4096));
        let usable_frames = usable_ranges
            .flat_map(|x| x.step_by(4096))
            .filter(|x| *x != 0);
        usable_frames.map(|x| PhysFrame::containing_address(PhysAddr::new(x)))
    }

    /// # Safety
    /// Caller must guarrantee that mmap is valid and that all frames marked as `USABLE` are unused
    pub unsafe fn new(mmap: &'static [MemoryDescriptor]) -> Self {
        Self {
            mmap,
            next_frame: 0,
        }
    }
}
unsafe impl FrameAllocator<Size4KiB> for BootFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let frame = self.usable_frames().nth(self.next_frame);
        self.next_frame += 1;
        frame
    }
}

/// # Safety
/// Requires that Cr3 holds a valid page table and that memory is completely mapped with the phisical_memory_offset provided
pub unsafe fn init_offset_page_table(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();
    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();
    unsafe { OffsetPageTable::new(&mut *page_table_ptr, physical_memory_offset) }
}
