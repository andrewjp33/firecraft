//! 2D affine transforms (symbol instances, the Transform tools).

use crate::image::{Rgba, RgbaImage};

/// `x' = a*x + c*y + e`, `y' = b*x + d*y + f` (Fireworks `MTXv`: a=M00, b=M01, c=M10, d=M11,
/// e=M20, f=M21; row-vector convention).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Affine = Affine { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn translate(dx: f64, dy: f64) -> Self {
        Affine { e: dx, f: dy, ..Self::IDENTITY }
    }

    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }

    /// `self` after `first`: apply(then(first)) = self.apply(first.apply(p)).
    pub fn then(&self, first: &Affine) -> Affine {
        Affine {
            a: self.a * first.a + self.c * first.b,
            b: self.b * first.a + self.d * first.b,
            c: self.a * first.c + self.c * first.d,
            d: self.b * first.c + self.d * first.d,
            e: self.a * first.e + self.c * first.f + self.e,
            f: self.b * first.e + self.d * first.f + self.f,
        }
    }

    pub fn det(&self) -> f64 {
        self.a * self.d - self.b * self.c
    }

    pub fn invert(&self) -> Option<Affine> {
        let det = self.det();
        if det.abs() < 1e-12 || !det.is_finite() {
            return None;
        }
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Some(Affine { a, b, c, d, e: -(a * self.e + c * self.f), f: -(b * self.e + d * self.f) })
    }

    /// Average linear scale (for stroke widths and effect sizes).
    pub fn scale(&self) -> f64 {
        self.det().abs().sqrt()
    }

    /// Bounding box (x, y, w, h) of the transformed rectangle.
    pub fn map_rect(&self, x: f64, y: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
        let pts = [self.apply(x, y), self.apply(x + w, y), self.apply(x, y + h), self.apply(x + w, y + h)];
        let x0 = pts.iter().map(|p| p.0).fold(f64::MAX, f64::min);
        let y0 = pts.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        let x1 = pts.iter().map(|p| p.0).fold(f64::MIN, f64::max);
        let y1 = pts.iter().map(|p| p.1).fold(f64::MIN, f64::max);
        (x0, y0, x1 - x0, y1 - y0)
    }
}

/// Transform an image placed at (`x`, `y`): returns the new image and its integer position.
/// Bilinear sampling on premultiplied colour. `None` for degenerate or huge results.
pub fn transform_image(img: &RgbaImage, x: i64, y: i64, m: &Affine) -> Option<(RgbaImage, i64, i64)> {
    let inv = m.invert()?;
    let (bx, by, bw, bh) = m.map_rect(x as f64, y as f64, img.width() as f64, img.height() as f64);
    let (x0, y0) = (bx.floor() as i64, by.floor() as i64);
    let (w, h) = ((bx + bw).ceil() as i64 - x0, (by + bh).ceil() as i64 - y0);
    if w <= 0 || h <= 0 || w * h > 1 << 28 {
        return None;
    }
    let mut out = RgbaImage::new(w as u32, h as u32, Rgba::TRANSPARENT);
    for ty in 0..h {
        for tx in 0..w {
            let (sx, sy) = inv.apply((x0 + tx) as f64 + 0.5, (y0 + ty) as f64 + 0.5);
            let (sx, sy) = (sx - x as f64 - 0.5, sy - y as f64 - 0.5);
            out.set(tx, ty, sample(img, sx, sy));
        }
    }
    Some((out, x0, y0))
}

/// Bilinear sample with transparent outside (premultiplied interpolation).
pub fn sample(img: &RgbaImage, x: f64, y: f64) -> Rgba {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (xi, yi) = (x0 as i64, y0 as i64);
    let px = |dx: i64, dy: i64| img.get(xi + dx, yi + dy).unwrap_or(Rgba::TRANSPARENT);
    let ps = [px(0, 0), px(1, 0), px(0, 1), px(1, 1)];
    let w = [(1.0 - fx) * (1.0 - fy), fx * (1.0 - fy), (1.0 - fx) * fy, fx * fy];
    let alpha: f64 = ps.iter().zip(w).map(|(p, w)| p.a as f64 * w).sum();
    if alpha < 0.5 {
        return Rgba::TRANSPARENT;
    }
    let ch = |f: fn(&Rgba) -> u8| -> u8 {
        (ps.iter().zip(w).map(|(p, w)| f(p) as f64 * p.a as f64 * w).sum::<f64>() / alpha).round().clamp(0.0, 255.0) as u8
    };
    Rgba::new(ch(|p| p.r), ch(|p| p.g), ch(|p| p.b), alpha.round().clamp(0.0, 255.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_and_invert() {
        let s = Affine { a: 2.0, d: 3.0, ..Affine::IDENTITY };
        let t = Affine::translate(10.0, 5.0);
        let m = t.then(&s);
        assert_eq!(m.apply(1.0, 1.0), (12.0, 8.0));
        let back = m.invert().unwrap().apply(12.0, 8.0);
        assert!((back.0 - 1.0).abs() < 1e-9 && (back.1 - 1.0).abs() < 1e-9);
        assert!(Affine { a: 0.0, d: 0.0, ..Affine::IDENTITY }.invert().is_none());
    }

    #[test]
    fn scaling_an_image_doubles_it() {
        let img = RgbaImage::new(4, 4, Rgba::new(255, 0, 0, 255));
        let (out, x, y) = transform_image(&img, 10, 10, &Affine { a: 2.0, d: 2.0, ..Affine::IDENTITY }).unwrap();
        assert_eq!((x, y, out.width(), out.height()), (20, 20, 8, 8));
        assert_eq!(out.get(4, 4), Some(Rgba::new(255, 0, 0, 255)));
    }
}
