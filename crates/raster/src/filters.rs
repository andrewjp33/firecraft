//! Fireworks' classic filters, used as Live Effects and from the Filters menu: Blur, Blur More,
//! Sharpen, Sharpen More, Find Edges, Unsharp Mask, and the table filters (Levels, Auto Levels,
//! Curves, Invert).
//!
//! Adapted from FirePixel's reconstructions (`crates/fp-raster/src/filters/convolve.rs`,
//! `table.rs`, `gauss.rs`; Apache-2.0), checked against Fireworks MX with the oracle probe
//! `crates/io/examples/filter_probe.rs` (docs/behavior/effects.md).
//!
//! The filters work on the object's own pixel rectangle (`region`): they never draw outside it,
//! and the 3x3 filters repeat its edge pixels beyond it. Colour is not premultiplied, and the
//! colour stored in transparent pixels takes part (measured: a bitmap whose transparent pixels
//! are green gets a green fringe from Blur and Find Edges).

use crate::image::{Rgba, RgbaImage};

/// A rectangle of an image: (x, y, width, height) in pixels.
pub type Region = (u32, u32, u32, u32);

/// The whole image.
pub fn whole(img: &RgbaImage) -> Region {
    (0, 0, img.width(), img.height())
}

/// The bounds of the pixels with alpha above 0 (`None` when there are none).
pub fn alpha_bounds(img: &RgbaImage) -> Option<Region> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let px = img.pixels();
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if px.get(y * w + x).is_some_and(|p| p.a > 0) {
                x0 = x0.min(x);
                x1 = x1.max(x + 1);
                y0 = y0.min(y);
                y1 = y1.max(y + 1);
            }
        }
    }
    (x1 > x0).then(|| (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32))
}

/// The smallest region holding both.
pub fn union(a: Region, b: Region) -> Region {
    let (x0, y0) = (a.0.min(b.0), a.1.min(b.1));
    let (x1, y1) = ((a.0 + a.2).max(b.0 + b.2), (a.1 + a.3).max(b.1 + b.3));
    (x0, y0, x1 - x0, y1 - y0)
}

fn clip(img: &RgbaImage, r: Region) -> Region {
    let x0 = r.0.min(img.width());
    let y0 = r.1.min(img.height());
    let x1 = (r.0 + r.2).min(img.width());
    let y1 = (r.1 + r.3).min(img.height());
    (x0, y0, x1 - x0, y1 - y0)
}

/// 3x3 convolution of `region`, per channel on straight (not premultiplied) values, in integers:
/// `v = (sum k_i p_i) / divisor` (division towards zero), clamped to 0..255. Outside the region
/// its edge pixels repeat. With `with_alpha` alpha is filtered too, otherwise each pixel keeps
/// its own.
pub fn convolve3x3(img: &mut RgbaImage, region: Region, kernel: [i32; 9], divisor: i32, with_alpha: bool) {
    let (rx, ry, rw, rh) = clip(img, region);
    if rw == 0 || rh == 0 || divisor == 0 {
        return;
    }
    let w = img.width() as usize;
    let (rx, ry, rw, rh) = (rx as usize, ry as usize, rw as usize, rh as usize);
    let src: Vec<[i32; 4]> = img
        .pixels()
        .iter()
        .map(|p| [p.r as i32, p.g as i32, p.b as i32, p.a as i32])
        .collect();
    let px = img.pixels_mut();
    for y in ry..ry + rh {
        let rows = [y.saturating_sub(1).max(ry), y, (y + 1).min(ry + rh - 1)];
        for x in rx..rx + rw {
            let xs = [x.saturating_sub(1).max(rx), x, (x + 1).min(rx + rw - 1)];
            let mut acc = [0i32; 4];
            for (ky, &yy) in rows.iter().enumerate() {
                for (kx, &xx) in xs.iter().enumerate() {
                    let k = kernel[ky * 3 + kx];
                    let p = src[yy * w + xx];
                    for c in 0..4 {
                        acc[c] += k * p[c];
                    }
                }
            }
            let v = |c: usize| (acc[c] / divisor).clamp(0, 255) as u8;
            let a = if with_alpha { v(3) } else { src[y * w + x][3] as u8 };
            px[y * w + x] = Rgba::new(v(0), v(1), v(2), a);
        }
    }
}

/// Blur (strength 3) and Blur More (7): neighbours 1, centre 11 - s, divisor 19 - s, alpha
/// included.
pub fn simple_blur(img: &mut RgbaImage, region: Region, strength: i32) {
    let s = strength.clamp(1, 10);
    convolve3x3(img, region, [1, 1, 1, 1, 11 - s, 1, 1, 1, 1], 19 - s, true);
}

/// Sharpen: neighbours -1, centre 19 - s, divisor 11 - s, alpha included. As a Live Effect both
/// Sharpen and Sharpen More use s = 1 (measured: Fireworks MX draws them identically).
pub fn sharpen(img: &mut RgbaImage, region: Region, strength: i32) {
    let s = strength.clamp(1, 10);
    convolve3x3(img, region, [-1, -1, -1, -1, 19 - s, -1, -1, -1, -1], 11 - s, true);
}

/// Find Edges: a Laplacian (neighbours 1, centre -8), each pixel keeping its alpha.
pub fn find_edges(img: &mut RgbaImage, region: Region) {
    convolve3x3(img, region, [1, 1, 1, 1, -8, 1, 1, 1, 1], 1, false);
}

/// Unsharp Mask: the region blurred with Gaussian Blur (radius up to 70); where a channel
/// (alpha too) exceeds its blurred value by more than `threshold`, it gains
/// `(round(amount% x 255) x difference) >> 8`, capped at 255.
pub fn unsharp_mask(img: &mut RgbaImage, region: Region, radius: f32, amount: f32, threshold: i32) {
    let (rx, ry, rw, rh) = clip(img, region);
    if rw == 0 || rh == 0 {
        return;
    }
    let sigma = if radius.is_nan() { 0.1 } else { radius.clamp(0.1, 70.0) };
    let mut part = RgbaImage::new(rw, rh, Rgba::TRANSPARENT);
    for y in 0..rh {
        for x in 0..rw {
            if let Some(p) = img.get((rx + x) as i64, (ry + y) as i64) {
                part.set(x as i64, y as i64, p);
            }
        }
    }
    let original = part.clone();
    crate::effects::gaussian_blur(&mut part, sigma);
    let k = ((amount.clamp(1.0, 500.0) as f64 / 100.0 * 255.0) + 0.5).floor() as i32;
    let thr = threshold.clamp(0, 255);
    for y in 0..rh {
        for x in 0..rw {
            let (Some(o), Some(b)) = (original.get(x as i64, y as i64), part.get(x as i64, y as i64)) else {
                continue;
            };
            let (o, b) = ([o.r, o.g, o.b, o.a], [b.r, b.g, b.b, b.a]);
            let mut n = o;
            for c in 0..4 {
                let d = o[c] as i32 - b[c] as i32;
                if d > thr {
                    n[c] = (((k * d) >> 8) + o[c] as i32).min(255) as u8;
                }
            }
            img.set((rx + x) as i64, (ry + y) as i64, Rgba::new(n[0], n[1], n[2], n[3]));
        }
    }
}

/// The four lookup tables of the table filters, applied as `rgb[channel[v]]` to every pixel
/// (alpha unchanged).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tables {
    pub rgb: [u8; 256],
    pub red: [u8; 256],
    pub green: [u8; 256],
    pub blue: [u8; 256],
}

pub fn identity() -> [u8; 256] {
    std::array::from_fn(|i| i as u8)
}

impl Default for Tables {
    fn default() -> Self {
        Tables {
            rgb: identity(),
            red: identity(),
            green: identity(),
            blue: identity(),
        }
    }
}

impl Tables {
    pub fn apply(&self, img: &mut RgbaImage) {
        for p in img.pixels_mut() {
            p.r = self.rgb[self.red[p.r as usize] as usize];
            p.g = self.rgb[self.green[p.g as usize] as usize];
            p.b = self.rgb[self.blue[p.b as usize] as usize];
        }
    }

    /// Invert: 255 - v.
    pub fn invert() -> Tables {
        Tables {
            rgb: std::array::from_fn(|i| 255 - i as u8),
            ..Tables::default()
        }
    }
}

/// One channel's Levels: input low/high, output low/high, gamma.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LevelsChannel {
    pub source_low: i32,
    pub source_high: i32,
    pub dest_low: i32,
    pub dest_high: i32,
    pub gamma: f64,
}

impl Default for LevelsChannel {
    fn default() -> Self {
        LevelsChannel {
            source_low: 0,
            source_high: 255,
            dest_low: 0,
            dest_high: 255,
            gamma: 1.0,
        }
    }
}

impl LevelsChannel {
    /// The channel's table: x clamped to the input range, t = (x - low) / (high - low) raised to
    /// 1 / gamma (gamma 0.1..10), output low + t (high - low), truncated. Values are read as
    /// bytes; an empty input range gives 0.
    pub fn table(&self) -> [u8; 256] {
        let lo = (self.source_low & 0xff) as f64;
        let hi = (self.source_high & 0xff) as f64;
        let dl = (self.dest_low & 0xff) as f64;
        let dh = (self.dest_high & 0xff) as f64;
        let g = if self.gamma >= 0.1 { self.gamma.min(10.0) } else { 0.1 };
        std::array::from_fn(|i| {
            let x = (i as f64).max(lo);
            let x = if x <= hi { x } else { hi };
            let mut t = ((x - lo) / (hi - lo)).abs();
            if g != 1.0 && !t.is_nan() {
                t = t.powf(1.0 / g);
            }
            let v = t * (dh - dl) + dl;
            if v.is_nan() || v.abs() > 2147483648.0 {
                0
            } else {
                ((v.trunc() as i64) & 0xff) as u8
            }
        })
    }
}

/// Levels for [rgb, red, green, blue].
pub fn levels_tables(ch: &[LevelsChannel; 4]) -> Tables {
    Tables {
        rgb: ch[0].table(),
        red: ch[1].table(),
        green: ch[2].table(),
        blue: ch[3].table(),
    }
}

/// The 0.5% limits of a histogram: the first and last levels at which the running count from
/// each end reaches 0.5% of all pixels (at least 1).
pub fn histogram_bounds(hist: &[u64; 256]) -> (i32, i32) {
    let total: u64 = hist.iter().sum();
    let thr = ((total as f64 * 0.005) as u64).max(1);
    let mut cum = 0u64;
    let lo = hist
        .iter()
        .position(|&h| {
            cum += h;
            cum >= thr
        })
        .unwrap_or(0) as i32;
    cum = 0;
    let hi = (0..256)
        .rev()
        .find(|&i| {
            cum += hist[i];
            cum >= thr
        })
        .unwrap_or(255) as i32;
    (lo, hi)
}

/// Per-channel histograms (R, G, B) of the pixels with alpha above 0.
pub fn histograms(img: &RgbaImage) -> [[u64; 256]; 3] {
    let mut h = [[0u64; 256]; 3];
    for p in img.pixels().iter().filter(|p| p.a > 0) {
        h[0][p.r as usize] += 1;
        h[1][p.g as usize] += 1;
        h[2][p.b as usize] += 1;
    }
    h
}

/// Auto Levels: each of red, green and blue stretched from its histogram's 0.5% limits to 0..255.
pub fn auto_levels(img: &RgbaImage) -> [LevelsChannel; 4] {
    let h = histograms(img);
    let ch = |c: usize| {
        let (lo, hi) = histogram_bounds(&h[c]);
        LevelsChannel {
            source_low: lo,
            source_high: hi,
            ..LevelsChannel::default()
        }
    };
    [LevelsChannel::default(), ch(0), ch(1), ch(2)]
}

fn f32r(x: f64) -> f64 {
    x as f32 as f64
}

/// Move (px, py) by `length` along the direction (dx, dy), in single precision.
fn offset_along(dx: f64, dy: f64, length: f64, px: f64, py: f64) -> (f64, f64) {
    let h = dx.hypot(dy);
    if h == 0.0 {
        return (f32r(px), f32r(py));
    }
    if dy.abs() < dx.abs() {
        let t = (length * dx) / h;
        (f32r(t + px), f32r((t * dy) / dx + py))
    } else {
        let t = (dy * length) / h;
        (f32r((dx * t) / dy + px), f32r(t + py))
    }
}

/// Adaptive subdivision of a cubic Bézier (depth at most 6): the end points of its flat pieces,
/// in single precision, without the start point.
fn flatten_cubic(p: [f64; 8]) -> Vec<(f64, f64)> {
    let [mut x0, mut y0, mut x1, mut y1, mut x2, mut y2, mut x3, mut y3] = p;
    let mut depth = 0.0f64;
    let mut stack: Vec<[f64; 9]> = Vec::new();
    let mut out = Vec::new();
    loop {
        if depth < 6.0 && (x0 != x1 || y0 != y1 || x3 != x2 || y3 != y2) {
            let (dx, dy) = (x0 - x3, y0 - y3);
            let (ax, ay) = (dx.abs(), dy.abs());
            let dist = if ay <= ax { ax + (ay + 1.0) * 0.5 } else { ax * 0.5 + ay };
            let tol = if dist == 0.0 { -1.0 } else { dist * 0.046875 };
            let flat = (dy * (x0 - x1) - dx * (y0 - y1)).abs() <= tol && (dy * (x0 - x2) - dx * (y0 - y2)).abs() <= tol;
            if !flat {
                let (x01, y01) = ((x1 + x0) * 0.5, (y1 + y0) * 0.5);
                let (x12, y12) = ((x2 + x1) * 0.5, (y2 + y1) * 0.5);
                let (x23, y23) = ((x3 + x2) * 0.5, (y3 + y2) * 0.5);
                let (x012, y012) = ((x12 + x01) * 0.5, (y12 + y01) * 0.5);
                let (x123, y123) = ((x23 + x12) * 0.5, (y23 + y12) * 0.5);
                let (xm, ym) = ((x123 + x012) * 0.5, (y123 + y012) * 0.5);
                depth += 1.0;
                stack.push([xm, ym, x123, y123, x23, y23, x3, y3, depth]);
                (x1, y1, x2, y2, x3, y3) = (x01, y01, x012, y012, xm, ym);
                continue;
            }
        }
        out.push((f32r(x3), f32r(y3)));
        match stack.pop() {
            Some(s) => [x0, y0, x1, y1, x2, y2, x3, y3, depth] = s,
            None => return out,
        }
    }
}

fn byte_of(v: f64) -> u8 {
    if v.is_finite() { ((v.trunc() as i64) & 0xff) as u8 } else { 0 }
}

/// Curves: control points (x ascending, 0..255) to a table. Before the first point and after the
/// last the curve is flat; between them a smooth curve through the points (each inner point's
/// handles lie along the line joining its neighbours, half the distance to each). Point lists
/// out of order or out of range leave the parts not yet drawn as they are.
pub fn curve_table(points: &[(f32, f32)]) -> [u8; 256] {
    let mut tab = identity();
    let Some(&(fx, fy)) = points.first() else {
        return tab;
    };
    let pts: Vec<(f64, f64)> = points.iter().map(|&(x, y)| (x as f64, y as f64)).collect();
    let (fx, fy) = (fx as f64, fy as f64);
    if !(fx < 255.0) {
        return tab;
    }
    if 0.0 < fx {
        let mut i = 0usize;
        loop {
            tab[i] = byte_of(fy);
            i += 1;
            if !((i as f32 as f64) < fx) || i > 255 {
                break;
            }
        }
    }
    let (lx, ly) = pts[pts.len() - 1];
    if !(0.0 < lx) {
        return tab;
    }
    let start = lx.trunc();
    if start <= 255.0 {
        for v in tab.iter_mut().skip(start.max(0.0) as usize) {
            *v = byte_of(ly);
        }
    }
    let mut arr: Vec<(f64, f64)> = vec![pts[0], pts[0]];
    for k in 1..pts.len().saturating_sub(1) {
        let (prv, cur, nxt) = (pts[k - 1], pts[k], pts[k + 1]);
        if prv.0 < 0.0 || cur.0 < 1.0 || cur.0 > 254.0 || nxt.0 > 255.0 || cur.0 <= prv.0 || nxt.0 <= cur.0 {
            return tab;
        }
        let (dx, dy) = (nxt.0 - prv.0, nxt.1 - prv.1);
        arr.push(offset_along(dx, dy, (prv.0 - cur.0) * 0.5, cur.0, cur.1));
        arr.push(cur);
        arr.push(offset_along(dx, dy, (nxt.0 - cur.0) * 0.5, cur.0, cur.1));
    }
    arr.push(pts[pts.len() - 1]);
    arr.push(pts[pts.len() - 1]);
    let mut s = 0;
    while s + 3 < arr.len() {
        let seg = [
            arr[s].0,
            arr[s].1,
            arr[s + 1].0,
            arr[s + 1].1,
            arr[s + 2].0,
            arr[s + 2].1,
            arr[s + 3].0,
            arr[s + 3].1,
        ];
        let (mut px, mut py) = arr[s];
        for (qx, qy) in flatten_cubic(seg) {
            let (qx, qy) = (f32r(qx.round()), f32r(qy.round()));
            if px < qx && qx <= 255.0 {
                let slope = (qy - py) / (qx - px).max(1.0);
                let mut i = px.trunc() as i64;
                while ((i as f32) as f64) < qx {
                    let v = (((i as f32) as f64 - px) * slope + py).round();
                    if (0..256).contains(&i) {
                        tab[i as usize] = v.clamp(0.0, 255.0) as u8;
                    }
                    i += 1;
                }
                px = qx;
                py = qy;
            }
        }
        s += 3;
    }
    tab
}

/// Curves for [rgb, red, green, blue]; an empty list is the identity.
pub fn curves_tables(ch: &[Vec<(f32, f32)>; 4]) -> Tables {
    Tables {
        rgb: curve_table(&ch[0]),
        red: curve_table(&ch[1]),
        green: curve_table(&ch[2]),
        blue: curve_table(&ch[3]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_image_unchanged_by_blur_and_sharpen() {
        let c = Rgba::new(40, 80, 120, 200);
        let img = RgbaImage::new(7, 5, c);
        for f in [
            |i: &mut RgbaImage| simple_blur(i, whole(i), 3),
            |i: &mut RgbaImage| simple_blur(i, whole(i), 7),
            |i: &mut RgbaImage| sharpen(i, whole(i), 1),
        ] {
            let mut x = img.clone();
            f(&mut x);
            assert_eq!(x, img);
        }
        let mut e = img.clone();
        find_edges(&mut e, whole(&img));
        assert!(e.pixels().iter().all(|p| *p == Rgba::new(0, 0, 0, 200)));
    }

    #[test]
    fn identity_tables() {
        assert_eq!(curve_table(&[(0.0, 0.0), (255.0, 255.0)]), identity());
        assert_eq!(LevelsChannel::default().table(), identity());
        let empty = LevelsChannel {
            source_low: 100,
            source_high: 100,
            ..LevelsChannel::default()
        };
        assert!(empty.table().iter().all(|&v| v == 0));
    }

    #[test]
    fn histogram_limits() {
        let mut h = [0u64; 256];
        h[10] = 1000;
        h[200] = 1000;
        assert_eq!(histogram_bounds(&h), (10, 200));
        assert_eq!(histogram_bounds(&[0; 256]), (0, 255));
    }
}
