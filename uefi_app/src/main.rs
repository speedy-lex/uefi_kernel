#![no_std]
#![no_main]

use uefi::prelude::*;

use core::fmt::Write;

use core::panic::PanicInfo;

#[entry]
fn efi_main() -> Status {
    uefi::helpers::init().unwrap();
    system::with_stdout(|x| {
        x.clear().unwrap();
        writeln!(x, "Hello World!");
    });
    boot::stall(3_000_000);
    system::with_stdout(|x| x.clear().unwrap());
    Status::SUCCESS
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}
