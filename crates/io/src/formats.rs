//! Opening flat image files (everything that isn't a Fireworks PNG) as documents.
//!
//! What Fireworks MX makes of each format was measured with the oracle (documented in
//! docs/behavior/opening-files.md): the layers it creates, the resolution it keeps and how it
//! splits an animated GIF into frames. Formats Fireworks MX can't open (WebP, JPEG XL, animated
//! PNG) follow the same conventions.

use std::io::Cursor;

use firecraft_doc::{Bitmap, Document, Element, ElementKind, Frame, Layer};
use firecraft_raster::{Rgba, RgbaImage};

use crate::{Error, Result};

/// Largest image we will decode (pixels per frame); guards hostile headers.
const MAX_PIXELS: u64 = 1 << 28;
/// Largest total of decoded frame pixels in an animation.
const MAX_ANIMATION_PIXELS: u64 = 1 << 30;

/// A file format recognised when opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Png,
    Jpeg,
    Gif,
    Bmp,
    Tiff,
    Tga,
    WebP,
    JpegXl,
    /// Recognised, not opened yet (roadmap).
    Photoshop,
    Illustrator,
    FreeHand,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Gif => "GIF",
            Self::Bmp => "BMP",
            Self::Tiff => "TIFF",
            Self::Tga => "Targa",
            Self::WebP => "WebP",
            Self::JpegXl => "JPEG XL",
            Self::Photoshop => "Photoshop",
            Self::Illustrator => "Illustrator",
            Self::FreeHand => "FreeHand",
        }
    }
}

/// File extensions FireCraft opens (for file dialogs), lower case, without dots.
pub const OPEN_EXTENSIONS: [&str; 12] = [
    "png", "jpg", "jpeg", "jpe", "gif", "bmp", "tif", "tiff", "tga", "webp", "jxl", "jfif",
];

fn extension(name: Option<&str>) -> String {
    name.and_then(|n| n.rsplit(['/', '\\']).next())
        .and_then(|n| n.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// The format of a file, from its first bytes (and its name, for formats without a signature).
pub fn sniff(bytes: &[u8], name: Option<&str>) -> Option<Format> {
    let ext = extension(name);
    let starts = |sig: &[u8]| bytes.starts_with(sig);
    if starts(b"\x89PNG\r\n\x1a\n") {
        Some(Format::Png)
    } else if starts(&[0xFF, 0xD8, 0xFF]) {
        Some(Format::Jpeg)
    } else if starts(b"GIF87a") || starts(b"GIF89a") {
        Some(Format::Gif)
    } else if starts(b"BM") && bytes.len() > 26 {
        Some(Format::Bmp)
    } else if starts(b"II*\0") || starts(b"MM\0*") {
        Some(Format::Tiff)
    } else if starts(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        Some(Format::WebP)
    } else if starts(&[0xFF, 0x0A]) || starts(b"\0\0\0\x0cJXL \r\n\x87\n") {
        Some(Format::JpegXl)
    } else if starts(b"8BPS") {
        Some(Format::Photoshop)
    } else if (starts(b"%PDF") || starts(b"%!PS-Adobe")) && ext == "ai" {
        Some(Format::Illustrator)
    } else if ext.starts_with("fh") || ext.starts_with("ft") && ext.len() <= 4 {
        Some(Format::FreeHand)
    } else if ext == "tga" || bytes.ends_with(b"TRUEVISION-XFILE.\0") {
        // Targa has no signature at the start (only version 2 files end with one).
        Some(Format::Tga)
    } else {
        None
    }
}

/// One picture of a decoded file: a bitmap at a position, shown for `delay` hundredths of a second.
struct Picture {
    x: i32,
    y: i32,
    image: RgbaImage,
}

/// How a flat file becomes a document, following what Fireworks MX does with each format.
struct Layout {
    /// Bottom to top: (name, shared across frames, the picture in each frame).
    layers: Vec<(String, bool, Vec<Picture>)>,
    delays: Option<Vec<u32>>,
    canvas: Option<Rgba>,
    resolution: (f64, bool),
}

/// Open a flat image file of any supported format as a document.
pub fn open_flat(bytes: &[u8], name: Option<String>) -> Result<Document> {
    let format = sniff(bytes, name.as_deref()).ok_or_else(|| {
        Error::Unsupported("not an image format FireCraft can open".into())
    })?;
    let layout = match format {
        Format::Png => png_layout(bytes)?,
        Format::Jpeg => single(decode_with_image(bytes, image::ImageFormat::Jpeg)?, true, jfif_resolution(bytes)),
        Format::Gif => gif_layout(bytes)?,
        Format::Bmp => single(decode_with_image(bytes, image::ImageFormat::Bmp)?, false, (72.0, false)),
        Format::Tiff => single(
            decode_with_image(bytes, image::ImageFormat::Tiff)?,
            false,
            tiff_resolution(bytes),
        ),
        Format::Tga => single(decode_with_image(bytes, image::ImageFormat::Tga)?, false, (72.0, false)),
        Format::WebP => webp_layout(bytes)?,
        Format::JpegXl => jxl_layout(bytes)?,
        Format::Photoshop | Format::Illustrator | Format::FreeHand => {
            return Err(Error::Unsupported(format!(
                "{} files can't be opened yet (it's on the roadmap)",
                format.name()
            )));
        }
    };
    let mut doc = build(layout, name);
    doc.export = export_settings_for(format, bytes, &doc);
    Ok(doc)
}

/// The export settings an opened image starts with, as Fireworks MX chooses them (measured with
/// `dom.exportOptions`): a JPEG exports as JPEG at the quality it was saved with, a GIF as GIF
/// (animated if it has frames) with its exact palette; FireCraft keeps PNG, WebP and JPEG XL
/// files in their own format.
fn export_settings_for(format: Format, bytes: &[u8], doc: &Document) -> firecraft_doc::ExportSettings {
    use firecraft_doc::{ExportFormat, ExportSettings, PaletteKind};
    let d = ExportSettings::default();
    match format {
        Format::Jpeg => ExportSettings {
            format: ExportFormat::Jpeg,
            jpeg_quality: jpeg_quality(bytes).map_or(80, crate::export::mx_quality),
            ..d
        },
        Format::Gif => ExportSettings {
            format: if doc.frames.len() > 1 {
                ExportFormat::AnimatedGif
            } else {
                ExportFormat::Gif
            },
            palette: PaletteKind::Exact,
            colors: 256,
            ..d
        },
        Format::Png => ExportSettings {
            format: match png_color_type(bytes) {
                Some(png::ColorType::Indexed) => ExportFormat::Png8,
                Some(png::ColorType::Rgb | png::ColorType::Grayscale) => ExportFormat::Png24,
                _ => ExportFormat::Png32,
            },
            palette: PaletteKind::Exact,
            colors: 256,
            ..d
        },
        Format::WebP => ExportSettings {
            format: ExportFormat::WebP,
            ..d
        },
        Format::JpegXl => ExportSettings {
            format: ExportFormat::JpegXl,
            ..d
        },
        _ => d,
    }
}

fn png_color_type(bytes: &[u8]) -> Option<png::ColorType> {
    let r = png::Decoder::new(Cursor::new(bytes)).read_info().ok()?;
    Some(r.info().color_type)
}

/// A JPEG's quality, estimated from its luminance quantization table against the IJG standard
/// table (the scaling libjpeg's quality setting applies).
pub(crate) fn jpeg_quality(bytes: &[u8]) -> Option<u8> {
    const STD: [u16; 64] = [
        16, 11, 10, 16, 24, 40, 51, 61, 12, 12, 14, 19, 26, 58, 60, 55, 14, 13, 16, 24, 40, 57, 69, 56, 14,
        17, 22, 29, 51, 87, 80, 62, 18, 22, 37, 56, 68, 109, 103, 77, 24, 35, 55, 64, 81, 104, 113, 92,
        49, 64, 78, 87, 103, 121, 120, 101, 72, 92, 95, 98, 112, 100, 103, 99,
    ];
    // Find the first DQT segment (FFDB) and its table 0.
    let mut at = 2;
    while at + 4 <= bytes.len() {
        if bytes.get(at) != Some(&0xFF) {
            return None;
        }
        let marker = *bytes.get(at + 1)?;
        let len = usize::from(u16::from_be_bytes([*bytes.get(at + 2)?, *bytes.get(at + 3)?]));
        if marker == 0xDB {
            let pq = bytes.get(at + 4)? >> 4;
            let table: Vec<u16> = if pq == 0 {
                bytes.get(at + 5..at + 69)?.iter().map(|&v| u16::from(v)).collect()
            } else {
                bytes
                    .get(at + 5..at + 133)?
                    .chunks_exact(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect()
            };
            // Tables are stored in zig-zag order; the standard table above is in natural order.
            const ZIGZAG: [usize; 64] = [
                0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27,
                20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51,
                58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
            ];
            let mut sum = 0f64;
            for (k, &q) in table.iter().enumerate() {
                let std = f64::from(*STD.get(*ZIGZAG.get(k)?)?);
                sum += f64::from(q) * 100.0 / std;
            }
            let scale = sum / 64.0;
            let q = if scale <= 100.0 { (200.0 - scale) / 2.0 } else { 5000.0 / scale };
            return Some(q.round().clamp(1.0, 100.0) as u8);
        }
        if marker == 0xDA {
            return None;
        }
        at += 2 + len;
    }
    None
}

fn build(layout: Layout, name: Option<String>) -> Document {
    let width = layout
        .layers
        .iter()
        .flat_map(|l| &l.2)
        .map(|p| p.x.max(0) as u32 + p.image.width())
        .max()
        .unwrap_or(1);
    let height = layout
        .layers
        .iter()
        .flat_map(|l| &l.2)
        .map(|p| p.y.max(0) as u32 + p.image.height())
        .max()
        .unwrap_or(1);
    let (res, cm) = layout.resolution;
    let mut doc = Document::new(width, height, res, layout.canvas);
    doc.resolution_cm = cm;
    doc.layers.clear();
    let frames = layout.delays.as_ref().map_or(1, |d| d.len().max(1));
    for (layer_name, shared, pictures) in layout.layers {
        let mut layer = Layer::new(layer_name);
        layer.shared = shared;
        for (frame, p) in pictures.into_iter().enumerate() {
            if let Some(cell) = layer.elements_mut(frame) {
                cell.push(Element::new(ElementKind::Bitmap(Bitmap {
                    x: p.x.into(),
                    y: p.y.into(),
                    image: p.image,
                })));
            }
        }
        if !shared {
            layer.cells.resize_with(frames, Vec::new);
        }
        doc.layers.push(layer);
    }
    doc.layers.push(Layer::web_layer());
    if let Some(delays) = layout.delays {
        doc.frames = delays
            .iter()
            .enumerate()
            .map(|(i, &delay)| Frame {
                name: format!("Frame {}", i + 1),
                delay,
                ..Frame::default()
            })
            .collect();
    }
    // Start on the top normal layer, as with any document Fireworks opens.
    doc.current_layer = doc.layers.len().saturating_sub(2);
    doc.path = name;
    doc
}

/// One still image: a "Background" layer, with an empty "Layer 1" above it for the formats
/// Fireworks MX opens that way (PNG, JPEG; TIFF, Targa and BMP get the Background only).
fn single(image: RgbaImage, layer_1: bool, resolution: (f64, bool)) -> Layout {
    let mut layers = vec![("Background".to_owned(), false, vec![Picture { x: 0, y: 0, image }])];
    if layer_1 {
        layers.push(("Layer 1".to_owned(), false, Vec::new()));
    }
    Layout {
        layers,
        delays: None,
        canvas: None,
        resolution,
    }
}

/// Full-canvas frames of an animation (WebP, JPEG XL, animated PNG) on one layer named after the
/// format, as Fireworks MX does with an animated GIF it can't split more cleverly.
fn animation(format: &str, frames: Vec<(RgbaImage, u32)>, resolution: (f64, bool)) -> Layout {
    let delays = frames.iter().map(|f| f.1).collect();
    let pictures = frames
        .into_iter()
        .map(|(image, _)| Picture { x: 0, y: 0, image })
        .collect();
    Layout {
        layers: vec![(format.to_owned(), false, pictures)],
        delays: Some(delays),
        canvas: None,
        resolution,
    }
}

fn to_rgba(img: image::RgbaImage) -> Result<RgbaImage> {
    let (w, h) = img.dimensions();
    RgbaImage::from_rgba_bytes(w, h, img.as_raw()).ok_or_else(|| Error::Decode("bad image dimensions".into()))
}

fn limits() -> image::Limits {
    let mut l = image::Limits::default();
    l.max_alloc = Some(MAX_PIXELS * 4 + (64 << 20));
    l
}

fn check_size(w: u64, h: u64) -> Result<()> {
    if w == 0 || h == 0 {
        Err(Error::Decode("the image is empty".into()))
    } else if w * h > MAX_PIXELS {
        Err(Error::Unsupported(format!("{w} x {h} image is too large")))
    } else {
        Ok(())
    }
}

/// Decode a still image with the `image` crate, turned upright by its EXIF orientation (photos
/// from phones and cameras). Fireworks MX ignored the orientation and showed such photos sideways.
fn decode_with_image(bytes: &[u8], format: image::ImageFormat) -> Result<RgbaImage> {
    use image::ImageDecoder;
    let mut reader = image::ImageReader::with_format(Cursor::new(bytes), format);
    reader.limits(limits());
    let mut decoder = reader.into_decoder().map_err(decode_error)?;
    let (w, h) = decoder.dimensions();
    check_size(w.into(), h.into())?;
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(decoder).map_err(decode_error)?;
    img.apply_orientation(orientation);
    to_rgba(img.into_rgba8())
}

fn decode_error(e: image::ImageError) -> Error {
    match e {
        image::ImageError::Unsupported(u) => Error::Unsupported(u.to_string()),
        e => Error::Decode(e.to_string()),
    }
}

/// An animation's frames as full canvases, with their delays in 1/100 s.
fn collect_frames(frames: image::Frames<'_>) -> Result<Vec<(RgbaImage, u32)>> {
    let mut out = Vec::new();
    let mut total = 0u64;
    for frame in frames {
        let frame = frame.map_err(decode_error)?;
        let (num, den) = frame.delay().numer_denom_ms();
        let delay = (f64::from(num) / f64::from(den.max(1)) / 10.0).round() as u32;
        let buffer = frame.into_buffer();
        total += u64::from(buffer.width()) * u64::from(buffer.height());
        if total > MAX_ANIMATION_PIXELS {
            return Err(Error::Unsupported("the animation is too large".into()));
        }
        out.push((to_rgba(buffer)?, delay));
    }
    Ok(out)
}

/// A PNG that isn't a Fireworks PNG: one bitmap, or the frames of an animated PNG.
fn png_layout(bytes: &[u8]) -> Result<Layout> {
    use image::AnimationDecoder;
    let resolution = png_resolution(bytes);
    let decoder = image::codecs::png::PngDecoder::with_limits(Cursor::new(bytes), limits()).map_err(decode_error)?;
    if decoder.is_apng().unwrap_or(false) {
        let frames = collect_frames(decoder.apng().map_err(decode_error)?.into_frames())?;
        if frames.len() > 1 {
            return Ok(animation("PNG", frames, resolution));
        }
    }
    Ok(single(crate::decode_png(bytes)?, true, resolution))
}

fn webp_layout(bytes: &[u8]) -> Result<Layout> {
    use image::AnimationDecoder;
    let decoder = image::codecs::webp::WebPDecoder::new(Cursor::new(bytes)).map_err(decode_error)?;
    if decoder.has_animation() {
        let frames = collect_frames(decoder.into_frames())?;
        if frames.len() > 1 {
            return Ok(animation("WebP", frames, (72.0, false)));
        }
    }
    Ok(single(decode_with_image(bytes, image::ImageFormat::WebP)?, true, (72.0, false)))
}

/// Resolution from a PNG's `pHYs`: Fireworks MX shows it in pixels/cm (the unit PNG stores).
fn png_resolution(bytes: &[u8]) -> (f64, bool) {
    let dec = png::Decoder::new(Cursor::new(bytes));
    let dims = dec.read_info().ok().and_then(|r| r.info().pixel_dims);
    match dims {
        Some(png::PixelDimensions {
            xppu,
            unit: png::Unit::Meter,
            ..
        }) if xppu > 0 => (f64::from(xppu) / 100.0, true),
        _ => (72.0, false),
    }
}

/// Resolution from a JPEG's JFIF header (pixels/inch or pixels/cm).
fn jfif_resolution(bytes: &[u8]) -> (f64, bool) {
    let app0 = bytes.get(2..20).filter(|s| s.starts_with(&[0xFF, 0xE0]) && s.get(4..9) == Some(b"JFIF\0"));
    let Some(s) = app0 else { return (72.0, false) };
    let density = s
        .get(12..14)
        .map_or(0, |d| u16::from_be_bytes([d.first().copied().unwrap_or(0), d.get(1).copied().unwrap_or(0)]));
    match (s.get(11), density) {
        (Some(1), d) if d > 0 => (f64::from(d), false),
        (Some(2), d) if d > 0 => (f64::from(d), true),
        _ => (72.0, false),
    }
}

/// Resolution from a TIFF's XResolution / ResolutionUnit tags.
fn tiff_resolution(bytes: &[u8]) -> (f64, bool) {
    use tiff::tags::Tag;
    let Ok(mut dec) = tiff::decoder::Decoder::new(Cursor::new(bytes)) else {
        return (72.0, false);
    };
    let res = dec.get_tag_f64(Tag::XResolution).ok().filter(|r| r.is_finite() && *r > 0.0);
    let unit = dec.find_tag_unsigned::<u16>(Tag::ResolutionUnit).ok().flatten().unwrap_or(2);
    match (res, unit) {
        (Some(r), 2) => (r, false),
        (Some(r), 3) => (r, true),
        _ => (72.0, false),
    }
}

// --- GIF ---------------------------------------------------------------------------------------

struct GifFrame {
    rect: (i32, i32, u32, u32),
    /// The frame's own pixels (transparent where the GIF leaves the previous picture showing).
    image: RgbaImage,
    delay: u32,
    dispose: gif::DisposalMethod,
}

/// Open a GIF the way Fireworks MX does (measured; see docs/behavior/opening-files.md):
///
/// * one frame: a "Layer 1" with the picture;
/// * every frame cleared to the background afterwards: a "GIF" layer with each frame's own
///   picture at its position;
/// * every frame kept and an opaque full first frame: a shared "Background" layer with the first
///   frame, and a "GIF" layer with each frame's picture over the area the later frames change;
///   the canvas takes the GIF's background colour;
/// * otherwise: a "GIF" layer with each frame as the whole picture shows it.
fn gif_layout(bytes: &[u8]) -> Result<Layout> {
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::RGBA);
    opts.set_memory_limit(gif::MemoryLimit::Bytes(
        std::num::NonZeroU64::new(MAX_PIXELS * 4).unwrap_or(std::num::NonZeroU64::MIN),
    ));
    opts.check_lzw_end_code(false);
    let mut dec = opts.read_info(Cursor::new(bytes)).map_err(|e| Error::Decode(e.to_string()))?;
    let (w, h) = (u32::from(dec.width()), u32::from(dec.height()));
    check_size(w.into(), h.into())?;
    let background = dec.bg_color().and_then(|i| {
        let p = dec.global_palette()?;
        Some(Rgba::new(*p.get(i * 3)?, *p.get(i * 3 + 1)?, *p.get(i * 3 + 2)?, 255))
    });
    let mut frames = Vec::new();
    let mut total = 0u64;
    loop {
        let frame = match dec.read_next_frame() {
            Ok(Some(f)) => f,
            Ok(None) => break,
            // A damaged later frame: keep what was read (as browsers do).
            Err(_) if !frames.is_empty() => break,
            Err(e) => return Err(Error::Decode(e.to_string())),
        };
        let (fw, fh) = (u32::from(frame.width), u32::from(frame.height));
        total += u64::from(fw) * u64::from(fh);
        if total > MAX_ANIMATION_PIXELS {
            return Err(Error::Unsupported("the animation is too large".into()));
        }
        let image = RgbaImage::from_rgba_bytes(fw, fh, &frame.buffer)
            .ok_or_else(|| Error::Decode("GIF frame size mismatch".into()))?;
        frames.push(GifFrame {
            rect: (i32::from(frame.left), i32::from(frame.top), fw, fh),
            image,
            delay: u32::from(frame.delay),
            dispose: frame.dispose,
        });
    }
    if frames.is_empty() {
        return Err(Error::Decode("the GIF has no image".into()));
    }
    Ok(gif_frames_layout(w, h, frames, background))
}

fn gif_frames_layout(w: u32, h: u32, frames: Vec<GifFrame>, background: Option<Rgba>) -> Layout {
    use gif::DisposalMethod as D;
    let delays: Vec<u32> = frames.iter().map(|f| f.delay).collect();
    let layout = |layers, canvas| Layout {
        layers,
        delays: Some(delays.clone()),
        canvas,
        resolution: (72.0, false),
    };
    if frames.len() == 1 {
        let pictures = frames.into_iter().map(|f| own_picture(w, h, f)).collect();
        return layout(vec![("Layer 1".to_owned(), false, pictures)], None);
    }
    if frames.iter().all(|f| f.dispose == D::Background) {
        let pictures = frames.into_iter().map(|f| own_picture(w, h, f)).collect();
        return layout(vec![("GIF".to_owned(), false, pictures)], None);
    }
    let composites = composite_gif(w, h, &frames);
    let kept = frames.iter().all(|f| matches!(f.dispose, D::Keep | D::Any));
    let first_full = frames.first().is_some_and(|f| {
        f.rect == (0, 0, w, h) && f.image.pixels().iter().all(|p| p.a == 255)
    });
    if kept && first_full {
        // The area the later frames change.
        let mut area: Option<(i32, i32, i32, i32)> = None;
        for f in frames.iter().skip(1) {
            let (x, y, fw, fh) = clip_rect(f.rect, w, h);
            if fw == 0 || fh == 0 {
                continue;
            }
            let r = (x, y, x + fw as i32, y + fh as i32);
            area = Some(area.map_or(r, |a| (a.0.min(r.0), a.1.min(r.1), a.2.max(r.2), a.3.max(r.3))));
        }
        let mut composites = composites.into_iter();
        let first = composites.next().unwrap_or_else(|| RgbaImage::new(w, h, Rgba::TRANSPARENT));
        let mut layers = Vec::new();
        let changes = area.map(|(x0, y0, x1, y1)| {
            std::iter::once(&first)
                .chain(composites.as_slice())
                .map(|c| Picture {
                    x: x0,
                    y: y0,
                    image: crop(c, x0, y0, (x1 - x0) as u32, (y1 - y0) as u32),
                })
                .collect::<Vec<_>>()
        });
        layers.push((
            "Background".to_owned(),
            true,
            vec![Picture {
                x: 0,
                y: 0,
                image: first.clone(),
            }],
        ));
        layers.push(("GIF".to_owned(), false, changes.unwrap_or_default()));
        return layout(layers, Some(background.unwrap_or(Rgba::new(255, 255, 255, 255))));
    }
    // The first frame keeps its own picture and position (anim2.gif: 167x221 at 5,4 on a 495x228
    // screen); the later ones are whole pictures.
    let mut pictures: Vec<Picture> = frames.into_iter().take(1).map(|f| own_picture(w, h, f)).collect();
    pictures.extend(composites.into_iter().skip(1).map(|image| Picture { x: 0, y: 0, image }));
    layout(vec![("GIF".to_owned(), false, pictures)], None)
}

/// A frame's own picture, clipped to the canvas.
fn own_picture(w: u32, h: u32, f: GifFrame) -> Picture {
    let (x, y, cw, ch) = clip_rect(f.rect, w, h);
    let image = if (x, y, cw, ch) == f.rect {
        f.image
    } else {
        crop(&f.image, x - f.rect.0, y - f.rect.1, cw.max(1), ch.max(1))
    };
    Picture { x, y, image }
}

fn clip_rect((x, y, fw, fh): (i32, i32, u32, u32), w: u32, h: u32) -> (i32, i32, u32, u32) {
    let x1 = (x as i64 + fw as i64).min(w as i64);
    let y1 = (y as i64 + fh as i64).min(h as i64);
    let (x0, y0) = (x.max(0) as i64, y.max(0) as i64);
    (x0 as i32, y0 as i32, (x1 - x0).max(0) as u32, (y1 - y0).max(0) as u32)
}

fn crop(src: &RgbaImage, x: i32, y: i32, w: u32, h: u32) -> RgbaImage {
    let mut out = RgbaImage::new(w, h, Rgba::TRANSPARENT);
    out.copy_from(src, -i64::from(x), -i64::from(y));
    out
}

/// Each frame as the GIF shows it: drawn over what the previous frame's disposal left.
fn composite_gif(w: u32, h: u32, frames: &[GifFrame]) -> Vec<RgbaImage> {
    use gif::DisposalMethod as D;
    let mut canvas = RgbaImage::new(w, h, Rgba::TRANSPARENT);
    let mut out = Vec::with_capacity(frames.len());
    for f in frames {
        let saved = (f.dispose == D::Previous).then(|| canvas.clone());
        let (x, y, _, _) = f.rect;
        for (i, p) in f.image.pixels().iter().enumerate() {
            if p.a > 0 {
                let (px, py) = (i as u32 % f.rect.2, i as u32 / f.rect.2.max(1));
                canvas.set(i64::from(x) + i64::from(px), i64::from(y) + i64::from(py), *p);
            }
        }
        out.push(canvas.clone());
        match f.dispose {
            D::Background => {
                let (cx, cy, cw, ch) = clip_rect(f.rect, w, h);
                for yy in cy..cy + ch as i32 {
                    for xx in cx..cx + cw as i32 {
                        canvas.set(xx.into(), yy.into(), Rgba::TRANSPARENT);
                    }
                }
            }
            D::Previous => {
                if let Some(s) = saved {
                    canvas = s;
                }
            }
            D::Keep | D::Any => {}
        }
    }
    out
}

// --- JPEG XL -----------------------------------------------------------------------------------

fn jxl_layout(bytes: &[u8]) -> Result<Layout> {
    let err = |e: Box<dyn std::error::Error + Send + Sync>| Error::Decode(format!("JPEG XL: {e}"));
    let mut image = jxl_oxide::JxlImage::builder().read(Cursor::new(bytes)).map_err(err)?;
    check_size(image.width().into(), image.height().into())?;
    image.set_cms(jxl_oxide::Moxcms);
    image.request_color_encoding(jxl_oxide::EnumColourEncoding::srgb(
        jxl_oxide::RenderingIntent::Relative,
    ));
    // Seconds per tick, for animation delays.
    let tick = image
        .image_header()
        .metadata
        .animation
        .as_ref()
        .filter(|a| a.tps_numerator > 0)
        .map(|a| f64::from(a.tps_denominator) / f64::from(a.tps_numerator));
    let count = image.num_loaded_keyframes();
    let mut frames = Vec::with_capacity(count);
    let mut total = 0u64;
    for k in 0..count {
        let render = image.render_frame(k).map_err(err)?;
        let mut stream = render.stream();
        let (w, h, ch) = (stream.width(), stream.height(), stream.channels() as usize);
        total += u64::from(w) * u64::from(h);
        if total > MAX_ANIMATION_PIXELS {
            return Err(Error::Unsupported("the image is too large".into()));
        }
        let mut buf = vec![0u8; w as usize * h as usize * ch];
        stream.write_to_buffer(&mut buf);
        let rgba: Vec<u8> = match ch {
            1 => buf.iter().flat_map(|&g| [g, g, g, 255]).collect(),
            2 => buf.as_chunks::<2>().0.iter().flat_map(|&[g, a]| [g, g, g, a]).collect(),
            3 => buf.as_chunks::<3>().0.iter().flat_map(|&[r, g, b]| [r, g, b, 255]).collect(),
            4 => buf,
            n => return Err(Error::Unsupported(format!("JPEG XL with {n} channels"))),
        };
        let img = RgbaImage::from_rgba_bytes(w, h, &rgba)
            .ok_or_else(|| Error::Decode("JPEG XL frame size mismatch".into()))?;
        let delay = tick.map_or(0, |t| (f64::from(render.duration()) * t * 100.0).round() as u32);
        frames.push((img, delay));
    }
    match frames.len() {
        0 => Err(Error::Decode("JPEG XL: no image".into())),
        1 => {
            let (img, _) = frames.remove(0);
            Ok(single(img, true, (72.0, false)))
        }
        _ => Ok(animation("JPEG XL", frames, (72.0, false))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_signatures_and_extensions() {
        assert_eq!(sniff(b"\xFF\xD8\xFF\xE0", None), Some(Format::Jpeg));
        assert_eq!(sniff(b"GIF89a....", None), Some(Format::Gif));
        assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 ", None), Some(Format::WebP));
        assert_eq!(sniff(b"\xFF\x0A....", None), Some(Format::JpegXl));
        assert_eq!(sniff(b"8BPS\0\x01", Some("a.psd")), Some(Format::Photoshop));
        assert_eq!(sniff(b"%PDF-1.4", Some("logo.AI")), Some(Format::Illustrator));
        assert_eq!(sniff(b"%PDF-1.4", Some("doc.pdf")), None);
        assert_eq!(sniff(b"FHD3....", Some("art.fh10")), Some(Format::FreeHand));
        assert_eq!(sniff(b"\0\0\x02\0", Some("C:\\x\\pic.TGA")), Some(Format::Tga));
        assert_eq!(sniff(b"\0\0\x02\0", Some("pic.dat")), None);
    }

    #[test]
    fn only_png_documents_save_in_place() {
        assert!(crate::saves_in_place("/a/b/Pic.PNG"));
        assert!(!crate::saves_in_place("/a/b/photo.jpg"));
        assert_eq!(crate::png_name("C:\\x\\photo.final.jpg"), "photo.final.png");
        assert_eq!(crate::png_name("handle://3/anim.gif"), "anim.png");
        assert_eq!(crate::png_name("Untitled-1.png"), "Untitled-1.png");
    }

    #[test]
    fn roadmap_formats_explain_themselves() {
        let e = open_flat(b"8BPS\0\x01\0\0", Some("a.psd".into())).err().map(|e| e.to_string());
        assert!(e.is_some_and(|m| m.contains("Photoshop") && m.contains("roadmap")));
    }

    fn encode(img: &image::RgbaImage, format: image::ImageFormat) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(img.clone())
            .to_rgb8()
            .write_to(&mut out, format)
            .unwrap_or_default();
        out.into_inner()
    }

    fn probe() -> image::RgbaImage {
        image::RgbaImage::from_fn(6, 4, |x, y| image::Rgba([x as u8 * 40, y as u8 * 60, 200, 255]))
    }

    #[test]
    fn opens_still_formats_like_fireworks() {
        for (format, layers) in [
            (image::ImageFormat::Bmp, vec!["Background", "Web Layer"]),
            (image::ImageFormat::Tiff, vec!["Background", "Web Layer"]),
            (image::ImageFormat::Jpeg, vec!["Background", "Layer 1", "Web Layer"]),
        ] {
            let bytes = encode(&probe(), format);
            let doc = open_flat(&bytes, Some("x".into())).unwrap_or_else(|e| unreachable!("{format:?}: {e}"));
            assert_eq!((doc.width, doc.height), (6, 4), "{format:?}");
            let names: Vec<&str> = doc.layers.iter().map(|l| l.name.as_str()).collect();
            assert_eq!(names, layers, "{format:?}");
            assert_eq!(doc.current_layer, layers.len() - 2);
            let px = doc.render(0).get(5, 3);
            if format != image::ImageFormat::Jpeg {
                assert_eq!(px, Some(Rgba::new(200, 180, 200, 255)), "{format:?}");
            }
        }
    }

    #[test]
    fn jpeg_quality_is_estimated_from_its_tables() {
        for q in [30u8, 50, 75, 86, 95] {
            let mut out = Vec::new();
            let enc = jpeg_encoder::Encoder::new(&mut out, q);
            enc.encode(&[200u8; 8 * 8 * 3], 8, 8, jpeg_encoder::ColorType::Rgb)
                .unwrap_or_else(|e| unreachable!("{e}"));
            assert_eq!(jpeg_quality(&out), Some(q));
        }
    }

    #[test]
    fn jfif_density_is_the_resolution() {
        let mut bytes = encode(&probe(), image::ImageFormat::Jpeg);
        // Patch the JFIF header to 300 dpi.
        if let Some(s) = bytes.get_mut(13..18) {
            s.copy_from_slice(&[1, 1, 44, 1, 44]);
        }
        assert_eq!(jfif_resolution(&bytes), (300.0, false));
    }

    #[test]
    fn png_resolution_is_kept_in_pixels_per_cm() {
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(&mut out, 1, 1);
            enc.set_color(png::ColorType::Rgba);
            enc.set_pixel_dims(Some(png::PixelDimensions {
                xppu: 5905,
                yppu: 5905,
                unit: png::Unit::Meter,
            }));
            if let Ok(mut w) = enc.write_header() {
                w.write_image_data(&[1, 2, 3, 255]).unwrap_or_default();
            }
        }
        let doc = open_flat(&out, None).unwrap_or_else(|e| unreachable!("{e}"));
        assert!(doc.resolution_cm);
        assert!((doc.resolution - 59.05).abs() < 1e-9);
    }

    fn frame(rect: (i32, i32, u32, u32), color: Rgba, dispose: gif::DisposalMethod) -> GifFrame {
        GifFrame {
            rect,
            image: RgbaImage::new(rect.2, rect.3, color),
            delay: 50,
            dispose,
        }
    }

    const RED: Rgba = Rgba::new(255, 0, 0, 255);
    const BLUE: Rgba = Rgba::new(0, 0, 255, 255);
    const GREEN: Rgba = Rgba::new(0, 255, 0, 255);

    #[test]
    fn kept_gif_frames_split_into_background_and_changes() {
        use gif::DisposalMethod as D;
        let frames = vec![
            frame((0, 0, 60, 40), RED, D::Keep),
            frame((30, 20, 20, 10), BLUE, D::Keep),
            frame((5, 25, 10, 10), GREEN, D::Keep),
        ];
        let doc = build(gif_frames_layout(60, 40, frames, None), None);
        let names: Vec<&str> = doc.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Background", "GIF", "Web Layer"]);
        assert!(doc.layers[0].shared);
        assert_eq!(doc.frames.len(), 3);
        for f in 0..3 {
            let b = doc.layers[1].elements(f).first().and_then(|e| e.bounds());
            assert_eq!(b, Some((5.0, 20.0, 45.0, 15.0)), "frame {f}");
        }
        assert_eq!(doc.canvas, Some(Rgba::new(255, 255, 255, 255)));
        // Frame 3 still shows frame 2's blue (kept) and its own green.
        let r = doc.render(2);
        assert_eq!(r.get(35, 22), Some(BLUE));
        assert_eq!(r.get(6, 26), Some(GREEN));
        assert_eq!(doc.render(1).get(6, 26), Some(RED));
    }

    #[test]
    fn cleared_gif_frames_keep_their_own_pictures() {
        use gif::DisposalMethod as D;
        let frames = vec![
            frame((0, 0, 60, 40), RED, D::Background),
            frame((30, 20, 20, 10), BLUE, D::Background),
        ];
        let doc = build(gif_frames_layout(60, 40, frames, None), None);
        let names: Vec<&str> = doc.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["GIF", "Web Layer"]);
        let b = doc.layers[0].elements(1).first().and_then(|e| e.bounds());
        assert_eq!(b, Some((30.0, 20.0, 20.0, 10.0)));
        assert_eq!(doc.canvas, None);
        assert_eq!(doc.frames.iter().map(|f| f.delay).collect::<Vec<_>>(), [50, 50]);
    }

    #[test]
    fn growing_gif_keeps_its_first_frame_and_composites_the_rest() {
        use gif::DisposalMethod as D;
        // anim2.gif's shape: frames grow from the left on a larger screen; the last is cleared.
        let frames = vec![
            frame((5, 4, 167, 221), RED, D::Keep),
            frame((5, 4, 239, 221), BLUE, D::Keep),
            frame((5, 4, 489, 221), GREEN, D::Background),
        ];
        let doc = build(gif_frames_layout(495, 228, frames, None), None);
        assert_eq!((doc.width, doc.height), (495, 228));
        let bounds = |f| doc.layers[0].elements(f).first().and_then(|e| e.bounds());
        assert_eq!(bounds(0), Some((5.0, 4.0, 167.0, 221.0)));
        assert_eq!(bounds(1), Some((0.0, 0.0, 495.0, 228.0)));
        assert_eq!(doc.render(1).get(4, 4), Some(Rgba::TRANSPARENT));
    }

    #[test]
    fn restored_gif_frames_become_whole_pictures() {
        use gif::DisposalMethod as D;
        let frames = vec![
            frame((0, 0, 60, 40), RED, D::Any),
            frame((30, 20, 20, 10), BLUE, D::Previous),
            frame((5, 25, 10, 10), GREEN, D::Any),
        ];
        let doc = build(gif_frames_layout(60, 40, frames, None), None);
        assert_eq!(doc.layers.len(), 2);
        let r = doc.render(2);
        // Frame 2's blue was restored away before frame 3.
        assert_eq!(r.get(35, 22), Some(RED));
        assert_eq!(r.get(6, 26), Some(GREEN));
    }
}
