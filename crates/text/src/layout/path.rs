//! Pfadgeometrie für Text auf Pfad und Text in Pfad: Bogenlänge, Punkt und Tangente,
//! waagerechte Spannen einer Form.

use crate::support::BezierContour;
use crate::support::ContourExt;

/// Abgeflachter Pfad mit kumulierter Bogenlänge.
#[derive(Debug, Clone, Default)]
pub struct PathSampler {
    pts: Vec<[f64; 2]>,
    cum: Vec<f64>,
}

impl PathSampler {
    /// Aus einer Bézier-Kontur (jedes Segment in 64 Teile, geschlossene Konturen mit Schlusssegment).
    pub fn new(c: &BezierContour, reversed: bool) -> Self {
        let c = if reversed { c.reversed() } else { c.clone() };
        let mut pts: Vec<[f64; 2]> = Vec::new();
        for s in c.segments() {
            if pts.is_empty() {
                pts.push([s.p0.x, s.p0.y]);
            }
            let steps = if s.is_line() { 1 } else { 64 };
            for k in 1..=steps {
                let p = s.eval(k as f64 / steps as f64);
                pts.push([p.x, p.y]);
            }
        }
        if pts.is_empty()
            && let Some(n) = c.nodes.first()
        {
            pts.push([n.pt.x as f64, n.pt.y as f64]);
        }
        let mut cum = Vec::with_capacity(pts.len());
        let mut acc = 0.0;
        for i in 0..pts.len() {
            if i > 0 {
                acc += (pts[i][0] - pts[i - 1][0]).hypot(pts[i][1] - pts[i - 1][1]);
            }
            cum.push(acc);
        }
        Self { pts, cum }
    }

    /// Gesamtlänge.
    pub fn length(&self) -> f64 {
        self.cum.last().copied().unwrap_or(0.0)
    }

    /// Kumulierte Länge der um `delta` zur Glyphenoberseite versetzten Kurve je Stützpunkt
    /// (Oberseite = Laufrichtung um −90° gedreht, im Dokument mit y nach unten also links).
    pub fn offset_cum(&self, delta: f64) -> Vec<f64> {
        let n = self.pts.len();
        if n < 2 || delta == 0.0 {
            return self.cum.clone();
        }
        let dir = |i: usize| -> [f64; 2] {
            let (a, b) = (self.pts[i], self.pts[i + 1]);
            let l = (b[0] - a[0]).hypot(b[1] - a[1]);
            if l > 0.0 {
                [(b[0] - a[0]) / l, (b[1] - a[1]) / l]
            } else {
                [0.0, 0.0]
            }
        };
        let q: Vec<[f64; 2]> = (0..n)
            .map(|i| {
                let d0 = if i > 0 { dir(i - 1) } else { dir(0) };
                let d1 = if i + 1 < n { dir(i) } else { dir(n - 2) };
                let (tx, ty) = (d0[0] + d1[0], d0[1] + d1[1]);
                let l = tx.hypot(ty);
                let (tx, ty) = if l > 1e-12 {
                    (tx / l, ty / l)
                } else {
                    (d1[0], d1[1])
                };
                [self.pts[i][0] + ty * delta, self.pts[i][1] - tx * delta]
            })
            .collect();
        let mut out = Vec::with_capacity(n);
        let mut acc = 0.0;
        for i in 0..n {
            if i > 0 {
                acc += (q[i][0] - q[i - 1][0]).hypot(q[i][1] - q[i - 1][1]);
            }
            out.push(acc);
        }
        out
    }

    /// Bogenlänge auf dem Pfad zu einer Länge `s` auf der versetzten Kurve (`offset_cum`).
    pub fn path_s(&self, ocum: &[f64], s: f64) -> f64 {
        if ocum.len() != self.cum.len() || ocum.len() < 2 {
            return s;
        }
        let last = ocum.len() - 1;
        if s <= 0.0 || ocum[last] <= 0.0 {
            return s * self.length() / ocum[last].max(1e-9);
        }
        if s >= ocum[last] {
            return self.cum[last] + (s - ocum[last]);
        }
        let i = match ocum.binary_search_by(|v| v.total_cmp(&s)) {
            Ok(i) => i.min(last - 1),
            Err(i) => i.saturating_sub(1).min(last - 1),
        };
        let seg = ocum[i + 1] - ocum[i];
        let t = if seg > 0.0 { (s - ocum[i]) / seg } else { 0.0 };
        self.cum[i] + (self.cum[i + 1] - self.cum[i]) * t
    }

    /// Punkt und Tangentenwinkel bei Bogenlänge `s` (außerhalb auf die Enden begrenzt).
    pub fn at(&self, s: f64) -> Option<([f64; 2], f64)> {
        if self.pts.len() < 2 {
            return self.pts.first().map(|p| (*p, 0.0));
        }
        let s = s.clamp(0.0, self.length());
        let i = match self.cum.binary_search_by(|v| v.total_cmp(&s)) {
            Ok(i) => i.min(self.pts.len() - 2),
            Err(i) => i.saturating_sub(1).min(self.pts.len() - 2),
        };
        // Strecke ohne Länge überspringen
        let mut j = i;
        while j + 1 < self.pts.len() && self.cum[j + 1] - self.cum[j] <= 0.0 {
            j += 1;
        }
        if j + 1 >= self.pts.len() {
            j = i;
        }
        let (a, b) = (self.pts[j], self.pts[j + 1]);
        let seg = self.cum[j + 1] - self.cum[j];
        let t = if seg > 0.0 {
            ((s - self.cum[j]) / seg).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let p = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
        Some((p, (b[1] - a[1]).atan2(b[0] - a[0])))
    }
}

/// Abgeflachte Polygone einer Form (für Text in Pfad).
pub fn polygons(contours: &[BezierContour]) -> Vec<Vec<[f64; 2]>> {
    contours
        .iter()
        .map(|c| {
            let mut pts = Vec::new();
            for s in c.segments() {
                if pts.is_empty() {
                    pts.push([s.p0.x, s.p0.y]);
                }
                let steps = if s.is_line() { 1 } else { 32 };
                for k in 1..=steps {
                    let p = s.eval(k as f64 / steps as f64);
                    pts.push([p.x, p.y]);
                }
            }
            pts
        })
        .collect()
}

/// Innenintervalle (Non-Zero) der Form auf der Höhe `y`, sortiert.
pub fn spans_at(polys: &[Vec<[f64; 2]>], y: f64) -> Vec<(f64, f64)> {
    let mut xs: Vec<(f64, i32)> = Vec::new();
    for p in polys {
        let n = p.len();
        for i in 0..n {
            let (a, b) = (p[i], p[(i + 1) % n]);
            if (a[1] <= y && y < b[1]) || (b[1] <= y && y < a[1]) {
                let x = a[0] + (y - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
                xs.push((x, if b[1] > a[1] { 1 } else { -1 }));
            }
        }
    }
    xs.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out = Vec::new();
    let mut w = 0;
    let mut start = 0.0;
    for (x, d) in xs {
        let before = w;
        w += d;
        if before == 0 && w != 0 {
            start = x;
        } else if before != 0 && w == 0 {
            out.push((start, x));
        }
    }
    out
}

/// Gemeinsames Innenintervall einer Zeile im Band `[top, bottom]` (Schnitt der Spannen oben, Mitte,
/// unten; das breiteste Ergebnis). `None`, wenn die Form das Band nicht ganz enthält.
pub fn band_span(polys: &[Vec<[f64; 2]>], top: f64, bottom: f64) -> Option<(f64, f64)> {
    let ys = [top + 1e-6, (top + bottom) / 2.0, bottom - 1e-6];
    let mut cur: Vec<(f64, f64)> = spans_at(polys, ys[0]);
    for &y in &ys[1..] {
        let next = spans_at(polys, y);
        let mut inter = Vec::new();
        for &(a0, a1) in &cur {
            for &(b0, b1) in &next {
                let (l, r) = (a0.max(b0), a1.min(b1));
                if r > l {
                    inter.push((l, r));
                }
            }
        }
        cur = inter;
    }
    cur.into_iter().max_by(|a, b| (a.1 - a.0).total_cmp(&(b.1 - b.0)))
}

/// y-Ausdehnung der Form.
pub fn y_range(polys: &[Vec<[f64; 2]>]) -> Option<(f64, f64)> {
    let mut it = polys.iter().flatten();
    let f = it.next()?;
    Some(it.fold((f[1], f[1]), |(lo, hi), p| (lo.min(p[1]), hi.max(p[1]))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::support::{BezierNode, Point};

    #[test]
    fn line_sampler() {
        let c = BezierContour::new(
            vec![
                BezierNode::corner(Point::new(0.0, 0.0)),
                BezierNode::corner(Point::new(100.0, 0.0)),
            ],
            false,
        );
        let s = PathSampler::new(&c, false);
        assert!((s.length() - 100.0).abs() < 1e-9);
        let (p, a) = s.at(25.0).unwrap();
        assert!((p[0] - 25.0).abs() < 1e-9 && a.abs() < 1e-12);
        let r = PathSampler::new(&c, true);
        let (p, a) = r.at(25.0).unwrap();
        assert!((p[0] - 75.0).abs() < 1e-9 && (a.abs() - std::f64::consts::PI).abs() < 1e-9);
    }

    #[test]
    fn rect_spans() {
        let polys = vec![vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]];
        assert_eq!(spans_at(&polys, 5.0), vec![(0.0, 10.0)]);
        assert_eq!(band_span(&polys, 1.0, 4.0), Some((0.0, 10.0)));
        assert_eq!(band_span(&polys, 8.0, 12.0), None);
    }
}
