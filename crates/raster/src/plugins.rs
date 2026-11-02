//! Built-in versions of the plug-in filters bundled with Fireworks MX: Alien Skin Eye Candy
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
    /// bevel, as measured), a smooth curve through the points (smoothstep segments).
    fn profile_at(&self, t: f32) -> f32 {
        let p = &self.profile;
        let t = t.clamp(0.0, 1.0);
        let k = p.iter().position(|q| q.0 >= t).unwrap_or(p.len() - 1).max(1);
        let (a, b) = (p[k - 1], p[k]);
        let span = (b.0 - a.0).max(1e-6);
        let u = ((t - a.0) / span).clamp(0.0, 1.0);
        // Smoothstep between neighbouring points (keeps the curve inside each segment).
        let s = u * u * (3.0 - 2.0 * u);
        a.1 + (b.1 - a.1) * s
    }

    /// Fitted to Fireworks MX renders of the bundled plug-in (docs/behavior/plugins.md):
    /// - height = profile(x) x width x Height Scale/100 x 1.5, with x = 1 at the object's edge
    ///   and 0 at `width` inside (flat beyond), smoothed by a Gaussian of 0.35 x Smoothness/100
    ///   x width;
    /// - colour = mix(Shadow Color, object, n.L) everywhere when Shade Interior (flat areas get
    ///   sin(inclination)), relative to a flat surface otherwise;
    /// - highlight = Highlight Color over it by (1 - angle(n, H) / (Highlight Size% of 45
    ///   degrees)) x Brightness, H halfway between the light and the viewer.
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
        let sigma = 0.35 * (self.smoothness as f64 / 100.0) * bw;
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
        let cone = ((self.highlight_size as f64 / 100.0) * 45.0).to_radians();
        let flat = light.2.max(1e-3);
        let src = img.clone();
        let at = |x: i64, y: i64| -> f64 {
            let (x, y) = (x.clamp(0, w as i64 - 1), y.clamp(0, h as i64 - 1));
            height.get(y as usize * w + x as usize).copied().unwrap_or(0.0)
        };
        let mix =
            |a: u8, b: u8, t: f64| (a as f64 + (b as f64 - a as f64) * t).round().clamp(0.0, 255.0) as u8;
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
                    (1.0 - ang / cone).max(0.0) * (self.brightness as f64 / 100.0)
                } else {
                    0.0
                };
                let mut p = src.pixels().get(i).copied().unwrap_or_default();
                if self.outside && !inside.get(i).copied().unwrap_or(false) {
                    // The outside rim takes the colour of the nearest object pixel.
                    let (dx, dy) = near.get(i).copied().unwrap_or((0, 0));
                    let j = ((y + dy as i64) as usize) * w + (x + dx as i64) as usize;
                    p = Rgba {
                        a: 255,
                        ..src.pixels().get(j).copied().unwrap_or_default()
                    };
                }
                let deep = (self.darken as f64 / 100.0)
                    * (1.0 - height.get(i).copied().unwrap_or(0.0) / scale.max(1e-6)).clamp(0.0, 1.0)
                    * 0.5;
                let lit = (diffuse * (1.0 - deep)).clamp(0.0, 1.0);
                let s = self.shadow;
                let mut q = Rgba::new(mix(s.r, p.r, lit), mix(s.g, p.g, lit), mix(s.b, p.b, lit), p.a);
                let k = spec.min(1.0);
                if k > 0.0 {
                    let hl = self.highlight;
                    q = Rgba::new(mix(q.r, hl.r, k), mix(q.g, hl.g, k), mix(q.b, hl.b, k), q.a);
                }
                if let Some(o) = img.pixels_mut().get_mut(i) {
                    *o = q;
                }
            }
        }
    }
}

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

    /// The trail streams out from the object towards `direction`, coloured by the object's
    /// edge (Smear Color from Edges) or its nearest pixels, fading over its length (Taper).
    pub fn apply(&self, img: &mut RgbaImage) {
        let (w, h) = (img.width() as i64, img.height() as i64);
        let steps = self.length.round().clamp(0.0, 2000.0) as i64;
        if steps < 1 {
            return;
        }
        let a = self.direction.to_radians();
        // From a trail pixel back towards the object.
        let (bx, by) = (-a.cos(), a.sin());
        let src = img.clone();
        let opacity = (self.opacity / 100.0).clamp(0.0, 1.0);
        let taper = (self.taper / 100.0).clamp(0.0, 1.0);
        for y in 0..h {
            for x in 0..w {
                let own = src.get(x, y).unwrap_or_default();
                if own.a == 255 || (self.outside_only && own.a > 0) {
                    continue;
                }
                // The first object pixel back along the trail: its colour and distance.
                let mut hit: Option<(i64, Rgba)> = None;
                for k in 1..=steps {
                    let (sx, sy) = (
                        (x as f32 + bx * k as f32).round() as i64,
                        (y as f32 + by * k as f32).round() as i64,
                    );
                    match src.get(sx, sy) {
                        Some(s) if s.a >= 128 => {
                            hit = Some((k, s));
                            break;
                        }
                        Some(_) => {}
                        None => break,
                    }
                }
                let Some((k, edge)) = hit else { continue };
                let fade = (1.0 - taper * (k as f32 / steps as f32)).max(0.0);
                let a = opacity * fade * (1.0 - k as f32 / (steps as f32 + 1.0)).sqrt();
                let colour = if self.smear { edge } else { edge };
                let trail = Rgba {
                    a: (a * 255.0).round() as u8,
                    ..colour
                };
                if let Some(p) = img.pixels_mut().get_mut((y * w + x) as usize) {
                    // Behind the object: the object's own (partial) pixel stays on top.
                    *p = trail.over(own, 255);
                }
            }
        }
    }
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
                        let (fu, fv) = ((u / cell).fract() - 0.5, (v / cell).fract() - 0.5);
                        let r = (fu * fu + fv * fv).sqrt();
                        if r < t * 0.72 { 1.0 } else { 0.0 }
                    }
                    // Halftone lines: lines thickening towards the inside.
                    5 => {
                        if ((v / cell).fract() - 0.5).abs() < t * 0.5 {
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
