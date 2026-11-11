//! Export a file with chosen settings: export_file <in> <out> <preset name | key=value ...>
//! Keys: format=gif|agif|jpeg|png8|png24|png32|webp|jxl palette=adaptive|websnap|web216|exact|
//! windows|mac|gray|bw|uniform colors=N dither=N quality=N alpha=N lossless=0|1 transparency=none|index|alpha
use firecraft_doc::{ExportFormat, ExportSettings, PaletteKind, Transparency};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let [input, out, rest @ ..] = a.as_slice() else {
        return Err("usage: export_file <in> <out> [settings]".into());
    };
    let doc = firecraft_io::open(&std::fs::read(input)?, Some(input.clone()))?;
    let mut s = ExportSettings::default();
    for kv in rest {
        let (k, v) = kv.split_once('=').ok_or("key=value")?;
        let n = || v.parse::<u16>().unwrap_or(0);
        match k {
            "format" => {
                s.format = match v {
                    "agif" => ExportFormat::AnimatedGif,
                    "jpeg" => ExportFormat::Jpeg,
                    "png8" => ExportFormat::Png8,
                    "png24" => ExportFormat::Png24,
                    "png32" => ExportFormat::Png32,
                    "webp" => ExportFormat::WebP,
                    "jxl" => ExportFormat::JpegXl,
                    _ => ExportFormat::Gif,
                }
            }
            "palette" => {
                s.palette = match v {
                    "websnap" => PaletteKind::WebAdaptive,
                    "web216" => PaletteKind::Web216,
                    "exact" => PaletteKind::Exact,
                    "windows" => PaletteKind::Windows,
                    "mac" => PaletteKind::Macintosh,
                    "gray" => PaletteKind::Grayscale,
                    "bw" => PaletteKind::BlackWhite,
                    "uniform" => PaletteKind::Uniform,
                    _ => PaletteKind::Adaptive,
                }
            }
            "colors" => s.colors = n(),
            "dither" => s.dither = n() as u8,
            "quality" => {
                s.quality = n() as u8;
                s.jpeg_quality = n() as u8;
            }
            "alpha" => s.alpha_quality = n() as u8,
            "lossless" => s.lossless = v == "1",
            "transparency" => {
                s.transparency = match v {
                    "index" => Transparency::Index,
                    "alpha" => Transparency::Alpha,
                    _ => Transparency::None,
                }
            }
            _ => return Err(format!("unknown key {k}").into()),
        }
    }
    let e = firecraft_io::export::export(&doc, &s)?;
    std::fs::write(out, &e.bytes)?;
    println!("{} bytes, {} palette colors", e.bytes.len(), e.palette.len());
    Ok(())
}
