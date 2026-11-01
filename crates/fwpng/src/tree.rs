//! The tagged document tree stored (zlib-compressed) in `mkTS` and `mkBS`.
//!
//! Grammar, as observed in files written by Fireworks MX:
//!
//! ```text
//! node   := TAG KIND '{' body '}'
//! TAG    := 3 ASCII bytes            e.g. "LAY", "WID", "TID"
//! KIND   := 'v' | 'i' | 'f' | 'b' | 's'
//! body   := node*                    for 'v' (vector / container)
//!         | hex digits               for 'i' (32-bit integer, two's complement), e.g. "ffffffff"
//!         | decimal text             for 'f' (float), e.g. "28.34", "-101"
//!         | "0" | "1"                for 'b'
//!         | u16be count, UTF-16BE    for 's' (count is in UTF-16 code units; may contain '}')
//! ```
//!
//! Scalar bodies are kept as their original text so files round-trip byte for byte.

use crate::{Error, Result};

/// Value kind of a node (the fourth byte of its header).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Vector,
    Int,
    Float,
    Bool,
    Str,
}

impl Kind {
    fn from_byte(b: u8) -> Option<Self> {
        Some(match b {
            b'v' => Self::Vector,
            b'i' => Self::Int,
            b'f' => Self::Float,
            b'b' => Self::Bool,
            b's' => Self::Str,
            _ => return None,
        })
    }
    fn byte(self) -> u8 {
        match self {
            Self::Vector => b'v',
            Self::Int => b'i',
            Self::Float => b'f',
            Self::Bool => b'b',
            Self::Str => b's',
        }
    }
}

/// One node of the tree.
#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub tag: [u8; 3],
    pub kind: Kind,
    /// Scalar body text (`i`, `f`, `b`) or decoded string (`s`). Empty for vectors.
    pub text: String,
    /// Children of a vector node, in file order (tags may repeat).
    pub children: Vec<Node>,
}

/// Maximum nesting depth accepted when parsing (documents are shallow; this bounds recursion).
pub const MAX_DEPTH: usize = 256;

impl Node {
    pub fn vector(tag: &str, children: Vec<Node>) -> Self {
        Self {
            tag: tag3(tag),
            kind: Kind::Vector,
            text: String::new(),
            children,
        }
    }
    pub fn int(tag: &str, v: i64) -> Self {
        Self::scalar(tag, Kind::Int, format!("{:x}", v as u32))
    }
    pub fn float(tag: &str, v: f64) -> Self {
        Self::scalar(tag, Kind::Float, fmt_float(v))
    }
    pub fn bool(tag: &str, v: bool) -> Self {
        Self::scalar(tag, Kind::Bool, if v { "1" } else { "0" }.to_owned())
    }
    pub fn string(tag: &str, v: &str) -> Self {
        Self::scalar(tag, Kind::Str, v.to_owned())
    }
    fn scalar(tag: &str, kind: Kind, text: String) -> Self {
        Self {
            tag: tag3(tag),
            kind,
            text,
            children: Vec::new(),
        }
    }

    /// The tag as a string, e.g. `"LAY"`.
    pub fn tag_str(&self) -> &str {
        std::str::from_utf8(&self.tag).unwrap_or("???")
    }

    pub fn is(&self, tag: &str) -> bool {
        self.tag_str() == tag
    }

    /// First child with this tag.
    pub fn child(&self, tag: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.is(tag))
    }

    pub fn child_mut(&mut self, tag: &str) -> Option<&mut Node> {
        self.children.iter_mut().find(|c| c.is(tag))
    }

    /// All children with this tag.
    pub fn children_tagged<'a>(&'a self, tag: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.is(tag))
    }

    /// Integer value (hex, 32-bit two's complement) as `i64`.
    pub fn as_int(&self) -> Option<i64> {
        if self.kind != Kind::Int {
            return None;
        }
        let t = self.text.trim();
        let (neg, digits) = match t.strip_prefix('-') {
            Some(d) => (true, d),
            None => (false, t),
        };
        let v = u32::from_str_radix(digits, 16).ok()? as i32 as i64;
        Some(if neg { -v } else { v })
    }

    /// Integer value as raw `u32` (colours, ids).
    pub fn as_u32(&self) -> Option<u32> {
        if self.kind != Kind::Int {
            return None;
        }
        u32::from_str_radix(self.text.trim(), 16).ok()
    }

    pub fn as_float(&self) -> Option<f64> {
        match self.kind {
            Kind::Float => self.text.trim().parse().ok().filter(|v: &f64| v.is_finite()),
            Kind::Int => self.as_int().map(|v| v as f64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        if self.kind != Kind::Bool {
            return None;
        }
        Some(self.text.trim() != "0")
    }

    pub fn as_str(&self) -> Option<&str> {
        (self.kind == Kind::Str).then_some(self.text.as_str())
    }

    pub fn int_of(&self, tag: &str) -> Option<i64> {
        self.child(tag)?.as_int()
    }
    pub fn u32_of(&self, tag: &str) -> Option<u32> {
        self.child(tag)?.as_u32()
    }
    pub fn float_of(&self, tag: &str) -> Option<f64> {
        self.child(tag)?.as_float()
    }
    pub fn bool_of(&self, tag: &str) -> Option<bool> {
        self.child(tag)?.as_bool()
    }
    pub fn str_of(&self, tag: &str) -> Option<&str> {
        self.child(tag)?.as_str()
    }

    /// Parse a tree from decompressed `mkTS`/`mkBS` bytes. Trailing bytes are ignored.
    pub fn parse(bytes: &[u8]) -> Result<Node> {
        let mut p = Parser { b: bytes, i: 0 };
        p.node(0)
    }

    /// Serialize back to the on-disk text form.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        self.write(&mut out, 0);
        out
    }

    fn write(&self, out: &mut Vec<u8>, depth: usize) {
        out.extend_from_slice(&self.tag);
        out.push(self.kind.byte());
        out.push(b'{');
        match self.kind {
            Kind::Vector => {
                if depth < MAX_DEPTH {
                    for c in &self.children {
                        c.write(out, depth + 1);
                    }
                }
            }
            Kind::Str => {
                let units: Vec<u16> = self.text.encode_utf16().collect();
                let n = u16::try_from(units.len()).unwrap_or(u16::MAX);
                out.extend_from_slice(&n.to_be_bytes());
                for u in units.iter().take(n as usize) {
                    out.extend_from_slice(&u.to_be_bytes());
                }
            }
            _ => out.extend_from_slice(self.text.as_bytes()),
        }
        out.push(b'}');
    }
}

fn tag3(tag: &str) -> [u8; 3] {
    let b = tag.as_bytes();
    [
        b.first().copied().unwrap_or(b'?'),
        b.get(1).copied().unwrap_or(b'?'),
        b.get(2).copied().unwrap_or(b'?'),
    ]
}

/// Float text in Fireworks' style: no trailing ".0" for integral values, otherwise up to 6
/// decimals (as Fireworks writes; fewer would move gradient handles and points slightly).
pub fn fmt_float(v: f64) -> String {
    if !v.is_finite() {
        return "0".to_owned();
    }
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.6}");
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}

impl Parser<'_> {
    fn err(&self, reason: &'static str) -> Error {
        Error::Tree {
            offset: self.i,
            reason,
        }
    }

    fn byte(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }

    fn expect(&mut self, c: u8, reason: &'static str) -> Result<()> {
        if self.byte() != Some(c) {
            return Err(self.err(reason));
        }
        self.i += 1;
        Ok(())
    }

    fn node(&mut self, depth: usize) -> Result<Node> {
        if depth > MAX_DEPTH {
            return Err(self.err("nesting too deep"));
        }
        let head = self
            .b
            .get(self.i..self.i + 4)
            .ok_or_else(|| self.err("truncated node header"))?;
        let tag = [head[0], head[1], head[2]];
        let kind = Kind::from_byte(head[3]).ok_or_else(|| self.err("unknown value kind"))?;
        self.i += 4;
        self.expect(b'{', "expected '{'")?;
        let mut node = Node {
            tag,
            kind,
            text: String::new(),
            children: Vec::new(),
        };
        match kind {
            Kind::Vector => loop {
                match self.byte() {
                    Some(b'}') => break,
                    Some(_) => node.children.push(self.node(depth + 1)?),
                    None => return Err(self.err("unterminated vector")),
                }
            },
            Kind::Str => {
                let n = self
                    .b
                    .get(self.i..self.i + 2)
                    .ok_or_else(|| self.err("truncated string length"))?;
                let n = u16::from_be_bytes([n[0], n[1]]) as usize;
                self.i += 2;
                let raw = self
                    .b
                    .get(self.i..self.i + 2 * n)
                    .ok_or_else(|| self.err("truncated string"))?;
                let units: Vec<u16> = raw
                    .chunks_exact(2)
                    .map(|c| u16::from_be_bytes([c[0], c[1]]))
                    .collect();
                node.text = String::from_utf16_lossy(&units);
                self.i += 2 * n;
            }
            _ => {
                let rest = self.b.get(self.i..).unwrap_or_default();
                let end = rest
                    .iter()
                    .position(|&c| c == b'}')
                    .ok_or_else(|| self.err("unterminated scalar"))?;
                node.text = String::from_utf8_lossy(&rest[..end]).into_owned();
                self.i += end;
            }
        }
        self.expect(b'}', "expected '}'")?;
        Ok(node)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_round_trips() {
        let mut src = b"MKBv{FRCi{1}WIDf{180}BGCi{ffffffff}LNMs{".to_vec();
        src.extend_from_slice(&[0, 3, 0, b'a', 0, b'}', 0, b'c']); // "a}c": string may contain '}'
        src.extend_from_slice(b"}VISb{1}LYLv{}}");
        let n = Node::parse(&src).unwrap();
        assert_eq!(n.tag_str(), "MKB");
        assert_eq!(n.int_of("FRC"), Some(1));
        assert_eq!(n.float_of("WID"), Some(180.0));
        assert_eq!(n.u32_of("BGC"), Some(0xffff_ffff));
        assert_eq!(n.int_of("BGC"), Some(-1));
        assert_eq!(n.str_of("LNM"), Some("a}c"));
        assert_eq!(n.bool_of("VIS"), Some(true));
        assert_eq!(n.to_bytes(), src);
    }

    #[test]
    fn builders_match_fireworks_text() {
        assert_eq!(Node::int("BGC", -1).text, "ffffffff");
        assert_eq!(Node::int("TSZ", 128).text, "80");
        assert_eq!(Node::float("RES", 72.0).text, "72");
        assert_eq!(Node::float("RES", 28.34).text, "28.34");
    }

    #[test]
    fn hostile_input_is_an_error() {
        for bad in [
            &b""[..],
            b"MKB",
            b"MKBx{}",
            b"MKBv{",
            b"MKBs{\xff\xff}",
            b"MKBi{12",
        ] {
            assert!(Node::parse(bad).is_err(), "{bad:?}");
        }
        let deep = "AAAv{".repeat(MAX_DEPTH + 10);
        assert!(Node::parse(deep.as_bytes()).is_err());
    }
}
