//! Reading and writing FireCraft documents.
//!
//! * [`open`] accepts a Fireworks PNG (editable layers) or any flat PNG (one bitmap).
//! * [`save_fireworks_png`] writes a Fireworks-compatible PNG: a normal flattened PNG that every
//!   viewer shows, with the editable document in Fireworks' private chunks.
//! * [`export_png`] writes a flat PNG.

mod flat;
mod read;
mod write;

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
    #[error("unsupported: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Open a PNG. Fireworks PNGs become editable documents; other PNGs become a single bitmap.
pub fn open(bytes: &[u8], name: Option<String>) -> Result<Document> {
    let fw = firecraft_fwpng::FwPng::read(bytes)?;
    if fw.is_fireworks() {
        let mut doc = document_from_fireworks(&fw)?;
        doc.path = name;
        return Ok(doc);
    }
    let img = decode_png(bytes)?;
    Ok(Document::from_image(img, name))
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
