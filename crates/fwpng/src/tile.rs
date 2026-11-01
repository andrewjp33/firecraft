//! `mkBT` pixel tiles.
//!
//! Layout (observed):
//!
//! ```text
//! u32be  magic   0xFACECAFE
//! u32be  id      referenced by TIDi{..} in the tree
//! u32be  flags   0 or 1; meaning unknown, preserved
//! [u8;64]        reserved, zero in every observed file
//! ...            zlib stream
//! ```
//!
//! Image tiles (`IMGv`, `TSZi{80}`) inflate to 128 x 128 pixels of 8-bit A,R,G,B (straight,
//! not premultiplied; verified against known pixel values). Mask tiles (`MSKv`, `TSZi{100}`)
//! inflate to 256 x 256 single-byte coverage values. Tiles are laid out row-major over the
//! element's `WPXi` x `HPXi` pixel area, in the order their ids are listed in `TILv`.

use crate::{Error, Result};

pub const TILE_MAGIC: u32 = 0xFACE_CAFE;
pub const HEADER_LEN: usize = 76;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tile {
    pub id: u32,
    pub flags: u32,
    pub reserved: [u8; 64],
    /// Inflated tile payload.
    pub data: Vec<u8>,
}

impl Tile {
    pub fn new(id: u32, data: Vec<u8>) -> Self {
        Self {
            id,
            flags: 0,
            reserved: [0; 64],
            data,
        }
    }

    pub fn decode(chunk: &[u8]) -> Result<Tile> {
        let word = |at: usize| -> Result<u32> {
            let s = chunk.get(at..at + 4).ok_or(Error::Tile("truncated header"))?;
            Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
        };
        if word(0)? != TILE_MAGIC {
            return Err(Error::Tile("bad magic"));
        }
        let reserved: [u8; 64] = chunk
            .get(12..HEADER_LEN)
            .and_then(|s| s.try_into().ok())
            .ok_or(Error::Tile("truncated header"))?;
        let body = chunk.get(HEADER_LEN..).ok_or(Error::Tile("missing payload"))?;
        // A 256 x 256 x 4 tile is the largest observed layout; allow that much.
        let data = crate::inflate(body, 256 * 256 * 4)?;
        Ok(Tile {
            id: word(4)?,
            flags: word(8)?,
            reserved,
            data,
        })
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + self.data.len() / 4);
        out.extend_from_slice(&TILE_MAGIC.to_be_bytes());
        out.extend_from_slice(&self.id.to_be_bytes());
        out.extend_from_slice(&self.flags.to_be_bytes());
        out.extend_from_slice(&self.reserved);
        out.extend_from_slice(&crate::deflate(&self.data));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut t = Tile::new(0x02c4_95b0, vec![0xAB; 128 * 128 * 4]);
        t.flags = 1;
        let back = Tile::decode(&t.encode()).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn rejects_bad_tiles() {
        assert!(Tile::decode(&[]).is_err());
        assert!(Tile::decode(&[0; 80]).is_err());
        let mut t = Tile::new(1, vec![0; 16]).encode();
        t.truncate(HEADER_LEN + 3);
        assert!(Tile::decode(&t).is_err());
    }
}
