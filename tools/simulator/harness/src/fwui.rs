//! Firmware UI helpers the simulator needs, copied from the firmware's
//! `src/ui/mod.rs` (bf-5rh, GPL-3.0): `TextBuf`, `clear_strip`, and the
//! screen constants. The list widget itself is in `list.rs`, a verbatim
//! copy of the firmware's `src/ui/list.rs`.

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};

pub mod list;

pub const SCREEN_W: i32 = crate::fb::WIDTH as i32;
pub const SCREEN_H: i32 = crate::fb::HEIGHT as i32;

pub fn clear_strip<D>(lcd: &mut D, y: i32, h: i32)
where
    D: DrawTarget<Color = Rgb565>,
{
    Rectangle::new(Point::new(0, y), Size::new(SCREEN_W as u32, h as u32))
        .into_styled(PrimitiveStyle::with_fill(Rgb565::BLACK))
        .draw(lcd)
        .ok();
}

#[derive(Clone, Copy, PartialEq)]
pub struct TextBuf<const N: usize> {
    buf: [u8; N],
    len: usize,
}

impl<const N: usize> TextBuf<N> {
    pub fn new() -> Self {
        TextBuf { buf: [0; N], len: 0 }
    }

    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.buf[..self.len]).unwrap_or("")
    }
}

impl<const N: usize> core::fmt::Write for TextBuf<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let bytes = s.as_bytes();
        let space = N - self.len;
        let n = bytes.len().min(space);
        self.buf[self.len..self.len + n].copy_from_slice(&bytes[..n]);
        self.len += n;
        Ok(())
    }
}
