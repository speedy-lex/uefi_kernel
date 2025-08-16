use core::{num::NonZero, slice};

use uefi::boot::{MemoryDescriptor, MemoryType};
use x86_64::{
    PhysAddr, VirtAddr,
    structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB},
};

pub struct BootFrameAllocator {
    pub frame_tracker: FrameTrackerArray,
    mmap: &'static [MemoryDescriptor],
    next_frame: usize,
}
impl BootFrameAllocator {
    pub fn usable_frames(mmap: &[MemoryDescriptor]) -> impl Iterator<Item = PhysFrame> {
        let usable_regions = mmap.iter().filter(|x| matches!(x.ty, MemoryType::CONVENTIONAL | MemoryType::BOOT_SERVICES_CODE | MemoryType::BOOT_SERVICES_DATA));
        let usable_ranges =
            usable_regions.map(|x| x.phys_start..(x.phys_start + x.page_count * 4096));
        let usable_frames = usable_ranges
            .flat_map(|x| x.step_by(4096))
            .filter(|x| *x != 0);
        usable_frames.map(|x| PhysFrame::containing_address(PhysAddr::new(x)))
    }

    /// # Safety
    /// Caller must guarrantee that mmap is valid, that all frames marked as `USABLE` are unused
    /// and that all memory is mapped with offset
    pub unsafe fn new(mmap: &'static [MemoryDescriptor], offset: VirtAddr) -> Self {
        // create frametracker
        let frame = Self::usable_frames(mmap).next().unwrap().start_address();
        let mut frame_tracker =
            unsafe { FrameTrackerArray::new((offset + frame.as_u64()).as_mut_ptr(), 4096) };
        frame_tracker.push_used_frame(UsedFrame {
            frame,
            count: NonZero::new(1).unwrap(),
            ty: FrameUsageType::FrameUsageBuffer,
        });

        Self {
            mmap,
            frame_tracker,
            next_frame: 1,
        }
    }
}
unsafe impl FrameAllocator<Size4KiB> for BootFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let frame = Self::usable_frames(self.mmap).nth(self.next_frame);
        if let Some(frame) = frame {
            self.frame_tracker.push_used_frame(UsedFrame {
                frame: frame.start_address(),
                count: NonZero::new(1).unwrap(),
                ty: FrameUsageType::Unknown,
            });
        }
        self.next_frame += 1;
        frame
    }
}
impl BootFrameAllocator {
    pub fn allocate_frame_ty(&mut self, ty: FrameUsageType) -> Option<PhysFrame<Size4KiB>> {
        let frame = Self::usable_frames(self.mmap).nth(self.next_frame);
        if let Some(frame) = frame {
            self.frame_tracker.push_used_frame(UsedFrame {
                frame: frame.start_address(),
                count: NonZero::new(1).unwrap(),
                ty,
            });
        }
        self.next_frame += 1;
        frame
    }
}

pub fn max_phys_addr(memmap: &[MemoryDescriptor]) -> u64 {
    memmap
        .iter()
        .filter(|x| x.ty.0 < 0x10) // skip weird custom stuff that is in the terabytes
        .map(|x| x.phys_start + x.page_count * 4096)
        .max()
        .unwrap()
        // Always cover at least the first 4 GiB of physical memory. That area
        // contains useful MMIO regions (local APIC, I/O APIC, PCI bars) that
        // we want to make accessible to the kernel even if no DRAM exists >4GiB.
        .max(0x1_0000_0000)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub enum FrameUsageType {
    KernelCode,
    KernelHeap,
    PageTable,
    FrameUsageBuffer,
    Reusable,
    Unknown,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct UsedFrame {
    pub frame: PhysAddr,
    pub count: NonZero<u32>, // Should be u64 but no one is mapping 17TiB continious of 1 type
    pub ty: FrameUsageType,
}
impl UsedFrame {
    pub fn can_merge(&self, other: &UsedFrame) -> bool {
        self.ty != FrameUsageType::Unknown
            && self.ty == other.ty
            && ((self.frame + self.count.get() as u64 * 4096) == other.frame
                || self.frame == (other.frame + other.count.get() as u64 * 4096))
    }
    /// # Safety
    /// Caller is required to check if the operation is valid by calling `can_merge`
    pub unsafe fn merge(&mut self, other: UsedFrame) {
        self.frame = if self.frame < other.frame {
            self.frame
        } else {
            other.frame
        };
        self.count = self.count.checked_add(other.count.get()).unwrap();
    }
}

pub struct FrameTrackerArray {
    used_frames: *mut UsedFrame,
    len: usize,
    cap: usize,
}
impl FrameTrackerArray {
    /// # Safety
    /// Caller must ensure there are `bytes` bytes of free space at `storage`
    pub unsafe fn new(storage: *mut UsedFrame, bytes: usize) -> Self {
        let cap = bytes / size_of::<UsedFrame>();
        Self {
            used_frames: storage,
            len: 0,
            cap,
        }
    }

    /// # Safety
    /// Caller must ensure there are `bytes` bytes of allocated space at `storage`
    /// containing `len` `UsedFrame`s
    pub unsafe fn new_existing(storage: *mut UsedFrame, bytes: usize, len: usize) -> Self {
        let cap = bytes / size_of::<UsedFrame>();
        Self {
            used_frames: storage,
            len,
            cap,
        }
    }

    pub fn push_used_frame(&mut self, used_frame: UsedFrame) {
        if self.len == self.cap {
            panic!("FrameTrackerArray buffer full");
        }

        unsafe { self.used_frames.add(self.len).write(used_frame) };
        self.len += 1;
    }

    pub fn sort_in_place(&mut self) {
        let len = self.len;
        let frames = self.as_mut();

        // Iterate from right to left (len-2 down to 0)
        for i in (0..len - 1).rev() {
            let key = frames[i];
            let mut j = i + 1;

            // Move elements on the right that are smaller than key one position to the left
            while j < len && frames[j].frame < key.frame {
                frames[j - 1] = frames[j];
                j += 1;
            }
            frames[j - 1] = key;
        }
    }

    pub fn merge_all(&mut self) {
        let len = self.len;

        // Sort by start address to make adjacent merging possible
        self.sort_in_place();

        let frames = self.as_mut();

        let mut write_idx = 0;
        let mut i = 0;

        while i < len {
            let mut current = frames[i];
            i += 1;

            // Merge all mergeable frames into `current`
            while i < len && current.can_merge(&frames[i]) {
                unsafe {
                    current.merge(frames[i]);
                }
                i += 1;
            }

            // Write back the merged result
            frames[write_idx] = current;
            write_idx += 1;
        }

        // Update len to reflect the new compacted array
        self.len = write_idx;
    }
    pub fn buffer(&self) -> *mut UsedFrame {
        self.used_frames
    }
}
impl AsMut<[UsedFrame]> for FrameTrackerArray {
    fn as_mut(&mut self) -> &mut [UsedFrame] {
        unsafe { slice::from_raw_parts_mut(self.used_frames, self.len) }
    }
}
impl AsRef<[UsedFrame]> for FrameTrackerArray {
    fn as_ref(&self) -> &[UsedFrame] {
        unsafe { slice::from_raw_parts(self.used_frames, self.len) }
    }
}

/// # Safety
/// Requires that Cr3 holds a valid page table and that memory is completely mapped with the physical_memory_offset provided
pub unsafe fn init_offset_page_table(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();
    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();
    unsafe { OffsetPageTable::new(&mut *page_table_ptr, physical_memory_offset) }
}
