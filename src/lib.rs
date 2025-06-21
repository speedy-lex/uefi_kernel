#![no_std]

use uefi::{boot::MemoryDescriptor, proto::console::gop::ModeInfo};

pub mod frame_alloc;

/// Must be 1GiB aligned
pub const MEM_OFFSET: u64 = 0xffff_8000_0000_0000;

pub const BOOT_INFO_VIRT: u64 = 0xffff_ffff_0000_0000;
pub const FRAME_TRACKER_VIRT: u64 = 0xffff_ffff_0000_1000;
pub const KERNEL_HEAP_VIRT: u64 = 0xffff_fffe_0000_0000;
/// Must be a multiple of 4 MiB
pub const KERNEL_HEAP_SIZE: u64 = 16 * 1024 * 1024;

pub const USER_SPACE_VIRT_END: u64 = 0x0000_7fff_ffff_ffff;

/// Must be honored by the kernel .elf
pub const KERNEL_VIRT: u64 = 0xffff_ffff_8000_0000;

#[derive(Debug, Clone, Copy)]
#[repr(C, align(4096))]
pub struct BootInfo {
    pub mmap: &'static [MemoryDescriptor],
    pub graphics_mode_info: ModeInfo,
    pub graphics_output: *mut u8,
}
