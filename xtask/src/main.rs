use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Build the bootloader
    let mut cmd = Command::new("cargo");

    let mut dir = env::current_dir().unwrap();
    dir.push("bootloader");

    cmd.arg("build")
        .arg("--target")
        .arg("x86_64-unknown-uefi")
        .arg("-Z")
        .arg("build-std=core,alloc")
        .arg("-Z")
        .arg("build-std-features=compiler-builtins-mem")
        .current_dir(dir);

    run_cmd(cmd);

    // Build the kernel
    let mut cmd = Command::new("cargo");

    let mut dir = env::current_dir().unwrap();
    dir.push("kernel");

    cmd.arg("build")
        .arg("--target")
        .arg("kernel_target.json")
        .arg("-Z")
        .arg("build-std=core,alloc")
        .arg("-Z")
        .arg("build-std-features=compiler-builtins-mem")
        .current_dir(dir)
        .env(
            "RUSTFLAGS",
            "-C code-model=large -C link-arg=-Tkernel/linker.ld -C relocation-model=static",
        );

    run_cmd(cmd);

    // Build the image
    let mut cmd = Command::new("cargo");

    cmd.arg("run")
        .arg("-p")
        .arg("imager")
        .arg("--")
        .arg("target/x86_64-unknown-uefi/debug/bootloader.efi")
        .arg("target/kernel_target/debug/kernel.elf");

    run_cmd(cmd);

    // Check if OVMF is found
    let ovmf =
        PathBuf::from("ovmfx64/code.fd").exists() && PathBuf::from("ovmfx64/vars.fd").exists();
    if !ovmf {
        panic!(
            "missing 1 or more OVMF files (in ovmfx64/{{code.fd, vars.fd}}\nfetch them from: https://github.com/rust-osdev/ovmf-prebuilt/releases/"
        );
    }
    // Run qemu
    let mut cmd = Command::new("qemu-system-x86_64");

    cmd.arg("-drive")
        .arg("format=raw,file=target/x86_64-unknown-uefi/debug/bootloader.gdt")
        .arg("-drive")
        .arg("if=pflash,format=raw,readonly=on,file=ovmfx64/code.fd")
        .arg("-drive")
        .arg("if=pflash,format=raw,file=ovmfx64/vars.fd");
    // .arg("-s").arg("-S");
    // .arg("-d").arg("int").arg("-M").arg("smm=off").arg("-D").arg("out.log"); // debug exceptions

    run_cmd(cmd);
}

fn run_cmd(mut cmd: Command) {
    println!("Running: {cmd:?}");

    let status = cmd
        .status()
        .unwrap_or_else(|_| panic!("Failed to run {cmd:?}"));

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}
