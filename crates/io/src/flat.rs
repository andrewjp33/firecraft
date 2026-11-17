//! Flat PNG decode/encode via the `png` crate.

use firecraft_raster::RgbaImage;

use crate::{Error, Result};

/// Largest image we will decode (pixels); guards hostile headers.
const MAX_PIXELS: u64 = 1 << 28;

pub fn decode_png(bytes: &[u8]) -> Result<RgbaImage> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(|e| Error::Png(e.to_string()))?;
    let (w, h) = (reader.info().width, reader.info().height);
    if w as u64 * h as u64 > MAX_PIXELS {
        return Err(Error::Unsupported(format!("{w} x {h} image is too large")));
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| Error::Png("image too large".into()))?;
    let mut buf = vec![0; size];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| Error::Png(e.to_string()))?;
    let data = buf.get(..info.buffer_size()).unwrap_or_default();
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .chunks_exact(3)
            .flat_map(|c| [c[0], c[1], c[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .chunks_exact(2)
            .flat_map(|c| [c[0], c[0], c[0], c[1]])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err(Error::Png("unexpanded palette".into())),
    };
    RgbaImage::from_rgba_bytes(w, h, &rgba).ok_or_else(|| Error::Png("pixel data size mismatch".into()))
}

/// A document's resolution: the value and whether it is in pixels/cm (else pixels/inch).
pub type Resolution = (f64, bool);

/// The `pHYs` pixels/metre Fireworks MX writes for a resolution, in saved and exported PNGs and
/// its clipboard data: the resolution truncated to whole pixels in its own unit, then
/// converted with integer arithmetic (measured 2026-11-16: 72 and 72.5 and 72.01 ppi -> 2834,
/// 71.99 -> 2795, 96 -> 3779, 150 -> 5905, 254 -> 10000, 2540 -> 100000; 28.35, 28.3465 and
/// 28.99 px/cm -> 2800, 37.8 -> 3700, 1 -> 100).
pub fn png_ppm((value, cm): Resolution) -> u32 {
    let whole = value.max(0.0).trunc() as u64;
    let ppm = if cm { whole * 100 } else { whole * 10_000 / 254 };
    u32::try_from(ppm).unwrap_or(u32::MAX)
}

/// The JFIF density (dots per inch) Fireworks MX writes in an exported JPEG: always per inch,
/// truncated (measured: 72.5 ppi -> 72, 254 -> 254; 28.99 px/cm -> 73, 28.35 px/cm -> 72).
pub fn jfif_dpi((value, cm): Resolution) -> u16 {
    let ppi = if cm { value * 2.54 } else { value };
    ppi.trunc().clamp(1.0, 65535.0) as u16
}

/// A flat 32-bit PNG at `ppi` pixels/inch.
pub fn encode_png(img: &RgbaImage, ppi: f64) -> Result<Vec<u8>> {
    encode_png_at(img, (ppi, false))
}

/// A flat 32-bit PNG whose `pHYs` says `res` as Fireworks MX writes it.
pub fn encode_png_at(img: &RgbaImage, res: Resolution) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.width().max(1), img.height().max(1));
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let ppm = png_ppm(res);
        enc.set_pixel_dims(Some(png::PixelDimensions {
            xppu: ppm,
            yppu: ppm,
            unit: png::Unit::Meter,
        }));
        let mut w = enc.write_header().map_err(|e| Error::Png(e.to_string()))?;
        let data = if img.width() == 0 || img.height() == 0 {
            vec![0; 4]
        } else {
            img.to_rgba_bytes()
        };
        w.write_image_data(&data).map_err(|e| Error::Png(e.to_string()))?;
    }
    Ok(out)
}

#[cfg(test)]
mod resolution_tests {
    use super::*;

    #[test]
    fn png_ppm_matches_fireworks_mx() {
        // Measured 2026-11-16: documents made in Fireworks MX at these resolutions, saved.
        for (ppi, ppm) in [
            (72.0, 2834),
            (72.5, 2834),
            (72.01, 2834),
            (71.99, 2795),
            (96.0, 3779),
            (150.0, 5905),
            (254.0, 10_000),
            (127.0, 5000),
            (300.0, 11_811),
            (508.0, 20_000),
            (600.0, 23_622),
            (1.0, 39),
            (2540.0, 100_000),
        ] {
            assert_eq!(png_ppm((ppi, false)), ppm, "{ppi} ppi");
        }
        for (pcm, ppm) in [(28.35, 2800), (28.3465, 2800), (28.99, 2800), (37.8, 3700), (100.0, 10_000), (72.0, 7200), (1.0, 100)] {
            assert_eq!(png_ppm((pcm, true)), ppm, "{pcm} px/cm");
        }
        assert_eq!(png_ppm((0.0, true)), 0);
    }

    #[test]
    fn jfif_dpi_matches_fireworks_mx() {
        // Measured 2026-11-16: JPEG exports from Fireworks MX.
        assert_eq!(jfif_dpi((72.5, false)), 72);
        assert_eq!(jfif_dpi((254.0, false)), 254);
        assert_eq!(jfif_dpi((150.0, false)), 150);
        assert_eq!(jfif_dpi((28.99, true)), 73);
        assert_eq!(jfif_dpi((28.35, true)), 72);
    }
}
