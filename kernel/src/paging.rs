use uefi_kernel::{BOOT_INFO_VIRT, MEM_OFFSET, frame_alloc::init_offset_page_table};
use x86_64::{
    VirtAddr,
    instructions::tlb,
    structures::paging::{Mapper, OffsetPageTable, Page, Size4KiB},
};

/// # Safety
/// Assumes that all phys addrs are mapped at offset MEM_OFFSET
pub unsafe fn get_page_table() -> OffsetPageTable<'static> {
    unsafe { init_offset_page_table(VirtAddr::new(MEM_OFFSET)) }
}

/// # Safety
/// User needs to make sure any addrs in lower half are out of use and that the use of BOOT_INFO_VIRT is done
pub unsafe fn cleanup_mappings(page_table: &mut OffsetPageTable) {
    // remove the boot info mapping
    // ignore the mapper flush since we flush tlb at the end of the function anyway
    let _ =page_table.unmap(Page::<Size4KiB>::containing_address(VirtAddr::new(
        BOOT_INFO_VIRT,
    ))).expect("failed to unmap boot_info");
    // remove uefi identity mapping uses directly deleting entries because we dont care to dealloc frames
    // since they were allocated by uefi fw
    // lower half of address space is p4 0..256, upper half (mapped) is 256..512
    let table = page_table.level_4_table_mut();
    for i in 0..256 {
        table[i].set_unused(); // clears the entry
    }
    tlb::flush_all(); // apply the changes
}
