//! Direkte Leser für die OpenType-Tabellen, die ttf-parser nicht oder nicht in der vom Original
//! benötigten Form liefert: `name` (Familie/Stil/PostScript-Name), `BASE` (ideographische
//! Grundlinie, 09 §5.3), GPOS-Paarkerning und `kern` (09 §5.6), dazu das Tabellenverzeichnis für den
//! schnellen Index der Systemschriften.
//!
//! Alle Zugriffe sind bereichsgeprüft; defekte Tabellen ergeben „nicht vorhanden“ statt einer Panik.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Bereichsgeprüfter Big-Endian-Lesezugriff.
#[derive(Clone, Copy)]
pub(crate) struct Bytes<'a>(pub &'a [u8]);

impl<'a> Bytes<'a> {
    pub fn u16(&self, o: usize) -> Option<u16> {
        let b = self.0.get(o..o.checked_add(2)?)?;
        Some(u16::from_be_bytes([b[0], b[1]]))
    }
    pub fn i16(&self, o: usize) -> Option<i16> {
        self.u16(o).map(|v| v as i16)
    }
    pub fn u32(&self, o: usize) -> Option<u32> {
        let b = self.0.get(o..o.checked_add(4)?)?;
        Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    pub fn tag(&self, o: usize) -> Option<[u8; 4]> {
        let b = self.0.get(o..o.checked_add(4)?)?;
        Some([b[0], b[1], b[2], b[3]])
    }
    pub fn slice(&self, o: usize, len: usize) -> Option<&'a [u8]> {
        self.0.get(o..o.checked_add(len)?)
    }
}

/// Namen einer Schrift aus der `name`-Tabelle (Regel wie die Python-Referenz `fontfile.py`:
/// Windows-Einträge, Englisch (0x409) bevorzugt; Mac-Einträge nur, wenn sonst nichts da ist).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FontNames {
    /// Familie: Name-ID 16 (typografische Familie), sonst 1.
    pub family: String,
    /// Stil: Name-ID 17, sonst 2.
    pub subfamily: String,
    /// PostScript-Name: Name-ID 6.
    pub ps_name: String,
    /// Vollständiger Name: Name-ID 4.
    pub full_name: String,
    /// Familie nach Name-ID 1 (Windows-kompatible Familie, z. B. „Arial Black“).
    pub legacy_family: String,
}

/// Liest die `name`-Tabelle.
pub(crate) fn parse_names(name: &[u8]) -> FontNames {
    let b = Bytes(name);
    let mut names: HashMap<u16, String> = HashMap::new();
    let mut unicode: HashMap<u16, String> = HashMap::new();
    let (Some(count), Some(soff)) = (b.u16(2), b.u16(4)) else {
        return FontNames::default();
    };
    for i in 0..count as usize {
        let r = 6 + 12 * i;
        let (Some(pid), Some(_eid), Some(lid), Some(nid), Some(len), Some(off)) = (
            b.u16(r),
            b.u16(r + 2),
            b.u16(r + 4),
            b.u16(r + 6),
            b.u16(r + 8),
            b.u16(r + 10),
        ) else {
            break;
        };
        let Some(raw) = b.slice(soff as usize + off as usize, len as usize) else {
            continue;
        };
        if pid == 3 || pid == 0 {
            let units: Vec<u16> = raw
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_be_bytes(*c))
                .collect();
            let s = String::from_utf16_lossy(&units);
            if pid == 0 {
                unicode.entry(nid).or_insert(s);
            } else if lid == 0x409 || !names.contains_key(&nid) {
                names.insert(nid, s);
            }
        } else if pid == 1 && !names.contains_key(&nid) {
            names.insert(nid, raw.iter().map(|&c| c as char).collect());
        }
    }
    // Unicode-Plattform (0) nur als letzter Ausweg
    for (k, v) in unicode {
        names.entry(k).or_insert(v);
    }
    let get = |a: u16, b: u16| {
        names
            .get(&a)
            .or_else(|| names.get(&b))
            .cloned()
            .unwrap_or_default()
    };
    FontNames {
        family: get(16, 1),
        subfamily: get(17, 2),
        ps_name: names.get(&6).cloned().unwrap_or_default(),
        full_name: names.get(&4).cloned().unwrap_or_default(),
        legacy_family: names.get(&1).cloned().unwrap_or_default(),
    }
}

/// Ideographische Grundlinie (`ideo`) aus `BASE` (horizontale Achse, Skript DFLT oder latn) in
/// Font-Einheiten; `None`, wenn nicht vorhanden. Myriad Pro: −163 (09 §5.3, B).
pub(crate) fn base_ideo(base: &[u8]) -> Option<i16> {
    let b = Bytes(base);
    let h = b.u16(4)? as usize;
    if h == 0 {
        return None;
    }
    let (tl, sl) = (b.u16(h)? as usize, b.u16(h + 2)? as usize);
    if tl == 0 || sl == 0 {
        return None;
    }
    let n = b.u16(h + tl)? as usize;
    let idx = (0..n).position(|i| b.tag(h + tl + 2 + 4 * i) == Some(*b"ideo"))?;
    let so = h + sl;
    for i in 0..b.u16(so)? as usize {
        let tag = b.tag(so + 2 + 6 * i)?;
        if &tag != b"DFLT" && &tag != b"latn" {
            continue;
        }
        let rec = so + b.u16(so + 6 + 6 * i)? as usize;
        let bv = b.u16(rec)? as usize;
        if bv == 0 {
            continue;
        }
        let bo = rec + bv;
        let co = bo + b.u16(bo + 4 + 2 * idx)? as usize;
        return b.i16(co + 2);
    }
    None
}

/// Ein Paarkerning-Untertabelle aus GPOS (LookupType 2).
#[derive(Debug, Clone)]
enum PairSubtable {
    /// Format 1: einzelne Paare.
    Pairs(HashMap<(u16, u16), i16>),
    /// Format 2: Klassenpaare; Werte als Matrix `class1 × class2`.
    Classes {
        coverage: HashSet<u16>,
        class1: HashMap<u16, u16>,
        class2: HashMap<u16, u16>,
        class2_count: usize,
        values: Vec<i16>,
    },
}

/// Paarkerning einer Schrift: GPOS-Lookups des Features `kern`, sonst die `kern`-Tabelle
/// (Regeln wie die Python-Referenz `fontfile.py`, 09 §5.6).
#[derive(Debug, Clone, Default)]
pub(crate) struct KernTables {
    gpos: Vec<PairSubtable>,
    kern: HashMap<(u16, u16), i16>,
}

impl KernTables {
    /// Liest GPOS und `kern` (beide optional).
    pub fn parse(gpos: Option<&[u8]>, kern: Option<&[u8]>) -> Self {
        let gpos = gpos.map(parse_gpos_pairs).unwrap_or_default();
        let kern = if gpos.is_empty() {
            kern.map(parse_kern).unwrap_or_default()
        } else {
            HashMap::new()
        };
        Self { gpos, kern }
    }

    /// Kerning des Paars (g1, g2) in Font-Einheiten (XAdvance der ersten Glyphe).
    ///
    /// Gibt es GPOS-Paarkerning, gewinnt die erste Untertabelle, die das Paar (Format 1) bzw. die erste
    /// Glyphe (Format 2) abdeckt; ein Paar ohne Treffer ergibt 0. Sonst gilt die `kern`-Tabelle.
    pub fn get(&self, g1: u16, g2: u16) -> i16 {
        if !self.gpos.is_empty() {
            for s in &self.gpos {
                match s {
                    PairSubtable::Pairs(p) => {
                        if let Some(v) = p.get(&(g1, g2)) {
                            return *v;
                        }
                    }
                    PairSubtable::Classes {
                        coverage,
                        class1,
                        class2,
                        class2_count,
                        values,
                    } => {
                        if !coverage.contains(&g1) {
                            continue;
                        }
                        let c1 = *class1.get(&g1).unwrap_or(&0) as usize;
                        let c2 = *class2.get(&g2).unwrap_or(&0) as usize;
                        return values.get(c1 * class2_count + c2).copied().unwrap_or(0);
                    }
                }
            }
            return 0;
        }
        self.kern.get(&(g1, g2)).copied().unwrap_or(0)
    }
}

fn coverage_table(b: Bytes, off: usize) -> Option<Vec<(u16, u16)>> {
    let fmt = b.u16(off)?;
    let n = b.u16(off + 2)? as usize;
    let mut out = Vec::new();
    if fmt == 1 {
        for i in 0..n {
            out.push((b.u16(off + 4 + 2 * i)?, i as u16));
        }
    } else if fmt == 2 {
        for i in 0..n {
            let (s, e, si) = (
                b.u16(off + 4 + 6 * i)?,
                b.u16(off + 6 + 6 * i)?,
                b.u16(off + 8 + 6 * i)?,
            );
            if e < s {
                continue;
            }
            for g in s..=e {
                out.push((g, si.wrapping_add(g - s)));
            }
        }
    }
    Some(out)
}

fn class_def(b: Bytes, off: usize) -> HashMap<u16, u16> {
    let mut cd = HashMap::new();
    let Some(fmt) = b.u16(off) else { return cd };
    if fmt == 1 {
        if let (Some(st), Some(n)) = (b.u16(off + 2), b.u16(off + 4)) {
            for i in 0..n as usize {
                match b.u16(off + 6 + 2 * i) {
                    Some(c) => {
                        cd.insert(st.wrapping_add(i as u16), c);
                    }
                    None => break,
                }
            }
        }
    } else if fmt == 2
        && let Some(n) = b.u16(off + 2)
    {
        for i in 0..n as usize {
            let (Some(s), Some(e), Some(c)) = (
                b.u16(off + 4 + 6 * i),
                b.u16(off + 6 + 6 * i),
                b.u16(off + 8 + 6 * i),
            ) else {
                break;
            };
            if e >= s {
                for g in s..=e {
                    cd.insert(g, c);
                }
            }
        }
    }
    cd
}

fn value_record_size(fmt: u16) -> usize {
    2 * (fmt & 0xff).count_ones() as usize
}

/// XAdvance aus einem ValueRecord (Reihenfolge XPlacement, YPlacement, XAdvance, …).
fn value_record_x_advance(b: Bytes, off: usize, fmt: u16) -> i16 {
    let mut o = off;
    for bit in 0..8 {
        if fmt & (1 << bit) != 0 {
            if bit == 2 {
                return b.i16(o).unwrap_or(0);
            }
            o += 2;
        }
    }
    0
}

fn parse_gpos_pairs(gpos: &[u8]) -> Vec<PairSubtable> {
    parse_gpos_inner(Bytes(gpos)).unwrap_or_default()
}

fn parse_gpos_inner(b: Bytes) -> Option<Vec<PairSubtable>> {
    let feat_list = b.u16(6)? as usize;
    let lookup_list = b.u16(8)? as usize;
    let mut lookups: Vec<u16> = Vec::new();
    for i in 0..b.u16(feat_list)? as usize {
        let tag = b.tag(feat_list + 2 + 6 * i)?;
        if &tag != b"kern" {
            continue;
        }
        let fo = feat_list + b.u16(feat_list + 6 + 6 * i)? as usize;
        for j in 0..b.u16(fo + 2)? as usize {
            lookups.push(b.u16(fo + 4 + 2 * j)?);
        }
    }
    lookups.sort_unstable();
    lookups.dedup();
    let mut subs = Vec::new();
    for li in lookups {
        let Some(lo) = b
            .u16(lookup_list + 2 + 2 * li as usize)
            .map(|o| lookup_list + o as usize)
        else {
            continue;
        };
        let (Some(ltype), Some(nsub)) = (b.u16(lo), b.u16(lo + 4)) else {
            continue;
        };
        for s in 0..nsub as usize {
            let Some(mut so) = b.u16(lo + 6 + 2 * s).map(|o| lo + o as usize) else {
                continue;
            };
            let mut t = ltype;
            if t == 9 {
                let (Some(et), Some(eo)) = (b.u16(so + 2), b.u32(so + 4)) else {
                    continue;
                };
                t = et;
                so += eo as usize;
            }
            if t != 2 {
                continue;
            }
            if let Some(sub) = pair_subtable(b, so) {
                subs.push(sub);
            }
        }
    }
    Some(subs)
}

fn pair_subtable(b: Bytes, so: usize) -> Option<PairSubtable> {
    let fmt = b.u16(so)?;
    let cov = coverage_table(b, so + b.u16(so + 2)? as usize)?;
    let (vf1, vf2) = (b.u16(so + 4)?, b.u16(so + 6)?);
    let (s1, s2) = (value_record_size(vf1), value_record_size(vf2));
    match fmt {
        1 => {
            let nps = b.u16(so + 8)? as usize;
            let mut pairs = HashMap::new();
            for (g, ci) in cov {
                let ci = ci as usize;
                if ci >= nps {
                    continue;
                }
                let Some(po) = b.u16(so + 10 + 2 * ci).map(|o| so + o as usize) else {
                    continue;
                };
                let Some(npv) = b.u16(po) else { continue };
                let rec = 2 + s1 + s2;
                for k in 0..npv as usize {
                    let ro = po + 2 + rec * k;
                    let Some(g2) = b.u16(ro) else { break };
                    pairs.insert((g, g2), value_record_x_advance(b, ro + 2, vf1));
                }
            }
            Some(PairSubtable::Pairs(pairs))
        }
        2 => {
            let class1 = class_def(b, so + b.u16(so + 8)? as usize);
            let class2 = class_def(b, so + b.u16(so + 10)? as usize);
            let (c1n, c2n) = (b.u16(so + 12)? as usize, b.u16(so + 14)? as usize);
            let rec = s1 + s2;
            let base = so + 16;
            let mut values = Vec::with_capacity(c1n * c2n);
            for i in 0..c1n * c2n {
                values.push(value_record_x_advance(b, base + i * rec, vf1));
            }
            Some(PairSubtable::Classes {
                coverage: cov.into_iter().map(|(g, _)| g).collect(),
                class1,
                class2,
                class2_count: c2n,
                values,
            })
        }
        _ => None,
    }
}

/// `kern`-Tabelle Version 0, horizontale Format-0-Untertabellen (spätere überschreiben frühere).
fn parse_kern(kern: &[u8]) -> HashMap<(u16, u16), i16> {
    let b = Bytes(kern);
    let mut d = HashMap::new();
    if b.u16(0) != Some(0) {
        return d;
    }
    let n = b.u16(2).unwrap_or(0) as usize;
    let mut o = 4usize;
    for _ in 0..n {
        let (Some(len), Some(cov)) = (b.u16(o + 2), b.u16(o + 4)) else {
            break;
        };
        if cov >> 8 == 0 && cov & 1 != 0 {
            let np = b.u16(o + 6).unwrap_or(0) as usize;
            for i in 0..np {
                let r = o + 14 + 6 * i;
                let (Some(l), Some(rt), Some(v)) = (b.u16(r), b.u16(r + 2), b.i16(r + 4)) else {
                    break;
                };
                d.insert((l, rt), v);
            }
        }
        if len == 0 {
            break;
        }
        o += len as usize;
    }
    d
}

/// Kurzinfo einer Schrift aus einer Datei, ohne die ganze Datei zu lesen (für den Schriftindex).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QuickFace {
    pub index: u32,
    pub names: FontNames,
    /// `OS/2.usWeightClass` (400 normal, 700 fett).
    pub weight: u16,
    /// Kursiv (OS/2 fsSelection Bit 0 bzw. 9, sonst `head.macStyle` Bit 1).
    pub italic: bool,
    /// Fett (fsSelection Bit 5 bzw. macStyle Bit 0).
    pub bold: bool,
}

fn read_at(f: &mut File, off: u64, len: usize) -> Option<Vec<u8>> {
    if len > 16 << 20 {
        return None;
    }
    f.seek(SeekFrom::Start(off)).ok()?;
    let mut buf = vec![0u8; len];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Liest Namen und Stilangaben aller Schriften einer Datei (`.ttf`, `.otf`, `.ttc`, `.otc`).
pub(crate) fn quick_scan(path: &Path) -> Vec<QuickFace> {
    let Ok(mut f) = File::open(path) else {
        return Vec::new();
    };
    let Some(head) = read_at(&mut f, 0, 12) else {
        return Vec::new();
    };
    let hb = Bytes(&head);
    let mut offsets: Vec<u32> = Vec::new();
    if hb.tag(0) == Some(*b"ttcf") {
        let n = hb.u32(8).unwrap_or(0).min(256) as usize;
        if let Some(tab) = read_at(&mut f, 12, 4 * n) {
            let tb = Bytes(&tab);
            offsets.extend((0..n).filter_map(|i| tb.u32(4 * i)));
        }
    } else {
        offsets.push(0);
    }
    let mut out = Vec::new();
    for (index, off) in offsets.into_iter().enumerate() {
        if let Some(q) = scan_face(&mut f, off as u64, index as u32) {
            out.push(q);
        }
    }
    out
}

fn scan_face(f: &mut File, off: u64, index: u32) -> Option<QuickFace> {
    let hdr = read_at(f, off, 12)?;
    let hb = Bytes(&hdr);
    let ver = hb.u32(0)?;
    if ver != 0x0001_0000 && ver != u32::from_be_bytes(*b"OTTO") && ver != u32::from_be_bytes(*b"true") {
        return None;
    }
    let n = hb.u16(4)? as usize;
    let dir = read_at(f, off + 12, 16 * n)?;
    let db = Bytes(&dir);
    let mut find = |tag: &[u8; 4]| -> Option<Vec<u8>> {
        for i in 0..n {
            if db.tag(16 * i) == Some(*tag) {
                let (o, l) = (db.u32(16 * i + 8)?, db.u32(16 * i + 12)?);
                return read_at(f, o as u64, l as usize);
            }
        }
        None
    };
    let names = parse_names(&find(b"name")?);
    let os2 = find(b"OS/2");
    let headt = find(b"head");
    let (mut weight, mut italic, mut bold) = (400u16, false, false);
    if let Some(o) = &os2 {
        let ob = Bytes(o);
        weight = ob.u16(4).unwrap_or(400);
        let fs = ob.u16(62).unwrap_or(0);
        italic = fs & 1 != 0 || fs & (1 << 9) != 0;
        bold = fs & (1 << 5) != 0;
    } else if let Some(h) = &headt {
        let ms = Bytes(h).u16(44).unwrap_or(0);
        bold = ms & 1 != 0;
        italic = ms & 2 != 0;
        if bold {
            weight = 700;
        }
    }
    Some(QuickFace {
        index,
        names,
        weight,
        italic,
        bold,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readers_never_panic_on_garbage() {
        let junk: Vec<u8> = (0..200u32).map(|i| (i * 37 % 251) as u8).collect();
        let _ = parse_names(&junk);
        let _ = base_ideo(&junk);
        let k = KernTables::parse(Some(&junk), Some(&junk));
        let _ = k.get(1, 2);
        let _ = parse_names(&[]);
        assert_eq!(base_ideo(&[]), None);
    }

    #[test]
    fn kern_table_format0() {
        // Version 0, 1 Untertabelle, horizontal Format 0, 1 Paar (3, 4) = −50
        let mut t = vec![0, 0, 0, 1];
        t.extend_from_slice(&[0, 0, 0, 20, 0, 1, 0, 1, 0, 6, 0, 0, 0, 0, 0, 3, 0, 4, 0xff, 0xce]);
        let k = KernTables::parse(None, Some(&t));
        assert_eq!(k.get(3, 4), -50);
        assert_eq!(k.get(4, 3), 0);
    }
}
