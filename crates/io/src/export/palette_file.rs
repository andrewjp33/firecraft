//! The Optimize panel's Load Palette and Save Palette: color tables as files.
//!
//! Fireworks MX saves palettes as `.act` files (Adobe Color Table, which Photoshop, Flash and
//! FreeHand read too) and loads `.act` files and the color table of a GIF. FireCraft also takes a
//! PNG 8's palette.
//!
//! An ACT file is 256 RGB triples (768 bytes), optionally followed by two big-endian 16-bit
//! numbers: how many of the colors are used, and the transparent one (0xFFFF: none).

use firecraft_raster::Rgba;

use crate::{Error, Result};

/// A loaded palette: its colors (opaque) and the transparent one, if it names one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteFile {
    pub colors: Vec<Rgba>,
    pub transparent: Option<usize>,
}

/// Read a color table from an ACT file, a GIF or a PNG 8.
pub fn read_palette(bytes: &[u8]) -> Result<PaletteFile> {
    if bytes.starts_with(b"GIF8") {
        return gif_palette(bytes);
    }
    if bytes.starts_with(b"\x89PNG") {
        return png_palette(bytes);
    }
    if bytes.len() == 768 || bytes.len() == 772 {
        return Ok(act_palette(bytes));
    }
    Err(Error::Unsupported("not a color table (.act), GIF or PNG 8 file".into()))
}

/// An ACT file of these colors (at most 256).
pub fn write_act(colors: &[Rgba], transparent: Option<usize>) -> Vec<u8> {
    let n = colors.len().min(256);
    let mut out = vec![0u8; 772];
    for (i, c) in colors.iter().take(n).enumerate() {
        out[i * 3..i * 3 + 3].copy_from_slice(&[c.r, c.g, c.b]);
    }
    out[768..770].copy_from_slice(&(n as u16).to_be_bytes());
    let t = transparent.filter(|&t| t < n).map_or(0xFFFF, |t| t as u16);
    out[770..772].copy_from_slice(&t.to_be_bytes());
    out
}

fn act_palette(bytes: &[u8]) -> PaletteFile {
    let (count, transparent) = match bytes.get(768..772) {
        Some(tail) => {
            let n = usize::from(u16::from_be_bytes([tail[0], tail[1]]));
            let t = usize::from(u16::from_be_bytes([tail[2], tail[3]]));
            (if n == 0 || n > 256 { 256 } else { n }, (t < 256).then_some(t))
        }
        None => (256, None),
    };
    let colors = bytes[..768]
        .chunks_exact(3)
        .take(count)
        .map(|c| Rgba::new(c[0], c[1], c[2], 255))
        .collect();
    PaletteFile {
        colors,
        transparent: transparent.filter(|&t| t < count),
    }
}

fn from_rgb(rgb: &[u8]) -> Vec<Rgba> {
    rgb.chunks_exact(3).map(|c| Rgba::new(c[0], c[1], c[2], 255)).collect()
}

/// A GIF's global color table, or its first frame's own.
fn gif_palette(bytes: &[u8]) -> Result<PaletteFile> {
    let bad = |e: gif::DecodingError| Error::Unsupported(format!("GIF: {e}"));
    let mut opts = gif::DecodeOptions::new();
    opts.set_color_output(gif::ColorOutput::Indexed);
    let mut dec = opts.read_info(std::io::Cursor::new(bytes)).map_err(bad)?;
    let global = dec.global_palette().map(<[u8]>::to_vec);
    let first = dec.read_next_frame().map_err(bad)?.map(|f| (f.palette.clone(), f.transparent));
    let (local, transparent) = first.unwrap_or((None, None));
    let rgb = local.or(global).ok_or_else(|| Error::Unsupported("the GIF has no color table".into()))?;
    let colors = from_rgb(&rgb);
    let transparent = transparent.map(usize::from).filter(|&t| t < colors.len());
    Ok(PaletteFile { colors, transparent })
}

/// A PNG 8's palette (the first entry with no opacity is the transparent one).
fn png_palette(bytes: &[u8]) -> Result<PaletteFile> {
    let dec = png::Decoder::new(std::io::Cursor::new(bytes));
    let reader = dec.read_info().map_err(|e| Error::Png(e.to_string()))?;
    let info = reader.info();
    let rgb = info
        .palette
        .as_ref()
        .ok_or_else(|| Error::Unsupported("the PNG has no palette (only PNG 8 files do)".into()))?;
    let colors = from_rgb(rgb);
    let transparent = info
        .trns
        .as_ref()
        .and_then(|t| t.iter().position(|&a| a == 0))
        .filter(|&t| t < colors.len());
    Ok(PaletteFile { colors, transparent })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn act_files_round_trip() {
        let colors = vec![Rgba::new(1, 2, 3, 255), Rgba::new(250, 128, 0, 255), Rgba::new(9, 9, 9, 255)];
        let bytes = write_act(&colors, Some(1));
        assert_eq!(bytes.len(), 772);
        let back = read_palette(&bytes).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(back.colors, colors);
        assert_eq!(back.transparent, Some(1));
        // A plain 768-byte table: 256 colors, none transparent.
        let plain = read_palette(&bytes[..768]).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(plain.colors.len(), 256);
        assert_eq!(plain.transparent, None);
        assert!(read_palette(b"hello").is_err());
    }

    #[test]
    fn a_gif_gives_its_color_table() {
        let mut out = Vec::new();
        {
            let rgb = [255, 0, 0, 0, 0, 255];
            let mut enc = gif::Encoder::new(&mut out, 2, 1, &rgb).unwrap_or_else(|e| unreachable!("{e}"));
            let frame = gif::Frame {
                width: 2,
                height: 1,
                buffer: std::borrow::Cow::Owned(vec![0, 1]),
                transparent: Some(1),
                ..gif::Frame::default()
            };
            enc.write_frame(&frame).unwrap_or_else(|e| unreachable!("{e}"));
        }
        let p = read_palette(&out).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(p.colors, vec![Rgba::new(255, 0, 0, 255), Rgba::new(0, 0, 255, 255)]);
        assert_eq!(p.transparent, Some(1));
    }
}
