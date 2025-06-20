#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

use alloc::vec;
use log::info;
use uefi::{
    boot::{MemoryDescriptor, MemoryType, OpenProtocolAttributes, OpenProtocolParams},
    mem::memory_map::MemoryMap,
    prelude::*,
    proto::{
        console::gop::{BltPixel, GraphicsOutput},
        media::file::{File, FileAttribute, FileMode},
    },
    table::cfg::{ACPI_GUID, ACPI2_GUID},
};
use uefi_kernel::{
    BOOT_INFO_VIRT, BootInfo, MEM_OFFSET,
    frame_alloc::{self, max_phys_addr},
};
use x86_64::{
    PhysAddr, VirtAddr, align_up,
    registers::control::{Cr0, Cr0Flags, Efer, EferFlags},
    structures::paging::{
        Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size1GiB, Size4KiB,
    },
};

extern crate alloc;

use core::alloc::Layout;
use core::slice;

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
    let mut graphics = unsafe {
        boot::open_protocol::<GraphicsOutput>(
            OpenProtocolParams {
                handle,
                agent: boot::image_handle(),
                controller: None,
            },
            OpenProtocolAttributes::GetProtocol,
        )
    }
    .unwrap();
    let graphics_mode_info = graphics.current_mode_info();

    let frame_buffer = graphics.frame_buffer().as_mut_ptr();
    info!("{:?}\n{:?}", graphics_mode_info, frame_buffer);

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
    info!("loaded kernel elf at {:p}", buffer.as_ptr());

    let mut mapper = unsafe { init_offset_page_table(VirtAddr::zero()) };

    // parse the elf and load the segments into memory
    let kernel = xmas_elf::ElfFile::new(&buffer).unwrap();
    xmas_elf::header::sanity_check(&kernel).unwrap();

    let mmap = boot::memory_map(MemoryType::LOADER_DATA).unwrap();
    let mmap = unsafe { slice::from_raw_parts(mmap.buffer().as_ptr().cast(), mmap.len()) };
    let mut frame_alloc = unsafe { frame_alloc::BootFrameAllocator::new(mmap) };
    let mut phys_segment_addr = 0;
    for desc in mmap {
        if desc.ty == MemoryType::CONVENTIONAL && desc.page_count >= 128 && desc.phys_start != 0 {
            info!("kernel loading to: {desc:?}");
            phys_segment_addr = desc.phys_start;
        }
    }
    assert_ne!(phys_segment_addr, 0);

    unsafe {
        Cr0::update(|x| x.remove(Cr0Flags::WRITE_PROTECT));
        Efer::update(|x| x.insert(EferFlags::NO_EXECUTE_ENABLE));
    };

    for segment in kernel.program_iter() {
        xmas_elf::program::sanity_check(segment, &kernel).unwrap();
        if let xmas_elf::program::Type::Load = segment.get_type().unwrap() {
            let mem_size = segment.mem_size();
            let file_size = segment.file_size();
            let virt_addr = segment.virtual_addr();
            let file_offset = segment.offset() as usize;

            let flags = {
                if !segment.flags().is_execute() {
                    PageTableFlags::NO_EXECUTE
                } else {
                    PageTableFlags::empty()
                }
                .union(if segment.flags().is_write() {
                    PageTableFlags::WRITABLE
                } else {
                    PageTableFlags::empty()
                })
            };

            info!(
                "{}: {:x} bytes at {:x}",
                segment.flags(),
                mem_size,
                virt_addr
            );
            let page_range = Page::<Size4KiB>::containing_address(VirtAddr::new(virt_addr))
                ..=Page::containing_address(VirtAddr::new(virt_addr) + mem_size);
            info!("mapping {:?} starting at {phys_segment_addr:x}", page_range);
            for (page, frame) in page_range.zip(0..) {
                let frame = PhysFrame::<Size4KiB>::containing_address(
                    PhysAddr::new(phys_segment_addr) + frame * 4096,
                );
                unsafe {
                    mapper.map_to(
                        page,
                        frame,
                        flags | PageTableFlags::PRESENT,
                        &mut frame_alloc,
                    )
                }
                .unwrap()
                .flush();
            }

            let dst =
                unsafe { core::slice::from_raw_parts_mut(virt_addr as *mut u8, mem_size as usize) };
            let src = &buffer[file_offset..file_offset + file_size as usize];
            dst[..file_size as usize].copy_from_slice(src);

            // bss section so fill with zeros
            if mem_size > file_size {
                dst[file_size as usize..].fill(0);
            }

            // sanity check
            // debug_assert_eq!(
            //     buffer[file_offset..file_offset + file_size as usize][..32],
            //     (*unsafe {
            //         core::slice::from_raw_parts_mut(virt_addr as *mut u8, mem_size as usize)
            //     })[..32]
            // );

            phys_segment_addr += mem_size;
            phys_segment_addr = align_up(phys_segment_addr, 4096) + 4096;
        }
    }
    info!(
        "loaded kernel. entry point {:x} mapped to {:?}",
        kernel.header.pt2.entry_point(),
        mapper.translate_page(Page::<Size4KiB>::containing_address(VirtAddr::new(
            kernel.header.pt2.entry_point()
        )))
    );
    let max_phys_addr = max_phys_addr(mmap);
    info!("offset mapping address range 0-{:x}", max_phys_addr);
    let page_range = Page::<Size1GiB>::from_start_address(VirtAddr::new(MEM_OFFSET)).unwrap()
        ..Page::containing_address(VirtAddr::new(MEM_OFFSET) + max_phys_addr.as_u64());
    for page in page_range {
        unsafe {
            mapper.map_to(
                page,
                PhysFrame::containing_address(PhysAddr::new(
                    page.start_address() - VirtAddr::new(MEM_OFFSET),
                )),
                PageTableFlags::PRESENT | PageTableFlags::NO_EXECUTE | PageTableFlags::WRITABLE,
                &mut frame_alloc,
            )
        }
        .unwrap()
        .flush();
    }
    unsafe {
        Cr0::update(|x| x.insert(Cr0Flags::WRITE_PROTECT));
    }

    graphics.blt(uefi::proto::console::gop::BltOp::VideoFill {
        color: BltPixel::new(0, 0, 0),
        dest: (0, 0),
        dims: graphics_mode_info.resolution(),
    });
    let mmap = unsafe { boot::exit_boot_services(None) };
    let mmap = unsafe {
        slice::from_raw_parts(
            // move the address into higher half addressing
            (mmap.buffer().as_ptr().cast::<MemoryDescriptor>() as usize + MEM_OFFSET as usize)
                as *const _,
            mmap.len(),
        )
    };
    let bootinfo = BootInfo {
        mmap,
        graphics_mode_info,
        // move the address into higher half addressing
        graphics_output: (graphics.frame_buffer().as_mut_ptr() as usize + MEM_OFFSET as usize)
            as *mut _,
    };
    unsafe {
        mapper.map_to(
            Page::<Size4KiB>::containing_address(VirtAddr::new(BOOT_INFO_VIRT)),
            PhysFrame::from_start_address(PhysAddr::new(
                (&bootinfo as *const BootInfo).addr() as u64
            ))
            .unwrap(),
            PageTableFlags::NO_EXECUTE | PageTableFlags::PRESENT,
            &mut frame_alloc,
        )
    }
    .unwrap()
    .flush();

    let k_entry = kernel.header.pt2.entry_point() as usize;
    let k_entry_fn: unsafe extern "C" fn() -> ! = unsafe { core::mem::transmute(k_entry) };

    unsafe { k_entry_fn() };
}

pub unsafe fn init_offset_page_table(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();
    info!("{:x}", level_4_table_frame.start_address());
    // let page_table = unsafe { (*(level_4_table_frame.start_address().as_u64() as *const PageTable)).clone() };
    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();
    unsafe { OffsetPageTable::new(&mut *page_table_ptr, physical_memory_offset) }
}

#[alloc_error_handler]
fn alloc_error(_layout: Layout) -> ! {
    panic!("out of memory")
}
