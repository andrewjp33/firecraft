//! 2D affine transforms (symbol instances, the Transform tools).

use crate::image::{Rgba, RgbaImage};

/// A 2D transform in Fireworks' `MTXv` form (row vector times a 3x3 matrix): with
/// `w = g*x + h*y + i`, `x' = (a*x + c*y + e) / w`, `y' = (b*x + d*y + f) / w`.
/// a=M00, b=M01, g=M02, c=M10, d=M11, h=M12, e=M20, f=M21, i=M22. `g`, `h` and `i` are the
/// perspective terms (0, 0, 1 for an affine transform; Fireworks' Distort/perspective sets them).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
    pub g: f64,
    pub h: f64,
    pub i: f64,
}

impl Default for Affine {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
        g: 0.0,
        h: 0.0,
        i: 1.0,
    };

    pub fn translate(dx: f64, dy: f64) -> Self {
        Affine {
            e: dx,
            f: dy,
            ..Self::IDENTITY
        }
    }

    /// The transform taking the rectangle (`x`, `y`, `w`, `h`) onto the quadrilateral
    /// `quad` = [top-left, top-right, bottom-right, bottom-left] (a perspective transform in
    /// general; affine when the quad is a parallelogram). `None` for degenerate input.
    pub fn rect_to_quad(rect: (f64, f64, f64, f64), quad: [(f64, f64); 4]) -> Option<Affine> {
        let (x, y, w, h) = rect;
        if w.abs() < 1e-9 || h.abs() < 1e-9 {
            return None;
        }
        // Unit square -> quad (Heckbert's square-to-quad).
        let [(x0, y0), (x1, y1), (x2, y2), (x3, y3)] = quad;
        let (sx, sy) = (x0 - x1 + x2 - x3, y0 - y1 + y2 - y3);
        let unit = if sx.abs() < 1e-9 && sy.abs() < 1e-9 {
            Affine {
                a: x1 - x0,
                b: y1 - y0,
                c: x3 - x0,
                d: y3 - y0,
                e: x0,
                f: y0,
                ..Self::IDENTITY
            }
        } else {
            let (dx1, dx2, dy1, dy2) = (x1 - x2, x3 - x2, y1 - y2, y3 - y2);
            let den = dx1 * dy2 - dx2 * dy1;
            if den.abs() < 1e-12 {
                return None;
            }
            let g = (sx * dy2 - dx2 * sy) / den;
            let hh = (dx1 * sy - sx * dy1) / den;
            Affine {
                a: x1 - x0 + g * x1,
                b: y1 - y0 + g * y1,
                g,
                c: x3 - x0 + hh * x3,
                d: y3 - y0 + hh * y3,
                h: hh,
                e: x0,
                f: y0,
                i: 1.0,
            }
        };
        let to_unit = Affine {
            a: 1.0 / w,
            d: 1.0 / h,
            e: -x / w,
            f: -y / h,
            ..Self::IDENTITY
        };
        Some(unit.then(&to_unit))
    }

    /// Row-major 3x3 (row-vector convention).
    fn rows(&self) -> [[f64; 3]; 3] {
        [
            [self.a, self.b, self.g],
            [self.c, self.d, self.h],
            [self.e, self.f, self.i],
        ]
    }

    fn from_rows(m: [[f64; 3]; 3]) -> Self {
        let [[a, b, g], [c, d, h], [e, f, i]] = m;
        Affine {
            a,
            b,
            c,
            d,
            e,
            f,
            g,
            h,
            i,
        }
    }

    /// True when there is no perspective (the last column is 0, 0, i).
    pub fn is_affine(&self) -> bool {
        self.g.abs() < 1e-12 && self.h.abs() < 1e-12
    }

    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let w = self.g * x + self.h * y + self.i;
        let w = if w.abs() < 1e-12 { 1e-12_f64.copysign(w) } else { w };
        (
            (self.a * x + self.c * y + self.e) / w,
            (self.b * x + self.d * y + self.f) / w,
        )
    }

    /// `self` after `first`: apply(then(first)) = self.apply(first.apply(p)).
    pub fn then(&self, first: &Affine) -> Affine {
        let (p, q) = (first.rows(), self.rows());
        let mut m = [[0.0; 3]; 3];
        for (r, row) in m.iter_mut().enumerate() {
            for (c, cell) in row.iter_mut().enumerate() {
                *cell = (0..3).map(|k| p[r][k] * q[k][c]).sum();
            }
        }
        Self::from_rows(m)
    }

    /// The affine transform taking each point of `from` to the same point of `to` (within
    /// `tol` pixels), if there is one (at least three points, not all on a line).
    pub fn fit(from: &[(f64, f64)], to: &[(f64, f64)], tol: f64) -> Option<Affine> {
        if from.len() != to.len() || from.len() < 3 {
            return None;
        }
        // The three points spanning the largest triangle.
        let n = from.len().min(64);
        let mut best = (0.0, 0, 1, 2);
        for i in 0..n {
            for j in i + 1..n {
                for k in j + 1..n {
                    let (p, q, r) = (from[i], from[j], from[k]);
                    let v = ((q.0 - p.0) * (r.1 - p.1) - (r.0 - p.0) * (q.1 - p.1)).abs();
                    if v > best.0 {
                        best = (v, i, j, k);
                    }
                }
            }
        }
        let (v, i, j, k) = best;
        if v < 1e-6 {
            return None;
        }
        let (p, q, r) = (from[i], from[j], from[k]);
        let det = p.0 * (q.1 - r.1) - p.1 * (q.0 - r.0) + (q.0 * r.1 - r.0 * q.1);
        // Solve u = x * s + y * t + w for the three points.
        let solve = |u0: f64, u1: f64, u2: f64| {
            let s = (u0 * (q.1 - r.1) - p.1 * (u1 - u2) + (u1 * r.1 - u2 * q.1)) / det;
            let t = (p.0 * (u1 - u2) - u0 * (q.0 - r.0) + (q.0 * u2 - r.0 * u1)) / det;
            let w = (p.0 * (q.1 * u2 - r.1 * u1) - p.1 * (q.0 * u2 - r.0 * u1)
                + u0 * (q.0 * r.1 - r.0 * q.1))
                / det;
            (s, t, w)
        };
        let (a, c, e) = solve(to[i].0, to[j].0, to[k].0);
        let (b, d, f) = solve(to[i].1, to[j].1, to[k].1);
        let m = Affine {
            a,
            b,
            c,
            d,
            e,
            f,
            ..Affine::IDENTITY
        };
        from.iter()
            .zip(to)
            .all(|(&(x, y), &(tx, ty))| {
                let (mx, my) = m.apply(x, y);
                (mx - tx).abs() <= tol && (my - ty).abs() <= tol
            })
            .then_some(m)
    }

    /// Determinant of the linear (2x2) part, normalised by i.
    pub fn det(&self) -> f64 {
        (self.a * self.d - self.b * self.c) / (self.i * self.i).max(1e-24)
    }

    pub fn invert(&self) -> Option<Affine> {
        let m = self.rows();
        let det = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
        if det.abs() < 1e-12 || !det.is_finite() {
            return None;
        }
        let cof = |r0: usize, r1: usize, c0: usize, c1: usize| m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0];
        let inv = [
            [
                cof(1, 2, 1, 2) / det,
                -cof(0, 2, 1, 2) / det,
                cof(0, 1, 1, 2) / det,
            ],
            [
                -cof(1, 2, 0, 2) / det,
                cof(0, 2, 0, 2) / det,
                -cof(0, 1, 0, 2) / det,
            ],
            [
                cof(1, 2, 0, 1) / det,
                -cof(0, 2, 0, 1) / det,
                cof(0, 1, 0, 1) / det,
            ],
        ];
        Some(Self::from_rows(inv))
    }

    /// Average linear scale (for stroke widths and effect sizes).
    pub fn scale(&self) -> f64 {
        self.det().abs().sqrt()
    }

    /// Bounding box (x, y, w, h) of the transformed rectangle.
    pub fn map_rect(&self, x: f64, y: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
        let pts = [
            self.apply(x, y),
            self.apply(x + w, y),
            self.apply(x, y + h),
            self.apply(x + w, y + h),
        ];
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
    // A tolerance keeps float noise (an exact 90 degree turn landing on 9.999999) from adding
    // an empty row or column.
    const EPS: f64 = 1e-6;
    let (x0, y0) = ((bx + EPS).floor() as i64, (by + EPS).floor() as i64);
    let (w, h) = (
        (bx + bw - EPS).ceil() as i64 - x0,
        (by + bh - EPS).ceil() as i64 - y0,
    );
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
    fn compose_and_invert() {
        let s = Affine {
            a: 2.0,
            d: 3.0,
            ..Affine::IDENTITY
        };
        let t = Affine::translate(10.0, 5.0);
        let m = t.then(&s);
        assert_eq!(m.apply(1.0, 1.0), (12.0, 8.0));
        let back = m.invert().unwrap().apply(12.0, 8.0);
        assert!((back.0 - 1.0).abs() < 1e-9 && (back.1 - 1.0).abs() < 1e-9);
        assert!(
            Affine {
                a: 0.0,
                d: 0.0,
                ..Affine::IDENTITY
            }
            .invert()
            .is_none()
        );
    }

    #[test]
    fn perspective_maps_and_inverts() {
        let m = Affine {
            a: 1.1,
            b: 0.006,
            g: -1.1e-5,
            c: 0.063,
            d: 0.44,
            h: -0.0013,
            e: 127.7,
            f: 53.15,
            i: 0.83,
        };
        let (x, y) = m.apply(40.0, -25.0);
        let back = m.invert().unwrap().apply(x, y);
        assert!((back.0 - 40.0).abs() < 1e-9 && (back.1 + 25.0).abs() < 1e-9);
        let t = Affine::translate(5.0, -3.0).then(&m);
        let (tx, ty) = t.apply(40.0, -25.0);
        assert!((tx - x - 5.0).abs() < 1e-9 && (ty - y + 3.0).abs() < 1e-9);
        assert!(!m.is_affine() && Affine::IDENTITY.is_affine());
    }

    #[test]
    fn rect_to_quad_hits_the_corners() {
        let rect = (10.0, 20.0, 100.0, 50.0);
        let quad = [(5.0, 25.0), (120.0, 10.0), (100.0, 90.0), (30.0, 60.0)];
        let m = Affine::rect_to_quad(rect, quad).unwrap();
        for ((x, y), (qx, qy)) in [(10.0, 20.0), (110.0, 20.0), (110.0, 70.0), (10.0, 70.0)]
            .into_iter()
            .zip(quad)
        {
            let (px, py) = m.apply(x, y);
            assert!(
                (px - qx).abs() < 1e-9 && (py - qy).abs() < 1e-9,
                "{px},{py} vs {qx},{qy}"
            );
        }
        // A parallelogram stays affine.
        let skew =
            Affine::rect_to_quad(rect, [(10.0, 20.0), (110.0, 20.0), (130.0, 70.0), (30.0, 70.0)]).unwrap();
        assert!(skew.is_affine());
    }

    #[test]
    fn scaling_an_image_doubles_it() {
        let img = RgbaImage::new(4, 4, Rgba::new(255, 0, 0, 255));
        let (out, x, y) = transform_image(
            &img,
            10,
            10,
            &Affine {
                a: 2.0,
                d: 2.0,
                ..Affine::IDENTITY
            },
        )
        .unwrap();
        assert_eq!((x, y, out.width(), out.height()), (20, 20, 8, 8));
        assert_eq!(out.get(4, 4), Some(Rgba::new(255, 0, 0, 255)));
    }
}
