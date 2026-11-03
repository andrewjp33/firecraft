//! Abflachung Bézier → Polylinie.
//!
//! # Original-Abflachung (`PCLv`), Spezifikation 16 §1.4 (B)
//!
//! Diese Punkte schreibt das Original als `PCLv` und rastert sie (07 §3). Referenz
//! `re\pfade\flatten.py`: 28.095 von 28.099 Korpus-Konturen punktgenau.
//!
//! 1. **Vorteilung**: Segment gleichmäßig per de-Casteljau-Halbierung in N Teilkurven,
//!    N = größte Zweierpotenz ≤ `BZCi` des Startknotens (im Korpus 1 oder 64).
//! 2. **Je Teilkurve adaptive Halbierung** mit Stapel, linke Hälfte zuerst, höchstens Tiefe 6:
//!    - Gerade mit eingezogenen Henkeln (P0 = P1 und P2 = P3): nicht teilen.
//!    - d = P0 − P3; Schnellnorm L = max(|dx|,|dy|) + min/2, und wenn |dy| < |dx| zusätzlich
//!      + 0,5 (Eigenheit: `(|dy| + 1)·0,5`).
//!    - Toleranz = 3/64·L (bei L = 0: −1, also immer teilen).
//!    - Flach, wenn |dy·(x0 − x1) − dx·(y0 − y1)| ≤ Toleranz und dasselbe für P2.
//!    - Halbierung in `f32` (Mittelwerte als `float` gespeichert), Test in `f64`.
//! 3. Ausgabe: Startpunkt, dann der Endpunkt jedes flachen Stücks.
//!
//! # Ältere Sehnen-Abflachung des Rasterizers, Spezifikation 07 §3 (A)
//!
//! Vor dem Fund der echten Abflachung benutzte die Rasterizer-Referenz (`re\vektor\fwraster.py`)
//! eine rekursive Halbierung, bis beide Kontrollpunkte höchstens 0,054 px von der Sehne entfernt
//! sind (Tiefe ≤ 10). Sie ist durch die Original-Abflachung ersetzt (16 §7) und hier nur für
//! Vergleiche mit der alten Referenz enthalten: [`flatten_chord`], [`flatten_contour_chord`].

use crate::support::geom::{BezierContour, Point, Polyline};

/// Kubisches Segment als acht `f32`-Werte `x0, y0, x1, y1, x2, y2, x3, y3`.
pub type Cubic32 = [f32; 8];

/// Einstellungen der Original-Abflachung.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlattenOptions {
    /// Faktor auf die Schnellnorm (Original: 3/64 = 0,046875, solange das globale Flag
    /// `DAT_01b89338` = 0 ist; sonst 1,0).
    pub factor: f64,
    /// Höchste Halbierungstiefe je Teilkurve (Original: 6).
    pub max_depth: u32,
    /// Schnellnorm-Eigenheit „+0,5, wenn |dy| < |dx|“ (Original: an; ohne sie stimmen nur 84 %).
    pub quirk: bool,
}

impl Default for FlattenOptions {
    fn default() -> Self {
        Self::ORIGINAL
    }
}

impl FlattenOptions {
    /// Die Werte des Originals.
    pub const ORIGINAL: FlattenOptions = FlattenOptions {
        factor: 0.046875,
        max_depth: 6,
        quirk: true,
    };
}

#[inline]
fn mid(a: f32, b: f32) -> f32 {
    ((a as f64 + b as f64) * 0.5) as f32
}

#[inline]
fn is_straight(c: &Cubic32) -> bool {
    c[0] == c[2] && c[1] == c[3] && c[6] == c[4] && c[7] == c[5]
}

/// Halbierung bei t = 0,5 in `f32` (im Original `@00518860`). Geraden mit eingezogenen Henkeln
/// werden linear geteilt und behalten eingezogene Henkel.
pub fn halve(c: &Cubic32) -> (Cubic32, Cubic32) {
    let [x0, y0, x1, y1, x2, y2, x3, y3] = *c;
    if is_straight(c) {
        let (mx, my) = (mid(x3, x0), mid(y3, y0));
        return ([x0, y0, x1, y1, mx, my, mx, my], [mx, my, mx, my, x2, y2, x3, y3]);
    }
    let (bx, by) = (mid(x2, x1), mid(y2, y1));
    let (ax, ay) = (mid(x0, x1), mid(y0, y1));
    let (cx, cy) = (mid(x3, x2), mid(y3, y2));
    let (dx, dy) = (mid(ax, bx), mid(ay, by));
    let (ex, ey) = (mid(cx, bx), mid(cy, by));
    let (mx, my) = (mid(dx, ex), mid(dy, ey));
    ([x0, y0, ax, ay, dx, dy, mx, my], [mx, my, ex, ey, cx, cy, x3, y3])
}

/// Schnellnorm von (dx, dy): max + min/2, mit Eigenheit (+0,5 bei |dy| < |dx|).
pub fn fast_len(dx: f64, dy: f64, quirk: bool) -> f64 {
    let (ax, ay) = (dx.abs(), dy.abs());
    if ay < ax {
        ax + (if quirk { ay + 1.0 } else { ay }) * 0.5
    } else {
        ay + ax * 0.5
    }
}

/// Flachheitstest (Kreuzprodukt beider Kontrollpunkte mit der Sehne gegen `factor`·Schnellnorm).
pub fn is_flat(c: &Cubic32, opts: &FlattenOptions) -> bool {
    let [x0, y0, x1, y1, x2, y2, x3, y3] = c.map(|v| v as f64);
    let dx = x0 - x3;
    let dy = y0 - y3;
    let l = fast_len(dx, dy, opts.quirk);
    let tol = if l == 0.0 { -1.0 } else { l * opts.factor };
    let c1 = (dy * (x0 - x1) - dx * (y0 - y1)).abs();
    // `!(c1 <= tol)` statt `c1 > tol`, damit NaN als „nicht flach“ gilt (wie die Referenz)
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    if !(c1 <= tol) {
        return false;
    }
    let c2 = (dy * (x0 - x2) - dx * (y0 - y2)).abs();
    c2 <= tol
}

/// Punkte einer Teilkurve ohne Startpunkt (Stapelverfahren wie `@00519a30`), angehängt an `out`.
pub fn flatten_piece(c: &Cubic32, opts: &FlattenOptions, out: &mut Vec<Point>) {
    let mut stack: Vec<(Cubic32, u32)> = Vec::with_capacity(opts.max_depth as usize + 1);
    let mut cur = *c;
    let mut depth = 0u32;
    loop {
        if depth < opts.max_depth && !is_straight(&cur) && !is_flat(&cur, opts) {
            let (left, right) = halve(&cur);
            depth += 1;
            stack.push((right, depth));
            cur = left;
            continue;
        }
        out.push(Point::new(cur[6], cur[7]));
        match stack.pop() {
            Some((c, d)) => {
                cur = c;
                depth = d;
            }
            None => return,
        }
    }
}

/// Gleichmäßige Vorteilung in die größte Zweierpotenz ≤ `n` Teilkurven (n = `BZCi`).
pub fn presplit(c: &Cubic32, n: u32) -> Vec<Cubic32> {
    let mut pieces = vec![*c];
    // Obergrenze 2^16 Teilkurven gegen unsinnige Dateiwerte
    while (pieces.len() as u64) * 2 <= n as u64 && pieces.len() < (1 << 16) {
        let mut next = Vec::with_capacity(pieces.len() * 2);
        for p in &pieces {
            let (a, b) = halve(p);
            next.push(a);
            next.push(b);
        }
        pieces = next;
    }
    pieces
}

/// Ein Segment abflachen; hängt die Punkte **ohne** Startpunkt an `out` an.
/// `bzc` = Zahl der Druck-Stützstellen des Startknotens (`BZCi`, fehlt = 1).
pub fn flatten_segment(p: [Point; 4], bzc: u32, opts: &FlattenOptions, out: &mut Vec<Point>) {
    let c: Cubic32 = [p[0].x, p[0].y, p[1].x, p[1].y, p[2].x, p[2].y, p[3].x, p[3].y];
    for piece in presplit(&c, bzc) {
        flatten_piece(&piece, opts, out);
    }
}

/// Eine Kontur abflachen wie das Original (= `PCLv`). Ergebnis beginnt mit dem ersten Anker; bei
/// geschlossenen Konturen endet es wieder am Startpunkt. `counts[i]` = `BZCi` des Knotens i
/// (fehlende Einträge = 1).
pub fn flatten_contour(contour: &BezierContour, counts: Option<&[u32]>, opts: &FlattenOptions) -> Polyline {
    let mut pts = Vec::new();
    if let Some(first) = contour.nodes.first() {
        pts.push(first.pt);
        for i in 0..contour.segment_count() {
            if let Some(seg) = contour.segment_points(i) {
                let n = counts.and_then(|c| c.get(i)).copied().unwrap_or(1);
                flatten_segment(seg, n, opts, &mut pts);
            }
        }
    }
    Polyline::new(pts, contour.closed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::geom::BezierNode;

    #[test]
    fn straight_segment_gives_endpoint() {
        let mut out = Vec::new();
        let p = [
            Point::new(0.0, 0.0),
            Point::new(0.0, 0.0),
            Point::new(10.0, 5.0),
            Point::new(10.0, 5.0),
        ];
        flatten_segment(p, 1, &FlattenOptions::ORIGINAL, &mut out);
        assert_eq!(out, vec![Point::new(10.0, 5.0)]);
        // mit BZCi = 64: 64 gleichmäßige Stücke
        out.clear();
        flatten_segment(p, 64, &FlattenOptions::ORIGINAL, &mut out);
        assert_eq!(out.len(), 64);
        assert_eq!(out[31], Point::new(5.0, 2.5));
    }

    #[test]
    fn curve_depth_bound_and_closure() {
        let n =
            |x, y, ix, iy, ox, oy| BezierNode::new(Point::new(x, y), Point::new(ix, iy), Point::new(ox, oy));
        let c = BezierContour::new(
            vec![
                n(0.0, 50.0, 0.0, 77.6, 0.0, 22.4),
                n(50.0, 0.0, 22.4, 0.0, 77.6, 0.0),
                n(100.0, 50.0, 100.0, 22.4, 100.0, 77.6),
            ],
            true,
        );
        let p = flatten_contour(&c, None, &FlattenOptions::ORIGINAL);
        assert_eq!(p.points.first(), p.points.last());
        // höchstens 2^6 Punkte je Segment
        assert!(p.points.len() <= 1 + 3 * 64);
        assert!(p.points.len() > 10);
        // L = 0 (Start = Ende) wird immer geteilt bis Tiefe 6
        let mut out = Vec::new();
        let loop_seg = [
            Point::new(0.0, 0.0),
            Point::new(10.0, 10.0),
            Point::new(-10.0, 10.0),
            Point::new(0.0, 0.0),
        ];
        flatten_segment(loop_seg, 1, &FlattenOptions::ORIGINAL, &mut out);
        assert!(out.len() >= 2);
    }

    #[test]
    fn presplit_counts() {
        let c: Cubic32 = [0.0, 0.0, 1.0, 2.0, 3.0, 2.0, 4.0, 0.0];
        assert_eq!(presplit(&c, 0).len(), 1);
        assert_eq!(presplit(&c, 1).len(), 1);
        assert_eq!(presplit(&c, 3).len(), 2);
        assert_eq!(presplit(&c, 64).len(), 64);
        assert_eq!(presplit(&c, 100).len(), 64);
    }
}
