//! Built-in versions of the plug-in filters bundled with Fireworks MX, the LE ("light") demo
//! editions of Alien Skin Eye Candy
//! 4000 LE (Bevel Boss, Motion Trail, Marble) and Splat LE (Edges).
//!
//! Clean-room: written from the plug-ins' documented parameters (their scripting dictionary:
//! names, keys and option values) and from renders, not from their code. Files keep the
//! original parameters (Photoshop scripting descriptors, `external_filter_key__XXXX`), so
//! documents still open with the real plug-ins in Fireworks.

use std::collections::HashMap;

use crate::image::{Rgba, RgbaImage};

/// Bevel Boss's EffectMoaID (and the others).
pub const BEVEL_BOSS: &str = "{d810e821-f86b-11d0-656341346564694e}";
pub const MOTION_TRAIL: &str = "{d810e821-f86b-11d0-6563413465646d54}";
pub const MARBLE: &str = "{d810e821-f86b-11d0-6563413465646d41}";
pub const EDGES: &str = "{d810e821-f86b-11d0-7371473173676544}";

/// Plug-in parameters as stored: `external_filter_key__Wdth` or `..__0x466c2020` ('Fl  ').
pub struct Params<'a>(pub &'a HashMap<String, String>);

impl Params<'_> {
    fn raw(&self, code: &str) -> Option<&str> {
        let plain = format!("external_filter_key__{code}");
        let hex = format!(
            "external_filter_key__0x{}",
            code.bytes().map(|b| format!("{b:02x}")).collect::<String>()
        );
        self.0
            .get(&plain)
            .or_else(|| self.0.get(&hex))
            .map(String::as_str)
    }

    pub fn num(&self, code: &str, default: f32) -> f32 {
        self.raw(code)
            .and_then(|v| v.trim().parse::<f32>().ok())
            .filter(|v| v.is_finite())
            .unwrap_or(default)
    }

    pub fn flag(&self, code: &str, default: bool) -> bool {
        self.raw(code).map_or(default, |v| v.trim() == "true")
    }

    /// A TEXT value: the first hex word decoded ("79.91 pixels", "RGB: ...", "Bevel: ...").
    pub fn text(&self, code: &str) -> Option<String> {
        let hex = self.raw(code)?.split_whitespace().next()?;
        let bytes: Option<Vec<u8>> = (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok())
            .collect();
        String::from_utf8(bytes?).ok()
    }

    /// An enumeration value (the second hex word, e.g. 'beT1').
    pub fn enumeration(&self, code: &str) -> Option<String> {
        let hex = self.raw(code)?.split_whitespace().nth(1)?.to_owned();
        let bytes: Option<Vec<u8>> = (0..hex.len() / 2)
            .map(|i| u8::from_str_radix(hex.get(2 * i..2 * i + 2)?, 16).ok())
            .collect();
        String::from_utf8(bytes?).ok()
    }

    /// "12.5 pixels" -> 12.5.
    pub fn pixels(&self, code: &str, default: f32) -> f32 {
        self.text(code)
            .and_then(|t| t.split_whitespace().next()?.parse::<f32>().ok())
            .unwrap_or(default)
    }

    /// "RGB: 100.0% 0.0% 0.0% (255 0 0)" -> the colour.
    pub fn color(&self, code: &str, default: Rgba) -> Rgba {
        let Some(t) = self.text(code) else { return default };
        let inner = t.split('(').nth(1).and_then(|s| s.split(')').next());
        let v: Vec<u8> = inner
            .map(|s| s.split_whitespace().filter_map(|n| n.parse().ok()).collect())
            .unwrap_or_default();
        match v.as_slice() {
            [r, g, b] => Rgba::new(*r, *g, *b, 255),
            _ => default,
        }
    }
}

/// Eye Candy 4000 Bevel Boss.
#[derive(Clone, Debug, PartialEq)]
pub struct BevelBoss {
    pub width: f32,
    /// Bevel Height Scale, 0..=100.
    pub height: f32,
    /// 0..=100.
    pub smoothness: f32,
    /// Bevel Placement: outside the marquee (the object) instead of inside.
    pub outside: bool,
    pub shade_interior: bool,
    /// Light direction (degrees, counter-clockwise from the right) and inclination (degrees
    /// above the surface).
    pub direction: f32,
    pub inclination: f32,
    /// Highlight Brightness and Highlight Size, 0..=100.
    pub brightness: f32,
    pub highlight_size: f32,
    pub highlight: Rgba,
    pub shadow: Rgba,
    /// Bevel Profile: (position from the edge 0..1, height 0..1) points.
    pub profile: Vec<(f32, f32)>,
    /// Darken Deep Areas, 0..=100.
    pub darken: f32,
}

impl BevelBoss {
    pub fn parse(p: &Params) -> BevelBoss {
        let profile = p
            .text("BEvl")
            .and_then(|t| {
                // "Bevel: 3/0.000 0.261 0/0.401 0.225 0/1.000 0.000 0"
                let body = t.strip_prefix("Bevel:")?.trim().to_owned();
                let mut parts = body.split('/');
                parts.next()?;
                let pts: Vec<(f32, f32)> = parts
                    .filter_map(|s| {
                        let mut n = s.split_whitespace().map(|v| v.parse::<f32>());
                        Some((n.next()?.ok()?, n.next()?.ok()?))
                    })
                    .collect();
                (pts.len() >= 2).then_some(pts)
            })
            .unwrap_or_else(|| vec![(0.0, 0.0), (1.0, 1.0)]);
        BevelBoss {
            width: p.pixels("Wdth", 10.0),
            height: p.num("Cntr", 50.0),
            smoothness: p.num("Smth", 50.0),
            outside: p.enumeration("bvlS").is_some_and(|e| e == "beT1"),
            shade_interior: p.flag("Fl  ", true),
            direction: p.num("Drct", 135.0),
            inclination: p.num("INcl", 45.0),
            brightness: p.num("Brgh", 100.0),
            highlight_size: p.num("Shrp", 25.0),
            highlight: p.color("Clr ", Rgba::WHITE),
            shadow: p.color("sdwC", Rgba::BLACK),
            profile,
            darken: p.num("Drkn", 0.0),
        }
    }

    pub fn margin(&self) -> f32 {
        if self.outside {
            self.width.max(0.0) + 2.0
        } else {
            0.0
        }
    }

    /// The profile height (0..1) at `t` (1 = the object's edge, 0 = the inner end of the
    /// bevel, as measured). Up to three points: the polynomial through all of them (with three,
    /// the parabola): eye2.png's dome has a slope falling linearly from the edge, the parabola's
    /// (eye2 87 -> 95%, eye 89 -> 93%, ball 99.5 -> 100%, bevel_probe 155 -> 117). More points
    /// (unmeasured): a monotone cubic through them.
    fn profile_at(&self, t: f32) -> f32 {
        let p = &self.profile;
        let t = t.clamp(0.0, 1.0);
        if p.len() <= 3 {
            // Lagrange form of the polynomial through the points.
            return p
                .iter()
                .enumerate()
                .map(|(i, pi)| {
                    let l: f32 = p
                        .iter()
                        .enumerate()
                        .filter(|&(j, _)| j != i)
                        .map(|(_, pj)| (t - pj.0) / (pi.0 - pj.0))
                        .product();
                    l * pi.1
                })
                .sum();
        }
        let k = p.iter().position(|q| q.0 >= t).unwrap_or(p.len() - 1).max(1);
        let (a, b) = (p[k - 1], p[k]);
        let span = (b.0 - a.0).max(1e-6);
        let u = ((t - a.0) / span).clamp(0.0, 1.0);
        let secant = |j: usize| (p[j + 1].1 - p[j].1) / (p[j + 1].0 - p[j].0).max(1e-6);
        let slope = |i: usize| -> f32 {
            if i == 0 {
                secant(0)
            } else if i == p.len() - 1 {
                secant(i - 1)
            } else {
                let (d0, d1) = (secant(i - 1), secant(i));
                if d0 * d1 <= 0.0 { 0.0 } else { 2.0 / (1.0 / d0 + 1.0 / d1) }
            }
        };
        let (m0, m1) = (slope(k - 1) * span, slope(k) * span);
        let (u2, u3) = (u * u, u * u * u);
        (2.0 * u3 - 3.0 * u2 + 1.0) * a.1 + (u3 - 2.0 * u2 + u) * m0 + (-2.0 * u3 + 3.0 * u2) * b.1 + (u3 - u2) * m1
    }

    /// Fitted to Fireworks MX renders of the bundled plug-in (docs/behavior/plugins.md,
    /// crates/io/examples/bevel_probe.rs rendered on Windows):
    /// - the object is its pixels at least half opaque; height = profile(x) x width x Height
    ///   Scale/100 x 1.5, with x = 1 at the object's edge and 0 at `width` inside (flat beyond),
    ///   smoothed by a Gaussian of 5.25 x Smoothness/100 px (not scaled by the width);
    /// - inside, every visible pixel is shaded (fainter ones beyond the half-opaque edge too) and
    ///   keeps its alpha; a semi-transparent pixel's colour is first scaled by a / (a + 0.2 (1 - a));
    /// - outside, the rim is the Shadow Color at alpha 1 - lit and the Highlight Color at alpha
    ///   k painted over the (usually transparent) pixels;
    /// - colour = mix(Shadow Color, object, n.L) everywhere when Shade Interior (flat areas get
    ///   sin(inclination)), relative to a flat surface otherwise;
    /// - highlight = Highlight Color over it by (1 - angle(n, H) / (Highlight Size% of 85.5
    ///   degrees))^2.2 x Brightness, H halfway between the light and the viewer.
    pub fn apply(&self, img: &mut RgbaImage) {
        let (w, h) = (img.width() as usize, img.height() as usize);
        if w == 0 || h == 0 || self.width <= 0.0 {
            return;
        }
        let inside: Vec<bool> = img.pixels().iter().map(|p| p.a >= 128).collect();
        if !inside.iter().any(|&b| b) {
            return;
        }
        let bw = self.width as f64;
        let scale = bw * (self.height as f64 / 100.0) * 1.5;
        let near = crate::bevel::nearest_seed(
            &if self.outside {
                inside.clone()
            } else {
                inside.iter().map(|&b| !b).collect::<Vec<_>>()
            },
            w,
            h,
            !self.outside,
        );
        // Height field and the region the bevel covers.
        let mut height = vec![0f64; w * h];
        let mut region = vec![false; w * h];
        for i in 0..w * h {
            let ins = inside.get(i).copied().unwrap_or(false);
            let (dx, dy) = near.get(i).copied().unwrap_or((0, 0));
            let d = ((dx * dx + dy * dy) as f64).sqrt();
            // x: 1 at the object's edge, 0 at `width` inside (Outside: 0 at the edge, 1 out).
            let (on, x) = if self.outside {
                (ins || d <= bw, if ins { 0.0 } else { d / bw })
            } else {
                (ins, 1.0 - (d / bw).min(1.0))
            };
            if let (Some(hh), Some(r)) = (height.get_mut(i), region.get_mut(i)) {
                *r = on;
                *hh = self.profile_at(x as f32) as f64 * scale;
            }
        }
        if !self.outside {
            // Inside, the fainter pixels beyond the half-opaque edge are shaded too (at the
            // edge's height before smoothing).
            for (r, p) in region.iter_mut().zip(img.pixels()) {
                *r = p.a > 0;
            }
        }
        let sigma = 5.25 * self.smoothness as f64 / 100.0;
        if sigma > 0.3 {
            // Smooth within the region only (normalised), so outside pixels don't pull it down.
            let mask: Vec<f64> = region.iter().map(|&r| if r { 1.0 } else { 0.0 }).collect();
            let weighted: Vec<f64> = height.iter().zip(&mask).map(|(h, m)| h * m).collect();
            let (num, den) = (
                crate::bevel::gaussian_plane(&weighted, w, h, sigma),
                crate::bevel::gaussian_plane(&mask, w, h, sigma),
            );
            for (i, hh) in height.iter_mut().enumerate() {
                let d = den.get(i).copied().unwrap_or(0.0);
                if d > 1e-6 {
                    *hh = num.get(i).copied().unwrap_or(0.0) / d;
                }
            }
        }
        let (az, el) = (
            (self.direction as f64).to_radians(),
            (self.inclination as f64).to_radians(),
        );
        let light = (el.cos() * az.cos(), -el.cos() * az.sin(), el.sin());
        let half = {
            let (x, y, z) = (light.0, light.1, light.2 + 1.0);
            let l = (x * x + y * y + z * z).sqrt().max(1e-9);
            (x / l, y / l, z / l)
        };
        // Highlight: (1 - angle(n, H) / cone)^2.2 with cone = Highlight Size% of 85.5 degrees
        // (fitted on bevel_probe, soft_bevel_probe and spectrum_wave.png: the linear cone of
        // Size% of 45 degrees ended too soon, losing the dim highlight on spectrum_wave's left
        // edge, 13 degrees off the half vector).
        let cone = ((self.highlight_size as f64 / 100.0) * 85.5).to_radians();
        let flat = light.2.max(1e-3);
        let src = img.clone();
        let at = |x: i64, y: i64| -> f64 {
            let (x, y) = (x.clamp(0, w as i64 - 1), y.clamp(0, h as i64 - 1));
            height.get(y as usize * w + x as usize).copied().unwrap_or(0.0)
        };
        for y in 0..h as i64 {
            for x in 0..w as i64 {
                let i = y as usize * w + x as usize;
                if !region.get(i).copied().unwrap_or(false) {
                    continue;
                }
                let (gx, gy) = (
                    (at(x + 1, y) - at(x - 1, y)) / 2.0,
                    (at(x, y + 1) - at(x, y - 1)) / 2.0,
                );
                let n = {
                    let l = (gx * gx + gy * gy + 1.0).sqrt();
                    (-gx / l, -gy / l, 1.0 / l)
                };
                let mut diffuse = (n.0 * light.0 + n.1 * light.1 + n.2 * light.2).max(0.0);
                if !self.shade_interior {
                    diffuse = (diffuse / flat).min(1.0);
                }
                let ang = (n.0 * half.0 + n.1 * half.1 + n.2 * half.2)
                    .clamp(-1.0, 1.0)
                    .acos();
                let spec = if cone > 0.0 {
                    (1.0 - ang / cone).max(0.0).powf(2.2) * (self.brightness as f64 / 100.0)
                } else {
                    0.0
                };
                let p = src.pixels().get(i).copied().unwrap_or_default();
                let deep = (self.darken as f64 / 100.0)
                    * (1.0 - height.get(i).copied().unwrap_or(0.0) / scale.max(1e-6)).clamp(0.0, 1.0)
                    * 0.5;
                let lit = (diffuse * (1.0 - deep)).clamp(0.0, 1.0);
                let k = spec.min(1.0);
                let q = if self.outside && !inside.get(i).copied().unwrap_or(false) {
                    // Measured (bevel_probe on Windows): outside the object the rim is the
                    // Shadow Color at alpha 1 - lit with the Highlight Color at alpha k over it,
                    // painted over the pixel (transparent ones stay see-through where neither
                    // shows; their stored colour doesn't matter).
                    over_f(over_f(p, self.shadow, 1.0 - lit), self.highlight, k)
                } else {
                    // Inside, a semi-transparent pixel's colour is first scaled by
                    // a / (a + 0.2 (1 - a)) (measured on soft_bevel_probe, Windows: 0.42 at alpha
                    // 33, 0.80 at 112, 0.90 at 160, 0.97 at 224). In f64, rounded once.
                    let dim = if p.a < 255 && !self.outside {
                        let a = p.a as f64 / 255.0;
                        a / (a + 0.2 * (1.0 - a))
                    } else {
                        1.0
                    };
                    let (s, hl) = (self.shadow, self.highlight);
                    let ch = |sc: u8, pc: u8, hc: u8| {
                        let base = sc as f64 + (pc as f64 * dim - sc as f64) * lit;
                        (base + (hc as f64 - base) * k).round().clamp(0.0, 255.0) as u8
                    };
                    Rgba::new(ch(s.r, p.r, hl.r), ch(s.g, p.g, hl.g), ch(s.b, p.b, hl.b), p.a)
                };
                if let Some(o) = img.pixels_mut().get_mut(i) {
                    *o = q;
                }
            }
        }
    }
}

/// `colour` (opaque) at `alpha` 0..1 over `base`, straight alpha, rounded once.
fn over_f(base: Rgba, colour: Rgba, alpha: f64) -> Rgba {
    let sa = alpha.clamp(0.0, 1.0);
    if sa <= 0.0 {
        return base;
    }
    let ba = base.a as f64 / 255.0;
    let out_a = sa + ba * (1.0 - sa);
    let ch = |s: u8, b: u8| ((s as f64 * sa + b as f64 * ba * (1.0 - sa)) / out_a).round().clamp(0.0, 255.0) as u8;
    Rgba::new(
        ch(colour.r, base.r),
        ch(colour.g, base.g),
        ch(colour.b, base.b),
        (out_a * 255.0).round() as u8,
    )
}

/// Motion Trail's narrowing: at taper 100% a copy k pixels along is squeezed by k/D (fitted).
const TAPER_DISTANCE: f64 = 57.3;

/// Eye Candy 4000 Motion Trail.
#[derive(Clone, Debug, PartialEq)]
pub struct MotionTrail {
    pub direction: f32,
    pub length: f32,
    /// 0..=100: how much the trail fades towards its end.
    pub taper: f32,
    /// 0..=100.
    pub opacity: f32,
    /// Smear Color from Edges.
    pub smear: bool,
    /// Draw Only Outside Selection.
    pub outside_only: bool,
}

impl MotionTrail {
    pub fn parse(p: &Params) -> MotionTrail {
        MotionTrail {
            direction: p.num("Drct", 0.0),
            length: p.pixels("Lngt", 40.0),
            taper: p.num("TApe", 40.0),
            opacity: p.num("Opct", 95.0),
            smear: p.flag("Edg ", true),
            outside_only: p.flag("Fl  ", false),
        }
    }

    pub fn margin(&self) -> f32 {
        self.length.max(0.0) + 2.0
    }

    /// The trail: copies of the object's shape (its pixels at least half opaque) shifted
    /// k = 1..=length pixels towards `direction`, each squeezed across the direction by
    /// 1 − taper·k/D about the object's centre line and faded to opacity·(1 − k/length). Each
    /// trail pixel takes the strongest copy, coloured by the object's pixel it maps to (its edge,
    /// for the nearest copy) smeared slightly along the edge; the trail is then softened by a
    /// Gaussian of sigma 1 and drawn behind the object. Fitted to Fireworks MX on Windows
    /// (crates/io/examples/trail_probe.rs, the user's render) and SOLAR.png: the fade is linear
    /// over exactly `length` whatever the taper; the narrowing depends on the taper only, not on
    /// length or object size (D 55–60 px on a 60 px disc and on 119 px and 231 px planets); a
    /// soft-edged object trails from its half-opaque outline at full strength. Not matched yet:
    /// "Smear Color from Edges" off (Fireworks MX then draws the trail over the object, colours
    /// dragged from its far side).
    pub fn apply(&self, img: &mut RgbaImage) {
        self.apply_with(img, crate::ColorMath::Classic);
    }

    /// [`apply`](Self::apply) with a choice of colour math: Precise keeps the trail in f64
    /// through the softening and the compositing and rounds once (Classic stores it in bytes and
    /// softens with Fireworks MX's blur, which rounds down after each pass).
    pub fn apply_with(&self, img: &mut RgbaImage, math: crate::ColorMath) {
        let (w, h) = (img.width() as i64, img.height() as i64);
        let steps = self.length.round().clamp(0.0, 2000.0) as i64;
        if steps < 1 {
            return;
        }
        let a = (self.direction as f64).to_radians();
        // Along the trail (y down) and across it.
        let (ux, uy) = (a.cos(), -a.sin());
        let (vx, vy) = (-uy, ux);
        let src = img.clone();
        let opacity = (self.opacity as f64 / 100.0).clamp(0.0, 1.0);
        let taper = (self.taper as f64 / 100.0).clamp(0.0, 1.0);
        // The object's centre line across the direction (pixels at least half opaque).
        let (mut lo, mut hi) = (f64::MAX, f64::MIN);
        for y in 0..h {
            for x in 0..w {
                if src.get(x, y).is_some_and(|p| p.a >= 128) {
                    let pe = (x as f64 + 0.5) * vx + (y as f64 + 0.5) * vy;
                    lo = lo.min(pe - 0.5);
                    hi = hi.max(pe + 0.5);
                }
            }
        }
        if lo > hi {
            return;
        }
        let mid = (lo + hi) / 2.0;
        let len = steps as f64;
        let sample = |x: f64, y: f64| crate::transform::sample(&src, x - 0.5, y - 0.5);
        // Drawn on its own first (under the object's opaque pixels too, so the softening sees
        // the whole trail): alpha 0..=255 and colour, unrounded.
        let (wu, hu) = (w as usize, h as usize);
        let mut alpha = vec![0.0f64; wu * hu];
        let mut colour = vec![[0.0f64; 3]; wu * hu];
        for y in 0..h {
            for x in 0..w {
                if self.outside_only && src.get(x, y).is_some_and(|p| p.a > 0) {
                    continue;
                }
                let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                let across = px * vx + py * vy - mid;
                // The nearest copy covering this pixel is the strongest.
                let mut hit = None;
                for k in 1..=steps {
                    let squeeze = 1.0 - taper * k as f64 / TAPER_DISTANCE;
                    if squeeze <= 0.0 {
                        break;
                    }
                    // The copy's pixel here, in the object: back k along, unsqueezed across.
                    let back = across / squeeze - across;
                    let (sx, sy) = (px - ux * k as f64 + vx * back, py - uy * k as f64 + vy * back);
                    if sample(sx, sy).a >= 128 {
                        hit = Some((k, sx, sy));
                        break;
                    }
                }
                let Some((k, sx, sy)) = hit else { continue };
                let i = (y * w + x) as usize;
                if let (Some(a), Some(c)) = (alpha.get_mut(i), colour.get_mut(i)) {
                    *a = opacity * (1.0 - k as f64 / len) * 255.0;
                    *c = smeared_colour(&sample, sx, sy, vx, vy);
                }
            }
        }
        if math == crate::ColorMath::Precise {
            blur_plane_f64(&mut alpha, &mut colour, wu, hu, 1.0);
        } else {
            // In bytes, softened with Fireworks MX's blur.
            let mut trail = RgbaImage::new(w as u32, h as u32, Rgba::TRANSPARENT);
            for (p, (a, c)) in trail.pixels_mut().iter_mut().zip(alpha.iter().zip(&colour)) {
                let b = |v: f64| v.round().clamp(0.0, 255.0) as u8;
                if b(*a) > 0 {
                    *p = Rgba::new(b(c[0]), b(c[1]), b(c[2]), b(*a));
                }
            }
            crate::effects::gaussian_blur(&mut trail, 1.0);
            for (p, (a, c)) in trail.pixels().iter().zip(alpha.iter_mut().zip(colour.iter_mut())) {
                *a = p.a as f64;
                *c = [p.r, p.g, p.b].map(f64::from);
            }
        }
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) as usize;
                let own = src.get(x, y).unwrap_or_default();
                let ta = alpha.get(i).copied().unwrap_or(0.0);
                if own.a == 255 || ta <= 0.0 || (self.outside_only && own.a > 0) {
                    continue;
                }
                let tc = colour.get(i).copied().unwrap_or_default();
                let Some(p) = img.pixels_mut().get_mut(i) else { continue };
                // Behind the object: the object's own (partial) pixel stays on top.
                if math == crate::ColorMath::Precise {
                    let (oa, ta) = (own.a as f64 / 255.0, ta / 255.0);
                    let out_a = oa + ta * (1.0 - oa);
                    let oc = [own.r, own.g, own.b].map(f64::from);
                    let ch = |c: usize| {
                        ((oc[c] * oa + tc[c] * ta * (1.0 - oa)) / out_a).round().clamp(0.0, 255.0) as u8
                    };
                    *p = Rgba::new(ch(0), ch(1), ch(2), (out_a * 255.0).round() as u8);
                } else {
                    let b = |v: f64| v.round().clamp(0.0, 255.0) as u8;
                    let t = Rgba::new(b(tc[0]), b(tc[1]), b(tc[2]), b(ta));
                    *p = t.over(own, 255);
                }
            }
        }
    }
}

/// The Gaussian blur of `effects::gaussian_blur` (same kernel, window cut at the edges) in f64,
/// without its rounding down after each pass, and with colour weighted by alpha (premultiplied):
/// Fireworks MX averages the colours of every pixel with any alpha alike, so a nearly
/// transparent pixel can swing an edge colour by tens of levels. `alpha` is 0..=255.
fn blur_plane_f64(alpha: &mut [f64], colour: &mut [[f64; 3]], w: usize, h: usize, sigma: f64) {
    let n = (sigma * 3.0).ceil() as usize;
    let kernel: Vec<f64> = (0..=2 * n)
        .map(|i| {
            let x = i as f64 - n as f64;
            (-(x * x) / (2.0 * sigma * sigma)).exp()
        })
        .collect();
    // One pass along `len` samples at `start + j * stride`.
    let pass = |alpha: &mut [f64], colour: &mut [[f64; 3]], start: usize, stride: usize, len: usize| {
        let a: Vec<f64> = (0..len).map(|j| alpha.get(start + j * stride).copied().unwrap_or(0.0)).collect();
        let c: Vec<[f64; 3]> = (0..len)
            .map(|j| colour.get(start + j * stride).copied().unwrap_or_default())
            .collect();
        for t in 0..len {
            let (lo, hi) = (t.saturating_sub(n), (len - 1).min(t + n));
            let (mut sw, mut sa, mut swc) = (0.0, 0.0, 0.0);
            let mut sc = [0.0f64; 3];
            for j in lo..=hi {
                let k = kernel.get(j + n - t).copied().unwrap_or(0.0);
                let (aj, cj) = (a.get(j).copied().unwrap_or(0.0), c.get(j).copied().unwrap_or_default());
                for (acc, v) in sc.iter_mut().zip(cj) {
                    *acc += v * k * aj;
                }
                swc += k * aj;
                sa += aj * k;
                sw += k;
            }
            let i = start + t * stride;
            if let (Some(ao), Some(co)) = (alpha.get_mut(i), colour.get_mut(i)) {
                *ao = if sw > 0.0 { sa / sw } else { 0.0 };
                *co = if swc > 0.0 { sc.map(|v| v / swc) } else { [0.0; 3] };
            }
        }
    };
    if w == 0 || h == 0 {
        return;
    }
    for y in 0..h {
        pass(alpha, colour, y * w, 1, w);
    }
    for x in 0..w {
        pass(alpha, colour, x, w, h);
    }
}

/// The object's colour at (x, y), averaged across the trail direction (v) with a Gaussian of
/// sigma 2 px, alpha-weighted (fitted on SOLAR.png; the probe is indifferent).
fn smeared_colour(sample: &impl Fn(f64, f64) -> Rgba, x: f64, y: f64, vx: f64, vy: f64) -> [f64; 3] {
    const SIGMA: f64 = 2.0;
    let (mut acc, mut total) = ([0.0f64; 3], 0.0f64);
    for j in -6i32..=6 {
        let g = (-(j * j) as f64 / (2.0 * SIGMA * SIGMA)).exp();
        let q = sample(x + vx * j as f64, y + vy * j as f64);
        let wq = g * q.a as f64;
        acc[0] += wq * q.r as f64;
        acc[1] += wq * q.g as f64;
        acc[2] += wq * q.b as f64;
        total += wq;
    }
    if total <= 0.0 {
        let p = sample(x, y);
        return [p.r, p.g, p.b].map(f64::from);
    }
    acc.map(|v| v / total)
}

/// Splat LE Edges.
#[derive(Clone, Debug, PartialEq)]
pub struct Edges {
    pub width: f32,
    pub margin: f32,
    pub feature: f32,
    pub direction: f32,
    /// 0 Lumpy, 1 Rough, 3 Halftone Dots, 4 Pixels, 5 Halftone Lines, 6 Torn Paper.
    pub mode: u8,
    pub color: Rgba,
    pub transparent: bool,
    pub seed: u32,
}

impl Edges {
    pub fn parse(p: &Params) -> Edges {
        let mode = p
            .enumeration("Edg ")
            .and_then(|e| e.strip_prefix("edM").and_then(|n| n.parse().ok()))
            .unwrap_or(3);
        Edges {
            width: p.pixels("Wdth", 12.0),
            margin: p.pixels("BrdT", 0.0),
            feature: p.num("Scl ", 1.0),
            direction: p.num("Drct", 45.0),
            mode,
            color: p.color("Clr ", Rgba::WHITE),
            transparent: p.flag("Fl  ", true),
            seed: p.num("RndS", 1.0) as u32,
        }
    }

    /// Edge band of `width` inside the object (after `margin`): the pattern decides, per pixel,
    /// whether the object shows or is replaced by the colour (or transparency).
    pub fn apply(&self, img: &mut RgbaImage) {
        let (w, h) = (img.width(), img.height());
        let outside: Vec<bool> = img.pixels().iter().map(|p| p.a < 128).collect();
        let dist = crate::effects::distance_to_pub(&outside, w, h);
        let width = self.width.max(0.5);
        let cell = (4.0 * self.feature.max(0.2)).max(2.0);
        let (s, c) = self.direction.to_radians().sin_cos();
        for (i, p) in img.pixels_mut().iter_mut().enumerate() {
            if p.a == 0 {
                continue;
            }
            let d = dist.get(i).copied().unwrap_or(f32::MAX) - self.margin;
            let (x, y) = ((i as u32 % w) as f32, (i as u32 / w) as f32);
            let cover = if d <= 0.0 {
                0.0
            } else if d >= width {
                1.0
            } else {
                let t = d / width; // 0 at the edge, 1 inside
                let (u, v) = (x * c + y * s, -x * s + y * c);
                let hash = |a: i64, b: i64| -> f32 {
                    let mut z = (a as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                        ^ (b as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
                        ^ self.seed as u64;
                    z ^= z >> 29;
                    z = z.wrapping_mul(0xBF58_476D_1CE4_E5B9);
                    z ^= z >> 32;
                    (z & 0xFFFF) as f32 / 65535.0
                };
                match self.mode {
                    // Halftone dots: dots on a rotated grid growing towards the inside.
                    3 => {
                        let (fu, fv) = ((u / cell).rem_euclid(1.0) - 0.5, (v / cell).rem_euclid(1.0) - 0.5);
                        let r = (fu * fu + fv * fv).sqrt();
                        if r < t * 0.72 { 1.0 } else { 0.0 }
                    }
                    // Halftone lines: lines thickening towards the inside.
                    5 => {
                        if ((v / cell).rem_euclid(1.0) - 0.5).abs() < t * 0.5 {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    // Pixels: square cells kept more often towards the inside.
                    4 => {
                        if hash((u / cell).floor() as i64, (v / cell).floor() as i64) < t {
                            1.0
                        } else {
                            0.0
                        }
                    }
                    // Lumpy / Rough / Torn Paper: a noisy threshold.
                    _ => {
                        let fine = hash(x as i64, y as i64);
                        let coarse = hash((x / cell).floor() as i64, (y / cell).floor() as i64);
                        let noise = match self.mode {
                            0 => coarse,
                            1 => 0.5 * coarse + 0.5 * fine,
                            _ => 0.3 * coarse + 0.7 * fine,
                        };
                        if t + (noise - 0.5) * 0.8 > 0.5 { 1.0 } else { 0.0 }
                    }
                }
            };
            if cover < 1.0 {
                *p = if self.transparent {
                    Rgba::TRANSPARENT
                } else {
                    Rgba { a: p.a, ..self.color }
                };
            }
        }
    }
}

/// Eye Candy 4000 Marble: veins through bedrock, filling the object.
#[derive(Clone, Debug, PartialEq)]
pub struct Marble {
    pub bedrock: Rgba,
    pub vein: Rgba,
    pub vein_size: f32,
    /// 0..=100 each.
    pub coverage: f32,
    pub thickness: f32,
    pub roughness: f32,
    pub seed: u32,
}

impl Marble {
    pub fn parse(p: &Params) -> Marble {
        Marble {
            bedrock: p.color("BEco", Rgba::new(200, 200, 200, 255)),
            vein: p.color("VEcl", Rgba::new(60, 60, 60, 255)),
            vein_size: p.pixels("FEsi", 50.0),
            coverage: p.num("VEcv", 50.0),
            thickness: p.num("VEth", 30.0),
            roughness: p.num("VEro", 50.0),
            seed: p.num("RndS", 1.0) as u32,
        }
    }

    pub fn apply(&self, img: &mut RgbaImage) {
        let w = img.width();
        let scale = self.vein_size.max(2.0);
        let noise = |x: f32, y: f32| value_noise(x, y, self.seed);
        for (i, p) in img.pixels_mut().iter_mut().enumerate() {
            if p.a == 0 {
                continue;
            }
            let (x, y) = ((i as u32 % w) as f32 / scale, (i as u32 / w) as f32 / scale);
            // Turbulence: summed octaves of noise; roughness keeps more fine octaves.
            let mut turb = 0.0;
            let (mut amp, mut freq) = (1.0, 1.0);
            let octaves = 2 + (self.roughness / 25.0) as i32;
            for _ in 0..octaves {
                turb += amp * noise(x * freq, y * freq);
                amp *= 0.5;
                freq *= 2.0;
            }
            let v = ((x + y) * 2.0 + turb * (2.0 + self.coverage / 20.0)).sin().abs();
            let line = 1.0 - (v / (0.05 + self.thickness / 200.0)).min(1.0);
            let k = (line * 255.0) as u8;
            *p = Rgba {
                a: p.a,
                ..Rgba {
                    a: 255,
                    ..self.bedrock
                }
                .over(Rgba { a: 255, ..self.vein }, k)
            };
        }
    }
}

/// Smooth value noise in -1..1.
fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let hash = |a: i32, b: i32| -> f32 {
        let mut z = (a as u32).wrapping_mul(0x27d4_eb2d)
            ^ (b as u32).wrapping_mul(0x1656_67b1)
            ^ seed.wrapping_mul(0x9e37_79b9);
        z ^= z >> 15;
        z = z.wrapping_mul(0x85eb_ca6b);
        z ^= z >> 13;
        (z & 0xFFFF) as f32 / 32767.5 - 1.0
    };
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (xi, yi) = (x0 as i32, y0 as i32);
    let a = hash(xi, yi) + (hash(xi + 1, yi) - hash(xi, yi)) * sx;
    let b = hash(xi, yi + 1) + (hash(xi + 1, yi + 1) - hash(xi, yi + 1)) * sx;
    a + (b - a) * sy
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(kv: &[(&str, &str)]) -> HashMap<String, String> {
        kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn decodes_descriptor_values() {
        // From ball.png's Bevel Boss.
        let kv = params(&[
            ("external_filter_key__Wdth", "37392e393120706978656c73 0"),
            ("external_filter_key__bvlS", "42657469 62655431"),
            (
                "external_filter_key__0x436c7220",
                "5247423a203130302e302520302e302520302e3025202832353520302030292030 0",
            ),
            (
                "external_filter_key__BEvl",
                "426576656c3a20332f302e30303020302e32363120302f302e3430312030",
            ),
            ("external_filter_key__INcl", "45"),
            ("external_filter_key__0x466c2020", "false"),
        ]);
        let b = BevelBoss::parse(&Params(&kv));
        assert!((b.width - 79.91).abs() < 1e-3);
        assert!(b.outside);
        assert_eq!(b.highlight, Rgba::new(255, 0, 0, 255));
        assert_eq!(b.profile.first().copied(), Some((0.0, 0.261)));
        assert_eq!(b.inclination, 45.0);
        assert!(!b.shade_interior);
    }

    #[test]
    fn rotated_halftone_covers_every_edge() {
        // The grid is rotated about the image's top-left: where its coordinates go negative
        // (left side, at 45 degrees) dots must still appear (fract() of negatives lost them).
        let mut img = RgbaImage::new(140, 140, Rgba::TRANSPARENT);
        img.draw(&RgbaImage::new(120, 120, Rgba::new(255, 0, 0, 255)), 10, 10, 255);
        let e = Edges {
            width: 20.0,
            margin: 0.0,
            feature: 1.5,
            direction: 45.0,
            mode: 3,
            color: Rgba::WHITE,
            transparent: true,
            seed: 1,
        };
        e.apply(&mut img);
        let kept = |x0: u32, x1: u32| {
            (10..110)
                .flat_map(|y| (x0..x1).map(move |x| (x, y)))
                .filter(|&(x, y)| img.get(x as i64, y as i64).is_some_and(|p| p.a > 0))
                .count()
        };
        // The outer half of the band: left side (grid coordinates positive) and right side
        // (negative at 45 degrees).
        let (left, right) = (kept(10, 20), kept(120, 130));
        assert!(left > 0 && right > 0, "left {left} right {right}");
        assert!(left * 2 > right && right * 2 > left, "left {left} right {right}");
    }

    #[test]
    fn bevel_boss_matches_measured_lighting() {
        let mut img = RgbaImage::new(60, 60, Rgba::TRANSPARENT);
        for y in 10..50 {
            for x in 10..50 {
                img.set(x, y, Rgba::new(128, 128, 128, 255));
            }
        }
        let kv = params(&[("external_filter_key__Wdth", "382e303020706978656c73 0")]);
        BevelBoss::parse(&Params(&kv)).apply(&mut img);
        // Flat interior: colour x sin(45 degrees) (Fireworks: 196 -> 139).
        assert!(
            img.get(30, 30).unwrap().r.abs_diff(91) <= 2,
            "{:?}",
            img.get(30, 30)
        );
        // The default profile (edge high) makes a dish: the side facing the light (upper left,
        // 135 degrees) is the darker one, as in Fireworks' renders.
        let (l, r) = (img.get(12, 30).unwrap().r, img.get(47, 30).unwrap().r);
        assert!(l < r, "left {l} right {r}");
        assert_eq!(img.get(5, 5).unwrap().a, 0);
    }

    /// Both colour maths draw the same trail; Precise only drops the byte rounding.
    #[test]
    fn motion_trail_precise_matches_classic_shape() {
        let mut img = RgbaImage::new(90, 40, Rgba::TRANSPARENT);
        for y in 10..30 {
            for x in 10..30 {
                img.set(x, y, Rgba::new((x * 8) as u8, 120, (y * 8) as u8, 255));
            }
        }
        let kv = params(&[
            ("external_filter_key__Drct", "0"),
            ("external_filter_key__Lngt", "34302e303020706978656c73 0"),
        ]);
        let m = MotionTrail::parse(&Params(&kv));
        let (mut c, mut q) = (img.clone(), img);
        m.apply_with(&mut c, crate::ColorMath::Classic);
        m.apply_with(&mut q, crate::ColorMath::Precise);
        for (a, b) in c.pixels().iter().zip(q.pixels()) {
            assert!(a.a.abs_diff(b.a) <= 2, "{a:?} vs {b:?}");
        }
        assert!(q.get(50, 20).unwrap().a > 0);
    }

    #[test]
    fn motion_trail_streams_behind() {
        let mut img = RgbaImage::new(80, 20, Rgba::TRANSPARENT);
        for y in 5..15 {
            for x in 50..60 {
                img.set(x, y, Rgba::new(255, 0, 0, 255));
            }
        }
        let kv = params(&[
            ("external_filter_key__Drct", "0"),
            ("external_filter_key__Lngt", "32302e303020706978656c73 0"),
        ]);
        MotionTrail::parse(&Params(&kv)).apply(&mut img);
        // Direction 0: the trail streams to the right.
        assert!(img.get(70, 10).unwrap().a > 0);
        assert_eq!(img.get(40, 10).unwrap().a, 0);
    }
}
