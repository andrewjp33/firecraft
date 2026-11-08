//! Farbwähler-Popup wie im Original (Nutzerbefund N27b, Messung `re\22-ansicht-bitmap-farbe.md` §2).
//!
//! Ein gemeinsames Popup für **alle** Farbfelder (Werkzeugleiste, Inspektor, Effekte, Filter- und
//! andere Dialoge, Bedienfelder, Bildvorschau): 259 × 277, festes Raster 21 × 15 (Formel §2.2),
//! darunter Vorschau, HEX mit Kopierknopf, Deckkraft mit Klappschieber, RGB mit Kopierknopf und eine
//! Fußleiste je Aufrufart ([`Footer`]). Verhalten wie §2.3:
//! - Pipette ab dem Öffnen: Vorschau, HEX und RGB zeigen live die Farbe unter dem Zeiger (Raster,
//!   Leinwand, übriges App-Fenster); ein Klick außerhalb des Popups übernimmt sie und schließt.
//! - Klick auf eine Rasterzelle übernimmt und schließt; Esc verwirft, Enter übernimmt Getipptes.
//! - Lage: Unterkante auf der Oberkante des Farbfelds, linke Kante beim Klickpunkt; reicht der
//!   Platz oben nicht, am oberen Rand.
//! - Farbkreis: Farbdialog (eigener Dialog statt des Windows-Dialogs, siehe [`sys_dialog`]).
//!
//! Rahmen, Fläche und Schrift kommen aus dem Firepixel-Design; Inhalt und Aufbau wie im Original.
//!
//! FireCraft: from FirePixel's `fp-ui/src/inspector/color_picker.rs` (Apache-2.0). Changes: the
//! eyedropper reads FireCraft's own window only (no screen reading outside it); the canvas
//! sampling hook ([`Sampler`]) replaces FirePixel's app state; FirePixel's selection-preview
//! tracking and dialog-dimming correction are left out; screenshots come through egui 0.37's
//! callback; a crosshair cursor while picking; [`color_box`] draws FireCraft's swatch.
#![allow(dead_code)] // FirePixel helpers FireCraft doesn't call (tests, other dialogs)

use super::fields;
use super::glyphs::Glyph;
use super::texts;
use super::theme::Theme;
use egui::{
    Align2, Color32, Id, Key, LayerId, Modifiers, Order, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui,
    pos2, vec2,
};
use std::sync::Arc;

/// Colour as in the Fireworks file format: 0xAARRGGBB, not premultiplied (FirePixel's `fp_color::Argb`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Argb(pub u32);

impl Argb {
    pub const fn a(self) -> u8 {
        (self.0 >> 24) as u8
    }
    pub const fn r(self) -> u8 {
        (self.0 >> 16) as u8
    }
    pub const fn g(self) -> u8 {
        (self.0 >> 8) as u8
    }
    pub const fn b(self) -> u8 {
        self.0 as u8
    }
}

/// The choice made in the pop-up (FirePixel's `popups::ColorChoice`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorChoice {
    Color(Argb),
    /// No colour ("None" button).
    None,
    /// Tools panel fill: Fireworks MX's "Fill Options" button.
    FillOptions,
    /// Tools panel fill: "Gradient" button.
    Gradient,
    /// Tools panel fill: "Pattern" button.
    Pattern,
    /// Stroke: placement 0 inside, 1 centred, 2 outside.
    Placement(u8),
}

/// FireCraft: the eyedropper's reading of the document under a screen point (composited on
/// white), in place of FirePixel's app state; `None` where there is no document.
pub type Sampler<'a> = &'a dyn Fn(Pos2) -> Option<Argb>;

/// Größe des Popups (Punkte), in allen Aufrufen gleich.
pub const POPUP_W: f32 = 259.0;
pub const POPUP_H: f32 = 277.0;
/// Raster: Spalten × Zeilen.
pub const GRID_COLS: usize = 21;
pub const GRID_ROWS: usize = 15;

/// FireCraft: the latest window screenshot for the eyedropper (taken through egui's screenshot
/// callback).
static SHOT: std::sync::Mutex<Option<Arc<egui::ColorImage>>> = std::sync::Mutex::new(None);

fn store_shot(img: Arc<egui::ColorImage>) {
    if let Ok(mut s) = SHOT.lock() {
        *s = Some(img);
    }
}

fn take_shot() -> Option<Arc<egui::ColorImage>> {
    SHOT.lock().ok().and_then(|mut s| s.take())
}
/// Merker: Id des gerade offenen Farbwähler-Popups (für [`any_open`]).
const OPEN_MARK: &str = "fp-color-popup-open";

/// Fußleiste je Aufrufart (§2.1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Footer {
    /// Nur Farbkreis (Inspektor-Füllung, Effekte, Dialoge, Bedienfelder).
    #[default]
    Plain,
    /// „Ohne“ + Farbkreis (Farbfelder, die keine Farbe zulassen, z. B. Leinwand transparent).
    NoneOnly,
    /// Leinwandfarbe (Dokument-Inspektor): Ohne + Farbkreis, **ohne** Deckkraftfeld wie im
    /// Original (die Leinwand ist deckend oder transparent, PAR 16/02).
    Canvas,
    /// Werkzeugleiste Füllung: Ohne, Einfarbig, Verlauf, Muster + Farbkreis. `kinds`: Verlauf und
    /// Muster bedienbar (es gibt eine Auswahl).
    Fill { kinds: bool },
    /// Fireworks MX's Tools panel fill: None and a Fill Options button.
    FillOptions,
    /// Kontur (Werkzeugleiste, Inspektor): Ohne, Strich innen/mittig/außen + Farbkreis.
    /// `placement`: gedrückte Lage (0 innen, 1 mittig, 2 außen), `enabled`: Lage-Knöpfe bedienbar.
    Stroke { placement: Option<u8>, enabled: bool },
}

impl Footer {
    fn has_none(self) -> bool {
        !matches!(self, Footer::Plain)
    }

    /// Show the opacity row (field and drop-down slider)? Only for fill and stroke colours:
    /// Fireworks CS6's pop-up has none for effect colours (Color Fill, shadows, glows; checked by
    /// the user in CS6), which have their own opacity setting, nor for the canvas or dialogs.
    /// FirePixel showed it everywhere but the canvas, where it snapped back to 100%.
    fn has_opacity(self) -> bool {
        matches!(
            self,
            Footer::Fill { .. } | Footer::FillOptions | Footer::Stroke { .. }
        )
    }
}

/// Farbe nach einem Klick in die Rasterzelle (c, r): wie im Original immer mit Deckkraft 100
/// (Alpha FF); die bisherige Deckkraft wird verworfen (re 28 D5: #ff000080, Klick auf Weiß →
/// #ffffff). Die Felder RGB/HEX behalten sie dagegen weiter (`CpState::finish`).
pub fn grid_choice(c: usize, r: usize) -> Argb {
    Argb(grid_color(c, r).0 | 0xff00_0000)
}

/// Farbe des festen Rasters, Spalte `c` (0…20), Zeile `r` (0…14), Formel §2.2 (gegen alle Zellen der
/// Messung `grid_21x15.csv` geprüft).
pub fn grid_color(c: usize, r: usize) -> Argb {
    let rgb = |r: f64, g: f64, b: f64| {
        Argb(
            0xff00_0000
                | (r.round().clamp(0.0, 255.0) as u32) << 16
                | (g.round().clamp(0.0, 255.0) as u32) << 8
                | b.round().clamp(0.0, 255.0) as u32,
        )
    };
    let hex = |v: u32| Argb(0xff00_0000 | v);
    if r < 11 {
        // reine Farbe P = HSV(15°·c, 1, 1)
        let h = 15.0 * c as f64 / 60.0;
        // die reine Farbe wird zuerst auf ganze Werte gerundet (sonst r1 c3 = FFE699 statt FFE599)
        let x = (255.0 * (1.0 - ((h % 2.0) - 1.0).abs())).round();
        let (pr, pg, pb) = match h as u32 {
            0 => (255.0, x, 0.0),
            1 => (x, 255.0, 0.0),
            2 => (0.0, 255.0, x),
            3 => (0.0, x, 255.0),
            4 => (x, 0.0, 255.0),
            _ => (255.0, 0.0, x),
        };
        return match r {
            0..=4 => {
                // Weißanteil t = 0,75 … 0,15 als 255 − Abdunkelstufe (Hälften zur geraden Zahl, wie gemessen:
                // 0,3 → 77, sonst r3 c1 = FF794D statt FF7A4D)
                let keep: f64 = [0.25, 0.40, 0.55, 0.70, 0.85][r];
                let w = (255.0 - (255.0 * keep).round_ties_even()) / 255.0;
                rgb(
                    pr + (255.0 - pr) * w,
                    pg + (255.0 - pg) * w,
                    pb + (255.0 - pb) * w,
                )
            }
            5 => rgb(pr, pg, pb),
            _ => {
                let k: f64 = [0.85, 0.70, 0.55, 0.40, 0.25][r - 6];
                // Stufe als ganzer Wert, Hälften zur geraden Zahl (0,7 → 178 wie gemessen)
                let v = (255.0 * k).round_ties_even() / 255.0;
                rgb(pr * v, pg * v, pb * v)
            }
        };
    }
    if r == 11 || r > 14 {
        return hex(0);
    }
    match c {
        0..=14 => {
            let c = c as u32;
            match r {
                12 => {
                    let v = 0xff - 0x11 * c;
                    hex(v << 16 | v << 8 | v)
                }
                _ if c == 0 => hex(0xff_ffff),
                13 => {
                    let v = 0xf0 - 0x11 * (c - 1);
                    hex(v << 16 | v << 8 | (v - 0x0f))
                }
                _ => {
                    let v = 0xec - 0x11 * (c - 1);
                    hex(v << 16 | v << 8 | (v + 0x0f))
                }
            }
        }
        15 | 16 => hex(0),
        _ => {
            const SIDE: [[u32; 4]; 3] = [
                [0xff0000, 0xff7f00, 0xffff00, 0x7fff00],
                [0x007fff, 0x00ffff, 0x00ff7f, 0x00ff00],
                [0x0000ff, 0x7f00ff, 0xff00ff, 0xff007f],
            ];
            hex(SIDE[r - 12][(c - 17).min(3)])
        }
    }
}

/// Lage des Popups (§2.3): Unterkante = Oberkante des Farbfelds `anchor`, linke Kante beim
/// Klickpunkt `click_x`; oben zu wenig Platz → an den oberen Rand von `screen`. Rechts bleibt es im
/// Fenster.
pub fn popup_pos(anchor: Rect, click_x: f32, screen: Rect) -> Pos2 {
    let x = click_x.round().min(screen.right() - POPUP_W).max(screen.left());
    let y = (anchor.top() - POPUP_H).max(screen.top());
    pos2(x, y)
}

/// Strichlage zum Knopf der Kontur-Fußleiste (Parameter placement von `pi.stroke`).
pub fn placement_name(k: u8) -> &'static str {
    ["inside", "center", "outside"][(k as usize).min(2)]
}

/// Value of the HEX field, parsed on every keystroke like the original (re 46 §2.2); the result is
/// always a color (`0xRRGGBB`), invalid input never keeps the old value:
/// - `0x` prefix (any case): stripped, then left-padded with `0` to six digits (`0x36c` → `00036C`)
/// - exactly three characters or `#` + three: shorthand, each digit doubled (`36c` → `3366CC`)
/// - empty: white
/// - `#…`: up to three two-digit hex pairs after the `#` (a fourth pair, alpha, is ignored)
/// - more than six characters without `#`: up to three decimal numbers separated by blanks (each
///   taken modulo 256; nonsense, but what the original does)
/// - otherwise up to three two-digit hex pairs
///
/// Reading stops at the first invalid pair; unread components stay 0 (`zz12` → `#000000`,
/// `12zz` → `#120000`, `abcde` → `#ABCD0E`). Leading blanks before a pair are skipped.
pub fn hex_value(text: &str) -> u32 {
    let mut s: Vec<char> = text.chars().collect();
    if s.len() >= 2 && s[0] == '0' && (s[1] == 'x' || s[1] == 'X') {
        s.drain(..2);
        while s.len() < 6 {
            s.insert(0, '0');
        }
    }
    if s.len() == 3 || (s.len() == 4 && s[0] == '#') {
        let t = &s[s.len() - 3..];
        s = vec![t[0], t[0], t[1], t[1], t[2], t[2]];
    }
    if s.is_empty() {
        return 0xff_ffff;
    }
    let comps = if s[0] == '#' {
        scan_hex_pairs(&s[1..])
    } else if s.len() > 6 {
        scan_decimals(&s)
    } else {
        scan_hex_pairs(&s)
    };
    comps[0] << 16 | comps[1] << 8 | comps[2]
}

/// `%02lx%02lx%02lx`: up to three values of at most two hex digits each, blanks before a value
/// skipped; stops at the first value without a digit.
fn scan_hex_pairs(s: &[char]) -> [u32; 3] {
    let mut out = [0u32; 3];
    let mut i = 0;
    for slot in &mut out {
        while i < s.len() && s[i].is_whitespace() {
            i += 1;
        }
        let mut v = 0u32;
        let mut n = 0;
        while n < 2 && i < s.len() {
            let Some(d) = s[i].to_digit(16) else { break };
            v = v * 16 + d;
            i += 1;
            n += 1;
        }
        if n == 0 {
            break;
        }
        *slot = v;
    }
    out
}

/// `%ld %ld %ld`: up to three decimal numbers (optional sign), each taken modulo 256.
fn scan_decimals(s: &[char]) -> [u32; 3] {
    let mut out = [0u32; 3];
    let mut i = 0;
    for slot in &mut out {
        while i < s.len() && s[i].is_whitespace() {
            i += 1;
        }
        let neg = i < s.len() && s[i] == '-';
        if i < s.len() && (s[i] == '-' || s[i] == '+') {
            i += 1;
        }
        let mut v: i64 = 0;
        let mut n = 0;
        while i < s.len() {
            let Some(d) = s[i].to_digit(10) else { break };
            v = v.wrapping_mul(10).wrapping_add(i64::from(d));
            i += 1;
            n += 1;
        }
        if n == 0 {
            break;
        }
        *slot = (if neg { v.wrapping_neg() } else { v } & 0xff) as u32;
    }
    out
}

/// Zeichenkette für den HEX-Kopierknopf und das Feld: `#RRGGBB`.
pub fn hex_text(c: Argb) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

/// Zeichenkette für den RGB-Kopierknopf: `rgba(r,g,b,a)`, a als Bruch mit zwei Nachkommastellen
/// (bei „Ohne“ `rgba(0,0,0,0.00)`).
pub fn rgba_text(c: Option<Argb>) -> String {
    match c {
        Some(c) => format!("rgba({},{},{},{:.2})", c.r(), c.g(), c.b(), c.a() as f64 / 255.0),
        None => "rgba(0,0,0,0.00)".into(),
    }
}

/// Bildschirmfotos für die Pipette anfordern? Nicht in cargo-Testprogrammen (…\deps\*.exe): dort
/// rendert egui_kittest jedes angeforderte Foto mit einem eigenen wgpu-Renderer, und parallel
/// angelegte Renderer bringen den Testprozess zum Absturz (AGENTEN_REGELN §2). Die Leinwand-Pipette
/// rechnet ohne Foto; die Brücke und die App selbst fotografieren wie vorgesehen.
fn screenshots_allowed() -> bool {
    static OK: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *OK.get_or_init(|| {
        !std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().and_then(|d| d.file_name()).map(|n| n == "deps"))
            .unwrap_or(false)
    })
}

/// Zustand eines offenen Popups.
#[derive(Clone)]
struct CpState {
    /// Lage (oben links) des Popups.
    pos: Pos2,
    /// Farbe beim Öffnen (`None` = Ohne).
    start: Option<Argb>,
    /// Arbeitsfarbe (getippt), deckend; anfangs die Startfarbe.
    work: Option<Argb>,
    /// Deckkraft 0…100, wie angezeigt.
    opacity: u8,
    /// Etwas getippt bzw. Deckkraft geändert (Enter übernimmt).
    dirty: bool,
    opacity_dirty: bool,
    hex: String,
    op: String,
    rgb: [String; 3],
    /// Klappschieber der Deckkraft offen.
    slider: bool,
    /// The opacity arrow was pressed and is dragged (RE-29, re 61 §2): the value follows the
    /// pointer and is applied on release, which closes the pop-up; Esc restores the opacity held
    /// here (value, typed flag) from before the drag.
    slider_drag: Option<(u8, bool)>,
    /// Farbdialog (Farbkreis) offen.
    sys: bool,
    /// Bild, in dem das Popup aufging (der öffnende Klick zählt nicht als Pipette).
    opened_pass: u64,
    /// Bildschirmfoto des App-Fensters und Zeitpunkt der letzten Anforderung.
    shot: Option<Arc<egui::ColorImage>>,
    shot_asked: f64,
    /// Zeigerlage beim letzten Foto (ruht der Zeiger, wird nicht neu fotografiert).
    shot_pos: Option<Pos2>,
    /// Foto angefordert, aber noch nicht da (Zeitpunkt): bis dahin wird der eigene Pipetten-Zeiger
    /// nicht gemalt. eframe fotografiert erst das **nächste** Bild nach der Anforderung – dort lag
    /// sonst die schwarze Spitze des Zeigers genau unter dem Zeiger, und die Pipette las über
    /// Bedienfeldern und Menüleiste immer #000000 (N27, NP-1).
    shot_pending: Option<f64>,
    /// Primary button went down on the grid or outside the popup and is still held: the choice
    /// follows the pointer and is taken on release (like the reference, TW-112).
    track: bool,
}

fn state_id(pid: Id) -> Id {
    pid.with("cp-state")
}

/// Popup-Id zu einem Farbfeld (`Id` der Farbfeld-Antwort), wie bei egui-Popups.
pub fn popup_id(swatch: Id) -> Id {
    swatch.with("popup")
}

/// Ist ein Farbwähler-Popup offen? (Die Tastenkürzel der Leinwand ruhen dann, §2.3 „Tippen ohne
/// Feldklick: ignoriert“.)
pub fn any_open(ctx: &egui::Context) -> bool {
    ctx.data(|d| d.get_temp::<Id>(Id::unique(OPEN_MARK)))
        .is_some_and(|pid| egui::Popup::is_id_open(ctx, pid))
}

/// Rechteck des offenen Popups zum Farbfeld `swatch` (Tests, Brücke).
pub fn open_rect(ctx: &egui::Context, swatch: Id) -> Option<Rect> {
    let pid = popup_id(swatch);
    if !egui::Popup::is_id_open(ctx, pid) {
        return None;
    }
    ctx.data(|d| d.get_temp::<CpState>(state_id(pid)))
        .map(|s| Rect::from_min_size(s.pos, vec2(POPUP_W, POPUP_H)))
}

/// Closes the open colour pop-up **without** applying anything, like Esc (FB-46: Undo, Redo and a
/// History jump first discard a pending colour or opacity). Returns whether one was open.
pub fn discard_open(ctx: &egui::Context) -> bool {
    let Some(pid) = ctx.data(|d| d.get_temp::<Id>(Id::unique(OPEN_MARK))) else {
        return false;
    };
    let open = egui::Popup::is_id_open(ctx, pid);
    if open {
        egui::Popup::close_id(ctx, pid);
    }
    ctx.data_mut(|d| {
        d.remove::<CpState>(state_id(pid));
        d.remove::<Id>(Id::unique(OPEN_MARK));
    });
    let fields = ["hex", "op", "r", "g", "b"].map(|f| pid.with(f));
    ctx.memory_mut(|m| {
        if let Some(f) = m.focused().filter(|f| fields.contains(f)) {
            m.surrender_focus(f);
        }
    });
    ctx.request_repaint();
    open
}

/// Angezeigte Farbe (Vorschaufeld) des offenen Popups zum Farbfeld `swatch` im letzten Bild:
/// Farbe unter dem Zeiger bzw. Arbeitsfarbe (Tests).
pub fn live_color(ctx: &egui::Context, swatch: Id) -> Option<Option<Argb>> {
    ctx.data(|d| d.get_temp::<Option<Argb>>(popup_id(swatch).with("live")))
}

/// Pending colour of the open pop-up of the swatch `swatch` once its opacity was changed (alpha
/// slider dragged or the opacity field typed): the colour Enter would apply, with the pending alpha
/// (FB-29, TW-206). The callers show it as a live preview on the canvas; `None` while the pop-up is
/// closed or its opacity untouched. Esc closing the pop-up discards it (nothing is applied).
pub fn pending_color(ctx: &egui::Context, swatch: Id) -> Option<Argb> {
    let pid = popup_id(swatch);
    if !egui::Popup::is_id_open(ctx, pid) {
        return None;
    }
    let s = ctx.data(|d| d.get_temp::<CpState>(state_id(pid)))?;
    s.opacity_dirty
        .then(|| s.finish(s.work.unwrap_or(Argb(0xff00_0000))))
}

/// Rechteck der Rasterzelle (c, r) im Popup mit Ursprung `o`.
pub fn cell_rect(o: Pos2, c: usize, r: usize) -> Rect {
    Rect::from_min_size(
        o + vec2(4.0 + 12.0 * c as f32, 4.0 + 12.0 * r as f32),
        vec2(11.0, 11.0),
    )
}

fn c32(c: Argb) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), c.a())
}

fn opaque(c: Argb) -> Argb {
    Argb(c.0 | 0xff00_0000)
}

fn opacity_of(start: Option<Argb>) -> u8 {
    match start {
        Some(c) => ((c.a() as f64) * 100.0 / 255.0).round() as u8,
        None => 0,
    }
}

impl CpState {
    fn new(pos: Pos2, start: Option<Argb>, pass: u64) -> CpState {
        let work = start.map(opaque);
        let mut s = CpState {
            pos,
            start,
            work,
            opacity: opacity_of(start),
            dirty: false,
            opacity_dirty: false,
            hex: String::new(),
            op: String::new(),
            rgb: Default::default(),
            slider: false,
            slider_drag: None,
            sys: false,
            opened_pass: pass,
            shot: None,
            shot_asked: f64::NEG_INFINITY,
            shot_pos: None,
            shot_pending: None,
            track: false,
        };
        // HEX/RGB always show the base colour, never blended with its alpha (RE-29, re 61 §2: the
        // reference keeps #FF0000 / 255 0 0 while the alpha is edited; its blended value on
        // reopening a semi-transparent colour is deliberately not copied); the alpha shows only
        // in the opacity field
        s.fill_buffers(work);
        s
    }

    fn fill_buffers(&mut self, c: Option<Argb>) {
        let c = c.unwrap_or(Argb(0xff00_0000));
        self.hex = hex_text(c);
        self.rgb = [c.r().to_string(), c.g().to_string(), c.b().to_string()];
        self.op = self.opacity.to_string();
    }

    /// Ergebnisfarbe mit Deckkraft: getippte Deckkraft, sonst die der Startfarbe (war die Farbe
    /// „Ohne“ bzw. ganz transparent: deckend, wie im Original wird die Füllung dann Einfarbig).
    fn finish(&self, rgb: Argb) -> Argb {
        let a = if self.opacity_dirty {
            (self.opacity as f64 * 255.0 / 100.0).round() as u32
        } else {
            match self.start {
                Some(c) if c.a() > 0 => c.a() as u32,
                _ => 255,
            }
        };
        Argb((rgb.0 & 0x00ff_ffff) | a << 24)
    }
}

/// Farbe unter dem Zeiger außerhalb des Popups: Leinwand (Dokumentpixel der aktiven Seite, auf Weiß)
/// bzw. das letzte Bildschirmfoto des App-Fensters.
fn sample(ctx: &egui::Context, st: Sampler<'_>, s: &mut CpState, p: Pos2, own: &[Id]) -> Option<Argb> {
    // Leinwand-Abkürzung nur, wenn dort kein Fenster (Dialog, schwebendes Bedienfeld, Menü) liegt:
    // sonst nahm die Pipette über dem Glühen-Dialog das Dokumentpixel darunter (N27, NP-1)
    // FireCraft: the document under the pointer, unless a window lies over it
    if !covered_by_window(ctx, p, own)
        && let Some(c) = st(p)
    {
        return Some(c);
    }
    // Outside FireCraft's window: no colour (the working colour stays).
    if !ctx.content_rect().contains(p) {
        return None;
    }
    let img = s.shot.as_ref()?;
    let ppp = ctx.pixels_per_point();
    let (x, y) = ((p.x * ppp).floor() as i64, (p.y * ppp).floor() as i64);
    let [w, h] = img.size;
    if x < 0 || y < 0 || x as usize >= w || y as usize >= h {
        return None;
    }
    let c = img.pixels[y as usize * w + x as usize];
    let k = 1.0f32;
    let un = |v: u8| ((v as f32 / k).round().clamp(0.0, 255.0)) as u32;
    Some(Argb(0xff00_0000 | un(c.r()) << 16 | un(c.g()) << 8 | un(c.b())))
}

/// Liegt über `p` ein Fenster/Bereich oberhalb der Grundebene (Dialog, schwebendes Bedienfeld,
/// Menü)? Die eigenen Ebenen des Popups (`own`: Popup und Hintergrund) zählen nicht.
fn covered_by_window(ctx: &egui::Context, p: Pos2, own: &[Id]) -> bool {
    ctx.memory(|m| {
        m.layer_ids().any(|l| {
            !matches!(l.order, Order::Background | Order::Tooltip | Order::Debug)
                && !own.contains(&l.id)
                && m.areas().is_visible(&l)
                && m.area_rect(l.id).is_some_and(|r| r.contains(p))
        })
    })
}

/// Eingabefeld des Popups (Rahmen im Firepixel-Design, Fokus = Unterlinie wie im Original).
fn field(ui: &mut Ui, id: Id, r: Rect, buf: &mut String, right: bool) -> Response {
    let t = Theme::current(ui.ctx());
    let c = &t.colors;
    ui.painter().rect_filled(r, 0.0, c.field_bg);
    ui.painter()
        .rect_stroke(r, 0.0, Stroke::new(1.0, c.field_border), StrokeKind::Inside);
    let te = egui::TextEdit::singleline(buf)
        .id(id)
        .frame(egui::Frame::NONE)
        .font(t.ui_font())
        .text_color(c.field_text)
        .margin(egui::Margin::ZERO)
        .align(if right {
            egui::Align2::RIGHT_CENTER
        } else {
            egui::Align2::LEFT_CENTER
        })
        .desired_width(r.width() - 6.0)
        .event_filter(fields::TEXT_FILTER);
    let resp = ui.put(r.shrink2(vec2(3.0, 1.0)), te);
    if ui.memory(|m| m.has_focus(id)) {
        ui.painter().hline(
            r.left() + 1.0..=r.right() - 1.0,
            r.bottom() - 1.5,
            Stroke::new(2.0, c.focus),
        );
    }
    resp
}

/// Kopierknopf (zwei Blätter).
fn copy_button(ui: &mut Ui, id: Id, r: Rect, tip: &str) -> bool {
    fields::icon_button(ui, id, r, Glyph::Copy, true, tip).clicked()
}

/// Ist im offenen Farbwähler der Farbdialog (Farbkreis) offen?
pub fn wheel_open(ctx: &egui::Context) -> bool {
    let Some(pid) = ctx.data(|d| d.get_temp::<Id>(Id::unique(OPEN_MARK))) else {
        return false;
    };
    egui::Popup::is_id_open(ctx, pid)
        && ctx
            .data(|d| d.get_temp::<CpState>(state_id(pid)))
            .is_some_and(|s| s.sys)
}

/// Der nächste [`show`] mit Öffnen für dieses Farbfeld zeigt gleich den Farbdialog (Farbkreis).
pub fn request_wheel(ctx: &egui::Context, swatch: Id) {
    ctx.data_mut(|d| d.insert_temp(popup_id(swatch).with("open-wheel"), true));
}

/// Farbkreis-Symbol des Knopfs rechts unten (auch Farbrad-Knopf des Verlaufs-Popups).
pub(crate) fn paint_wheel(p: &egui::Painter, center: Pos2, radius: f32) {
    let n = 24;
    for i in 0..n {
        let a0 = std::f32::consts::TAU * i as f32 / n as f32;
        let a1 = std::f32::consts::TAU * (i + 1) as f32 / n as f32;
        let h = i as f32 / n as f32;
        let col: Color32 = egui::ecolor::Hsva::new(h, 1.0, 1.0, 1.0).into();
        let pts = vec![
            center,
            center + vec2(a0.cos(), a0.sin()) * radius,
            center + vec2(a1.cos(), a1.sin()) * radius,
        ];
        p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
    }
    p.circle_filled(center, radius * 0.35, Color32::WHITE);
    p.circle_stroke(center, radius, Stroke::new(1.0, Color32::from_gray(60)));
}

/// Farbfeld mit Popup an einer Antwort: öffnet beim Klick (linke Kante beim Klickpunkt).
pub fn popup(
    ui: &mut Ui,
    resp: &Response,
    st: Sampler<'_>,
    color: Option<Argb>,
    footer: Footer,
) -> Option<ColorChoice> {
    let open_at = resp.clicked().then(|| {
        resp.interact_pointer_pos()
            .map(|p| p.x)
            .unwrap_or(resp.rect.left())
    });
    show(ui, resp.id, resp.rect, open_at, st, color, footer)
}

/// Farbwähler-Popup zum Farbfeld mit der Id `swatch` und dem Rechteck `anchor`. `open_at`: in diesem
/// Bild öffnen (x des Klickpunkts). Liefert die Wahl; das Popup ist danach zu.
pub fn show(
    ui: &mut Ui,
    swatch: Id,
    anchor: Rect,
    open_at: Option<f32>,
    st: Sampler<'_>,
    color: Option<Argb>,
    footer: Footer,
) -> Option<ColorChoice> {
    let ctx = ui.ctx().clone();
    let pid = popup_id(swatch);
    let sid = state_id(pid);
    let pass = ctx.cumulative_pass_nr();
    let screen = ctx.content_rect();
    let open = egui::Popup::is_id_open(&ctx, pid);
    if let Some(x) = open_at
        && !open
    {
        let mut s = CpState::new(popup_pos(anchor, x, screen), color, pass);
        // gleich mit dem Farbdialog (Farbkreis) öffnen, z. B. Farbrad-Knopf im Verlaufs-Popup
        let wheel_flag = pid.with("open-wheel");
        if ctx
            .data_mut(|d| d.remove_temp::<bool>(wheel_flag))
            .unwrap_or(false)
        {
            s.sys = true;
            ctx.data_mut(|d| d.insert_temp(picker_id(swatch).with("sys"), true));
        }
        ctx.data_mut(|d| {
            d.insert_temp(sid, s);
            d.insert_temp(Id::unique(OPEN_MARK), pid);
        });
        egui::Popup::open_id(&ctx, pid);
    } else if !open {
        // zu (anderes Popup geöffnet, Aufrufer nicht gezeichnet): Zustand verwerfen
        if ctx.data(|d| d.get_temp::<CpState>(sid)).is_some() {
            ctx.data_mut(|d| d.remove::<CpState>(sid));
        }
        return None;
    }
    let mut s = ctx.data(|d| d.get_temp::<CpState>(sid)).unwrap_or_else(|| {
        let s = CpState::new(popup_pos(anchor, anchor.left(), screen), color, pass);
        s
    });
    // offen halten (egui schließt Popups, die ein Bild lang nicht gezeigt werden)
    egui::Popup::open_id(&ctx, pid);
    ctx.data_mut(|d| d.insert_temp(Id::unique(OPEN_MARK), pid));
    let (out, close) = body(ui, &ctx, swatch, st, &mut s, footer, pass);
    if close {
        egui::Popup::close_id(&ctx, pid);
        ctx.data_mut(|d| {
            d.remove::<CpState>(sid);
            d.remove::<Id>(Id::unique(OPEN_MARK));
            d.remove::<Color32>(picker_id(swatch).with("syscol"));
            d.insert_temp(picker_id(swatch).with("sys"), false);
        });
        ctx.memory_mut(|m| {
            if m.focused().is_some_and(|f| {
                [
                    pid.with("hex"),
                    pid.with("op"),
                    pid.with("r"),
                    pid.with("g"),
                    pid.with("b"),
                ]
                .contains(&f)
            }) {
                m.surrender_focus(m.focused().unwrap_or(pid));
            }
        });
    } else {
        ctx.data_mut(|d| d.insert_temp(sid, s));
    }
    out
}

/// Inhalt, Eingaben und Pipette eines offenen Popups. Liefert (Wahl, schließen).
fn body(
    ui: &mut Ui,
    ctx: &egui::Context,
    swatch: Id,
    st: Sampler<'_>,
    s: &mut CpState,
    footer: Footer,
    pass: u64,
) -> (Option<ColorChoice>, bool) {
    let pid = popup_id(swatch);
    let t = Theme::current(ctx);
    let c = &t.colors;
    let swatch_pid = picker_id(swatch);
    let o = s.pos;
    let rect = Rect::from_min_size(o, vec2(POPUP_W, POPUP_H));
    let grid = Rect::from_min_max(o + vec2(3.0, 3.0), o + vec2(256.0, 184.0));
    let slider_rect = slider_rect(o, ctx.content_rect());
    let sys_rect = sys_rect(o, ctx.content_rect());
    // Bildschirmfotos für die Pipette über dem App-Fenster einsammeln
    // FireCraft's egui delivers screenshots to a callback, which stores the latest here.
    let shots: Vec<Arc<egui::ColorImage>> = take_shot().into_iter().collect();
    let now = ctx.input(|i| i.time);
    if let Some(img) = shots.into_iter().last() {
        s.shot = Some(img);
        s.shot_pending = None;
    }
    if s.shot_pending.is_some_and(|t| now - t > 1.0) {
        s.shot_pending = None; // kein Foto gekommen (z. B. Fenster verdeckt): Zeiger wieder zeigen
    }
    let mut out = None;
    let mut close = false;
    // ---- Tasten (vor den Feldern verbraucht, damit weder Dialog noch Feld sie bekommt)
    let esc = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
    let enter = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter));
    if esc {
        if let Some((op, dirty)) = s.slider_drag.take() {
            // Esc during a drag from the arrow: back to the opacity before the drag
            s.opacity = op;
            s.opacity_dirty = dirty;
            s.op = op.to_string();
            s.slider = false;
        } else if s.slider {
            s.slider = false;
        } else if s.sys {
            s.sys = false;
            ctx.data_mut(|d| d.remove::<Color32>(swatch_pid.with("syscol")));
            ctx.data_mut(|d| d.insert_temp(swatch_pid.with("sys"), false));
        } else {
            return (None, true);
        }
    }
    if enter && !s.sys {
        if s.dirty || s.opacity_dirty {
            let rgb = s.work.unwrap_or(Argb(0xff00_0000));
            return (Some(ColorChoice::Color(s.finish(rgb))), true);
        }
        return (None, true);
    }
    // ---- Zeiger: Farbe unter dem Zeiger (Pipette ab dem Öffnen)
    let pointer = ctx.input(|i| i.pointer.latest_pos());
    let field_ids = [
        pid.with("hex"),
        pid.with("op"),
        pid.with("r"),
        pid.with("g"),
        pid.with("b"),
    ];
    let focused = ctx
        .memory(|m| m.focused())
        .is_some_and(|f| field_ids.contains(&f));
    let cell_at = |p: Pos2| -> Option<(usize, usize)> {
        if !grid.contains(p) {
            return None;
        }
        let cc = ((p.x - o.x - 3.5) / 12.0)
            .floor()
            .clamp(0.0, (GRID_COLS - 1) as f32) as usize;
        let rr = ((p.y - o.y - 3.5) / 12.0)
            .floor()
            .clamp(0.0, (GRID_ROWS - 1) as f32) as usize;
        Some((cc, rr))
    };
    let own = [pid.with("backdrop"), pid.with("area")];
    let (sys_open, slider_open) = (s.sys, s.slider);
    let dragging = s.slider_drag.is_some();
    let blocked =
        |p: Pos2| dragging || (sys_open && sys_rect.contains(p)) || (slider_open && slider_rect.contains(p));
    let mut hover_cell = None;
    let mut shot_frame = false;
    let mut live: Option<Argb> = None;
    if let Some(p) = pointer.filter(|p| !blocked(*p)) {
        if rect.contains(p) {
            hover_cell = cell_at(p);
            live = hover_cell.map(|(cc, rr)| grid_color(cc, rr));
        } else if !s.sys {
            live = sample(ctx, st, s, p, &own);
            // Bildschirmfoto auffrischen, solange der Zeiger draußen ist (höchstens alle 0,4 s)
            // (das Foto zeigt dieses Bild: hier ohne den eigenen Zeiger, sonst läse die Pipette ihn selbst)
            if now - s.shot_asked > 0.4 && s.shot_pos != Some(p) && screenshots_allowed() {
                s.shot_asked = now;
                s.shot_pos = Some(p);
                s.shot_pending = Some(now);
                shot_frame = true;
                let repaint = ctx.clone();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(
                    move |img| {
                        store_shot(img);
                        repaint.request_repaint();
                    },
                )));
                ctx.request_repaint();
            }
        }
    }
    // ---- Press on a grid cell or outside (eyedropper) starts tracking; the colour under the
    // pointer at release is taken and the popup closes (press, drag across the grid, release =
    // that swatch; Esc while held cancels, see above)
    let press = ctx.input(|i| {
        if i.pointer.primary_pressed() {
            i.pointer.press_origin().or(i.pointer.latest_pos())
        } else {
            None
        }
    });
    if let Some(p) = press
        && pass > s.opened_pass
        && !blocked(p)
    {
        if s.slider && !slider_rect.contains(p) {
            s.slider = false;
        } else if s.sys {
            // Klick neben den Farbdialog: nichts (modal wie der Systemdialog)
        } else if cell_at(p).is_some() || !rect.contains(p) {
            s.track = true;
        }
    }
    let release = ctx.input(|i| {
        if i.pointer.primary_released() {
            i.pointer.latest_pos().or(i.pointer.interact_pos())
        } else {
            None
        }
    });
    if s.track && (release.is_some() || !ctx.input(|i| i.pointer.primary_down())) {
        s.track = false;
        if let Some(p) = release.filter(|p| !blocked(*p) && !s.sys) {
            if let Some((cc, rr)) = cell_at(p) {
                return (Some(ColorChoice::Color(grid_choice(cc, rr))), true);
            } else if !rect.contains(p) {
                let picked = sample(ctx, st, s, p, &own);
                return (picked.map(|col| ColorChoice::Color(s.finish(col))), true);
            }
        }
    }
    // ---- Hintergrund (schluckt Klicks außerhalb, damit die Leinwand sie nicht bekommt)
    let back_layer = LayerId::new(Order::Foreground, pid.with("backdrop"));
    egui::Area::new(back_layer.id)
        .order(Order::Foreground)
        .fixed_pos(ctx.content_rect().min)
        .constrain(false)
        .show(ctx, |ui| {
            ui.allocate_exact_size(ctx.content_rect().size(), Sense::click_and_drag());
        });
    // ---- Popup
    let layer = LayerId::new(Order::Foreground, pid.with("area"));
    let shown = live.or(s.work);
    ctx.data_mut(|d| d.insert_temp(pid.with("live"), shown));
    if !focused {
        let keep_op = s.op.clone();
        s.fill_buffers(shown);
        if s.opacity_dirty {
            s.op = keep_op;
        }
    }
    // Popup, Klappschieber und Farbdialog liegen in **einer** Ebene direkt über dem Hintergrund (egui
    // ordnet Ebenen, die gleichzeitig nach oben wollen, nicht nach Aufrufreihenfolge)
    let mut union = rect;
    if s.slider {
        union = union.union(slider_rect);
    }
    if s.sys {
        union = union.union(sys_rect);
    }
    ctx.set_sublayer(back_layer, layer);
    let mut sys_res = None;
    let mut drag_released = false;
    egui::Area::new(layer.id)
        .order(Order::Foreground)
        .fixed_pos(union.min)
        .constrain(false)
        .show(ctx, |ui| {
            let (_, _bg) = ui.allocate_exact_size(union.size(), Sense::click());
            let p = ui.painter().clone();
            p.rect_filled(rect, 0.0, c.menu_bg);
            p.rect_stroke(rect, 0.0, Stroke::new(1.0, c.menu_border), StrokeKind::Inside);
            // Raster
            p.rect_filled(grid, 0.0, Color32::BLACK);
            for rr in 0..GRID_ROWS {
                for cc in 0..GRID_COLS {
                    p.rect_filled(cell_rect(o, cc, rr), 0.0, c32(grid_color(cc, rr)));
                }
            }
            let frame = Color32::from_gray(0x64);
            p.vline(o.x + 3.5, (o.y + 3.0)..=(o.y + 184.0), Stroke::new(1.0, frame));
            p.vline(o.x + 255.5, (o.y + 3.0)..=(o.y + 184.0), Stroke::new(1.0, frame));
            p.hline((o.x + 3.0)..=(o.x + 256.0), o.y + 3.5, Stroke::new(1.0, frame));
            p.hline((o.x + 3.0)..=(o.x + 256.0), o.y + 183.5, Stroke::new(1.0, frame));
            if let Some((cc, rr)) = hover_cell {
                p.rect_stroke(
                    cell_rect(o, cc, rr).expand(1.0),
                    0.0,
                    Stroke::new(1.0, Color32::WHITE),
                    StrokeKind::Inside,
                );
            }
            // Vorschau
            let pv = Rect::from_min_max(o + vec2(3.0, 191.0), o + vec2(48.0, 211.0));
            match if focused { s.work } else { shown } {
                Some(col) => {
                    p.rect_filled(pv, 0.0, c32(opaque(col)));
                }
                None => {
                    p.rect_filled(pv, 0.0, Color32::WHITE);
                    p.line_segment([pv.left_bottom(), pv.right_top()], Stroke::new(1.5, c.icon_red));
                }
            }
            p.rect_stroke(
                pv,
                0.0,
                Stroke::new(1.0, Color32::from_gray(0x69)),
                StrokeKind::Inside,
            );
            // HEX
            fields::label_right(
                ui,
                Rect::from_min_max(o + vec2(96.0, 191.0), o + vec2(133.0, 212.0)),
                &texts::t("pi-color-hex"),
                true,
            );
            let hex_r = Rect::from_min_max(o + vec2(137.0, 191.0), o + vec2(234.0, 212.0));
            let mut hex = s.hex.clone();
            if field(ui, pid.with("hex"), hex_r, &mut hex, false).changed() {
                // every keystroke sets the color (re 46 §2.3); the field keeps the typed text
                s.hex = hex.clone();
                let v = hex_value(&hex);
                s.work = Some(Argb(0xff00_0000 | v));
                s.dirty = true;
                s.rgb = [
                    (v >> 16 & 0xff).to_string(),
                    (v >> 8 & 0xff).to_string(),
                    (v & 0xff).to_string(),
                ];
                // alpha comes from the opacity field; at 0 it jumps to 100
                if footer.has_opacity() && s.opacity == 0 {
                    s.opacity = 100;
                    s.op = "100".into();
                    s.opacity_dirty = true;
                }
            }
            if copy_button(
                ui,
                pid.with("copy-hex"),
                Rect::from_min_max(o + vec2(238.0, 193.0), o + vec2(254.0, 209.0)),
                &texts::t("pi-color-copy-hex"),
            ) {
                ctx.copy_text(hex_text(shown.unwrap_or(Argb(0xff00_0000))));
                close = true;
            }
            // Deckkraft (nicht bei der Leinwandfarbe, PAR 16/02)
            if footer.has_opacity() {
                let ck = Rect::from_min_max(o + vec2(3.0, 219.0), o + vec2(13.0, 229.0));
                super::fields::checker(&p, ck, 2.5, &t);
                p.rect_stroke(ck, 0.0, Stroke::new(1.0, c.field_border), StrokeKind::Inside);
                let op_r = Rect::from_min_max(o + vec2(15.0, 214.0), o + vec2(46.0, 235.0));
                let mut op = s.op.clone();
                if field(ui, pid.with("op"), op_r, &mut op, false).changed() {
                    s.op = op.clone();
                    if let Some(v) = fields::parse_digits(&op) {
                        s.opacity = v.clamp(0.0, 100.0) as u8;
                        s.opacity_dirty = true;
                    }
                }
                let dd = Rect::from_min_max(o + vec2(47.0, 214.0), o + vec2(61.0, 235.0));
                let ddr = ui
                    .interact(dd, pid.with("op-dd"), Sense::click_and_drag())
                    .on_hover_text(texts::t("pi-color-opacity"));
                p.rect_stroke(
                    dd,
                    0.0,
                    Stroke::new(1.0, if ddr.hovered() { c.focus } else { c.field_border }),
                    StrokeKind::Inside,
                );
                super::glyphs::chevron_down(&p, dd, c.icon_fg);
                if ddr.clicked() {
                    s.slider = !s.slider;
                }
                // press on the arrow and drag: the slider follows the pointer, release applies
                if ddr.drag_started() {
                    s.slider = true;
                    s.slider_drag = Some((s.opacity, s.opacity_dirty));
                }
                if ddr.drag_stopped() && s.slider_drag.is_some() {
                    drag_released = true;
                }
            }
            // RGB
            fields::label_right(
                ui,
                Rect::from_min_max(o + vec2(96.0, 214.0), o + vec2(133.0, 235.0)),
                &texts::t("pi-color-rgb"),
                true,
            );
            for (k, (x0, x1)) in [(137.0, 168.0), (171.0, 202.0), (204.0, 235.0)]
                .into_iter()
                .enumerate()
            {
                let r = Rect::from_min_max(o + vec2(x0, 214.0), o + vec2(x1, 235.0));
                let mut b = s.rgb[k].clone();
                if field(ui, pid.with(["r", "g", "b"][k]), r, &mut b, false).changed() {
                    s.rgb[k] = b.clone();
                    if let Some(v) = fields::parse_digits(&b) {
                        let v = v.clamp(0.0, 255.0) as u32;
                        let w = s.work.unwrap_or(Argb(0xff00_0000)).0;
                        let shift = 16 - 8 * k as u32;
                        s.work = Some(Argb((w & !(0xff << shift)) | v << shift | 0xff00_0000));
                        s.dirty = true;
                        s.hex = hex_text(s.work.unwrap_or(Argb(0xff00_0000)));
                    }
                }
            }
            if copy_button(
                ui,
                pid.with("copy-rgb"),
                Rect::from_min_max(o + vec2(238.0, 216.0), o + vec2(254.0, 232.0)),
                &texts::t("pi-color-copy-rgb"),
            ) {
                let col = if s.start.is_none() && !s.dirty && !s.opacity_dirty {
                    None
                } else {
                    shown.map(|x| s.finish(x))
                };
                ctx.copy_text(rgba_text(col));
                close = true;
            }
            // Fußleiste
            p.hline(
                (o.x + 1.0)..=(o.x + POPUP_W - 1.0),
                o.y + 245.5,
                Stroke::new(1.0, c.line_dark),
            );
            let btn = |cx: f32| Rect::from_center_size(o + vec2(cx, 260.5), vec2(20.0, 20.0));
            if footer.has_none()
                && fields::toggle(
                    ui,
                    pid.with("none"),
                    btn(16.0),
                    Glyph::FillNone,
                    s.start.is_none(),
                    true,
                    &texts::t("pi-fill-none"),
                )
            {
                out = Some(ColorChoice::None);
                close = true;
            }
            match footer {
                Footer::Fill { kinds } => {
                    // Einfarbig: gedrückt, lässt das Popup offen und ändert noch nichts (§2.3)
                    fields::toggle(
                        ui,
                        pid.with("solid"),
                        btn(40.5),
                        Glyph::FillSolid,
                        s.start.is_some(),
                        true,
                        &texts::t("pi-fill-solid"),
                    );
                    if fields::toggle(
                        ui,
                        pid.with("gradient"),
                        btn(64.5),
                        Glyph::FillGradient,
                        false,
                        kinds,
                        &texts::t("pi-fill-gradient"),
                    ) {
                        out = Some(ColorChoice::Gradient);
                        close = true;
                    }
                    if fields::toggle(
                        ui,
                        pid.with("pattern"),
                        btn(88.5),
                        Glyph::FillPattern,
                        false,
                        kinds,
                        &texts::t("pi-fill-pattern"),
                    ) {
                        out = Some(ColorChoice::Pattern);
                        close = true;
                    }
                }
                Footer::FillOptions => {
                    let r = Rect::from_min_size(btn(40.5).min, vec2(90.0, btn(40.5).height()));
                    if ui.put(r, egui::Button::new("Fill Options...")).clicked() {
                        out = Some(ColorChoice::FillOptions);
                        close = true;
                    }
                }
                Footer::Stroke { placement, enabled } => {
                    for (k, (g, tip)) in [
                        (Glyph::StrokeInside, "pi-stroke-inside"),
                        (Glyph::StrokeCenter, "pi-stroke-center"),
                        (Glyph::StrokeOutside, "pi-stroke-outside"),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        if fields::toggle(
                            ui,
                            pid.with(("place", k)),
                            btn(40.5 + 24.0 * k as f32),
                            g,
                            placement == Some(k as u8),
                            enabled,
                            &texts::t(tip),
                        ) {
                            out = Some(ColorChoice::Placement(k as u8));
                            close = true;
                        }
                    }
                }
                Footer::Plain | Footer::NoneOnly | Footer::Canvas => {}
            }
            let wr = btn(234.5);
            let wresp = fields::icon_button(
                ui,
                swatch_pid.with("sysb"),
                wr,
                Glyph::Picker,
                true,
                &texts::t("pi-color-system"),
            );
            p.rect_filled(wr.shrink(3.0), 2.0, c.menu_bg);
            paint_wheel(&p, wr.center(), 6.0);
            if wresp.clicked() {
                s.sys = true;
                ctx.data_mut(|d| d.insert_temp(swatch_pid.with("sys"), true));
            }
            // ---- Klappschieber der Deckkraft (senkrecht, oben = 100)
            if s.slider {
                let r = slider_rect;
                let resp = ui.interact(r, pid.with("op-slider"), Sense::click_and_drag());
                ui.painter().rect_filled(r, 0.0, c.menu_bg);
                ui.painter()
                    .rect_stroke(r, 0.0, Stroke::new(1.0, c.menu_border), StrokeKind::Inside);
                let (tt, tl) = alpha_track(r);
                let track =
                    Rect::from_min_max(pos2(r.center().x - 2.0, tt), pos2(r.center().x + 2.0, tt + tl));
                ui.painter().rect_filled(track, 1.0, c.field_bg);
                ui.painter()
                    .rect_stroke(track, 1.0, Stroke::new(1.0, c.field_border), StrokeKind::Inside);
                let at = if s.slider_drag.is_some() {
                    pointer
                } else {
                    resp.interact_pointer_pos()
                        .filter(|_| resp.dragged() || resp.clicked() || resp.is_pointer_button_down_on())
                };
                if let Some(pp) = at {
                    let v = (1.0 - (pp.y - track.top()) / track.height()).clamp(0.0, 1.0);
                    s.opacity = (v * 100.0).round() as u8;
                    s.opacity_dirty = true;
                    s.op = s.opacity.to_string();
                }
                let ky = track.bottom() - track.height() * s.opacity as f32 / 100.0;
                let knob = Rect::from_center_size(pos2(r.center().x, ky), vec2(16.0, 7.0));
                ui.painter().rect_filled(knob, 1.0, c.icon_fg);
            }
            // ---- Farbdialog (Farbkreis)
            if s.sys {
                sys_res = sys_dialog(ui, swatch_pid, s, sys_rect);
            }
        });
    ctx.move_to_top(back_layer);
    if close {
        return (out, true);
    }
    // release after dragging from the opacity arrow: apply and close (RE-29, re 61 §2)
    if drag_released || (s.slider_drag.is_some() && !ctx.input(|i| i.pointer.primary_down())) {
        s.slider_drag = None;
        s.slider = false;
        let rgb = s.work.unwrap_or(Argb(0xff00_0000));
        return (Some(ColorChoice::Color(s.finish(rgb))), true);
    }
    if s.sys {
        match sys_res {
            Some(Some(col)) => return (Some(ColorChoice::Color(s.finish(col))), true),
            Some(None) => {
                s.sys = false;
                ctx.data_mut(|d| d.insert_temp(swatch_pid.with("sys"), false));
            }
            None => {}
        }
    }
    // ---- Zeiger: Pipette (eigener Zeiger) über allem außer Eingabefeldern
    if let Some(p) = pointer
        && !s.sys
    {
        let over_field = [
            (137.0, 191.0, 234.0, 212.0),
            (15.0, 214.0, 46.0, 235.0),
            (137.0, 214.0, 235.0, 235.0),
        ]
        .iter()
        .any(|(a, b, cx, d)| Rect::from_min_max(o + vec2(*a, *b), o + vec2(*cx, *d)).contains(p));
        let over_controls = rect.contains(p) && !grid.contains(p);
        if !over_field && !over_controls && !(s.slider && slider_rect.contains(p)) {
            // FireCraft: the crosshair while picking (FireCraft has no eyedropper cursor bitmap)
            if !shot_frame && s.shot_pending.is_none() {
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
            }
        }
    }
    let _ = ui;
    (None, false)
}

/// Top end (thumb of 100 %) and length of the track of the alpha slider `r` (from
/// [`slider_rect`]): like the opacity pop-ups, 2 px per per cent with 100 % 10 px below the top edge
/// (R-32, [`crate::inspector::fields::VTrack::PERCENT`], RE spec 60 §2.3 / 61 §4.1); the slider gives
/// whole per cents.
pub fn alpha_track(r: Rect) -> (f32, f32) {
    use super::fields::{VSLIDER_PERCENT_PAD, VSLIDER_PERCENT_TRACK};
    (r.top() + 2.0 + VSLIDER_PERCENT_PAD, VSLIDER_PERCENT_TRACK)
}

/// Klappschieber der Deckkraft (24 × 222) unter seinem Knopf; reicht das Fenster nach unten nicht,
/// wird er wie das Popup selbst nach oben in das Fenster geschoben (CT-2, 900 × 650).
pub fn slider_rect(o: Pos2, screen: Rect) -> Rect {
    let size = vec2(24.0, 222.0);
    let top = (o.y + 235.0)
        .min(screen.bottom() - size.y - 2.0)
        .max(screen.top());
    Rect::from_min_size(pos2(o.x + 47.0, top), size)
}

fn sys_rect(o: Pos2, screen: Rect) -> Rect {
    let size = vec2(300.0, 400.0);
    let mut min = pos2(o.x + POPUP_W + 6.0, o.y);
    if min.x + size.x > screen.right() {
        min.x = (o.x - size.x - 6.0).max(screen.left());
    }
    min.y = min.y.min(screen.bottom() - size.y).max(screen.top());
    Rect::from_min_size(min, size)
}

/// Farbdialog hinter dem Farbkreis-Knopf. Das Original öffnet den Windows-Farbdialog (`ChooseColor`);
/// den gibt es ohne `unsafe` in keiner vorhandenen sicheren Bindung (rfd kennt nur Datei-Dialoge),
/// darum ein eigener Dialog mit Farbfläche, Farbtonleiste und Übernehmen/Abbrechen. Das Popup bleibt
/// dahinter offen; Esc bzw. Abbrechen führt zurück ins Popup. Liefert `Some(Some(farbe))` beim
/// Übernehmen, `Some(None)` beim Abbrechen.
fn sys_dialog(ui: &mut Ui, swatch_pid: Id, s: &CpState, r: Rect) -> Option<Option<Argb>> {
    let t = Theme::current(ui.ctx());
    let c = &t.colors;
    let col_key = swatch_pid.with("syscol");
    let mut res = None;
    let rr = r;
    ui.painter().rect_filled(rr, 0.0, c.menu_bg);
    ui.painter()
        .rect_stroke(rr, 0.0, Stroke::new(1.0, c.menu_border), StrokeKind::Inside);
    ui.painter().text(
        rr.left_top() + vec2(10.0, 12.0),
        Align2::LEFT_CENTER,
        texts::t("pi-color-dialog-title"),
        t.ui_font(),
        c.text,
    );
    let start = s
        .work
        .map(|w| Color32::from_rgb(w.r(), w.g(), w.b()))
        .unwrap_or(Color32::WHITE);
    let mut rgb = ui.data(|d| d.get_temp::<Color32>(col_key)).unwrap_or(start);
    let pick = Rect::from_min_max(rr.min + vec2(10.0, 26.0), rr.max - vec2(10.0, 44.0));
    ui.scope_builder(egui::UiBuilder::new().max_rect(pick), |ui| {
        ui.set_clip_rect(pick);
        ui.spacing_mut().slider_width = 200.0;
        if egui::color_picker::color_picker_color32(ui, &mut rgb, egui::color_picker::Alpha::Opaque) {
            ui.data_mut(|d| d.insert_temp(col_key, rgb));
        }
    });
    let (ok_l, cancel_l) = (texts::t("btn-apply"), texts::t("btn-cancel"));
    let by = rr.bottom() - 32.0;
    let ok_r = Rect::from_min_size(pos2(rr.right() - 10.0 - 2.0 * 90.0 - 8.0, by), vec2(90.0, 23.0));
    let cancel_r = Rect::from_min_size(pos2(rr.right() - 10.0 - 90.0, by), vec2(90.0, 23.0));
    if super::fields::push_button(ui, ok_r, &ok_l, true, true) {
        let col = ui.data(|d| d.get_temp::<Color32>(col_key)).unwrap_or(rgb);
        ui.data_mut(|d| d.remove::<Color32>(col_key));
        res = Some(Some(Argb(
            0xff00_0000 | (col.r() as u32) << 16 | (col.g() as u32) << 8 | col.b() as u32,
        )));
    }
    if super::fields::push_button(ui, cancel_r, &cancel_l, false, true) {
        ui.data_mut(|d| d.remove::<Color32>(col_key));
        res = Some(None);
    }
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Q-49 (re 46 §2.2): HEX field parser examples.
    #[test]
    fn hex_field_parser_like_the_original() {
        for (text, want) in [
            ("3366CC", 0x3366cc),
            ("#3366cc", 0x3366cc),
            ("3366cc", 0x3366cc),
            ("36c", 0x3366cc),
            ("#36C", 0x3366cc),
            ("0x36c", 0x00036c),
            ("0X3366CC", 0x3366cc),
            ("zz12", 0x000000),
            ("12zz", 0x120000),
            ("abcd", 0xabcd00),
            ("abcde", 0xabcd0e),
            ("#12345678", 0x123456),
            ("", 0xffffff),
            ("#", 0x000000),
            ("3", 0x030000),
            ("3366", 0x336600),
            ("aBcDeF", 0xabcdef),
            ("1 2 3 4", 0x010203),
            ("256 257 1", 0x000101),
        ] {
            assert_eq!(hex_value(text), want, "{text:?}");
        }
    }

    #[test]
    fn grid_formula_spot_checks() {
        assert_eq!(grid_color(0, 0).0 & 0xff_ffff, 0xFFBFBF);
        assert_eq!(grid_color(1, 5).0 & 0xff_ffff, 0xFF4000);
        assert_eq!(grid_color(0, 10).0 & 0xff_ffff, 0x400000);
        assert_eq!(grid_color(14, 13).0 & 0xff_ffff, 0x131304);
        assert_eq!(grid_color(14, 14).0 & 0xff_ffff, 0x0F0F1E);
        assert_eq!(grid_color(20, 14).0 & 0xff_ffff, 0xFF007F);
    }

    #[test]
    fn placement_above_and_clamped_at_top() {
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1600.0, 1000.0));
        let anchor = Rect::from_min_size(pos2(100.0, 500.0), vec2(20.0, 16.0));
        assert_eq!(popup_pos(anchor, 105.0, screen), pos2(105.0, 500.0 - POPUP_H));
        let high = Rect::from_min_size(pos2(100.0, 40.0), vec2(20.0, 16.0));
        assert_eq!(popup_pos(high, 105.0, screen).y, 0.0);
        assert_eq!(popup_pos(anchor, 1590.0, screen).x, 1600.0 - POPUP_W);
    }
}

/// Id base of a swatch's colour picker (FirePixel's `popups::picker_id`; colour dialog:
/// `.with("sys")`, `.with("sysb")`, `.with("syscol")`).
pub fn picker_id(id: Id) -> Id {
    id.with("picker")
}

/// FireCraft: a colour box (FireCraft's swatch look) of `size` that opens the picker; the choice
/// made, if any. `id` must be stable across frames.
pub fn color_box(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    size: egui::Vec2,
    color: Option<firecraft_raster::Rgba>,
    footer: Footer,
) -> Option<ColorChoice> {
    let (r, _) = ui.allocate_exact_size(size, Sense::hover());
    let resp = ui.interact(r, Id::unique(id), Sense::click());
    crate::tools_panel::paint_swatch(ui.painter(), r, color);
    let argb = color.map(to_argb);
    popup(ui, &resp, &|_| None, argb, footer)
}

pub fn to_argb(c: firecraft_raster::Rgba) -> Argb {
    Argb((c.a as u32) << 24 | (c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32)
}

pub fn to_rgba(c: Argb) -> firecraft_raster::Rgba {
    firecraft_raster::Rgba::new(c.r(), c.g(), c.b(), c.a())
}
