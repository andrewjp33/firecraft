//! Fireworks blend modes, measured against Fireworks MX (see docs/behavior/blend-modes.md).
//!
//! Colour modes work in HSV (not Photoshop's luminosity model): Hue/Saturation/Color keep the
//! base colour's HSV value; Luminosity sets the HSV value to the blend colour's luma. Tint and the
//! luma weights come from Fireworks.exe itself (blend function table at 0x56d9c4).

use crate::colormath::{self, ColorMath};
use crate::fwhsl;
use crate::image::Rgba;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Darken,
    Lighten,
    Difference,
    Hue,
    Saturation,
    Color,
    Luminosity,
    Invert,
    Tint,
    Erase,
}

impl BlendMode {
    pub const ALL: [BlendMode; 13] = [
        BlendMode::Normal,
        BlendMode::Multiply,
        BlendMode::Screen,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::Difference,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
        BlendMode::Invert,
        BlendMode::Tint,
        BlendMode::Erase,
    ];

    /// The `BLDi` value Fireworks stores (Normal is stored as an absent tag).
    pub fn fw_code(self) -> i64 {
        match self {
            BlendMode::Normal => 0,
            BlendMode::Multiply => 2,
            BlendMode::Difference => 3,
            BlendMode::Screen => 5,
            BlendMode::Darken => 9,
            BlendMode::Lighten => 10,
            BlendMode::Hue => 11,
            BlendMode::Saturation => 12,
            BlendMode::Color => 13,
            BlendMode::Luminosity => 14,
            BlendMode::Invert => 17,
            BlendMode::Erase => 25,
            BlendMode::Tint => 29,
        }
    }

    /// From a `BLDi` value; unknown values fall back to Normal.
    pub fn from_fw_code(v: i64) -> Self {
        Self::ALL
            .into_iter()
            .find(|m| m.fw_code() == v)
            .unwrap_or_default()
    }

    /// Name as shown in the Property inspector.
    pub fn label(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Multiply => "Multiply",
            BlendMode::Screen => "Screen",
            BlendMode::Darken => "Darken",
            BlendMode::Lighten => "Lighten",
            BlendMode::Difference => "Difference",
            BlendMode::Hue => "Hue",
            BlendMode::Saturation => "Saturation",
            BlendMode::Color => "Color",
            BlendMode::Luminosity => "Luminosity",
            BlendMode::Invert => "Invert",
            BlendMode::Tint => "Tint",
            BlendMode::Erase => "Erase",
        }
    }

    /// Composite `top` (straight alpha) onto `base` with extra `opacity` (0..=255), bit-exact
    /// Fireworks MX.
    pub fn composite(self, base: Rgba, top: Rgba, opacity: u8) -> Rgba {
        self.composite_with(base, top, opacity, ColorMath::Classic)
    }

    /// [`composite`](Self::composite) with a choice of colour-math precision.
    pub fn composite_with(self, base: Rgba, top: Rgba, opacity: u8, math: ColorMath) -> Rgba {
        let sa = top.a as f64 * opacity as f64 / (255.0 * 255.0);
        if sa <= 0.0 {
            return base;
        }
        if self == BlendMode::Normal {
            return base.over(top, opacity);
        }
        if self == BlendMode::Erase {
            let a = (base.a as f64 * (1.0 - sa)).round() as u8;
            return if a == 0 {
                Rgba::TRANSPARENT
            } else {
                Rgba { a, ..base }
            };
        }
        let ba = base.a as f64 / 255.0;
        let cb = [base.r as f64, base.g as f64, base.b as f64];
        let cs = [top.r as f64, top.g as f64, top.b as f64];
        let mixed = match math {
            ColorMath::Classic => self.mix(cb, cs),
            ColorMath::Precise => self.mix_precise(cb, cs),
        };
        // W3C compositing: where the base is transparent the source shows unblended.
        let out_a = sa + ba * (1.0 - sa);
        if out_a <= 0.0 {
            return Rgba::TRANSPARENT;
        }
        let ch = |i: usize| {
            let src = (1.0 - ba) * cs[i] + ba * mixed[i];
            ((src * sa + cb[i] * ba * (1.0 - sa)) / out_a)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        Rgba::new(ch(0), ch(1), ch(2), (out_a * 255.0).round() as u8)
    }

    /// The blend function B(base, blend) on opaque colours (0..=255 per channel).
    fn mix(self, b: [f64; 3], s: [f64; 3]) -> [f64; 3] {
        let per = |f: fn(f64, f64) -> f64| [f(b[0], s[0]), f(b[1], s[1]), f(b[2], s[2])];
        match self {
            BlendMode::Normal | BlendMode::Erase => s,
            BlendMode::Multiply => per(|b, s| b * s / 255.0),
            BlendMode::Screen => per(|b, s| 255.0 - (255.0 - b) * (255.0 - s) / 255.0),
            BlendMode::Darken => per(f64::min),
            BlendMode::Lighten => per(f64::max),
            BlendMode::Difference => per(|b, s| (b - s).abs()),
            BlendMode::Invert => per(|b, _| 255.0 - b),
            BlendMode::Hue => {
                // Bit-exact: the blend colour's hue with the base's lightness and saturation, in
                // Fireworks' byte HSL (blend function 0x56d270).
                let (bb, ss) = (byte(b), byte(s));
                let (sh, _, _) = fwhsl::to_hsl(ss[0], ss[1], ss[2]);
                let (_, bl, bs) = fwhsl::to_hsl(bb[0], bb[1], bb[2]);
                rgb(fwhsl::from_hsl(sh, bl, bs))
            }
            // Bit-exact: Fireworks' integer HSV (fwhsl::to_hsv / from_hsv).
            BlendMode::Saturation => {
                let (bh, bs, bv) = hsv(b);
                let (_, ss, _) = hsv(s);
                // A grey base has no hue to saturate: unchanged.
                if bs == 0 {
                    b
                } else {
                    rgb(fwhsl::from_hsv(bh, ss, bv))
                }
            }
            BlendMode::Color => {
                let (_, _, bv) = hsv(b);
                let (sh, ss, _) = hsv(s);
                rgb(fwhsl::from_hsv(sh, ss, bv))
            }
            BlendMode::Luminosity => {
                let (bh, bs, _) = hsv(b);
                rgb(fwhsl::from_hsv(bh, bs, luma(s).clamp(0, 255) as u8))
            }
            BlendMode::Tint => {
                // Fireworks MX (blend function at 0x56d6b0): move the base towards the blend colour
                // by how dark the base is, in integer arithmetic with /255 rounding.
                let k = 255 - luma(b);
                let ch = |b: f64, s: f64| -> f64 {
                    let v = (s as i32 - b as i32) * k + b as i32 * 255 + 0x80;
                    (((v >> 8) + v) >> 8) as f64
                };
                [ch(b[0], s[0]), ch(b[1], s[1]), ch(b[2], s[2])]
            }
        }
    }

    /// The same blend functions in double precision (no byte quantization in between).
    fn mix_precise(self, b: [f64; 3], s: [f64; 3]) -> [f64; 3] {
        match self {
            BlendMode::Hue => {
                let (sh, _, _) = colormath::to_hsl(s);
                let (_, bs, bl) = colormath::to_hsl(b);
                colormath::from_hsl(sh, bs, bl)
            }
            BlendMode::Saturation => {
                let (bh, bs, bv) = colormath::to_hsv(b);
                let (_, ss, _) = colormath::to_hsv(s);
                if bs <= 0.0 {
                    b
                } else {
                    colormath::from_hsv(bh, ss, bv)
                }
            }
            BlendMode::Color => {
                let (_, _, bv) = colormath::to_hsv(b);
                let (sh, ss, _) = colormath::to_hsv(s);
                colormath::from_hsv(sh, ss, bv)
            }
            BlendMode::Luminosity => {
                let (bh, bs, _) = colormath::to_hsv(b);
                colormath::from_hsv(bh, bs, colormath::luma(s))
            }
            BlendMode::Tint => {
                let k = (255.0 - colormath::luma(b)) / 255.0;
                [0, 1, 2].map(|i| b[i] + (s[i] - b[i]) * k)
            }
            // The rest are already exact in floating point.
            _ => self.mix(b, s),
        }
    }
}

/// Fireworks' luma: Rec. 709 weights 0.2125/0.7154/0.0721 scaled by 512 (109/366/37), computed
/// once at startup in Fireworks MX and shared by the blend functions.
fn luma(c: [f64; 3]) -> i32 {
    (c[0] as i32 * 109 + c[1] as i32 * 366 + c[2] as i32 * 37) >> 9
}

fn byte(c: [f64; 3]) -> [u8; 3] {
    c.map(|v| v.round().clamp(0.0, 255.0) as u8)
}

fn hsv(c: [f64; 3]) -> (u8, u8, u8) {
    let [r, g, b] = byte(c);
    fwhsl::to_hsv(r, g, b)
}

fn rgb(c: [u8; 3]) -> [f64; 3] {
    c.map(f64::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn op(r: u8, g: u8, b: u8) -> Rgba {
        Rgba::new(r, g, b, 255)
    }

    // Values measured in Fireworks MX (docs/behavior/blend-modes.md).
    #[test]
    fn matches_fireworks_measurements() {
        let cases = [
            (
                BlendMode::Multiply,
                op(128, 128, 128),
                op(100, 200, 150),
                op(50, 100, 75),
            ),
            (BlendMode::Screen, op(200, 40, 40), op(255, 0, 0), op(255, 40, 40)),
            (
                BlendMode::Difference,
                op(128, 128, 128),
                op(100, 200, 150),
                op(28, 72, 22),
            ),
            (
                BlendMode::Color,
                op(128, 128, 128),
                op(100, 200, 150),
                op(64, 128, 96),
            ),
            (
                BlendMode::Saturation,
                op(40, 180, 60),
                op(255, 0, 0),
                op(0, 180, 26),
            ),
            (
                BlendMode::Saturation,
                op(128, 128, 128),
                op(255, 0, 0),
                op(128, 128, 128),
            ),
            (
                BlendMode::Luminosity,
                op(200, 40, 40),
                op(255, 0, 0),
                op(54, 10, 10),
            ),
            (
                BlendMode::Luminosity,
                op(128, 128, 128),
                op(240, 120, 20),
                op(138, 138, 138),
            ),
            (BlendMode::Invert, op(200, 40, 40), op(1, 2, 3), op(55, 215, 215)),
        ];
        for (mode, base, top, want) in cases {
            let got = mode.composite(base, top, 255);
            let close = |a: u8, b: u8| a.abs_diff(b) <= 1;
            assert!(
                close(got.r, want.r) && close(got.g, want.g) && close(got.b, want.b),
                "{mode:?}: {got:?} vs {want:?}"
            );
        }
        // Hue: bit-exact (byte HSL).
        assert_eq!(
            BlendMode::Hue.composite(op(40, 180, 60), op(255, 0, 0), 255),
            op(178, 39, 39)
        );
        assert_eq!(
            BlendMode::Erase.composite(op(1, 2, 3), op(9, 9, 9), 255),
            Rgba::TRANSPARENT
        );
        // Tint: exact (formula from Fireworks.exe, values measured in Fireworks MX).
        for (base, top, want) in [
            (op(200, 40, 40), op(255, 0, 0), op(239, 12, 12)),
            (op(40, 180, 60), op(255, 0, 0), op(136, 100, 33)),
            (op(250, 250, 250), op(255, 0, 0), op(250, 245, 245)),
            (op(220, 200, 40), op(60, 60, 60), op(180, 165, 45)),
        ] {
            assert_eq!(BlendMode::Tint.composite(base, top, 255), want);
        }
    }

    #[test]
    fn codes_round_trip() {
        for m in BlendMode::ALL {
            assert_eq!(BlendMode::from_fw_code(m.fw_code()), m);
        }
        assert_eq!(BlendMode::from_fw_code(13), BlendMode::Color);
        assert_eq!(BlendMode::from_fw_code(999), BlendMode::Normal);
    }

    #[test]
    fn transparent_base_shows_source() {
        let got = BlendMode::Multiply.composite(Rgba::TRANSPARENT, op(10, 20, 30), 255);
        assert_eq!(got, op(10, 20, 30));
    }
}
