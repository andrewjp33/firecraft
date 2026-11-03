//! Deckungsmasken: die drei Rasterverfahren des Originals und die harte Kante
//! (Spezifikation 07 §4, Referenz `re\vektor\fwraster.py`, bitgleich).
//!
//! Gemeinsam: 4×4-Teilabtastung; Teilzeilen bei `y = Pixel + (k + 0,5)/4`; Schnittpunkte werden
//! auf Teilspalten gerundet (`floor(4x + 0,5)` bzw. `ceil(4x − 0,5)` für negative Werte); eine
//! Teilspalte `i` ist bedeckt, wenn `s ≤ i < e`. Kanten sind halboffen (`ya ≤ y < yb`), waagerechte
//! Kanten entfallen, jede Kontur wird implizit geschlossen.
//!
//! | Verfahren | Wahl | Regel |
//! |---|---|---|
//! | A [`Rasterizer::Xor`] | `EOFb = 1`, kein AGM | je Kontur gerade/ungerade, Byte-Beiträge 16 je Teilspalte, Vollpixel 64; in Teilzeile 0 jeder Beitrag − 1; Masken **bitweise XOR** (`@007202c0`, `@0082f910`) |
//! | B [`Rasterizer::Winding`] | `EOFb = 0` | alle Konturen, Windungszahl ≠ 0; Vollpixel in Teilzeile 0 → 63; Spannen, die im selben Pixel beginnen und enden, entfallen (`@0071fc50`) |
//! | G [`Rasterizer::Agm`] | `EOFb = 1` und `AGMb = 1` | konservativ: jede vom Inneren geschnittene Teilzelle zählt, `α = min(255, 16·n)`; Eckpunkte vorher auf 16.16 gerundet (A, `AGM_SNAP`) |
//! | hart | `FETi = 0` | ein Abtastpunkt je Pixelmitte, `ceil(a − 0,5) ≤ x < ceil(b − 0,5)`; im AGM-Zweig stattdessen AA-Maske `> 191` |
//!
//! Alle Summen laufen modulo 256 in einem Byte (`wrapping_add`) wie im Original. Zeilen werden mit
//! rayon parallel gerechnet; das Ergebnis ist unabhängig von der Thread-Zahl.

use crate::support::geom::{IRect, Vec2};
use crate::support::raster::Mask;

/// Teilabtastungen je Achse (Original: Shift 2).
pub const SUBSAMPLES: i64 = 4;

/// Schwelle für die harte Kante im AGM-Zweig (`@00738aa0`: Binarisierung bei Alpha > 191).
pub const HARD_AGM_THRESHOLD: u8 = 191;

/// Festkomma-Raster der AGM-Eckpunkte (16.16, A: 255 statt 254 von 269 Fällen exakt).
pub const AGM_SNAP: f64 = 65536.0;

/// Rasterverfahren (Zweig von „Region → Maske“ `@00738aa0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Rasterizer {
    /// Verfahren A: je Kontur, Masken per XOR verknüpft (`EOFb = 1`, kein AGM).
    Xor,
    /// Verfahren B: Windungszahl über alle Konturen (`EOFb = 0`).
    Winding,
    /// Verfahren G: AGM-Nachbildung (`EOFb = 1` und `AGMb = 1`).
    Agm,
}

impl Rasterizer {
    /// Zweigwahl nach `EOFb`/`AGMb` (07 §2).
    pub fn select(even_odd: bool, agm: bool) -> Self {
        if !even_odd {
            Self::Winding
        } else if agm {
            Self::Agm
        } else {
            Self::Xor
        }
    }
}

/// 8-Bit-Deckung über einem Pixelrechteck (Zeilen von oben nach unten).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Coverage {
    /// Lage in Dokumentpixeln.
    pub rect: IRect,
    /// `width·height` Bytes.
    pub data: Vec<u8>,
}

impl Coverage {
    /// Leere (0) Deckung über `rect`.
    pub fn new(rect: IRect) -> Self {
        let n = rect.width() as usize * rect.height() as usize;
        Self {
            rect,
            data: vec![0; n],
        }
    }

    /// Breite in Pixeln.
    pub fn width(&self) -> usize {
        self.rect.width() as usize
    }

    /// Höhe in Pixeln.
    pub fn height(&self) -> usize {
        self.rect.height() as usize
    }

    /// Wert an Dokumentposition (0 außerhalb).
    pub fn get(&self, x: i32, y: i32) -> u8 {
        if !self.rect.contains(x, y) {
            return 0;
        }
        let i = (y - self.rect.top) as usize * self.width() + (x - self.rect.left) as usize;
        self.data.get(i).copied().unwrap_or(0)
    }

    /// Als [`crate::support::raster::Mask`] (gleiche Lage); `None`, wenn das Rechteck zu groß ist.
    pub fn into_mask(self) -> Option<Mask> {
        Mask::from_raw(self.rect, self.data).ok()
    }
}

/// Kante `(ya, yb, xa, dx/dy, Richtung)` mit `ya < yb`.
#[derive(Debug, Clone, Copy)]
struct Edge {
    ya: f64,
    yb: f64,
    xa: f64,
    k: f64,
    dir: i32,
}

impl Edge {
    #[inline]
    fn x_at(&self, y: f64) -> f64 {
        self.xa + (y - self.ya) * self.k
    }
}

fn push_edges(pts: &[Vec2], tag: u32, out: &mut Vec<(u32, Edge)>) {
    let m = pts.len();
    for i in 0..m {
        let (a, b) = (pts[i], pts[(i + 1) % m]);
        if a.y == b.y || !(a.y.is_finite() && b.y.is_finite() && a.x.is_finite() && b.x.is_finite()) {
            continue;
        }
        let e = if a.y < b.y {
            Edge {
                ya: a.y,
                yb: b.y,
                xa: a.x,
                k: (b.x - a.x) / (b.y - a.y),
                dir: 1,
            }
        } else {
            Edge {
                ya: b.y,
                yb: a.y,
                xa: b.x,
                k: (a.x - b.x) / (a.y - b.y),
                dir: -1,
            }
        };
        out.push((tag, e));
    }
}

/// Kanten je Pixelzeile des Fensters (Obermenge: jede Kante, deren y-Bereich die Zeile berührt).
struct RowBuckets {
    edges: Vec<(u32, Edge)>,
    rows: Vec<Vec<u32>>,
}

impl RowBuckets {
    fn new(polys: &[&[Vec2]], y0: i32, h: usize) -> Self {
        let mut edges = Vec::new();
        for (ci, p) in polys.iter().enumerate() {
            push_edges(p, ci as u32, &mut edges);
        }
        let mut rows = vec![Vec::new(); h];
        for (i, (_, e)) in edges.iter().enumerate() {
            let jf = (e.ya - y0 as f64).floor() - 1.0;
            let jl = (e.yb - y0 as f64).floor() + 1.0;
            if jl < 0.0 || jf >= h as f64 {
                continue;
            }
            let a = jf.max(0.0) as usize;
            let b = (jl.min(h as f64 - 1.0)) as usize;
            for r in rows.iter_mut().take(b + 1).skip(a) {
                r.push(i as u32);
            }
        }
        Self { edges, rows }
    }
}

/// Schnittpunkte `(x, Richtung)` der Teilzeile `y`, sortiert wie Python-Tupel (erst x, dann Richtung).
fn crossings(b: &RowBuckets, row: &[u32], y: f64, contour: Option<u32>, out: &mut Vec<(f64, i32)>) {
    out.clear();
    for &i in row {
        let (c, e) = &b.edges[i as usize];
        if let Some(cc) = contour
            && *c != cc
        {
            continue;
        }
        if e.ya <= y && y < e.yb {
            let x = e.x_at(y);
            if !x.is_nan() {
                out.push((x, e.dir));
            }
        }
    }
    out.sort_by(|a, b| {
        a.0.partial_cmp(&b.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });
}

/// Rundung wie das Original: `floor(v + 0,5)` bzw. `ceil(v − 0,5)` für negative Werte.
#[inline]
fn fw_round(v: f64) -> i64 {
    let r = if v >= 0.0 {
        (v + 0.5).floor()
    } else {
        (v - 0.5).ceil()
    };
    sat_i64(r)
}

#[inline]
fn sat_i64(v: f64) -> i64 {
    if v.is_nan() {
        0
    } else {
        v.clamp(-(1i64 << 52) as f64, (1i64 << 52) as f64) as i64
    }
}

#[inline]
fn sub_y(y0: i32, j: usize, sr: i64) -> f64 {
    (y0 as i64 + j as i64) as f64 + (sr as f64 + 0.5) / SUBSAMPLES as f64
}

/// Verfahren A (`@007202c0`): je Kontur gerade/ungerade Paarung, Masken bitweise XOR.
pub fn coverage_xor(polys: &[&[Vec2]], rect: IRect) -> Coverage {
    let mut cov = Coverage::new(rect);
    let (w, h) = (cov.width(), cov.height());
    if w == 0 || h == 0 {
        return cov;
    }
    let b = RowBuckets::new(polys, rect.top, h);
    let x0s = rect.left as i64 * SUBSAMPLES;
    let ws = w as i64 * SUBSAMPLES;
    cov.data.chunks_mut(w).enumerate().for_each(|(j, out)| {
        let row = &b.rows[j];
        if row.is_empty() {
            return;
        }
        let mut contours: Vec<u32> = row.iter().map(|&i| b.edges[i as usize].0).collect();
        contours.sort_unstable();
        contours.dedup();
        let mut m = vec![0u8; w];
        let mut xs = Vec::new();
        for &c in &contours {
            let (mut lo, mut hi) = (usize::MAX, 0usize);
            for sr in 0..SUBSAMPLES {
                let y = sub_y(rect.top, j, sr);
                crossings(&b, row, y, Some(c), &mut xs);
                let full: u8 = if sr == 0 { 63 } else { 64 };
                let dec: u8 = if sr == 0 { 1 } else { 0 };
                let mut k = 0;
                while k + 1 < xs.len() {
                    let mut s = fw_round(xs[k].0 * SUBSAMPLES as f64) - x0s;
                    let mut e = fw_round(xs[k + 1].0 * SUBSAMPLES as f64) - x0s;
                    k += 2;
                    if s < 0 {
                        s = 0;
                    }
                    if e > ws {
                        e = ws;
                    }
                    if s >= ws || s > e {
                        continue;
                    }
                    let (mut ps, pe) = ((s >> 2) as usize, (e >> 2) as usize);
                    lo = lo.min(ps);
                    hi = hi.max(pe.min(w - 1));
                    if ps == pe {
                        let v = ((e - s) * 16) as u32;
                        if v != 0 {
                            let add = (v - dec as u32).min(full as u32) as u8;
                            m[ps] = m[ps].wrapping_add(add);
                        }
                        continue;
                    }
                    if s & 3 != 0 {
                        m[ps] = m[ps].wrapping_add(((4 - (s & 3)) * 16) as u8 - dec);
                        ps += 1;
                    }
                    for v in &mut m[ps..pe] {
                        *v = v.wrapping_add(full);
                    }
                    if e & 3 != 0 {
                        m[pe] = m[pe].wrapping_add(((e & 3) * 16) as u8 - dec);
                    }
                }
            }
            if lo <= hi {
                for i in lo..=hi {
                    out[i] ^= m[i];
                    m[i] = 0;
                }
            }
        }
    });
    cov
}

/// Verfahren B (`@0071fc50`): alle Konturen, Windungszahl ≠ 0.
pub fn coverage_winding(polys: &[&[Vec2]], rect: IRect) -> Coverage {
    let mut cov = Coverage::new(rect);
    let (w, h) = (cov.width(), cov.height());
    if w == 0 || h == 0 {
        return cov;
    }
    let b = RowBuckets::new(polys, rect.top, h);
    let x0s = rect.left as i64 * SUBSAMPLES;
    let x1s = (rect.left as i64 + w as i64) * SUBSAMPLES;
    cov.data.chunks_mut(w).enumerate().for_each(|(j, out)| {
        let row = &b.rows[j];
        if row.is_empty() {
            return;
        }
        let mut xs = Vec::new();
        for sr in 0..SUBSAMPLES {
            let y = sub_y(rect.top, j, sr);
            crossings(&b, row, y, None, &mut xs);
            let full: u8 = if sr == 0 { 63 } else { 64 };
            let n = xs.len();
            let mut i = 0;
            while i < n {
                let (xa, mut wn) = xs[i];
                i += 1;
                let mut xb = xa;
                while i < n {
                    let (x, dd) = xs[i];
                    xb = x;
                    wn += dd;
                    i += 1;
                    if wn == 0 {
                        break;
                    }
                }
                let s = fw_round(xa * SUBSAMPLES as f64).max(x0s) - x0s;
                let mut e = fw_round(xb * SUBSAMPLES as f64).min(x1s) - x0s;
                if e < s {
                    e = s;
                }
                let (mut ps, pe) = ((s >> 2) as usize, (e >> 2) as usize);
                if ps < pe {
                    if s & 3 != 0 {
                        out[ps] = out[ps].wrapping_add(((4 - (s & 3)) * 16) as u8);
                        ps += 1;
                    }
                    for v in &mut out[ps..pe] {
                        *v = v.wrapping_add(full);
                    }
                    if e & 3 != 0 {
                        out[pe] = out[pe].wrapping_add(((e & 3) * 16) as u8);
                    }
                }
            }
        }
    });
    cov
}

/// Eckpunkte auf das 16.16-Raster runden (Python `round` = halbe Werte zur geraden Zahl).
pub fn agm_snap(polys: &[&[Vec2]]) -> Vec<Vec<Vec2>> {
    polys
        .iter()
        .map(|p| {
            p.iter()
                .map(|v| {
                    Vec2::new(
                        (v.x * AGM_SNAP).round_ties_even() / AGM_SNAP,
                        (v.y * AGM_SNAP).round_ties_even() / AGM_SNAP,
                    )
                })
                .collect()
        })
        .collect()
}

/// Verfahren G (AGM-Nachbildung, A): je Teilzeilen-Band zählt jede Teilzelle, deren Inneres die
/// Fläche schneidet; `α = min(255, 16·n)`. `nonzero` = Windungsregel statt gerade/ungerade.
/// Die Punkte werden **nicht** gerundet (dafür [`agm_snap`]).
pub fn coverage_agm(polys: &[&[Vec2]], rect: IRect, nonzero: bool) -> Coverage {
    const EPS: f64 = 1e-6;
    let mut cov = Coverage::new(rect);
    let (w, h) = (cov.width(), cov.height());
    if w == 0 || h == 0 {
        return cov;
    }
    let b = RowBuckets::new(polys, rect.top, h);
    let mut vys: Vec<f64> = polys
        .iter()
        .flat_map(|p| p.iter().map(|v| v.y))
        .filter(|y| y.is_finite())
        .collect();
    vys.sort_by(f64::total_cmp);
    vys.dedup();
    let ws = w * SUBSAMPLES as usize;
    let x0 = rect.left as f64;
    cov.data.chunks_mut(w).enumerate().for_each(|(j, out)| {
        let row = &b.rows[j];
        if row.is_empty() {
            return;
        }
        let mut cnt = vec![0u32; w];
        let mut diff = vec![0i32; ws + 1];
        let mut xs = Vec::new();
        let mut ivs: Vec<(f64, f64)> = Vec::new();
        let mut ys: Vec<f64> = Vec::new();
        for sr in 0..SUBSAMPLES as usize {
            let jj = j * SUBSAMPLES as usize + sr;
            let y0 = rect.top as f64 + jj as f64 / SUBSAMPLES as f64;
            let y1 = y0 + 1.0 / SUBSAMPLES as f64;
            ivs.clear();
            let mut band: Vec<u32> = Vec::new();
            for &i in row {
                let e = &b.edges[i as usize].1;
                if e.yb <= y0 || e.ya >= y1 {
                    continue;
                }
                band.push(i);
                let ta = e.ya.max(y0);
                let tb = e.yb.min(y1);
                let xa = e.xa + (ta - e.ya) * e.k;
                let xb = e.xa + (tb - e.ya) * e.k;
                ivs.push((xa.min(xb), xa.max(xb)));
            }
            if band.is_empty() {
                continue;
            }
            ys.clear();
            ys.push(y0 + EPS);
            ys.push(y1 - EPS);
            let lo = vys.partition_point(|&v| v <= y0);
            let hi = vys.partition_point(|&v| v < y1);
            for &vy in vys.get(lo..hi.max(lo)).unwrap_or(&[]) {
                ys.push(vy - EPS);
                ys.push(vy + EPS);
            }
            for &y in &ys {
                crossings(&b, &band, y, None, &mut xs);
                if nonzero {
                    let mut wn = 0;
                    let mut st = 0.0;
                    for &(x, dd) in &xs {
                        if wn == 0 {
                            st = x;
                        }
                        wn += dd;
                        if wn == 0 {
                            ivs.push((st, x));
                        }
                    }
                } else {
                    let mut k = 0;
                    while k + 1 < xs.len() {
                        ivs.push((xs[k].0, xs[k + 1].0));
                        k += 2;
                    }
                }
            }
            diff.iter_mut().for_each(|v| *v = 0);
            let mut any = false;
            for &(a, bb) in &ivs {
                let s0 = sat_i64(((a - x0) * SUBSAMPLES as f64).floor()).max(0);
                let t = sat_i64(((bb - x0) * SUBSAMPLES as f64).ceil()).min(ws as i64);
                if t > s0 {
                    diff[s0 as usize] += 1;
                    diff[t as usize] -= 1;
                    any = true;
                }
            }
            if !any {
                continue;
            }
            let mut run = 0;
            for (c, d) in diff.iter().take(ws).enumerate() {
                run += d;
                if run > 0 {
                    cnt[c >> 2] += 1;
                }
            }
        }
        for (o, c) in out.iter_mut().zip(&cnt) {
            *o = (16 * c).min(255) as u8;
        }
    });
    cov
}

/// Harte Kante (`FETi = 0`): ein Abtastpunkt je Pixelmitte.
pub fn coverage_hard(polys: &[&[Vec2]], rect: IRect, nonzero: bool) -> Coverage {
    let mut cov = Coverage::new(rect);
    let (w, h) = (cov.width(), cov.height());
    if w == 0 || h == 0 {
        return cov;
    }
    let b = RowBuckets::new(polys, rect.top, h);
    let x0 = rect.left as f64;
    cov.data.chunks_mut(w).enumerate().for_each(|(j, out)| {
        let row = &b.rows[j];
        if row.is_empty() {
            return;
        }
        let y = (rect.top as i64 + j as i64) as f64 + 0.5;
        let mut xs = Vec::new();
        crossings(&b, row, y, None, &mut xs);
        let mut ivs = Vec::new();
        if nonzero {
            let mut wn = 0;
            let mut st = 0.0;
            for &(x, d) in &xs {
                if wn == 0 {
                    st = x;
                }
                wn += d;
                if wn == 0 {
                    ivs.push((st, x));
                }
            }
        } else {
            let mut k = 0;
            while k + 1 < xs.len() {
                ivs.push((xs[k].0, xs[k + 1].0));
                k += 2;
            }
        }
        for (a, bb) in ivs {
            let s = sat_i64((a - x0 - 0.5).ceil()).max(0);
            let e = sat_i64((bb - x0 - 0.5).ceil()).min(w as i64);
            if e > s {
                out[s as usize..e as usize].fill(255);
            }
        }
    });
    cov
}

/// Deckung einer Füllfläche wie das Original (Region → Maske `@00738aa0`): Zweig nach
/// `even_odd`/`agm`, ohne Kantenglättung harte Kante bzw. im AGM-Zweig AA-Maske `> 191`.
pub fn fill_coverage(polys: &[&[Vec2]], rect: IRect, even_odd: bool, agm: bool, antialias: bool) -> Coverage {
    let snapped;
    fn agm_polys(snapped: &[Vec<Vec2>]) -> Vec<&[Vec2]> {
        snapped.iter().map(|p| p.as_slice()).collect()
    }
    if !antialias {
        if even_odd && agm {
            snapped = agm_snap(polys);
            let mut g = coverage_agm(&agm_polys(&snapped), rect, false);
            g.data
                .iter_mut()
                .for_each(|v| *v = if *v > HARD_AGM_THRESHOLD { 255 } else { 0 });
            return g;
        }
        return coverage_hard(polys, rect, !even_odd);
    }
    match Rasterizer::select(even_odd, agm) {
        Rasterizer::Winding => coverage_winding(polys, rect),
        Rasterizer::Agm => {
            snapped = agm_snap(polys);
            coverage_agm(&agm_polys(&snapped), rect, false)
        }
        Rasterizer::Xor => coverage_xor(polys, rect),
    }
}

/// Grenzen (aufgerundet auf ganze Pixel) der Punkte aller Konturen; `None` ohne Punkte.
pub fn polys_bounds(polys: &[&[Vec2]]) -> Option<IRect> {
    let mut it = polys
        .iter()
        .flat_map(|p| p.iter())
        .filter(|v| v.x.is_finite() && v.y.is_finite());
    let f = it.next()?;
    let (mut x0, mut y0, mut x1, mut y1) = (f.x, f.y, f.x, f.y);
    for v in it {
        x0 = x0.min(v.x);
        y0 = y0.min(v.y);
        x1 = x1.max(v.x);
        y1 = y1.max(v.y);
    }
    let c = |v: f64| v.clamp(-1e9, 1e9) as i32;
    Some(IRect::new(
        c(x0.floor()),
        c(y0.floor()),
        c(x1.ceil()) + 1,
        c(y1.ceil()) + 1,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Vec2> {
        vec![
            Vec2::new(x0, y0),
            Vec2::new(x1, y0),
            Vec2::new(x1, y1),
            Vec2::new(x0, y1),
        ]
    }

    #[test]
    fn full_pixels_are_255() {
        let p = sq(1.0, 1.0, 5.0, 4.0);
        let r = IRect::new(0, 0, 6, 5);
        for m in [Rasterizer::Xor, Rasterizer::Winding, Rasterizer::Agm] {
            let c = fill_coverage(&[&p], r, m != Rasterizer::Winding, m == Rasterizer::Agm, true);
            assert_eq!(c.get(2, 2), 255, "{m:?}");
            assert_eq!(c.get(0, 0), 0);
            assert_eq!(c.get(5, 2), 0);
        }
        let h = fill_coverage(&[&p], r, true, false, false);
        assert_eq!(h.get(1, 1), 255);
        assert_eq!(h.get(5, 1), 0);
    }

    #[test]
    fn half_pixel_edges() {
        // Kante bei x = 1,5: Teilspalten 6..8 von Pixel 1 → 2·16 je Teilzeile, A: Teilzeile 0 −1
        let p = sq(1.5, 0.0, 3.0, 1.0);
        let r = IRect::new(0, 0, 4, 1);
        let a = coverage_xor(&[&p], r);
        assert_eq!(a.data, vec![0, 127, 255, 0]);
        let b = coverage_winding(&[&p], r);
        assert_eq!(b.data, vec![0, 128, 255, 0]);
    }

    #[test]
    fn xor_of_overlapping_contours() {
        let a = sq(0.0, 0.0, 4.0, 1.0);
        let b = sq(2.0, 0.0, 6.0, 1.0);
        let c = coverage_xor(&[&a, &b], IRect::new(0, 0, 6, 1));
        assert_eq!(c.data, vec![255, 255, 0, 0, 255, 255]);
        let w = coverage_winding(&[&a, &b], IRect::new(0, 0, 6, 1));
        assert_eq!(w.data, vec![255; 6]);
    }

    #[test]
    fn agm_is_conservative() {
        let p = sq(1.1, 0.0, 2.9, 1.0);
        let g = coverage_agm(&[&p], IRect::new(0, 0, 4, 1), false);
        assert_eq!(g.data, vec![0, 255, 255, 0]);
        assert_eq!(fw_round(-1.5), -2);
        assert_eq!(fw_round(1.5), 2);
    }
}
