//! Raw PNG chunk framing.

use crate::{Error, Result};

pub const PNG_SIGNATURE: [u8; 8] = [0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n'];

/// One PNG chunk, kept as raw bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub kind: [u8; 4],
    pub data: Vec<u8>,
}

impl Chunk {
    pub fn new(kind: &[u8; 4], data: Vec<u8>) -> Self {
        Self { kind: *kind, data }
    }

    pub fn is(&self, kind: &[u8; 4]) -> bool {
        &self.kind == kind
    }

    pub fn kind_str(&self) -> String {
        String::from_utf8_lossy(&self.kind).into_owned()
    }
}

fn be32(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
}

/// Split a PNG file into chunks. CRCs are verified.
pub fn read_chunks(bytes: &[u8]) -> Result<Vec<Chunk>> {
    if bytes.get(..8) != Some(&PNG_SIGNATURE[..]) {
        return Err(Error::NotPng);
    }
    let mut chunks = Vec::new();
    let mut pos = 8usize;
    while pos < bytes.len() {
        let len = be32(bytes, pos).ok_or(Error::Truncated("chunk length"))? as usize;
        let kind_at = pos + 4;
        let data_at = kind_at + 4;
        let end = data_at.checked_add(len).ok_or(Error::Truncated("chunk"))?;
        let kind: [u8; 4] = bytes
            .get(kind_at..data_at)
            .and_then(|s| s.try_into().ok())
            .ok_or(Error::Truncated("chunk type"))?;
        let data = bytes.get(data_at..end).ok_or(Error::Truncated("chunk data"))?;
        let crc = be32(bytes, end).ok_or(Error::Truncated("chunk crc"))?;
        let mut h = crc32fast_like::Hasher::new();
        h.update(&kind);
        h.update(data);
        if h.finalize() != crc {
            return Err(Error::BadCrc(String::from_utf8_lossy(&kind).into_owned()));
        }
        chunks.push(Chunk {
            kind,
            data: data.to_vec(),
        });
        pos = end + 4;
        if &kind == b"IEND" {
            break;
        }
    }
    Ok(chunks)
}

/// Serialize chunks into a PNG file.
pub fn write_chunks(chunks: &[Chunk]) -> Vec<u8> {
    let total: usize = chunks.iter().map(|c| c.data.len() + 12).sum();
    let mut out = Vec::with_capacity(8 + total);
    out.extend_from_slice(&PNG_SIGNATURE);
    for c in chunks {
        let len = u32::try_from(c.data.len()).unwrap_or(u32::MAX);
        out.extend_from_slice(&len.to_be_bytes());
        out.extend_from_slice(&c.kind);
        out.extend_from_slice(&c.data);
        let mut h = crc32fast_like::Hasher::new();
        h.update(&c.kind);
        h.update(&c.data);
        out.extend_from_slice(&h.finalize().to_be_bytes());
    }
    out
}

/// Small table-driven CRC-32 (IEEE), as used by PNG.
mod crc32fast_like {
    const fn table() -> [u32; 256] {
        let mut t = [0u32; 256];
        let mut n = 0;
        while n < 256 {
            let mut c = n as u32;
            let mut k = 0;
            while k < 8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
                k += 1;
            }
            t[n] = c;
            n += 1;
        }
        t
    }
    static TABLE: [u32; 256] = table();

    pub struct Hasher(u32);
    impl Hasher {
        pub fn new() -> Self {
            Self(0xFFFF_FFFF)
        }
        pub fn update(&mut self, data: &[u8]) {
            let mut c = self.0;
            for &b in data {
                c = TABLE[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
            }
            self.0 = c;
        }
        pub fn finalize(&self) -> u32 {
            self.0 ^ 0xFFFF_FFFF
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_crc() {
        let chunks = vec![Chunk::new(b"IHDR", vec![0; 13]), Chunk::new(b"IEND", vec![])];
        let bytes = write_chunks(&chunks);
        assert_eq!(read_chunks(&bytes).unwrap(), chunks);
        // IEND's CRC is the well-known AE426082.
        assert_eq!(&bytes[bytes.len() - 4..], &[0xAE, 0x42, 0x60, 0x82]);
    }

    #[test]
    fn rejects_garbage_without_panicking() {
        assert!(read_chunks(b"nope").is_err());
        let mut bytes = write_chunks(&[Chunk::new(b"IHDR", vec![1, 2, 3])]);
        bytes.truncate(bytes.len() - 2);
        assert!(read_chunks(&bytes).is_err());
        let mut huge = PNG_SIGNATURE.to_vec();
        huge.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, b'm', b'k', b'T', b'S']);
        assert!(read_chunks(&huge).is_err());
    }
}
