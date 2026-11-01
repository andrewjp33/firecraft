//! Live Effects (Fireworks MX "Effects" list on an object), applied in list order to the
//! object's own pixels before its opacity and blend mode.
//!
//! Effects are identified in files by `EffectMoaID` and carry string parameters; [`Effect::parse`]
//! maps the ones FireCraft renders. Formulas are first implementations checked against Fireworks'
//! own renders with `parity_report`; exactness work per effect is tracked in
//! docs/behavior/effects.md. Third-party plug-ins (Eye Candy: Bevel Boss, Motion Trail, Edges)
//! are kept in the file but not rendered.

use std::collections::HashMap;

use crate::blend::BlendMode;
use crate::image::{Rgba, RgbaImage};

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    HueSaturation { hue: f32, saturation: f32, lightness: f32, colorize: bool },
    BrightnessContrast { brightness: f32, contrast: f32 },
    /// Input low/high, output low/high, gamma per channel: [rgb, r, g, b].
    Levels { channels: [[f32; 5]; 4] },
    Invert,
    ConvertToAlpha,
    ColorFill { color: Rgba, opacity: f32, mode: BlendMode },
    GaussianBlur { radius: f32 },
    MotionBlur { distance: f32, angle: f32 },
    RadialBlur { amount: f32 },
    /// Drop shadow (`inner` = Inner Shadow).
    Shadow { color: Rgba, distance: f32, angle: f32, softness: f32, inner: bool, knock_out: bool },
    /// Glow (outside) or Inner Glow.
    Glow { color: Rgba, width: f32, opacity: f32, softness: f32, offset: f32, inner: bool },
    /// Curves: control points per channel ([rgb, r, g, b]), each (input, output) 0..255.
    Curves { channels: [Vec<(f32, f32)>; 4] },
}

const HUE_SAT: &str = "{3439b08d-1922-11d3-9bde00e02910d580}";
const BRIGHT_CONTRAST: &str = "{3439b08c-1921-11d3-9bde00e02910d580}";
const CURVES: &str = "{3439b08e-1923-11d3-9bde00e02910d580}";
const LEVELS: &str = "{d04ef8c2-71b5-11d1-8c8200a024cdc039}";
const AUTO_LEVELS: &str = "{d04ef8c1-71b4-11d1-8c8200a024cdc039}";
const GAUSSIAN: &str = "{d04ef8c0-71b3-11d1-8c8200a024cdc039}";
const MOTION_BLUR: &str = "{538016b6-ffd6-418f-a10c590e6ee841ae}";
const RADIAL_BLUR: &str = "{d1c33142-2ad8-4215-a39fc5d934c8fc0c}";
const COLOR_FILL: &str = "{dd54adc0-a279-11d3-b92a000502f3fdbe}";
const DROP_SHADOW: &str = "{a7944db8-6ce2-11d1-8c76000502701850}";
const BEVEL_GLOW: &str = "{7fe61102-6ce2-11d1-8c76000502701850}";
const INNER_GLOW: &str = "{2ba87123-8220-11d3-baad0000861f4d01}";

impl Effect {
    /// From an effect's parameters (`EffectMoaID` plus the other keys), if FireCraft renders it.
    /// `vectors` holds vector-valued parameters (Curves points) as lists of (x, y).
    pub fn parse(p: &HashMap<String, String>, vectors: &HashMap<String, Vec<(f32, f32)>>) -> Option<Effect> {
        let num = |k: &str, d: f32| p.get(k).and_then(|v| v.trim().parse::<f32>().ok()).filter(|v| v.is_finite()).unwrap_or(d);
        let color = |k: &str, d: Rgba| p.get(k).and_then(|v| Rgba::from_hex(v)).unwrap_or(d);
        let truthy = |k: &str| p.get(k).is_some_and(|v| v == "true" || v == "1");
        let name = p.get("name").map(String::as_str).unwrap_or("");
        let id = p.get("EffectMoaID").map(String::as_str).unwrap_or("");
        Some(match id {
            HUE_SAT => Effect::HueSaturation {
                hue: num("hue_amount", 0.0),
                saturation: num("saturation_amount", 0.0),
                lightness: num("lightness_amount", 0.0),
                colorize: truthy("hls_colorize"),
            },
            BRIGHT_CONTRAST => Effect::BrightnessContrast { brightness: num("brightness_amount", 0.0), contrast: num("contrast_amount", 0.0) },
            LEVELS | AUTO_LEVELS => {
                let ch = |c: &str| {
                    [
                        num(&format!("source_low_{c}"), 0.0),
                        num(&format!("source_high_{c}"), 255.0),
                        num(&format!("dest_low_{c}"), 0.0),
                        num(&format!("dest_high_{c}"), 255.0),
                        num(&format!("gamma_{c}"), 1.0),
                    ]
                };
                Effect::Levels { channels: [ch("rgb"), ch("red"), ch("green"), ch("blue")] }
            }
            CURVES => {
                let pts = |k: &str| vectors.get(k).cloned().unwrap_or_default();
                Effect::Curves { channels: [pts("rgb_points"), pts("red_points"), pts("green_points"), pts("blue_points")] }
            }
            // Radius in pixels (measured: radius 100 spreads a square ~100 px each way).
            GAUSSIAN => Effect::GaussianBlur { radius: num("gaussian_blur_radius", 1.0) },
            MOTION_BLUR => Effect::MotionBlur { distance: num("motion_blur_distance", 10.0), angle: num("motion_blur_angle", 0.0) },
            RADIAL_BLUR => Effect::RadialBlur { amount: num("radial_blur_amount", 5.0) },
            COLOR_FILL => Effect::ColorFill {
                color: color("Color", Rgba::BLACK),
                opacity: num("Opacity", 100.0) / 100.0,
                mode: BlendMode::from_fw_code(num("Blendmode", 0.0) as i64),
            },
            DROP_SHADOW => Effect::Shadow {
                color: color("ShadowColor", Rgba::new(0, 0, 0, 165)),
                distance: num("ShadowDistance", 7.0),
                angle: num("ShadowAngle", 315.0),
                softness: num("ShadowBlur", 4.0),
                inner: num("ShadowType", 0.0) == 1.0,
                knock_out: truthy("ShadowKnockOut") || p.get("ShowObject").is_some_and(|v| v == "false") && name.contains("Knock"),
            },
            // Measured: the glow's width is GlowWidth (0 = no glow), colour OuterBevelColor,
            // softness MaskSoftness, gap GlowStartDistance, opacity BevelContrast.
            BEVEL_GLOW if num("BevelType", 0.0) == 4.0 => Effect::Glow {
                color: color("OuterBevelColor", Rgba::BLACK),
                width: num("GlowWidth", 0.0),
                opacity: num("BevelContrast", 100.0) / 100.0,
                softness: num("MaskSoftness", 0.0),
                offset: num("GlowStartDistance", 0.0),
                inner: false,
            },
            INNER_GLOW => Effect::Glow {
                color: color("OuterBevelColor", Rgba::BLACK),
                width: num("GlowWidth", 4.0),
                opacity: num("BevelContrast", 100.0) / 100.0,
                softness: num("MaskSoftness", 0.0),
                offset: num("GlowStartDistance", 0.0),
                inner: true,
            },
            _ if name == "Invert" || id == "{3439b08f-1924-11d3-9bde00e02910d580}" => Effect::Invert,
            _ if name.starts_with("Convert to Alpha") => Effect::ConvertToAlpha,
            _ => return None,
        })
    }

    /// Apply to `img` (the object's pixels; effects may draw outside the object, so callers pass
    /// a buffer with room around it).
    /// Apply the effect, bit-exact Fireworks MX.
    pub fn apply(&self, img: &mut RgbaImage) {
        self.apply_with(img, crate::ColorMath::Classic);
    }

    /// Apply the effect with a choice of colour-math precision.
    pub fn apply_with(&self, img: &mut RgbaImage, math: crate::ColorMath) {
        match self {
            Effect::HueSaturation { hue, saturation, lightness, colorize: false } if math == crate::ColorMath::Precise => {
                for p in img.pixels_mut() {
                    if p.a != 0 {
                        let c = [p.r, p.g, p.b].map(f64::from);
                        let [r, g, b] = crate::colormath::hue_saturation(c, *hue as f64, *saturation as f64, *lightness as f64)
                            .map(|v| v.round().clamp(0.0, 255.0) as u8);
                        *p = Rgba::new(r, g, b, p.a);
                    }
                }
            }
            Effect::HueSaturation { hue, saturation, lightness, colorize: false } => {
                // Bit-exact Fireworks MX (byte-quantized HSL, see fwhsl).
                for p in img.pixels_mut() {
                    if p.a != 0 {
                        let [r, g, b] = crate::fwhsl::hue_saturation(p.r, p.g, p.b, hue.round() as i32, *saturation as f64, *lightness as f64);
                        *p = Rgba::new(r, g, b, p.a);
                    }
                }
            }
            Effect::HueSaturation { hue, saturation, lightness, colorize: true } => {
                map_rgb(img, |c| hue_saturation(c, *hue, *saturation, *lightness, true))
            }
            Effect::BrightnessContrast { brightness, contrast } => {
                let (b, c) = (*brightness, *contrast);
                let k = (100.0 + c) / 100.0;
                map_rgb(img, |v| v.map(|x| ((x + b - 127.5) * k + 127.5).clamp(0.0, 255.0)))
            }
            Effect::Levels { channels } => {
                let lut = |ch: [f32; 5]| -> [f32; 256] {
                    let [il, ih, ol, oh, g] = ch;
                    std::array::from_fn(|i| {
                        let t = ((i as f32 - il) / (ih - il).max(1.0)).clamp(0.0, 1.0);
                        ol + (oh - ol) * t.powf(1.0 / g.max(0.01))
                    })
                };
                let all = lut(channels[0]);
                let per: [[f32; 256]; 3] = [lut(channels[1]), lut(channels[2]), lut(channels[3])];
                map_rgb(img, |v| std::array::from_fn(|i| per[i][all[v[i] as usize] as usize]))
            }
            Effect::Curves { channels } => {
                let lut = |pts: &Vec<(f32, f32)>| -> [f32; 256] {
                    if pts.len() < 2 {
                        return std::array::from_fn(|i| i as f32);
                    }
                    let mut p = pts.clone();
                    p.sort_by(|a, b| a.0.total_cmp(&b.0));
                    std::array::from_fn(|i| {
                        let x = i as f32;
                        let k = p.iter().position(|q| q.0 >= x).unwrap_or(p.len() - 1).max(1);
                        let (a, b) = (p[k - 1], p[k]);
                        let t = if b.0 > a.0 { ((x - a.0) / (b.0 - a.0)).clamp(0.0, 1.0) } else { 0.0 };
                        (a.1 + (b.1 - a.1) * t).clamp(0.0, 255.0)
                    })
                };
                let all = lut(&channels[0]);
                let per = [lut(&channels[1]), lut(&channels[2]), lut(&channels[3])];
                map_rgb(img, |v| std::array::from_fn(|i| per[i][all[v[i] as usize] as usize]))
            }
            Effect::Invert => map_rgb(img, |v| v.map(|x| 255.0 - x)),
            Effect::ConvertToAlpha => {
                for p in img.pixels_mut() {
                    let luma = (p.r as u32 * 109 + p.g as u32 * 366 + p.b as u32 * 37) >> 9;
                    let a = (255 - luma) * p.a as u32 / 255;
                    *p = if a == 0 { Rgba::TRANSPARENT } else { Rgba::new(0, 0, 0, a as u8) };
                }
            }
            Effect::ColorFill { color, opacity, mode } => {
                let op = (opacity.clamp(0.0, 1.0) * 255.0) as u8;
                for p in img.pixels_mut() {
                    if p.a == 0 {
                        continue;
                    }
                    let opaque = Rgba { a: 255, ..*p };
                    let c = mode.composite_with(opaque, *color, op, math);
                    *p = Rgba { a: p.a, ..c };
                }
            }
            Effect::GaussianBlur { radius } => {
                if let Some(sigma) = gaussian_sigma(*radius) {
                    blur_rgba(img, sigma)
                }
            }
            Effect::MotionBlur { distance, angle } => motion_blur(img, *distance, *angle),
            Effect::RadialBlur { amount } => radial_blur(img, *amount),
            Effect::Shadow { color, distance, angle, softness, inner, knock_out } => {
                let a = angle.to_radians();
                // Fireworks measures the angle counter-clockwise with y up.
                let (dx, dy) = ((a.cos() * distance).round() as i64, (-a.sin() * distance).round() as i64);
                let mut alpha = alpha_plane(img);
                if *inner {
                    alpha.iter_mut().for_each(|v| *v = 255.0 - *v);
                }
                let shifted = shift_plane(&alpha, img.width(), img.height(), dx, dy, if *inner { 255.0 } else { 0.0 });
                let soft = blur_plane(&shifted, img.width(), img.height(), *softness * SOFTNESS_SIGMA);
                if *inner {
                    paint_inside(img, &soft, *color);
                } else {
                    paint_under(img, &soft, *color, *knock_out);
                }
            }
            Effect::Glow { color, width, opacity, softness, offset, inner } => {
                if *width <= 0.0 {
                    return;
                }
                let (w, h) = (img.width(), img.height());
                // Measured: a hard band of pixels whose Euclidean distance d to the object
                // (outside it, or to the outside for Inner Glow) satisfies offset < d <= offset + width.
                let inside: Vec<bool> = img.pixels().iter().map(|p| (p.a >= 128) != *inner).collect();
                let dist = distance_to(&inside, w, h);
                let band: Vec<f32> = dist
                    .iter()
                    .map(|&d| if d > *offset && d <= offset + width { 255.0 } else { 0.0 })
                    .collect();
                let mut soft = blur_plane(&band, w, h, *softness * SOFTNESS_SIGMA);
                soft.iter_mut().for_each(|v| *v *= opacity.clamp(0.0, 1.0));
                let c = Rgba { a: 255, ..*color };
                if *inner {
                    paint_inside(img, &soft, c);
                } else {
                    paint_under(img, &soft, c, false);
                }
            }
        }
    }

    /// Extra room an effect needs around the object, in pixels.
    pub fn margin(&self) -> u32 {
        let m = match self {
            // The box-blur approximation reaches 3 sigma (Fireworks' kernel ~2.5).
            Effect::GaussianBlur { radius } => gaussian_sigma(*radius).map_or(0.0, |s| s * 3.0),
            Effect::MotionBlur { distance, .. } => *distance,
            Effect::Shadow { distance, softness, .. } => distance + softness * 2.0,
            Effect::Glow { width, softness, offset, .. } => width + offset + softness,
            _ => 0.0,
        };
        m.clamp(0.0, 500.0).ceil() as u32 + 2
    }
}

/// Measured in Fireworks MX for Drop Shadow and Glow softness: a Gaussian with this sigma per
/// unit of softness (fit RMS 0.006 on a softness-8 shadow edge).
const SOFTNESS_SIGMA: f32 = 0.475;

/// Euclidean distance from each pixel to the nearest `true` pixel (0 on them). Exact squared
/// distance transform (Felzenszwalb & Huttenlocher), separable, O(pixels).
fn distance_to(set: &[bool], w: u32, h: u32) -> Vec<f32> {
    const INF: f32 = 1e20;
    let (w, h) = (w as usize, h as usize);
    let mut d: Vec<f32> = set.iter().map(|&s| if s { 0.0 } else { INF }).collect();
    let mut f = vec![0f32; w.max(h)];
    let mut out = vec![0f32; w.max(h)];
    for x in 0..w {
        for y in 0..h {
            f[y] = d.get(y * w + x).copied().unwrap_or(INF);
        }
        edt_1d(&f[..h], &mut out[..h]);
        for y in 0..h {
            if let Some(v) = d.get_mut(y * w + x) {
                *v = out[y];
            }
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(d.get(y * w..y * w + w).unwrap_or(&[]));
        edt_1d(&f[..w], &mut out[..w]);
        if let Some(row) = d.get_mut(y * w..y * w + w) {
            row.copy_from_slice(&out[..w]);
        }
    }
    d.iter().map(|v| v.sqrt()).collect()
}

fn edt_1d(f: &[f32], d: &mut [f32]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut v = vec![0usize; n];
    let mut z = vec![0f32; n + 1];
    let mut k = 0usize;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        loop {
            let p = v[k];
            let s = ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * q as f32 - 2.0 * p as f32);
            if s <= z[k] && k > 0 {
                k -= 1;
                continue;
            }
            if s <= z[k] {
                // k == 0: replace the only parabola.
                v[0] = q;
                z[1] = f32::INFINITY;
                break;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f32::INFINITY;
            break;
        }
    }
    k = 0;
    for (q, dq) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let p = v[k];
        *dq = (q as f32 - p as f32).powi(2) + f[p];
    }
}

fn map_rgb(img: &mut RgbaImage, f: impl Fn([f32; 3]) -> [f32; 3]) {
    for p in img.pixels_mut() {
        if p.a == 0 {
            continue;
        }
        let o = f([p.r as f32, p.g as f32, p.b as f32]);
        let c = |v: f32| v.round().clamp(0.0, 255.0) as u8;
        *p = Rgba::new(c(o[0]), c(o[1]), c(o[2]), p.a);
    }
}

/// Gaussian Blur's sigma for a `gaussian_blur_radius`, measured in Fireworks MX (step-edge fits for
/// radii 1-249): sigma = 1.021 x radius with the kernel reaching ~2.5 x radius; radii above 70 blur
/// like 70, and radii above 200 don't blur at all.
pub fn gaussian_sigma(radius: f32) -> Option<f32> {
    if !(radius > 0.0 && radius <= 200.0) {
        return None;
    }
    Some(radius.min(70.0) * 1.021)
}

fn hue_saturation(c: [f32; 3], hue: f32, sat: f32, light: f32, colorize: bool) -> [f32; 3] {
    let (mut h, mut s, mut l) = rgb_to_hsl(c);
    if colorize {
        h = hue.rem_euclid(360.0) / 360.0;
        s = (sat.clamp(0.0, 100.0) / 100.0).max(0.25);
    } else {
        // Measured in Fireworks MX: hue adds degrees; saturation and lightness move towards 0 by
        // (1 + k) for negative k and towards 1 by (1 - x) * k for positive k (k = amount / 100).
        h = (h + hue / 360.0).rem_euclid(1.0);
        let k = sat.clamp(-100.0, 100.0) / 100.0;
        s = if k >= 0.0 { s + (1.0 - s) * k } else { s * (1.0 + k) }.clamp(0.0, 1.0);
    }
    l = if light >= 0.0 { l + (1.0 - l) * light / 100.0 } else { l * (1.0 + light / 100.0) };
    hsl_to_rgb(h, s, l.clamp(0.0, 1.0))
}

fn rgb_to_hsl(c: [f32; 3]) -> (f32, f32, f32) {
    let [r, g, b] = c.map(|v| v / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d <= 0.0 {
        return (0.0, 0.0, l);
    }
    let s = if l <= 0.5 { d / (max + min) } else { d / (2.0 - max - min) };
    let h = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let p = 2.0 * l - q;
    let f = |t: f32| {
        let t = t.rem_euclid(1.0);
        let v = if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        };
        v * 255.0
    };
    [f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0)]
}

fn alpha_plane(img: &RgbaImage) -> Vec<f32> {
    img.pixels().iter().map(|p| p.a as f32).collect()
}

fn shift_plane(a: &[f32], w: u32, h: u32, dx: i64, dy: i64, fill: f32) -> Vec<f32> {
    let (w, h) = (w as i64, h as i64);
    let mut out = vec![fill; a.len()];
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = (x - dx, y - dy);
            if sx >= 0 && sy >= 0 && sx < w && sy < h {
                if let (Some(o), Some(&v)) = (out.get_mut((y * w + x) as usize), a.get((sy * w + sx) as usize)) {
                    *o = v;
                }
            }
        }
    }
    out
}

/// Gaussian blur of a single plane with standard deviation `sigma`, as three box passes (box
/// radius r gives sigma ~ r + 0.5).
fn blur_plane(a: &[f32], w: u32, h: u32, sigma: f32) -> Vec<f32> {
    let mut v = a.to_vec();
    if sigma < 0.5 || w == 0 || h == 0 {
        return v;
    }
    let r = (sigma.min(400.0) - 0.5).round().max(1.0) as i64;
    for _ in 0..3 {
        box_pass(&mut v, w as i64, h as i64, r, true);
        box_pass(&mut v, w as i64, h as i64, r, false);
    }
    v
}

fn box_pass(v: &mut [f32], w: i64, h: i64, r: i64, horizontal: bool) {
    let src = v.to_vec();
    let (outer, inner) = if horizontal { (h, w) } else { (w, h) };
    let at = |o: i64, i: i64| -> f32 {
        if i < 0 || i >= inner {
            return 0.0;
        }
        let idx = if horizontal { o * w + i } else { i * w + o };
        src.get(idx as usize).copied().unwrap_or(0.0)
    };
    let n = (2 * r + 1) as f32;
    for o in 0..outer {
        let mut sum: f32 = (-r..=r).map(|k| at(o, k)).sum();
        for i in 0..inner {
            let idx = if horizontal { o * w + i } else { i * w + o };
            if let Some(d) = v.get_mut(idx as usize) {
                *d = sum / n;
            }
            sum += at(o, i + r + 1) - at(o, i - r);
        }
    }
}

/// Composite a coloured plane underneath the object's pixels.
fn paint_under(img: &mut RgbaImage, plane: &[f32], color: Rgba, knock_out: bool) {
    for (p, &a) in img.pixels_mut().iter_mut().zip(plane) {
        let sa = (a * color.a as f32 / 255.0).round().clamp(0.0, 255.0) as u8;
        let under = Rgba { a: sa, ..color };
        *p = if knock_out {
            // Only the shadow, cut out where the object is.
            Rgba { a: (sa as u32 * (255 - p.a as u32) / 255) as u8, ..color }
        } else {
            under.over(*p, 255)
        };
    }
}

/// Composite a coloured plane over the object, inside its alpha.
fn paint_inside(img: &mut RgbaImage, plane: &[f32], color: Rgba) {
    for (p, &a) in img.pixels_mut().iter_mut().zip(plane) {
        if p.a == 0 {
            continue;
        }
        let k = (a * color.a as f32 / 255.0).round().clamp(0.0, 255.0) as u8;
        let mixed = Rgba { a: 255, ..*p }.over(Rgba { a: 255, ..color }, k);
        *p = Rgba { a: p.a, ..mixed };
    }
}

fn blur_rgba(img: &mut RgbaImage, radius: f32) {
    let (w, h) = (img.width(), img.height());
    // Blur premultiplied channels so transparent pixels don't darken edges.
    let px = img.pixels();
    let a: Vec<f32> = px.iter().map(|p| p.a as f32).collect();
    let pm = |f: fn(&Rgba) -> u8| -> Vec<f32> { px.iter().map(|p| f(p) as f32 * p.a as f32 / 255.0).collect() };
    let (r, g, b) = (pm(|p| p.r), pm(|p| p.g), pm(|p| p.b));
    let (a, r, g, b) = (blur_plane(&a, w, h, radius), blur_plane(&r, w, h, radius), blur_plane(&g, w, h, radius), blur_plane(&b, w, h, radius));
    write_premultiplied(img, &a, &r, &g, &b);
}

fn write_premultiplied(img: &mut RgbaImage, a: &[f32], r: &[f32], g: &[f32], b: &[f32]) {
    for (i, p) in img.pixels_mut().iter_mut().enumerate() {
        let al = a.get(i).copied().unwrap_or(0.0);
        if al < 0.5 {
            *p = Rgba::TRANSPARENT;
            continue;
        }
        let c = |v: Option<&f32>| (v.copied().unwrap_or(0.0) * 255.0 / al).round().clamp(0.0, 255.0) as u8;
        *p = Rgba::new(c(r.get(i)), c(g.get(i)), c(b.get(i)), al.round().clamp(0.0, 255.0) as u8);
    }
}

fn motion_blur(img: &mut RgbaImage, distance: f32, angle: f32) {
    let steps = distance.clamp(0.0, 200.0).round() as i64;
    if steps < 1 {
        return;
    }
    let (ca, sa) = (angle.to_radians().cos(), -angle.to_radians().sin());
    let src = img.clone();
    let (w, h) = (img.width() as i64, img.height() as i64);
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0f32; 4];
            for s in 0..=steps {
                let t = s as f32 - steps as f32 / 2.0;
                let p = src.get(x + (ca * t).round() as i64, y + (sa * t).round() as i64).unwrap_or_default();
                let a = p.a as f32;
                acc[0] += p.r as f32 * a;
                acc[1] += p.g as f32 * a;
                acc[2] += p.b as f32 * a;
                acc[3] += a;
            }
            let n = (steps + 1) as f32;
            let out = if acc[3] <= 0.0 {
                Rgba::TRANSPARENT
            } else {
                Rgba::new((acc[0] / acc[3]) as u8, (acc[1] / acc[3]) as u8, (acc[2] / acc[3]) as u8, (acc[3] / n) as u8)
            };
            img.set(x, y, out);
        }
    }
}

fn radial_blur(img: &mut RgbaImage, amount: f32) {
    // Spin blur around the image centre: average samples along an arc.
    let (w, h) = (img.width() as f32, img.height() as f32);
    let (cx, cy) = (w / 2.0, h / 2.0);
    let src = img.clone();
    let arc = amount.clamp(0.0, 100.0).to_radians();
    let n = 12;
    for y in 0..img.height() as i64 {
        for x in 0..img.width() as i64 {
            let (dx, dy) = (x as f32 - cx, y as f32 - cy);
            let mut acc = [0f32; 4];
            for k in 0..n {
                let t = (k as f32 / (n - 1) as f32 - 0.5) * arc;
                let (s, c) = t.sin_cos();
                let p = src.get((cx + dx * c - dy * s).round() as i64, (cy + dx * s + dy * c).round() as i64).unwrap_or_default();
                let a = p.a as f32;
                acc[0] += p.r as f32 * a;
                acc[1] += p.g as f32 * a;
                acc[2] += p.b as f32 * a;
                acc[3] += a;
            }
            let out = if acc[3] <= 0.0 {
                Rgba::TRANSPARENT
            } else {
                Rgba::new((acc[0] / acc[3]) as u8, (acc[1] / acc[3]) as u8, (acc[2] / acc[3]) as u8, (acc[3] / n as f32) as u8)
            };
            img.set(x, y, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(kv: &[(&str, &str)]) -> HashMap<String, String> {
        kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn parses_known_effects() {
        let e = Effect::parse(&params(&[("EffectMoaID", GAUSSIAN), ("gaussian_blur_radius", "100")]), &HashMap::new());
        assert_eq!(e, Some(Effect::GaussianBlur { radius: 100.0 }));
        assert_eq!(gaussian_sigma(100.0), gaussian_sigma(70.0));
        assert_eq!(gaussian_sigma(201.0), None);
        let e = Effect::parse(&params(&[("EffectMoaID", BEVEL_GLOW), ("BevelType", "0")]), &HashMap::new());
        assert_eq!(e, None, "inner bevel not rendered yet");
        assert!(Effect::parse(&params(&[("name", "Invert")]), &HashMap::new()).is_some());
    }

    #[test]
    fn shadow_and_glow_add_pixels_outside() {
        let mut img = RgbaImage::new(40, 40, Rgba::TRANSPARENT);
        for y in 15..25 {
            for x in 15..25 {
                img.set(x, y, Rgba::new(255, 0, 0, 255));
            }
        }
        let mut s = img.clone();
        Effect::Shadow { color: Rgba::new(0, 0, 0, 165), distance: 5.0, angle: 315.0, softness: 2.0, inner: false, knock_out: false }.apply(&mut s);
        assert!(s.get(28, 28).is_some_and(|p| p.a > 50), "shadow down-right");
        assert_eq!(s.get(20, 20), Some(Rgba::new(255, 0, 0, 255)), "object on top");
        let mut g = img.clone();
        Effect::Glow { color: Rgba::WHITE, width: 4.0, opacity: 1.0, softness: 0.0, offset: 0.0, inner: false }.apply(&mut g);
        assert!(g.get(12, 20).is_some_and(|p| p.a == 255 && p.r == 255 && p.g == 255));
        assert!(g.get(10, 20).is_some_and(|p| p.a == 0), "width 4 stops at distance 4");
        // Measured corner rounding: diagonal distance 4*sqrt2 = 5.66 is outside a width-4 glow.
        assert!(g.get(11, 11).is_some_and(|p| p.a == 0));
    }

    #[test]
    fn distance_transform_is_euclidean() {
        let mut set = vec![false; 25];
        set[12] = true; // centre of 5x5
        let d = distance_to(&set, 5, 5);
        assert_eq!(d[12], 0.0);
        assert!((d[0] - 8f32.sqrt()).abs() < 1e-4);
        assert!((d[14] - 2.0).abs() < 1e-4);
    }

    /// Fireworks MX renders (oracle_hsl): (input, [hue, sat, light], output).
    #[test]
    fn hue_saturation_matches_fireworks() {
        let cases: [([u8; 3], [f32; 3], [u8; 3]); 8] = [
            ([200, 40, 40], [-172.0, 0.0, 0.0], [40, 175, 199]),
            ([40, 180, 60], [-172.0, 0.0, 0.0], [178, 39, 140]),
            ([30, 60, 200], [45.0, 0.0, 0.0], [121, 30, 199]),
            ([128, 128, 128], [0.0, 50.0, 0.0], [191, 64, 64]),
            ([200, 40, 40], [0.0, 50.0, 0.0], [219, 20, 20]),
            ([200, 40, 40], [0.0, -50.0, 0.0], [160, 80, 80]),
            ([200, 40, 40], [0.0, 0.0, -40.0], [119, 24, 24]),
            ([200, 40, 40], [0.0, 0.0, 40.0], [228, 119, 119]),
        ];
        for (input, [h, s, l], want) in cases {
            let mut img = RgbaImage::new(1, 1, Rgba::new(input[0], input[1], input[2], 255));
            Effect::HueSaturation { hue: h, saturation: s, lightness: l, colorize: false }.apply(&mut img);
            let p = img.get(0, 0).unwrap();
            let close = [p.r, p.g, p.b].iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 2);
            assert!(close, "{input:?} {h},{s},{l}: got {p:?}, want {want:?}");
        }
    }

    #[test]
    fn hue_shift_rotates_colour() {
        let mut img = RgbaImage::new(1, 1, Rgba::new(255, 0, 0, 255));
        Effect::HueSaturation { hue: 120.0, saturation: 0.0, lightness: 0.0, colorize: false }.apply(&mut img);
        let p = img.get(0, 0).unwrap();
        assert!(p.g > 250 && p.r < 5, "{p:?}");
    }
}
