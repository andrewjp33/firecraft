//! Device-independent bitmaps: the Windows clipboard's picture formats (`CF_DIB`, `CF_DIBV5`)
//! and BMP files (the X11 target `image/bmp`, which is a DIB behind a 14-byte file header; Wine
//! offers a Windows program's `CF_DIB` this way).
//!
//! Screenshots (Print Screen) and most Windows programs put a picture on the clipboard only as a
//! DIB. Its fourth byte per pixel at 32 bits is officially unused and holds anything from 0 to
//! leftovers, so it counts as alpha only where the header names an alpha mask (a V4/V5 header
//! or `BI_ALPHABITFIELDS`), and an image whose alpha is 0 everywhere is taken as opaque (what
//! GDI leaves behind), never as fully transparent.

use firecraft_raster::RgbaImage;

use crate::{Error, Result};

/// Largest picture decoded (pixels); guards hostile headers.
const MAX_PIXELS: u64 = 1 << 28;

const BI_RGB: u32 = 0;
const BI_BITFIELDS: u32 = 3;
const BI_ALPHABITFIELDS: u32 = 6;

fn bad(what: &str) -> Error {
    Error::Decode(format!("bitmap: {what}"))
}

fn u16_at(b: &[u8], at: usize) -> Result<u16> {
    b.get(at..at + 2)
        .and_then(|s| s.try_into().ok())
        .map(u16::from_le_bytes)
        .ok_or_else(|| bad("truncated header"))
}

fn u32_at(b: &[u8], at: usize) -> Result<u32> {
    b.get(at..at + 4)
        .and_then(|s| s.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| bad("truncated header"))
}

/// A BMP file (`BM` + file header + DIB).
pub fn decode_bmp(bytes: &[u8]) -> Result<RgbaImage> {
    if !bytes.starts_with(b"BM") {
        return Err(bad("not a BMP file"));
    }
    let off = u32_at(bytes, 10)? as usize;
    let dib = bytes.get(14..).ok_or_else(|| bad("truncated file header"))?;
    decode(dib, off.checked_sub(14))
}

/// A packed DIB (`CF_DIB` / `CF_DIBV5`: header, masks, palette, pixels).
pub fn decode_dib(bytes: &[u8]) -> Result<RgbaImage> {
    decode(bytes, None)
}

/// A channel given by a bit mask, scaled to 8 bits.
#[derive(Clone, Copy)]
struct Channel {
    mask: u32,
    shift: u32,
    max: u32,
}

impl Channel {
    fn new(mask: u32) -> Channel {
        let shift = if mask == 0 { 0 } else { mask.trailing_zeros() };
        Channel {
            mask,
            shift,
            max: mask.checked_shr(shift).unwrap_or(0),
        }
    }

    fn get(self, v: u32) -> u8 {
        if self.max == 0 {
            return 0;
        }
        let x = (v & self.mask) >> self.shift;
        ((u64::from(x) * 255 + u64::from(self.max) / 2) / u64::from(self.max)) as u8
    }
}

/// `pixels_at`: where the pixel rows start within `b`, when a file header says so.
fn decode(b: &[u8], pixels_at: Option<usize>) -> Result<RgbaImage> {
    let header = u32_at(b, 0)? as usize;
    let (w, h, bpp, compression, colors_used) = match header {
        12 => (
            i64::from(u16_at(b, 4)?),
            i64::from(u16_at(b, 6)?),
            u16_at(b, 10)?,
            BI_RGB,
            0,
        ),
        40.. => (
            i64::from(u32_at(b, 4)? as i32),
            i64::from(u32_at(b, 8)? as i32),
            u16_at(b, 14)?,
            u32_at(b, 16)?,
            u32_at(b, 32)? as usize,
        ),
        _ => return Err(bad("unknown header")),
    };
    let top_down = h < 0;
    let (w, h) = (w, h.abs());
    if w <= 0 || h == 0 || (w as u64) * (h as u64) > MAX_PIXELS {
        return Err(bad("bad size"));
    }
    let (w, h) = (w as usize, h as usize);
    if !matches!(compression, BI_RGB | BI_BITFIELDS | BI_ALPHABITFIELDS) {
        return Err(bad("compressed bitmaps aren't supported"));
    }
    if !matches!(bpp, 1 | 2 | 4 | 8 | 16 | 24 | 32) {
        return Err(bad("unsupported bit depth"));
    }
    let bitfields = compression != BI_RGB;

    // Masks: inside a V2+ header, or right after a plain 40-byte one.
    let mut after_header = header;
    let (mut r, mut g, mut bl, mut a) = match bpp {
        16 => (0x7c00, 0x03e0, 0x001f, 0),
        _ => (0x00ff_0000, 0x0000_ff00, 0x0000_00ff, 0),
    };
    if header >= 52 {
        if bitfields {
            (r, g, bl) = (u32_at(b, 40)?, u32_at(b, 44)?, u32_at(b, 48)?);
        }
        if header >= 56 && (bitfields || bpp == 32) {
            a = u32_at(b, 52)?;
        }
    } else if header == 40 && bitfields {
        (r, g, bl) = (u32_at(b, 40)?, u32_at(b, 44)?, u32_at(b, 48)?);
        after_header += 12;
        if compression == BI_ALPHABITFIELDS {
            a = u32_at(b, 52)?;
            after_header += 4;
        }
    }

    let palette: Vec<[u8; 4]> = if bpp <= 8 {
        let n = if colors_used == 0 || colors_used > 1 << bpp {
            1usize << bpp
        } else {
            colors_used
        };
        let size = if header == 12 { 3 } else { 4 };
        let entries = b
            .get(after_header..after_header + n * size)
            .ok_or_else(|| bad("truncated palette"))?;
        after_header += n * size;
        entries
            .chunks_exact(size)
            .map(|c| match c {
                [b, g, r, ..] => [*r, *g, *b, 255],
                _ => [0, 0, 0, 255],
            })
            .collect()
    } else {
        Vec::new()
    };

    let stride = (w * bpp as usize).div_ceil(32) * 4;
    let image_size = stride.checked_mul(h).ok_or_else(|| bad("bad size"))?;
    let mut at = pixels_at.unwrap_or(after_header);
    if pixels_at.is_none() && header > 40 && bitfields && b.len() == at + 12 + image_size {
        // Some programs repeat the three masks after a V4/V5 header.
        at += 12;
    }
    if b.len() < at + image_size && b.len() >= image_size && pixels_at.is_none() {
        // The pixels end the DIB; trust that over a miscounted palette.
        at = b.len() - image_size;
    }
    let data = b.get(at..at + image_size).ok_or_else(|| bad("truncated pixels"))?;

    let (cr, cg, cb, ca) = (Channel::new(r), Channel::new(g), Channel::new(bl), Channel::new(a));
    let le = |s: &[u8], at: usize, n: usize| -> u32 {
        s.get(at..at + n)
            .map_or(0, |b| b.iter().rev().fold(0, |v, &x| (v << 8) | u32::from(x)))
    };
    let mut out = vec![0u8; w * h * 4];
    // Rows top first: a positive height stores the bottom row first.
    let rows: Box<dyn Iterator<Item = &[u8]>> = if top_down {
        Box::new(data.chunks_exact(stride))
    } else {
        Box::new(data.chunks_exact(stride).rev())
    };
    for (src, dst) in rows.zip(out.chunks_exact_mut(w * 4)) {
        for (x, px) in dst.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let rgba: [u8; 4] = match bpp {
                16 | 32 => {
                    let v = le(src, x * usize::from(bpp / 8), usize::from(bpp / 8));
                    let alpha = if a == 0 { 255 } else { ca.get(v) };
                    [cr.get(v), cg.get(v), cb.get(v), alpha]
                }
                24 => {
                    let v = le(src, x * 3, 3);
                    [(v >> 16) as u8, (v >> 8) as u8, v as u8, 255]
                }
                _ => {
                    let bits = usize::from(bpp);
                    let bit = x * bits;
                    let byte = src.get(bit / 8).copied().unwrap_or(0);
                    let idx = (byte >> (8 - bits - bit % 8)) & (((1u16 << bits) - 1) as u8);
                    palette.get(usize::from(idx)).copied().unwrap_or([0, 0, 0, 255])
                }
            };
            *px = rgba;
        }
    }
    if a != 0 && out.iter().skip(3).step_by(4).all(|&v| v == 0) {
        out.iter_mut().skip(3).step_by(4).for_each(|v| *v = 255);
    }
    RgbaImage::from_rgba_bytes(w as u32, h as u32, &out).ok_or_else(|| bad("bad size"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use firecraft_raster::Rgba;

    fn info(header: u32, w: i32, h: i32, bpp: u16, compression: u32) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend(header.to_le_bytes());
        v.extend(w.to_le_bytes());
        v.extend(h.to_le_bytes());
        v.extend(1u16.to_le_bytes());
        v.extend(bpp.to_le_bytes());
        v.extend(compression.to_le_bytes());
        v.extend([0u8; 20]);
        v
    }

    #[test]
    fn bottom_up_24_bit_rows_are_padded() {
        // 3 x 2: rows of 9 bytes padded to 12; the first row stored is the bottom one.
        let mut d = info(40, 3, 2, 24, BI_RGB);
        d.extend([1, 2, 3, 4, 5, 6, 7, 8, 9, 0, 0, 0]);
        d.extend([10, 20, 30, 40, 50, 60, 70, 80, 90, 0, 0, 0]);
        let img = decode_dib(&d).unwrap();
        assert_eq!(img.get(0, 0).unwrap(), Rgba::new(30, 20, 10, 255));
        assert_eq!(img.get(2, 1).unwrap(), Rgba::new(9, 8, 7, 255));
    }

    #[test]
    fn screenshot_32_bit_with_zero_padding_byte_is_opaque() {
        let mut d = info(40, 2, -1, 32, BI_RGB);
        d.extend([10, 20, 30, 0, 40, 50, 60, 0]);
        let img = decode_dib(&d).unwrap();
        assert_eq!(img.get(0, 0).unwrap(), Rgba::new(30, 20, 10, 255));
        assert_eq!(img.get(1, 0).unwrap(), Rgba::new(60, 50, 40, 255));
        // Without an alpha mask the fourth byte is ignored even when it varies.
        let mut d = info(40, 2, -1, 32, BI_RGB);
        d.extend([10, 20, 30, 7, 40, 50, 60, 0]);
        assert!(decode_dib(&d).unwrap().pixels().iter().all(|p| p.a == 255));
    }

    #[test]
    fn v5_alpha_mask_gives_alpha() {
        let mut d = info(124, 2, -1, 32, BI_BITFIELDS);
        d.extend(0x00ff_0000u32.to_le_bytes());
        d.extend(0x0000_ff00u32.to_le_bytes());
        d.extend(0x0000_00ffu32.to_le_bytes());
        d.extend(0xff00_0000u32.to_le_bytes());
        d.resize(124, 0);
        d.extend([10, 20, 30, 128, 40, 50, 60, 0]);
        let img = decode_dib(&d).unwrap();
        assert_eq!(img.get(0, 0).unwrap(), Rgba::new(30, 20, 10, 128));
        assert_eq!(img.get(1, 0).unwrap().a, 0);
        // All-zero alpha: opaque.
        let n = d.len();
        d[n - 5] = 0;
        assert!(decode_dib(&d).unwrap().pixels().iter().all(|p| p.a == 255));
    }

    #[test]
    fn bitfields_after_a_plain_header_and_16_bit() {
        // 565 masks after a 40-byte header.
        let mut d = info(40, 1, 1, 16, BI_BITFIELDS);
        d.extend(0xf800u32.to_le_bytes());
        d.extend(0x07e0u32.to_le_bytes());
        d.extend(0x001fu32.to_le_bytes());
        d.extend([0xff, 0xff, 0, 0]);
        assert_eq!(decode_dib(&d).unwrap().get(0, 0).unwrap(), Rgba::new(255, 255, 255, 255));
        // Default 555.
        let mut d = info(40, 1, 1, 16, BI_RGB);
        d.extend([0x00, 0x7c, 0, 0]);
        assert_eq!(decode_dib(&d).unwrap().get(0, 0).unwrap(), Rgba::new(255, 0, 0, 255));
    }

    #[test]
    fn palette_and_bmp_file_header() {
        // 4 bpp, two palette entries, a 3 x 1 image: indices 1, 0, 1.
        let mut dib = info(40, 3, 1, 4, BI_RGB);
        dib[32..36].copy_from_slice(&2u32.to_le_bytes());
        dib.extend([0, 0, 255, 0, 255, 0, 0, 0]);
        dib.extend([0x10, 0x10, 0, 0]);
        let img = decode_dib(&dib).unwrap();
        assert_eq!(img.get(0, 0).unwrap(), Rgba::new(0, 0, 255, 255));
        assert_eq!(img.get(1, 0).unwrap(), Rgba::new(255, 0, 0, 255));
        assert_eq!(img.get(2, 0).unwrap(), Rgba::new(0, 0, 255, 255));
        let mut file = b"BM".to_vec();
        file.extend(((14 + dib.len()) as u32).to_le_bytes());
        file.extend([0; 4]);
        file.extend((14u32 + 48).to_le_bytes());
        file.extend(&dib);
        assert_eq!(decode_bmp(&file).unwrap().get(1, 0).unwrap(), Rgba::new(255, 0, 0, 255));
    }

    #[test]
    fn hostile_input_is_an_error() {
        assert!(decode_dib(&[]).is_err());
        assert!(decode_dib(&info(40, 100_000, 100_000, 32, BI_RGB)).is_err());
        assert!(decode_dib(&info(40, 2, 2, 32, 1)).is_err());
        assert!(decode_dib(&info(40, 0, 2, 32, BI_RGB)).is_err());
    }
}
