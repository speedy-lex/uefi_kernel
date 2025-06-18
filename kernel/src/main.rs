#![no_std]
#![no_main]

use core::{arch::naked_asm, panic::PanicInfo};

use uefi::{boot::MemoryDescriptor, proto::console::gop::ModeInfo};

const BOOT_STACK_LEN: usize = 64 * 1024;

#[unsafe(link_section = ".stack")]
static mut BOOT_STACK: [u8; BOOT_STACK_LEN] = [0; BOOT_STACK_LEN];

#[unsafe(no_mangle)]
#[unsafe(naked)]
pub unsafe extern "C" fn _start(mmap: *const MemoryDescriptor, len: usize, graphics_mode_info: *const ModeInfo, graphics_output: *mut u8) -> ! {
    naked_asm!(
        "lea rsp, [{stack} + {stack_size}]",
        "mov rdi, rcx", // swap from microsoft calling conv to system V because x86_64-unknown-uefi "is-like-windows"
        "mov rsi, rdx",
        "mov rdx, r8",
        "mov rcx, r9",
        "jmp {main}",
        stack = sym BOOT_STACK,
        stack_size = const BOOT_STACK_LEN,
        main = sym main,
    );
}

extern "C" fn main(mmap: *const MemoryDescriptor, len: usize, graphics_mode_info: *const ModeInfo, graphics_output: *mut u8) -> ! {
    unsafe {
        graphics_output.write_volatile(0);
        graphics_output.offset(1).write_volatile(0);
        graphics_output.offset(2).write_volatile(255);
        graphics_output.offset(3).write_volatile(0)
    };
    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
