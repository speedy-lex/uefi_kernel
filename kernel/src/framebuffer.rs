use core::convert::Infallible;

use uefi::proto::console::gop::ModeInfo;

use embedded_graphics::{
    draw_target::DrawTarget,
    pixelcolor::Rgb888,
    prelude::{Dimensions, Point, RgbColor, Size},
    primitives::Rectangle,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}
impl From<Rgb888> for Color {
    fn from(value: Rgb888) -> Self {
        Self {
            r: value.r(),
            g: value.g(),
            b: value.b(),
        }
    }
}

pub struct FrameBuffer {
    bytes: &'static mut [u8],
    mode_info: ModeInfo,
    row: usize,
}
impl FrameBuffer {
    pub unsafe fn new(mode_info: ModeInfo, ptr: *mut u8) -> Self {
        let len = mode_info.resolution().1 * mode_info.stride() * 4;
        Self {
            bytes: unsafe { core::slice::from_raw_parts_mut(ptr, len) },
            mode_info,
            row: 0,
        }
    }
    pub fn set_pixel(&mut self, x: usize, y: usize, color: Color) {
        let index = y * self.mode_info.stride() + x;
        let byte = index * 4;
        match self.mode_info.pixel_format() {
            uefi::proto::console::gop::PixelFormat::Rgb => {
                self.bytes[byte..byte + 4].copy_from_slice(&[color.r, color.g, color.b, 0])
            }
            uefi::proto::console::gop::PixelFormat::Bgr => {
                self.bytes[byte..byte + 4].copy_from_slice(&[color.b, color.g, color.r, 0])
            }
            _ => panic!("unsupported pixel format"),
        }
    }
}

impl Dimensions for FrameBuffer {
    fn bounding_box(&self) -> embedded_graphics::primitives::Rectangle {
        let resolution = self.mode_info.resolution();
        Rectangle::new(
            Point::zero(),
            Size::new(resolution.0 as u32, resolution.1 as u32),
        )
    }
}

impl DrawTarget for FrameBuffer {
    type Color = Rgb888;

    type Error = Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = embedded_graphics::Pixel<Self::Color>>,
    {
        for pixel in pixels {
            if self.bounding_box().contains(pixel.0) {
                self.set_pixel(pixel.0.x as usize, pixel.0.y as usize, pixel.1.into());
            }
        }
        Ok(())
    }
}
