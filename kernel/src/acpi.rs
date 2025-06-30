use core::ptr;

use acpi::{AcpiHandler, PhysicalMapping};
use uefi_kernel::MEM_OFFSET;

#[derive(Clone, Copy)]
pub struct Mapper;
impl AcpiHandler for Mapper {
    unsafe fn map_physical_region<T>(
        &self,
        physical_address: usize,
        size: usize,
    ) -> PhysicalMapping<Self, T> {
        // return the already offset mapped region
        unsafe {
            PhysicalMapping::new(
                physical_address,
                ptr::NonNull::new((physical_address + MEM_OFFSET as usize) as *mut _).unwrap(),
                size,
                size,
                self.clone(),
            )
        }
    }

    fn unmap_physical_region<T>(_region: &PhysicalMapping<Self, T>) {}
}
