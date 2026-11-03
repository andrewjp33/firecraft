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

/// Stroke a polyline the way Fireworks MX's brushes do (measured, docs/behavior/strokes.md):
/// the brush tip is stamped ("dabs") every 1 px along the line, from its start up to (not
/// including) its end; vertices are first snapped down to 1/16 px. Overlapping dabs combine
/// like layered paint (coverage 1 - (1 - a)(1 - b)). An anti-aliased tip is placed at the
/// dab's exact position (each tip pixel spread over the 2x2 pixels under it, bilinearly); a
/// non-anti-aliased tip snaps to whole pixels. The tip is a disc `diameter` px across (a
/// 1-px anti-aliased tip is a single pixel); `softness` 0..=100 fades its edge (approximate).
pub fn stamp_stroke(
    img: &mut RgbaImage,
    line: &[Pt],
    color: Rgba,
    diameter: f64,
    softness: u8,
    anti_alias: bool,
) {
    if line.is_empty() || !(diameter > 0.0) || line.iter().any(|p| !p.0.is_finite() || !p.1.is_finite()) {
        return;
    }
    let d = (diameter.round() as i64).clamp(1, 1000);
    let tip = brush_tip(d as usize, softness.min(100) as f64 / 100.0, anti_alias);
    let q = |v: f64| (v * 16.0).floor() / 16.0;
    let pts: Vec<Pt> = line.iter().map(|p| (q(p.0), q(p.1))).collect();
    // Dab positions: 1 px apart along the line; each segment of length L gets round(L) dabs
    // starting at its start (a carried remainder keeps the spacing across vertices).
    let mut dabs: Vec<Pt> = Vec::new();
    if pts.len() == 1 {
        dabs.push(pts[0]);
    }
    for seg in pts.windows(2) {
        let (a, b) = (seg[0], seg[1]);
        let len = dist(a, b);
        if len <= 0.0 {
            continue;
        }
        let n = len.round().max(1.0) as usize;
        let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
        dabs.extend((0..n).map(|k| (a.0 + ux * k as f64, a.1 + uy * k as f64)));
    }
    // Coverage over the stroke's bounding box.
    let (w, h) = (img.width() as i64, img.height() as i64);
    let half = d as f64 / 2.0 + 2.0;
    let bx0 = (dabs.iter().map(|p| p.0).fold(f64::MAX, f64::min) - half)
        .floor()
        .max(0.0) as i64;
    let by0 = (dabs.iter().map(|p| p.1).fold(f64::MAX, f64::min) - half)
        .floor()
        .max(0.0) as i64;
    let bx1 = ((dabs.iter().map(|p| p.0).fold(f64::MIN, f64::max) + half).ceil() as i64).min(w);
    let by1 = ((dabs.iter().map(|p| p.1).fold(f64::MIN, f64::max) + half).ceil() as i64).min(h);
    if bx1 <= bx0 || by1 <= by0 {
        return;
    }
    let (cw, ch) = ((bx1 - bx0) as usize, (by1 - by0) as usize);
    // Coverage is kept as "what's left": 1 - coverage, multiplied by (1 - a) per dab.
    let mut left = vec![1.0f32; cw * ch];
    let mut paint = |x: i64, y: i64, a: f32| {
        if a > 0.0 && x >= bx0 && y >= by0 && x < bx1 && y < by1 {
            let k = (y - by0) as usize * cw + (x - bx0) as usize;
            if let Some(v) = left.get_mut(k) {
                *v *= 1.0 - a.min(1.0);
            }
        }
    };
    let dn = d as usize;
    for (cx, cy) in dabs {
        let (ox, oy) = (cx - d as f64 / 2.0, cy - d as f64 / 2.0);
        if anti_alias {
            let (bx, by) = (ox.floor(), oy.floor());
            let (fx, fy) = ((ox - bx) as f32, (oy - by) as f32);
            let wts = [
                (0, 0, (1.0 - fx) * (1.0 - fy)),
                (1, 0, fx * (1.0 - fy)),
                (0, 1, (1.0 - fx) * fy),
                (1, 1, fx * fy),
            ];
            for j in 0..dn {
                for i in 0..dn {
                    let a = tip[j * dn + i];
                    if a <= 0.0 {
                        continue;
                    }
                    for &(ex, ey, wt) in &wts {
                        paint(bx as i64 + i as i64 + ex, by as i64 + j as i64 + ey, a * wt);
                    }
                }
            }
        } else {
            let (bx, by) = ((ox + 0.5).floor() as i64, (oy + 0.5).floor() as i64);
            for j in 0..dn {
                for i in 0..dn {
                    paint(bx + i as i64, by + j as i64, tip[j * dn + i]);
                }
            }
        }
    }
    for y in 0..ch {
        for x in 0..cw {
            let c = 1.0 - left[y * cw + x];
            if c > 0.0 {
                let (px, py) = (bx0 + x as i64, by0 + y as i64);
                if let Some(dst) = img.get(px, py) {
                    img.set(
                        px,
                        py,
                        dst.over(color, (c * 255.0).round().clamp(0.0, 255.0) as u8),
                    );
                }
            }
        }
    }
}

/// A brush tip: `d` x `d` alpha values. Anti-aliased: the share of each pixel inside the disc
/// (a 1-px tip is one full pixel); otherwise the pixels whose centre is inside it.
fn brush_tip(d: usize, soft: f64, anti_alias: bool) -> Vec<f32> {
    let r = d as f64 / 2.0;
    let mut tip = vec![0.0f32; d * d];
    for j in 0..d {
        for i in 0..d {
            let cov = if d == 1 {
                1.0
            } else if anti_alias {
                const SS: usize = 8;
                let mut n = 0;
                for b in 0..SS {
                    for a in 0..SS {
                        let x = i as f64 + (a as f64 + 0.5) / SS as f64 - r;
                        let y = j as f64 + (b as f64 + 0.5) / SS as f64 - r;
                        if x * x + y * y <= r * r {
                            n += 1;
                        }
                    }
                }
                n as f64 / (SS * SS) as f64
            } else {
                let (x, y) = (i as f64 + 0.5 - r, j as f64 + 0.5 - r);
                if x * x + y * y <= r * r { 1.0 } else { 0.0 }
            };
            // Softness (interim fit, docs/behavior/strokes.md): the tip fades linearly over the
            // outer part of its radius, and each dab is weakened so that the dabs a pixel
            // receives along the line (about half the diameter) add up to that fade.
            let alpha = if soft > 0.0 && d > 1 {
                let rr = ((i as f64 + 0.5 - r).powi(2) + (j as f64 + 0.5 - r).powi(2)).sqrt() / r;
                let fade = ((1.0 + 0.2 * soft - rr) / (0.9 * soft)).clamp(0.0, 0.999_999);
                let dabs = (0.481 * d as f64).max(1.0);
                cov * (1.0 - (1.0 - fade).powf(1.0 / dabs))
            } else {
                cov
            };
            if let Some(t) = tip.get_mut(j * d + i) {
                *t = alpha as f32;
            }
        }
    }
    tip
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
        stamp_stroke(&mut img, &[(0.0, 0.0)], Rgba::BLACK, f64::NAN, 0, true);
        assert_eq!(img.get(0, 0), Some(Rgba::WHITE));
    }

    /// The 1-px anti-aliased pencil, as measured in Fireworks MX: a line on a row centre
    /// gives 191 in that row; on a row boundary 112/111 split; the last pixel before the end
    /// gets one dab (64 on a boundary).
    #[test]
    fn one_pixel_line_matches_fireworks() {
        let at = |y: f64| {
            let mut img = RgbaImage::new(80, 10, Rgba::BLACK);
            stamp_stroke(&mut img, &[(20.0, y), (60.0, y)], Rgba::WHITE, 1.0, 0, true);
            img
        };
        let c = at(4.5);
        assert_eq!(
            (c.get(40, 4).map(|p| p.r), c.get(40, 3).map(|p| p.r)),
            (Some(191), Some(0))
        );
        let b = at(5.0);
        assert_eq!(
            (b.get(40, 4).map(|p| p.r), b.get(40, 5).map(|p| p.r)),
            (Some(112), Some(112))
        );
        assert_eq!(
            (b.get(59, 4).map(|p| p.r), b.get(60, 4).map(|p| p.r)),
            (Some(64), Some(0))
        );
        // Quarter steps: Fireworks snaps to 1/16, and 0.0625 = 1 - (1 - 0.5 (1 - d))^2.
        let q = at(4.5625);
        assert_eq!(
            (q.get(40, 4).map(|p| p.r), q.get(40, 5).map(|p| p.r)),
            (Some(183), Some(16))
        );
    }

    #[test]
    fn stroke_paints_line() {
        let mut img = RgbaImage::new(20, 5, Rgba::WHITE);
        stamp_stroke(&mut img, &[(2.0, 2.5), (18.0, 2.5)], Rgba::BLACK, 3.0, 0, false);
        assert_eq!(img.get(10, 2), Some(Rgba::BLACK));
        assert_eq!(img.get(10, 0), Some(Rgba::WHITE));
    }
}
