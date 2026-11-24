//! `crate::ui` shim for the core-UI simulator.
//!
//! Mirrors the fork's `src/ui/mod.rs` (commit 7d2de0d) reduced to
//! what the standby renderer names through `super::`: the theme,
//! icons and standby modules (copied verbatim from the fork by
//! gen_core.py into `src/ui/` beside this file), the screen
//! constants, and `TextBuf` with its shared non-generic cores —
//! copied verbatim below, including the fork's slimming refactor.
//! The firmware takes SCREEN_W/H from the ST7735 driver
//! (160x128); the constants here are the same values.
//!
//! Copied into each generated variant crate as `src/ui.rs` by
//! gen_core.py — edit it here, not in gencore/.

pub mod theme;
mod icons;
pub mod standby;

/// Host-side handle for the renderer's cache. The fork's
/// `standby::Cache` is `pub(super)` (crate-internal on the radio),
/// so the shim wraps it and re-exposes the draw entry point.
pub struct RenderCache(standby::Cache);

impl RenderCache {
    pub fn new() -> Self {
        RenderCache(standby::Cache::new())
    }
}

pub fn draw_standby<D>(lcd: &mut D, app: &crate::app::App, cache: &mut RenderCache)
where
    D: DrawTarget<Color = Rgb565>,
{
    standby::draw_standby(lcd, app, &mut cache.0)
}

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::{PrimitiveStyle, Rectangle};

pub const SCREEN_W: i32 = 160;
pub const SCREEN_H: i32 = 128;

#[allow(dead_code)]
pub(crate) fn clear_strip<D>(lcd: &mut D, y: i32, h: i32)
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
        TextBuf {
            buf: [0; N],
            len: 0,
        }
    }

    pub fn as_str(&self) -> &str {
        textbuf_as_str(&self.buf, self.len)
    }
}

/// Non-generic core of `TextBuf::as_str`, shared by every capacity so
/// the body exists exactly once in the binary (fork slimming — the
/// per-`N` monomorphizations cost ~200 B across the sizes in use).
#[inline(never)]
fn textbuf_as_str(buf: &[u8], len: usize) -> &str {
    core::str::from_utf8(&buf[..len]).unwrap_or("")
}

/// Non-generic core of `TextBuf::write_str` (same reasoning as above;
/// the per-`N` copies cost ~450 B).
#[inline(never)]
fn textbuf_push(buf: &mut [u8], len: &mut usize, s: &str) {
    let bytes = s.as_bytes();
    let space = buf.len() - *len;
    let n = bytes.len().min(space);
    buf[*len..*len + n].copy_from_slice(&bytes[..n]);
    *len += n;
}

impl<const N: usize> core::fmt::Write for TextBuf<N> {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        textbuf_push(&mut self.buf, &mut self.len, s);
        Ok(())
    }
}
