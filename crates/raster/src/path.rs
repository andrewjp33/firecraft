//! Vector rasterization: filled polygons (anti-aliased) and stamped strokes.
//!
//! Fireworks renders strokes by stamping its brush tip along the path (its trees store a
//! flattened, pressure-annotated polyline next to each path for that reason), so strokes here are
//! stamped too.

use crate::image::{Rgba, RgbaImage};

/// A point in canvas pixels (y down). Pixel centres are at +0.5.
pub type Pt = (f64, f64);

/// Flatten a cubic bezier from `p0` to `p3` into `out` (excluding `p0`).
pub fn flatten_cubic(p0: Pt, p1: Pt, p2: Pt, p3: Pt, out: &mut Vec<Pt>) {
    let len = dist(p0, p1) + dist(p1, p2) + dist(p2, p3);
    let steps = (len / 2.0).ceil().clamp(1.0, 512.0) as usize;
    for i in 1..=steps {
        let t = i as f64 / steps as f64;
        let mt = 1.0 - t;
        let a = mt * mt * mt;
        let b = 3.0 * mt * mt * t;
        let c = 3.0 * mt * t * t;
        let d = t * t * t;
        out.push((
            a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
            a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
        ));
    }
}

fn dist(a: Pt, b: Pt) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Fill closed polygons with the even-odd rule. `aa` gives 4x4 supersampled edges.
pub fn fill_polygons(img: &mut RgbaImage, polys: &[Vec<Pt>], color: Rgba, aa: bool) {
    fill_polygons_with(img, polys, aa, &|_, _| color);
}

/// Like [`fill_polygons`], with a per-pixel colour (`shade(x, y)` at the pixel centre), used for
/// gradient fills.
pub fn fill_polygons_with(
    img: &mut RgbaImage,
    polys: &[Vec<Pt>],
    aa: bool,
    shade: &dyn Fn(f64, f64) -> Rgba,
) {
    let pts = polys.iter().flatten();
    let (mut minx, mut miny, mut maxx, mut maxy) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in pts {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        minx = minx.min(x);
        miny = miny.min(y);
        maxx = maxx.max(x);
        maxy = maxy.max(y);
    }
    if minx > maxx {
        return;
    }
    let x0 = (minx.floor() as i64).max(0);
    let y0 = (miny.floor() as i64).max(0);
    let x1 = (maxx.ceil() as i64).min(img.width() as i64);
    let y1 = (maxy.ceil() as i64).min(img.height() as i64);
    let ss: i64 = if aa { 4 } else { 1 };
    let w = (x1 - x0).max(0) as usize;
    let mut cov = vec![0u32; w];
    let mut xs: Vec<f64> = Vec::new();
    for py in y0..y1 {
        cov.iter_mut().for_each(|c| *c = 0);
        for sy in 0..ss {
            let y = py as f64 + (sy as f64 + 0.5) / ss as f64;
            xs.clear();
            for poly in polys {
                let n = poly.len();
                for i in 0..n {
                    let (Some(&a), Some(&b)) = (poly.get(i), poly.get((i + 1) % n)) else {
                        continue;
                    };
                    if (a.1 <= y && b.1 > y) || (b.1 <= y && a.1 > y) {
                        xs.push(a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0));
                    }
                }
            }
            xs.sort_by(f64::total_cmp);
            for pair in xs.chunks_exact(2) {
                let (l, r) = (pair[0], pair[1]);
                for sx in 0..(w as i64 * ss) {
                    let x = x0 as f64 + (sx as f64 + 0.5) / ss as f64;
                    if x >= l && x < r {
                        if let Some(c) = cov.get_mut((sx / ss) as usize) {
                            *c += 1;
                        }
                    }
                }
            }
        }
        let full = (ss * ss) as u32;
        for (i, &c) in cov.iter().enumerate() {
            if c == 0 {
                continue;
            }
            let x = x0 + i as i64;
            if let Some(d) = img.get(x, py) {
                let opacity = (c * 255 / full) as u8;
                img.set(x, py, d.over(shade(x as f64 + 0.5, py as f64 + 0.5), opacity));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_fill_covers_exactly() {
        let mut img = RgbaImage::new(10, 10, Rgba::WHITE);
        let sq = vec![(2.0, 2.0), (8.0, 2.0), (8.0, 8.0), (2.0, 8.0)];
        fill_polygons(&mut img, &[sq], Rgba::BLACK, true);
        assert_eq!(img.get(2, 2), Some(Rgba::BLACK));
        assert_eq!(img.get(7, 7), Some(Rgba::BLACK));
        assert_eq!(img.get(8, 8), Some(Rgba::WHITE));
        assert_eq!(img.get(1, 5), Some(Rgba::WHITE));
    }

    #[test]
    fn half_pixel_edge_is_antialiased() {
        let mut img = RgbaImage::new(4, 4, Rgba::WHITE);
        fill_polygons(
            &mut img,
            &[vec![(0.0, 0.0), (1.5, 0.0), (1.5, 4.0), (0.0, 4.0)]],
            Rgba::BLACK,
            true,
        );
        let g = img.get(1, 1).unwrap().r;
        assert!((100..160).contains(&g), "{g}");
    }

    #[test]
    fn hostile_geometry_is_ignored() {
        let mut img = RgbaImage::new(4, 4, Rgba::WHITE);
        fill_polygons(
            &mut img,
            &[vec![(f64::NAN, 0.0), (1.0, 1.0), (0.0, 1.0)]],
            Rgba::BLACK,
            true,
        );
        assert_eq!(img.get(0, 0), Some(Rgba::WHITE));
    }
}

/// A fill with a feathered edge of radius `r`: the anti-aliased coverage filtered with Fireworks'
/// triangle filter, then shaded. Adapted from FirePixel's `fp-vector/src/feather.rs` (Apache-2.0;
/// see ATTRIBUTION.md): width `2r + 1`, weights `1, 2, .. r + 1, .. 1`, horizontal then vertical,
/// each pass dividing by `(r + 1)^2` and truncating; the last row that has coverage comes out as
/// `screen(v, v)`.
pub fn fill_polygons_feathered(
    img: &mut RgbaImage,
    polys: &[Vec<Pt>],
    r: usize,
    shade: &dyn Fn(f64, f64) -> Rgba,
) {
    let r = r.min(1024);
    let (w, h) = (img.width() as usize, img.height() as usize);
    let (mw, mh) = (w + 2 * r, h + 2 * r);
    let mut big = RgbaImage::new(mw as u32, mh as u32, Rgba::TRANSPARENT);
    let moved: Vec<Vec<Pt>> = polys
        .iter()
        .map(|p| p.iter().map(|q| (q.0 + r as f64, q.1 + r as f64)).collect())
        .collect();
    fill_polygons(&mut big, &moved, Rgba::WHITE, true);
    let cover: Vec<u32> = big.pixels().iter().map(|p| p.a as u32).collect();
    let den = ((r + 1) * (r + 1)) as u64;
    let mut sums = Vec::new();
    let mut across = vec![0u32; w * mh];
    let mut last_row = None;
    for (j, row) in cover.chunks_exact(mw).enumerate() {
        if row.iter().all(|&v| v == 0) {
            continue;
        }
        last_row = Some(j);
        triangle_sums(row, r, w, &mut sums);
        for (d, s) in across[j * w..(j + 1) * w].iter_mut().zip(&sums) {
            *d = (s / den) as u32;
        }
    }
    let mut column = vec![0u32; mh];
    for i in 0..w {
        for (j, c) in column.iter_mut().enumerate() {
            *c = across.get(j * w + i).copied().unwrap_or(0);
        }
        triangle_sums(&column, r, h, &mut sums);
        for (j, s) in sums.iter().enumerate().take(h) {
            let mut v = (s / den) as u32;
            if Some(j) == last_row {
                let t = (255 - v) * (255 - v) + 0x80;
                v = 255 - (((t >> 8) + t) >> 8);
            }
            if v != 0 {
                let (x, y) = (i as i64, j as i64);
                if let Some(d) = img.get(x, y) {
                    img.set(
                        x,
                        y,
                        d.over(shade(i as f64 + 0.5, j as f64 + 0.5), v.min(255) as u8),
                    );
                }
            }
        }
    }
}

/// `out[i] = sum over k of a[i + k] * w[k]`, `k` in `0..=2r`, for `i < n_out`, the triangle being
/// two box filters of width `r + 1` (missing values count as 0).
fn triangle_sums(a: &[u32], r: usize, n_out: usize, out: &mut Vec<u64>) {
    let n = a.len();
    let mut pre = vec![0u64; n + 1];
    let mut total = 0;
    for (p, &v) in pre.iter_mut().skip(1).zip(a) {
        total += v as u64;
        *p = total;
    }
    let at = |i: usize| pre.get(i.min(n)).copied().unwrap_or(0);
    let nb = n_out + r;
    let mut box_pre = vec![0u64; nb + 1];
    let mut total = 0;
    for (i, p) in box_pre.iter_mut().skip(1).enumerate() {
        total += at(i + r + 1) - at(i);
        *p = total;
    }
    let boxed = |i: usize| box_pre.get(i.min(nb)).copied().unwrap_or(0);
    out.clear();
    out.extend((0..n_out).map(|i| boxed(i + r + 1) - boxed(i)));
}
