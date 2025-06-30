#![no_std]
#![no_main]
#![feature(iter_array_chunks)]
#![feature(maybe_uninit_array_assume_init)]

extern crate alloc;

use core::{arch::naked_asm, panic::PanicInfo};

use ::acpi::{AcpiTables, mcfg::Mcfg};
use log::info;
use uefi_kernel::{BootInfo, frame_alloc::FrameTrackerArray};

use crate::{
    frame_alloc::KernelFrameAllocator,
    framebuffer::FrameBuffer,
    paging::{cleanup_mappings, get_page_table},
};

mod acpi;
#[macro_use]
mod entry;
mod frame_alloc;
mod framebuffer;
mod heap;
mod logger;
mod paging;

entry_point!(kmain);
fn kmain(boot_info: BootInfo, frame_tracker: FrameTrackerArray, framebuffer: FrameBuffer) -> ! {
    let mut frame_alloc = KernelFrameAllocator::new(frame_tracker, boot_info.mmap);

    let mut page_table = unsafe { get_page_table() };

    heap::init(&mut frame_alloc, &mut page_table);

    logger::init(framebuffer);
    info!("Kernel initialized");

    info!("Cleaning up old page mappings");
    unsafe { cleanup_mappings(&mut page_table) };

    info!("Reading acpi tables");
    let acpi = unsafe { AcpiTables::from_rsdp(acpi::Mapper, boot_info.rsdp.addr()) }.unwrap();
    let mcfg = acpi.find_table::<Mcfg>().unwrap();
    info!("mcfg entries: {:?}", mcfg.entries());

    info!("done");
    loop {}
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    info!("{:?}: {}", info.location(), info.message());
    loop {}
}
