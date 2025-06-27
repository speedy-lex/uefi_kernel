use core::fmt::Write;

use alloc::vec;
use alloc::{string::String, vec::Vec};
use embedded_graphics::{
    Drawable,
    mono_font::{MonoFont, MonoTextStyleBuilder, ascii::FONT_10X20},
    pixelcolor::Rgb888,
    prelude::{Point, RgbColor},
    text::{Baseline, LineHeight, Text, TextStyleBuilder},
};
use log::{LevelFilter, Log};
use spin::Once;
use spin::mutex::SpinMutex;

use crate::framebuffer::FrameBuffer;

static LOGGER: Once<Logger> = Once::new();

pub fn init(framebuffer: FrameBuffer) {
    let logger = Logger::new(framebuffer);
    let log_ref = LOGGER.call_once(|| logger);
    log::set_logger(log_ref).unwrap();
    log::set_max_level(LevelFilter::max());
}

pub struct Logger {
    inner: SpinMutex<LoggerInner>,
}
impl Logger {
    pub fn new(framebuffer: FrameBuffer) -> Self {
        Self {
            inner: SpinMutex::new(LoggerInner::new(framebuffer)),
        }
    }
}
impl Log for Logger {
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }
    fn log(&self, record: &log::Record) {
        let mut lock = self.inner.lock();
        writeln!(
            lock,
            "[{}, {}@{}]: {}",
            record.level(),
            record.file().unwrap_or_default(),
            record.line().unwrap_or_default(),
            record.args()
        )
        .unwrap();
        lock.update();
    }

    fn flush(&self) {}
}
struct LoggerInner {
    framebuffer: FrameBuffer,
    text: Vec<String>,
    dims: (usize, usize),
}
impl LoggerInner {
    const FONT: MonoFont<'static> = FONT_10X20;
    fn new(framebuffer: FrameBuffer) -> Self {
        let char_width = Self::FONT.character_size.width + Self::FONT.character_spacing;
        let char_height = Self::FONT.character_size.height;
        let res = framebuffer.resolution();
        let dims = (res.0 / char_width as usize, res.1 / char_height as usize);
        Self {
            framebuffer,
            text: vec![String::new(); dims.1],
            dims,
        }
    }
    fn update(&mut self) {
        self.framebuffer.clear_black();
        for (i, row) in self.text.iter().enumerate() {
            let character_style = MonoTextStyleBuilder::new()
                .font(&Self::FONT)
                .text_color(Rgb888::WHITE)
                .build();
            let text_style = TextStyleBuilder::new()
                .baseline(Baseline::Top)
                .line_height(LineHeight::Pixels(20))
                .build();
            Text::with_text_style(
                &row,
                Point::new(0, i as i32 * 20),
                character_style,
                text_style,
            )
            .draw(&mut self.framebuffer)
            .unwrap();
        }
    }
}
impl Write for LoggerInner {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let mut out = vec![];
        let mut line = self.text.last().unwrap().clone();
        for ch in s.chars() {
            if ch == '\n' {
                out.push(line);
                line = String::new();
                continue;
            }
            line.push(ch);
            if line.len() >= self.dims.0 {
                out.push(line);
                line = String::new();
            }
        }
        out.push(line);
        self.text.rotate_left(out.len() - 1);
        self.text[self.dims.1 - out.len()..].clone_from_slice(&out);
        Ok(())
    }
}
