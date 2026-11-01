//! A whole Fireworks PNG: ordinary PNG chunks plus the private document chunks.

use std::collections::HashMap;

use crate::chunk::{read_chunks, write_chunks, Chunk};
use crate::tile::Tile;
use crate::tree::Node;
use crate::{inflate, deflate, Result, MAX_INFLATE};

/// The 72-byte `mkBF` header Fireworks MX writes (magic `FADECAFE`, then `00000004`, then zeros).
pub fn default_mkbf() -> Vec<u8> {
    let mut v = vec![0u8; 72];
    v[..8].copy_from_slice(&[0xFA, 0xDE, 0xCA, 0xFE, 0, 0, 0, 4]);
    v
}

/// A parsed Fireworks PNG. Chunks are kept in file order so unknown ones survive a re-save.
#[derive(Clone, Debug, Default)]
pub struct FwPng {
    pub chunks: Vec<Chunk>,
}

impl FwPng {
    pub fn read(bytes: &[u8]) -> Result<Self> {
        Ok(Self { chunks: read_chunks(bytes)? })
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        write_chunks(&self.chunks)
    }

    /// True if the file carries an editable Fireworks document (an `mkTS` chunk).
    pub fn is_fireworks(&self) -> bool {
        self.chunks.iter().any(|c| c.is(b"mkTS"))
    }

    fn chunk(&self, kind: &[u8; 4]) -> Option<&Chunk> {
        self.chunks.iter().find(|c| c.is(kind))
    }

    /// The main document tree (`mkTS`).
    pub fn tree(&self) -> Result<Option<Node>> {
        self.parse_tree(b"mkTS")
    }

    /// The secondary tree (`mkBS`), when present.
    pub fn secondary_tree(&self) -> Result<Option<Node>> {
        self.parse_tree(b"mkBS")
    }

    fn parse_tree(&self, kind: &[u8; 4]) -> Result<Option<Node>> {
        let Some(c) = self.chunk(kind) else { return Ok(None) };
        let raw = inflate(&c.data, MAX_INFLATE)?;
        Ok(Some(Node::parse(&raw)?))
    }

    /// All pixel tiles by id. Undecodable tiles are reported as an error.
    pub fn tiles(&self) -> Result<HashMap<u32, Tile>> {
        let mut map = HashMap::new();
        for c in self.chunks.iter().filter(|c| c.is(b"mkBT")) {
            let t = Tile::decode(&c.data)?;
            map.insert(t.id, t);
        }
        Ok(map)
    }

    /// Build a Fireworks PNG from a flattened image (a complete PNG, as written by any encoder)
    /// plus the document. Chunk order follows Fireworks MX: header chunks, `mkBF`, `mkTS`,
    /// optional `mkBS`, every `mkBT`, then the image data and `IEND`.
    pub fn assemble(
        flat_png: &[u8],
        tree: &Node,
        secondary: Option<&Node>,
        tiles: &[Tile],
        extra: &[Chunk],
    ) -> Result<Self> {
        let flat = read_chunks(flat_png)?;
        let mut chunks = Vec::with_capacity(flat.len() + tiles.len() + 4);
        let (head, tail): (Vec<_>, Vec<_>) =
            flat.into_iter().partition(|c| !c.is(b"IDAT") && !c.is(b"IEND"));
        chunks.extend(head);
        chunks.extend(extra.iter().cloned());
        chunks.push(Chunk::new(b"mkBF", default_mkbf()));
        chunks.push(Chunk::new(b"mkTS", deflate(&tree.to_bytes())));
        if let Some(s) = secondary {
            chunks.push(Chunk::new(b"mkBS", deflate(&s.to_bytes())));
        }
        chunks.extend(tiles.iter().map(|t| Chunk::new(b"mkBT", t.encode())));
        chunks.extend(tail);
        Ok(Self { chunks })
    }
}
