#![no_std]
#![no_main]
#![feature(iter_array_chunks)]
#![feature(maybe_uninit_array_assume_init)]

extern crate alloc;

use core::{arch::naked_asm, mem::MaybeUninit, panic::PanicInfo};

use alloc::{boxed::Box, format};
use arrayvec::ArrayString;

use embedded_graphics::{
    Drawable,
    mono_font::{MonoTextStyleBuilder, ascii::FONT_10X20},
    pixelcolor::Rgb888,
    prelude::{Dimensions, Point, RgbColor},
    primitives::{PrimitiveStyle, Rectangle, StyledDrawable},
    text::{Baseline, LineHeight, Text, TextStyleBuilder},
};

use uefi_kernel::{
    BOOT_INFO_VIRT, BootInfo, FRAME_TRACKER_VIRT, KERNEL_HEAP_SIZE, KERNEL_HEAP_VIRT, MEM_OFFSET,
    frame_alloc::{FrameTrackerArray, FrameUsageType, UsedFrame, init_offset_page_table},
};
use x86_64::{
    VirtAddr,
    instructions::tlb,
    structures::paging::{Mapper, OffsetPageTable, Page, PageTableFlags, Size2MiB, Size4KiB},
};

use linked_list_allocator::LockedHeap;

use crate::{frame_alloc::KernelFrameAllocator, framebuffer::FrameBuffer};

mod frame_alloc;
mod framebuffer;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

const BOOT_STACK_LEN: usize = 512 * 1024;

#[unsafe(link_section = ".stack")]
static mut BOOT_STACK: [u8; BOOT_STACK_LEN] = [0; BOOT_STACK_LEN];

#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start(frame_tracker_len: usize) -> ! {
    naked_asm!(
        "lea rsp, [{stack} + {stack_size}]",
        "mov rdi, rcx", // swap from microsoft calling conv to system V because x86_64-unknown-uefi "is-like-windows"
        "jmp {main}",
        stack = sym BOOT_STACK,
        stack_size = const BOOT_STACK_LEN,
        main = sym main,
    );
}

extern "C" fn main(frame_tracker_len: usize) -> ! {
    let boot_info = unsafe { *(BOOT_INFO_VIRT as *const BootInfo) };
    let mut framebuffer =
        unsafe { FrameBuffer::new(boot_info.graphics_mode_info, boot_info.graphics_output) };
    let mut row = 0;
    row += draw_text(&mut framebuffer, "Kernel Booted", row);
    let mmap = boot_info.mmap;

    let mut frame_alloc = KernelFrameAllocator::new(
        unsafe {
            FrameTrackerArray::new_existing(
                FRAME_TRACKER_VIRT as *mut UsedFrame,
                0x1000,
                frame_tracker_len,
            )
        },
        mmap,
    );

    let mut mapper = unsafe { init_offset_page_table(VirtAddr::new(MEM_OFFSET)) };
    row += draw_text(&mut framebuffer, "Cleaning up BootInfo mapping", row);
    // remove the boot info mapping
    mapper.unmap(Page::<Size4KiB>::containing_address(VirtAddr::new(
        BOOT_INFO_VIRT,
    )));
    row += draw_text(
        &mut framebuffer,
        "Cleaning up old uefi identity mappings",
        row,
    );
    // remove uefi identity mapping uses directly deleting entries because we dont care to dealloc frames
    // since they were allocated by uefi fw
    // lower half of address space is p4 0..256, upper half (mapped) is 256..512
    let table = mapper.level_4_table_mut();
    for i in 0..256 {
        table[i].set_unused(); // clears the entry
    }
    tlb::flush_all(); // apply the changes

    row += draw_text(&mut framebuffer, "Initializing heap", row);
    init_heap(&mut frame_alloc, &mut mapper);

    let b = Box::new(42usize);
    row += draw_text(
        &mut framebuffer,
        &format!("Box: {b} at {:p}", b.as_ref() as *const _),
        row,
    );

    row += draw_text(&mut framebuffer, "Done", row);

    loop {}
}

fn draw_text(framebuffer: &mut FrameBuffer, text: &str, row: usize) -> usize {
    let character_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(Rgb888::WHITE)
        .background_color(Rgb888::BLACK)
        .build();

    let char_size = character_style.font.character_spacing + 10;
    let chars_per_line = (framebuffer.bounding_box().size.width / char_size) as usize;

    let mut rows_drawn = 0;
    let mut line = arrayvec::ArrayString::<256>::new();
    for x in text.chars() {
        line.push(x);
        if line.len() == chars_per_line {
            Text::with_text_style(
                &line,
                Point::new(0, ((row + rows_drawn) * 25) as i32),
                character_style,
                TextStyleBuilder::new()
                    .baseline(Baseline::Top)
                    .line_height(LineHeight::Pixels(20))
                    .build(),
            )
            .draw(framebuffer);
            line.clear();
            rows_drawn += 1;
        }
    }
    if !line.is_empty() {
        Text::with_text_style(
            &line,
            Point::new(0, ((row + rows_drawn) * 25) as i32),
            character_style,
            TextStyleBuilder::new()
                .baseline(Baseline::Top)
                .line_height(LineHeight::Pixels(20))
                .build(),
        )
        .draw(framebuffer);
        rows_drawn += 1;
    }
    rows_drawn
}
fn clear_row(framebuffer: &mut FrameBuffer, row: usize) {
    let mut size = framebuffer.bounding_box().size;
    size.height = 25;
    Rectangle::new(
        Point {
            x: 0,
            y: row as i32 * 25,
        },
        size,
    )
    .draw_styled(&PrimitiveStyle::with_fill(Rgb888::BLACK), framebuffer);
}

fn init_heap(frame_alloc: &mut KernelFrameAllocator, mapper: &mut OffsetPageTable) {
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

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
