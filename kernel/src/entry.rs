macro_rules! entry_point {
    ($fn:expr) => {
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
            use ::uefi_kernel::{BOOT_INFO_VIRT, FRAME_TRACKER_VIRT};
            use ::uefi_kernel::frame_alloc::UsedFrame;

            let boot_info = unsafe { *(BOOT_INFO_VIRT as *const BootInfo) };
            let frame_tracker = unsafe { FrameTrackerArray::new_existing(
                FRAME_TRACKER_VIRT as *mut UsedFrame,
                0x1000,
                frame_tracker_len,
            ) };
            let framebuffer =
                unsafe { FrameBuffer::new(boot_info.graphics_mode_info, boot_info.graphics_output) };
            $fn(boot_info, frame_tracker, framebuffer)
        }
    };
}
