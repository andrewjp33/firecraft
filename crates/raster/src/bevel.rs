//! Fireworks MX's Bevel and Emboss effect (`BevelType` 0-3: Inner Bevel, Outer Bevel, Raised
//! Emboss, Inset Emboss). BevelType 4 is the Glow, see `effects`.
//!
//! Read from `Fireworks.exe` with REA/disassembly and checked against Fireworks renders
//! (docs/behavior/bevel.md):
//!
//! - every pixel in the band (inside the object for Inner Bevel, outside it otherwise) at
//!   Euclidean distance `d` <= width from the edge gets the outward normal angle `n` of its
//!   nearest edge pixel (integer atan2 with Fireworks' table, `0x5649f0`);
//! - edge shape (`0x5dcad0`): `s = profile(t) * SlopeMultiplier`, `t = d / width` (Outer:
//!   `1 - d / width`); |s| > 1 becomes 1/s; Inset Emboss and the Down button states negate it;
//! - lighting (`0x5dcbc0`): `a = LightAngle + n` (minus 180 when s < 0) in (-180, 180];
//!   highlight = 255 * max(0, 1 - |a|/90), shadow = 255 * max(0, |a|/120 - 1/2) (Emboss:
//!   max(0, |a|/90 - 1)), each rounded and then scaled by |s| * contrast (truncated);
//! - AngleSoftness blurs the highlight and shadow maps (sigma ~0.26 x softness, capped at 10),
//!   kept inside the band; shadow colour then highlight colour are composited over the band;
//! - button states: Over adds HitBlendColor over the result, Down inverts and adds
//!   DownBlendColor, Over While Down inverts and adds HitBlendColor.

use std::collections::HashMap;

use crate::image::{Rgba, RgbaImage};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BevelKind {
    Inner,
    Outer,
    RaisedEmboss,
    InsetEmboss,
}

impl BevelKind {
    fn emboss(self) -> bool {
        matches!(self, BevelKind::RaisedEmboss | BevelKind::InsetEmboss)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bevel {
    pub kind: BevelKind,
    pub width: f32,
    /// BevelContrast, 0..=100 (percent).
    pub contrast: f32,
    pub softness: f32,
    /// Degrees, counter-clockwise from the right (Fireworks' LightAngle).
    pub light_angle: f32,
    /// 0 Flat, 1 Smooth, 2 Sloped, 3 Frame 1, 4 Frame 2, 5 Ring, 6 Ruffle.
    pub slope: u8,
    pub multiplier: f32,
    /// 0 Up, 1 Over, 2 Down, 3 Over While Down.
    pub button: u8,
    pub hilite: Rgba,
    pub shadow: Rgba,
    /// Outer Bevel's band colour.
    pub outer_color: Rgba,
    /// Emboss: what shows in place of the object when `show_object` is off.
    pub face: Rgba,
    pub show_object: bool,
    pub hit_color: Rgba,
    pub down_color: Rgba,
}

impl Bevel {
    /// From the effect's parameters (BevelType 0-3).
    pub fn parse(p: &HashMap<String, String>) -> Option<Bevel> {
        let num = |k: &str, d: f32| {
            p.get(k)
                .and_then(|v| v.trim().parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .unwrap_or(d)
        };
        let color = |k: &str, d: Rgba| p.get(k).and_then(|v| Rgba::from_hex(v)).unwrap_or(d);
        let kind = match num("BevelType", 0.0) as i32 {
            0 => BevelKind::Inner,
            1 => BevelKind::Outer,
            2 => BevelKind::RaisedEmboss,
            3 => BevelKind::InsetEmboss,
            _ => return None,
        };
        Some(Bevel {
            kind,
            width: num("BevelWidth", 10.0),
            contrast: num("BevelContrast", 75.0),
            softness: num("AngleSoftness", 3.0),
            light_angle: num("LightAngle", 135.0),
            slope: num("SlopeType", 0.0).clamp(0.0, 6.0) as u8,
            multiplier: num("SlopeMultiplier", 1.0),
            button: num("ButtonState", 0.0).clamp(0.0, 3.0) as u8,
            hilite: color("HiliteColor", Rgba::WHITE),
            shadow: color("ShadowColor", Rgba::BLACK),
            outer_color: color("OuterBevelColor", Rgba::new(0xdf, 0, 0, 255)),
            face: color("EmbossFaceColor", Rgba::TRANSPARENT),
            show_object: p.get("ShowObject").is_some_and(|v| v == "true" || v == "1"),
            hit_color: color("HitBlendColor", Rgba::new(255, 255, 255, 0x3f)),
            down_color: color("DownBlendColor", Rgba::new(0, 0, 0, 0x3f)),
        })
    }

    /// Pixels the effect may draw outside the object.
    pub fn margin(&self) -> f32 {
        match self.kind {
            BevelKind::Inner => 0.0,
            _ => self.width.max(0.0) + 2.0,
        }
    }

    /// Edge shape (Fireworks `0x5dcad0`) at `t` in 0..=1.
    pub fn profile(slope: u8, t: f64) -> f64 {
        match slope {
            0 => 1.0,
            1 => 1.0 - t,
            2 => t,
            3 => {
                if t > 0.65 {
                    -1.0
                } else {
                    1.0
                }
            }
            4 => {
                if t > 0.7 {
                    -1.0
                } else if t > 0.4 {
                    0.0
                } else {
                    1.0
                }
            }
            5 => (std::f64::consts::PI * t).cos(),
            _ => {
                let c = (std::f64::consts::PI * t * 3.0).cos();
                if t > 0.33333 && t < 0.66667 { -c } else { c }
            }
        }
    }

    /// Highlight and shadow alpha (0..=255) for a band pixel at distance `d` with normal `n`.
    /// Signed edge-shape value at distance `d` (Fireworks `0x5dcbc0`, before lighting).
    fn slope_at(&self, d: f64) -> f64 {
        let w = self.width.max(1.0) as f64;
        let t = (if self.kind == BevelKind::Outer {
            1.0 - d / w
        } else {
            d / w
        })
        .clamp(0.0, 1.0);
        // Embosses ignore the edge shape (measured).
        let mut s = if self.kind.emboss() {
            1.0
        } else {
            Self::profile(self.slope, t) * self.multiplier as f64
        };
        if self.kind == BevelKind::InsetEmboss {
            s = -s;
        }
        if self.button >= 2 {
            s = -s;
        }
        if s.abs() > 1.0 {
            s = 1.0 / s;
        }
        s
    }

    /// Highlight and shadow alpha (0..=255) for a surface tilted towards angle `n` (degrees,
    /// y-down) by `strength` (|s|; a negative slope is a normal turned by 180 degrees).
    fn light(&self, n: f64, strength: f64) -> (f32, f32) {
        let a = (self.light_angle.round() as f64 + n + 180.0).rem_euclid(360.0) - 180.0;
        let a = a.abs();
        let round = |x: f64| (x * 255.0).round();
        let hi = round((1.0 - a / 90.0).max(0.0));
        let sh = if self.kind.emboss() {
            round((a / 90.0 - 1.0).max(0.0))
        } else {
            round((a / 120.0 - 0.5).max(0.0))
        };
        let k = strength.min(1.0) * (self.contrast as f64 * 0.01);
        (
            (hi * k).trunc().clamp(0.0, 255.0) as f32,
            (sh * k).trunc().clamp(0.0, 255.0) as f32,
        )
    }

    /// Apply to the object's pixels in `img` (with room around the object for outer bands).
    pub fn apply(&self, img: &mut RgbaImage) {
        let (w, h) = (img.width() as usize, img.height() as usize);
        if w == 0 || h == 0 || self.width <= 0.0 {
            return;
        }
        let inside: Vec<bool> = img.pixels().iter().map(|p| p.a >= 128).collect();
        if !inside.iter().any(|&b| b) {
            return;
        }
        let band = self.band(&inside, w, h);
        let (hi, sh) = self.shading(&band, w, h);
        let src = img.clone();
        for (i, p) in img.pixels_mut().iter_mut().enumerate() {
            let px = BandPixel {
                orig: src.pixels().get(i).copied().unwrap_or_default(),
                in_band: band.on.get(i).copied().unwrap_or(false),
                inside: inside.get(i).copied().unwrap_or(false),
                cover: band.cover.get(i).copied().unwrap_or(1.0),
                hi: hi.get(i).copied().unwrap_or(0.0),
                sh: sh.get(i).copied().unwrap_or(0.0),
            };
            *p = self.composite(&px);
        }
    }

    /// Band pixels with their distance, coverage and outward normal.
    fn band(&self, inside: &[bool], w: usize, h: usize) -> Band {
        // Inner: distance to the nearest outside pixel; otherwise to the nearest object pixel.
        let inner = self.kind == BevelKind::Inner;
        let seeds: Vec<bool> = if inner {
            inside.iter().map(|&b| !b).collect()
        } else {
            inside.to_vec()
        };
        let near = nearest_seed(&seeds, w, h, inner);
        let width = self.width as f64;
        let mut b = Band {
            on: vec![false; w * h],
            dist: vec![0.0; w * h],
            cover: vec![0.0; w * h],
            normal: vec![0; w * h],
        };
        for i in 0..w * h {
            let (Some(&(dx, dy)), Some(&ins)) = (near.get(i), inside.get(i)) else {
                continue;
            };
            if ins != inner || (dx, dy) == (0, 0) {
                continue;
            }
            let d = ((dx * dx + dy * dy) as f64).sqrt();
            // Outside bands are anti-aliased at their outer edge (rounded corners).
            let c = if inner {
                if d > width { 0.0 } else { 1.0 }
            } else {
                (width + EDGE_AA - d).clamp(0.0, 1.0)
            };
            if c <= 0.0 {
                continue;
            }
            let v = tie_vector(&near, w, h, i, (dx, dy), d);
            // Outward normal: towards the outside for Inner, away from the object otherwise.
            let n = if inner {
                fw_angle(v.0, v.1)
            } else {
                fw_angle(-v.0, -v.1)
            };
            if let (Some(on), Some(dd), Some(cv), Some(nn)) = (
                b.on.get_mut(i),
                b.dist.get_mut(i),
                b.cover.get_mut(i),
                b.normal.get_mut(i),
            ) {
                *on = true;
                *dd = d;
                *cv = c as f32;
                *nn = n;
            }
        }
        b
    }

    /// Highlight and shadow alpha maps. The tilt field is s * unit normal (a negative edge
    /// shape faces the other way); AngleSoftness smooths it with a Gaussian (sigma ~0.26 x
    /// softness, softness capped at 10), so corners round off and the tilt fades near the band's
    /// edges and where the shape changes sign.
    fn shading(&self, b: &Band, w: usize, h: usize) -> (Vec<f32>, Vec<f32>) {
        let (mut vx, mut vy) = (vec![0f64; w * h], vec![0f64; w * h]);
        for i in 0..w * h {
            if b.on.get(i).copied().unwrap_or(false) {
                let s = self.slope_at(b.dist.get(i).copied().unwrap_or(0.0));
                let r = (b.normal.get(i).copied().unwrap_or(0) as f64).to_radians();
                if let (Some(x), Some(y)) = (vx.get_mut(i), vy.get_mut(i)) {
                    *x = s * r.cos();
                    *y = s * r.sin();
                }
            }
        }
        let soft = self.softness.min(10.0);
        if soft > 0.0 {
            let sigma = 0.26 * soft as f64;
            vx = gaussian_plane(&vx, w, h, sigma);
            vy = gaussian_plane(&vy, w, h, sigma);
        }
        let mut hi = vec![0f32; w * h];
        let mut sh = vec![0f32; w * h];
        for i in 0..w * h {
            if !b.on.get(i).copied().unwrap_or(false) {
                continue;
            }
            let (x, y) = (
                vx.get(i).copied().unwrap_or(0.0),
                vy.get(i).copied().unwrap_or(0.0),
            );
            // Without softness use the integer normal exactly (the vector would round it).
            let n = if soft > 0.0 {
                y.atan2(x).to_degrees()
            } else {
                let flip = self.slope_at(b.dist.get(i).copied().unwrap_or(0.0)) < 0.0;
                b.normal.get(i).copied().unwrap_or(0) as f64 + if flip { 180.0 } else { 0.0 }
            };
            let (a, c) = self.light(n, x.hypot(y));
            if let (Some(hh), Some(ss)) = (hi.get_mut(i), sh.get_mut(i)) {
                *hh = a;
                *ss = c;
            }
        }
        (hi, sh)
    }

    /// One output pixel: band colour, shading, then the button overlay.
    fn composite(&self, p: &BandPixel) -> Rgba {
        let over = |base: Rgba, c: Rgba, alpha: f32| -> Rgba {
            let a = (alpha.clamp(0.0, 255.0) * c.a as f32 / 255.0).round() as u8;
            base.over(Rgba { a: 255, ..c }, a)
        };
        let orig = p.orig;
        let mut out = match self.kind {
            BevelKind::Inner => orig,
            BevelKind::Outer if p.in_band => Rgba {
                a: (p.cover * self.outer_color.a as f32).round() as u8,
                ..self.outer_color
            }
            .over(orig, 255),
            BevelKind::Outer => orig,
            _ if p.inside && !self.show_object => Rgba {
                a: (self.face.a as u32 * orig.a as u32 / 255) as u8,
                ..self.face
            },
            _ => orig,
        };
        if p.in_band {
            let c = if self.kind.emboss() { p.cover } else { 1.0 };
            let (a_hi, a_sh) = (p.hi * c, p.sh * c);
            if self.kind.emboss() {
                // Shading over a transparent band.
                out = out.over(
                    self.shadow,
                    (a_sh.clamp(0.0, 255.0) * self.shadow.a as f32 / 255.0).round() as u8,
                );
                out = out.over(
                    self.hilite,
                    (a_hi.clamp(0.0, 255.0) * self.hilite.a as f32 / 255.0).round() as u8,
                );
            } else {
                let alpha = out.a;
                out = over(over(out, self.shadow, a_sh), self.hilite, a_hi);
                out.a = alpha;
            }
        }
        let overlay = match self.button {
            1 | 3 => Some(self.hit_color),
            2 => Some(self.down_color),
            _ => None,
        };
        if let (Some(c), true) = (overlay, out.a > 0 && (p.in_band || p.inside)) {
            let alpha = out.a;
            out = Rgba { a: 255, ..out }.over(Rgba { a: 255, ..c }, c.a);
            out.a = alpha;
        }
        out
    }
}

struct Band {
    on: Vec<bool>,
    dist: Vec<f64>,
    cover: Vec<f32>,
    normal: Vec<i32>,
}

struct BandPixel {
    orig: Rgba,
    in_band: bool,
    inside: bool,
    cover: f32,
    hi: f32,
    sh: f32,
}

/// Two equally near edges (a mitred corner): Fireworks lights along their sum when they are
/// within 90 degrees of each other (|sum| >= sqrt(2) d). Otherwise the nearest-edge vector.
fn tie_vector(near: &[(i32, i32)], w: usize, h: usize, i: usize, v: (i32, i32), d: f64) -> (i32, i32) {
    let (x, y) = ((i % w) as i32, (i / w) as i32);
    let d2 = v.0 * v.0 + v.1 * v.1;
    for (ox, oy) in [
        (-1, 0),
        (1, 0),
        (0, -1),
        (0, 1),
        (-1, -1),
        (1, -1),
        (-1, 1),
        (1, 1),
    ] {
        let (nx, ny) = (x + ox, y + oy);
        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
            continue;
        }
        let Some(&(cx, cy)) = near.get(ny as usize * w + nx as usize) else {
            continue;
        };
        let c = (cx + ox, cy + oy);
        if c != v && c.0 * c.0 + c.1 * c.1 == d2 {
            let sum = (v.0 + c.0, v.1 + c.1);
            if ((sum.0 * sum.0 + sum.1 * sum.1) as f64).sqrt() + 1e-9 >= std::f64::consts::SQRT_2 * d {
                return sum;
            }
            break;
        }
    }
    v
}

/// Outside bands' anti-aliased edge: coverage = clamp(width + EDGE_AA - d, 0, 1).
const EDGE_AA: f64 = 1.0;

/// Fireworks' integer atan2 in degrees for a y-down vector (`0x5649f0`): 0 = right, 90 = down;
/// a 16-segment linear table with rounding to whole degrees.
pub fn fw_angle(dx: i32, dy: i32) -> i32 {
    const BASE: [u32; 16] = [
        8388608, 68389542, 127926546, 186556858, 243877696, 299540828, 353261819, 404823782, 454076210,
        500929980, 545349835, 587345552, 626962845, 664274723, 699373771, 732365571,
    ];
    const SLOPE: [u32; 16] = [
        14649, 14535, 14314, 13994, 13590, 13115, 12588, 12025, 11439, 10845, 10253, 9672, 9109, 8569, 8055,
        7568,
    ];
    let atan = |num: u32, den: u32| -> i32 {
        if den == 0 {
            return 0;
        }
        let r = ((num & 0xffff) << 16) / (den & 0xffff).max(1);
        let (i, f) = ((r >> 12) as usize, r & 0xfff);
        let (b, s) = (
            BASE.get(i).copied().unwrap_or(0),
            SLOPE.get(i).copied().unwrap_or(0),
        );
        (s.wrapping_mul(f).wrapping_add(b) >> 24) as i32
    };
    if dy == 0 {
        return if dx < 0 { 180 } else { 0 };
    }
    if dx == 0 {
        return if dy < 0 { -90 } else { 90 };
    }
    let (mut ax, mut ay) = (dx.unsigned_abs(), dy.unsigned_abs());
    while (ax | ay) >= 0x7fff {
        ax >>= 1;
        ay >>= 1;
    }
    let mut a = if ax == ay {
        45
    } else if ax < ay {
        90 - atan(ax, ay)
    } else {
        atan(ay, ax)
    };
    if dx < 0 {
        a = 180 - a;
    }
    if dy < 0 {
        a = 360 - a;
    }
    a
}

/// For every pixel, the offset (dx, dy) to the nearest seed pixel (exact Euclidean, Felzenszwalb
/// with argmin tracking). With `edge_is_seed`, the area outside the image counts as seeds.
pub(crate) fn nearest_seed(seeds: &[bool], w: usize, h: usize, edge_is_seed: bool) -> Vec<(i32, i32)> {
    const INF: i64 = 1 << 40;
    // Column pass: nearest seed row in each column.
    let mut col_row = vec![-1i64; w * h];
    for x in 0..w {
        let mut last: i64 = if edge_is_seed { -1 } else { -INF };
        for y in 0..h {
            if seeds.get(y * w + x).copied().unwrap_or(false) {
                last = y as i64;
            }
            if let Some(c) = col_row.get_mut(y * w + x) {
                *c = last;
            }
        }
        let mut next: i64 = if edge_is_seed { h as i64 } else { INF };
        for y in (0..h).rev() {
            if seeds.get(y * w + x).copied().unwrap_or(false) {
                next = y as i64;
            }
            if let Some(c) = col_row.get_mut(y * w + x) {
                let yy = y as i64;
                if (next - yy).abs() < (yy - *c).abs() {
                    *c = next;
                }
            }
        }
    }
    // Row pass: lower envelope of parabolas x' -> (x - x')^2 + vy(x')^2, tracking x'.
    let mut out = vec![(0i32, 0i32); w * h];
    let mut f = vec![0f64; w];
    for y in 0..h {
        let rows = col_row.get(y * w..(y + 1) * w).unwrap_or(&[]);
        for (fx, &r) in f.iter_mut().zip(rows) {
            let dy = (r - y as i64).abs() as f64;
            *fx = dy * dy;
        }
        for (x, sx) in lower_envelope(&f).into_iter().enumerate() {
            let r = rows.get(sx).copied().unwrap_or(-INF);
            if let Some(o) = out.get_mut(y * w + x) {
                *o = (
                    (sx as i64 - x as i64) as i32,
                    (r - y as i64).clamp(-(1 << 30), 1 << 30) as i32,
                );
            }
        }
    }
    // The image edge as seeds (Inner Bevel on an object touching the buffer's edge).
    if edge_is_seed {
        for y in 0..h {
            for x in 0..w {
                if let Some(o) = out.get_mut(y * w + x) {
                    let d2 = |p: (i32, i32)| (p.0 as i64).pow(2) + (p.1 as i64).pow(2);
                    for e in [(-(x as i32) - 1, 0), ((w - x) as i32, 0)] {
                        if d2(e) < d2(*o) {
                            *o = e;
                        }
                    }
                }
            }
        }
    }
    out
}

/// Separable Gaussian blur of a plane (exact kernel, radius 3 sigma; zero outside).
pub(crate) fn gaussian_plane(src: &[f64], w: usize, h: usize, sigma: f64) -> Vec<f64> {
    if w == 0 || h == 0 || src.len() < w * h {
        return src.to_vec();
    }
    let r = (sigma * 3.0).ceil().max(1.0) as i64;
    let kernel: Vec<f64> = (-r..=r)
        .map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f64 = kernel.iter().sum();
    let kernel: Vec<f64> = kernel.iter().map(|k| k / sum).collect();
    let r = r as usize;
    // Same taps in the same order as a direct convolution (in-range taps only), so results
    // don't depend on this layout; rows are processed whole for speed.
    let mut tmp = vec![0f64; w * h];
    for (row_in, row_out) in src.chunks_exact(w).zip(tmp.chunks_exact_mut(w)) {
        for (x, o) in row_out.iter_mut().enumerate() {
            let k0 = r.saturating_sub(x);
            let k1 = (r + w - x).min(2 * r + 1);
            let mut acc = 0.0;
            let taps = kernel.get(k0..k1).unwrap_or(&[]);
            for (kv, v) in taps.iter().zip(row_in.get(x + k0 - r..).unwrap_or(&[])) {
                acc += kv * v;
            }
            *o = acc;
        }
    }
    let mut out = vec![0f64; w * h];
    for (y, row_out) in out.chunks_exact_mut(w).enumerate() {
        let k0 = r.saturating_sub(y);
        let k1 = (r + h - y).min(2 * r + 1);
        for (k, kv) in kernel.iter().enumerate().take(k1).skip(k0) {
            let sy = y + k - r;
            let row_in = tmp.get(sy * w..(sy + 1) * w).unwrap_or(&[]);
            for (o, v) in row_out.iter_mut().zip(row_in) {
                *o += kv * v;
            }
        }
    }
    out
}

/// For each x, the x' minimizing (x - x')^2 + f[x'] (Felzenszwalb & Huttenlocher).
fn lower_envelope(f: &[f64]) -> Vec<usize> {
    let n = f.len();
    let mut v: Vec<usize> = Vec::with_capacity(n);
    let mut z: Vec<f64> = Vec::with_capacity(n + 1);
    let fv = |i: usize| f.get(i).copied().unwrap_or(f64::MAX) + (i * i) as f64;
    for q in 0..n {
        let mut s = f64::NEG_INFINITY;
        while let (Some(&vk), Some(&zk)) = (v.last(), z.last()) {
            s = (fv(q) - fv(vk)) / (2.0 * (q as f64 - vk as f64));
            if s <= zk {
                v.pop();
                z.pop();
                s = f64::NEG_INFINITY;
            } else {
                break;
            }
        }
        v.push(q);
        z.push(s);
    }
    let mut out = Vec::with_capacity(n);
    let mut k = 0usize;
    for x in 0..n {
        while z.get(k + 1).is_some_and(|&b| b < x as f64) {
            k += 1;
        }
        out.push(v.get(k).copied().unwrap_or(x));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gaussian_plane_matches_direct_convolution() {
        let (w, h) = (37usize, 23usize);
        let src: Vec<f64> = (0..w * h).map(|i| ((i * 7919) % 101) as f64 / 7.0).collect();
        for sigma in [0.6, 2.5, 9.0] {
            let r = (sigma * 3.0f64).ceil().max(1.0) as i64;
            let k: Vec<f64> = (-r..=r)
                .map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).exp())
                .collect();
            let sum: f64 = k.iter().sum();
            let k: Vec<f64> = k.iter().map(|v| v / sum).collect();
            let pass = |input: &[f64], horizontal: bool| -> Vec<f64> {
                let mut out = vec![0f64; w * h];
                for y in 0..h as i64 {
                    for x in 0..w as i64 {
                        let mut acc = 0.0;
                        for (o, kv) in (-r..=r).zip(&k) {
                            let (sx, sy) = if horizontal { (x + o, y) } else { (x, y + o) };
                            if sx >= 0 && sy >= 0 && sx < w as i64 && sy < h as i64 {
                                acc += kv * input[sy as usize * w + sx as usize];
                            }
                        }
                        out[y as usize * w + x as usize] = acc;
                    }
                }
                out
            };
            assert_eq!(
                gaussian_plane(&src, w, h, sigma),
                pass(&pass(&src, true), false),
                "sigma {sigma}"
            );
        }
    }

    #[test]
    fn angle_table_matches_atan2() {
        for (dx, dy) in [
            (5, 0),
            (0, 5),
            (-5, 0),
            (0, -5),
            (3, 3),
            (-3, -3),
            (7, 2),
            (-2, 9),
        ] {
            let want = (dy as f64).atan2(dx as f64).to_degrees().rem_euclid(360.0);
            let got = (fw_angle(dx, dy) as f64).rem_euclid(360.0);
            assert!((got - want).abs() <= 0.6, "{dx},{dy}: {got} vs {want}");
        }
    }

    #[test]
    fn nearest_seed_is_exact() {
        let (w, h) = (9, 7);
        let mut seeds = vec![false; w * h];
        seeds[2 * w + 3] = true;
        seeds[5 * w + 8] = true;
        let near = nearest_seed(&seeds, w, h, false);
        for y in 0..h {
            for x in 0..w {
                let (dx, dy) = near[y * w + x];
                let d = dx * dx + dy * dy;
                let best = [(3, 2), (8, 5)]
                    .iter()
                    .map(|&(sx, sy)| (sx - x as i32).pow(2) + (sy - y as i32).pow(2))
                    .min()
                    .unwrap();
                assert_eq!(d, best, "({x},{y})");
                assert!(seeds[((y as i32 + dy) as usize) * w + (x as i32 + dx) as usize]);
            }
        }
    }

    fn square_bevel(kind: BevelKind, light: f32) -> RgbaImage {
        let mut img = RgbaImage::new(80, 80, Rgba::TRANSPARENT);
        for y in 10..70 {
            for x in 10..70 {
                img.set(x, y, Rgba::new(128, 128, 128, 255));
            }
        }
        let b = Bevel {
            kind,
            width: 10.0,
            contrast: 100.0,
            softness: 0.0,
            light_angle: light,
            slope: 0,
            multiplier: 1.0,
            button: 0,
            hilite: Rgba::WHITE,
            shadow: Rgba::BLACK,
            outer_color: Rgba::new(0xdf, 0, 0, 255),
            face: Rgba::TRANSPARENT,
            show_object: false,
            hit_color: Rgba::new(255, 255, 255, 0x3f),
            down_color: Rgba::new(0, 0, 0, 0x3f),
        };
        b.apply(&mut img);
        img
    }

    /// Fireworks MX (oracle_bevel2): flat Inner Bevel, contrast 100, softness 0, #808080.
    #[test]
    fn inner_bevel_matches_fireworks() {
        let img = square_bevel(BevelKind::Inner, 135.0);
        let v = |x, y| img.get(x, y).unwrap().r;
        // Shadow sides are exact; lit sides within Fireworks' ~2 degree normal tilt.
        assert_eq!(v(69, 40), 48);
        assert!(v(40, 10).abs_diff(190) <= 3, "{}", v(40, 10));
        assert_eq!(v(40, 40), 128);
        let img = square_bevel(BevelKind::Inner, 0.0);
        assert_eq!(img.get(69, 40).unwrap().r, 255);
        assert!(img.get(10, 40).unwrap().r <= 3);
    }

    #[test]
    fn outer_bevel_draws_a_band_outside() {
        let img = square_bevel(BevelKind::Outer, 135.0);
        let p = img.get(5, 40).unwrap();
        assert_eq!(p.a, 255);
        assert!(p.r.abs_diff(239) <= 2, "{p:?}");
        assert_eq!(img.get(40, 40).unwrap().r, 128);
        assert!(img.get(74, 40).unwrap().r.abs_diff(84) <= 2);
    }
}
