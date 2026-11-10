//! File > Export: the document flattened and encoded with its export settings (the Optimize
//! panel): GIF, animated GIF, JPEG, PNG 8/24/32, WebP and JPEG XL.

pub(crate) mod expv;

use std::borrow::Cow;

use firecraft_doc::{Document, ExportFormat, ExportSettings, PaletteKind, Transparency};
use firecraft_raster::quantize::{self, Entry};
use firecraft_raster::{Rgba, RgbaImage};

use crate::{Error, Result};

/// An encoded export.
#[derive(Clone, Debug)]
pub struct Exported {
    pub bytes: Vec<u8>,
    /// How the file looks (its first frame), decoded back.
    pub preview: RgbaImage,
    /// Colours in the palette of an indexed export.
    pub palette: Vec<Entry>,
}

/// The colour semi-transparent pixels are flattened onto when the format can't keep them.
pub fn matte(doc: &Document, s: &ExportSettings) -> Rgba {
    s.matte
        .or(doc.canvas.filter(|c| c.a == 255))
        .unwrap_or(Rgba::new(255, 255, 255, 255))
}

/// Whether this build can write `format` (WebP needs libwebp, which the browser build lacks).
pub fn available(format: ExportFormat) -> bool {
    format != ExportFormat::WebP || cfg!(not(target_arch = "wasm32"))
}

/// Export the document's current frame (every exported frame for an animated GIF).
pub fn export(doc: &Document, s: &ExportSettings) -> Result<Exported> {
    if !available(s.format) {
        return Err(Error::Unsupported(format!("{} export isn't available here", s.format.label())));
    }
    let frame = doc.current_frame.min(doc.frames.len().saturating_sub(1));
    let img = doc.render(frame);
    if img.width() == 0 || img.height() == 0 {
        return Err(Error::Unsupported("the document is empty".into()));
    }
    let m = matte(doc, s);
    match s.format {
        ExportFormat::Gif => gif(&[(img, 0)], s, m),
        ExportFormat::AnimatedGif => {
            let frames: Vec<(RgbaImage, u32)> = doc
                .frames
                .iter()
                .enumerate()
                .filter(|(_, f)| f.export)
                .map(|(i, f)| (doc.render(i), f.delay))
                .collect();
            if frames.is_empty() {
                return Err(Error::Unsupported("no frame is set to export".into()));
            }
            gif(&frames, s, m)
        }
        ExportFormat::Png8 => png8(&img, s, m, doc.resolution),
        ExportFormat::Png24 => {
            let flat = flatten(&img, m);
            let bytes = png_rgb(&flat, doc.resolution, false)?;
            Ok(Exported {
                bytes,
                preview: flat,
                palette: Vec::new(),
            })
        }
        ExportFormat::Png32 => Ok(Exported {
            bytes: png_rgb(&img, doc.resolution, true)?,
            preview: img,
            palette: Vec::new(),
        }),
        ExportFormat::Jpeg => jpeg(&img, s, m, doc.resolution),
        ExportFormat::WebP => webp(&img, s),
        ExportFormat::JpegXl => jxl(&img, s),
    }
}

/// The image composited onto `matte` (fully opaque).
fn flatten(img: &RgbaImage, matte: Rgba) -> RgbaImage {
    let mut out = RgbaImage::new(img.width(), img.height(), Rgba { a: 255, ..matte });
    out.draw(img, 0, 0, 255);
    out
}

fn entries(img: &RgbaImage) -> Vec<Entry> {
    img.pixels().iter().map(|p| [p.r, p.g, p.b, p.a]).collect()
}

/// Pixels ready for a palette: opaque ones as they are, semi-transparent ones flattened onto
/// the matte; with `cut`, pixels under half opacity become fully transparent.
fn prepare(img: &RgbaImage, matte: Rgba, cut: bool) -> Vec<Entry> {
    img.pixels()
        .iter()
        .map(|&p| {
            if cut && p.a < 128 {
                [0, 0, 0, 0]
            } else if p.a == 255 {
                [p.r, p.g, p.b, 255]
            } else {
                let c = Rgba { a: 255, ..matte }.over(p, 255);
                [c.r, c.g, c.b, 255]
            }
        })
        .collect()
}

/// The palette of an indexed export for these (prepared) pixels, at most `n` colours.
fn palette_for(pixels: &[Entry], s: &ExportSettings, n: usize, alpha: bool) -> Vec<Entry> {
    let opaque: Vec<Entry> = pixels.iter().copied().filter(|p| alpha || p[3] != 0).collect();
    let hist = quantize::histogram(&opaque);
    match s.palette {
        PaletteKind::Adaptive | PaletteKind::Exact => quantize::adaptive(&hist, n, alpha),
        PaletteKind::WebAdaptive => {
            let mut p = quantize::adaptive(&hist, n, alpha);
            quantize::web_snap(&mut p, s.websnap_tolerance);
            p.dedup();
            p
        }
        PaletteKind::Web216 => quantize::most_used(&quantize::web216(), &hist, n),
        PaletteKind::Uniform => quantize::uniform(n),
        PaletteKind::Windows => quantize::most_used(&quantize::windows(), &hist, n),
        PaletteKind::Macintosh => quantize::most_used(&quantize::macintosh(), &hist, n),
        PaletteKind::Grayscale => quantize::grayscale(n),
        PaletteKind::BlackWhite => quantize::grayscale(2),
    }
}

/// Palette indices for several frames sharing one palette (and a transparent index last).
struct Indexed {
    palette: Vec<Entry>,
    frames: Vec<Vec<u8>>,
    transparent: Option<u8>,
}

fn index(images: &[&RgbaImage], s: &ExportSettings, matte: Rgba, png_alpha: bool) -> Indexed {
    let cut = s.transparency != Transparency::None && !png_alpha;
    let grey = matches!(s.palette, PaletteKind::Grayscale | PaletteKind::BlackWhite);
    let prepared: Vec<Vec<Entry>> = images
        .iter()
        .map(|img| if png_alpha { entries(img) } else { prepare(img, matte, cut) })
        .map(|px| if grey { px.into_iter().map(luma).collect() } else { px })
        .collect();
    let has_clear = cut && prepared.iter().flatten().any(|p| p[3] == 0);
    let limit = if s.palette == PaletteKind::BlackWhite {
        2
    } else {
        usize::from(s.colors.clamp(2, 256))
    };
    let n = if has_clear { limit.min(255) } else { limit };
    let all: Vec<Entry> = prepared.iter().flatten().copied().collect();
    let mut palette = palette_for(&all, s, n, png_alpha);
    if palette.is_empty() {
        palette.push([0, 0, 0, 255]);
    }
    let transparent = has_clear.then(|| {
        palette.push([0, 0, 0, 0]);
        (palette.len() - 1).min(255) as u8
    });
    let skip_index = transparent.unwrap_or(0);
    let colours = palette
        .get(..palette.len() - usize::from(transparent.is_some()))
        .unwrap_or(&[]);
    let frames = prepared
        .iter()
        .zip(images)
        .map(|(px, img)| {
            quantize::map(
                px,
                img.width() as usize,
                colours,
                s.dither,
                png_alpha,
                &|p: Entry| cut && p[3] == 0,
                skip_index,
            )
        })
        .collect();
    Indexed {
        palette,
        frames,
        transparent,
    }
}

/// A pixel as grey: Fireworks MX's grayscale palette maps colours by Rec. 709 luma, truncated
/// (97.3% of an RGB cube exactly as Fireworks MX exports it, the rest within one level).
fn luma(c: Entry) -> Entry {
    let v = (0.2126 * f64::from(c[0]) + 0.7152 * f64::from(c[1]) + 0.0722 * f64::from(c[2])).floor() as u8;
    [v, v, v, c[3]]
}

fn render_indexed(idx: &[u8], palette: &[Entry], w: u32, h: u32) -> RgbaImage {
    let px: Vec<Rgba> = idx
        .iter()
        .map(|&i| {
            let c = palette.get(i as usize).copied().unwrap_or([0, 0, 0, 0]);
            Rgba::new(c[0], c[1], c[2], c[3])
        })
        .collect();
    RgbaImage::from_pixels(w, h, px).unwrap_or_else(|| RgbaImage::new(w, h, Rgba::TRANSPARENT))
}

// --- GIF ---------------------------------------------------------------------------------------

fn gif(frames: &[(RgbaImage, u32)], s: &ExportSettings, matte: Rgba) -> Result<Exported> {
    let err = |e: gif::EncodingError| Error::Png(format!("GIF: {e}"));
    let Some(first) = frames.first() else {
        return Err(Error::Unsupported("no frame to export".into()));
    };
    let (w, h) = (first.0.width(), first.0.height());
    let (gw, gh) = (u16::try_from(w), u16::try_from(h));
    let (Ok(gw), Ok(gh)) = (gw, gh) else {
        return Err(Error::Unsupported("GIF images are at most 65535 pixels wide and high".into()));
    };
    let images: Vec<&RgbaImage> = frames.iter().map(|f| &f.0).collect();
    let ix = index(&images, s, matte, false);
    let mut rgb: Vec<u8> = ix.palette.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
    // GIF colour tables hold a power of two of entries.
    let size = ix.palette.len().next_power_of_two().max(2);
    rgb.resize(size * 3, 0);
    let mut out = Vec::new();
    {
        let mut enc = gif::Encoder::new(&mut out, gw, gh, &rgb).map_err(err)?;
        if frames.len() > 1 {
            let repeat = if s.loops == 0 {
                gif::Repeat::Infinite
            } else {
                gif::Repeat::Finite(s.loops)
            };
            enc.set_repeat(repeat).map_err(err)?;
        }
        // Later frames are cropped to what changed (Fireworks' Auto Crop), unless transparency
        // forces whole frames that clear the previous one.
        let crop = ix.transparent.is_none();
        let mut prev: Option<&Vec<u8>> = None;
        for (i, data) in ix.frames.iter().enumerate() {
            let (mut left, mut top, mut fw, mut fh) = (0u32, 0u32, w, h);
            if crop && let Some(p) = prev {
                match changed_rect(p, data, w, h) {
                    Some(r) => (left, top, fw, fh) = r,
                    None => (left, top, fw, fh) = (0, 0, 1, 1),
                }
            }
            let mut buf = Vec::with_capacity((fw * fh) as usize);
            for y in top..top + fh {
                let row = (y * w) as usize;
                buf.extend_from_slice(data.get(row + left as usize..row + (left + fw) as usize).unwrap_or(&[]));
            }
            let frame = gif::Frame {
                left: left as u16,
                top: top as u16,
                width: fw as u16,
                height: fh as u16,
                buffer: Cow::Owned(buf),
                transparent: ix.transparent,
                delay: frames.get(i).map_or(0, |f| f.1.min(65535) as u16),
                dispose: if crop {
                    gif::DisposalMethod::Keep
                } else {
                    gif::DisposalMethod::Background
                },
                interlaced: s.interlaced,
                ..gif::Frame::default()
            };
            enc.write_frame(&frame).map_err(err)?;
            prev = Some(data);
        }
    }
    let preview = render_indexed(ix.frames.first().map_or(&[][..], |f| f), &ix.palette, w, h);
    Ok(Exported {
        bytes: out,
        preview,
        palette: ix.palette,
    })
}

/// The smallest rectangle (left, top, width, height) holding every pixel that differs.
fn changed_rect(a: &[u8], b: &[u8], w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..h {
        for x in 0..w {
            let i = (y * w + x) as usize;
            if a.get(i) != b.get(i) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    (x0 <= x1).then(|| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

// --- PNG ---------------------------------------------------------------------------------------

fn png_header(enc: &mut png::Encoder<'_, &mut Vec<u8>>, ppi: f64) {
    let ppm = (ppi.max(1.0) / 0.0254).round() as u32;
    enc.set_pixel_dims(Some(png::PixelDimensions {
        xppu: ppm,
        yppu: ppm,
        unit: png::Unit::Meter,
    }));
}

fn png_rgb(img: &RgbaImage, ppi: f64, alpha: bool) -> Result<Vec<u8>> {
    let err = |e: png::EncodingError| Error::Png(e.to_string());
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.width(), img.height());
        enc.set_color(if alpha { png::ColorType::Rgba } else { png::ColorType::Rgb });
        enc.set_depth(png::BitDepth::Eight);
        png_header(&mut enc, ppi);
        let mut w = enc.write_header().map_err(err)?;
        let data: Vec<u8> = if alpha {
            img.to_rgba_bytes()
        } else {
            img.pixels().iter().flat_map(|p| [p.r, p.g, p.b]).collect()
        };
        w.write_image_data(&data).map_err(err)?;
    }
    Ok(out)
}

fn png8(img: &RgbaImage, s: &ExportSettings, matte: Rgba, ppi: f64) -> Result<Exported> {
    let err = |e: png::EncodingError| Error::Png(e.to_string());
    let alpha = s.transparency == Transparency::Alpha;
    let ix = index(&[img], s, matte, alpha);
    let (w, h) = (img.width(), img.height());
    let n = ix.palette.len();
    // The smallest bit depth that holds the palette.
    let (depth, bits) = match n {
        0..=2 => (png::BitDepth::One, 1),
        3..=4 => (png::BitDepth::Two, 2),
        5..=16 => (png::BitDepth::Four, 4),
        _ => (png::BitDepth::Eight, 8),
    };
    let data = ix.frames.first().map_or(&[][..], |f| f);
    let packed: Vec<u8> = if bits == 8 {
        data.to_vec()
    } else {
        let per = 8 / bits;
        data.chunks(w.max(1) as usize)
            .flat_map(|row| {
                row.chunks(per).map(|c| {
                    c.iter()
                        .enumerate()
                        .fold(0u8, |acc, (k, &v)| acc | (v << (8 - bits * (k + 1))))
                })
            })
            .collect()
    };
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Indexed);
        enc.set_depth(depth);
        enc.set_palette(ix.palette.iter().flat_map(|c| [c[0], c[1], c[2]]).collect::<Vec<u8>>());
        let trns: Vec<u8> = ix.palette.iter().map(|c| c[3]).collect();
        if trns.iter().any(|&a| a < 255) {
            // Only up to the last entry that isn't opaque.
            let last = trns.iter().rposition(|&a| a < 255).map_or(0, |i| i + 1);
            enc.set_trns(trns.get(..last).unwrap_or(&[]).to_vec());
        }
        png_header(&mut enc, ppi);
        let mut wr = enc.write_header().map_err(err)?;
        wr.write_image_data(&packed).map_err(err)?;
    }
    let preview = render_indexed(data, &ix.palette, w, h);
    Ok(Exported {
        bytes: out,
        preview,
        palette: ix.palette,
    })
}

// --- JPEG --------------------------------------------------------------------------------------

fn jpeg(img: &RgbaImage, s: &ExportSettings, matte: Rgba, ppi: f64) -> Result<Exported> {
    let mut flat = flatten(img, matte);
    if s.jpeg_smoothing > 0 {
        // Smoothing blurs before compressing (an approximation of Fireworks MX's levels 1-8).
        firecraft_raster::effects::Effect::GaussianBlur {
            radius: f32::from(s.jpeg_smoothing.min(8)) * 0.5,
        }
        .apply(&mut flat);
        flat = flatten(&flat, matte);
    }
    let rgb: Vec<u8> = flat.pixels().iter().flat_map(|p| [p.r, p.g, p.b]).collect();
    let (w, h) = (
        u16::try_from(flat.width()).map_err(|_| Error::Unsupported("JPEG images are at most 65535 pixels wide".into()))?,
        u16::try_from(flat.height()).map_err(|_| Error::Unsupported("JPEG images are at most 65535 pixels high".into()))?,
    );
    let mut out = Vec::new();
    let mut enc = jpeg_encoder::Encoder::new(&mut out, ijg_quality(s.jpeg_quality).clamp(1, 100));
    enc.set_progressive(s.progressive);
    enc.set_sampling_factor(if s.jpeg_full_color {
        jpeg_encoder::SamplingFactor::R_4_4_4
    } else {
        jpeg_encoder::SamplingFactor::R_4_2_0
    });
    let dpi = ppi.round().clamp(1.0, 65535.0) as u16;
    enc.set_density(jpeg_encoder::PixelDensity {
        density: (dpi, dpi),
        unit: jpeg_encoder::PixelDensityUnit::Inches,
    });
    enc.encode(&rgb, w, h, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| Error::Png(format!("JPEG: {e}")))?;
    let preview = decode_still(&out, image::ImageFormat::Jpeg).unwrap_or(flat);
    Ok(Exported {
        bytes: out,
        preview,
        palette: Vec::new(),
    })
}

fn decode_still(bytes: &[u8], format: image::ImageFormat) -> Option<RgbaImage> {
    let img = image::load_from_memory_with_format(bytes, format).ok()?.into_rgba8();
    RgbaImage::from_rgba_bytes(img.width(), img.height(), img.as_raw())
}

// --- WebP --------------------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
fn webp(img: &RgbaImage, s: &ExportSettings) -> Result<Exported> {
    let rgba = img.to_rgba_bytes();
    let mut config = webp::WebPConfig::new().map_err(|()| Error::Png("WebP: no encoder".into()))?;
    config.lossless = i32::from(s.lossless);
    config.quality = f32::from(s.quality.min(100));
    config.alpha_quality = i32::from(s.alpha_quality.min(100));
    // Lossy alpha below 100 (Alpha Quality); lossless alpha at 100.
    config.alpha_compression = i32::from(!s.lossless);
    config.method = 4;
    let mem = webp::Encoder::from_rgba(&rgba, img.width(), img.height())
        .encode_advanced(&config)
        .map_err(|e| Error::Png(format!("WebP: {e:?}")))?;
    let bytes = mem.to_vec();
    let preview = decode_still(&bytes, image::ImageFormat::WebP).unwrap_or_else(|| img.clone());
    Ok(Exported {
        bytes,
        preview,
        palette: Vec::new(),
    })
}

#[cfg(target_arch = "wasm32")]
fn webp(_img: &RgbaImage, _s: &ExportSettings) -> Result<Exported> {
    Err(Error::Unsupported("WebP export isn't available in the browser version yet".into()))
}

// --- JPEG XL -----------------------------------------------------------------------------------

fn jxl(img: &RgbaImage, s: &ExportSettings) -> Result<Exported> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let opaque = img.pixels().iter().all(|p| p.a == 255);
    let mut config = jixel::EncodeConfig::default().with_lossless(s.lossless);
    if !s.lossless {
        config = config
            .with_quality(f32::from(s.quality.min(100)))
            .with_alpha_distance(jixel::distance_from_quality(f32::from(s.alpha_quality.min(100))));
    }
    let err = |e: jixel::EncodeError| Error::Png(format!("JPEG XL: {e:?}"));
    let bytes = if opaque {
        let rgb: Vec<u8> = img.pixels().iter().flat_map(|p| [p.r, p.g, p.b]).collect();
        jixel::encode_image(&rgb, w, h, &config).map_err(err)?
    } else {
        jixel::encode_image_with_alpha(&img.to_rgba_bytes(), w, h, &config).map_err(err)?
    };
    let preview = crate::formats::open_flat(&bytes, None)
        .map(|d| d.render(0))
        .unwrap_or_else(|_| img.clone());
    Ok(Exported {
        bytes,
        preview,
        palette: Vec::new(),
    })
}

/// Fireworks MX's JPEG quality setting and the IJG (libjpeg) quality its tables equal, measured
/// on JPEGs Fireworks MX exported at each setting (estimated from their quantization tables).
const MX_JPEG_QUALITY: [(u8, u8); 12] = [
    (0, 8),
    (10, 9),
    (20, 11),
    (30, 17),
    (40, 40),
    (50, 51),
    (60, 64),
    (70, 74),
    (80, 86),
    (90, 94),
    (95, 96),
    (100, 100),
];

fn interpolate(table: &[(u8, u8)], q: u8) -> u8 {
    let q = f64::from(q.min(100));
    for w in table.windows(2) {
        let [(a0, b0), (a1, b1)] = [w.first().copied().unwrap_or_default(), w.get(1).copied().unwrap_or_default()];
        let (a0, b0, a1, b1) = (f64::from(a0), f64::from(b0), f64::from(a1), f64::from(b1));
        if q <= a1 {
            let t = if a1 > a0 { (q - a0) / (a1 - a0) } else { 0.0 };
            return (b0 + t * (b1 - b0)).round().clamp(1.0, 100.0) as u8;
        }
    }
    100
}

/// The libjpeg quality that encodes like Fireworks MX's quality `q`.
pub fn ijg_quality(q: u8) -> u8 {
    interpolate(&MX_JPEG_QUALITY, q)
}

/// Fireworks MX's quality setting for a JPEG whose tables equal libjpeg quality `ijg`.
pub fn mx_quality(ijg: u8) -> u8 {
    let inverse: Vec<(u8, u8)> = MX_JPEG_QUALITY.iter().map(|&(a, b)| (b, a)).collect();
    if ijg <= 8 {
        return 0;
    }
    interpolate(&inverse, ijg)
}

/// A file name for the export: the document's name with the format's extension. PNG exports
/// get "-export", so they don't replace the editable Fireworks PNG of the same name.
pub fn file_name(doc_title: &str, format: ExportFormat) -> String {
    let stem = doc_title.rsplit_once('.').map_or(doc_title, |(s, _)| s);
    match format.extension() {
        "png" => format!("{stem}-export.png"),
        ext => format!("{stem}.{ext}"),
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn doc() -> Document {
        let mut d = Document::new(40, 30, 72.0, None);
        let mut img = RgbaImage::new(40, 30, Rgba::TRANSPARENT);
        for y in 0..30 {
            for x in 0..40 {
                let a = if x < 10 { 0 } else { 255 };
                img.set(x, y, Rgba::new((x * 6) as u8, (y * 8) as u8, 120, a));
            }
        }
        if let Some(c) = d.layers[0].elements_mut(0) {
            c.push(firecraft_doc::Element::new(firecraft_doc::ElementKind::Bitmap(
                firecraft_doc::Bitmap { x: 0, y: 0, image: img },
            )));
        }
        d
    }

    fn with(format: ExportFormat) -> ExportSettings {
        ExportSettings {
            format,
            ..ExportSettings::default()
        }
    }

    #[test]
    fn jpeg_quality_follows_fireworks_scale() {
        assert_eq!(ijg_quality(80), 86);
        assert_eq!(ijg_quality(30), 17);
        assert_eq!(mx_quality(86), 80);
        // A libjpeg quality 92 file: Fireworks MX shows 86 (its own estimate); this gives 88.
        assert_eq!(mx_quality(92), 88);
        let d = doc();
        let s = ExportSettings {
            jpeg_quality: 80,
            ..with(ExportFormat::Jpeg)
        };
        let e = export(&d, &s).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(crate::formats::jpeg_quality(&e.bytes), Some(86));
    }

    #[test]
    fn every_format_encodes_and_reads_back() {
        let d = doc();
        for f in ExportFormat::ALL {
            if !available(f) {
                continue;
            }
            let e = export(&d, &with(f)).unwrap_or_else(|e| unreachable!("{f:?}: {e}"));
            assert!(!e.bytes.is_empty(), "{f:?}");
            assert_eq!((e.preview.width(), e.preview.height()), (40, 30), "{f:?}");
            let back = crate::formats::open_flat(&e.bytes, Some(file_name("x.png", f)))
                .unwrap_or_else(|e| unreachable!("{f:?} reads back: {e}"));
            assert_eq!((back.width, back.height), (40, 30), "{f:?}");
        }
    }

    #[test]
    fn gif_transparency_cuts_at_half_opacity() {
        let d = doc();
        let s = ExportSettings {
            transparency: Transparency::Index,
            ..with(ExportFormat::Gif)
        };
        let e = export(&d, &s).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(e.preview.get(2, 2).map(|p| p.a), Some(0));
        assert_eq!(e.preview.get(30, 2).map(|p| p.a), Some(255));
        // No transparency: flattened onto white.
        let e = export(&d, &with(ExportFormat::Gif)).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(e.preview.get(2, 2), Some(Rgba::new(255, 255, 255, 255)));
    }

    #[test]
    fn png8_keeps_few_colours_exactly_at_a_small_depth() {
        let mut d = Document::new(8, 8, 72.0, Some(Rgba::new(255, 0, 0, 255)));
        d.layers.retain(|l| l.web);
        let s = ExportSettings {
            palette: PaletteKind::Exact,
            ..with(ExportFormat::Png8)
        };
        let e = export(&d, &s).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(e.palette.len(), 1);
        assert_eq!(e.preview.get(3, 3), Some(Rgba::new(255, 0, 0, 255)));
        let dec = png::Decoder::new(Cursor::new(&e.bytes)).read_info().map(|r| r.info().bit_depth);
        assert!(matches!(dec, Ok(png::BitDepth::One)));
    }

    #[test]
    fn animated_gif_keeps_frames_and_delays() {
        let mut d = doc();
        d.frames.push(firecraft_doc::Frame {
            delay: 50,
            ..firecraft_doc::Frame::default()
        });
        let e = export(&d, &with(ExportFormat::AnimatedGif)).unwrap_or_else(|e| unreachable!("{e}"));
        let back = crate::formats::open_flat(&e.bytes, None).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(back.frames.len(), 2);
        assert_eq!(back.frames[1].delay, 50);
    }

    #[test]
    fn modern_formats_honour_quality() {
        let d = doc();
        for f in [ExportFormat::WebP, ExportFormat::JpegXl] {
            if !available(f) {
                continue;
            }
            let size = |q: u8, aq: u8| {
                let s = ExportSettings {
                    quality: q,
                    alpha_quality: aq,
                    ..with(f)
                };
                export(&d, &s).map(|e| e.bytes.len()).unwrap_or(0)
            };
            assert!(size(20, 100) < size(95, 100), "{f:?}: lower quality is smaller");
            let lossless = ExportSettings {
                lossless: true,
                ..with(f)
            };
            let e = export(&d, &lossless).unwrap_or_else(|e| unreachable!("{f:?}: {e}"));
            assert_eq!(e.preview.get(30, 5), d.render(0).get(30, 5), "{f:?} lossless");
        }
    }
}
