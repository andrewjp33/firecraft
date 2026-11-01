//! Gradient fills.
//!
//! A Fireworks gradient has colour stops and opacity stops (each 0..1 along the ramp) and three
//! handles in canvas coordinates: `start` (`PSXf/PSYf`), `end` (`PEXf/PEYf`) and a perpendicular
//! `side` handle (`PFXf/PFYf`) that sets the second axis for 2D shapes. With
//! `u` = position along start->end and `v` = along start->side (both 1.0 at the handle):
//!
//! | kind | ramp position |
//! |---|---|
//! | Linear | u |
//! | Radial (`fc_Circular`) | distance from start / length of start->end |
//! | Elliptical | sqrt(u^2 + v^2) |
//! | Rectangular | max(abs u, abs v) |
//! | Cone (`fc_Conical`) | angle between start->point and start->end, over pi |
//!
//! The decorative kinds (Bars, Ripple, Waves, Satin, Folds, Pinch) and the exact handle frame
//! come from Fireworks MX's gradient function (`0x61ea90`); see docs/behavior/gradients.md.

use crate::image::Rgba;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientKind {
    Linear,
    Radial,
    Elliptical,
    Rectangular,
    Cone,
    Starburst,
    Bars,
    Ripples,
    Waves,
    Satin,
    Folds,
    Pinch,
}

impl GradientKind {
    /// From a Fireworks fill category (`fc_Linear`, ...); `None` for solid and patterns.
    pub fn from_category(cat: &str) -> Option<Self> {
        Some(match cat {
            "fc_Linear" => Self::Linear,
            "fc_Circular" | "fc_Radial" => Self::Radial,
            "fc_Elliptical" => Self::Elliptical,
            "fc_Rectangular" => Self::Rectangular,
            "fc_Conical" | "fc_Cone" => Self::Cone,
            "fc_Starburst" => Self::Starburst,
            "fc_Bars" => Self::Bars,
            "fc_Ripple" | "fc_Ripples" => Self::Ripples,
            "fc_Waves" => Self::Waves,
            "fc_Satin" => Self::Satin,
            "fc_Folds" => Self::Folds,
            "fc_Pinch" => Self::Pinch,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    /// (position 0..1, colour), sorted.
    pub colors: Vec<(f32, Rgba)>,
    /// (position 0..1, opacity 0..=255), sorted; empty means opaque.
    pub opacity: Vec<(f32, u8)>,
    pub start: (f64, f64),
    pub end: (f64, f64),
    pub side: (f64, f64),
}

fn ramp<T: Copy>(stops: &[(f32, T)], t: f32, lerp: impl Fn(T, T, f32) -> T, default: T) -> T {
    let Some(&(p0, first)) = stops.first() else {
        return default;
    };
    if t <= p0 {
        return first;
    }
    for w in stops.windows(2) {
        let ((pa, a), (pb, b)) = (w[0], w[1]);
        if t <= pb {
            let f = if pb > pa { (t - pa) / (pb - pa) } else { 1.0 };
            return lerp(a, b, f);
        }
    }
    stops.last().map(|s| s.1).unwrap_or(default)
}

impl Gradient {
    /// Ramp position for the canvas point (`x`, `y`): Fireworks MX's formulas, read from its
    /// gradient function (`0x61ea90`). `p` = point - start, `d` = end - start; (u, v) = the point
    /// in the handle frame (start->end, start->side; 1.0 at each handle); a = p.y/d.y and
    /// b = p.x/d.x (9999999.9 when the divisor is 0).
    pub fn position(&self, x: f64, y: f64) -> f32 {
        const HUGE: f64 = 9_999_999.9;
        let (sx, sy) = self.start;
        let (dx, dy) = (self.end.0 - sx, self.end.1 - sy);
        let (bx, by) = (self.side.0 - sx, self.side.1 - sy);
        let (px, py) = (x - sx, y - sy);
        let ld = dx * dx + dy * dy;
        // Handle frame: solve p = u d + v b.
        let det = dx * by - dy * bx;
        let (u, v) = if det.abs() > 1e-12 {
            ((px * by - py * bx) / det, (dx * py - dy * px) / det)
        } else {
            let lb = (bx * bx + by * by).max(1e-12);
            ((px * dx + py * dy) / ld.max(1e-12), (px * bx + py * by) / lb)
        };
        let a = if dy == 0.0 { HUGE } else { py / dy };
        let b = if dx == 0.0 { HUGE } else { px / dx };
        let t = match self.kind {
            GradientKind::Linear | GradientKind::Bars => (px * dx + py * dy) / ld.max(1e-12),
            GradientKind::Radial => (px * px + py * py).sqrt() / ld.sqrt().max(1e-12),
            GradientKind::Cone => {
                // The angle between p and d over pi (law of cosines, as Fireworks computes it).
                let (qx, qy) = (x - self.end.0, y - self.end.1);
                let lp = px * px + py * py;
                let c = (ld + lp - (qx * qx + qy * qy)) / (2.0 * ld.sqrt() * lp.sqrt());
                if c.is_finite() {
                    c.clamp(-1.0, 1.0).acos() * std::f64::consts::FRAC_1_PI
                } else {
                    0.0
                }
            }
            GradientKind::Satin => ((a.sin() * (b - a)) - (3.0 * b).cos()).abs() * 0.5,
            GradientKind::Pinch => (u * v).abs(),
            GradientKind::Folds => {
                let x1 = (b + 0.25f64.sin()).sin() * (a + b).cos();
                ((3.0 * b * x1).cos() * (24.0 * a * b).sin() * (a + b * a.sin()) * 0.5).abs()
            }
            GradientKind::Elliptical | GradientKind::Ripples => (u * u + v * v).sqrt(),
            GradientKind::Rectangular => u.abs().max(v.abs()),
            GradientKind::Waves => (a.sin() * (b - a) * (b - a) - (3.0 * b).cos() * (b - a)).abs() * 0.5,
            // Not in Fireworks MX (a later fill type); kept for files from newer versions.
            GradientKind::Starburst => {
                let ang = py.atan2(px) - dy.atan2(dx);
                (px * px + py * py).sqrt() / ld.sqrt().max(1e-12) * (0.75 + 0.25 * (8.0 * ang).cos())
            }
        };
        // Bars, Ripple, Satin, Pinch and Folds repeat, mirrored, past 1.
        let t = match self.kind {
            GradientKind::Bars
            | GradientKind::Ripples
            | GradientKind::Satin
            | GradientKind::Pinch
            | GradientKind::Folds => {
                let t = t.abs();
                if t > 1.0 && t.is_finite() {
                    let n = t.trunc();
                    let f = t - n;
                    if (n as i64) % 2 == 1 { 1.0 - f } else { f }
                } else {
                    t
                }
            }
            _ => t,
        };
        if t.is_finite() {
            t.clamp(0.0, 1.0) as f32
        } else {
            0.0
        }
    }

    pub fn color_at(&self, x: f64, y: f64) -> Rgba {
        let t = self.position(x, y);
        let lerp_c = |a: Rgba, b: Rgba, f: f32| {
            let m = |p: u8, q: u8| (p as f32 + (q as f32 - p as f32) * f).round() as u8;
            Rgba::new(m(a.r, b.r), m(a.g, b.g), m(a.b, b.b), m(a.a, b.a))
        };
        let c = ramp(&self.colors, t, lerp_c, Rgba::BLACK);
        let a = ramp(
            &self.opacity,
            t,
            |a: u8, b: u8, f| (a as f32 + (b as f32 - a as f32) * f).round() as u8,
            255,
        );
        Rgba {
            a: (c.a as u32 * a as u32 / 255) as u8,
            ..c
        }
    }

    /// Move all handles.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        for p in [&mut self.start, &mut self.end, &mut self.side] {
            p.0 += dx;
            p.1 += dy;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(kind: GradientKind) -> Gradient {
        Gradient {
            kind,
            colors: vec![(0.0, Rgba::BLACK), (1.0, Rgba::WHITE)],
            opacity: vec![],
            start: (0.0, 0.0),
            end: (100.0, 0.0),
            side: (0.0, 100.0),
        }
    }

    #[test]
    fn linear_and_rectangular() {
        let l = g(GradientKind::Linear);
        assert_eq!(l.color_at(0.0, 50.0), Rgba::BLACK);
        assert_eq!(l.color_at(50.0, 50.0).r, 128);
        assert_eq!(l.color_at(150.0, 0.0), Rgba::WHITE);
        let r = g(GradientKind::Rectangular);
        assert_eq!(r.color_at(50.0, 20.0).r, 128);
        assert_eq!(r.color_at(20.0, -50.0).r, 128);
    }

    #[test]
    fn opacity_stops_scale_alpha() {
        let mut l = g(GradientKind::Linear);
        l.opacity = vec![(0.0, 0), (1.0, 76)];
        assert_eq!(l.color_at(0.0, 0.0).a, 0);
        assert_eq!(l.color_at(100.0, 0.0).a, 76);
    }

    #[test]
    fn degenerate_handles_do_not_panic() {
        let mut l = g(GradientKind::Cone);
        l.end = l.start;
        l.side = l.start;
        let _ = l.color_at(5.0, 5.0);
        assert_eq!(ramp::<u8>(&[], 0.5, |a, _, _| a, 7), 7);
    }
}
