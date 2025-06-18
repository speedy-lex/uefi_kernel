use alloc::boxed::Box;
use uefi::proto::media::file::{Directory, FileInfo};

pub struct EnumerateDir {
    dir: Directory,
}
impl Iterator for EnumerateDir {
    type Item = Box<FileInfo>;

    fn next(&mut self) -> Option<Self::Item> {
        self.dir.read_entry_boxed().unwrap()
    }
}
impl From<Directory> for EnumerateDir {
    fn from(mut value: Directory) -> Self {
        value.reset_entry_readout().unwrap();
        Self { dir: value }
    }
}
