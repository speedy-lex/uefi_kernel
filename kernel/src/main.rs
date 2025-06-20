#![no_std]
#![no_main]

use core::{arch::naked_asm, panic::PanicInfo};

use arrayvec::ArrayString;

use embedded_graphics::{
    Drawable,
    mono_font::{MonoTextStyleBuilder, ascii::FONT_10X20},
    pixelcolor::Rgb888,
    prelude::{Point, RgbColor},
    text::{Baseline, LineHeight, Text, TextStyleBuilder},
};

use uefi::boot::MemoryType;
use uefi_kernel::{BOOT_INFO_VIRT, BootInfo};

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
    let mmap = boot_info.mmap;

    let mut row = 0;
    for desc in mmap.iter() {
        if desc.ty == MemoryType::CONVENTIONAL {
            let mut str = ArrayString::<256>::new();
            write!(str, "{desc:?}");
            draw_text(&mut framebuffer, &str, row);
            row += 1;
        }
    }
    loop {}
}

fn draw_text(framebuffer: &mut FrameBuffer, text: &str, row: usize) {
    let character_style = MonoTextStyleBuilder::new()
        .font(&FONT_10X20)
        .text_color(Rgb888::WHITE)
        .build();
    Text::with_text_style(
        text,
        Point::new(0, (row * 25) as i32),
        character_style,
        TextStyleBuilder::new()
            .baseline(Baseline::Top)
            .line_height(LineHeight::Pixels(20))
            .build(),
    )
    .draw(framebuffer);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
