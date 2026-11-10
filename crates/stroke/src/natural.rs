//! Natürliche Pinsel (`BRTi = 0`), Mehrfachspitzen, Empfindlichkeiten, alphaRemap und Strichtextur
//! (Spezifikation 07 §6.3–§6.8). Die Python-Referenz hat dafür keinen Läufer; umgesetzt ist der
//! Code-Befund der Spezifikation, geprüft an den Laufzeit-Testdateien T2 (`re\_testdateien\07\t2_*`).
//!
//! **Stand Phase 5 (B, Disassemblierung + eigene Laufzeitdateien `re\_testdateien\phase5\pinsel`,
//! Prüfung `fp-render\tests\brush_fit.rs`; T2 28/28 Dateien pixelgenau):**
//!
//! **Läufer natural** (`@787db0`, je Segment):
//! - Bresenham in 1/16 px; Druck und Geschwindigkeit laufen linear mit (16.16).
//! - Zufallsfolge je Segment ab `RNDi + 1` des Startpunkts (MINSTD).
//! - Stempelabstand `trunc(max(1; BSPi·D′·0,001)·65536)` (`@7853e0`).
//! - Gestempelt wird, wenn der Abstand erreicht ist oder (Fluss ≠ 0) der Fluss die Schwelle
//!   `1001 − BFRi` erreicht; danach Abstand abziehen, Fluss um die Schwelle senken bzw. auf 0.
//!   Flusszuwachs je Schritt `min(1, ⌊len·256/v⌋ / n)` (ganzzahlig, v = Geschwindigkeit des
//!   Startpunkts, `VELf·256`).
//! - Je Ereignis und Spitze: r0 = nächster Wert → Streuung (`@786010`, `r0 % R`), ra, rb → Versatz
//!   `(ra % sc − sc>>1, rb % sc − sc>>1)`; der Stempel (`@788ae0`) arbeitet mit einer lokalen
//!   Kopie ab rb: Stufe (`@7863b0`, Schritt nur bei R ≠ 0), Farbjitter (`@785b00`, nur bei einer
//!   Spitze, immer drei Schritte), Deckkraft mit next(lokal), Schwärze mit dem Wert danach.
//! - Versätze (`@785530`) und Farben (`@786f30`) mehrerer Spitzen einmal je Strich ab `PTHv/RNDi`.
//! - Stempelrechteck `[x, x + N)` ohne, `[p16>>4, + N + 1)` mit Kantenglättung.
//!
//! **Pixel** (`@6f9380`, `@6ead80`, `@6eaa20`, `@6ea860`): Mischen mit `W = DIV[sa][a′]`, direkt
//! nur bei voller Maske und voller Deckkraft; Puffer anfangs weiß-transparent. Schwärze: nur das
//! Blau-Byte wird abgesenkt (die Parameterbytes sind im Original verschoben), siehe
//! `ArgbInk::stamp`. alphaRemap-Basiskurven abgeschnitten, angewandt nach der Innen/außen-Maske.
//!
//! **Offen**: alphaRemap bei Konzentration < 100 an einzelnen Pixeln ±1; Fluss-Verweilen am
//! Segmentende (`@787520`, nur bei `DURi ≠ 0`) nicht umgesetzt; Strichtextur der natürlichen
//! Pinsel nur aus der Spezifikation (Random Squares exakt).
use std::collections::HashMap;

use crate::attrs::{
    BrushParams, Feedback, SenseSource as Src, SenseTarget as Tgt, Stroke, StrokePlacement, TipColoringMode,
    TipShape, TipSpacingMode, VectorPath,
};
use crate::brush::{Minstd, Tip, TipTransform, hypot_cr, shift_tip, transformed_tip};
use crate::paint::mul;
use crate::stroke::{InkBuffer, InkBufferOut, effective_diameter, feedback_code};
use crate::support::color::DIV;
use crate::support::geom::{IRect, Vec2};
use crate::support::raster::Mask;

#[inline]
fn screen(a: u8, b: u8) -> u8 {
    255 - mul(255 - a, 255 - b)
}

/// Strichtextur auf einen Maskenwert (07 §6.8, `@6ea950`): `inv = 255 − BTBi·255/1000`,
/// `e = BTEi·255/1000`.
#[inline]
pub fn textured_mask(mask: u8, t: u8, btb: u32, bte: u32) -> u8 {
    let inv = (255 - btb.min(1000) * 255 / 1000) as u8;
    let e = (bte.min(1000) * 255 / 1000) as u8;
    if e == 0 {
        mul(screen(t, inv), mask)
    } else if mask <= e {
        mul(mask, t)
    } else {
        let scaled = ((mask - e) as u32 * 255 / (255 - e) as u32) as u8;
        mul(screen(mul(scaled, inv), t), mask)
    }
}

/// Kachel-Nachschlag einer Texturmaske (Ursprung `⌊TOXf⌋`, `⌊TOYf⌋`).
pub(crate) struct TexLookup<'a> {
    mask: &'a Mask,
    ox: i64,
    oy: i64,
}

impl<'a> TexLookup<'a> {
    pub fn new(mask: &'a Mask, offset: Vec2) -> Self {
        let f = |v: f64| {
            if v.is_finite() {
                v.floor().clamp(-9e15, 9e15) as i64
            } else {
                0
            }
        };
        Self {
            mask,
            ox: f(offset.x),
            oy: f(offset.y),
        }
    }

    #[inline]
    pub fn value(&self, x: i32, y: i32) -> u8 {
        let (tw, th) = (self.mask.width() as i64, self.mask.height() as i64);
        if tw == 0 || th == 0 {
            return 255;
        }
        let u = (x as i64 - self.ox).rem_euclid(tw);
        let v = (y as i64 - self.oy).rem_euclid(th);
        self.mask
            .data()
            .get((v * tw + u) as usize)
            .copied()
            .unwrap_or(255)
    }
}

/// Basiskurve eines alphaRemap-Effekts (07 §6.8).
fn remap_base(effect: u32) -> [u8; 256] {
    let mut out = [0u8; 256];
    // abgeschnitten, nicht gerundet (Laufzeit phase5 `p_remap`: alle Kurven und beide Konzentrationen)
    let r = |v: f64| v.trunc().clamp(0.0, 255.0) as u8;
    for (i, o) in out.iter_mut().enumerate() {
        let fi = i as f64;
        let j = fi - 128.0;
        *o = match effect {
            1 | 5 => {
                if i < 128 {
                    (2 * i).min(255) as u8
                } else if i < 192 {
                    r(j + 255.0 - j / 0.198_113_207_55)
                } else if effect == 5 {
                    0
                } else {
                    r((fi - 192.0) / 0.247_058_823_53)
                }
            }
            2 => {
                if i < 128 {
                    r(1.5 * fi)
                } else if i < 192 {
                    r(j + 191.0 - j / 0.633_633_663_4)
                } else {
                    r((fi - 38.0) / 1.148_437_5)
                }
            }
            3 => {
                if i == 0 {
                    0
                } else if i < 64 {
                    let d = 11.0 - (fi - 1.0) * 0.296_53;
                    r(122.0 - d * d)
                } else {
                    i as u8
                }
            }
            4 => {
                let m = (i % 64) as f64;
                r(7.968_75 * m.min(64.0 - m))
            }
            _ => i as u8,
        };
    }
    out
}

/// alphaRemap-Tabelle (`@6fa450`, `@6e96e0`): Zeile `k = trunc((BCNi·255/1000)·49/255)`,
/// `out[i] = trunc(base[min(255, trunc(i·50/k))]·k/50)`; Zeile 0 von Effekt e = Zeile 50 von e − 1
/// (bei e = 1 leer). Effekt 0 = Identität.
pub fn alpha_remap_table(effect: u32, concentration_permille: u32) -> [u8; 256] {
    let mut id = [0u8; 256];
    for (i, v) in id.iter_mut().enumerate() {
        *v = i as u8;
    }
    if effect == 0 || effect > 5 {
        return id;
    }
    let l = concentration_permille.min(1000) * 255 / 1000;
    let k = l * 49 / 255;
    if k == 0 {
        return if effect == 1 {
            [0u8; 256]
        } else {
            remap_base(effect - 1)
        };
    }
    let base = remap_base(effect);
    let mut out = [0u8; 256];
    for (i, o) in out.iter_mut().enumerate() {
        let idx = ((i as u32 * 50) / k).min(255) as usize;
        *o = (base[idx] as u32 * k / 50) as u8;
    }
    out
}

/// Zeilenwert der Empfindlichkeit (0..128).
#[inline]
pub(crate) fn sense(b: &BrushParams, t: Tgt, s: Src) -> i64 {
    b.sense[t as usize][s as usize] as i64
}

/// Stiftzustand je Schritt.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PenState {
    /// Druck 0..255.
    pub p: i64,
    /// Geschwindigkeit 0..
    pub spd: i64,
    /// geglättete Richtung (−128..128).
    pub h: i64,
    pub v: i64,
}

/// Wert mit Empfindlichkeiten (`@786290`-Muster): Basis `l`, Ergebnis in [0, l]; `rv` ist der vom
/// Stempel übergebene Zufallswert (`rv % R`).
pub(crate) fn sensed(b: &BrushParams, t: Tgt, l: i64, st: &PenState, rv: i64) -> i64 {
    let pl = (sense(b, t, Src::Pressure) * l) >> 7;
    let sl = (sense(b, t, Src::Speed) * l) >> 7;
    let hl = (sense(b, t, Src::Horizontal) * l) >> 7;
    let vl = (sense(b, t, Src::Vertical) * l) >> 7;
    let rl = (sense(b, t, Src::Random) * l) >> 7;
    let r = if rl > 0 { rv % rl } else { 0 };
    let val = (l - pl) + pl * st.p / 255
        - sl * st.spd.min(256) / 256
        - hl * st.h.abs() / 128
        - vl * st.v.abs() / 128
        + r;
    val.clamp(0, l.max(0))
}

/// Größen-/Winkelstufe 0..M (`@7863b0`, 0 ohne wirksame Empfindlichkeit). Gemeinsam für den
/// natürlichen und den simple-Läufer (re 39 §2.2: beide rufen dieselbe Stempel-Funktion); die Stufe
/// wählt die Abtastmatrix der Spitze `s = m + (1 − m)·((M − k)/M)^1,5`
/// ([`crate::brush::transformed_tip`]), nicht einen kleineren Durchmesser.
pub(crate) fn stage(b: &BrushParams, st: &PenState, rng: &mut Minstd) -> usize {
    let m = b.max_count.min(64) as i64;
    if m == 0 {
        return 0;
    }
    let angle_mode = b.row_active(Tgt::Angle);
    let t = if angle_mode { Tgt::Angle } else { Tgt::Size };
    if !b.row_active(t) {
        return 0;
    }
    let xm = |s: Src| (sense(b, t, s) * m) >> 7;
    let (pm, sm, hm, vm, rm) = (
        xm(Src::Pressure),
        xm(Src::Speed),
        xm(Src::Horizontal),
        xm(Src::Vertical),
        xm(Src::Random),
    );
    let r = if rm > 0 { rng.modulo(rm) } else { 0 };
    let v = if angle_mode {
        let hx = (hm * st.h) >> 7;
        let vy = (vm * st.v) >> 7;
        let deg = (vy as f64).atan2(hx as f64).to_degrees().trunc() as i64;
        let dir = ((deg + 360) % 180) * m / 180;
        // Winkelmodus: Druckterm nicht umgekehrt (`@7863b0`)
        pm * st.p / 255 + ((sm * st.spd.min(256)) >> 8) + dir + r
    } else {
        (pm - pm * st.p / 255)
            + ((sm * st.spd.min(256)) >> 8)
            + ((hm * st.h.abs()) >> 7)
            + ((vm * st.v.abs()) >> 7)
            + r
    };
    v.clamp(0, m) as usize
}

/// Werte eines Stempels des simple-Läufers (re 39 §2.2, F26): Der Läufer ruft die Streuung
/// (`@786010`) selbst und den gemeinsamen Stempel (`@788ae0`) wie der natürliche Läufer – Stufe,
/// HLS-Farbjitter (eine Spitze), Deckkraft und Schwärze. Abstand und Fluss liest er nicht.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SimpleStamp {
    /// Streuversatz in px.
    pub ox: i64,
    pub oy: i64,
    pub stage: usize,
    pub color: [u8; 3],
    pub opacity: u8,
    pub black: Option<Blackness>,
}

/// Überlappungszahl n wie die Überlappungstabelle (07 §6.5), für die Schwärze.
pub(crate) fn overlap_count(b: &BrushParams, dp: f64) -> i64 {
    let nov = if b.spacing as f64 * dp > 1000.0 {
        (1000 / b.spacing.max(1)).max(1) as f64
    } else {
        dp
    };
    if nov.is_finite() {
        nov.clamp(1.0, 100.0) as i64
    } else {
        100
    }
}

/// Muss der simple-Läufer Farbe je Pixel führen (Farbjitter oder Schwärze)?
pub(crate) fn simple_needs_rgb(b: &BrushParams) -> bool {
    b.blackness > 0 || b.row_active(Tgt::Hue) || b.row_active(Tgt::Lightness) || b.row_active(Tgt::Saturation)
}

/// Ein Stempel des simple-Läufers in der Zufallsfolge des natürlichen (`@787db0`/`@788ae0`):
/// Streuung mit r0, Versatz mit ra/rb, der Stempel mit eigener Folge ab rb.
pub(crate) fn simple_stamp(
    b: &BrushParams,
    st: &PenState,
    rng: &mut Minstd,
    dp: f64,
    o_base: u8,
    color: [u8; 3],
) -> SimpleStamp {
    let scatter_c = (10.0 * dp) as i64;
    let r0 = rng.next_value() as i64;
    let srow = |s: Src| (sense(b, Tgt::Scatter, s) * scatter_c) >> 7;
    let mut sc = srow(Src::Pressure) * st.p / 255
        - srow(Src::Speed) * st.spd.min(256) / 256
        - srow(Src::Horizontal) * st.h.abs() / 128
        - srow(Src::Vertical) * st.v.abs() / 128;
    let rc = srow(Src::Random);
    if rc > 0 {
        sc += r0 % rc;
    }
    let sc = sc.clamp(0, scatter_c.max(0));
    let ra = rng.next_value() as i64;
    let rb = rng.next_value() as i64;
    let (ox, oy) = if sc != 0 {
        (ra % sc - (sc >> 1), rb % sc - (sc >> 1))
    } else {
        (0, 0)
    };
    let mut local = Minstd { state: rb as i32 };
    let stage = stage(b, st, &mut local);
    let color = jitter_color(b, color, st, Some(&mut local));
    let ro = local.next_value() as i64;
    let opacity = if b.row_active(Tgt::Opacity) {
        sensed(b, Tgt::Opacity, o_base as i64, st, ro).clamp(0, 255) as u8
    } else {
        o_base
    };
    let k_black = (b.blackness.min(1000) * 255 / 1000) as i64;
    let rk = Minstd { state: ro as i32 }.next_value() as i64;
    let kb = if k_black > 0 {
        sensed(b, Tgt::Blackness, k_black, st, rk)
    } else {
        0
    };
    let black = (kb > 0).then(|| {
        let inv = (255 - kb).clamp(0, 255) as u8;
        let q = (51 / overlap_count(b, dp)).clamp(0, 255) as u8;
        let step = |c: u8| mul(c - mul(c, inv), q);
        Blackness {
            dark_g: step(color[0]),
            dark_b: step(color[1]),
            step: step(color[2]),
        }
    });
    SimpleStamp {
        ox,
        oy,
        stage,
        color,
        opacity,
        black,
    }
}

/// Spitzen je Stufe (Größe bzw. Winkel), zwischengespeichert.
struct TipSet {
    dp: f64,
    round: bool,
    soft: u32,
    bell: bool,
    aspect: u32,
    angle: u32,
    min_size: u32,
    m: usize,
    angle_mode: bool,
    tips: HashMap<usize, Tip>,
}

impl TipSet {
    fn get(&mut self, stage: usize) -> &Tip {
        let (dp, round, soft, bell) = (self.dp, self.round, self.soft, self.bell);
        let st = (stage as u32, self.m as u32);
        let t = TipTransform {
            aspect_permille: self.aspect,
            angle_deg: self.angle,
            size_stage: (!self.angle_mode).then_some((st.0, st.1, self.min_size)),
            angle_stage: self.angle_mode.then_some(st),
        };
        self.tips
            .entry(stage)
            .or_insert_with(|| transformed_tip(dp, round, soft, bell, &t))
    }
}

/// ARGB-Strichpuffer der natürlichen Pinsel.
pub(crate) struct ArgbInk<'a> {
    pub(crate) base: InkBuffer<'a>,
    rgb: Vec<[u8; 3]>,
    /// Rechteck des aktuellen Stempels (links, oben, rechts, unten), außerhalb wird nicht gestempelt.
    clip: Option<(i64, i64, i64, i64)>,
    /// Schwärze des aktuellen Stempels.
    black: Option<Blackness>,
}

/// Schwärze je Stempel (Bytes 7–9 des Pixelparameterblocks von `@6f8e10`): Dunkelwerte Grün/Blau
/// und Schritt.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Blackness {
    dark_g: u8,
    dark_b: u8,
    step: u8,
}

/// Jeder Stempel wird auf sein eigenes Rechteck `[(p16)/16, p16/16 + w)` begrenzt (A, T2 „Dashed“).
const PER_STAMP_CLIP: bool = true;

impl<'a> ArgbInk<'a> {
    /// An ink made of a kept stroke's buffers (a live stroke).
    pub(crate) fn from_parts(base: InkBuffer<'a>, rgb: Vec<[u8; 3]>) -> Self {
        Self {
            base,
            rgb,
            clip: None,
            black: None,
        }
    }

    pub(crate) fn into_parts(self) -> (InkBuffer<'a>, Vec<[u8; 3]>) {
        (self.base, self.rgb)
    }
}

impl ArgbInk<'_> {
    /// Leerer Puffer über `rect` (Farbe weiß, Alpha 0).
    pub(crate) fn new(rect: IRect) -> Self {
        ArgbInk {
            base: InkBuffer::new(rect),
            rgb: vec![[255; 3]; rect.width() as usize * rect.height() as usize],
            clip: None,
            black: None,
        }
    }

    /// Schwärze des nächsten Stempels setzen.
    pub(crate) fn set_black(&mut self, b: Option<Blackness>) {
        self.black = b;
    }

    /// Farbe je Pixel.
    pub(crate) fn into_out(mut self) -> InkBufferOut {
        self.base.clip_to_bounds();
        InkBufferOut {
            data: self.base.data,
            rgb: Some(self.rgb),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn stamp(
        &mut self,
        x: i64,
        y: i64,
        m: &Tip,
        o: u8,
        cap: bool,
        color: [u8; 3],
        tex: Option<(&TexLookup, u32, u32)>,
    ) {
        let (x0, y0) = (self.base.rect.left as i64, self.base.rect.top as i64);
        let (w, h) = (self.base.w as i64, self.base.h as i64);
        // Stempel ganz außerhalb des Fensters: nichts zu tun (Teilfenster der Live-Vorschau)
        if x >= x0 + w || y >= y0 + h || x + m.n as i64 <= x0 || y + m.n as i64 <= y0 {
            return;
        }
        // Only the part of the tip inside the window.
        let n = m.n as i64;
        let (j0, j1) = ((y0 - y).max(0), (y0 + h - y).min(n));
        let (i0, i1) = ((x0 - x).max(0), (x0 + w - x).min(n));
        // Plain slices and a while loop: per-pixel iterator and index calls dominate a debug build.
        let tip: &[u8] = &m.data;
        let tip_n = m.n;
        let clip = self.clip;
        for j in j0 as usize..j1 as usize {
            let yy = y + j as i64 - y0;
            let mut column = i0 as usize;
            while column < i1 as usize {
                let i = column;
                column += 1;
                let mut v = tip[j * tip_n + i];
                if v == 0 {
                    continue;
                }
                let xx = x + i as i64 - x0;
                if let Some((cx0, cy0, cx1, cy1)) = clip {
                    let (dx, dy) = (x0 + xx, y0 + yy);
                    if dx < cx0 || dx >= cx1 || dy < cy0 || dy >= cy1 {
                        continue;
                    }
                }
                if let Some((t, btb, bte)) = tex {
                    v = textured_mask(v, t.value((x0 + xx) as i32, (y0 + yy) as i32), btb, bte);
                    if v == 0 {
                        continue;
                    }
                }
                let k = (yy * w + xx) as usize;
                let sa = mul(v, o);
                if sa == 0 {
                    continue;
                }
                let da = self.base.data[k];
                let mut a2 = da as u32 + sa as u32 - mul(da, sa) as u32;
                if cap {
                    a2 = a2.min(o as u32);
                }
                let a2 = a2.min(255) as u8;
                if let Some(bk) = self.black {
                    // Schwärze-Tinte (`@6eaa20`): erst wie üblich mischen (W = DIV[sa][a′] mit
                    // ungedeckeltem a′), dann je Kanal mit Ziel ≤ Quelle: min(Mischwert,
                    // max(Ziel − mul(Schritt, Maske), Dunkelwert)). Schritt und Dunkelwerte kommen als
                    // einzelne Bytes, daher wirkt der Schritt nur auf Blau, der Rot-Dunkelwert ist das
                    // gepackte RGB (wirkungslos), Grün steigt nicht über den Zielwert.
                    let au = (da as u32 + sa as u32 - mul(da, sa) as u32).min(255) as usize;
                    let w = DIV[sa as usize][au] as i32;
                    let dst = self.rgb[k];
                    let mixed: [i32; 3] = std::array::from_fn(|c| {
                        let (s, d) = (color[c] as i32, dst[c] as i32);
                        let x = s * w - d * w;
                        (((s * w) >> 8) - ((d * w) >> 8) + (d << 8) + 127 + x) >> 8
                    });
                    // gepackte Multiplikation mit +0x7f (`@6eaa20`)
                    let sv = bk.step as i32 * v as i32;
                    let step = (sv + (sv >> 8) + 0x7f) >> 8;
                    let mut out = mixed;
                    if dst[1] <= color[1] {
                        out[1] = mixed[1].min((dst[1] as i32).max(bk.dark_g as i32));
                    }
                    if dst[2] <= color[2] {
                        out[2] = mixed[2].min((dst[2] as i32 - step).max(bk.dark_b as i32));
                    }
                    self.rgb[k] = out.map(|c| c.clamp(0, 255) as u8);
                } else if da == 0 || (v == 255 && o == 255) {
                    // direkt nur bei voller Maske und voller Deckkraft (`@6f9380`), sonst mischen
                    self.rgb[k] = color;
                } else if a2 > 0 {
                    let wgt = DIV[sa as usize][a2 as usize] as i32;
                    for (d, &s) in self.rgb[k].iter_mut().zip(&color) {
                        let (sc, dc) = (s as i32, *d as i32);
                        let x = (sc - dc) * wgt + 128 + 255 * dc;
                        *d = ((x + (x >> 8)) >> 8).clamp(0, 255) as u8;
                    }
                }
                self.base.data[k] = a2;
            }
        }
    }
}

/// Versätze mehrerer Spitzen (`@785530`, einmal je Strich) in px: `S = ⌊BTSi·D′/100⌋` (ganzzahlig),
/// zufällig `(rnd % S − S>>1, rnd % S − S>>1)` mit eigener Folge ab dem Startwert, diagonal
/// `trunc(−S/2 + i·S/(n−1))`, kreisförmig `trunc(cos a·S), trunc(sin a·S)` mit a ab 5π/4 in Schritten
/// von 2π/(n−1), letzte Spitze (0, 0).
fn tip_offsets(b: &BrushParams, dp: f64, seed: u32) -> Vec<(i64, i64)> {
    let n = b.tip_count.clamp(1, 32) as usize;
    let s = (b.tip_spacing as i64 * dp as i64) / 100;
    if n < 2 || s <= 0 {
        return vec![(0, 0); n];
    }
    match b.tip_spacing_mode {
        TipSpacingMode::Random => {
            let mut rng = Minstd::new(seed);
            let half = s >> 1;
            (0..n)
                .map(|_| {
                    let x = rng.next_value() as i64 % s - half;
                    let y = rng.next_value() as i64 % s - half;
                    (x, y)
                })
                .collect()
        }
        TipSpacingMode::Diagonal => {
            let inc = s as f64 / (n - 1) as f64;
            let mut v = s as f64 * -0.5;
            (0..n)
                .map(|_| {
                    let t = v.trunc() as i64;
                    v += inc;
                    (t, t)
                })
                .collect()
        }
        TipSpacingMode::Circular => {
            let k = n - 1;
            let step = std::f64::consts::TAU / k as f64;
            let mut a = 3.926_990_816_987_241_4;
            let mut out: Vec<(i64, i64)> = (0..k)
                .map(|_| {
                    let (sn, cs) = crate::brush::sin_cos_x87(a);
                    let p = ((cs * s as f64).trunc() as i64, (sn * s as f64).trunc() as i64);
                    a -= step;
                    p
                })
                .collect();
            out.push((0, 0));
            out
        }
    }
}

/// Farben der Spitzen (`@786f30`, einmal je Strich). Bei einer Spitze leer (dann je Stempel
/// [`jitter_color`]). Gleichmäßig: Grundfarbe; komplementär: Farbton `fmod(H + (i+1)·255/n, 255)`;
/// Farbton: abwechselnd H − 13 (gerade i, unter 0 + 255) und `fmod(H + 13, 255)`; Schatten:
/// abwechselnd L + 26 (höchstens 255) und L − 26 (mindestens 0); zufällig: je Spitze
/// [`jitter_color`] ohne Stiftwerte mit eigener Folge ab dem Startwert. Die letzte Spitze behält
/// außer bei „zufällig“ die Grundfarbe; Kanäle abgeschnitten.
fn tip_colors(b: &BrushParams, base: [u8; 3], seed: u32) -> Vec<[u8; 3]> {
    let n = b.tip_count.clamp(1, 32) as usize;
    if n <= 1 {
        return vec![base];
    }
    let (h, l, s) = rgb_to_hls(base);
    let mut out = vec![base; n];
    match b.tip_coloring_mode {
        TipColoringMode::Uniform => {}
        TipColoringMode::Complementary => {
            let step = 255.0 / n as f64;
            for (i, o) in out.iter_mut().take(n - 1).enumerate() {
                *o = hls_to_rgb(((i + 1) as f64 * step + h) % 255.0, l, s);
            }
        }
        TipColoringMode::Hue => {
            let mut h1 = h - 13.0;
            if h1 < 0.0 {
                h1 += 255.0;
            }
            let c_even = hls_to_rgb(h1, l, s);
            let c_odd = hls_to_rgb((h + 13.0) % 255.0, l, s);
            for (i, o) in out.iter_mut().take(n - 1).enumerate() {
                *o = if i % 2 == 1 { c_odd } else { c_even };
            }
        }
        TipColoringMode::Shadow => {
            let c_even = hls_to_rgb(h, (l + 26.0).min(255.0), s);
            let c_odd = hls_to_rgb(h, (l - 26.0).max(0.0), s);
            for (i, o) in out.iter_mut().take(n - 1).enumerate() {
                *o = if i % 2 == 1 { c_odd } else { c_even };
            }
        }
        TipColoringMode::Random => {
            let mut rng = Minstd::new(seed);
            let st = PenState::default();
            for o in out.iter_mut() {
                *o = jitter_color_with(b, base, &st, Some(&mut rng), true);
            }
        }
    }
    out
}

/// Farbjitter (`@785b00`): mit Zufallsfolge immer drei Schritte r1, r2, r3; nur wenn eine der
/// Zeilen Farbton/Helligkeit/Sättigung aktiv ist, wird die Farbe verändert (Gewichte
/// `Zeilenbyte·255 >> 7` für Farbton und Sättigung, für Helligkeit `Byte·L >> 7` bzw.
/// `Byte·(255 − L) >> 7`):
/// - `H′ = fmod(H + ⌊Hp·p/255⌋ + ⌊Hs·spd/256⌋ + (Hh·|h| >> 7) + (Hv·|v| >> 7) + r1 % Hr − (Hr >> 1) + 256, 256)`
/// - `S′ = clamp(S + ⌊Sp·p/255⌋ − ⌊Ss·spd/256⌋ − (Sh·|h|) >> 7 − (Sv·|v|) >> 7 + r2 % Sr, 0, 255)`
/// - `L′ = clamp(L − ⌊Lp·p/255⌋ + ⌊Ls·spd/256⌋ + (Lh·|h| >> 7) + (Lv·|v| >> 7) + r3 % Lr − (Lr >> 1), 0, 255)`
fn jitter_color(b: &BrushParams, base: [u8; 3], st: &PenState, rng: Option<&mut Minstd>) -> [u8; 3] {
    jitter_color_with(b, base, st, rng, false)
}

/// `raw_random`: the tips' colours (`@786f30`) take the random hue range as the row byte itself,
/// not `byte·255 >> 7` (Fireworks MX, measured on folds7's Outline stroke: four tips, random
/// colours, hue 703‰ = byte 90; its three visible tips are 172°, 185° and 204° from #3399cc,
/// within the glow's tint of what Fireworks MX shows, where `·255 >> 7` gives 91°, 186° and 290°).
/// Saturation keeps the scaled weight (lines3's Yarn fits it better); single-tip strokes keep
/// FirePixel's runtime-verified formula.
fn jitter_color_with(
    b: &BrushParams,
    base: [u8; 3],
    st: &PenState,
    rng: Option<&mut Minstd>,
    raw_random: bool,
) -> [u8; 3] {
    let (r1, r2, r3) = match rng {
        Some(r) => (
            r.next_value() as i64,
            r.next_value() as i64,
            r.next_value() as i64,
        ),
        None => (0, 0, 0),
    };
    if !(b.row_active(Tgt::Hue) || b.row_active(Tgt::Lightness) || b.row_active(Tgt::Saturation)) {
        return base;
    }
    let raw = |t: Tgt, s: Src| sense(b, t, s);
    let w = |t: Tgt, s: Src| {
        if raw_random && s == Src::Random && t == Tgt::Hue {
            raw(t, s)
        } else {
            (raw(t, s) * 255) >> 7
        }
    };
    let spd = st.spd.min(256);
    let (ah, av) = (st.h.abs(), st.v.abs());
    let (h, l, s) = rgb_to_hls(base);
    // Farbton
    let hr = w(Tgt::Hue, Src::Random);
    let rt_h = if hr != 0 {
        (r1 % hr - (hr >> 1)) as f64
    } else {
        0.0
    };
    let ph = w(Tgt::Hue, Src::Pressure) * st.p / 255;
    let sh = w(Tgt::Hue, Src::Speed) * spd / 256;
    let hh = (w(Tgt::Hue, Src::Horizontal) * ah) >> 7;
    let vh = (w(Tgt::Hue, Src::Vertical) * av) >> 7;
    let h2 = (ph as f64 + h + sh as f64 + hh as f64 + vh as f64 + (rt_h + 256.0)) % 256.0;
    // Sättigung
    let sr = w(Tgt::Saturation, Src::Random);
    let rt_s = if sr != 0 { (r2 % sr) as f64 } else { 0.0 };
    let ps = w(Tgt::Saturation, Src::Pressure) * st.p / 255;
    let ss = -(w(Tgt::Saturation, Src::Speed) * spd / 256);
    let hs = (-(w(Tgt::Saturation, Src::Horizontal) * ah)) >> 7;
    let vs = (-(w(Tgt::Saturation, Src::Vertical) * av)) >> 7;
    let s2 = (rt_s + (ps as f64 + s + ss as f64 + hs as f64 + vs as f64)).clamp(0.0, 255.0);
    // Helligkeit
    let lr = w(Tgt::Lightness, Src::Random);
    let rt_l = if lr != 0 {
        (r3 % lr - (lr >> 1)) as f64
    } else {
        0.0
    };
    let inv = (255.0 - l).trunc() as i64;
    let li = l.trunc() as i64;
    let pl = -(((raw(Tgt::Lightness, Src::Pressure) * li) >> 7) * st.p / 255);
    let sl = (((raw(Tgt::Lightness, Src::Speed) * inv) >> 7) * spd) / 256;
    let hl = (((raw(Tgt::Lightness, Src::Horizontal) * inv) >> 7) * ah) >> 7;
    let vl = (((raw(Tgt::Lightness, Src::Vertical) * inv) >> 7) * av) >> 7;
    let l2 = (rt_l + (l + pl as f64 + sl as f64 + hl as f64 + vl as f64)).clamp(0.0, 255.0);
    hls_to_rgb(h2, l2, s2)
}

/// RGB → HLS wie `@6d0930` (doppelte Genauigkeit): Farbton auf 0..255 (= 0..360°), L und S 0..255.
fn rgb_to_hls(c: [u8; 3]) -> (f64, f64, f64) {
    const INV255: f64 = 0.003_921_568_627_450_98;
    let (r, g, b) = (c[0] as f64 * INV255, c[1] as f64 * INV255, c[2] as f64 * INV255);
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let l = (mx + mn) * 0.5;
    if mx == mn {
        return (0.0, (l * 255.0).clamp(0.0, 255.0), 0.0);
    }
    let d = mx - mn;
    let s = if l <= 0.5 {
        d / (mx + mn)
    } else {
        d / (2.0 - mx - mn)
    };
    let mut h = if r == mx {
        (g - b) / d
    } else if g == mx {
        2.0 + (b - r) / d
    } else {
        4.0 + (r - g) / d
    };
    h *= 60.0;
    if h < 0.0 {
        h += 360.0;
    }
    let hv = h * 0.002_777_777_777_777_778 * 255.0;
    (
        hv.clamp(0.0, 255.0),
        (l * 255.0).clamp(0.0, 255.0),
        (s * 255.0).clamp(0.0, 255.0),
    )
}

/// HLS → RGB wie `@6cf420`/`@6cf380`, Kanäle abgeschnitten (Rundungsart 0xC00).
fn hls_to_rgb(h: f64, l: f64, s: f64) -> [u8; 3] {
    const INV255: f64 = 0.003_921_568_627_450_98;
    let hd = h * INV255 * 360.0;
    let (l, s) = (l * INV255, s * INV255);
    let cut = |v: f64| (v * 255.0).clamp(0.0, 255.0).trunc() as u8;
    if s == 0.0 {
        let v = cut(l);
        return [v, v, v];
    }
    let m2 = if 0.5 < l { l + s - l * s } else { l * (1.0 + s) };
    let m1 = l + l - m2;
    let value = |hue: f64| {
        let mut hue = hue;
        if hue > 360.0 {
            hue -= 360.0;
        } else if hue < 0.0 {
            hue += 360.0;
        }
        if hue < 60.0 {
            m1 + hue * (m2 - m1) * 0.016_666_666_666_666_666
        } else if hue < 180.0 {
            m2
        } else if hue < 240.0 {
            m1 + (240.0 - hue) * (m2 - m1) * 0.016_666_666_666_666_666
        } else {
            m1
        }
    };
    [cut(value(hd + 120.0)), cut(value(hd)), cut(value(hd - 120.0))]
}

/// Skalierung `VELf` → Geschwindigkeit des Flusses (`len·256/spd`); A, aus den Laufzeitdateien T2
/// abgeleitet (`VELf = 0,125` bei per Skript erzeugten Pfaden).
pub const VELOCITY_SCALE: f64 = 256.0;

/// Geschwindigkeit der Empfindlichkeitsformeln (`spd`, 1/256 px/ms) aus `VELf` in px/ms (re\34
/// §2.2): `spd = round(VELf·256)`, `VELf ≤ 0` ⇒ 0, `VELf ≥ 255,996` ⇒ 0xFF (Eigenheit des
/// Lesers), nach oben sonst offen. Die Formeln des Läufers begrenzen mit `min(spd, 256)`; Punkte
/// ohne eigene Dynamik haben `VELf` 0,125 ⇒ `spd` 32.
pub fn sensitivity_speed(velf: f32) -> f64 {
    let v = velf as f64;
    if v.is_nan() || v <= 0.0 {
        0.0
    } else if v >= 255.996_093_75 {
        255.0
    } else {
        (v * VELOCITY_SCALE).round()
    }
}

/// Spitzenfaktor bei 100 % „Größe ← Geschwindigkeit“ (`1 − min(spd, 256)/256`).
pub fn speed_size_factor(velf: f32) -> f64 {
    1.0 - sensitivity_speed(velf).min(256.0) / 256.0
}

/// Strichpuffer eines natürlichen Pinsels (A in den Details, siehe Moduldoku).
pub(crate) fn natural_ink(
    path: &VectorPath,
    stroke: &Stroke,
    placement: StrokePlacement,
    rect: IRect,
) -> InkBufferOut {
    let mut ink = ArgbInk {
        base: InkBuffer::new(rect),
        rgb: vec![[255; 3]; rect.width() as usize * rect.height() as usize],
        clip: None,
        black: None,
    };
    natural_run(path, stroke, placement, &mut ink, None);
    ink.base.clip_to_bounds();
    InkBufferOut {
        data: ink.base.data,
        rgb: Some(ink.rgb),
    }
}

/// What a natural brush's walk carries from one piece of a live stroke to the next.
#[derive(Clone, Debug)]
pub(crate) struct NaturalResume {
    acc: i64,
    flow: i64,
    st: PenState,
    dash: NaturalDash,
}

/// Stamp `path` into `ink`. With `resume`, `path` is the next piece of one open contour.
pub(crate) fn natural_run(
    path: &VectorPath,
    stroke: &Stroke,
    placement: StrokePlacement,
    ink: &mut ArgbInk<'_>,
    mut resume: Option<&mut Option<NaturalResume>>,
) {
    let b = &stroke.brush;
    let dp = effective_diameter(stroke, placement);
    let aa = b.antialiased;
    let m = b.max_count.min(64) as usize;
    let tip_key = crate::tipcache::TipKey {
        natural: true,
        dp,
        round: b.shape == TipShape::Circle,
        soft: b.softness,
        bell: b.bell_curve,
        aspect: b.aspect,
        angle: b.angle,
        min_size: b.min_size,
        m,
        angle_mode: b.row_active(Tgt::Angle),
    };
    let crate::tipcache::Tips { stages, mut shifted } = crate::tipcache::take(tip_key);
    let mut tips = TipSet {
        dp,
        round: b.shape == TipShape::Circle,
        soft: b.softness,
        bell: b.bell_curve,
        aspect: b.aspect,
        angle: b.angle,
        min_size: b.min_size,
        m,
        angle_mode: b.row_active(Tgt::Angle),
        tips: stages,
    };
    let n_tip = tips.get(0).n as i64 - 1;
    let tipw = n_tip as f64;
    let o_base =
        crate::support::color::brush_stamp_opacity(b.concentration, feedback_code(b.feedback), b.spacing, dp)
            as i64;
    let cap = b.feedback != Feedback::Brush;
    let k_black = (b.blackness.min(1000) * 255 / 1000) as i64;
    // Überlappungszahl n wie die Überlappungstabelle (07 §6.5)
    let n_overlap = {
        let nov = if b.spacing as f64 * dp > 1000.0 {
            (1000 / b.spacing.max(1)).max(1) as f64
        } else {
            dp
        };
        if nov.is_finite() {
            nov.clamp(1.0, 100.0) as i64
        } else {
            100
        }
    };
    let color = [stroke.color.r(), stroke.color.g(), stroke.color.b()];
    let spacing16 = ((b.spacing as f64 * dp / 1000.0).max(1.0) * 65536.0) as i64;
    let flow_threshold = 1001 - b.flow_rate.min(1000) as i64;
    let tex = stroke
        .texture
        .as_ref()
        .filter(|_| b.texture_blend > 0 || b.texture_edge > 0)
        .map(|t| TexLookup::new(&t.mask, path.texture_offset));
    let half = (n_tip as f64 * 0.5 * 16.0) as i64;
    let scatter_c = (10.0 * dp) as i64;
    for c in &path.contours {
        if c.points.is_empty() {
            continue;
        }
        let seed = path.random_seed;
        let mut rng: Minstd;
        // Je Strich einmal (`@789060`): Versätze (`@785530`) und Farben (`@786f30`) der Spitzen
        let offs = tip_offsets(b, dp, seed);
        let tip_cols = tip_colors(b, color, seed);
        let mut seq: Vec<(Vec2, crate::attrs::PointDynamics)> = c
            .points
            .iter()
            .enumerate()
            .map(|(i, p)| (*p, c.dynamics_at(i)))
            .collect();
        if c.closed && seq.len() > 1 {
            seq.push(seq[0]);
        }
        if seq.len() == 1 {
            seq.push(seq[0]);
        }
        let continued = resume.as_deref_mut().and_then(Option::take);
        let resumed = continued.is_some();
        let (mut acc, mut flow, mut st, mut dash) = match continued {
            Some(r) => (r.acc, r.flow, r.st, r.dash),
            None => (spacing16, 0i64, PenState::default(), NaturalDash::new(b)),
        };
        for (si, w) in seq.windows(2).enumerate() {
            let ((p, dp0), (q, dp1)) = (w[0], w[1]);
            // Der Läufer arbeitet je Segment mit eigener Folge ab RNDi + 1 des Startpunkts
            // (Laufzeit phase5 `p_zufall`: Farbton-Zufall 26/26 Stempel)
            rng = Minstd::new(dp0.seed.wrapping_add(1));
            let first = si == 0 && !resumed;
            let a = ((p.x * 16.0).trunc() as i64, (p.y * 16.0).trunc() as i64);
            let bb = ((q.x * 16.0).trunc() as i64, (q.y * 16.0).trunc() as i64);
            let (dx, dy) = ((bb.0 - a.0).abs(), (bb.1 - a.1).abs());
            let swap = dx < dy;
            let (major, minor) = if swap { (dy, dx) } else { (dx, dy) };
            if major == 0 && !first {
                continue;
            }
            let (smj, smn) = if swap {
                (if bb.1 > a.1 { 1 } else { -1 }, if bb.0 > a.0 { 1 } else { -1 })
            } else {
                (if bb.0 > a.0 { 1 } else { -1 }, if bb.1 > a.1 { 1 } else { -1 })
            };
            let (mut mj, mut mn) = if swap { (a.1, a.0) } else { (a.0, a.1) };
            let len = hypot_cr(q.x - p.x, q.y - p.y);
            let n = major.max(1);
            let inc = ((len * 65536.0).trunc() as i64) / n;
            // Flusszuwachs je Schritt: min(1, ⌊len·256/Geschwindigkeit⌋ / n), Geschwindigkeit des
            // Startpunkts (0 ⇒ kein Fluss)
            let spd_a = (dp0.velocity.max(0.0) as f64 * VELOCITY_SCALE) as i64;
            let flow_inc = if spd_a > 0 {
                (((len * 256.0) / spd_a as f64).trunc() as i64 / n).clamp(0, 1)
            } else {
                0
            };
            if len > 0.0 {
                let dirx = ((q.x - p.x) * 128.0 / len).trunc() as i64;
                let diry = ((q.y - p.y) * 128.0 / len).trunc() as i64;
                st.h += (dirx >> 3) - (st.h >> 3);
                st.v += (diry >> 3) - (st.v >> 3);
            }
            let p0 = (dp0.pressure.clamp(0.0, 1.0) as f64 * 255.0) as i64;
            let p1 = (dp1.pressure.clamp(0.0, 1.0) as f64 * 255.0) as i64;
            let v0 = sensitivity_speed(dp0.velocity);
            let v1 = sensitivity_speed(dp1.velocity);
            let mut err = 2 * minor - major;
            for step in 0..n {
                let f = step as f64 / n as f64;
                st.p = p0 + ((p1 - p0) as f64 * f) as i64;
                st.spd = (v0 + (v1 - v0) * f) as i64;
                // Läufer (`@787db0`): gestempelt wird, wenn der Abstand erreicht ist oder (bei
                // Fluss ≠ 0) der Fluss die Schwelle erreicht; nach jedem Stempel Abstand abziehen und
                // Fluss um die Schwelle senken bzw. auf 0 setzen.
                let events = (acc >= spacing16 || (b.flow_rate > 0 && flow >= flow_threshold)) as usize;
                if events > 0 {
                    if acc >= spacing16 {
                        acc -= spacing16;
                    }
                    if flow >= flow_threshold {
                        flow -= flow_threshold;
                    } else {
                        flow = 0;
                    }
                }
                let on = dash.step(events > 0);
                for _ in 0..events {
                    if !on {
                        continue;
                    }
                    let (x16, y16) = if swap { (mn, mj) } else { (mj, mn) };
                    let ntips = offs.len();
                    for (ti, (tx, ty)) in offs.iter().enumerate() {
                        // Läufer (`@787db0`): Streuung mit dem nächsten Zufallswert, zwei weitere für
                        // den Versatz; der Stempel erhält den letzten Wert als lokale Kopie.
                        let r0 = rng.next_value() as i64;
                        let srow = |s: Src| (sense(b, Tgt::Scatter, s) * scatter_c) >> 7;
                        let mut sc = srow(Src::Pressure) * st.p / 255
                            - srow(Src::Speed) * st.spd.min(256) / 256
                            - srow(Src::Horizontal) * st.h.abs() / 128
                            - srow(Src::Vertical) * st.v.abs() / 128;
                        let rc = srow(Src::Random);
                        if rc > 0 {
                            sc += r0 % rc;
                        }
                        let sc = sc.clamp(0, scatter_c.max(0));
                        let ra = rng.next_value() as i64;
                        let rb = rng.next_value() as i64;
                        let (ox, oy) = if sc != 0 {
                            (ra % sc - (sc >> 1), rb % sc - (sc >> 1))
                        } else {
                            (0, 0)
                        };
                        // Stempel (`@788ae0`) mit lokaler Zufallsfolge ab rb
                        let mut local = Minstd { state: rb as i32 };
                        let stg = stage(b, &st, &mut local);
                        let col = if ntips == 1 {
                            jitter_color(b, color, &st, Some(&mut local))
                        } else {
                            tip_cols[ti]
                        };
                        let ro = local.next_value() as i64;
                        let o = sensed(b, Tgt::Opacity, o_base, &st, ro).clamp(0, 255) as u8;
                        let rk = Minstd { state: ro as i32 }.next_value() as i64;
                        let kb = if k_black > 0 {
                            sensed(b, Tgt::Blackness, k_black, &st, rk)
                        } else {
                            0
                        };
                        // `@6ea860`: Dunkelwert mul(c, 255 − K), Schritt mul(c − dunkel, ⌊51/n⌋) je Kanal.
                        // `@6f8e10` reicht die Bytes verschoben weiter: Untergrenze Grün = Schritt Rot,
                        // Untergrenze Blau = Schritt Grün, Schritt = Schritt Blau (nur Blau wirkt).
                        ink.black = (kb > 0).then(|| {
                            let inv = (255 - kb).clamp(0, 255) as u8;
                            let q = (51 / n_overlap).clamp(0, 255) as u8;
                            let step = |c: u8| mul(c - mul(c, inv), q);
                            Blackness {
                                dark_g: step(col[0]),
                                dark_b: step(col[1]),
                                step: step(col[2]),
                            }
                        });
                        let mut px16 = x16 + (ox + tx) * 16 - half;
                        let mut py16 = y16 + (oy + ty) * 16 - half;
                        ink.base.note_bounds(px16, py16, tipw);
                        if !aa {
                            if n_tip % 2 == 1 {
                                px16 += 8;
                                py16 += 8;
                            }
                            let x = (px16 as f64 / 16.0 + 0.5).floor() as i64;
                            let y = (py16 as f64 / 16.0 + 0.5).floor() as i64;
                            // Stempelrechteck `@788ae0`: [x, x + N) ohne Kantenglättung
                            if PER_STAMP_CLIP {
                                ink.clip = Some((x, y, x + n_tip, y + n_tip));
                            }
                            let tip = tips.get(stg);
                            ink.stamp(
                                x,
                                y,
                                tip,
                                o,
                                cap,
                                col,
                                tex.as_ref().map(|t| (t, b.texture_blend, b.texture_edge)),
                            );
                        } else {
                            // mit Kantenglättung [p16 >> 4, + N + 1)
                            if PER_STAMP_CLIP {
                                let (cx, cy) = (px16 >> 4, py16 >> 4);
                                ink.clip = Some((cx, cy, cx + n_tip + 1, cy + n_tip + 1));
                            }
                            let key = (stg, (px16 & 15) as usize, (py16 & 15) as usize);
                            let mk = shifted
                                .entry(key)
                                .or_insert_with(|| shift_tip(tips.get(stg), key.1, key.2));
                            ink.stamp(
                                px16 >> 4,
                                py16 >> 4,
                                mk,
                                o,
                                cap,
                                col,
                                tex.as_ref().map(|t| (t, b.texture_blend, b.texture_edge)),
                            );
                        }
                    }
                }
                if err >= 0 {
                    mn += smn;
                    err += 2 * (minor - major);
                } else {
                    err += 2 * minor;
                }
                acc += inc;
                flow += flow_inc;
                mj += smj;
            }
        }
        if let Some(r) = resume.as_deref_mut() {
            *r = Some(NaturalResume { acc, flow, st, dash });
        }
    }
    crate::tipcache::keep(
        tip_key,
        crate::tipcache::Tips {
            stages: tips.tips,
            shifted,
        },
    );
}

/// Strichmuster der natürlichen Pinsel (07 §6.7): an, solange Zähler < DO + 1; Aus-Schwelle
/// `(D + DF)·10`, im Aus-Zustand zählt jeder 1/16-Schritt.
#[derive(Clone, Debug)]
struct NaturalDash {
    d: i64,
    pairs: Vec<(i64, i64)>,
    on: bool,
    cnt: i64,
    idx: usize,
}

impl NaturalDash {
    fn new(b: &BrushParams) -> Self {
        Self {
            d: b.diameter as i64,
            pairs: b
                .dashes
                .iter()
                .take(3)
                .map(|&(a, c)| (a as i64, c as i64))
                .collect(),
            on: true,
            cnt: 0,
            idx: 0,
        }
    }

    /// Ein 1/16-Schritt; `event` = in diesem Schritt fällt ein Stempel-Ereignis. Liefert „an“.
    fn step(&mut self, event: bool) -> bool {
        if self.pairs.is_empty() {
            return true;
        }
        let (on_s, off_s) = self.pairs[self.idx];
        if self.on {
            if event {
                self.cnt += 1;
                if self.cnt > on_s {
                    self.cnt = 0;
                    self.on = false;
                }
            }
        } else {
            self.cnt += 1;
            if self.cnt >= (self.d + off_s) * 10 {
                self.cnt = 0;
                self.on = true;
                self.idx = (self.idx + 1) % self.pairs.len();
            }
        }
        self.on
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn random_tip_colours_stay_near_the_stroke_colour() {
        let mut b = BrushParams {
            tip_count: 4,
            tip_coloring_mode: TipColoringMode::Random,
            ..BrushParams::default()
        };
        b.sense[Tgt::Hue as usize][Src::Random as usize] = BrushParams::sense_from_permille(703);
        let cols = tip_colors(&b, [0x33, 0x99, 0xcc], 0xf314_2f77);
        let hues: Vec<i64> = cols
            .iter()
            .take(3)
            .map(|&c| (rgb_to_hls(c).0 * 360.0 / 255.0).round() as i64)
            .collect();
        // Fireworks MX draws folds7's outline 170-174°, 186° and about 195° (under a red glow).
        assert_eq!(hues, [172, 185, 204]);
    }

    #[test]
    fn remap_tables() {
        let t = alpha_remap_table(1, 1000);
        assert_eq!(t[0], 0);
        assert!(t[100] > 100);
        let id = alpha_remap_table(0, 1000);
        assert_eq!(id[77], 77);
        assert_eq!(textured_mask(200, 255, 0, 0), 200);
        assert_eq!(textured_mask(200, 0, 1000, 0), 0);
        let (h, l, s) = rgb_to_hls([192, 64, 64]);
        assert_eq!(hls_to_rgb(h, l, s), [192, 64, 64]);
    }
}
