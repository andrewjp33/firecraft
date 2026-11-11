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
    /// Colors in the palette of an indexed export.
    pub palette: Vec<Entry>,
}

/// The color semi-transparent pixels are flattened onto when the format can't keep them.
pub fn matte(doc: &Document, s: &ExportSettings) -> Rgba {
    s.matte
        .or(doc.canvas.filter(|c| c.a == 255))
        .unwrap_or(Rgba::new(255, 255, 255, 255))
}

/// Whether this build can write `format` (WebP needs libwebp, which the browser build lacks).
pub fn available(format: ExportFormat) -> bool {
    format != ExportFormat::WebP || cfg!(not(target_arch = "wasm32"))
}

/// The file File > Export writes: [`export`]'s bytes, then for PNG formats the lossless PNG
/// optimizer at the settings' level. (The Preview shows [`export`] alone, which is fast.)
pub fn export_file(doc: &Document, s: &ExportSettings) -> Result<Vec<u8>> {
    let e = export(doc, s)?;
    Ok(if s.format.png() {
        optimize_png(e.bytes, s.png_optimize)
    } else {
        e.bytes
    })
}

/// Whether this build has the PNG optimizer (it links libdeflate, which the browser build lacks).
pub fn png_optimizer_available() -> bool {
    cfg!(not(target_arch = "wasm32"))
}

/// Losslessly recompress a PNG (oxipng, an OptiPNG-style optimizer): level 1-6 are its presets
/// 0-5 (filters, bit depth, color type and palette reductions, deflate effort), 7 its strongest
/// preset with Zopfli deflate (much slower). 0 or an error keeps the file as it is.
#[cfg(not(target_arch = "wasm32"))]
pub fn optimize_png(bytes: Vec<u8>, level: u8) -> Vec<u8> {
    if level == 0 {
        return bytes;
    }
    let mut opts = oxipng::Options::from_preset(level.min(7).saturating_sub(1).min(6));
    if level >= 7 {
        opts.deflater = oxipng::Deflater::Zopfli(oxipng::ZopfliOptions::default());
    }
    match oxipng::optimize_from_memory(&bytes, &opts) {
        Ok(smaller) if smaller.len() < bytes.len() => smaller,
        _ => bytes,
    }
}

#[cfg(target_arch = "wasm32")]
pub fn optimize_png(bytes: Vec<u8>, _level: u8) -> Vec<u8> {
    bytes
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

/// Below this alpha a pixel is transparent in a GIF, or a PNG 8 with a fixed palette, under
/// Alpha Transparency (Fireworks MX: alpha 12 vanishes, 19 is drawn opaque).
const ALPHA_CUT: u8 = 16;

/// How an indexed export treats transparency.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Clear {
    /// Everything flattened onto the matte.
    None,
    /// Everything flattened onto the matte, whose palette entry becomes transparent (Fireworks
    /// MX's Index Transparency: white pixels of a white matte vanish too).
    Key,
    /// Pixels under [`ALPHA_CUT`] transparent, the rest flattened onto the matte.
    Cut,
    /// A palette with alpha (PNG 8, adaptive palettes); `exact`: alpha as it is, otherwise in
    /// Fireworks MX's 16 levels.
    Palette { exact: bool },
}

fn clear_mode(s: &ExportSettings, png8: bool) -> Clear {
    let alpha_palette = matches!(
        s.palette,
        PaletteKind::Adaptive | PaletteKind::WebAdaptive | PaletteKind::Exact
    );
    match s.transparency {
        Transparency::None => Clear::None,
        Transparency::Index => Clear::Key,
        Transparency::Alpha if png8 && alpha_palette => Clear::Palette {
            exact: s.exact_alpha,
        },
        Transparency::Alpha => Clear::Cut,
    }
}

/// Pixels ready for a palette.
fn prepare(img: &RgbaImage, matte: Rgba, mode: Clear) -> Vec<Entry> {
    img.pixels()
        .iter()
        .map(|&p| match mode {
            Clear::Palette { exact } => {
                // Fireworks MX rounds alpha to multiples of 17 (6 vanishes, 12 and 19 give 17).
                let a = if exact {
                    p.a
                } else {
                    ((u16::from(p.a) + 8) / 17 * 17).min(255) as u8
                };
                if a == 0 { [0, 0, 0, 0] } else { [p.r, p.g, p.b, a] }
            }
            Clear::Cut if p.a < ALPHA_CUT => [0, 0, 0, 0],
            _ if p.a == 255 => [p.r, p.g, p.b, 255],
            _ => {
                let c = Rgba { a: 255, ..matte }.over(p, 255);
                [c.r, c.g, c.b, 255]
            }
        })
        .collect()
}

/// The palette of an indexed export for these (prepared) pixels, at most `n` colors.
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

/// Palette indices for several frames sharing one palette.
struct Indexed {
    palette: Vec<Entry>,
    frames: Vec<Vec<u8>>,
    /// The transparent entry (Index or Alpha Transparency, or Auto Difference).
    transparent: Option<u8>,
    /// Some pixels are transparent (frames have to clear what was under them).
    has_clear: bool,
}

impl Indexed {
    /// The palette as the file shows it: the transparent entry with alpha 0.
    fn shown(&self) -> Vec<Entry> {
        let mut p = self.palette.clone();
        if let Some(c) = self.transparent.and_then(|t| p.get_mut(usize::from(t))) {
            c[3] = 0;
        }
        p
    }
}

/// `reserve`: keep a transparent entry even if no pixel needs one (an animation's Auto Difference).
fn index(images: &[&RgbaImage], s: &ExportSettings, matte: Rgba, png8: bool, reserve: bool) -> Indexed {
    let mode = clear_mode(s, png8);
    let alpha_palette = matches!(mode, Clear::Palette { .. });
    let grey = matches!(s.palette, PaletteKind::Grayscale | PaletteKind::BlackWhite);
    let prepared: Vec<Vec<Entry>> = images
        .iter()
        .map(|img| prepare(img, matte, mode))
        .map(|px| if grey { px.into_iter().map(luma).collect() } else { px })
        .collect();
    let cut = mode == Clear::Cut;
    let has_cut = cut && prepared.iter().flatten().any(|p| p[3] == 0);
    let limit = if s.palette == PaletteKind::BlackWhite {
        2
    } else {
        usize::from(s.colors.clamp(2, 256))
    };
    let all: Vec<Entry> = prepared.iter().flatten().copied().collect();
    if alpha_palette {
        // Transparency is in the palette itself.
        let mut palette = palette_for(&all, s, limit, true);
        if palette.is_empty() {
            palette.push([0, 0, 0, 0]);
        }
        let frames = map_frames(&prepared, images, &palette, s.dither, true, &|_| false, 0);
        let has_clear = palette.iter().any(|c| c[3] < 255);
        return Indexed {
            palette,
            frames,
            transparent: None,
            has_clear,
        };
    }
    let key = mode == Clear::Key;
    let extra = usize::from(has_cut || key || reserve);
    let mut palette = palette_for(&all, s, limit.saturating_sub(extra).max(1), false);
    if palette.is_empty() {
        palette.push([0, 0, 0, 255]);
    }
    if key {
        // The matte's entry is the transparent one; pixels nearest it vanish.
        let m = [matte.r, matte.g, matte.b, 255];
        let at = palette.iter().position(|&c| c == m).unwrap_or_else(|| {
            palette.push(m);
            palette.len() - 1
        });
        let frames = map_frames(&prepared, images, &palette, s.dither, false, &|_| false, 0);
        let t = at.min(255) as u8;
        let has_clear = frames.iter().flatten().any(|&i| i == t);
        return Indexed {
            palette,
            frames,
            transparent: Some(t),
            has_clear,
        };
    }
    let transparent = (has_cut || reserve).then(|| {
        palette.push([0, 0, 0, 0]);
        (palette.len() - 1).min(255) as u8
    });
    let colors = palette
        .get(..palette.len() - usize::from(transparent.is_some()))
        .unwrap_or(&[]);
    let frames = map_frames(
        &prepared,
        images,
        colors,
        s.dither,
        false,
        &|p: Entry| cut && p[3] == 0,
        transparent.unwrap_or(0),
    );
    Indexed {
        palette,
        frames,
        transparent,
        has_clear: has_cut,
    }
}

fn map_frames(
    prepared: &[Vec<Entry>],
    images: &[&RgbaImage],
    palette: &[Entry],
    dither: u8,
    alpha: bool,
    skip: &dyn Fn(Entry) -> bool,
    skip_index: u8,
) -> Vec<Vec<u8>> {
    prepared
        .iter()
        .zip(images)
        .map(|(px, img)| quantize::map(px, img.width() as usize, palette, dither, alpha, skip, skip_index))
        .collect()
}

/// A pixel as grey: Fireworks MX's grayscale palette maps colors by Rec. 709 luma, truncated
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
    let animated = frames.len() > 1;
    // Auto Difference marks unchanged pixels with the transparent entry: keep one free for it
    // (the index is decided before knowing whether the frames have transparency of their own).
    let reserve = animated && s.auto_difference && s.transparency == Transparency::None;
    let ix = index(&images, s, matte, false, reserve);
    let mut rgb: Vec<u8> = ix.palette.iter().flat_map(|c| [c[0], c[1], c[2]]).collect();
    // GIF color tables hold a power of two of entries.
    let size = ix.palette.len().next_power_of_two().max(2);
    rgb.resize(size * 3, 0);
    let mut out = Vec::new();
    {
        let mut enc = gif::Encoder::new(&mut out, gw, gh, &rgb).map_err(err)?;
        if animated {
            let repeat = if s.loops == 0 {
                gif::Repeat::Infinite
            } else {
                gif::Repeat::Finite(s.loops)
            };
            enc.set_repeat(repeat).map_err(err)?;
        }
        // Frames with transparent pixels must clear what the previous frame drew (disposal
        // "background"), so Auto Crop keeps each frame to its visible pixels and Auto Difference
        // can't apply. Opaque frames build on the previous one ("keep"): Auto Crop writes only
        // the area that changed, Auto Difference leaves unchanged pixels transparent.
        let clears = ix.has_clear;
        let mut prev: Option<&Vec<u8>> = None;
        for (i, data) in ix.frames.iter().enumerate() {
            let rect = if !animated || !s.auto_crop {
                None
            } else if clears {
                visible_rect(data, ix.transparent, w, h)
            } else {
                prev.map(|p| changed_rect(p, data, w, h).unwrap_or((0, 0, 1, 1)))
            };
            let (left, top, fw, fh) = rect.unwrap_or((0, 0, w, h));
            let difference = (!clears && s.auto_difference).then_some(ix.transparent).flatten();
            let mut buf = Vec::with_capacity((fw * fh) as usize);
            for y in top..top + fh {
                for x in left..left + fw {
                    let at = (y * w + x) as usize;
                    let v = data.get(at).copied().unwrap_or(0);
                    let same = prev.is_some_and(|p| p.get(at) == Some(&v));
                    buf.push(match difference {
                        Some(t) if same => t,
                        _ => v,
                    });
                }
            }
            let frame = gif::Frame {
                left: left as u16,
                top: top as u16,
                width: fw as u16,
                height: fh as u16,
                buffer: Cow::Owned(buf),
                transparent: ix.transparent,
                delay: frames.get(i).map_or(0, |f| f.1.min(65535) as u16),
                dispose: if clears {
                    gif::DisposalMethod::Background
                } else {
                    gif::DisposalMethod::Keep
                },
                interlaced: s.interlaced,
                ..gif::Frame::default()
            };
            enc.write_frame(&frame).map_err(err)?;
            prev = Some(data);
        }
    }
    let shown = ix.shown();
    let preview = render_indexed(ix.frames.first().map_or(&[][..], |f| f), &shown, w, h);
    Ok(Exported {
        bytes: out,
        preview,
        palette: ix.palette,
    })
}

/// The smallest rectangle (left, top, width, height) holding every pixel that differs.
fn changed_rect(a: &[u8], b: &[u8], w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    bounding(w, h, |i| a.get(i) != b.get(i))
}

/// The smallest rectangle holding every pixel that isn't transparent (at least 1x1).
fn visible_rect(data: &[u8], transparent: Option<u8>, w: u32, h: u32) -> Option<(u32, u32, u32, u32)> {
    Some(bounding(w, h, |i| data.get(i).copied() != transparent).unwrap_or((0, 0, 1, 1)))
}

fn bounding(w: u32, h: u32, hit: impl Fn(usize) -> bool) -> Option<(u32, u32, u32, u32)> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if hit((y * w + x) as usize) {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x0 <= x1 {
        Some((x0, y0, x1 - x0 + 1, y1 - y0 + 1))
    } else {
        None
    }
}

// --- PNG ---------------------------------------------------------------------------------------

fn png_header(enc: &mut png::Encoder<'_, &mut Vec<u8>>, ppi: f64) {
    // Exports are written once and downloaded many times: the smallest file is worth the time.
    enc.set_compression(png::Compression::High);
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
    let ix = index(&[img], s, matte, true, false);
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
        // Palette images compress best unfiltered (as libpng and Fireworks write them).
        png_header(&mut enc, ppi);
        enc.set_filter(png::Filter::NoFilter);
        enc.set_palette(ix.palette.iter().flat_map(|c| [c[0], c[1], c[2]]).collect::<Vec<u8>>());
        let trns: Vec<u8> = ix.shown().iter().map(|c| c[3]).collect();
        if trns.iter().any(|&a| a < 255) {
            // Only up to the last entry that isn't opaque.
            let last = trns.iter().rposition(|&a| a < 255).map_or(0, |i| i + 1);
            enc.set_trns(trns.get(..last).unwrap_or(&[]).to_vec());
        }
        let mut wr = enc.write_header().map_err(err)?;
        wr.write_image_data(&packed).map_err(err)?;
    }
    let preview = render_indexed(data, &ix.shown(), w, h);
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

    /// A white square on a transparent canvas, with a soft edge (alpha 12 and 19 bands).
    fn soft() -> Document {
        let mut d = Document::new(30, 10, 72.0, None);
        let mut img = RgbaImage::new(30, 10, Rgba::TRANSPARENT);
        for y in 0..10 {
            for x in 0..30 {
                let a = match x {
                    0..10 => 0,
                    10 => 12,
                    11 => 19,
                    _ => 255,
                };
                img.set(x, y, Rgba::new(40, 90, 200, a));
            }
        }
        if let Some(c) = d.layers[0].elements_mut(0) {
            c.push(firecraft_doc::Element::new(firecraft_doc::ElementKind::Bitmap(
                firecraft_doc::Bitmap { x: 0, y: 0, image: img },
            )));
        }
        d
    }

    #[test]
    fn png8_grayscale_with_alpha_keeps_the_background_clear() {
        let d = soft();
        let s = ExportSettings {
            palette: PaletteKind::Grayscale,
            transparency: Transparency::Alpha,
            ..with(ExportFormat::Png8)
        };
        let e = export(&d, &s).unwrap_or_else(|e| unreachable!("{e}"));
        // As Fireworks MX: transparent below alpha 16, opaque grey (over white) above.
        assert_eq!(e.preview.get(2, 2).map(|p| p.a), Some(0));
        assert_eq!(e.preview.get(10, 2).map(|p| p.a), Some(0));
        assert_eq!(e.preview.get(11, 2).map(|p| p.a), Some(255));
        assert_eq!(e.preview.get(20, 2).map(|p| p.a), Some(255));
        let back = crate::formats::open_flat(&e.bytes, None).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(back.render(0).get(2, 2).map(|p| p.a), Some(0));
    }

    #[test]
    fn png8_alpha_palettes_round_alpha_unless_exact() {
        let d = soft();
        let s = ExportSettings {
            palette: PaletteKind::Exact,
            transparency: Transparency::Alpha,
            ..with(ExportFormat::Png8)
        };
        let e = export(&d, &s).unwrap_or_else(|e| unreachable!("{e}"));
        // Fireworks MX's 16 levels: 12 and 19 both become 17.
        assert_eq!(e.preview.get(10, 2).map(|p| p.a), Some(17));
        assert_eq!(e.preview.get(11, 2).map(|p| p.a), Some(17));
        let exact = ExportSettings { exact_alpha: true, ..s };
        let e = export(&d, &exact).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(e.preview.get(10, 2), Some(Rgba::new(40, 90, 200, 12)));
        assert_eq!(e.preview.get(11, 2), Some(Rgba::new(40, 90, 200, 19)));
        assert_eq!(e.preview.get(2, 2).map(|p| p.a), Some(0));
    }

    #[test]
    fn index_transparency_keys_out_the_matte() {
        let d = soft();
        for f in [ExportFormat::Gif, ExportFormat::Png8] {
            let s = ExportSettings {
                transparency: Transparency::Index,
                ..with(f)
            };
            let e = export(&d, &s).unwrap_or_else(|e| unreachable!("{e}"));
            // Flattened onto white, and white is the transparent entry (Fireworks MX).
            assert_eq!(e.preview.get(2, 2).map(|p| p.a), Some(0), "{f:?}");
            assert_eq!(e.preview.get(20, 2).map(|p| p.a), Some(255), "{f:?}");
        }
    }

    fn two_frames(clear: bool) -> Document {
        let mut d = Document::new(20, 20, 72.0, if clear { None } else { Some(Rgba::WHITE) });
        d.frames.push(firecraft_doc::Frame::default());
        let mut a = RgbaImage::new(4, 4, Rgba::new(255, 0, 0, 255));
        a.set(0, 0, Rgba::new(0, 0, 255, 255));
        for (frame, x) in [(0usize, 2i64), (1, 10)] {
            if let Some(c) = d.layers[0].elements_mut(frame) {
                c.push(firecraft_doc::Element::new(firecraft_doc::ElementKind::Bitmap(
                    firecraft_doc::Bitmap { x, y: 2, image: a.clone() },
                )));
            }
        }
        d
    }

    fn gif_frames(bytes: &[u8]) -> Vec<(u16, u16, u16, u16, Option<u8>, gif::DisposalMethod)> {
        let mut o = gif::DecodeOptions::new();
        o.set_color_output(gif::ColorOutput::Indexed);
        let mut dec = o.read_info(Cursor::new(bytes)).unwrap_or_else(|e| unreachable!("{e}"));
        let mut out = Vec::new();
        while let Ok(Some(f)) = dec.read_next_frame() {
            out.push((f.left, f.top, f.width, f.height, f.transparent, f.dispose));
        }
        out
    }

    #[test]
    fn auto_crop_and_difference_shape_the_frames() {
        let d = two_frames(false);
        let s = with(ExportFormat::AnimatedGif);
        let e = export(&d, &s).unwrap_or_else(|e| unreachable!("{e}"));
        let f = gif_frames(&e.bytes);
        assert_eq!(f.len(), 2);
        assert_eq!((f[0].0, f[0].1, f[0].2, f[0].3), (0, 0, 20, 20));
        // Frame 2 covers only the change: the square left x 2..6, arrived at 10..14.
        assert_eq!((f[1].0, f[1].1, f[1].2, f[1].3), (2, 2, 12, 4));
        assert!(f[1].4.is_some(), "Auto Difference uses a transparent entry");
        assert_eq!(f[1].5, gif::DisposalMethod::Keep);
        let back = crate::formats::open_flat(&e.bytes, None).unwrap_or_else(|e| unreachable!("{e}"));
        for i in 0..2 {
            assert_eq!(back.render(i), d.render(i), "frame {i}");
        }
        // Both off: whole frames, no transparent entry.
        let plain = ExportSettings {
            auto_crop: false,
            auto_difference: false,
            ..s
        };
        let f = gif_frames(&export(&d, &plain).map(|e| e.bytes).unwrap_or_default());
        assert_eq!((f[1].2, f[1].3, f[1].4), (20, 20, None));
    }

    #[test]
    fn transparent_animations_clear_each_frame() {
        let d = two_frames(true);
        let s = ExportSettings {
            transparency: Transparency::Alpha,
            ..with(ExportFormat::AnimatedGif)
        };
        let e = export(&d, &s).unwrap_or_else(|e| unreachable!("{e}"));
        let f = gif_frames(&e.bytes);
        // Auto Crop keeps each frame to its visible pixels; frames clear behind them.
        assert_eq!((f[1].0, f[1].1, f[1].2, f[1].3), (10, 2, 4, 4));
        assert_eq!(f[0].5, gif::DisposalMethod::Background);
        let back = crate::formats::open_flat(&e.bytes, None).unwrap_or_else(|e| unreachable!("{e}"));
        for i in 0..2 {
            assert_eq!(back.render(i), d.render(i), "frame {i}");
        }
    }

    #[test]
    fn png_optimizer_shrinks_without_changing_pixels() {
        if !png_optimizer_available() {
            return;
        }
        let d = doc();
        for f in [ExportFormat::Png8, ExportFormat::Png24, ExportFormat::Png32] {
            let plain = export(&d, &with(f)).unwrap_or_else(|e| unreachable!("{e}"));
            let s = ExportSettings {
                png_optimize: 3,
                ..with(f)
            };
            let small = export_file(&d, &s).unwrap_or_else(|e| unreachable!("{e}"));
            assert!(small.len() <= plain.bytes.len(), "{f:?}");
            let a = crate::formats::open_flat(&plain.bytes, None).map(|d| d.render(0));
            let b = crate::formats::open_flat(&small, None).map(|d| d.render(0));
            assert_eq!(a.ok(), b.ok(), "{f:?} pixels unchanged");
        }
    }

    #[test]
    fn png8_keeps_few_colors_exactly_at_a_small_depth() {
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
