#![no_std]
#![no_main]

use core::{arch::naked_asm, panic::PanicInfo};

use arrayvec::ArrayString;

use embedded_graphics::{
    Drawable,
    mono_font::{MonoTextStyleBuilder, ascii::FONT_10X20},
    pixelcolor::Rgb888,
    prelude::{Dimensions, Point, RgbColor},
    text::{Baseline, LineHeight, Text, TextStyleBuilder},
};

use uefi::boot::MemoryType;
use uefi_kernel::{
    BOOT_INFO_VIRT, BootInfo, MEM_OFFSET, USER_SPACE_VIRT_END, frame_alloc::init_offset_page_table,
};
use x86_64::{
    VirtAddr,
    structures::paging::{FrameDeallocator, Page, PageSize, mapper::CleanUp},
};

use core::fmt::Write;

use crate::framebuffer::FrameBuffer;

mod framebuffer;

const BOOT_STACK_LEN: usize = 64 * 1024;

#[unsafe(link_section = ".stack")]
static mut BOOT_STACK: [u8; BOOT_STACK_LEN] = [0; BOOT_STACK_LEN];

#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start() -> ! {
    naked_asm!(
        "lea rsp, [{stack} + {stack_size}]",
        // "mov rdi, rcx", // swap from microsoft calling conv to system V because x86_64-unknown-uefi "is-like-windows"
        "jmp {main}",
        stack = sym BOOT_STACK,
        stack_size = const BOOT_STACK_LEN,
        main = sym main,
    );
}

extern "C" fn main() -> ! {
    let boot_info = unsafe { *(BOOT_INFO_VIRT as *const BootInfo) };
    let mut framebuffer =
        unsafe { FrameBuffer::new(boot_info.graphics_mode_info, boot_info.graphics_output) };
    let mut row = 0;
    row += draw_text(&mut framebuffer, "Kernel Booted", row);
    let mmap = boot_info.mmap;

    row += draw_text(&mut framebuffer, "Cleaning up old uefi identity mappings", row);
    let mut mapper = unsafe { init_offset_page_table(VirtAddr::new(MEM_OFFSET)) };
    unsafe {
        mapper.clean_up_addr_range(
            x86_64::structures::paging::page::PageRangeInclusive {
                start: Page::containing_address(VirtAddr::zero()),
                end: Page::containing_address(VirtAddr::new(USER_SPACE_VIRT_END)),
            },
            &mut DummyAlloc,
        )
    };

    for desc in mmap.iter() {
        if desc.ty == MemoryType::CONVENTIONAL {
            let mut str = ArrayString::<256>::new();
            write!(str, "{desc:?}");
            row += draw_text(&mut framebuffer, &str, row);
        }
    }
    loop {}
}

// TODO: memory tracking
struct DummyAlloc;
impl<S: PageSize> FrameDeallocator<S> for DummyAlloc {
    unsafe fn deallocate_frame(&mut self, _frame: x86_64::structures::paging::PhysFrame<S>) {}
}

fn draw_text(framebuffer: &mut FrameBuffer, text: &str, row: usize) -> usize {
    let character_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(Rgb888::WHITE)
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

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
