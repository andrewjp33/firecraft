//! Fireworks MX's byte-quantized colour models (its function addresses for reference).
//!
//! HSL: RGB -> HSL at `0x564e70`, HSL -> RGB at `0x565110`, used by the Hue/Saturation effect
//! (`0x7d7433`) and the Hue blend mode (`0x56d270`).
//! HSV: integer RGB -> HSV at `0x565490`, HSV -> RGB at `0x565590` (hue sextants of 42/43),
//! used by the Saturation, Color and Luminosity blend modes (`0x56d370`, `0x56d480`, `0x56d580`).
//!
//! Each component is a byte, truncated (x87 `ftol`). Channels are scaled by the double constant
//! `1/255` (multiplication, not division: that matters for truncation), hue bytes are hue/360*255.
//! Verified bit-exact against 48 Hue/Saturation and 36 Hue-blend renders by Fireworks MX.

const INV_255: f64 = 0.003_921_568_627_450_98;

fn trunc_byte(x: f64) -> u8 {
    x.clamp(0.0, 255.0) as u8
}

/// RGB -> (hue, lightness, saturation) bytes.
pub fn to_hsl(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (rr, gg, bb) = (r as f64 * INV_255, g as f64 * INV_255, b as f64 * INV_255);
    let (mx, mn) = (rr.max(gg).max(bb), rr.min(gg).min(bb));
    let l = (mx + mn) * 0.5;
    let (mut h, s) = if mx == mn {
        (0.0, 0.0)
    } else {
        let d = mx - mn;
        let s = if l <= 0.5 {
            d / (mx + mn)
        } else {
            d / (2.0 - mx - mn)
        };
        let h = if rr == mx {
            (gg - bb) / d
        } else if gg == mx {
            2.0 + (bb - rr) / d
        } else {
            4.0 + (rr - gg) / d
        };
        (h * 60.0, s)
    };
    if h < 0.0 {
        h += 360.0;
    }
    (
        trunc_byte(h * (1.0 / 360.0) * 255.0),
        trunc_byte(l * 255.0),
        trunc_byte(s * 255.0),
    )
}

/// (hue, lightness, saturation) bytes -> RGB.
pub fn from_hsl(h: u8, l: u8, s: u8) -> [u8; 3] {
    let (h, l, s) = (h as f64 * INV_255 * 360.0, l as f64 * INV_255, s as f64 * INV_255);
    if s == 0.0 {
        let v = trunc_byte(l * 255.0);
        return [v, v, v];
    }
    let m2 = if l <= 0.5 { l * (1.0 + s) } else { l + s - l * s };
    let m1 = 2.0 * l - m2;
    let value = |mut hh: f64| {
        if hh > 360.0 {
            hh -= 360.0;
        } else if hh < 0.0 {
            hh += 360.0;
        }
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
    [value(h + 120.0), value(h), value(h - 120.0)].map(|v| trunc_byte(v * 255.0))
}

/// Integer RGB -> (hue, saturation, value) bytes (`0x565490`). C division truncates toward zero.
pub fn to_hsv(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (r, g, b) = (r as i32, g as i32, b as i32);
    let v = r.max(g).max(b);
    if v == 0 {
        return (0, 0, 0);
    }
    let d = v - r.min(g).min(b);
    let s = d * 255 / v;
    if s == 0 {
        return (0, 0, v as u8);
    }
    let mut h = if r == v {
        (g - b) * 42 / d
    } else if g == v {
        (b - r) * 42 / d + 85
    } else {
        (r - g) * 42 / d + 170
    };
    if h < 0 {
        h += 255;
    }
    (h as u8, s as u8, v as u8)
}

/// (hue, saturation, value) bytes -> RGB (`0x565590`).
pub fn from_hsv(h: u8, s: u8, v: u8) -> [u8; 3] {
    let (h, s, v) = (h as i32, s as i32, v as i32);
    if s == 0 {
        return [v as u8; 3];
    }
    let (sector, f) = match h {
        255 => (0, 0),
        0..42 => (0, h),
        42..85 => (1, h - 42),
        85..127 => (2, h - 85),
        127..170 => (3, h - 127),
        170..212 => (4, h - 170),
        _ => (5, h - 212),
    };
    let p = (255 - s) * v / 255;
    let q = (255 - s * f / 42) * v / 255;
    let t = (255 - s * (42 - f) / 42) * v / 255;
    let [r, g, b] = match sector {
        0 => [v, t, p],
        1 => [q, v, p],
        2 => [p, v, t],
        3 => [p, q, v],
        4 => [t, p, v],
        _ => [v, p, q],
    };
    [r, g, b].map(|c| c.clamp(0, 255) as u8)
}

/// The Hue/Saturation effect (not colorize): hue in degrees, saturation/lightness in -100..100.
pub fn hue_saturation(r: u8, g: u8, b: u8, hue: i32, sat: f64, light: f64) -> [u8; 3] {
    let (h, l, s) = to_hsl(r, g, b);
    // Byte hue -> whole degrees with /256 (not /255), shift, and back; the byte store wraps.
    let deg = h as i32 * 360 / 256;
    let h2 = (((deg + hue) * 256) / 360) as u8;
    let adjust = |x: u8, amount: f64| -> u8 {
        let k = amount * 0.01;
        let delta = if k >= 0.0 {
            (255.0 - x as f64) * k
        } else {
            x as f64 * k
        };
        (x as i32 + delta as i32) as u8
    };
    from_hsl(h2, adjust(l, light), adjust(s, sat))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trips_primaries() {
        for c in [[255, 0, 0], [0, 255, 0], [0, 0, 255], [128, 128, 128], [0, 0, 0]] {
            let (h, s, v) = to_hsv(c[0], c[1], c[2]);
            assert_eq!(from_hsv(h, s, v), c);
        }
    }

    #[test]
    fn matches_fireworks_hue_saturation() {
        // (input, hue, sat, light, Fireworks MX output)
        let cases = [
            ([200, 40, 40], -172, 0.0, 0.0, [40, 175, 199]),
            ([40, 180, 60], -172, 0.0, 0.0, [178, 39, 140]),
            ([40, 180, 60], 45, 0.0, 0.0, [39, 178, 160]),
            ([30, 60, 200], 45, 0.0, 0.0, [121, 30, 199]),
            ([128, 128, 128], 0, 50.0, 0.0, [191, 64, 64]),
            ([200, 40, 40], 0, 50.0, 0.0, [219, 20, 20]),
            ([40, 180, 60], 0, -50.0, 0.0, [74, 143, 82]),
            ([200, 40, 40], 0, 0.0, -40.0, [119, 24, 24]),
            ([200, 40, 40], 0, 0.0, 40.0, [228, 119, 119]),
        ];
        for (c, h, s, l, want) in cases {
            assert_eq!(
                hue_saturation(c[0], c[1], c[2], h, s, l),
                want,
                "{c:?} {h} {s} {l}"
            );
        }
    }
}
