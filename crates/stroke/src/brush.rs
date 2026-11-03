//! Bausteine der Pinsel-Engine: Spitzenmaske, Subpixel-Verschiebung, Strichmuster, Läufer der
//! simple-Pinsel, Zufallsgenerator (Spezifikation 07 §6.2–§6.4, §6.7; Referenz
//! `re\vektor\stroke.py`, bitgleich).
//!
//! **Spitzenmaske** (`CDbBrushMask`, `@782cc0`): Größe `w = h = D′`, eckige Spitzen zusätzlich
//! `ceil(0,4142136·D′)` auf gerade Zahl aufgerundet (Rotationsrand); Bitmap `trunc(w+1)²`,
//! Mittelpunkt `trunc(w·8)` in 1/16 px, Radius `R = trunc(D′·8)`. Rund: Abstand²
//! `d2 = (x²>>4) + (y²>>4)` in 1/16 px; weich linear im **quadrierten** Abstand zwischen
//! `in2 = ((max(R−16,0))²>>4)·(255−s)>>8` und `out2 = (R+16)²>>4`, Glocke
//! `G[t] = min(255, trunc(255·exp(−4,5·t/255)))`. Eckig: Maximumnorm, linearer Abfall.
//! Transformierte Spitzen (Aspekt, Winkel, Größenstufen) siehe [`transformed_tip`].
//!
//! **Subpixel** (`BIAb = 1`, `@835b30`): Maske je Stempel bilinear um `(fx/16, fy/16)` verschoben,
//! Gewichte aus der 16er-Tabelle, jedes der vier Produkte mit `mul255`.
//!
//! **Läufer simple** (`@007876b0`): siehe [`walk_simple`].

use crate::support::color::mul255;
use crate::support::geom::Vec2;

/// Subpixel-Gewichte (`@17b5978`).
pub const WSHIFT: [u32; 16] = [
    0, 16, 32, 48, 64, 80, 96, 112, 128, 143, 159, 175, 191, 207, 223, 239,
];

/// Glockenkurve `G[t] = min(255, trunc(255·exp(−4,5·t/255)))`.
pub fn bell_table() -> [u8; 256] {
    let mut g = [0u8; 256];
    for (t, v) in g.iter_mut().enumerate() {
        *v = ((255.0 * (-4.5 * t as f64 / 255.0).exp()) as i64).min(255) as u8;
    }
    g
}

/// Quadratische 8-Bit-Spitze (`n × n`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tip {
    pub n: usize,
    pub data: Vec<u8>,
}

impl Tip {
    /// Wert an (x, y); außerhalb 0.
    #[inline]
    pub fn at(&self, x: i64, y: i64) -> u8 {
        if x < 0 || y < 0 || x >= self.n as i64 || y >= self.n as i64 {
            0
        } else {
            self.data[y as usize * self.n + x as usize]
        }
    }
}

/// Weichheit als Byte: `BSEi·255/1000`, bei `D′ < 2` immer 0.
#[inline]
pub fn softness_byte(dp: f64, soft_permille: u32) -> u32 {
    if dp >= 2.0 {
        soft_permille.min(1000) * 255 / 1000
    } else {
        0
    }
}

/// Breite der Spitzen-Bitmap ohne `+1`: `D′`, eckig plus Rotationsrand.
pub fn tip_width(dp: f64, round: bool) -> f64 {
    if round {
        dp
    } else {
        let mut pad = (0.414_213_6 * dp).ceil();
        if (pad as i64) & 1 == 1 {
            pad += 1.0;
        }
        dp + pad
    }
}

/// Unverschobene Spitze für den wirksamen Durchmesser `dp` (07 §6.3, ohne Transformation).
pub fn tip_mask(dp: f64, round: bool, soft_permille: u32, bell: bool) -> Tip {
    let w = tip_width(dp, round);
    let n = (w + 1.0).clamp(0.0, 4096.0) as usize;
    let ox = (w * 8.0) as i64;
    let rr = (dp * 8.0) as i64;
    let s = softness_byte(dp, soft_permille) as i64;
    let g = bell_table();
    let mut data = vec![0u8; n * n];
    for y in 0..n {
        let fy = y as i64 * 16 - ox + 8;
        for x in 0..n {
            let fx = x as i64 * 16 - ox + 8;
            data[y * n + x] = tip_value(fx, fy, rr, s, round, bell, &g);
        }
    }
    Tip { n, data }
}

/// Wert der Spitze an `(fx, fy)` (1/16 px relativ zur Mitte), Radius `rr` (1/16 px), Weichheit `s`.
#[inline]
pub fn tip_value(fx: i64, fy: i64, rr: i64, s: i64, round: bool, bell: bool, g: &[u8; 256]) -> u8 {
    if round {
        let d2 = ((fx * fx) >> 4) + ((fy * fy) >> 4);
        if s == 0 {
            return if d2 <= (rr * rr) >> 4 { 255 } else { 0 };
        }
        let out2 = ((rr + 16) * (rr + 16)) >> 4;
        if d2 > out2 {
            return 0;
        }
        let m = (rr - 16).max(0);
        let in2 = (((m * m) >> 4) * (255 - s)) >> 8;
        if d2 <= in2 {
            return 255;
        }
        let t = ((d2 - in2) * 255 / (out2 - in2)).clamp(0, 255);
        if bell { g[t as usize] } else { (255 - t) as u8 }
    } else {
        let m = fx.abs().max(fy.abs());
        if m > rr {
            return 0;
        }
        // `@7829d0`: ohne Weichheit gleich 255 (sonst fiele der Ring m = R auf 0)
        if s == 0 {
            return 255;
        }
        let inner = ((255 - s) * rr) >> 8;
        if m > inner && rr > inner {
            (255 - (m - inner) * 255 / (rr - inner)).clamp(0, 255) as u8
        } else {
            255
        }
    }
}

/// Transformation einer Spitze (Eingaben von `@782cc0`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TipTransform {
    /// `BASi` in Promille.
    pub aspect_permille: u32,
    /// `BANi` in Grad.
    pub angle_deg: u32,
    /// Größenstufe (Stufe, M, `BMSi` in Promille); Stufe 0 = volle Größe.
    pub size_stage: Option<(u32, u32, u32)>,
    /// Winkelstufe (Stufe, M).
    pub angle_stage: Option<(u32, u32)>,
}

/// 2×2-Teil einer 3×3-Matrix, Produkt `a·b` wie `@5ea680`.
fn mat_mul(a: [[f64; 2]; 2], b: [[f64; 2]; 2]) -> [[f64; 2]; 2] {
    std::array::from_fn(|i| std::array::from_fn(|j| a[i][0] * b[0][j] + a[i][1] * b[1][j]))
}

/// Drehmatrix wie `@5ead80`: `[[cos, sin], [−sin, cos]]`.
fn mat_rot(t: f64) -> [[f64; 2]; 2] {
    let (s, c) = sin_cos_x87(t);
    [[c, s], [-s, c]]
}

/// Sinus und Kosinus wie x87 `fsin`/`fcos` mit anschließender Rundung auf `f64` (erweiterte
/// Genauigkeit ⇒ praktisch korrekt gerundet; z. B. sin 30° = 0,5 exakt statt 0,49999999999999994 der
/// Laufzeitbibliothek). Rechnung in doppelt-doppelter Genauigkeit (Taylorreihe nach Reduktion).
pub fn sin_cos_x87(t: f64) -> (f64, f64) {
    #[derive(Clone, Copy)]
    struct Dd(f64, f64);
    fn two_sum(a: f64, b: f64) -> Dd {
        let s = a + b;
        let bb = s - a;
        Dd(s, (a - (s - bb)) + (b - bb))
    }
    fn add(a: Dd, b: Dd) -> Dd {
        let s = two_sum(a.0, b.0);
        let e = s.1 + a.1 + b.1;
        two_sum(s.0, e)
    }
    fn mul(a: Dd, b: Dd) -> Dd {
        let p = a.0 * b.0;
        let e = a.0.mul_add(b.0, -p) + a.0 * b.1 + a.1 * b.0;
        two_sum(p, e)
    }
    fn scale(a: Dd, k: f64) -> Dd {
        mul(a, Dd(k, 0.0))
    }
    fn div(a: Dd, k: f64) -> Dd {
        let q = a.0 / k;
        let r = (-q).mul_add(k, a.0) + a.1;
        two_sum(q, r / k)
    }
    if !t.is_finite() {
        return (f64::NAN, f64::NAN);
    }
    // Reduktion auf [−π, π] mit 2π in doppelt-doppelter Genauigkeit
    let two_pi = Dd(std::f64::consts::TAU, 2.449_293_598_294_706_4e-16);
    let k = (t / two_pi.0).round();
    let x = add(Dd(t, 0.0), scale(two_pi, -k));
    let x2 = mul(x, x);
    let (mut sin, mut cos) = (Dd(0.0, 0.0), Dd(0.0, 0.0));
    let mut term_s = x;
    let mut term_c = Dd(1.0, 0.0);
    for n in 0..40 {
        sin = add(sin, term_s);
        cos = add(cos, term_c);
        let (a, b) = ((2 * n + 2) as f64, (2 * n + 3) as f64);
        term_s = div(mul(term_s, x2), -(a * b));
        term_c = div(mul(term_c, x2), -((a - 1.0) * a));
        if term_s.0.abs() < 1e-40 && term_c.0.abs() < 1e-40 {
            break;
        }
    }
    (sin.0 + sin.1, cos.0 + cos.1)
}

/// Inverse Matrix der Spitze in 8.8-Festkomma (`@782cc0`): `M = R(Stufe)·R(−BANi)·S(1, Aspekt)·S(Größe)`,
/// Inversion über Adjunkte/Determinante (`@5ea5b0`, Determinante in erweiterter Genauigkeit),
/// Einträge `trunc(m·256)` (`_ftol`). `None` = Einheitsmatrix (Abtastung ohne Transformation).
pub fn tip_inverse_fixed(dp: f64, t: &TipTransform) -> Option<[i64; 4]> {
    let mut m = [[1.0, 0.0], [0.0, 1.0]];
    if let Some((stage, mm, min_size)) = t.size_stage.filter(|&(s, mm, _)| mm > 0 && s > 0) {
        let q = (mm - stage.min(mm)) as f64 / mm as f64;
        let ms = min_size as f64 / 1000.0;
        let s = ms + (1.0 - ms) * q.powf(1.5);
        m = mat_mul([[s, 0.0], [0.0, s]], m);
    }
    let (aspect, angle) = if dp < 4.0 {
        (1000, 0)
    } else {
        (t.aspect_permille, t.angle_deg)
    };
    let sm = [[1.0, 0.0], [0.0, aspect as f64 / 1000.0]];
    let rm = mat_rot(angle as f64 * std::f64::consts::PI / -180.0);
    m = mat_mul(mat_mul(rm, sm), m);
    if let Some((stage, mm)) = t.angle_stage.filter(|&(s, mm)| mm > 0 && s > 0) {
        m = mat_mul(mat_rot(stage as f64 * -std::f64::consts::PI / mm as f64), m);
    }
    if m == [[1.0, 0.0], [0.0, 1.0]] {
        return None;
    }
    // Determinante m00·m11 − m01·m10 einmal gerundet (x87 rechnet die Produkte erweitert)
    let p = m[0][1] * m[1][0];
    let e = m[0][1].mul_add(m[1][0], -p);
    let det = m[0][0].mul_add(m[1][1], -p) - e;
    if det.abs() < 1e-12 {
        return Some([0; 4]);
    }
    let inv = [m[1][1] / det, -m[0][1] / det, -m[1][0] / det, m[0][0] / det];
    Some(inv.map(|v| {
        let x = (v * 256.0).trunc();
        if x.is_finite() {
            x.clamp(-1e9, 1e9) as i64
        } else {
            0
        }
    }))
}

/// Spitze mit Transformation (07 §6.3, `@782cc0`): Abtastung als Mittel der vier Pixelecken
/// `(f1+f2+f3+f4) >> 2`, Ecken ohne +8, Koordinaten `X = (a·px + b·py) >> 8`,
/// `Y = (c·px + d·py) >> 8` mit [`tip_inverse_fixed`]. Bei `D′ < 4` werden Aspekt und Winkel
/// ignoriert.
pub fn transformed_tip(dp: f64, round: bool, soft_permille: u32, bell: bool, t: &TipTransform) -> Tip {
    let Some(m) = tip_inverse_fixed(dp, t) else {
        return tip_mask(dp, round, soft_permille, bell);
    };
    let w = tip_width(dp, round);
    let n = (w + 1.0).clamp(0.0, 4096.0) as usize;
    let ox = (w * 8.0) as i64;
    let rr = (dp * 8.0) as i64;
    let s = softness_byte(dp, soft_permille) as i64;
    let g = bell_table();
    let sample = |px: i64, py: i64| -> u32 {
        // px, py in 1/16 px relativ zur Mitte
        let qx = (m[0] * px + m[1] * py) >> 8;
        let qy = (m[2] * px + m[3] * py) >> 8;
        tip_value(qx, qy, rr, s, round, bell, &g) as u32
    };
    let mut data = vec![0u8; n * n];
    for y in 0..n {
        let fy = y as i64 * 16 - ox;
        for x in 0..n {
            let fx = x as i64 * 16 - ox;
            let v =
                (sample(fx, fy) + sample(fx + 16, fy) + sample(fx, fy + 16) + sample(fx + 16, fy + 16)) >> 2;
            data[y * n + x] = v as u8;
        }
    }
    Tip { n, data }
}

/// Bilineare Verschiebung um `(fx/16, fy/16)` px; Ergebnis `(n+1) × (n+1)`.
pub fn shift_tip(tip: &Tip, fx: usize, fy: usize) -> Tip {
    let (a, b) = (WSHIFT[fx & 15], WSHIFT[fy & 15]);
    let c00 = mul255(255 - a, 255 - b);
    let c10 = mul255(a, 255 - b);
    let c01 = mul255(255 - a, b);
    let c11 = mul255(a, b);
    let n = tip.n as i64;
    let nn = tip.n + 1;
    let mut data = vec![0u8; nn * nn];
    for y in 0..nn as i64 {
        for x in 0..nn as i64 {
            let s00 = tip.at(x, y) as u32;
            let s10 = if x > 0 && x <= n {
                tip.at(x - 1, y) as u32
            } else {
                0
            };
            let s01 = if y > 0 && y <= n {
                tip.at(x, y - 1) as u32
            } else {
                0
            };
            let s11 = if x > 0 && x <= n && y > 0 && y <= n {
                tip.at(x - 1, y - 1) as u32
            } else {
                0
            };
            let v = mul255(s00, c00) + mul255(s10, c10) + mul255(s01, c01) + mul255(s11, c11);
            data[y as usize * nn + x as usize] = v.min(255) as u8;
        }
    }
    Tip { n: nn, data }
}

/// Strichmuster-Zustand je Teilpfad (`@0078a1f0`: an = 1, Zähler 0, Index 0; Fortschreibung je
/// Stempel-Ereignis in `@007876b0`). `d` ist `BDIi` (nicht D′).
#[derive(Debug, Clone)]
pub struct Dash {
    d: i64,
    on_sizes: [i64; 3],
    off_sizes: [i64; 3],
    n: usize,
    on: bool,
    cnt: i64,
    idx: usize,
}

impl Dash {
    /// Aus `BDIi` und den Musterpaaren (höchstens 3 werden benutzt).
    pub fn new(d: u32, dashes: &[(u32, u32)]) -> Option<Self> {
        if dashes.is_empty() {
            return None;
        }
        let n = dashes.len().min(3);
        let mut on_sizes = [0i64; 3];
        let mut off_sizes = [0i64; 3];
        for (k, &(a, b)) in dashes.iter().take(3).enumerate() {
            on_sizes[k] = a as i64;
            off_sizes[k] = b as i64;
        }
        Some(Self {
            d: d as i64,
            on_sizes,
            off_sizes,
            n,
            on: true,
            cnt: 0,
            idx: 0,
        })
    }

    /// Ein Stempel-Ereignis; `true` = stempeln.
    pub fn event(&mut self) -> bool {
        self.cnt += 1;
        if self.on && self.cnt >= self.on_sizes[self.idx] {
            self.cnt = 0;
            self.on = false;
        } else if !self.on && self.d + self.off_sizes[self.idx] <= self.cnt {
            self.cnt = 0;
            self.on = true;
            self.idx += 1;
            if self.idx >= self.n {
                self.idx = 0;
            }
        }
        self.on
    }
}

/// `hypot` mit Korrekturschritt (nahezu korrekt gerundet wie Pythons `math.hypot`).
pub fn hypot_cr(x: f64, y: f64) -> f64 {
    let (x, y) = (x.abs(), y.abs());
    if !x.is_finite() || !y.is_finite() {
        return x.hypot(y);
    }
    let x2 = x * x;
    let ex = x.mul_add(x, -x2);
    let y2 = y * y;
    let ey = y.mul_add(y, -y2);
    let s = x2 + y2;
    let bb = s - x2;
    let es = (x2 - (s - bb)) + (y2 - bb) + ex + ey;
    if s == 0.0 {
        return 0.0;
    }
    let h = s.sqrt();
    let r = (-h).mul_add(h, s) + es;
    h + r / (2.0 * h)
}

#[inline]
fn round_half_away(v: f64) -> i64 {
    let r = if v >= 0.0 {
        (v + 0.5).floor()
    } else {
        -((-v + 0.5).floor())
    };
    sat(r)
}

#[inline]
fn sat(v: f64) -> i64 {
    if v.is_finite() {
        v.clamp(-(1i64 << 40) as f64, (1i64 << 40) as f64) as i64
    } else {
        0
    }
}

/// Stempelpositionen (1/16 px, ganzzahlig) eines Teilpfads für simple-Pinsel (`@007876b0`, B).
///
/// - Linienschritt über die Hauptachse: Fehler `e = 2·neben − haupt`; je Besuch bei `e ≥ 0`
///   Nebenachse +1 und `e += 2(neben − haupt)`, sonst `e += 2·neben`; dann Hauptachse +1.
/// - **AA**: Punkte `trunc(16·p)`; je Segment `n` = Hauptachsenlänge Besuche (Start ein-, Ende
///   ausgeschlossen); Stempel-Ereignis, wenn `acc > 0xFFFF` (dann `acc −= 0x10000`), danach
///   `acc += trunc(|Segment|·65536) div n`. `acc` startet je Teilpfad bei 0x10000.
/// - **ohne AA**: Endpunkte bei geradem `N` halb-weg gerundet, bei ungeradem abgerundet;
///   `(haupt >> 4) + 1` Besuche, jeder ein Ereignis.
/// - Segmente der Länge 0 entfallen, außer als erstes; das Ende des letzten Segments wird nie besucht.
pub fn walk_simple(
    pts: &[Vec2],
    closed: bool,
    n_tip: i64,
    aa: bool,
    dash: Option<&mut Dash>,
) -> Vec<(i64, i64)> {
    walk_simple_steps(pts, closed, n_tip, aa, dash)
        .into_iter()
        .map(|s| (s.x, s.y))
        .collect()
}

/// Ein Stempel des simple-Läufers mit Lage im Segment (für die 16.16-Interpolation von Druck und
/// Geschwindigkeit, re 39 §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SimpleStep {
    /// Position in 1/16 px.
    pub x: i64,
    pub y: i64,
    /// Segmentindex (Punkt `seg` → `seg + 1`, geschlossen mit Rücklauf zum Start).
    pub seg: usize,
    /// Anteil im Segment in 16.16 (`step·0x10000 / n`).
    pub frac16: i64,
}

/// Wie [`walk_simple`], mit Segment und Anteil je Stempel.
pub fn walk_simple_steps(
    pts: &[Vec2],
    closed: bool,
    n_tip: i64,
    aa: bool,
    mut dash: Option<&mut Dash>,
) -> Vec<SimpleStep> {
    let mut out = Vec::new();
    if pts.is_empty() {
        return out;
    }
    let mut seq: Vec<Vec2> = pts.to_vec();
    if closed && pts.len() > 1 {
        seq.push(pts[0]);
    }
    let segs: Vec<(Vec2, Vec2)> = if seq.len() > 1 {
        seq.windows(2).map(|w| (w[0], w[1])).collect()
    } else {
        vec![(seq[0], seq[0])]
    };
    let mut acc: i64 = 0;
    for (si, (p, q)) in segs.iter().enumerate() {
        let first = si == 0;
        if first {
            acc = 0x10000;
        }
        let (a, b) = if aa {
            (
                (sat((p.x * 16.0).trunc()), sat((p.y * 16.0).trunc())),
                (sat((q.x * 16.0).trunc()), sat((q.y * 16.0).trunc())),
            )
        } else {
            let rnd = |v: f64| {
                if n_tip % 2 == 0 {
                    round_half_away(v)
                } else {
                    sat(v.floor())
                }
            };
            ((rnd(p.x) * 16, rnd(p.y) * 16), (rnd(q.x) * 16, rnd(q.y) * 16))
        };
        let (dx, dy) = ((b.0 - a.0).abs(), (b.1 - a.1).abs());
        let swap = dx < dy;
        let (major, minor, s_major, mut s_minor, mut mj, mut mn) = if swap {
            (
                dy,
                dx,
                if b.1 > a.1 { 1 } else { -1 },
                if b.0 > a.0 { 1 } else { -1 },
                a.1,
                a.0,
            )
        } else {
            (
                dx,
                dy,
                if b.0 > a.0 { 1 } else { -1 },
                if b.1 > a.1 { 1 } else { -1 },
                a.0,
                a.1,
            )
        };
        if major == 0 && !first {
            continue;
        }
        let (n, inc, step_major) = if aa {
            let mut n = major;
            let inc = if n > 0 {
                sat((hypot_cr(p.x - q.x, p.y - q.y) * 65536.0).trunc()).div_euclid(n)
            } else {
                0
            };
            if n < 1 {
                n = if first { 1 } else { 0 };
            }
            (n, inc, s_major)
        } else {
            s_minor *= 16;
            ((major >> 4) + 1, 0, 16 * s_major)
        };
        let mut err = 2 * minor - major;
        for step in 0..n {
            if !aa || acc > 0xFFFF {
                let stamp = match dash.as_deref_mut() {
                    Some(d) => d.event(),
                    None => true,
                };
                if stamp {
                    let (x, y) = if swap { (mn, mj) } else { (mj, mn) };
                    out.push(SimpleStep {
                        x,
                        y,
                        seg: si,
                        frac16: if n > 0 { (step << 16) / n } else { 0 },
                    });
                }
                if aa {
                    acc -= 0x10000;
                }
            }
            if err >= 0 {
                mn += s_minor;
                err += 2 * (minor - major);
            } else {
                err += 2 * minor;
            }
            acc += inc;
            mj += step_major;
        }
    }
    out
}

/// Zufallsgenerator MINSTD (48271) nach Schrage (07 §6.2): `s = s·48271 − (s/44488)·0x7FFFFFFF`;
/// ist `s < 1`, wird `0x7FFFFFFF` addiert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Minstd {
    pub state: i32,
}

impl Minstd {
    pub fn new(seed: u32) -> Self {
        Self {
            state: (seed & 0x7FFF_FFFF) as i32,
        }
    }

    /// Nächster Wert (1..0x7FFFFFFE).
    pub fn next_value(&mut self) -> i32 {
        let s = self.state as i64;
        let mut v = (s % 44488) * 48271 - (s / 44488) * 3399;
        if v < 1 {
            v += 0x7FFF_FFFF;
        }
        self.state = v as i32;
        self.state
    }

    /// `rnd % m` (0 bei `m ≤ 0`).
    pub fn modulo(&mut self, m: i64) -> i64 {
        let r = self.next_value() as i64;
        if m <= 0 { 0 } else { r % m }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Inverse Spitzenmatrix gegen die Einzelstempel des Originals (`phase5\pinsel\p_einzel`).
    #[test]
    fn tip_inverse_matches_original() {
        let t = |angle_deg: u32, aspect_permille: u32| TipTransform {
            aspect_permille,
            angle_deg,
            size_stage: None,
            angle_stage: None,
        };
        assert_eq!(tip_inverse_fixed(15.0, &t(30, 1000)), Some([221, 127, -127, 221]));
        assert_eq!(tip_inverse_fixed(15.0, &t(30, 500)), Some([221, 127, -255, 443]));
        assert_eq!(tip_inverse_fixed(20.0, &t(15, 700)), Some([247, 66, -94, 353]));
        let (s, c) = sin_cos_x87(30.0 * std::f64::consts::PI / -180.0);
        assert_eq!((s, c), (-0.49999999999999994, 0.8660254037844387));
        let tip = transformed_tip(15.0, false, 0, false, &t(30, 1000));
        assert_eq!(
            &tip.data[24..48],
            &[
                0, 0, 0, 0, 0, 0, 0, 0, 63, 127, 63, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0
            ]
        );
    }

    #[test]
    fn hard_tip_and_shift() {
        let t = tip_mask(1.0, true, 0, true);
        assert_eq!(t.n, 2);
        let t3 = tip_mask(3.0, true, 0, true);
        assert_eq!(t3.n, 4);
        assert_eq!(t3.at(1, 1), 255);
        let s = shift_tip(&t, 0, 0);
        assert_eq!(s.n, 3);
        let sq = tip_mask(4.0, false, 0, true);
        assert_eq!(sq.n, 7); // 4 + ceil(1,657)=2 → 6, +1
        assert_eq!(bell_table()[0], 255);
    }

    #[test]
    fn walker_basic() {
        // waagerechte Gerade 0..3 px, AA: Stempel bei 0, 1, 2 (Ende nicht besucht)
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(3.0, 0.0)];
        let w = walk_simple(&pts, false, 1, true, None);
        assert_eq!(w, vec![(0, 0), (16, 0), (32, 0)]);
        // ohne AA, N ungerade: 4 Besuche (beide Enden)
        let w = walk_simple(&pts, false, 1, false, None);
        assert_eq!(w, vec![(0, 0), (16, 0), (32, 0), (48, 0)]);
        let mut d = Dash::new(1, &[(2, 1)]).unwrap();
        let w = walk_simple(&pts, false, 1, false, Some(&mut d));
        assert_eq!(w.len(), 2);
    }

    #[test]
    fn minstd() {
        let mut r = Minstd::new(1);
        assert_eq!(r.next_value(), 48271);
        assert_eq!(r.next_value(), 182_605_794);
        assert!((hypot_cr(3.0, 4.0) - 5.0).abs() < 1e-15);
    }
}
