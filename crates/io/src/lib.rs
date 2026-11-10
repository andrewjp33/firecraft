//! Reading and writing FireCraft documents.
//!
//! * [`open`] accepts a Fireworks PNG (editable layers) or a flat image: PNG, JPEG, GIF (animated
//!   GIFs become frames), BMP, TIFF, Targa, WebP or JPEG XL ([`formats`]).
//! * [`save_fireworks_png`] writes a Fireworks-compatible PNG: a normal flattened PNG that every
//!   viewer shows, with the editable document in Fireworks' private chunks.
//! * [`export_png`] writes a flat PNG.

mod clip;
pub mod export;
mod flat;
pub mod formats;
mod read;
pub mod textures;
mod write;

pub use clip::{clipboard_png, read_clipboard_png};
pub use flat::{decode_png, encode_png};
pub use read::document_from_fireworks;
pub use write::fireworks_tree;

use firecraft_doc::Document;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Format(#[from] firecraft_fwpng::Error),
    #[error("PNG: {0}")]
    Png(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Decode(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Open a file. Fireworks PNGs become editable documents; flat images open as Fireworks MX opens
/// them (a bitmap on a Background layer; animations as frames). `name` is the file's name or path
/// (it also identifies formats without a signature, such as Targa).
pub fn open(bytes: &[u8], name: Option<String>) -> Result<Document> {
    if formats::sniff(bytes, name.as_deref()) == Some(formats::Format::Png) {
        let fw = firecraft_fwpng::FwPng::read(bytes)?;
        if fw.is_fireworks() {
            let mut doc = document_from_fireworks(&fw)?;
            doc.path = name;
            return Ok(doc);
        }
    }
    formats::open_flat(bytes, name)
}

/// Whether a document opened from `path` can be saved back to it as a Fireworks PNG. Other
/// formats (a JPEG, a GIF...) get Save As with a `.png` name, as in Fireworks, so the original
/// file is never overwritten with a different format.
pub fn saves_in_place(path: &str) -> bool {
    path.to_ascii_lowercase().ends_with(".png")
}

/// `path`'s file name with its extension replaced by `.png` (the name Save As suggests).
pub fn png_name(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    format!("{stem}.png")
}

/// Save as Fireworks PNG.
pub fn save_fireworks_png(doc: &Document) -> Result<Vec<u8>> {
    let flat = encode_png(&doc.render(0), doc.resolution)?;
    let (tree, tiles) = fireworks_tree(doc);
    let fw = firecraft_fwpng::FwPng::assemble(&flat, &tree, doc.raw_secondary.as_ref(), &tiles, &[])?;
    Ok(fw.to_bytes())
}

/// Export the current frame as a flat 32-bit PNG.
pub fn export_png(doc: &Document) -> Result<Vec<u8>> {
    encode_png(&doc.render(doc.current_frame), doc.resolution)
}
