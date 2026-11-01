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

pub fn encode_png(img: &RgbaImage, ppi: f64) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.width().max(1), img.height().max(1));
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let ppm = (ppi.max(1.0) / 0.0254).round() as u32;
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
