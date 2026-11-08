//! Textures for strokes: Fireworks' texture folders, read at run time from an installed copy
//! (nothing of Fireworks is bundled), a few original built-in textures, and image files.
//!
//! A texture is an 8-bit mask. From an image it is the inverse of the brightness: Fireworks MX
//! embeds `255 - grey` of its texture file in the documents it saves (all 16384 pixels of
//! "Grain" match; docs/behavior/strokes.md). Fireworks draws a stroke from the mask stored in
//! the file, not from its folder (verified with tools/fwmx-oracle), so textures from anywhere
//! show the same in Fireworks.

use std::path::{Path, PathBuf};

use firecraft_doc::Texture;
use firecraft_raster::{Rgba, RgbaImage};

use crate::{Error, Result};

/// The image extensions read as textures (Fireworks' own are PNG and GIF).
pub const EXTENSIONS: [&str; 2] = ["png", "gif"];

/// Built-in textures (drawn in code; FireCraft's own, for machines without Fireworks).
pub const BUILTIN: [&str; 3] = ["FireCraft Noise", "FireCraft Weave", "FireCraft Diagonal"];

/// The texture new strokes get, as in Fireworks, when its folder is available.
pub const FIREWORKS_DEFAULT: &str = "Grain";

/// A texture's mask from an image: `255 - grey`, transparent pixels counting as white.
pub fn mask_from_image(img: &RgbaImage) -> Vec<u8> {
    img.pixels()
        .iter()
        .map(|p| {
            let grey = (p.r as u32 * 299 + p.g as u32 * 587 + p.b as u32 * 114) / 1000;
            let over_white = (grey * p.a as u32 + 255 * (255 - p.a as u32)) / 255;
            (255 - over_white.min(255)) as u8
        })
        .collect()
}

/// Read an image file (PNG or GIF; JPEG, BMP and TIFF for patterns).
pub fn decode_image(bytes: &[u8]) -> Result<RgbaImage> {
    if bytes.starts_with(b"GIF8") {
        decode_gif(bytes)
    } else if bytes.starts_with(b"\x89PNG") {
        crate::decode_png(bytes)
    } else {
        decode_other(bytes)
    }
}

fn decode_other(bytes: &[u8]) -> Result<RgbaImage> {
    let decoded = image::load_from_memory(bytes)
        .map_err(|e| crate::Error::Unsupported(e.to_string()))?
        .into_rgba8();
    let (w, h) = decoded.dimensions();
    RgbaImage::from_rgba_bytes(w, h, decoded.as_raw())
        .ok_or_else(|| crate::Error::Unsupported("bad image dimensions".into()))
}

/// A texture from an image file, named after the file.
pub fn texture_from_file(path: &Path) -> Option<Texture> {
    let name = path.file_stem()?.to_string_lossy().to_string();
    texture_from_bytes(name, &std::fs::read(path).ok()?)
}

/// A texture from the contents of an image file.
fn texture_from_bytes(name: String, bytes: &[u8]) -> Option<Texture> {
    let img = decode_image(bytes).ok()?;
    Texture::new(name, img.width(), img.height(), mask_from_image(&img))
}

/// The texture name for an image file's path; `None` if it isn't a texture image.
fn texture_name(path: &Path) -> Option<String> {
    let ext = path.extension()?;
    if !EXTENSIONS.iter().any(|e| ext.eq_ignore_ascii_case(e)) {
        return None;
    }
    Some(path.file_stem()?.to_string_lossy().to_string())
}

/// A built-in texture by name.
pub fn builtin(name: &str) -> Option<Texture> {
    // A small integer hash (deterministic noise).
    let hash = |x: u32, y: u32, s: u32| {
        let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77) ^ s.wrapping_mul(0xC2B2_AE3D);
        h ^= h >> 15;
        h = h.wrapping_mul(0x2C1B_3C6D);
        h ^= h >> 12;
        h & 0xFF
    };
    let (size, f): (u32, Box<dyn Fn(u32, u32) -> u32>) = match name {
        // Fine grain: noise, lightly smoothed.
        "FireCraft Noise" => (
            128,
            Box::new(move |x, y| {
                let n = |dx: u32, dy: u32| hash((x + dx) % 128, (y + dy) % 128, 7);
                (n(0, 0) * 2 + n(1, 0) + n(0, 1)) / 4
            }),
        ),
        // Woven threads: alternating horizontal and vertical bands with fibre noise.
        "FireCraft Weave" => (
            32,
            Box::new(move |x, y| {
                let (cx, cy) = (x / 8, y / 8);
                let across = (cx + cy) % 2 == 0;
                let t = if across { y % 8 } else { x % 8 };
                let thread = 220u32.saturating_sub(((t as i32 - 4).unsigned_abs()) * 40);
                (thread + hash(x, y, 3) / 8).min(255)
            }),
        ),
        // Diagonal lines, 6 px apart.
        "FireCraft Diagonal" => (
            24,
            Box::new(|x, y| {
                let d = (x + y) % 6;
                [255, 160, 30, 0, 0, 30][d as usize]
            }),
        ),
        _ => return None,
    };
    let mask = (0..size * size)
        .map(|i| f(i % size, i / size).min(255) as u8)
        .collect();
    Texture::new(name, size, size, mask)
}

/// The file types read as patterns (as Fireworks MX's manual lists them, less Mac PICT).
pub const PATTERN_EXTENSIONS: [&str; 7] = ["png", "gif", "jpg", "jpeg", "bmp", "tif", "tiff"];

/// A pattern for machines without Fireworks: FireCraft's own, drawn in code.
pub const BUILTIN_PATTERN: &str = "FireCraft Checks";

fn builtin_pattern() -> Option<RgbaImage> {
    let (light, dark) = (Rgba::new(0xd8, 0xd8, 0xd8, 255), Rgba::new(0x98, 0x98, 0x98, 255));
    let px = (0..32u32 * 32)
        .map(|i| {
            if (i % 32 / 16 + i / 32 / 16) % 2 == 0 {
                light
            } else {
                dark
            }
        })
        .collect();
    RgbaImage::from_pixels(32, 32, px)
}

#[derive(Clone, Debug)]
enum PatternSource {
    File(PathBuf),
    Bytes(Vec<u8>),
    /// Chosen with Other...: kept when the folders are read again.
    Added(Vec<u8>),
    Builtin,
}

/// Fireworks MX's menu names where they differ from the file's: (file, menu).
const PATTERN_LABELS: [(&str, &str); 3] = [
    ("Berber Rug", "Berber"),
    ("Illusion 1", "Illusion"),
    ("Impressionist-Red", "Impressionist"),
];

/// A pattern's name on the menu.
pub fn pattern_label(file_stem: &str) -> &str {
    PATTERN_LABELS
        .iter()
        .find(|(file, _)| *file == file_stem)
        .map_or(file_stem, |(_, label)| label)
}

fn pattern_stem(file: &Path) -> Option<String> {
    let ext = file.extension()?;
    PATTERN_EXTENSIONS
        .iter()
        .any(|e| ext.eq_ignore_ascii_case(e))
        .then(|| file.file_stem().map(|n| n.to_string_lossy().to_string()))?
}

/// The patterns offered: Fireworks' Patterns folder, FireCraft's own and those added.
#[derive(Clone, Debug, Default)]
pub struct PatternLibrary {
    /// (file stem, where it is), in menu order.
    entries: Vec<(String, PatternSource)>,
}

impl PatternLibrary {
    /// From the files of a folder already read (the browser), else `configured` or the usual
    /// install folders. By menu name, as Fireworks lists them, then FireCraft's own.
    pub fn scan(configured: Option<&Path>, files: Vec<(String, Vec<u8>)>) -> Self {
        let mut entries: Vec<(String, PatternSource)> = files
            .into_iter()
            .filter_map(|(file, b)| Some((pattern_stem(Path::new(&file))?, PatternSource::Bytes(b))))
            .collect();
        if entries.is_empty() {
            for dir in config_dirs(configured, "Patterns") {
                let Ok(rd) = std::fs::read_dir(&dir) else {
                    continue;
                };
                entries.extend(
                    rd.flatten()
                        .map(|e| e.path())
                        .filter_map(|p| Some((pattern_stem(&p)?, PatternSource::File(p)))),
                );
                if !entries.is_empty() {
                    break;
                }
            }
        }
        entries.sort_by_key(|(n, _)| pattern_label(n).to_lowercase());
        entries.push((BUILTIN_PATTERN.to_owned(), PatternSource::Builtin));
        Self { entries }
    }

    /// The menu's names.
    pub fn names(&self) -> Vec<String> {
        self.entries
            .iter()
            .map(|(n, _)| pattern_label(n).to_owned())
            .collect()
    }

    /// The pattern with this menu name: its file stem (the name stored in documents) and image.
    pub fn get(&self, label: &str) -> Option<(String, RgbaImage)> {
        let (stem, source) = self.entries.iter().find(|(n, _)| pattern_label(n) == label)?;
        let image = match source {
            PatternSource::File(p) => decode_image(&std::fs::read(p).ok()?).ok(),
            PatternSource::Bytes(b) | PatternSource::Added(b) => decode_image(b).ok(),
            PatternSource::Builtin => builtin_pattern(),
        }?;
        Some((stem.clone(), image))
    }

    /// The first pattern ("Berber Rug" with Fireworks' folder).
    pub fn first(&self) -> Option<(String, RgbaImage)> {
        let (name, _) = self.entries.first()?;
        self.get(pattern_label(name))
    }

    /// Add an image file (Other...) at the bottom of the list: its stem and image.
    pub fn add(&mut self, file: &str, bytes: Vec<u8>) -> std::result::Result<(String, RgbaImage), String> {
        let image = decode_image(&bytes).map_err(|e| format!("Could not read {file}: {e}"))?;
        let stem = Path::new(file)
            .file_stem()
            .map_or_else(|| file.to_owned(), |n| n.to_string_lossy().to_string());
        self.entries.retain(|(n, _)| *n != stem);
        self.entries.push((stem.clone(), PatternSource::Added(bytes)));
        Ok((stem, image))
    }

    /// Keep the patterns added to `old`, after reading the folders again.
    pub fn keep_added(&mut self, old: Self) {
        self.entries.extend(
            old.entries
                .into_iter()
                .filter(|(_, s)| matches!(s, PatternSource::Added(_))),
        );
    }
}

/// Fireworks texture folders: in `configured` (a Fireworks install folder, or a folder of
/// textures), `FIRECRAFT_FWMX_DIR`, and the usual install locations (also under Wine).
pub fn fireworks_dirs(configured: Option<&Path>) -> Vec<PathBuf> {
    config_dirs(configured, "Textures")
        .into_iter()
        .filter(|d| !files_in(d).is_empty())
        .collect()
}

/// The folders named `sub` of the Fireworks installs found, or `configured` itself.
fn config_dirs(configured: Option<&Path>, sub: &str) -> Vec<PathBuf> {
    let mut installs: Vec<PathBuf> = Vec::new();
    if let Some(c) = configured {
        installs.push(c.to_path_buf());
    }
    if let Some(d) = std::env::var_os("FIRECRAFT_FWMX_DIR") {
        installs.push(PathBuf::from(d));
    }
    let mut roots: Vec<PathBuf> = ["C:\\Program Files", "C:\\Program Files (x86)"]
        .iter()
        .map(PathBuf::from)
        .collect();
    if let Some(home) = std::env::var_os("HOME") {
        let wine = PathBuf::from(home).join(".wine").join("drive_c");
        roots.push(wine.join("Program Files"));
        roots.push(wine.join("Program Files (x86)"));
    }
    for r in &roots {
        installs.push(r.join("Macromedia").join("Fireworks MX"));
        installs.push(r.join("Macromedia").join("Fireworks MX 2004"));
        installs.push(r.join("Macromedia").join("Fireworks 8"));
        for v in ["CS6", "CS5.1", "CS5", "CS4", "CS3"] {
            installs.push(r.join("Adobe").join(format!("Adobe Fireworks {v}")));
        }
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for i in installs {
        let t = i.join("Configuration").join(sub);
        let dir = if t.is_dir() { t } else { i };
        if dir.is_dir() && !out.contains(&dir) {
            out.push(dir);
        }
    }
    out
}

/// The texture images in `dir` (name, path), by name.
pub fn files_in(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<(String, PathBuf)> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .filter_map(|p| Some((texture_name(&p)?, p)))
        .collect();
    v.sort_by_key(|(n, _)| n.to_lowercase());
    v
}

#[derive(Clone, Debug)]
enum Source {
    File(PathBuf),
    /// An image file's contents (a folder the browser read for us).
    Image(Vec<u8>),
    Builtin,
    Loaded(Texture),
}

/// The textures FireCraft offers: Fireworks' folders, the built-in ones, and those added
/// (from documents or picked files). Files are read when first used.
#[derive(Clone, Debug, Default)]
pub struct TextureLibrary {
    entries: Vec<(String, Source)>,
}

impl TextureLibrary {
    /// Fireworks' textures (from `files`, the (file name, contents) of a folder already read,
    /// then `dirs`; the first of a name wins) and the built-in ones.
    pub fn scan(dirs: &[PathBuf], files: Vec<(String, Vec<u8>)>) -> TextureLibrary {
        let mut lib = TextureLibrary::default();
        let mut files: Vec<(String, Vec<u8>)> = files
            .into_iter()
            .filter_map(|(file, bytes)| Some((texture_name(Path::new(&file))?, bytes)))
            .collect();
        files.sort_by_key(|(n, _)| n.to_lowercase());
        for (name, bytes) in files {
            if !lib.contains(&name) {
                lib.entries.push((name, Source::Image(bytes)));
            }
        }
        for d in dirs {
            for (name, path) in files_in(d) {
                if !lib.contains(&name) {
                    lib.entries.push((name, Source::File(path)));
                }
            }
        }
        for b in BUILTIN {
            lib.entries.push((b.to_owned(), Source::Builtin));
        }
        lib
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.iter().any(|(n, _)| n.eq_ignore_ascii_case(name))
    }

    /// The names, in list order.
    pub fn names(&self) -> Vec<String> {
        self.entries.iter().map(|(n, _)| n.clone()).collect()
    }

    /// Whether Fireworks' own textures were found.
    pub fn has_fireworks(&self) -> bool {
        self.entries
            .iter()
            .any(|(_, s)| matches!(s, Source::File(_) | Source::Image(_)))
    }

    /// Add a texture (one from a document or a picked file), replacing one of the same name
    /// that isn't loaded from Fireworks' folder.
    pub fn add(&mut self, t: Texture) {
        match self
            .entries
            .iter_mut()
            .find(|(n, _)| n.eq_ignore_ascii_case(&t.name))
        {
            Some((_, s @ Source::Loaded(_))) => *s = Source::Loaded(t),
            Some(_) => {}
            None => self.entries.push((t.name.clone(), Source::Loaded(t))),
        }
    }

    /// The textures added from documents or files (kept when the list is read again).
    pub fn loaded(self) -> Vec<Texture> {
        self.entries
            .into_iter()
            .filter_map(|(_, s)| match s {
                Source::Loaded(t) if !BUILTIN.contains(&t.name.as_str()) => Some(t),
                _ => None,
            })
            .collect()
    }

    /// The texture called `name` (read on first use).
    pub fn get(&mut self, name: &str) -> Option<Texture> {
        let (_, src) = self
            .entries
            .iter_mut()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))?;
        let t = match src {
            Source::Loaded(t) => return Some(t.clone()),
            Source::File(p) => texture_from_file(p)?,
            Source::Image(bytes) => texture_from_bytes(name.to_owned(), bytes)?,
            Source::Builtin => builtin(name)?,
        };
        *src = Source::Loaded(t.clone());
        Some(t)
    }

    /// The texture new strokes get: Fireworks' "Grain" when available, else the first.
    pub fn default_texture(&mut self) -> Option<Texture> {
        self.get(FIREWORKS_DEFAULT).or_else(|| {
            self.entries
                .first()
                .map(|e| e.0.clone())
                .and_then(|n| self.get(&n))
        })
    }
}

/// The first frame of a GIF (Fireworks' line and grid textures are GIFs).
pub fn decode_gif(bytes: &[u8]) -> Result<RgbaImage> {
    let bad = |m: &str| Error::Unsupported(format!("GIF: {m}"));
    let b = |i: usize| bytes.get(i).copied().ok_or_else(|| bad("truncated"));
    let word = |i: usize| -> Result<u32> { Ok(b(i)? as u32 | (b(i + 1)? as u32) << 8) };
    if !bytes.starts_with(b"GIF8") {
        return Err(bad("not a GIF"));
    }
    let (w, h) = (word(6)?, word(8)?);
    if w == 0 || h == 0 || w as u64 * h as u64 > 1 << 26 {
        return Err(bad("bad size"));
    }
    let packed = b(10)?;
    let mut at = 13;
    let table = |at: usize, n: usize| -> Result<Vec<[u8; 3]>> {
        (0..n)
            .map(|k| Ok([b(at + k * 3)?, b(at + k * 3 + 1)?, b(at + k * 3 + 2)?]))
            .collect()
    };
    let mut global = Vec::new();
    if packed & 0x80 != 0 {
        let n = 2usize << (packed & 7);
        global = table(at, n)?;
        at += n * 3;
    }
    let mut transparent: Option<u8> = None;
    let sub_blocks = |mut at: usize, out: &mut Vec<u8>| -> Result<usize> {
        loop {
            let len = b(at)? as usize;
            at += 1;
            if len == 0 {
                return Ok(at);
            }
            out.extend_from_slice(bytes.get(at..at + len).ok_or_else(|| bad("truncated"))?);
            at += len;
        }
    };
    loop {
        match b(at)? {
            0x21 => {
                let label = b(at + 1)?;
                let mut data = Vec::new();
                let next = sub_blocks(at + 2, &mut data)?;
                if label == 0xF9 && data.len() >= 4 && data[0] & 1 != 0 {
                    transparent = Some(data[3]);
                }
                at = next;
            }
            0x2C => {
                let (x0, y0, fw, fh) = (word(at + 1)?, word(at + 3)?, word(at + 5)?, word(at + 7)?);
                let p = b(at + 9)?;
                at += 10;
                let mut colors = global.clone();
                if p & 0x80 != 0 {
                    let n = 2usize << (p & 7);
                    colors = table(at, n)?;
                    at += n * 3;
                }
                let min_code = b(at)?;
                let mut data = Vec::new();
                sub_blocks(at + 1, &mut data)?;
                let indices = weezl::decode::Decoder::new(weezl::BitOrder::Lsb, min_code.clamp(2, 11))
                    .decode(&data)
                    .or_else(|e| {
                        // Many encoders end without the end code; weezl reports what it decoded.
                        if matches!(e, weezl::LzwError::InvalidCode) {
                            Err(bad("bad LZW data"))
                        } else {
                            Ok(Vec::new())
                        }
                    })?;
                let mut img = RgbaImage::new(w, h, firecraft_raster::Rgba::TRANSPARENT);
                // Interlaced rows come in four passes.
                let rows: Vec<u32> = if p & 0x40 != 0 {
                    [(0, 8), (4, 8), (2, 4), (1, 2)]
                        .iter()
                        .flat_map(|&(s, step)| (s..fh).step_by(step))
                        .collect()
                } else {
                    (0..fh).collect()
                };
                for (k, idx) in indices.iter().enumerate().take((fw * fh) as usize) {
                    let (col, row) = (k as u32 % fw, rows.get(k / fw as usize).copied().unwrap_or(0));
                    if Some(*idx) == transparent {
                        continue;
                    }
                    if let Some(c) = colors.get(*idx as usize) {
                        img.set(
                            (x0 + col) as i64,
                            (y0 + row) as i64,
                            firecraft_raster::Rgba::new(c[0], c[1], c[2], 255),
                        );
                    }
                }
                return Ok(img);
            }
            _ => return Err(bad("no image")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_is_inverse_brightness() {
        let mut img = RgbaImage::new(3, 1, firecraft_raster::Rgba::TRANSPARENT);
        img.set(0, 0, firecraft_raster::Rgba::new(148, 148, 148, 255));
        img.set(1, 0, firecraft_raster::Rgba::new(33, 33, 33, 255));
        // Fireworks' Grain: 148 -> 107, 33 -> 222; transparent counts as white.
        assert_eq!(mask_from_image(&img), vec![107, 222, 0]);
    }

    #[test]
    fn builtins_exist() {
        for n in BUILTIN {
            let t = builtin(n).unwrap_or_else(|| panic!("{n}"));
            assert_eq!(t.mask.len(), (t.width * t.height) as usize);
        }
    }

    #[test]
    fn decodes_a_small_gif() {
        // 2 x 1, black and white, from a standard encoder (GIF89a, global table of 2 colours).
        let gif: [u8; 35] = [
            0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x02, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00,
            0xFF, 0xFF, 0xFF, 0x2C, 0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x01, 0x00, 0x00, 0x02, 0x02, 0x44,
            0x0A, 0x00, 0x3B,
        ];
        let img = decode_gif(&gif).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(img.get(0, 0).map(|p| p.r), Some(0));
        assert_eq!(img.get(1, 0).map(|p| p.r), Some(255));
    }
}
