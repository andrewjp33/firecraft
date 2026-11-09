//! Glyphenkonturen in Font-Einheiten (y nach oben) und ihre Abflachung.

use crate::support::{BezierContour, BezierNode, Point};

/// Punkt in Font-Einheiten bzw. Pixeln (`f64`).
pub type P2 = [f64; 2];

/// Segment einer Glyphenkontur.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    Line(P2, P2),
    Quad(P2, P2, P2),
    Cubic(P2, P2, P2, P2),
}

impl Segment {
    /// Alle Punkte mit `f` abgebildet.
    pub fn map(&self, f: impl Fn(P2) -> P2) -> Segment {
        match *self {
            Segment::Line(a, b) => Segment::Line(f(a), f(b)),
            Segment::Quad(a, b, c) => Segment::Quad(f(a), f(b), f(c)),
            Segment::Cubic(a, b, c, d) => Segment::Cubic(f(a), f(b), f(c), f(d)),
        }
    }

    /// Startpunkt.
    pub fn start(&self) -> P2 {
        match *self {
            Segment::Line(a, _) | Segment::Quad(a, _, _) | Segment::Cubic(a, _, _, _) => a,
        }
    }

    /// Endpunkt.
    pub fn end(&self) -> P2 {
        match *self {
            Segment::Line(_, b) | Segment::Quad(_, _, b) | Segment::Cubic(_, _, _, b) => b,
        }
    }
}

/// Kontur einer Glyphe: geschlossene Folge von Segmenten.
pub type Contour = Vec<Segment>;

/// Umriss einer Glyphe in Font-Einheiten (y nach oben) mit vorab berechneter Tinten-Box.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GlyphOutline {
    pub contours: Vec<Contour>,
    /// (xMin, xMax, yMin, yMax) der abgeflachten Kontur (Toleranz 0,5 Font-Einheiten wie die
    /// Python-Referenz `layout.ink_x`); `None` bei leerer Glyphe.
    pub ink: Option<[f64; 4]>,
    /// Punkte der abgeflachten Kontur (Toleranz 0,5), für die Tinten-Box geschert (Faux-Kursiv).
    pub ink_points: Vec<P2>,
}

impl GlyphOutline {
    /// Aus Konturen; berechnet die Tinten-Box.
    pub fn new(contours: Vec<Contour>) -> Self {
        let mut pts = Vec::new();
        for c in &contours {
            flatten_contour(c, 0.5, &mut |a, b| {
                pts.push(a);
                pts.push(b);
            });
        }
        pts.dedup();
        let ink = bbox(&pts);
        Self {
            contours,
            ink,
            ink_points: pts,
        }
    }

    /// Ist die Glyphe leer (z. B. Leerzeichen)?
    pub fn is_empty(&self) -> bool {
        self.contours.is_empty()
    }

    /// x-Ausdehnung der Tinte nach der Scherung x' = x + tan·y (Faux-Kursiv), Font-Einheiten.
    pub fn ink_x_sheared(&self, tan: f64) -> Option<(f64, f64)> {
        if tan == 0.0 {
            return self.ink.map(|b| (b[0], b[1]));
        }
        let mut it = self.ink_points.iter().map(|p| p[0] + tan * p[1]);
        let first = it.next()?;
        Some(it.fold((first, first), |(lo, hi), v| (lo.min(v), hi.max(v))))
    }
}

fn bbox(pts: &[P2]) -> Option<[f64; 4]> {
    let first = pts.first()?;
    let mut b = [first[0], first[0], first[1], first[1]];
    for p in pts {
        b[0] = b[0].min(p[0]);
        b[1] = b[1].max(p[0]);
        b[2] = b[2].min(p[1]);
        b[3] = b[3].max(p[1]);
    }
    Some(b)
}

/// Unterteilungszahl einer Kurve aus der Länge des Kontrollpolygons (wie `raster.flatten` der
/// Python-Referenz): `n = clamp(ceil(sqrt(L / tol)), 2, 64)`.
fn subdivisions(pts: &[P2], tol: f64) -> usize {
    let mut len = 0.0;
    for w in pts.windows(2) {
        len += (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1]);
    }
    let n = (len / tol).sqrt().ceil();
    if n.is_nan() { 2 } else { (n as usize).clamp(2, 64) }
}

/// Flacht eine Kontur zu Strecken ab (Aufruf `line(a, b)` je Strecke). Kurven werden gleichmäßig in
/// `t` unterteilt; die Auswertung folgt der Bernstein-Form der Python-Referenz.
pub fn flatten_contour(c: &Contour, tol: f64, line: &mut impl FnMut(P2, P2)) {
    for s in c {
        flatten_segment(s, tol, line);
    }
}

/// Flacht ein Segment ab.
pub fn flatten_segment(s: &Segment, tol: f64, line: &mut impl FnMut(P2, P2)) {
    match *s {
        Segment::Line(a, b) => line(a, b),
        Segment::Quad(p0, p1, p2) => {
            let n = subdivisions(&[p0, p1, p2], tol);
            let mut prev = p0;
            for k in 1..=n {
                let t = k as f64 / n as f64;
                let u = 1.0 - t;
                let x = u * u * p0[0] + 2.0 * u * t * p1[0] + t * t * p2[0];
                let y = u * u * p0[1] + 2.0 * u * t * p1[1] + t * t * p2[1];
                line(prev, [x, y]);
                prev = [x, y];
            }
        }
        Segment::Cubic(p0, p1, p2, p3) => {
            let n = subdivisions(&[p0, p1, p2, p3], tol);
            let mut prev = p0;
            for k in 1..=n {
                let t = k as f64 / n as f64;
                let u = 1.0 - t;
                let (u3, t3) = (u.powi(3), t.powi(3));
                let x = u3 * p0[0] + 3.0 * u * u * t * p1[0] + 3.0 * u * t * t * p2[0] + t3 * p3[0];
                let y = u3 * p0[1] + 3.0 * u * u * t * p1[1] + 3.0 * u * t * t * p2[1] + t3 * p3[1];
                line(prev, [x, y]);
                prev = [x, y];
            }
        }
    }
}

/// Sammelt Konturen über ttf-parsers `OutlineBuilder`. Offene Konturen werden mit einer Strecke zum
/// Startpunkt geschlossen.
#[derive(Default)]
pub(crate) struct Collector {
    pub contours: Vec<Contour>,
    cur: Vec<Segment>,
    start: P2,
    pos: P2,
    open: bool,
}

impl Collector {
    fn finish(&mut self) {
        if self.open {
            if self.pos != self.start {
                self.cur.push(Segment::Line(self.pos, self.start));
            }
            if !self.cur.is_empty() {
                self.contours.push(std::mem::take(&mut self.cur));
            }
            self.cur.clear();
            self.open = false;
        }
    }

    pub fn into_contours(mut self) -> Vec<Contour> {
        self.finish();
        self.contours
    }
}

impl ttf_parser::OutlineBuilder for Collector {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish();
        self.start = [x as f64, y as f64];
        self.pos = self.start;
        self.open = true;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = [x as f64, y as f64];
        self.cur.push(Segment::Line(self.pos, p));
        self.pos = p;
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let p = [x as f64, y as f64];
        self.cur.push(Segment::Quad(self.pos, [x1 as f64, y1 as f64], p));
        self.pos = p;
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let p = [x as f64, y as f64];
        self.cur.push(Segment::Cubic(
            self.pos,
            [x1 as f64, y1 as f64],
            [x2 as f64, y2 as f64],
            p,
        ));
        self.pos = p;
    }
    fn close(&mut self) {
        self.finish();
    }
}

/// Wandelt eine Kontur (bereits in Dokumentkoordinaten) in eine geschlossene fp-geom-Bézier-Kontur
/// um. Quadratische Segmente werden exakt in kubische angehoben.
pub fn to_bezier_contour(c: &Contour) -> BezierContour {
    let pt = |p: P2| Point::new(p[0] as f32, p[1] as f32);
    // je Segment: (Start, Ausgangshenkel des Starts, Eingangshenkel des Endes)
    let segs: Vec<(P2, P2, P2)> = c
        .iter()
        .map(|s| match *s {
            Segment::Line(a, b) => (a, a, b),
            Segment::Quad(a, q, b) => {
                let c1 = [a[0] + 2.0 / 3.0 * (q[0] - a[0]), a[1] + 2.0 / 3.0 * (q[1] - a[1])];
                let c2 = [b[0] + 2.0 / 3.0 * (q[0] - b[0]), b[1] + 2.0 / 3.0 * (q[1] - b[1])];
                (a, c1, c2)
            }
            Segment::Cubic(a, c1, c2, _) => (a, c1, c2),
        })
        .collect();
    let n = segs.len();
    let nodes = (0..n)
        .map(|i| {
            let (a, out, _) = segs[i];
            let (_, _, inc) = segs[(i + n - 1) % n];
            BezierNode::new(pt(a), pt(inc), pt(out))
        })
        .collect();
    BezierContour::new(nodes, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_ink_and_contour() {
        let sq = vec![
            Segment::Line([0.0, 0.0], [10.0, 0.0]),
            Segment::Line([10.0, 0.0], [10.0, 20.0]),
            Segment::Line([10.0, 20.0], [0.0, 0.0]),
        ];
        let o = GlyphOutline::new(vec![sq.clone()]);
        assert_eq!(o.ink, Some([0.0, 10.0, 0.0, 20.0]));
        assert_eq!(o.ink_x_sheared(0.5), Some((0.0, 20.0)));
        let bc = to_bezier_contour(&sq);
        assert_eq!(bc.nodes.len(), 3);
        assert!(bc.closed);
        assert_eq!(bc.nodes[1].pt, Point::new(10.0, 0.0));
        assert!(bc.nodes[1].is_corner());
    }

    #[test]
    fn quad_elevation_keeps_midpoint() {
        let c = vec![
            Segment::Quad([0.0, 0.0], [5.0, 10.0], [10.0, 0.0]),
            Segment::Line([10.0, 0.0], [0.0, 0.0]),
        ];
        let bc = to_bezier_contour(&c);
        let seg = crate::support::ContourExt::segments(&bc)
            .first()
            .copied()
            .unwrap();
        let m = seg.eval(0.5);
        assert!((m.x - 5.0).abs() < 1e-5 && (m.y - 5.0).abs() < 1e-5);
    }
}
