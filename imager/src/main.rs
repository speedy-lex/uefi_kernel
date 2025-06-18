use std::{
    env,
    fs::{self, File},
    io::{self, Seek},
    path::{Path, PathBuf},
};

fn main() {
    let mut args = env::args().skip(1);
    let efi_path = PathBuf::from(args.next().expect("Missing .efi file path"));

    let fat_path = efi_path.with_extension("fat");
    let disk_path = fat_path.with_extension("gdt");

    create_fs(&fat_path, &efi_path);
    create_disk(&disk_path, &fat_path);
}

fn create_fs(path: &Path, efi: &Path) {
    let efi_size = fs::metadata(efi).unwrap().len();

    let mb = 2u64.pow(20);
    let efi_rounded = (((efi_size - 1) / mb) + 1) * mb;

    let image = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .unwrap();
    image.set_len(efi_rounded).unwrap();

    let options = fatfs::FormatVolumeOptions::new();
    fatfs::format_volume(&image, options).unwrap();
    let fs = fatfs::FileSystem::new(&image, fatfs::FsOptions::new()).unwrap();

    let root = fs.root_dir();
    root.create_dir("efi").unwrap();
    root.create_dir("efi/boot").unwrap();
    let mut bootefi = root.create_file("efi/boot/bootx64.efi").unwrap();
    bootefi.truncate().unwrap();
    std::io::copy(&mut fs::File::open(efi).unwrap(), &mut bootefi).unwrap();
}

fn create_disk(path: &Path, fs: &Path) {
    let mut disk = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(path)
        .unwrap();

    let partition_size = fs::metadata(fs).unwrap().len();
    let disk_size = partition_size + 64 * 1024; // 64KB for gpt headers
    disk.set_len(disk_size).unwrap();

    // create a protective MBR at LBA0 so that disk is not considered
    // unformatted on BIOS systems
    let mbr = gpt::mbr::ProtectiveMBR::with_lb_size(
        u32::try_from((disk_size / 512) - 1).unwrap_or(0xFF_FF_FF_FF),
    );
    mbr.overwrite_lba0(&mut disk).unwrap();

    let block_size = gpt::disk::LogicalBlockSize::Lb512;
    let mut gpt = gpt::GptConfig::new()
        .writable(true)
        .logical_block_size(block_size)
        .create_from_device(&mut disk, None)
        .unwrap();
    gpt.update_partitions(Default::default()).unwrap();

    let partition_id = gpt
        .add_partition("boot", partition_size, gpt::partition_types::EFI, 0, None)
        .unwrap();
    let partition = gpt.partitions().get(&partition_id).unwrap();
    let offset = partition.bytes_start(block_size).unwrap();

    gpt.write().unwrap();

    disk.seek(io::SeekFrom::Start(offset)).unwrap();
    io::copy(&mut File::open(fs).unwrap(), &mut disk).unwrap();
}
