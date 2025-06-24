use core::{mem::MaybeUninit, num::NonZero};

use uefi::boot::{MemoryDescriptor, MemoryType};
use uefi_kernel::frame_alloc::{FrameTrackerArray, FrameUsageType, UsedFrame};
use x86_64::{
    PhysAddr,
    structures::paging::{FrameAllocator, PageSize, PhysFrame, Size4KiB},
};

pub struct KernelFrameAllocator {
    pub frame_tracker: FrameTrackerArray,
    pub mmap: &'static [MemoryDescriptor],
}
impl KernelFrameAllocator {
    pub fn new(frame_tracker: FrameTrackerArray, mmap: &'static [MemoryDescriptor]) -> Self {
        Self {
            frame_tracker,
            mmap,
        }
    }
    fn usable_frames<P: PageSize>(&self) -> impl Iterator<Item = PhysFrame<P>> {
        let usable_regions = self.mmap.iter().filter(|x| {
            matches!(
                x.ty,
                MemoryType::CONVENTIONAL
                    | MemoryType::BOOT_SERVICES_CODE
                    | MemoryType::BOOT_SERVICES_DATA
            )
        });
        let usable_ranges =
            usable_regions.map(|x| x.phys_start..(x.phys_start + x.page_count * 4096));
        let usable_frames = usable_ranges
            .map(move |mut x: core::ops::Range<u64>| {
                x.start = x86_64::align_up(x.start, P::SIZE);
                x
            })
            .flat_map(move |x| x.step_by(P::SIZE as usize))
            .filter(|x| *x != 0)
            .filter(|x| {
                self.frame_tracker.as_ref().iter().all(|frame| {
                    !(frame.frame..frame.frame + frame.count.get() as u64 * 4096)
                        .contains(&PhysAddr::new(*x))
                })
            });
        usable_frames.map(|x| PhysFrame::containing_address(PhysAddr::new(x)))
    }
    pub fn allocate_frame_ty<P: PageSize>(&mut self, ty: FrameUsageType) -> Option<PhysFrame<P>> {
        let frame = self.usable_frames().next();
        if let Some(frame) = frame {
            self.frame_tracker.push_used_frame(UsedFrame {
                frame: frame.start_address(),
                count: NonZero::new((P::SIZE / 4096) as u32).unwrap(),
                ty,
            });
        }
        frame
    }
    /// Returns how many frames it managed to allocate
    /// ty must not be `FrameUsageType::Unknown`
    pub fn allocate_frames_ty<P: PageSize>(
        &mut self,
        frames: &mut [MaybeUninit<PhysFrame<P>>],
        ty: FrameUsageType,
    ) -> usize {
        assert!(ty != FrameUsageType::Unknown, "ty must not be Unknown");

        let mut frames_allocated = 0;
        for (frame, out) in self.usable_frames().zip(frames.iter_mut()) {
            *out = MaybeUninit::new(frame);
            frames_allocated += 1;
        }

        let mut curr: Option<UsedFrame> = None;

        for frame in &frames[0..frames_allocated] {
            let frame = unsafe { frame.assume_init_read() };
            match curr {
                Some(ref mut current) => {
                    let next_expected_addr = current.frame + current.count.get() as u64 * P::SIZE;
                    if frame.start_address() == next_expected_addr {
                        current.count = current.count.checked_add((P::SIZE / 4096) as u32).unwrap();
                    } else {
                        // push current range and start new
                        self.frame_tracker.push_used_frame(*current);
                        curr = Some(UsedFrame {
                            frame: frame.start_address(),
                            count: NonZero::new((P::SIZE / 4096) as u32).unwrap(),
                            ty,
                        });
                    }
                }
                None => {
                    // first frame
                    curr = Some(UsedFrame {
                        frame: frame.start_address(),
                        count: NonZero::new((P::SIZE / 4096) as u32).unwrap(),
                        ty,
                    });
                }
            }
        }

        // Push the last range
        if let Some(current) = curr {
            self.frame_tracker.push_used_frame(current);
        }

        self.frame_tracker.merge_all();
        frames_allocated
    }
}

unsafe impl FrameAllocator<Size4KiB> for KernelFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let frame = self.usable_frames().next();
        if let Some(frame) = frame {
            self.frame_tracker.push_used_frame(UsedFrame {
                frame: frame.start_address(),
                count: NonZero::new(1).unwrap(),
                ty: FrameUsageType::PageTable,
            });
            self.frame_tracker.sort_in_place();
        }
        frame
    }
}
