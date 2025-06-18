#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

use alloc::vec;
use log::info;
use uefi::boot::MemoryDescriptor;
use uefi::boot::OpenProtocolAttributes;
use uefi::boot::OpenProtocolParams;
use uefi::mem::memory_map::MemoryMap;
use uefi::prelude::*;
use uefi::proto::console::gop::FrameBuffer;
use uefi::proto::console::gop::GraphicsOutput;
use uefi::proto::console::gop::ModeInfo;
use uefi::proto::media::file::File;
use uefi::proto::media::file::FileAttribute;
use uefi::proto::media::file::FileMode;
use uefi::table::cfg::ACPI_GUID;
use uefi::table::cfg::ACPI2_GUID;

extern crate alloc;

use core::alloc::Layout;
use core::arch::asm;
use core::mem::forget;

use crate::enumerate_dir::EnumerateDir;

mod enumerate_dir;

#[entry]
fn efi_main() -> Status {
    uefi::helpers::init().unwrap();
    system::with_stdout(|x| x.clear());

    // get the acpi rsdp table
    let rsdp = system::with_config_table(|entries| {
        entries
            .iter()
            .find(|entry| matches!(entry.guid, ACPI_GUID | ACPI2_GUID))
            .map(|entry| entry.address)
    });
    info!("rsdp found at: {rsdp:?}");

    // initialize framebuffer
    let handle = boot::get_handle_for_protocol::<GraphicsOutput>().unwrap();
    let mut graphics = unsafe { boot::open_protocol::<GraphicsOutput>(OpenProtocolParams { handle, agent: boot::image_handle(), controller: None }, OpenProtocolAttributes::GetProtocol) }.unwrap();
    let graphics_mode_info = graphics.current_mode_info();

    // find and load the kernel into memory
    let fs = EnumerateDir::from(
        boot::get_image_file_system(boot::image_handle())
            .unwrap()
            .open_volume()
            .unwrap(),
    )
    .find(|x| x.is_regular_file() && x.file_name() == uefi::cstr16!("kernel.elf"));
    info!("bootfs loaded");

    let kernel_size = fs.expect("couldn't find kernel.elf").file_size();
    info!("found kernel");
    let mut kernel = boot::get_image_file_system(boot::image_handle())
        .unwrap()
        .open_volume()
        .unwrap()
        .open(
            cstr16!("kernel.elf"),
            FileMode::Read,
            FileAttribute::empty(),
        )
        .unwrap()
        .into_regular_file()
        .unwrap();
    let mut buffer = vec![0; kernel_size as usize];
    let mut read = 0;
    while read < kernel_size {
        read += kernel.read(&mut buffer[read as usize..]).unwrap() as u64;
    }
    info!("loaded kernel elf");

    // parse the elf and load the segments into memory
    let kernel = xmas_elf::ElfFile::new(&buffer).unwrap();
    xmas_elf::header::sanity_check(&kernel).unwrap();
    for segment in kernel.program_iter() {
        xmas_elf::program::sanity_check(segment, &kernel).unwrap();
        if let xmas_elf::program::Type::Load = segment.get_type().unwrap() {
            let mem_size = segment.mem_size() as usize;
            let file_size = segment.file_size() as usize;
            let virt_addr = segment.virtual_addr() as *mut u8;
            let file_offset = segment.offset() as usize;

            unsafe {
                let dst = core::slice::from_raw_parts_mut(virt_addr, mem_size);
                let src = &buffer[file_offset..file_offset + file_size];
                dst[..file_size].copy_from_slice(src);
                
                // bss section so fill with zeros
                if mem_size > file_size {
                    dst[file_size..].fill(0);
                }
            }
        }
    }
    info!("loaded kernel");

    let frame_buffer = graphics.frame_buffer().as_mut_ptr();
    info!("{:?}\n{:?}", graphics_mode_info, frame_buffer);

    let mmap = unsafe { boot::exit_boot_services(None) };

    let k_entry = kernel.header.pt2.entry_point() as usize;
    let k_entry_fn: unsafe extern "C" fn(*const MemoryDescriptor, usize, *const ModeInfo, *mut u8) -> ! = unsafe {
        core::mem::transmute(k_entry)
    };

    forget(graphics);

    unsafe { k_entry_fn(mmap.buffer().as_ptr().cast(), mmap.len(), &graphics_mode_info, frame_buffer) };
}

#[alloc_error_handler]
fn alloc_error(_layout: Layout) -> ! {
    panic!("out of memory")
}
