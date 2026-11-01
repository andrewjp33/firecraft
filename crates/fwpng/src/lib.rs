//! Fireworks PNG container: the PNG file Fireworks uses as its native document format.
//!
//! A Fireworks PNG is an ordinary PNG (its `IDAT` holds the flattened canvas, so any viewer
//! shows it) with private chunks carrying the editable document:
//!
//! | chunk  | contents                                                                 |
//! |--------|--------------------------------------------------------------------------|
//! | `mkBF` | 72-byte header, magic `FADECAFE`; preserved verbatim                      |
//! | `mkTS` | zlib-compressed tagged tree (see [`tree`]): the whole document            |
//! | `mkBS` | zlib-compressed tagged tree, a small secondary document; preserved        |
//! | `mkBT` | one pixel tile each (see [`tile`]); referenced from the tree by `TIDi`    |
//! | `prVW` | zlib-compressed preview, magic `CAFEBEEF`; preserved verbatim             |
//!
//! Everything here was derived from files written by Fireworks MX itself (black-box), see
//! `docs/fireworks-png.md`. Unknown tags and chunks are kept, so a read/write round trip is
//! lossless.

pub mod chunk;
pub mod file;
pub mod tile;
pub mod tree;

pub use chunk::Chunk;
pub use file::FwPng;
pub use tile::Tile;
pub use tree::{Kind, Node};

/// Errors from reading or writing a Fireworks PNG.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not a PNG file")]
    NotPng,
    #[error("truncated {0}")]
    Truncated(&'static str),
    #[error("bad CRC in chunk {0}")]
    BadCrc(String),
    #[error("malformed document tree at byte {offset}: {reason}")]
    Tree { offset: usize, reason: &'static str },
    #[error("malformed tile: {0}")]
    Tile(&'static str),
    #[error("decompressed data exceeds the {0}-byte limit")]
    TooLarge(usize),
    #[error("zlib: {0}")]
    Zlib(String),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Upper bound for any single inflated stream (document tree or tile). Fireworks tiles inflate
/// to 64 KiB; trees of large documents reach a few MiB. This guards hostile input.
pub const MAX_INFLATE: usize = 256 * 1024 * 1024;

pub(crate) fn inflate(data: &[u8], limit: usize) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut out = Vec::new();
    let mut dec = flate2::read::ZlibDecoder::new(data).take(limit as u64 + 1);
    dec.read_to_end(&mut out)
        .map_err(|e| Error::Zlib(e.to_string()))?;
    if out.len() > limit {
        return Err(Error::TooLarge(limit));
    }
    Ok(out)
}

pub(crate) fn deflate(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    // Writing into a Vec cannot fail; fall back to an empty stream rather than panicking.
    if enc.write_all(data).is_err() {
        return Vec::new();
    }
    enc.finish().unwrap_or_default()
}
