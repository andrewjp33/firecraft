//! 9-slice scaling: corners keep their size, edges stretch along one axis, the centre stretches
//! along both.
//!
//! The per-axis mapping rule is adapted from FirePixel's `fp-render/src/nine.rs`
//! (Copyright the FirePixel authors, Apache-2.0; see ATTRIBUTION.md): guides are rounded to whole
//! pixels; a guide outside the reference range contributes a zero margin; when the margins overlap
//! or the target is narrower than both margins together, the axis falls back to plain scaling.

use crate::image::{Rgba, RgbaImage};

/// Mapping of one axis from a reference range `[lo, hi]` to a target range `[lo2, hi2]`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AxisMap {
    pub lo: f64,
    pub hi: f64,
    /// Fixed margins at the low and high end (>= 0).
    pub m1: f64,
    pub m2: f64,
    pub lo2: f64,
    pub hi2: f64,
    /// 9-slice active (else plain scaling).
    pub nine: bool,
}

impl AxisMap {
    /// `g1`/`g2` are the guide positions (absolute, same space as `lo`/`hi`).
    pub fn new(lo: f64, hi: f64, g1: f64, g2: f64, lo2: f64, hi2: f64) -> AxisMap {
        let (g1, g2) = (g1.round(), g2.round());
        let m1 = if lo < g1 && g1 < hi { g1 - lo } else { 0.0 };
        let m2 = if lo < g2 && g2 < hi { hi - g2 } else { 0.0 };
        let nine = lo + m1 < hi - m2 && (hi2 - lo2) >= m1 + m2;
        AxisMap {
            lo,
            hi,
            m1,
            m2,
            lo2,
            hi2,
            nine,
        }
    }

    /// Reference -> target.
    pub fn apply(&self, x: f64) -> f64 {
        let (g1, g2) = (self.lo + self.m1, self.hi - self.m2);
        if !self.nine {
            if self.hi == self.lo {
                return self.lo2 + (x - self.lo);
            }
            return self.lo2 + (x - self.lo) * (self.hi2 - self.lo2) / (self.hi - self.lo);
        }
        if x <= g1 {
            self.lo2 + (x - self.lo)
        } else if x >= g2 {
            self.hi2 - (self.hi - x)
        } else {
            (self.lo2 + self.m1) + (x - g1) * ((self.hi2 - self.m2) - (self.lo2 + self.m1)) / (g2 - g1)
        }
    }

    /// Target -> reference (used to resample bitmaps).
    pub fn invert(&self, x2: f64) -> f64 {
        if !self.nine {
            if self.hi2 == self.lo2 {
                return self.lo + (x2 - self.lo2);
            }
            return self.lo + (x2 - self.lo2) * (self.hi - self.lo) / (self.hi2 - self.lo2);
        }
        let (t1, t2) = (self.lo2 + self.m1, self.hi2 - self.m2);
        let (g1, g2) = (self.lo + self.m1, self.hi - self.m2);
        if x2 <= t1 {
            self.lo + (x2 - self.lo2)
        } else if x2 >= t2 {
            self.hi - (self.hi2 - x2)
        } else if t2 > t1 {
            g1 + (x2 - t1) * (g2 - g1) / (t2 - t1)
        } else {
            g1
        }
    }
}

impl AxisMap {
    /// The source range (reference space, half-open) of the slice that target coordinate `x2`
    /// falls in. Sampling is clamped to it so slices never blend across a guide.
    pub fn source_slice(&self, x2: f64) -> (f64, f64) {
        if !self.nine {
            return (self.lo, self.hi);
        }
        let (g1, g2) = (self.lo + self.m1, self.hi - self.m2);
        if x2 < self.lo2 + self.m1 {
            (self.lo, g1)
        } else if x2 >= self.hi2 - self.m2 {
            (g2, self.hi)
        } else {
            (g1, g2)
        }
    }
}

/// Guides as insets from the reference box edges (Fireworks shows them as four dashed lines).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Insets {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

impl Insets {
    /// Default guides for a box: a third of each side, as the 9-slice tool starts out.
    pub fn thirds(w: f64, h: f64) -> Self {
        Self {
            left: (w / 3.0).round(),
            right: (w / 3.0).round(),
            top: (h / 3.0).round(),
            bottom: (h / 3.0).round(),
        }
    }
}

/// The two axis maps for scaling box (x, y, w, h) to (x2, y2, w2, h2) with guides `ins`.
pub fn maps(from: (f64, f64, f64, f64), to: (f64, f64, f64, f64), ins: Insets) -> (AxisMap, AxisMap) {
    let (x, y, w, h) = from;
    let (x2, y2, w2, h2) = to;
    (
        AxisMap::new(x, x + w, x + ins.left, x + w - ins.right, x2, x2 + w2),
        AxisMap::new(y, y + h, y + ins.top, y + h - ins.bottom, y2, y2 + h2),
    )
}

/// Resample `img` (placed with its top-left at `origin` in reference space) into a new image of
/// `size`, whose top-left lands at `target_origin`. Bilinear sampling, straight alpha.
pub fn resample(
    img: &RgbaImage,
    origin: (f64, f64),
    mx: &AxisMap,
    my: &AxisMap,
    target_origin: (i64, i64),
    size: (u32, u32),
) -> RgbaImage {
    let mut out = RgbaImage::new(size.0, size.1, Rgba::TRANSPARENT);
    // Nothing to sample, as with text that draws no pixels.
    if img.width() == 0 || img.height() == 0 {
        return out;
    }
    for ty in 0..size.1 {
        for tx in 0..size.0 {
            let (cx, cy) = (
                target_origin.0 as f64 + tx as f64 + 0.5,
                target_origin.1 as f64 + ty as f64 + 0.5,
            );
            // Sample in the source slice this pixel belongs to (pixel-centre coordinates).
            let (ax, bx) = mx.source_slice(cx);
            let (ay, by) = my.source_slice(cy);
            let sx = mx.invert(cx).clamp(ax + 0.5, (bx - 0.5).max(ax + 0.5)) - origin.0 - 0.5;
            let sy = my.invert(cy).clamp(ay + 0.5, (by - 0.5).max(ay + 0.5)) - origin.1 - 0.5;
            out.set(tx as i64, ty as i64, bilinear(img, sx, sy));
        }
    }
    out
}

fn bilinear(img: &RgbaImage, x: f64, y: f64) -> Rgba {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let clampx = |v: f64| (v as i64).clamp(0, img.width() as i64 - 1);
    let clampy = |v: f64| (v as i64).clamp(0, img.height() as i64 - 1);
    let px = |xx: f64, yy: f64| img.get(clampx(xx), clampy(yy)).unwrap_or(Rgba::TRANSPARENT);
    let (a, b, c, d) = (
        px(x0, y0),
        px(x0 + 1.0, y0),
        px(x0, y0 + 1.0),
        px(x0 + 1.0, y0 + 1.0),
    );
    // Interpolate premultiplied so transparent pixels don't bleed colour.
    let w = [(1.0 - fx) * (1.0 - fy), fx * (1.0 - fy), (1.0 - fx) * fy, fx * fy];
    let ps = [a, b, c, d];
    let alpha: f64 = ps.iter().zip(w).map(|(p, w)| p.a as f64 * w).sum();
    if alpha <= 0.0 {
        return Rgba::TRANSPARENT;
    }
    let ch = |f: fn(&Rgba) -> u8| -> u8 {
        (ps.iter()
            .zip(w)
            .map(|(p, w)| f(p) as f64 * p.a as f64 * w)
            .sum::<f64>()
            / alpha)
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Rgba::new(
        ch(|p| p.r),
        ch(|p| p.g),
        ch(|p| p.b),
        alpha.round().clamp(0.0, 255.0) as u8,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_an_empty_image_gives_an_empty_result() {
        let map = AxisMap::new(0.0, 10.0, 2.0, 8.0, 0.0, 20.0);
        let out = resample(
            &RgbaImage::new(0, 0, Rgba::TRANSPARENT),
            (0.0, 0.0),
            &map,
            &map,
            (0, 0),
            (4, 4),
        );
        assert!(out.pixels().iter().all(|p| p.a == 0));
    }

    #[test]
    fn corners_keep_size_and_centre_stretches() {
        let m = AxisMap::new(0.0, 30.0, 10.0, 20.0, 0.0, 100.0);
        assert!(m.nine);
        assert_eq!(m.apply(0.0), 0.0);
        assert_eq!(m.apply(10.0), 10.0);
        assert_eq!(m.apply(20.0), 90.0);
        assert_eq!(m.apply(30.0), 100.0);
        assert_eq!(m.apply(15.0), 50.0);
        for x2 in [0.0, 5.0, 10.0, 50.0, 90.0, 95.0, 100.0] {
            assert!((m.apply(m.invert(x2)) - x2).abs() < 1e-9);
        }
    }

    #[test]
    fn falls_back_to_plain_scaling() {
        // Target narrower than both margins.
        let m = AxisMap::new(0.0, 30.0, 10.0, 20.0, 0.0, 15.0);
        assert!(!m.nine);
        assert_eq!(m.apply(30.0), 15.0);
        // Overlapping guides.
        assert!(!AxisMap::new(0.0, 30.0, 20.0, 10.0, 0.0, 60.0).nine);
        // Guide outside the range: zero margin on that side.
        let m = AxisMap::new(0.0, 30.0, -5.0, 20.0, 0.0, 60.0);
        assert_eq!(m.m1, 0.0);
        assert_eq!(m.m2, 10.0);
    }

    #[test]
    fn rounded_rectangle_bitmap_keeps_its_corners() {
        // 30x30 image: a 10 px red border, white middle.
        let mut img = RgbaImage::new(30, 30, Rgba::WHITE);
        for y in 0..30 {
            for x in 0..30 {
                if !(10..20).contains(&x) || !(10..20).contains(&y) {
                    img.set(x, y, Rgba::new(255, 0, 0, 255));
                }
            }
        }
        let (mx, my) = maps(
            (0.0, 0.0, 30.0, 30.0),
            (0.0, 0.0, 90.0, 60.0),
            Insets {
                left: 10.0,
                top: 10.0,
                right: 10.0,
                bottom: 10.0,
            },
        );
        let out = resample(&img, (0.0, 0.0), &mx, &my, (0, 0), (90, 60));
        // Borders stay 10 px wide; the middle stretches.
        assert_eq!(out.get(5, 30).map(|p| p.g), Some(0));
        assert_eq!(out.get(12, 30).map(|p| p.g), Some(255));
        assert_eq!(out.get(77, 30).map(|p| p.g), Some(255));
        assert_eq!(out.get(84, 30).map(|p| p.g), Some(0));
        assert_eq!(out.get(45, 55).map(|p| p.g), Some(0));
        assert_eq!(out.get(45, 48).map(|p| p.g), Some(255));
    }
}
