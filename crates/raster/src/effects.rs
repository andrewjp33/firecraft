//! Live Effects (Fireworks MX "Effects" list on an object), applied in list order to the
//! object's own pixels before its opacity and blend mode.
//!
//! Effects are identified in files by `EffectMoaID` and carry string parameters; [`Effect::parse`]
//! maps the ones FireCraft renders. Formulas are first implementations checked against Fireworks'
//! own renders with `parity_report`; exactness work per effect is tracked in
//! docs/behavior/effects.md. The bundled plug-ins (Eye Candy 4000 LE, Splat LE) have built-in
//! versions in `plugins`.

use std::collections::HashMap;

use crate::blend::BlendMode;
use crate::image::{Rgba, RgbaImage};

#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    HueSaturation {
        hue: f32,
        saturation: f32,
        lightness: f32,
        colorize: bool,
    },
    BrightnessContrast {
        brightness: f32,
        contrast: f32,
    },
    /// Input low/high, output low/high, gamma per channel: [rgb, r, g, b].
    Levels {
        channels: [[f32; 5]; 4],
    },
    /// Auto Levels: Levels from the object's own histogram (see [`crate::filters::auto_levels`]).
    AutoLevels,
    Invert,
    ConvertToAlpha,
    /// Blur (strength 3) and Blur More (7), a 3x3 filter.
    SimpleBlur {
        strength: i32,
    },
    /// Sharpen and Sharpen More, a 3x3 filter (`sharpen_amount`; Live Effects store none: 1).
    Sharpen {
        strength: i32,
    },
    FindEdges,
    UnsharpMask {
        radius: f32,
        amount: f32,
        threshold: i32,
    },
    /// FireCraft's Adjust Color > Grayscale: each pixel's perceived lightness, from its
    /// luminance (see [`perceptual_gray`]). Stored as Hue/Saturation at saturation −100 with a
    /// `firecraft_effect` marker, so Fireworks MX shows its own desaturation instead.
    Grayscale,
    ColorFill {
        color: Rgba,
        opacity: f32,
        mode: BlendMode,
    },
    GaussianBlur {
        radius: f32,
    },
    MotionBlur {
        distance: f32,
        angle: f32,
    },
    RadialBlur {
        amount: f32,
    },
    /// Drop shadow (`inner` = Inner Shadow).
    Shadow {
        color: Rgba,
        distance: f32,
        angle: f32,
        softness: f32,
        inner: bool,
        knock_out: bool,
    },
    /// Glow (outside) or Inner Glow.
    Glow {
        color: Rgba,
        width: f32,
        opacity: f32,
        softness: f32,
        offset: f32,
        inner: bool,
    },
    /// Curves: control points per channel ([rgb, r, g, b]), each (input, output) 0..255.
    Curves {
        channels: [Vec<(f32, f32)>; 4],
    },
    /// Inner/Outer Bevel, Raised/Inset Emboss.
    Bevel(crate::bevel::Bevel),
    /// The bundled plug-ins (the LE demo editions of Eye Candy 4000 and Splat), built in.
    BevelBoss(crate::plugins::BevelBoss),
    MotionTrail(crate::plugins::MotionTrail),
    Edges(crate::plugins::Edges),
    Marble(crate::plugins::Marble),
}

const HUE_SAT: &str = "{3439b08d-1922-11d3-9bde00e02910d580}";
/// The parameter naming a FireCraft effect stored as a Fireworks one (Fireworks ignores it).
pub const FIRECRAFT_EFFECT: &str = "firecraft_effect";
const BRIGHT_CONTRAST: &str = "{3439b08c-1921-11d3-9bde00e02910d580}";
const CURVES: &str = "{3439b08e-1923-11d3-9bde00e02910d580}";
const LEVELS: &str = "{d04ef8c2-71b5-11d1-8c8200a024cdc039}";
const AUTO_LEVELS: &str = "{d04ef8c1-71b4-11d1-8c8200a024cdc039}";
const INVERT: &str = "{d2541291-70d6-11d1-8c8000a024cdc039}";
const BLUR: &str = "{f1cfce41-718e-11d1-8c8200a024cdc039}";
const BLUR_MORE: &str = "{f1cfce42-718e-11d1-8c8200a024cdc039}";
const SIMPLE_BLUR: &str = "{a2dea491-fd45-11d0-8be900a024cdc039}";
const SHARPEN: &str = "{c20952b1-fc76-11d0-8be700a024cdc039}";
const SHARPEN_MORE: &str = "{1f2f2591-9db7-11d1-8cad00a024cdc039}";
const UNSHARP_MASK: &str = "{f1cfce44-718e-11d1-8c8200a024cdc039}";
const FIND_EDGES: &str = "{fc7093f1-f95c-11d0-8be200a024cdc039}";
const GAUSSIAN: &str = "{d04ef8c0-71b3-11d1-8c8200a024cdc039}";
const MOTION_BLUR: &str = "{538016b6-ffd6-418f-a10c590e6ee841ae}";
const RADIAL_BLUR: &str = "{d1c33142-2ad8-4215-a39fc5d934c8fc0c}";
const COLOR_FILL: &str = "{dd54adc0-a279-11d3-b92a000502f3fdbe}";
const DROP_SHADOW: &str = "{a7944db8-6ce2-11d1-8c76000502701850}";
const BEVEL_GLOW: &str = "{7fe61102-6ce2-11d1-8c76000502701850}";
const INNER_GLOW: &str = "{2ba87123-8220-11d3-baad0000861f4d01}";
const INNER_SHADOW: &str = "{5600f702-774c-11d3-baad0000861f4d01}";

impl Effect {
    /// The effect as it would look on an object drawn `factor` times larger: its pixel sizes grown
    /// to match. `None` for effects that can't be resized this way.
    pub fn scaled(&self, factor: f32) -> Option<Effect> {
        if self.is_scale_free() {
            return Some(self.clone());
        }
        let mut e = self.clone();
        match &mut e {
            Effect::GaussianBlur { radius } => *radius *= factor,
            Effect::MotionBlur { distance, .. } => *distance *= factor,
            Effect::Shadow {
                distance, softness, ..
            } => {
                *distance *= factor;
                *softness *= factor;
            }
            Effect::Glow {
                width,
                softness,
                offset,
                ..
            } => {
                *width *= factor;
                *softness *= factor;
                *offset *= factor;
            }
            Effect::Bevel(b) => {
                b.width *= factor;
                b.softness *= factor;
            }
            _ => return None,
        }
        Some(e)
    }

    /// Whether the effect works on each pixel by itself, so it looks the same at any drawing
    /// scale (colour adjustments), unlike blurs, shadows, glows and bevels, sized in pixels.
    pub fn is_scale_free(&self) -> bool {
        matches!(
            self,
            Effect::HueSaturation { .. }
                | Effect::BrightnessContrast { .. }
                | Effect::Levels { .. }
                | Effect::Curves { .. }
                | Effect::AutoLevels
                | Effect::Invert
                | Effect::ConvertToAlpha
                | Effect::Grayscale
                | Effect::ColorFill { .. }
        )
    }

    /// From an effect's parameters (`EffectMoaID` plus the other keys), if FireCraft renders it.
    /// `vectors` holds vector-valued parameters (Curves points) as lists of (x, y).
    pub fn parse(p: &HashMap<String, String>, vectors: &HashMap<String, Vec<(f32, f32)>>) -> Option<Effect> {
        let num = |k: &str, d: f32| {
            p.get(k)
                .and_then(|v| v.trim().parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .unwrap_or(d)
        };
        let color = |k: &str, d: Rgba| p.get(k).and_then(|v| Rgba::from_hex(v)).unwrap_or(d);
        let truthy = |k: &str| p.get(k).is_some_and(|v| v == "true" || v == "1");
        let name = p.get("name").map(String::as_str).unwrap_or("");
        let id = p.get("EffectMoaID").map(String::as_str).unwrap_or("");
        // FireCraft's own effects, stored as a Fireworks effect Fireworks MX renders instead:
        // only while the record is still exactly what FireCraft wrote. Fireworks MX keeps the
        // marker when the effect is edited there, but renames it "Hue/Saturation..." and stores
        // the new values (minions_banana2_gray.png: saturation -60), and then it's that effect.
        let is = |k: &str, v: &str| p.get(k).map(String::as_str) == Some(v);
        let untouched = name == "Grayscale"
            && num("saturation_amount", 0.0) == -100.0
            && num("hue_amount", 0.0) == 0.0
            && num("lightness_amount", 0.0) == 0.0
            && !truthy("hls_colorize");
        if is(FIRECRAFT_EFFECT, "grayscale") && untouched && id == HUE_SAT {
            return Some(Effect::Grayscale);
        }
        Some(match id {
            HUE_SAT => Effect::HueSaturation {
                hue: num("hue_amount", 0.0),
                saturation: num("saturation_amount", 0.0),
                lightness: num("lightness_amount", 0.0),
                colorize: truthy("hls_colorize"),
            },
            BRIGHT_CONTRAST => Effect::BrightnessContrast {
                brightness: num("brightness_amount", 0.0),
                contrast: num("contrast_amount", 0.0),
            },
            AUTO_LEVELS => Effect::AutoLevels,
            INVERT => Effect::Invert,
            BLUR => Effect::SimpleBlur { strength: 3 },
            BLUR_MORE => Effect::SimpleBlur { strength: 7 },
            SIMPLE_BLUR => Effect::SimpleBlur {
                strength: num("simple_blur_amount", 1.0) as i32,
            },
            // Live Effects store no `sharpen_amount`: both draw with strength 1 (measured).
            SHARPEN | SHARPEN_MORE => Effect::Sharpen {
                strength: num("sharpen_amount", 1.0) as i32,
            },
            FIND_EDGES => Effect::FindEdges,
            UNSHARP_MASK => Effect::UnsharpMask {
                radius: num("unsharp_mask_radius", 4.1),
                amount: num("unsharp_mask_amount", 50.0),
                threshold: num("unsharp_mask_threshold", 0.0) as i32,
            },
            LEVELS => {
                let ch = |c: &str| {
                    [
                        num(&format!("source_low_{c}"), 0.0),
                        num(&format!("source_high_{c}"), 255.0),
                        num(&format!("dest_low_{c}"), 0.0),
                        num(&format!("dest_high_{c}"), 255.0),
                        num(&format!("gamma_{c}"), 1.0),
                    ]
                };
                Effect::Levels {
                    channels: [ch("rgb"), ch("red"), ch("green"), ch("blue")],
                }
            }
            CURVES => {
                let pts = |k: &str| vectors.get(k).cloned().unwrap_or_default();
                Effect::Curves {
                    channels: [
                        pts("rgb_points"),
                        pts("red_points"),
                        pts("green_points"),
                        pts("blue_points"),
                    ],
                }
            }
            // Radius in pixels (measured: radius 100 spreads a square ~100 px each way).
            GAUSSIAN => Effect::GaussianBlur {
                radius: num("gaussian_blur_radius", 1.0),
            },
            MOTION_BLUR => Effect::MotionBlur {
                distance: num("motion_blur_distance", 10.0),
                angle: num("motion_blur_angle", 0.0),
            },
            RADIAL_BLUR => Effect::RadialBlur {
                amount: num("radial_blur_amount", 5.0),
            },
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
                knock_out: truthy("ShadowKnockOut")
                    || p.get("ShowObject").is_some_and(|v| v == "false") && name.contains("Knock"),
            },
            // Measured: the glow's width is GlowWidth (0 with no softness = no glow), colour OuterBevelColor,
            // softness MaskSoftness, gap GlowStartDistance, opacity BevelContrast.
            BEVEL_GLOW if num("BevelType", 0.0) == 4.0 => Effect::Glow {
                color: color("OuterBevelColor", Rgba::BLACK),
                width: num("GlowWidth", 0.0),
                opacity: num("BevelContrast", 100.0) / 100.0,
                softness: num("MaskSoftness", 0.0),
                offset: num("GlowStartDistance", 0.0),
                inner: false,
            },
            BEVEL_GLOW => Effect::Bevel(crate::bevel::Bevel::parse(p)?),
            crate::plugins::BEVEL_BOSS => {
                Effect::BevelBoss(crate::plugins::BevelBoss::parse(&crate::plugins::Params(p)))
            }
            crate::plugins::MOTION_TRAIL => {
                Effect::MotionTrail(crate::plugins::MotionTrail::parse(&crate::plugins::Params(p)))
            }
            crate::plugins::EDGES => Effect::Edges(crate::plugins::Edges::parse(&crate::plugins::Params(p))),
            crate::plugins::MARBLE => {
                Effect::Marble(crate::plugins::Marble::parse(&crate::plugins::Params(p)))
            }
            INNER_SHADOW => Effect::Shadow {
                color: color("ShadowColor", Rgba::new(0, 0, 0, 165)),
                distance: num("ShadowDistance", 7.0),
                angle: num("ShadowAngle", 315.0),
                softness: num("ShadowBlur", 4.0),
                inner: true,
                knock_out: false,
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
        let region = crate::filters::alpha_bounds(img).unwrap_or((0, 0, 0, 0));
        self.apply_in(img, math, region);
    }

    /// Apply the effect, `region` being the object's pixel rectangle in `img`: the classic
    /// filters (Blur, Sharpen, Find Edges, Unsharp Mask) work within it and repeat its edges.
    pub fn apply_in(&self, img: &mut RgbaImage, math: crate::ColorMath, region: crate::filters::Region) {
        match self {
            Effect::HueSaturation {
                hue,
                saturation,
                lightness,
                colorize: false,
            } if math == crate::ColorMath::Precise => {
                for p in img.pixels_mut() {
                    if p.a != 0 {
                        let c = [p.r, p.g, p.b].map(f64::from);
                        let [r, g, b] = crate::colormath::hue_saturation(
                            c,
                            *hue as f64,
                            *saturation as f64,
                            *lightness as f64,
                        )
                        .map(|v| v.round().clamp(0.0, 255.0) as u8);
                        *p = Rgba::new(r, g, b, p.a);
                    }
                }
            }
            Effect::HueSaturation {
                hue,
                saturation,
                lightness,
                colorize: false,
            } => {
                // Bit-exact Fireworks MX (byte-quantized HSL, see fwhsl).
                for p in img.pixels_mut() {
                    if p.a != 0 {
                        let [r, g, b] = crate::fwhsl::hue_saturation(
                            p.r,
                            p.g,
                            p.b,
                            hue.round() as i32,
                            *saturation as f64,
                            *lightness as f64,
                        );
                        *p = Rgba::new(r, g, b, p.a);
                    }
                }
            }
            Effect::HueSaturation {
                hue,
                saturation,
                lightness,
                colorize: true,
            } => map_rgb(img, |c| hue_saturation(c, *hue, *saturation, *lightness, true)),
            Effect::BrightnessContrast { brightness, contrast } => {
                let (b, c) = (*brightness, *contrast);
                map_rgb(img, |v| v.map(|x| brightness_contrast(x, b, c)))
            }
            Effect::Levels { channels } => {
                let ch = channels.map(|[il, ih, ol, oh, g]| crate::filters::LevelsChannel {
                    source_low: il as i32,
                    source_high: ih as i32,
                    dest_low: ol as i32,
                    dest_high: oh as i32,
                    gamma: g as f64,
                });
                crate::filters::levels_tables(&ch).apply(img);
            }
            Effect::AutoLevels => {
                let ch = crate::filters::auto_levels(img);
                crate::filters::levels_tables(&ch).apply(img);
            }
            Effect::Curves { channels } => crate::filters::curves_tables(channels).apply(img),
            // Measured: Fireworks MX draws the inverted pixels over the original ones, so a
            // semi-transparent pixel gains alpha (60 becomes 106: 1 - (1 - a)^2).
            Effect::Invert => {
                let original = img.clone();
                crate::filters::Tables::invert().apply(img);
                for (p, o) in img.pixels_mut().iter_mut().zip(original.pixels()) {
                    if p.a < 255 && p.a > 0 {
                        *p = o.over(*p, 255);
                    }
                }
            }
            Effect::SimpleBlur { strength } => crate::filters::simple_blur(img, region, *strength),
            Effect::Sharpen { strength } => crate::filters::sharpen(img, region, *strength),
            Effect::FindEdges => crate::filters::find_edges(img, region),
            Effect::UnsharpMask {
                radius,
                amount,
                threshold,
            } => crate::filters::unsharp_mask(img, region, *radius, *amount, *threshold),
            Effect::Grayscale => {
                let lin = srgb_to_linear_table();
                map_rgb(img, |v| {
                    let at = |x: f32| lin.get(x as usize).copied().unwrap_or(0.0);
                    let y = perceptual_gray(at(v[0]), at(v[1]), at(v[2]));
                    [y; 3]
                })
            }
            Effect::ConvertToAlpha => {
                for p in img.pixels_mut() {
                    let luma = (p.r as u32 * 109 + p.g as u32 * 366 + p.b as u32 * 37) >> 9;
                    let a = (255 - luma) * p.a as u32 / 255;
                    *p = if a == 0 {
                        Rgba::TRANSPARENT
                    } else {
                        Rgba::new(0, 0, 0, a as u8)
                    };
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
            Effect::GaussianBlur { radius } => match gaussian_sigma(*radius) {
                Some(sigma) => gaussian_blur(img, sigma),
                // Above 200 Fireworks fills the object's rectangle with its average colour.
                None if *radius > 200.0 => average_colour(img),
                None => {}
            },
            Effect::MotionBlur { distance, angle } => motion_blur(img, *distance, *angle),
            Effect::RadialBlur { amount } => radial_blur(img, *amount),
            Effect::Shadow {
                color,
                distance,
                angle,
                softness,
                inner,
                knock_out,
            } => {
                let a = angle.to_radians();
                // Fireworks measures the angle counter-clockwise with y up.
                // The offset is truncated, not rounded (FirePixel's measurement).
                let (dx, dy) = (
                    (a.cos() * distance).trunc() as i64,
                    (-a.sin() * distance).trunc() as i64,
                );
                let radius = softness.round().max(0.0) as usize;
                let blurred = tent_plane(&alpha_plane(img), img.width(), img.height(), radius, math);
                let mut soft = shift_plane(&blurred, img.width(), img.height(), dx, dy, 0.0);
                if *inner {
                    soft.iter_mut().for_each(|x| *x = 255.0 - *x);
                }
                if *inner {
                    paint_inside(img, &soft, *color);
                } else {
                    paint_under(img, &soft, *color, *knock_out);
                }
            }
            Effect::Bevel(b) => b.apply_with(img, math),
            Effect::BevelBoss(b) => b.apply(img),
            Effect::MotionTrail(m) => m.apply(img),
            Effect::Edges(e) => e.apply(img),
            Effect::Marble(m) => m.apply(img),
            Effect::Glow {
                color,
                width,
                opacity,
                softness,
                offset,
                inner,
            } => {
                // A width of 0 still glows when softened: the object itself feeds the softening.
                if *width <= 0.0 && *softness <= 0.0 {
                    return;
                }
                let (w, h) = (img.width(), img.height());
                // Measured: a hard band of pixels whose Euclidean distance d to the object
                // (outside it, or to the outside for Inner Glow) satisfies offset < d <= offset + width.
                // For Glow every visible pixel is the object: a block of alpha 1 glows exactly as
                // an opaque one (oracle probe, docs/behavior/effects.md). Inner Glow keeps the
                // half-alpha edge (not measured on faint pixels yet).
                let inside: Vec<bool> = img
                    .pixels()
                    .iter()
                    .map(|p| if *inner { p.a < 128 } else { p.a > 0 })
                    .collect();
                let dist = distance_to(&inside, w, h);
                // With no gap the object's own pixels feed the softening, which keeps the glow's
                // strength at the edge (FirePixel's measurement), and the glow is painted under
                // them too: it shows through a semi-transparent object (measured: under an alpha-1
                // block the softened glow keeps rising inward, 144 to 159 at softness 12).
                let filled = !*inner && *offset == 0.0;
                // Measured on a Glow of width 5 at 65% (oracle probe): hard, its band is
                // anti-aliased, coverage clamp(width + 1 - d, 0, 1) (all 63 pixels around a
                // corner within 1 level); softened, it is a hard band one pixel wider
                // (d <= width + 1; softness 12 within 2 levels everywhere). Inner Glow keeps the
                // hard band of its width.
                let soft_glow = !*inner && *softness > 0.0;
                let reach = if soft_glow { *width + 1.0 } else { *width };
                let band: Vec<f32> = dist
                    .iter()
                    .map(|&d| {
                        if filled && d == 0.0 {
                            255.0
                        } else if d <= *offset {
                            0.0
                        } else if *inner || soft_glow {
                            if d <= offset + reach { 255.0 } else { 0.0 }
                        } else {
                            (offset + reach + 1.0 - d).clamp(0.0, 1.0) * 255.0
                        }
                    })
                    .collect();
                let mut soft = tent_plane(&band, w, h, softness.round().max(0.0) as usize, math);
                if *inner || math == crate::ColorMath::Precise {
                    // Precise colour math keeps the fractions; the result is rounded once, when
                    // it is painted.
                    soft.iter_mut().for_each(|v| *v *= opacity.clamp(0.0, 1.0));
                } else {
                    // Fireworks MX takes the opacity as a byte (65% is 165) and truncates the
                    // product: coverage 230 at 65% paints 148.
                    let ob = (opacity.clamp(0.0, 1.0) * 255.0).floor();
                    soft.iter_mut().for_each(|v| *v = (v.floor() * ob / 255.0).floor());
                }
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
            // Fireworks' blurred output reaches ceil(2.5 sigma) beyond the object.
            Effect::GaussianBlur { radius } => gaussian_sigma(*radius).map_or(0.0, |s| (s * 2.5).ceil()),
            Effect::MotionBlur { distance, .. } => *distance,
            Effect::Shadow {
                distance, softness, ..
            } => distance + softness * 2.0,
            Effect::Glow {
                width,
                softness,
                offset,
                ..
            } => width + offset + softness,
            Effect::Bevel(b) => b.margin(),
            Effect::BevelBoss(b) => b.margin(),
            Effect::MotionTrail(m) => m.margin(),
            Effect::Edges(_) | Effect::Marble(_) => 0.0,
            _ => 0.0,
        };
        m.clamp(0.0, 500.0).ceil() as u32 + 2
    }
}

/// Euclidean distance from each pixel to the nearest `true` pixel (0 on them). Exact squared
/// distance transform (Felzenszwalb & Huttenlocher), separable, O(pixels).
/// Euclidean distance to the nearest set pixel (for other effect modules).
pub(crate) fn distance_to_pub(set: &[bool], w: u32, h: u32) -> Vec<f32> {
    distance_to(set, w, h)
}

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

/// Brightness/Contrast on one channel, as Fireworks MX does it (measured on a grey ramp at 14
/// settings: every value within one level, 90.8% exact): brightness is added and clamped first;
/// contrast then scales around 128 by 100 / (100 − c) (c > 0; 50 doubles, 75 quadruples) or
/// (100 + c) / 100 (c < 0; −100 gives flat grey 128), and 100 is a hard threshold at 128.
/// (Fireworks MX's 99 is gentler than the formula: a slope of about 11, not 100.)
fn brightness_contrast(x: f32, brightness: f32, contrast: f32) -> f32 {
    let v = (x + brightness).clamp(0.0, 255.0);
    if contrast >= 100.0 {
        return if v >= 128.0 { 255.0 } else { 0.0 };
    }
    let k = if contrast > 0.0 {
        100.0 / (100.0 - contrast)
    } else {
        (100.0 + contrast.max(-100.0)) / 100.0
    };
    ((v - 128.0) * k + 128.0 + 0.5).floor().clamp(0.0, 255.0)
}

/// The 256 sRGB levels in linear light (IEC 61966-2-1).
fn srgb_to_linear_table() -> [f32; 256] {
    std::array::from_fn(|i| {
        let c = i as f64 / 255.0;
        (if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }) as f32
    })
}

/// The grey (0..255, sRGB) that looks as light as a color given in linear light: its luminance
/// Y = 0.2126 R + 0.7152 G + 0.0722 B (the sRGB / Rec. 709 primaries), encoded back to sRGB.
/// Weighting the gamma-encoded values instead (the usual shortcut, and Fireworks MX's
/// desaturation) makes saturated blues too dark and greens too light.
fn perceptual_gray(r: f32, g: f32, b: f32) -> f32 {
    let y = (0.2126 * f64::from(r) + 0.7152 * f64::from(g) + 0.0722 * f64::from(b)).clamp(0.0, 1.0);
    let e = if y <= 0.003_130_8 {
        12.92 * y
    } else {
        1.055 * y.powf(1.0 / 2.4) - 0.055
    };
    (e * 255.0) as f32
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

/// Gaussian Blur's sigma for a `gaussian_blur_radius`: the radius itself (FirePixel's
/// reconstruction, `fp-raster/src/filters/gauss.rs`; it also matches FireCraft's step-edge
/// measurements in Fireworks MX, which fitted 1.021 x radius); radii above 70 blur like 70. Radii
/// above 200 don't blur (`None`): the object becomes its average colour instead (see
/// [`average_colour`]; measured in Fireworks MX).
/// Fireworks' Gaussian Blur above radius 200: the object's pixel rectangle (the bounds of its
/// pixels with alpha > 0) is filled, fully opaque, with the plain average of every pixel in that
/// rectangle, transparent ones counting as white (Fireworks' empty buffer). Verified in Fireworks
/// MX: a red circle became a (255, 53, 53) square (circleBlur.png); the rule follows FirePixel's
/// reconstruction (`gauss.rs`, Apache-2.0).
pub fn average_colour(img: &mut RgbaImage) {
    let (w, h) = (img.width() as i64, img.height() as i64);
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, -1i64, -1i64);
    for y in 0..h {
        for x in 0..w {
            if img.get(x, y).is_some_and(|p| p.a > 0) {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
            }
        }
    }
    if x1 < x0 {
        // No visible pixels: the whole area (white when empty).
        (x0, y0, x1, y1) = (0, 0, w - 1, h - 1);
    }
    let mut sum = [0u64; 3];
    let mut n = 0u64;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let p = img.get(x, y).unwrap_or(Rgba::TRANSPARENT);
            let c = if p.a == 0 {
                [255, 255, 255]
            } else {
                [p.r, p.g, p.b]
            };
            for (s, v) in sum.iter_mut().zip(c) {
                *s += v as u64;
            }
            n += 1;
        }
    }
    if n == 0 {
        return;
    }
    let avg = |k: usize| (sum[k] / n).min(255) as u8;
    let fill = Rgba::new(avg(0), avg(1), avg(2), 255);
    for y in y0..=y1 {
        for x in x0..=x1 {
            img.set(x, y, fill);
        }
    }
}

pub fn gaussian_sigma(radius: f32) -> Option<f32> {
    if !(radius > 0.0 && radius <= 200.0) {
        return None;
    }
    Some(radius.clamp(0.1, 70.0))
}

/// Fireworks' Gaussian blur (adapted from FirePixel's `gaussian_blur_raw`, single precision,
/// Apache-2.0): a separable kernel of half-width ceil(3 sigma), weights
/// exp(-x^2 / 2 sigma^2) / sqrt(2 pi sigma) computed in `f32`; horizontal pass, then vertical,
/// each rounding down to 8 bits. Per output pixel the window is cut at the image edge (weights
/// renormalized); alpha is the weighted mean over the window, colour the weighted mean over
/// the window's pixels with alpha above 0 (not alpha-weighted).
pub fn gaussian_blur(img: &mut RgbaImage, sigma: f32) {
    let (w, h) = (img.width() as usize, img.height() as usize);
    if w == 0 || h == 0 || !(sigma > 0.0) {
        return;
    }
    let s = sigma as f64;
    let f = |v: f64| v as f32 as f64;
    let n = (s * 3.0).ceil().max(0.0) as usize;
    let two = f(f(s + s) * s);
    let inv = f(1.0 / f(s * std::f64::consts::TAU));
    let norm = inv.powf(0.5);
    let mut kernel: Vec<f32> = (0..=n)
        .map(|i| {
            let x = i as i64 - n as i64;
            ((f(-((x * x) as f64)) / two).exp() * norm) as f32
        })
        .collect();
    let tail: Vec<f32> = kernel.iter().rev().skip(1).copied().collect();
    kernel.extend(tail);
    let pass = |src: &[[u8; 4]], dst: &mut [[u8; 4]]| {
        let len = src.len();
        for (t, d) in dst.iter_mut().enumerate() {
            let (lo, hi) = (t.saturating_sub(n), (len - 1).min(t + n));
            let (mut sw, mut sa, mut swc) = (0.0f32, 0.0f32, 0.0f32);
            let mut sc = [0.0f32; 3];
            for (j, p) in src.iter().enumerate().take(hi + 1).skip(lo) {
                let k = kernel.get(j + n - t).copied().unwrap_or(0.0);
                if p[3] != 0 {
                    for (c, acc) in sc.iter_mut().enumerate() {
                        *acc += p[c] as f32 * k;
                    }
                    swc += k;
                }
                sa += p[3] as f32 * k;
                sw += k;
            }
            let fl = |v: f32| v.floor().clamp(0.0, 255.0) as u8;
            let col = |c: usize| if swc != 0.0 { fl(sc[c] / swc) } else { 0 };
            *d = [col(0), col(1), col(2), if sw != 0.0 { fl(sa / sw) } else { 0 }];
        }
    };
    let src: Vec<[u8; 4]> = img.pixels().iter().map(|p| [p.r, p.g, p.b, p.a]).collect();
    // Horizontal pass, row by row.
    let mut tmp = vec![[0u8; 4]; w * h];
    for (row_in, row_out) in src.chunks(w).zip(tmp.chunks_mut(w)) {
        pass(row_in, row_out);
    }
    // Vertical pass, column by column.
    let mut col_in = vec![[0u8; 4]; h];
    let mut col_out = vec![[0u8; 4]; h];
    let px = img.pixels_mut();
    for x in 0..w {
        for (y, c) in col_in.iter_mut().enumerate() {
            *c = tmp[y * w + x];
        }
        pass(&col_in, &mut col_out);
        for (y, c) in col_out.iter().enumerate() {
            if let Some(p) = px.get_mut(y * w + x) {
                *p = if c[3] == 0 {
                    Rgba::TRANSPARENT
                } else {
                    Rgba::new(c[0], c[1], c[2], c[3])
                };
            }
        }
    }
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
        s = if k >= 0.0 {
            s + (1.0 - s) * k
        } else {
            s * (1.0 + k)
        }
        .clamp(0.0, 1.0);
    }
    l = if light >= 0.0 {
        l + (1.0 - l) * light / 100.0
    } else {
        l * (1.0 + light / 100.0)
    };
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
    let s = if l <= 0.5 {
        d / (max + min)
    } else {
        d / (2.0 - max - min)
    };
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
                if let (Some(o), Some(&v)) =
                    (out.get_mut((y * w + x) as usize), a.get((sy * w + sx) as usize))
                {
                    *o = v;
                }
            }
        }
    }
    out
}

/// Gaussian blur of a single plane with standard deviation `sigma`, as three box passes (box
/// radius r gives sigma ~ r + 0.5).
/// Fireworks' softness: a triangle filter of radius `r` along each axis (FirePixel's measurement,
/// `fp-raster/src/mask.rs`). Fireworks MX's colour math truncates to whole levels after each pass;
/// FireCraft's keeps the fractions. Outside the image counts as 0.
pub(crate) fn tent_plane(a: &[f32], w: u32, h: u32, r: usize, math: crate::ColorMath) -> Vec<f32> {
    let (w, h) = (w as usize, h as usize);
    if r == 0 || w == 0 || h == 0 {
        return a.to_vec();
    }
    let classic = math == crate::ColorMath::Classic;
    let denominator = ((r + 1) * (r + 1)) as f64;
    let along = |line: &[f64], out: &mut [f64]| {
        let n = line.len();
        let mut first = vec![0f64; n + 1];
        for i in 0..n {
            first[i + 1] = first[i] + line[i];
        }
        let mut second = vec![0f64; n + 1];
        for i in 0..n {
            let boxed = first[i + 1] - first[i.saturating_sub(r)];
            second[i + 1] = second[i] + boxed;
        }
        for i in 0..n {
            let value = (second[(i + r + 1).min(n)] - second[i]) / denominator;
            out[i] = if classic { value.floor() } else { value };
        }
    };
    let mut v: Vec<f64> = a
        .iter()
        .map(|x| {
            let x = f64::from(x.clamp(0.0, 255.0));
            if classic { x.round() } else { x }
        })
        .collect();
    let mut out = vec![0f64; w.max(h)];
    for row in v.chunks_mut(w) {
        along(row, &mut out[..w]);
        row.copy_from_slice(&out[..w]);
    }
    let mut column = vec![0f64; h];
    for x in 0..w {
        for y in 0..h {
            column[y] = v[y * w + x];
        }
        along(&column, &mut out[..h]);
        for y in 0..h {
            v[y * w + x] = out[y];
        }
    }
    v.into_iter().map(|x| x.clamp(0.0, 255.0) as f32).collect()
}

/// Composite a coloured plane underneath the object's pixels.
fn paint_under(img: &mut RgbaImage, plane: &[f32], color: Rgba, knock_out: bool) {
    for (p, &a) in img.pixels_mut().iter_mut().zip(plane) {
        let sa = (a * color.a as f32 / 255.0).round().clamp(0.0, 255.0) as u8;
        let under = Rgba { a: sa, ..color };
        *p = if knock_out {
            // Only the shadow, cut out where the object is.
            Rgba {
                a: (sa as u32 * (255 - p.a as u32) / 255) as u8,
                ..color
            }
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
                let p = src
                    .get(x + (ca * t).round() as i64, y + (sa * t).round() as i64)
                    .unwrap_or_default();
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
                Rgba::new(
                    (acc[0] / acc[3]) as u8,
                    (acc[1] / acc[3]) as u8,
                    (acc[2] / acc[3]) as u8,
                    (acc[3] / n) as u8,
                )
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
                let p = src
                    .get(
                        (cx + dx * c - dy * s).round() as i64,
                        (cy + dx * s + dy * c).round() as i64,
                    )
                    .unwrap_or_default();
                let a = p.a as f32;
                acc[0] += p.r as f32 * a;
                acc[1] += p.g as f32 * a;
                acc[2] += p.b as f32 * a;
                acc[3] += a;
            }
            let out = if acc[3] <= 0.0 {
                Rgba::TRANSPARENT
            } else {
                Rgba::new(
                    (acc[0] / acc[3]) as u8,
                    (acc[1] / acc[3]) as u8,
                    (acc[2] / acc[3]) as u8,
                    (acc[3] / n as f32) as u8,
                )
            };
            img.set(x, y, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_fireworks_mx_math_truncates_the_softness_filter() {
        let plane = vec![101.0f32; 81];
        let classic = tent_plane(&plane, 9, 9, 4, crate::ColorMath::Classic);
        let precise = tent_plane(&plane, 9, 9, 4, crate::ColorMath::Precise);
        assert!(classic.iter().all(|v| v.fract() == 0.0), "whole levels");
        assert!(precise.iter().any(|v| v.fract() != 0.0), "keeps fractions");
        assert!(
            classic.iter().zip(&precise).all(|(c, p)| c <= p),
            "truncation only lowers"
        );
    }

    #[test]
    fn inner_shadow_softness_matches_fireworks_mx_exactly() {
        // innerShadowZero.png: #33FFFF, black at 65%, distance 0, softness 4: green along the edge.
        let mut img = RgbaImage::new(60, 40, Rgba::TRANSPARENT);
        for y in 10..30 {
            for x in 10..50 {
                img.set(x, y, Rgba::new(51, 255, 255, 255));
            }
        }
        Effect::Shadow {
            color: Rgba::new(0, 0, 0, 166),
            distance: 0.0,
            angle: 315.0,
            softness: 4.0,
            inner: true,
            knock_out: false,
        }
        .apply(&mut img);
        let green: Vec<u8> = (10..15).filter_map(|x| img.get(x, 20)).map(|p| p.g).collect();
        assert_eq!(green, [189, 215, 235, 248, 255]);
    }

    #[test]
    fn a_zero_width_glow_still_shows_its_softness() {
        let square = || {
            let mut img = RgbaImage::new(80, 80, Rgba::TRANSPARENT);
            for y in 30..50 {
                for x in 30..50 {
                    img.set(x, y, Rgba::new(10, 20, 200, 255));
                }
            }
            img
        };
        let glow = |softness| Effect::Glow {
            color: Rgba::new(255, 0, 0, 255),
            width: 0.0,
            opacity: 1.0,
            softness,
            offset: 0.0,
            inner: false,
        };
        let mut soft = square();
        glow(7.0).apply(&mut soft);
        assert!(soft.get(28, 40).is_some_and(|p| p.a > 40), "softness shows");
        let mut hard = square();
        glow(0.0).apply(&mut hard);
        assert_eq!(hard, square(), "no width and no softness: no glow");
    }

    #[test]
    fn a_softened_glow_keeps_its_strength_at_the_edge() {
        let mut img = RgbaImage::new(120, 120, Rgba::TRANSPARENT);
        for y in 30..90 {
            for x in 30..90 {
                img.set(x, y, Rgba::new(10, 20, 200, 128));
            }
        }
        Effect::Glow {
            color: Rgba::new(255, 0, 0, 255),
            width: 5.0,
            opacity: 1.0,
            softness: 12.0,
            offset: 0.0,
            inner: false,
        }
        .apply(&mut img);
        let edge = img.get(29, 60).expect("a pixel");
        assert!(edge.a > 150, "the glow next to the object: {edge:?}");
        // Painted under a semi-transparent object too (measured in Fireworks MX).
        let under = img.get(60, 60).expect("a pixel");
        assert!(under.a == 255 && under.r > 100, "the glow shows through: {under:?}");
    }

    /// Fireworks MX's Glow, width 5 at 65%, measured with the oracle on blocks of white at
    /// alpha 1..255 (docs/behavior/effects.md).
    fn measured_glow(softness: f32) -> Effect {
        Effect::Glow {
            color: Rgba::new(0xdf, 0, 0, 255),
            width: 5.0,
            opacity: 0.65,
            softness,
            offset: 0.0,
            inner: false,
        }
    }

    fn block(alpha: u8) -> RgbaImage {
        let mut img = RgbaImage::new(60, 60, Rgba::TRANSPARENT);
        for y in 20..40 {
            for x in 20..40 {
                img.set(x, y, Rgba::new(255, 255, 255, alpha));
            }
        }
        img
    }

    #[test]
    fn grayscale_keeps_perceived_lightness() {
        let gray = |c: [u8; 3]| {
            let mut img = RgbaImage::new(1, 1, Rgba::new(c[0], c[1], c[2], 255));
            Effect::Grayscale.apply(&mut img);
            let p = img.get(0, 0).expect("a pixel");
            assert!(p.r == p.g && p.g == p.b, "{p:?}");
            p.r
        };
        // Greys stay as they are; white and black too.
        for v in [0u8, 1, 50, 128, 200, 255] {
            assert_eq!(gray([v, v, v]), v);
        }
        // Luminance of the primaries (Y 0.2126, 0.7152, 0.0722) encoded as sRGB.
        assert_eq!(gray([255, 0, 0]), 127);
        assert_eq!(gray([0, 255, 0]), 220);
        assert_eq!(gray([0, 0, 255]), 76);
        let p = params(&[
            ("EffectMoaID", HUE_SAT),
            ("saturation_amount", "-100"),
            (FIRECRAFT_EFFECT, "grayscale"),
        ]);
        let plain = Effect::HueSaturation {
            hue: 0.0,
            saturation: -100.0,
            lightness: 0.0,
            colorize: false,
        };
        assert_eq!(Effect::parse(&p, &HashMap::new()), Some(plain), "no Grayscale name");
        let mine = params(&[
            ("EffectMoaID", HUE_SAT),
            ("name", "Grayscale"),
            ("saturation_amount", "-100"),
            (FIRECRAFT_EFFECT, "grayscale"),
        ]);
        assert_eq!(Effect::parse(&mine, &HashMap::new()), Some(Effect::Grayscale));
        // Edited in Fireworks MX (minions_banana2_gray.png): renamed, new saturation, marker kept.
        let edited = params(&[
            ("EffectMoaID", HUE_SAT),
            ("name", "Hue/Saturation..."),
            ("saturation_amount", "-60"),
            (FIRECRAFT_EFFECT, "grayscale"),
        ]);
        assert!(matches!(
            Effect::parse(&edited, &HashMap::new()),
            Some(Effect::HueSaturation { saturation, .. }) if saturation == -60.0
        ));
    }

    #[test]
    fn brightness_contrast_matches_fireworks_mx() {
        // Fireworks MX's output for inputs 0, 64, 112, 127, 128, 144, 192, 255 (measured).
        let cases: [(f32, f32, [f32; 8]); 6] = [
            (81.0, 0.0, [81.0, 145.0, 193.0, 208.0, 209.0, 225.0, 255.0, 255.0]),
            (-50.0, 0.0, [0.0, 14.0, 62.0, 77.0, 78.0, 94.0, 142.0, 205.0]),
            (0.0, 50.0, [0.0, 0.0, 96.0, 126.0, 128.0, 160.0, 255.0, 255.0]),
            (0.0, 75.0, [0.0, 0.0, 64.0, 124.0, 128.0, 192.0, 255.0, 255.0]),
            (0.0, 100.0, [0.0, 0.0, 0.0, 0.0, 255.0, 255.0, 255.0, 255.0]),
            (81.0, 100.0, [0.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0, 255.0]),
        ];
        for (b, c, want) in cases {
            for (x, w) in [0.0, 64.0, 112.0, 127.0, 128.0, 144.0, 192.0, 255.0].iter().zip(want) {
                let got = brightness_contrast(*x, b, c);
                assert!((got - w).abs() <= 1.0, "b {b} c {c} x {x}: {got} vs Fireworks MX {w}");
            }
        }
    }

    #[test]
    fn a_faint_object_glows_like_an_opaque_one() {
        let (mut faint, mut opaque) = (block(1), block(255));
        measured_glow(0.0).apply(&mut faint);
        measured_glow(0.0).apply(&mut opaque);
        let alphas = |img: &RgbaImage| (10..20).map(|x| img.get(x, 30).map_or(0, |p| p.a)).collect::<Vec<_>>();
        // Five pixels at 65% (truncated: 165), as Fireworks MX paints them.
        assert_eq!(alphas(&opaque), [0, 0, 0, 0, 0, 165, 165, 165, 165, 165]);
        assert_eq!(alphas(&faint), alphas(&opaque));
        // And under the faint block (Fireworks MX: df0101 at 165).
        assert_eq!(faint.get(25, 30), Some(Rgba::new(0xdf, 1, 1, 165)));
    }

    #[test]
    fn precise_colour_math_rounds_the_glow_once() {
        let (mut classic, mut precise) = (block(255), block(255));
        measured_glow(0.0).apply_with(&mut classic, crate::ColorMath::Classic);
        measured_glow(0.0).apply_with(&mut precise, crate::ColorMath::Precise);
        // 65% of 255 is 165.75: Fireworks MX truncates, Precise rounds.
        assert_eq!(classic.get(18, 30).map(|p| p.a), Some(165));
        assert_eq!(precise.get(18, 30).map(|p| p.a), Some(166));
        // Corner coverage 0.901 (229.75 of 255) at 65%: Fireworks MX truncates twice to 148;
        // Precise keeps 149.34 and rounds it to 149.
        assert_eq!(classic.get(19, 15).map(|p| p.a), Some(148));
        assert_eq!(precise.get(19, 15).map(|p| p.a), Some(149));
    }

    #[test]
    fn a_hard_glow_is_anti_aliased_round_corners() {
        let mut img = block(255);
        measured_glow(0.0).apply(&mut img);
        // Fireworks MX: 1 across and 5 up from the corner (d = 5.10) is 148; (4, 4) is 56.
        assert_eq!(img.get(19, 15).map(|p| p.a), Some(148));
        assert_eq!(img.get(16, 16).map(|p| p.a), Some(56));
    }

    #[test]
    fn a_softened_glow_falls_off_as_in_fireworks() {
        let mut img = RgbaImage::new(80, 60, Rgba::TRANSPARENT);
        for y in 0..60 {
            for x in 40..80 {
                img.set(x, y, Rgba::new(255, 255, 255, 255));
            }
        }
        measured_glow(12.0).apply_with(&mut img, crate::ColorMath::Classic);
        // Fireworks MX, the 24 pixels left of an opaque edge.
        let mx = [
            0, 0, 0, 0, 0, 0, 0, 2, 5, 9, 14, 20, 27, 35, 43, 53, 64, 76, 88, 100, 111, 121, 129, 137,
        ];
        for (x, want) in (16..40).zip(mx) {
            let got = i32::from(img.get(x, 30).map_or(0, |p| p.a));
            assert!((got - want).abs() <= 2, "x {x}: {got} vs Fireworks MX {want}");
        }
    }

    #[test]
    fn only_effects_that_ignore_neighbouring_pixels_are_scale_free() {
        assert!(Effect::Invert.is_scale_free());
        assert!(!Effect::GaussianBlur { radius: 2.0 }.is_scale_free());
        assert_eq!(
            Effect::GaussianBlur { radius: 2.0 }.scaled(4.0),
            Some(Effect::GaussianBlur { radius: 8.0 })
        );
    }

    fn params(kv: &[(&str, &str)]) -> HashMap<String, String> {
        kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    #[test]
    fn parses_known_effects() {
        let e = Effect::parse(
            &params(&[("EffectMoaID", GAUSSIAN), ("gaussian_blur_radius", "100")]),
            &HashMap::new(),
        );
        assert_eq!(e, Some(Effect::GaussianBlur { radius: 100.0 }));
        assert_eq!(gaussian_sigma(100.0), gaussian_sigma(70.0));
        assert_eq!(gaussian_sigma(201.0), None);
        // Above 200: the object's rectangle filled opaque with the plain average of its pixels.
        let mut img = RgbaImage::new(4, 1, Rgba::TRANSPARENT);
        img.set(0, 0, Rgba::new(0, 0, 0, 255));
        img.set(1, 0, Rgba::new(200, 100, 50, 255));
        img.set(2, 0, Rgba::new(100, 100, 100, 128));
        Effect::GaussianBlur { radius: 250.0 }.apply_with(&mut img, crate::ColorMath::Classic);
        assert_eq!(img.get(0, 0), Some(Rgba::new(100, 66, 50, 255)));
        assert_eq!(img.get(2, 0), Some(Rgba::new(100, 66, 50, 255)));
        assert_eq!(img.get(3, 0), Some(Rgba::TRANSPARENT));
        // Transparent pixels inside the rectangle count as white (a red circle: (255, 53, 53)).
        let mut img = RgbaImage::new(3, 1, Rgba::TRANSPARENT);
        img.set(0, 0, Rgba::new(255, 0, 0, 255));
        img.set(2, 0, Rgba::new(255, 0, 0, 255));
        Effect::GaussianBlur { radius: 250.0 }.apply_with(&mut img, crate::ColorMath::Classic);
        assert_eq!(img.get(1, 0), Some(Rgba::new(255, 85, 85, 255)));
        let e = Effect::parse(
            &params(&[("EffectMoaID", BEVEL_GLOW), ("BevelType", "0")]),
            &HashMap::new(),
        );
        assert!(matches!(e, Some(Effect::Bevel(_))));
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
        Effect::Shadow {
            color: Rgba::new(0, 0, 0, 165),
            distance: 5.0,
            angle: 315.0,
            softness: 2.0,
            inner: false,
            knock_out: false,
        }
        .apply(&mut s);
        assert!(s.get(26, 26).is_some_and(|p| p.a > 50), "shadow down-right");
        assert_eq!(s.get(20, 20), Some(Rgba::new(255, 0, 0, 255)), "object on top");
        let mut g = img.clone();
        Effect::Glow {
            color: Rgba::WHITE,
            width: 4.0,
            opacity: 1.0,
            softness: 0.0,
            offset: 0.0,
            inner: false,
        }
        .apply(&mut g);
        assert!(
            g.get(12, 20)
                .is_some_and(|p| p.a == 255 && p.r == 255 && p.g == 255)
        );
        assert!(
            g.get(10, 20).is_some_and(|p| p.a == 0),
            "width 4 stops at distance 4"
        );
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
            Effect::HueSaturation {
                hue: h,
                saturation: s,
                lightness: l,
                colorize: false,
            }
            .apply(&mut img);
            let p = img.get(0, 0).unwrap();
            let close = [p.r, p.g, p.b].iter().zip(want).all(|(a, b)| a.abs_diff(b) <= 2);
            assert!(close, "{input:?} {h},{s},{l}: got {p:?}, want {want:?}");
        }
    }

    #[test]
    fn hue_shift_rotates_colour() {
        let mut img = RgbaImage::new(1, 1, Rgba::new(255, 0, 0, 255));
        Effect::HueSaturation {
            hue: 120.0,
            saturation: 0.0,
            lightness: 0.0,
            colorize: false,
        }
        .apply(&mut img);
        let p = img.get(0, 0).unwrap();
        assert!(p.g > 250 && p.r < 5, "{p:?}");
    }
}
