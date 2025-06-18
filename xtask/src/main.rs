use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Build the app
    let mut cmd = Command::new("cargo");

    let mut dir = env::current_dir().unwrap();
    dir.push("uefi_app");

    cmd.arg("build")
        .arg("--target")
        .arg("x86_64-unknown-uefi")
        .arg("-Z")
        .arg("build-std=core")
        .arg("-Z")
        .arg("build-std-features=compiler-builtins-mem")
        .current_dir(dir);

    run_cmd(cmd);

    // Build the image
    let mut cmd = Command::new("cargo");

    cmd.arg("run")
        .arg("-p")
        .arg("imager")
        .arg("--")
        .arg("target/x86_64-unknown-uefi/debug/uefi_app.efi");

    run_cmd(cmd);

    // Check if OVMF is found
    let ovmf = PathBuf::from("uefi_app/ovmfx64/code.fd").exists()
        && PathBuf::from("uefi_app/ovmfx64/vars.fd").exists();
    if !ovmf {
        panic!(
            "missing 1 or more OVMF files (in uefi_app/ovmfx64/{{code.fd, vars.fd}}\nfetch them from: https://github.com/rust-osdev/ovmf-prebuilt/releases/"
        );
    }
    // Run qemu
    let mut cmd = Command::new("qemu-system-x86_64");

    cmd.arg("-drive")
        .arg("format=raw,file=target/x86_64-unknown-uefi/debug/uefi_app.gdt")
        .arg("-drive")
        .arg(r"if=pflash,format=raw,readonly=on,file=uefi_app/ovmfx64\code.fd")
        .arg("-drive")
        .arg(r"if=pflash,format=raw,file=uefi_app/ovmfx64/vars.fd");

    run_cmd(cmd);
}

fn run_cmd(mut cmd: Command) {
    println!("Running: {cmd:?}");

    let status = cmd.status().expect("Failed to run cargo build");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}
