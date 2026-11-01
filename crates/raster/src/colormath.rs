//! Colour math precision: Fireworks MX's exact (integer / byte-quantized) formulas, or the same
//! colour models in double precision.
//!
//! [`ColorMath::Classic`] reproduces Fireworks MX bit for bit (see `fwhsl`), including its
//! rounding quirks: hue in steps of ~1.4 degrees, saturation and lightness truncated to bytes.
//! [`ColorMath::Precise`] uses the same models and parameters (so a document looks the same) but
//! computes in `f64` and rounds once, when the result is stored as 8-bit RGBA. Images between
//! steps stay 8 bits per channel, like Fireworks.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ColorMath {
    /// Bit-exact Fireworks MX.
    #[default]
    Classic,
    /// Double-precision versions of the same formulas.
    Precise,
}

impl ColorMath {
    pub const ALL: [ColorMath; 2] = [ColorMath::Classic, ColorMath::Precise];

    pub fn label(self) -> &'static str {
        match self {
            ColorMath::Classic => "Fireworks MX",
            ColorMath::Precise => "High precision",
        }
    }
}

/// Rec. 709 luma weights (Fireworks' integer version: 109/366/37 over 512).
pub fn luma(c: [f64; 3]) -> f64 {
    0.2125 * c[0] + 0.7154 * c[1] + 0.0721 * c[2]
}

/// RGB (0..=255) -> (hue degrees 0..360, saturation 0..1, lightness 0..1).
pub fn to_hsl(c: [f64; 3]) -> (f64, f64, f64) {
    let [r, g, b] = c.map(|v| v / 255.0);
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mx + mn) / 2.0;
    let d = mx - mn;
    if d <= 0.0 {
        return (0.0, 0.0, l);
    }
    let s = if l <= 0.5 {
        d / (mx + mn)
    } else {
        d / (2.0 - mx - mn)
    };
    (hue_of(r, g, b, mx, d), s, l)
}

pub fn from_hsl(h: f64, s: f64, l: f64) -> [f64; 3] {
    if s <= 0.0 {
        return [l * 255.0; 3];
    }
    let m2 = if l <= 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let m1 = 2.0 * l - m2;
    let value = |hh: f64| {
        let hh = hh.rem_euclid(360.0);
        if hh < 60.0 {
            m1 + (m2 - m1) * hh / 60.0
        } else if hh < 180.0 {
            m2
        } else if hh < 240.0 {
            m1 + (m2 - m1) * (240.0 - hh) / 60.0
        } else {
            m1
        }
    };
    [value(h + 120.0), value(h), value(h - 120.0)].map(|v| v * 255.0)
}

/// RGB (0..=255) -> (hue degrees 0..360, saturation 0..1, value 0..=255).
pub fn to_hsv(c: [f64; 3]) -> (f64, f64, f64) {
    let [r, g, b] = c;
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let d = mx - mn;
    if mx <= 0.0 || d <= 0.0 {
        return (0.0, 0.0, mx);
    }
    (hue_of(r, g, b, mx, d), d / mx, mx)
}

pub fn from_hsv(h: f64, s: f64, v: f64) -> [f64; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let m = v - c;
    let [r, g, b] = match h as u32 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    [r + m, g + m, b + m]
}

fn hue_of(r: f64, g: f64, b: f64, mx: f64, d: f64) -> f64 {
    let h = if r == mx {
        (g - b) / d
    } else if g == mx {
        2.0 + (b - r) / d
    } else {
        4.0 + (r - g) / d
    };
    (h * 60.0).rem_euclid(360.0)
}

/// The Hue/Saturation effect (not colorize) in double precision: fractional hue degrees, no
/// byte steps. Same parameter meaning as Fireworks (`fwhsl::hue_saturation`).
pub fn hue_saturation(c: [f64; 3], hue: f64, sat: f64, light: f64) -> [f64; 3] {
    let (h, s, l) = to_hsl(c);
    let adjust = |x: f64, amount: f64| {
        let k = (amount / 100.0).clamp(-1.0, 1.0);
        if k >= 0.0 {
            x + (1.0 - x) * k
        } else {
            x * (1.0 + k)
        }
    };
    from_hsl(
        h + hue,
        adjust(s, sat).clamp(0.0, 1.0),
        adjust(l, light).clamp(0.0, 1.0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [f64; 3], b: [f64; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9)
    }

    #[test]
    fn round_trips_are_lossless() {
        for c in [
            [200.0, 40.0, 40.0],
            [40.0, 180.0, 60.0],
            [30.0, 60.0, 200.0],
            [128.0, 128.0, 128.0],
            [12.5, 200.25, 99.0],
        ] {
            let (h, s, l) = to_hsl(c);
            assert!(close(from_hsl(h, s, l), c), "hsl {c:?}");
            let (h, s, v) = to_hsv(c);
            assert!(close(from_hsv(h, s, v), c), "hsv {c:?}");
        }
    }

    #[test]
    fn precise_hue_shift_is_reversible() {
        let c = [200.0, 40.0, 40.0];
        let there = hue_saturation(c, -172.0, 0.0, 0.0);
        assert!(close(hue_saturation(there, 172.0, 0.0, 0.0), c));
        // Fireworks' byte hue can't do this: [40, 175, 199] shifted back is [199, 40, 44].
        let fw = crate::fwhsl::hue_saturation(40, 175, 199, 172, 0.0, 0.0);
        assert_ne!(fw, [200, 40, 40]);
    }
}
