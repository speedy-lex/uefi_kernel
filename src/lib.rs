#![no_std]

use uefi::{boot::MemoryDescriptor, proto::console::gop::ModeInfo};

pub mod frame_alloc;

pub const MEM_OFFSET: u64 = 0xffff_0000_0000_0000;
pub const BOOT_INFO_VIRT: u64 = 0xffff_ffff_0000_0000;

#[derive(Debug, Clone, Copy)]
#[repr(C, align(4096))]
pub struct BootInfo {
    pub mmap: &'static [MemoryDescriptor],
    pub graphics_mode_info: ModeInfo,
    pub graphics_output: *mut u8,
    // pub kernel_elf: &'a [u8],
}
