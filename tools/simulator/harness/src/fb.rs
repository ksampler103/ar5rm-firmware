//! 160x128 RGB565 framebuffer standing in for the ST7735 panel, plus a
//! minimal PNG writer (flate2 supplies the zlib stream).
//!
//! The framebuffer stores the raw u16 RGB565 values exactly as the apps
//! pass them to the firmware, so a frame here is pixel-identical to what
//! the firmware's draw calls would put on the panel.

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::OriginDimensions;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::primitives::Rectangle;
use embedded_graphics::prelude::*;
use embedded_graphics::Pixel;

pub const WIDTH: usize = 160;
pub const HEIGHT: usize = 128;

pub struct Framebuffer {
    pub px: [u16; WIDTH * HEIGHT],
}

impl Framebuffer {
    pub fn new() -> Self {
        Framebuffer {
            px: [0; WIDTH * HEIGHT],
        }
    }

    pub fn clear(&mut self, rgb565: u16) {
        self.px.fill(rgb565);
    }

    pub fn set(&mut self, x: i32, y: i32, rgb565: u16) {
        if x >= 0 && y >= 0 && (x as usize) < WIDTH && (y as usize) < HEIGHT {
            self.px[y as usize * WIDTH + x as usize] = rgb565;
        }
    }

    /// Write this frame as `<name>.png` (native 160x128) and
    /// `<name>@4x.png` (nearest-neighbour upscale).
    pub fn save_pngs(&self, base: &std::path::Path) {
        for (suffix, scale) in [("", 1usize), ("@4x", 4usize)] {
            let mut path = base.to_path_buf();
            let stem = path.file_name().unwrap().to_string_lossy().into_owned();
            path.set_file_name(format!("{stem}{suffix}.png"));
            let data = encode_png(self, scale);
            if let Err(e) = std::fs::write(&path, &data) {
                eprintln!("[sim] failed to write {}: {e}", path.display());
            } else {
                println!("[sim] frame -> {}", path.display());
            }
        }
    }
}

impl OriginDimensions for Framebuffer {
    fn size(&self) -> Size {
        Size::new(WIDTH as u32, HEIGHT as u32)
    }
}

impl DrawTarget for Framebuffer {
    type Color = Rgb565;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels.into_iter() {
            self.set(point.x, point.y, color.into_storage());
        }
        Ok(())
    }

    fn fill_contiguous<I>(&mut self, area: &Rectangle, colors: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Self::Color>,
    {
        self.draw_iter(
            Rectangle::new(area.top_left, area.size)
                .points()
                .zip(colors)
                .map(|(pos, color)| Pixel(pos, color)),
        )
    }
}

fn rgb565_to_rgb888(v: u16) -> [u8; 3] {
    let r5 = (v >> 11) & 0x1F;
    let g6 = (v >> 5) & 0x3F;
    let b5 = v & 0x1F;
    [
        ((r5 << 3) | (r5 >> 2)) as u8,
        ((g6 << 2) | (g6 >> 4)) as u8,
        ((b5 << 3) | (b5 >> 2)) as u8,
    ]
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc_data = kind.to_vec();
    crc_data.extend_from_slice(data);
    out.extend_from_slice(&crc32(&crc_data).to_be_bytes());
}

fn encode_png(fb: &Framebuffer, scale: usize) -> Vec<u8> {
    let w = WIDTH * scale;
    let h = HEIGHT * scale;
    // Raw image data: filter byte 0 per scanline + RGB bytes.
    let mut raw = Vec::with_capacity(h * (1 + w * 3));
    for y in 0..HEIGHT {
        for _ in 0..scale {
            raw.push(0);
            for x in 0..WIDTH {
                let [r, g, b] = rgb565_to_rgb888(fb.px[y * WIDTH + x]);
                for _ in 0..scale {
                    raw.extend_from_slice(&[r, g, b]);
                }
            }
        }
    }
    // zlib stream via flate2 (miniz_oxide backend).
    use std::io::Write as _;
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&raw).expect("zlib write to memory");
    let z = enc.finish().expect("zlib finish");

    let mut png = Vec::new();
    png.extend_from_slice(&[137, 80, 78, 71, 13, 10, 26, 10]);
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8-bit, truecolor
    chunk(&mut png, b"IHDR", &ihdr);
    chunk(&mut png, b"IDAT", &z);
    chunk(&mut png, b"IEND", &[]);
    png
}
