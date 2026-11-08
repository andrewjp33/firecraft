//! Magic Wand and Select Similar.
//!
//! # Fireworks MX behaviour (measured, see `docs/behavior/magic-wand.md`)
//!
//! * Colour distance from the clicked (seed) pixel:
//!   `d = max(0, max(ΔR, ΔG, ΔB)) + max(0, -min(ΔR, ΔG, ΔB))`, i.e. the largest channel
//!   increase plus the largest channel decrease.
//! * The selected region is the 4-connected set of pixels with `d <= tolerance`.
//! * **Hard**: the region at full strength.
//! * **Anti-alias**: additionally, each pixel outside the region that is 4-adjacent to it is
//!   selected at `clamp(255 + tolerance - d, 0, 255)`. On a hard colour edge (`d` = 255) that
//!   is exactly `tolerance`, so the effect vanishes at low tolerance (tolerance 0 behaves like
//!   Hard) and only shows near tolerance ~100, where the region itself starts leaking. This is
//!   [`AntiAliasStyle::Classic`].
//! * **Feather**: the anti-aliased mask, then feathered by the amount.
//!
//! # FireCraft improvement
//!
//! [`AntiAliasStyle::Smooth`] (the default) decouples edge softness from tolerance: each edge
//! pixel's coverage is estimated by *unmixing* it between the seed colour and the colour beyond
//! the edge (recovering the partial pixels of an anti-aliased source), and staircase edges are
//! smoothed geometrically. Tolerance then only decides which colours belong to the region.

use std::collections::VecDeque;

use crate::image::{Rgba, RgbaImage};
use crate::mask::Mask;

/// Edge treatment, as in the Property inspector's Edge menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Hard,
    AntiAlias,
    /// Feather amount in pixels.
    Feather(u32),
}

/// How Anti-alias edges are computed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AntiAliasStyle {
    /// FireCraft's coverage-based edge (independent of tolerance).
    #[default]
    Smooth,
    /// Bit-compatible with Fireworks MX (edge strength equals tolerance).
    Classic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WandOptions {
    /// 0..=255, as in the Property inspector.
    pub tolerance: u8,
    pub edge: Edge,
    pub aa_style: AntiAliasStyle,
    /// `true`: flood from the clicked pixel (Magic Wand). `false`: every matching pixel in the
    /// image (Select > Select Similar).
    pub contiguous: bool,
}

impl Default for WandOptions {
    fn default() -> Self {
        // Fireworks MX's shipped default tolerance is 32.
        Self {
            tolerance: 32,
            edge: Edge::AntiAlias,
            aa_style: AntiAliasStyle::Smooth,
            contiguous: true,
        }
    }
}

/// Fireworks' colour distance (see module docs).
pub fn distance(seed: Rgba, p: Rgba) -> u32 {
    // Alpha counts like a colour channel (measured with the Paint Bucket and the wand on soft
    // edges in Fireworks MX: docs/behavior/painting.md).
    let d = [
        p.r as i32 - seed.r as i32,
        p.g as i32 - seed.g as i32,
        p.b as i32 - seed.b as i32,
        p.a as i32 - seed.a as i32,
    ];
    let up = d.iter().copied().max().unwrap_or(0).max(0);
    let down = (-d.iter().copied().min().unwrap_or(0)).max(0);
    (up + down) as u32
}

const N4: [(i64, i64); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// Select from the pixel at (`x`, `y`). Returns an empty mask if the point is off the image.
pub fn magic_wand(img: &RgbaImage, x: i64, y: i64, opts: WandOptions) -> Mask {
    let mut mask = Mask::new(img.width(), img.height());
    let Some(seed) = img.get(x, y) else { return mask };
    let tol = opts.tolerance as u32;

    // 1. Region.
    if opts.contiguous {
        let mut queue = VecDeque::from([(x, y)]);
        mask.set(x, y, 255);
        while let Some((cx, cy)) = queue.pop_front() {
            for (dx, dy) in N4 {
                let (nx, ny) = (cx + dx, cy + dy);
                match img.get(nx, ny) {
                    Some(p) if mask.get(nx, ny) == 0 && distance(seed, p) <= tol => {
                        mask.set(nx, ny, 255);
                        queue.push_back((nx, ny));
                    }
                    _ => {}
                }
            }
        }
    } else {
        for (m, &p) in mask.data_mut().iter_mut().zip(img.pixels()) {
            if distance(seed, p) <= tol {
                *m = 255;
            }
        }
    }

    // 2. Edge.
    match opts.edge {
        Edge::Hard => {}
        Edge::AntiAlias => anti_alias(img, &mut mask, seed, opts),
        Edge::Feather(amount) => {
            anti_alias(img, &mut mask, seed, opts);
            mask.feather(amount);
        }
    }
    mask
}

fn ring_pixels(mask: &Mask) -> Vec<(i64, i64)> {
    let mut ring = Vec::new();
    for y in 0..mask.height() as i64 {
        for x in 0..mask.width() as i64 {
            if mask.get(x, y) == 0 && N4.iter().any(|(dx, dy)| mask.get(x + dx, y + dy) == 255) {
                ring.push((x, y));
            }
        }
    }
    ring
}

fn anti_alias(img: &RgbaImage, mask: &mut Mask, seed: Rgba, opts: WandOptions) {
    let tol = opts.tolerance as i64;
    let ring = ring_pixels(mask);
    let values: Vec<u8> = ring
        .iter()
        .map(|&(x, y)| {
            let Some(p) = img.get(x, y) else { return 0 };
            match opts.aa_style {
                AntiAliasStyle::Classic => (255 + tol - distance(seed, p) as i64).clamp(0, 255) as u8,
                AntiAliasStyle::Smooth => smooth_coverage(img, mask, seed, x, y, p),
            }
        })
        .collect();
    for (&(x, y), v) in ring.iter().zip(values) {
        mask.set(x, y, v);
    }
}

/// Coverage for one edge pixel: max of colour unmixing and geometric smoothing.
fn smooth_coverage(img: &RgbaImage, mask: &Mask, seed: Rgba, x: i64, y: i64, p: Rgba) -> u8 {
    // Colour beyond the edge: among 8-neighbours outside the region, the one farthest from seed.
    let mut outside = p;
    let mut best = distance(seed, p);
    let mut inside_count = 0u32;
    for dy in -1..=1 {
        for dx in -1..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            if mask.get(x + dx, y + dy) == 255 {
                inside_count += 1;
                continue;
            }
            if let Some(q) = img.get(x + dx, y + dy) {
                let d = distance(seed, q);
                if d > best {
                    best = d;
                    outside = q;
                }
            }
        }
    }
    // Unmix p = a*seed + (1-a)*outside (least squares over RGB).
    let v = |c: Rgba| [c.r as f32, c.g as f32, c.b as f32];
    let (s, o, pp) = (v(seed), v(outside), v(p));
    let mut num = 0.0;
    let mut den = 0.0;
    for i in 0..3 {
        let e = s[i] - o[i];
        num += (pp[i] - o[i]) * e;
        den += e * e;
    }
    let unmix = if den > 1.0 {
        (num / den).clamp(0.0, 1.0)
    } else {
        0.0
    };
    // Geometric: share of the 3x3 block inside the region (a straight edge gives 1/3).
    let geo = inside_count as f32 / 9.0;
    (unmix.max(geo) * 255.0).round() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 100x100 white image with a hard red disc, like the oracle test image.
    fn disc(soft: bool) -> RgbaImage {
        let mut img = RgbaImage::new(100, 100, Rgba::WHITE);
        for y in 0..100 {
            for x in 0..100 {
                let cov = if soft {
                    let mut n = 0;
                    for sy in 0..8 {
                        for sx in 0..8 {
                            let fx = x as f64 + (sx as f64 + 0.5) / 8.0 - 50.0;
                            let fy = y as f64 + (sy as f64 + 0.5) / 8.0 - 50.0;
                            if fx * fx + fy * fy < 900.0 {
                                n += 1;
                            }
                        }
                    }
                    n as f64 / 64.0
                } else {
                    let (fx, fy) = (x as f64 - 49.5, y as f64 - 49.5);
                    if fx * fx + fy * fy < 900.0 { 1.0 } else { 0.0 }
                };
                let g = (255.0 * (1.0 - cov)).round() as u8;
                img.set(x, y, Rgba::new(255, g, g, 255));
            }
        }
        img
    }

    fn classic(tol: u8, edge: Edge) -> WandOptions {
        WandOptions {
            tolerance: tol,
            edge,
            aa_style: AntiAliasStyle::Classic,
            contiguous: true,
        }
    }

    fn histogram(m: &Mask) -> (usize, usize, Vec<u8>) {
        let full = m.data().iter().filter(|&&v| v == 255).count();
        let partial: Vec<u8> = m.data().iter().copied().filter(|&v| v > 0 && v < 255).collect();
        (full, partial.len(), partial)
    }

    // Oracle numbers below were measured in Fireworks MX (docs/behavior/magic-wand.md).
    #[test]
    fn classic_matches_fireworks_mx_on_hard_disc() {
        let img = disc(false);
        for (tol, expect) in [(10u8, 10u8), (32, 32), (102, 102)] {
            let (full, partial, vals) = histogram(&magic_wand(&img, 50, 50, classic(tol, Edge::AntiAlias)));
            assert_eq!(full, 2828);
            assert_eq!(partial, 172);
            assert!(vals.iter().all(|&v| v == expect), "tol {tol}");
        }
        // Tolerance 0: identical to Hard (the "anti-alias does nothing" report).
        let aa0 = magic_wand(&img, 50, 50, classic(0, Edge::AntiAlias));
        assert_eq!(aa0, magic_wand(&img, 50, 50, classic(0, Edge::Hard)));
    }

    #[test]
    fn distance_metric_matches_measurements() {
        let base = Rgba::new(120, 100, 90, 255);
        let cases = [
            ((30, 30, 30), 30),
            ((20, 60, 0), 60),
            ((0, 60, -20), 80),
            ((40, -40, 0), 80),
            ((25, -25, 25), 50),
            ((0, -60, 60), 120),
            ((15, 45, -15), 60),
        ];
        for ((dr, dg, db), want) in cases {
            let p = Rgba::new((120 + dr) as u8, (100 + dg) as u8, (90 + db) as u8, 255);
            assert_eq!(distance(base, p), want, "{dr},{dg},{db}");
        }
    }

    #[test]
    fn classic_soft_disc_matches_measured_pairs() {
        // (g of ring pixel, selected) pairs measured at tolerance 32: selected = 255 + 32 - g.
        let img = disc(true);
        let m = magic_wand(&img, 50, 50, classic(32, Edge::AntiAlias));
        for y in 0..100 {
            for x in 0..100 {
                let v = m.get(x, y);
                if v > 0 && v < 255 {
                    let g = img.get(x, y).unwrap().g as i64;
                    assert_eq!(v as i64, (255 + 32 - g).clamp(0, 255));
                }
            }
        }
    }

    #[test]
    fn smooth_aa_is_visible_at_any_tolerance() {
        let img = disc(false);
        for tol in [0u8, 10, 32] {
            let opts = WandOptions {
                tolerance: tol,
                ..WandOptions::default()
            };
            let (_, partial, vals) = histogram(&magic_wand(&img, 50, 50, opts));
            assert!(partial > 100, "tol {tol}");
            let mean = vals.iter().map(|&v| v as u32).sum::<u32>() / vals.len() as u32;
            assert!((60..200).contains(&mean), "tol {tol}: mean edge {mean}");
        }
    }

    #[test]
    fn smooth_aa_recovers_soft_edge_coverage() {
        let img = disc(true);
        let opts = WandOptions {
            tolerance: 10,
            ..WandOptions::default()
        };
        let m = magic_wand(&img, 50, 50, opts);
        // A ring pixel whose source is ~50% red should be ~50% selected.
        let mut checked = 0;
        for y in 0..100 {
            for x in 0..100 {
                let v = m.get(x, y);
                let g = img.get(x, y).unwrap().g;
                if v > 0 && v < 255 && (100..160).contains(&g) {
                    let truth = 255 - g as i32;
                    assert!((v as i32 - truth).abs() < 40, "({x},{y}) g={g} sel={v}");
                    checked += 1;
                }
            }
        }
        assert!(checked > 0);
    }

    #[test]
    fn off_image_click_is_empty() {
        let img = disc(false);
        assert!(magic_wand(&img, -1, 5, WandOptions::default()).is_empty());
        assert!(magic_wand(&RgbaImage::new(0, 0, Rgba::WHITE), 0, 0, WandOptions::default()).is_empty());
    }

    #[test]
    fn delete_after_classic_aa_reproduces_oracle_alpha() {
        let mut img = disc(false);
        let m = magic_wand(&img, 50, 50, classic(32, Edge::AntiAlias));
        img.clear_masked(&m);
        let partial: Vec<u8> = img
            .pixels()
            .iter()
            .map(|p| p.a)
            .filter(|&a| a > 0 && a < 255)
            .collect();
        assert_eq!(partial.len(), 172);
        assert!(partial.iter().all(|&a| a == 223)); // measured: 255 - 32
        assert_eq!(img.get(50, 50), Some(Rgba::CLEARED));
    }
}
