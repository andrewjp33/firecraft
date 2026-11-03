//! The few items FirePixel's stroke modules used from its `fp-geom`, `fp-color` and
//! `fp-raster` crates, copied from FirePixel (Apache-2.0) where noted; `Mask`, `RgbaImage` and
//! the contour types are reduced to what the stroke engine uses.
#![allow(dead_code)]

pub mod geom {
    use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

    // fp-geom/src/lib.rs
    #[derive(Debug, Clone, Copy, PartialEq, Default)]
    pub struct Point {
        pub x: f32,
        pub y: f32,
    }

    impl Point {
        pub const fn new(x: f32, y: f32) -> Self {
            Self { x, y }
        }
    }

    /// Achsenparalleles Rechteck (links/oben inklusiv, rechts/unten exklusiv bei Pixelrastern).

    // fp-geom/src/vec2.rs

    /// Punkt bzw. Vektor mit `f64`-Koordinaten.
    #[derive(Debug, Clone, Copy, PartialEq, Default)]
    pub struct Vec2 {
        pub x: f64,
        pub y: f64,
    }

    impl Vec2 {
        /// Nullvektor.
        pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

        pub const fn new(x: f64, y: f64) -> Self {
            Self { x, y }
        }

        /// Skalarprodukt.
        #[inline]
        pub fn dot(self, o: Vec2) -> f64 {
            self.x * o.x + self.y * o.y
        }

        /// Kreuzprodukt (z-Komponente): `self.x·o.y − self.y·o.x`.
        #[inline]
        pub fn cross(self, o: Vec2) -> f64 {
            self.x * o.y - self.y * o.x
        }

        /// Euklidische Länge (`hypot`).
        #[inline]
        pub fn length(self) -> f64 {
            self.x.hypot(self.y)
        }

        /// Quadrat der Länge.
        #[inline]
        pub fn length_sq(self) -> f64 {
            self.x * self.x + self.y * self.y
        }

        /// Abstand zu `o`.
        #[inline]
        pub fn distance(self, o: Vec2) -> f64 {
            (self.x - o.x).hypot(self.y - o.y)
        }

        /// Lineare Interpolation `self + (o − self)·t` (Formel wie in den Python-Referenzen).
        #[inline]
        pub fn lerp(self, o: Vec2, t: f64) -> Vec2 {
            Vec2::new(self.x + (o.x - self.x) * t, self.y + (o.y - self.y) * t)
        }

        /// Einheitsvektor; Länge ≤ 1e-12 ergibt den Nullvektor.
        pub fn normalize(self) -> Vec2 {
            let l = self.length();
            if l > 1e-12 {
                Vec2::new(self.x / l, self.y / l)
            } else {
                Vec2::ZERO
            }
        }

        /// Senkrechte „links“ im mathematischen Sinn: `(−y, x)`.
        #[inline]
        pub fn perp(self) -> Vec2 {
            Vec2::new(-self.y, self.x)
        }

        /// Umwandlung in einen `f32`-Punkt.
        #[inline]
        pub fn to_point(self) -> Point {
            Point::new(self.x as f32, self.y as f32)
        }

        /// Beide Koordinaten endlich?
        #[inline]
        pub fn is_finite(self) -> bool {
            self.x.is_finite() && self.y.is_finite()
        }
    }

    impl From<Point> for Vec2 {
        #[inline]
        fn from(p: Point) -> Self {
            Vec2::new(p.x as f64, p.y as f64)
        }
    }

    impl From<Vec2> for Point {
        #[inline]
        fn from(v: Vec2) -> Self {
            v.to_point()
        }
    }

    impl From<(f64, f64)> for Vec2 {
        #[inline]
        fn from((x, y): (f64, f64)) -> Self {
            Vec2::new(x, y)
        }
    }

    impl Add for Vec2 {
        type Output = Vec2;
        #[inline]
        fn add(self, o: Vec2) -> Vec2 {
            Vec2::new(self.x + o.x, self.y + o.y)
        }
    }

    impl AddAssign for Vec2 {
        #[inline]
        fn add_assign(&mut self, o: Vec2) {
            self.x += o.x;
            self.y += o.y;
        }
    }

    impl Sub for Vec2 {
        type Output = Vec2;
        #[inline]
        fn sub(self, o: Vec2) -> Vec2 {
            Vec2::new(self.x - o.x, self.y - o.y)
        }
    }

    impl SubAssign for Vec2 {
        #[inline]
        fn sub_assign(&mut self, o: Vec2) {
            self.x -= o.x;
            self.y -= o.y;
        }
    }

    impl Mul<f64> for Vec2 {
        type Output = Vec2;
        #[inline]
        fn mul(self, s: f64) -> Vec2 {
            Vec2::new(self.x * s, self.y * s)
        }
    }

    impl Div<f64> for Vec2 {
        type Output = Vec2;
        #[inline]
        fn div(self, s: f64) -> Vec2 {
            Vec2::new(self.x / s, self.y / s)
        }
    }

    impl Neg for Vec2 {
        type Output = Vec2;
        #[inline]
        fn neg(self) -> Vec2 {
            Vec2::new(-self.x, -self.y)
        }
    }

    fn round_away(x: f64) -> f64 {
        crate::support::color::round_away(x)
    }

    // fp-geom/src/ops.rs (IRect)
    // ------------------------------------------------------------------ IRect

    /// Ganzzahliges Pixelrechteck, links/oben inklusiv, rechts/unten exklusiv.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
    pub struct IRect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    impl IRect {
        pub const fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
            Self {
                left,
                top,
                right,
                bottom,
            }
        }

        /// Rechteck aus Ursprung und Größe.
        pub const fn from_xywh(x: i32, y: i32, w: i32, h: i32) -> Self {
            Self::new(x, y, x.saturating_add(w), y.saturating_add(h))
        }

        /// Breite (0, wenn leer).
        pub fn width(&self) -> u32 {
            (self.right as i64 - self.left as i64).max(0) as u32
        }

        /// Höhe (0, wenn leer).
        pub fn height(&self) -> u32 {
            (self.bottom as i64 - self.top as i64).max(0) as u32
        }

        /// Fläche in Pixeln.
        pub fn area(&self) -> u64 {
            self.width() as u64 * self.height() as u64
        }

        pub fn is_empty(&self) -> bool {
            self.right <= self.left || self.bottom <= self.top
        }

        /// Vereinigung; leere Rechtecke sind neutral.
        pub fn union(&self, o: &IRect) -> IRect {
            if self.is_empty() {
                return *o;
            }
            if o.is_empty() {
                return *self;
            }
            IRect::new(
                self.left.min(o.left),
                self.top.min(o.top),
                self.right.max(o.right),
                self.bottom.max(o.bottom),
            )
        }

        /// Schnittmenge; leer ergibt `IRect::default()`.
        pub fn intersect(&self, o: &IRect) -> IRect {
            let r = IRect::new(
                self.left.max(o.left),
                self.top.max(o.top),
                self.right.min(o.right),
                self.bottom.min(o.bottom),
            );
            if r.is_empty() { IRect::default() } else { r }
        }

        /// Vergrößert um `d` Pixel auf jeder Seite (negativ = verkleinern).
        pub fn inflate(&self, dx: i32, dy: i32) -> IRect {
            IRect::new(
                self.left.saturating_sub(dx),
                self.top.saturating_sub(dy),
                self.right.saturating_add(dx),
                self.bottom.saturating_add(dy),
            )
        }

        /// Verschiebt um (`dx`, `dy`).
        pub fn translate(&self, dx: i32, dy: i32) -> IRect {
            IRect::new(
                self.left.saturating_add(dx),
                self.top.saturating_add(dy),
                self.right.saturating_add(dx),
                self.bottom.saturating_add(dy),
            )
        }

        /// Pixel (x, y) enthalten?
        pub fn contains(&self, x: i32, y: i32) -> bool {
            x >= self.left && x < self.right && y >= self.top && y < self.bottom
        }
    }

    // fp-geom/src/contour.rs (BezierNode; BezierContour and Polyline reduced)

    #[derive(Debug, Clone, Copy, PartialEq, Default)]
    pub struct BezierNode {
        /// Anker (`XLC/YLC`).
        pub pt: Point,
        /// Eingangshenkel (pred, `XPC/YPC`).
        pub ctrl_in: Point,
        /// Ausgangshenkel (succ, `XSC/YSC`).
        pub ctrl_out: Point,
    }

    impl BezierNode {
        pub const fn new(pt: Point, ctrl_in: Point, ctrl_out: Point) -> Self {
            Self {
                pt,
                ctrl_in,
                ctrl_out,
            }
        }

        /// Eckpunkt ohne Henkel (beide Henkel auf dem Anker).
        pub const fn corner(pt: Point) -> Self {
            Self {
                pt,
                ctrl_in: pt,
                ctrl_out: pt,
            }
        }

        /// Liegen beide Henkel auf dem Anker?
        pub fn is_corner(&self) -> bool {
            self.ctrl_in == self.pt && self.ctrl_out == self.pt
        }

        /// Ein- und Ausgangshenkel getauscht (für die Richtungsumkehr).
        pub fn swapped(&self) -> BezierNode {
            BezierNode::new(self.pt, self.ctrl_out, self.ctrl_in)
        }
    }

    #[derive(Debug, Clone, PartialEq, Default)]
    pub struct BezierContour {
        pub nodes: Vec<BezierNode>,
        pub closed: bool,
    }

    impl BezierContour {
        pub fn new(nodes: Vec<BezierNode>, closed: bool) -> Self {
            Self { nodes, closed }
        }

        /// Anzahl der Segmente (`n` geschlossen, `n − 1` offen, 0 bei weniger als 2 Knoten offen).
        pub fn segment_count(&self) -> usize {
            let n = self.nodes.len();
            if n == 0 {
                0
            } else if self.closed {
                n
            } else {
                n - 1
            }
        }

        /// Segment `i` als `f32`-Kontrollpunkte `[p0, p1, p2, p3]`; `None` außerhalb des Bereichs.
        pub fn segment_points(&self, i: usize) -> Option<[Point; 4]> {
            if i >= self.segment_count() {
                return None;
            }
            let a = &self.nodes[i];
            let b = &self.nodes[(i + 1) % self.nodes.len()];
            Some([a.pt, a.ctrl_out, b.ctrl_in, b.pt])
        }
    }

    #[derive(Debug, Clone, PartialEq, Default)]
    pub struct Polyline {
        pub points: Vec<Point>,
        pub closed: bool,
    }

    impl Polyline {
        pub fn new(points: Vec<Point>, closed: bool) -> Self {
            Self { points, closed }
        }
    }
}

pub mod color {
    /// Colour as in the file format: 0xAARRGGBB, not premultiplied (fp-color/src/lib.rs).
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
    pub struct Argb(pub u32);

    impl Argb {
        pub const fn a(self) -> u8 {
            (self.0 >> 24) as u8
        }
        pub const fn r(self) -> u8 {
            (self.0 >> 16) as u8
        }
        pub const fn g(self) -> u8 {
            (self.0 >> 8) as u8
        }
        pub const fn b(self) -> u8 {
            self.0 as u8
        }
    }

    // fp-color/src/round.rs

    /// Halbe Werte weg von null runden.
    #[inline]
    pub fn round_away(x: f64) -> f64 {
        if x >= 0.0 {
            (x + 0.5).floor()
        } else {
            (x - 0.5).ceil()
        }
    }

    /// Halbe Werte zur geraden Zahl runden (x87-`fistp`).
    #[inline]
    pub fn round_even(x: f64) -> f64 {
        x.round_ties_even()
    }

    /// Richtung null abschneiden, als `i64` (nicht endlich → 0, sonst gesättigt).
    #[inline]
    pub fn trunc(x: f64) -> i64 {
        if x.is_finite() { x as i64 } else { 0 }
    }

    /// C-Ganzzahldivision (Richtung null); Division durch 0 ergibt 0.
    #[inline]
    pub fn cdiv(a: i64, b: i64) -> i64 {
        if b == 0 { 0 } else { a.wrapping_div(b) }
    }

    /// `round(a·b/255)` in der Original-Form `t = a·b + 128; (t + (t>>8)) >> 8`.
    #[inline]
    pub const fn mul255(a: u32, b: u32) -> u32 {
        let t = a * b + 128;
        (t + (t >> 8)) >> 8
    }

    /// `round(v/255)` in der Original-Form (`div255` aus 05 §2.5) für `v ≥ 0`.
    #[inline]
    pub const fn div255(v: u32) -> u32 {
        let t = v + 128;
        ((t >> 8) + t) >> 8
    }

    const fn build_div() -> [[u8; 256]; 256] {
        let mut t = [[0u8; 256]; 256];
        let mut a = 0;
        while a < 256 {
            let mut b = 0;
            while b < 256 {
                if b != 0 && b >= a {
                    t[a][b] = (255 * a / b) as u8;
                }
                b += 1;
            }
            a += 1;
        }
        t
    }

    /// Divisionstabelle `DAT_01c65758` (aufgebaut in `@006fa880`): `DIV[a][b] = ⌊255·a/b⌋` für
    /// `b ≥ a` und `b > 0`, sonst 0. Benutzt vom Compositor, der Tinten-Akkumulation der Pinsel und
    /// der Farbfüllung.
    pub static DIV: [[u8; 256]; 256] = build_div();

    /// `float32`-Zwischenspeicherung (wie x87-`fstp dword`).
    #[inline]
    pub fn f32r(x: f64) -> f64 {
        x as f32 as f64
    }

    // fp-color/src/blend.rs (brush stamp opacity)
    /// Überlappungstabelle `T[n][A]` (07 §6.5): kleinstes `a`, für das `n`-mal `u = u + a − mul(u,a)`
    /// (ab `u = 0`) mindestens `A` ergibt; 255, wenn keines reicht.
    pub fn overlap_alpha(n: u32, target: u8) -> u8 {
        for a in 0..=255u32 {
            let mut u = 0u32;
            for _ in 0..n {
                u = u + a - mul255(u, a);
            }
            if u >= target as u32 {
                return a as u8;
            }
        }
        255
    }

    /// Promille-Umweg der Stempeldeckkraft (`@00785750`/`@00786290`, B): `⌊⌊max(1,L)·1000/255⌋·255/1000⌋`,
    /// also `L − 1` außer bei Vielfachen von 51. Nicht „optimieren“.
    pub fn stamp_opacity_roundtrip(l: u8) -> u8 {
        let l = (l as u32).max(1);
        ((l * 1000 / 255) * 255 / 1000) as u8
    }

    /// Wirksame Stempeldeckkraft eines Pinsels (`stroke.py`): `L = ⌊BCNi·255/1000⌋`; bei `BFBi = 1`
    /// `L = T[n][L]` mit `n = max(1, 1000/BSPi)`, wenn `BSPi·D′ > 1000`, sonst `D′` (höchstens 100);
    /// danach der Promille-Umweg.
    pub fn brush_stamp_opacity(bcni_permille: u32, bfbi: u32, bspi: u32, d_prime: f64) -> u8 {
        let l = (bcni_permille.min(1000) * 255 / 1000) as u8;
        let o = if bfbi == 1 {
            let nov = if bspi as f64 * d_prime > 1000.0 {
                (1000 / bspi.max(1)).max(1) as f64
            } else {
                d_prime
            };
            let n = if nov.is_finite() {
                nov.clamp(0.0, 100.0) as u32
            } else {
                100
            };
            overlap_alpha(n.min(100), l)
        } else {
            l
        };
        stamp_opacity_roundtrip(o)
    }
}

pub mod raster {
    use super::geom::IRect;

    /// An 8-bit mask over a pixel rectangle (reduced from fp-raster's `Mask`).
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub struct Mask {
        bounds: IRect,
        pub data: Vec<u8>,
    }

    impl Mask {
        pub fn from_raw(bounds: IRect, data: Vec<u8>) -> Result<Self, ()> {
            let (w, h) = (bounds.width().max(0) as usize, bounds.height().max(0) as usize);
            if data.len() != w * h {
                return Err(());
            }
            Ok(Self { bounds, data })
        }
        pub fn bounds(&self) -> IRect {
            self.bounds
        }
        pub fn width(&self) -> usize {
            self.bounds.width().max(0) as usize
        }
        pub fn height(&self) -> usize {
            self.bounds.height().max(0) as usize
        }
        pub fn data(&self) -> &[u8] {
            &self.data
        }
    }

    /// An RGBA image over a pixel rectangle (fill patterns; reduced from fp-raster's).
    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub struct RgbaImage {
        pub bounds: IRect,
        pub px: Vec<[u8; 4]>,
    }
}
