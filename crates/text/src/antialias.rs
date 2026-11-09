//! Tonkurven der Text-Kantenglättung (09 §6.2, Laufzeit T9): Trefferzahl k von N² Unterpixeln →
//! Alpha.
//!
//! - „Glatt“ (0): N = 4, Tabelle `@01b4c088` (α = 16·k, k = 16 → 255).
//! - „Scharf“ (1) und „System“ (3): N = 4, Tabelle `@01b4c09c` (Kontrastkurve).
//! - „Stark“ (2): eigener Renderer-Pfad im Original; die Caches enthalten genau die Werte der
//!   Glatt-Tabelle, aber mehr Deckung (T9). Nachbau (A): N = 4 und die empirische Tabelle aus 6.2,
//!   deren Werte alle in der Glatt-Tabelle liegen.
//! - „Benutzerdefiniert“ (4): N = `TOSi` (4/8/16), kubische Bézier-Kurve mit den Stützwerten
//!   0, P1 = `TA2i` (Stärke), P2 = `TA1i` (Schärfe), 255; Ergebnis **abgeschnitten** (B, T9 15/15).
//! - Ohne Glättung (`FETi = 0`): N = 1, binär.

use crate::model::{AntiAlias, AntiAliasMode};

/// Tabelle „glatt“ (`Original-EXE@01b4c088`).
pub const LUT_SMOOTH: [u8; 17] = [
    0, 16, 32, 48, 64, 80, 96, 112, 128, 144, 160, 176, 192, 208, 224, 240, 255,
];
/// Tabelle „scharf“ (`Original-EXE@01b4c09c`).
pub const LUT_CRISP: [u8; 17] = [
    0, 0, 8, 18, 40, 62, 84, 106, 128, 150, 167, 194, 216, 238, 248, 255, 255,
];
/// Empirische Tabelle für „stark“ (09 §6.2, A).
pub const LUT_STRONG: [u8; 17] = [
    0, 48, 80, 96, 128, 128, 144, 160, 192, 208, 208, 240, 255, 255, 255, 255, 255,
];

/// Tabelle der benutzerdefinierten Glättung für `n × n` Unterpixel:
/// `α(k) = trunc(255·u³ + 3·S·u²·t + 3·W·u·t²)` mit u = k/n², t = 1 − u, S = `TA1i`, W = `TA2i`
/// (Rechnung in `f64` wie die Python-Referenz `glaettung.custom_lut`).
pub fn custom_lut(n: u32, sharpness: u8, strength: u8) -> Vec<u8> {
    let n = n.max(1);
    let m = (n * n) as f64;
    let (s, w) = (sharpness as f64, strength as f64);
    (0..=n * n)
        .map(|k| {
            let u = k as f64 / m;
            let t = 1.0 - u;
            let v = 255.0 * u * u * u + 3.0 * s * u * u * t + 3.0 * w * u * t * t;
            v.clamp(0.0, 255.0) as u8
        })
        .collect()
}

/// Wirksame Überabtastung N und Tonkurve (Länge N² + 1) für eine Glättungseinstellung.
///
/// `TOSi` außer 4/8/16 wird auf den nächsten zulässigen Wert gebracht (Dialogbereich, A).
pub fn sampling(aa: &AntiAlias) -> (u32, Vec<u8>) {
    if !aa.enabled {
        return (1, vec![0, 255]);
    }
    match aa.mode.effective() {
        AntiAliasMode::Smooth => (4, LUT_SMOOTH.to_vec()),
        AntiAliasMode::Crisp | AntiAliasMode::System => (4, LUT_CRISP.to_vec()),
        AntiAliasMode::Strong => (4, LUT_STRONG.to_vec()),
        AntiAliasMode::Custom => {
            let n = match aa.oversample {
                0..=5 => 4,
                6..=11 => 8,
                _ => 16,
            };
            (n, custom_lut(n, aa.sharpness, aa.strength))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_custom_is_nearly_linear() {
        let l = custom_lut(8, 192, 64);
        assert_eq!(l.len(), 65);
        assert_eq!(l[0], 0);
        assert_eq!(l[64], 255);
        assert_eq!(l[32], 127); // 127,5 abgeschnitten
    }

    #[test]
    fn modes() {
        let mut aa = AntiAlias::default();
        assert_eq!(sampling(&aa).0, 4);
        aa.mode = AntiAliasMode::System;
        assert_eq!(sampling(&aa).1, LUT_CRISP.to_vec());
        aa.mode = AntiAliasMode::Custom;
        aa.oversample = 16;
        assert_eq!(sampling(&aa).1.len(), 257);
        aa.enabled = false;
        assert_eq!(sampling(&aa), (1, vec![0, 255]));
    }
}
