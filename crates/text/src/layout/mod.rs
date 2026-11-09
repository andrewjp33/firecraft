//! Satz von Original-Text (09 §5, Laufzeitergebnisse 8a).
//!
//! # Breitenmodell (09 §5.6, B)
//!
//! ```text
//! v_i = round_half_up( ((advance(g_i) + kern_i) · PTSf/upm + fb) · HSCf ) + t
//! t   = RKNf · PTSf / 10      hinter JEDER Glyphe, nicht gerundet
//! fb  = 0,027 · PTSf bei Faux-Fett
//! kern_i: Paar-Kerning der Schrift zur nächsten Glyphe gleicher Schrift und Größe (auch über
//!         Laufgrenzen, T7); an, wenn die Story-Kerning-Art (/11) ≥ 1 bzw. ohne Story ATKb
//! ```
//! Ausrichtung mit `w_a = Σ v_i − t_letzte`; Rahmen = Vereinigung aus Vorschubbox und Tinten-Box
//! (Faux-Fett links `PTSf/72`, rechts `PTSf/36`, Faux-Kursiv geschert), nach außen gerundet.
//! Oben `TOP = Anker_y − ceil(asc·PTSf)` (erste Zeile), unten `BOT = ceil(letzte Grundlinie +
//! desc·PTSf)`.
//!
//! # Rahmenarten
//!
//! - **Punkttext** (`TFTi 0`): Zeilen nur an `\r`, `\u{3}`, `\n`; Anker = Ausrichtungspunkt der ersten
//!   Grundlinie. Bitgleich zur Python-Referenz `layout.layout_point_text` (bis auf Tabulatoren, die
//!   die Referenz nicht kennt, und die Absatzausrichtung je Absatz).
//! - **Rahmen fester Breite** (`TFTi 1`, A): Umbruch an Leerzeichen (und nach Bindestrichen),
//!   Erstzeileneinzug `PINf`, Blocksatz; Höhe = Oberkante + em-Höhe der ersten Zeile + Zeilenabstände
//!   (Testdateien T1/T2/T8: 200×100 → 80 px, Zeilenabstand exakt 30 → 110 px; 8/8 ohne den
//!   JS-Sonderfall `setTextRectangleAuto`), erste Grundlinie = Oberkante + lateinische Oberlänge
//!   ([`Font::roman_ascent`], Pixelvergleich T1/T2/T8).
//! - **Text auf Pfad** (`TFTi 2`, A): ungerundete Vorschübe (T3: Rahmen an der Linie exakt), Glyphenmitte
//!   auf dem Pfad, Drehung (`TAMi 0`) bzw. aufrecht/geschert nach `TAMi`, Versatz `CHOf`, Ausrichtung
//!   über die Pfadlänge; Laufweite entlang der um [`PATH_TEXT_OFFSET`] bzw.
//!   [`PATH_TEXT_OFFSET_UPRIGHT`] zur Glyphenoberseite versetzten Kurve (Geraden unverändert);
//!   Glyphen jenseits der Pfadenden sind unsichtbar. Rahmen = Hülle der Boxen
//!   (Tinte waagerecht, em-Ober-/Unterlänge senkrecht) der sichtbaren Glyphen.
//! - **Text in Pfad** (`TFTi 3`, A): Umbruch in den waagerechten Spannen der Form, erste Grundlinie =
//!   Formoberkante + lateinische Oberlänge, Zeilen außerhalb der Form unsichtbar (T2: Rahmen exakt).
//! - **Vertikal** (`ORIi` 1/3, A): eine Spalte je Zeile, lateinische Glyphen um 90° gedreht; Rahmen
//!   `PTSf` breit auf dem Anker zentriert, Höhe = Zeilenbreite (T5).
//! - **RTL** (`ORIi` 2): Ausrichtung und Rahmen wie links→rechts (T5); Abschnitte mit RTL-Schrift
//!   (Hebräisch/Arabisch) werden umgekehrt gesetzt (vereinfachte Bidi-Regel, kein vollständiges UAX #9).

mod hit;
pub mod path;
mod types;

pub use hit::{Caret, HitResult};
pub use types::*;

use std::sync::Arc;

use crate::support::Rect;

use crate::font::{Font, FontDb};
use crate::model::{Alignment, CharAttrs, FrameKind, Orientation, PathTextMode, TextBlock};
use crate::raster::Affine;

/// Versatz (px) der Kurve, entlang der das Original die Laufweite von Text auf Pfad misst, für
/// Modi mit gedrehter Grundlinie (drehen, waagerecht verzerrt), zur Glyphenoberseite. A, Laufzeit
/// phase5 `pt_kreis`, `pt_kreis2`: driftfrei wären Arial 20 pt ≈ 5,3, Arial 40 pt ≈ 6,6,
/// Myriad 20 pt ≈ 2,8, innen ≈ 7 – eine geschlossene Regel ergibt sich daraus nicht.
pub const PATH_TEXT_OFFSET: f64 = 5.0;

/// Versatz für aufrechte Glyphen (Modus senkrecht, vertikal verzerrt; Korpus 09 T3 am besten bei 2).
pub const PATH_TEXT_OFFSET_UPRIGHT: f64 = 2.0;

/// Shown width (px) of a horizontal fixed-width frame of width 0 (re 63 section 2.2, B: script
/// `addNewText` with a zero-width rect gives an object 14 px wide and no ink).
pub const ZERO_WIDTH_FRAME: f64 = 10.0;

/// Umbruchart eines Steuerzeichens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Break {
    /// `\r`: Absatzende.
    Paragraph,
    /// `\u{3}` oder `\n`: Zeilenumbruch im Absatz.
    Line,
}

fn break_kind(ch: char) -> Option<Break> {
    match ch {
        '\r' => Some(Break::Paragraph),
        '\n' | '\u{3}' | '\u{2028}' => Some(Break::Line),
        '\u{2029}' => Some(Break::Paragraph),
        _ => None,
    }
}

/// Zeichen mit starker RTL-Richtung (Hebräisch, Arabisch, Syrisch, Thaana, NKo, Präsentationsformen).
fn is_rtl(ch: char) -> bool {
    matches!(ch as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF)
}

/// Zeichen mit starker LTR-Richtung (Buchstaben/Ziffern außerhalb der RTL-Bereiche).
fn is_strong_ltr(ch: char) -> bool {
    ch.is_alphanumeric() && !is_rtl(ch)
}

/// Ein gesetztes Element (Glyphe) während des Satzes.
#[derive(Debug, Clone)]
struct Item {
    ch: char,
    char_index: usize,
    char_len: usize,
    run: usize,
    font: Arc<Font>,
    gid: u16,
    kern_on: bool,
    tab: bool,
    /// HarfBuzz: Vorschub in Font-Einheiten (inkl. Kerning) und Versatz.
    hb_advance: Option<(i32, i32, i32)>,
    adv: f64,
    trk: f64,
}

/// Rohzeile vor dem Positionieren.
#[derive(Debug, Clone)]
struct RawLine {
    items: Vec<Item>,
    /// Lauf, der für leere Zeilen und Absatzabstände gilt (Lauf des Umbruchzeichens davor).
    meta_run: usize,
    para_start: bool,
    /// Zeichenbereich inklusive abschließendem Umbruchzeichen.
    chars: std::ops::Range<usize>,
    /// Breite für die Ausrichtung beim Umbruch (ohne Leerzeichen am Ende).
    wrapped: bool,
    last_of_paragraph: bool,
}

/// Setzt einen Textblock mit den Systemschriften ([`FontDb::shared`]) und Standardeinstellungen.
pub fn layout_simple(block: &TextBlock) -> TextLayout {
    layout(block, &FontDb::shared(), &LayoutOptions::default())
}

/// Setzt einen Textblock. Fehlende Schriften werden durch die Rückfall-Schrift ersetzt (Name in
/// [`TextLayout::missing_fonts`]); ohne jede Schrift entsteht ein Layout ohne Glyphen.
pub fn layout(block: &TextBlock, fonts: &FontDb, opts: &LayoutOptions) -> TextLayout {
    Builder::new(block, fonts, opts).run()
}

struct Builder<'a> {
    block: &'a TextBlock,
    opts: &'a LayoutOptions,
    run_fonts: Vec<Option<Arc<Font>>>,
    missing: Vec<String>,
    default_attrs: CharAttrs,
}

impl<'a> Builder<'a> {
    fn new(block: &'a TextBlock, fonts: &FontDb, opts: &'a LayoutOptions) -> Self {
        let mut missing = Vec::new();
        let mut cache: Vec<(String, Option<Arc<Font>>)> = Vec::new();
        let run_fonts = block
            .runs
            .iter()
            .map(|r| {
                if let Some((_, f)) = cache.iter().find(|(n, _)| *n == r.attrs.font) {
                    return f.clone();
                }
                let res = fonts.resolve(&r.attrs.font, false, false);
                if res.as_ref().is_none_or(|x| x.substituted) && !missing.contains(&r.attrs.font) {
                    missing.push(r.attrs.font.clone());
                }
                let f = res.map(|x| x.font);
                cache.push((r.attrs.font.clone(), f.clone()));
                f
            })
            .collect();
        Self {
            block,
            opts,
            run_fonts,
            missing,
            default_attrs: CharAttrs::default(),
        }
    }

    fn attrs(&self, run: usize) -> &CharAttrs {
        self.block
            .runs
            .get(run)
            .map(|r| &r.attrs)
            .unwrap_or(&self.default_attrs)
    }

    /// Kerning-Schalter je Zeichen (Story `/11` vor `ATKb`).
    fn kern_flags(&self, n: usize) -> Vec<bool> {
        let mut v = vec![self.block.auto_kern; n];
        if let Some(sk) = &self.block.story_kerning {
            let mut i = 0usize;
            for r in sk {
                for _ in 0..r.len {
                    if i >= n {
                        break;
                    }
                    if let Some(k) = r.kind {
                        v[i] = k.is_on();
                    }
                    i += 1;
                }
            }
        }
        v
    }

    fn run(self) -> TextLayout {
        let n_chars = self.block.char_count();
        let kern = self.kern_flags(n_chars);
        // 1. Zeichen → Elemente, Zeilen an Umbruchzeichen
        let mut lines: Vec<RawLine> = vec![RawLine {
            items: Vec::new(),
            meta_run: 0,
            para_start: true,
            chars: 0..0,
            wrapped: false,
            last_of_paragraph: true,
        }];
        let mut ci = 0usize;
        for (ri, r) in self.block.runs.iter().enumerate() {
            let font = self.run_fonts[ri].clone();
            for ch in r.text.chars() {
                let idx = ci;
                ci += 1;
                if let Some(b) = break_kind(ch) {
                    if let Some(l) = lines.last_mut() {
                        l.chars.end = idx + 1;
                        l.last_of_paragraph = b == Break::Paragraph;
                    }
                    lines.push(RawLine {
                        items: Vec::new(),
                        meta_run: ri,
                        para_start: b == Break::Paragraph,
                        chars: idx + 1..idx + 1,
                        wrapped: false,
                        last_of_paragraph: true,
                    });
                    continue;
                }
                let Some(font) = font.clone() else { continue };
                let tab = ch == '\t';
                let gid = if tab {
                    font.glyph_index(' ')
                } else {
                    font.glyph_index(ch)
                };
                if let Some(l) = lines.last_mut() {
                    l.chars.end = idx + 1;
                    l.items.push(Item {
                        ch,
                        char_index: idx,
                        char_len: 1,
                        run: ri,
                        font,
                        gid,
                        kern_on: kern.get(idx).copied().unwrap_or(true),
                        tab,
                        hb_advance: None,
                        adv: 0.0,
                        trk: 0.0,
                    });
                }
            }
        }
        // 2. Shaping (optional) und Vorschübe
        let unrounded = self.block.kind == FrameKind::OnPath;
        for l in &mut lines {
            if self.opts.shaping == Shaping::Harfbuzz {
                self.shape_line(l);
            }
            self.advances(&mut l.items, unrounded);
        }
        // 3. Umbruch bei Rahmen- und Formtext
        match self.block.kind {
            FrameKind::FixedWidth => {
                let w = (self.block.bounds.right - self.block.bounds.left) as f64;
                lines = self.wrap(lines, &|_| Some((0.0, w)));
            }
            FrameKind::InPath => return self.in_path(lines, n_chars),
            _ => {}
        }
        self.position(lines, n_chars)
    }

    /// Vorschübe nach dem Breitenmodell (09 §5.6). Tabulatoren erhalten ihren Vorschub erst beim
    /// Positionieren.
    fn advances(&self, items: &mut [Item], unrounded: bool) {
        let n = items.len();
        for i in 0..n {
            let a = self.attrs(items[i].run).clone();
            let k = a.size / items[i].font.units_per_em() as f64;
            let trk = a.tracking_px();
            items[i].trk = trk;
            if items[i].tab {
                items[i].adv = 0.0;
                continue;
            }
            let base = match items[i].hb_advance {
                Some((adv, _, _)) => adv as f64,
                None => {
                    let next = items.get(i + 1).filter(|nx| {
                        !nx.tab && Arc::ptr_eq(&nx.font, &items[i].font) && self.attrs(nx.run).size == a.size
                    });
                    let kern = match next {
                        Some(nx) if items[i].kern_on => items[i].font.kerning(items[i].gid, nx.gid),
                        _ => 0,
                    };
                    items[i].font.advance(items[i].gid) as f64 + kern as f64
                }
            };
            let extra = if a.bold { FAUX_BOLD_ADVANCE * a.size } else { 0.0 };
            let v = (base * k + extra) * a.h_scale;
            items[i].adv = if unrounded {
                v + trk
            } else {
                (v + 0.5).floor() + trk
            };
        }
    }

    /// HarfBuzz-Shaping einer Zeile: Abschnitte gleicher Schrift/Größe/Richtung werden mit rustybuzz
    /// gesetzt (Ligaturen an, Kerning nach Schalter).
    fn shape_line(&self, line: &mut RawLine) {
        let items = std::mem::take(&mut line.items);
        let mut out: Vec<Item> = Vec::with_capacity(items.len());
        let mut i = 0;
        while i < items.len() {
            let mut j = i + 1;
            let rtl = is_rtl(items[i].ch);
            while j < items.len()
                && !items[j].tab
                && !items[i].tab
                && Arc::ptr_eq(&items[j].font, &items[i].font)
                && items[j].run == items[i].run
                && items[j].kern_on == items[i].kern_on
                && (is_rtl(items[j].ch) == rtl || !is_strong_ltr(items[j].ch) && !is_rtl(items[j].ch))
            {
                j += 1;
            }
            let seg = &items[i..j];
            if seg[0].tab {
                out.extend_from_slice(seg);
                i = j;
                continue;
            }
            match shape_segment(seg, rtl) {
                Some(mut v) => out.append(&mut v),
                None => out.extend_from_slice(seg),
            }
            i = j;
        }
        line.items = out;
    }

    /// Greedy-Umbruch an Leerzeichen (und nach Bindestrichen). `span(i)` liefert (x links, Breite)
    /// der Ausgabezeile `i`; `None` = kein Platz mehr (der Rest bleibt in einer Überlaufzeile).
    fn wrap(&self, lines: Vec<RawLine>, span: &dyn Fn(usize) -> Option<(f64, f64)>) -> Vec<RawLine> {
        let mut out: Vec<RawLine> = Vec::new();
        for l in lines {
            let indent = if l.para_start {
                l.items.first().map(|it| self.attrs(it.run).indent).unwrap_or(0.0)
            } else {
                0.0
            };
            if l.items.is_empty() {
                out.push(l);
                continue;
            }
            let mut start = 0usize;
            let mut first = true;
            let items = &l.items;
            while start < items.len() {
                let avail = span(out.len()).map(|s| s.1).unwrap_or(f64::INFINITY);
                let width = if first { avail - indent } else { avail };
                let mut x = 0.0;
                let mut last_break: Option<usize> = None;
                let mut end = items.len();
                for (k, it) in items.iter().enumerate().skip(start) {
                    let adv = if it.tab { self.tab_advance(x) } else { it.adv };
                    let fit = x + adv - it.trk;
                    if !it.ch.is_whitespace() && fit > width + 1e-9 && k > start {
                        end = match last_break {
                            Some(b) if b > start => b,
                            _ => k,
                        };
                        break;
                    }
                    x += adv;
                    if it.ch == ' ' || it.ch == '\u{3000}' || it.ch == '-' {
                        last_break = Some(k + 1);
                    }
                }
                let chunk: Vec<Item> = items[start..end].to_vec();
                let c0 = if first {
                    l.chars.start
                } else {
                    chunk.first().map(|c| c.char_index).unwrap_or(l.chars.start)
                };
                let c1 = if end >= items.len() {
                    l.chars.end
                } else {
                    items[end].char_index
                };
                out.push(RawLine {
                    items: chunk,
                    meta_run: l.meta_run,
                    para_start: first && l.para_start,
                    chars: c0..c1,
                    wrapped: end < items.len(),
                    last_of_paragraph: end >= items.len() && l.last_of_paragraph,
                });
                first = false;
                start = end;
            }
        }
        out
    }

    /// Vorschub eines Tabulators an der Stelle `x` (relativ zum Zeilenbeginn): bis zum nächsten
    /// Vielfachen von [`LayoutOptions::tab_interval`], das mindestens `tab_min_gap` entfernt ist.
    fn tab_advance(&self, x: f64) -> f64 {
        let iv = self.opts.tab_interval.max(1.0);
        let mut stop = ((x / iv).floor() + 1.0) * iv;
        while stop - x < self.opts.tab_min_gap {
            stop += iv;
        }
        stop - x
    }

    /// Relative Lagen der Elemente einer Zeile (Tabulatoren aufgelöst, RTL-Abschnitte umgekehrt):
    /// liefert (Reihenfolge der Elemente in visueller Folge, x je Element relativ, Breite, w_a).
    fn arrange(&self, items: &mut [Item]) -> (Vec<usize>, Vec<f64>, f64, f64) {
        let mut x = 0.0;
        for it in items.iter_mut() {
            if it.tab {
                it.adv = self.tab_advance(x);
            }
            x += it.adv;
        }
        let w = x;
        let wa = items.last().map(|l| w - l.trk).unwrap_or(0.0);
        let order = self.visual_order(items);
        let mut xs = vec![0.0; items.len()];
        let mut x = 0.0;
        for &i in &order {
            xs[i] = x;
            x += items[i].adv;
        }
        (order, xs, w, wa)
    }

    /// Visuelle Reihenfolge: RTL-Abschnitte umgekehrt; bei RTL-Absatzrichtung auch die Abschnitte.
    fn visual_order(&self, items: &[Item]) -> Vec<usize> {
        let n = items.len();
        let para_rtl = self.block.orientation == Orientation::HorizontalRtl;
        if !items.iter().any(|it| is_rtl(it.ch)) {
            // rein lateinische Zeilen stehen auch bei RTL-Absatzrichtung wie links→rechts (T5)
            return (0..n).collect();
        }
        // Richtung je Element: stark RTL/LTR, neutral erbt die vorige starke Richtung
        let mut cur = para_rtl;
        let mut segs: Vec<(bool, Vec<usize>)> = Vec::new();
        for (i, it) in items.iter().enumerate() {
            if is_rtl(it.ch) {
                cur = true;
            } else if is_strong_ltr(it.ch) {
                cur = false;
            }
            match segs.last_mut() {
                Some((d, v)) if *d == cur => v.push(i),
                _ => segs.push((cur, vec![i])),
            }
        }
        // HarfBuzz liefert RTL-Abschnitte bereits in visueller Folge
        for (d, v) in segs.iter_mut() {
            if *d && self.opts.shaping == Shaping::Original {
                v.reverse();
            }
        }
        if para_rtl {
            segs.reverse();
        }
        segs.into_iter().flat_map(|(_, v)| v).collect()
    }

    /// Oberlänge (px, inkl. Grundlinienversatz) und Unterlänge einer Zeile; leere Zeilen nutzen die
    /// Attribute `fallback_run`.
    fn line_extents(&self, items: &[Item], fallback_run: usize) -> (f64, f64, f64) {
        let mut asc = f64::NEG_INFINITY;
        let mut desc = f64::NEG_INFINITY;
        let mut emb = f64::NEG_INFINITY;
        for it in items {
            let a = self.attrs(it.run);
            asc = asc.max(it.font.ascent() * a.size + a.baseline_shift);
            desc = desc.max(it.font.descent() * a.size - a.baseline_shift);
            emb = emb.max((1.0 - it.font.ascent()) * a.size - a.baseline_shift);
        }
        if items.is_empty() {
            let a = self.attrs(fallback_run);
            match self.run_fonts.get(fallback_run).cloned().flatten() {
                Some(f) => {
                    asc = f.ascent() * a.size;
                    desc = f.descent() * a.size;
                    emb = (1.0 - f.ascent()) * a.size;
                }
                None => {
                    asc = 0.0;
                    desc = 0.0;
                    emb = 0.0;
                }
            }
        }
        (asc, desc, emb)
    }

    /// Größte lateinische Oberlänge (px, inkl. Grundlinienversatz) einer Zeile.
    fn roman_ascent(&self, items: &[Item], fallback_run: usize) -> f64 {
        let v = items
            .iter()
            .map(|it| {
                let a = self.attrs(it.run);
                it.font.roman_ascent() * a.size + a.baseline_shift
            })
            .fold(f64::NEG_INFINITY, f64::max);
        if v.is_finite() {
            return v;
        }
        let a = self.attrs(fallback_run);
        self.run_fonts
            .get(fallback_run)
            .cloned()
            .flatten()
            .map(|f| f.roman_ascent() * a.size)
            .unwrap_or(0.0)
    }

    /// Zeilenabstand vor einer Zeile (größter Wert der Zeichen der Zeile) und Absatzabstand.
    fn line_step(&self, l: &RawLine) -> f64 {
        let meta = l.items.first().map(|it| it.run).unwrap_or(l.meta_run);
        let mut step = f64::NEG_INFINITY;
        for it in &l.items {
            step = step.max(self.attrs(it.run).line_advance());
        }
        if l.items.is_empty() {
            step = self.attrs(meta).line_advance();
        }
        if l.para_start {
            let a = self.attrs(meta);
            step += a.space_after + a.space_before;
        }
        step
    }

    fn alignment_of(&self, l: &RawLine) -> Alignment {
        let run = l.items.first().map(|it| it.run).unwrap_or(l.meta_run);
        self.attrs(run).alignment
    }

    /// Positioniert waagerechte Zeilen (Punkttext, Rahmen) bzw. vertikale Spalten und Pfadtext.
    // Die Zeilenschleife greift auf Vorgänger-Daten und mehrere Felder derselben Zeile zu.
    #[allow(clippy::needless_range_loop)]
    fn position(mut self, mut lines: Vec<RawLine>, n_chars: usize) -> TextLayout {
        let b = self.block;
        let vertical = b.orientation.is_vertical();
        let fixed = b.kind == FrameKind::FixedWidth;
        let on_path = b.kind == FrameKind::OnPath && b.path.as_ref().is_some_and(|p| !p.contours.is_empty());
        // Text am Pfad zeigt wie im Original nur den ersten Absatz (m1 Nebenbefund zu Q-16: TX2 mit
        // drei Absätzen an P2 hat im Original nur die Tinte von „Zeile eins“, 50,390–104,408)
        let first_para_end = lines
            .iter()
            .find(|l| l.last_of_paragraph)
            .map(|l| l.chars.end)
            .unwrap_or(n_chars);
        if on_path {
            // Pfadtext ist einzeilig: Umbrüche entfallen
            let mut all = RawLine {
                items: Vec::new(),
                meta_run: 0,
                para_start: true,
                chars: 0..n_chars,
                wrapped: false,
                last_of_paragraph: true,
            };
            for l in lines.drain(..) {
                all.items.extend(l.items);
            }
            lines = vec![all];
        }
        // TW-40 (Q-15, TX1 10 px breit, 18/24 pt): Ist ein Zeichen breiter als der feste Textrahmen,
        // setzt das Original gar nichts – keine Tinte, und „In Pfade konvertieren“ ändert nichts
        let too_narrow = fixed && !vertical && {
            let width = (b.bounds.right - b.bounds.left) as f64;
            lines
                .iter()
                .flat_map(|l| l.items.iter())
                .any(|it| !it.tab && !it.ch.is_whitespace() && it.adv - it.trk > width + 1e-9)
        };
        let x_anchor = b.bounds.left as f64;
        let y_anchor = b.bounds.top as f64;
        let mut out_lines: Vec<LineLayout> = Vec::new();
        let mut glyphs: Vec<PositionedGlyph> = Vec::new();
        let mut decorations = Vec::new();
        let mut lefts: Vec<f64> = Vec::new();
        let mut rights: Vec<f64> = Vec::new();
        let first_ext = self.line_extents(&lines[0].items, 0);
        // Rahmentext: Höhe nach em-Boxen (oben asc·PTSf), Glyphen aber mit der ersten Grundlinie auf
        // Oberkante + lateinische Oberlänge (A, siehe Font::roman_ascent)
        let mut y = if fixed { y_anchor + first_ext.0 } else { y_anchor };
        let base_shift = if fixed {
            first_ext.0 - self.roman_ascent(&lines[0].items, 0)
        } else {
            0.0
        };
        let last_run = b.runs.len().saturating_sub(1);
        let n_lines = lines.len();
        let mut last_ext = first_ext;
        let mut last_base = y - base_shift;
        for li in 0..n_lines {
            if li > 0 {
                y += self.line_step(&lines[li]);
            }
            let fallback = if li == n_lines - 1 && lines[li].items.is_empty() {
                last_run
            } else {
                lines[li].meta_run
            };
            let ext = self.line_extents(&lines[li].items, if li == 0 { 0 } else { fallback });
            last_ext = ext;
            let align = self.alignment_of(&lines[li]);
            let para_start = lines[li].para_start;
            let wrapped = lines[li].wrapped;
            let last_of_para = lines[li].last_of_paragraph;
            let chars = lines[li].chars.clone();
            let items = &mut lines[li].items;
            // Paragraph indent (TW-204, re 60 section 3): the first line of every paragraph moves
            // by the indent, on point text as on fixed-width text (text on a path ignores it)
            let indent = if para_start && !on_path {
                items.first().map(|it| self.attrs(it.run).indent).unwrap_or(0.0)
            } else {
                0.0
            };
            // Leerzeichen am Ende umbrochener Zeilen zählen für die Ausrichtung nicht
            let (order, mut xs, w, mut wa) = self.arrange(items);
            if fixed && wrapped {
                let mut k = items.len();
                while k > 0 && items[k - 1].ch.is_whitespace() {
                    k -= 1;
                }
                wa = items[..k].iter().map(|it| it.adv).sum::<f64>()
                    - items[..k].last().map(|it| it.trk).unwrap_or(0.0);
            }
            let x0 = if fixed {
                let width = (b.bounds.right - b.bounds.left) as f64 - indent;
                let left = x_anchor + indent;
                let justify = match align {
                    Alignment::Justify => wrapped && !last_of_para,
                    Alignment::Stretch => true,
                    _ => false,
                };
                if justify {
                    justify_line(items, &order, &mut xs, width - wa);
                }
                match align {
                    Alignment::Center => left + (width - wa) / 2.0,
                    Alignment::Right => left + width - wa,
                    _ => left,
                }
            } else {
                indent
                    + match align {
                        Alignment::Center => x_anchor - wa / 2.0,
                        Alignment::Right => x_anchor - wa,
                        _ => x_anchor,
                    }
            };
            let g0 = glyphs.len();
            let mut carets = vec![x0 + w; chars.len() + 1];
            // TW-214 (re 63 section 2.3): fixed-width text sits on whole pixels, each line at
            // `round(top + d·s + k·step)` (Arial 12 pt: 109/123/137 from 108.6). The hinted
            // rasterizer rounds the baseline (`render::snapped_origin`); the layout keeps the exact
            // value, which "Convert to Paths" needs (TW-56b, Myriad Pro 14 pt ink bottom 194)
            for (k, it) in items.iter().enumerate() {
                let a = self.attrs(it.run).clone();
                let gx = x0 + xs[k];
                let gy = y - base_shift - a.baseline_shift;
                let ksz = a.size / it.font.units_per_em() as f64;
                let tan = if a.italic { self.opts.italic_shear } else { 0.0 };
                let (ox, oy) = it
                    .hb_advance
                    .map(|(_, dx, dy)| (dx as f64 * ksz, dy as f64 * ksz))
                    .unwrap_or((0.0, 0.0));
                let transform = Affine {
                    a: ksz * a.h_scale,
                    b: 0.0,
                    c: ksz * tan,
                    d: -ksz,
                    e: gx + ox,
                    f: gy - oy,
                };
                for c in 0..it.char_len {
                    let ci = it.char_index + c;
                    if ci >= chars.start && ci < chars.end {
                        carets[ci - chars.start] = gx + (it.adv - it.trk) * c as f64 / it.char_len as f64;
                    }
                }
                glyphs.push(PositionedGlyph {
                    font: it.font.clone(),
                    glyph_id: it.gid,
                    char_index: it.char_index,
                    char_len: it.char_len,
                    ch: it.ch,
                    run: it.run,
                    line: li,
                    x: gx,
                    y: gy,
                    size: a.size,
                    advance: it.adv,
                    tracking: it.trk,
                    h_scale: a.h_scale,
                    faux_bold: a.bold,
                    faux_italic: a.italic,
                    color: a.color,
                    transform,
                    visible: true,
                    is_tab: it.tab,
                });
            }
            // Caret am Ende eines Umbruchzeichens = Zeilenende; Zeichen ohne Glyphe bekommen die Lage
            // der folgenden Glyphe
            for k in (0..chars.len()).rev() {
                let ci = chars.start + k;
                if !glyphs[g0..]
                    .iter()
                    .any(|g| ci >= g.char_index && ci < g.char_index + g.char_len)
                {
                    carets[k] = carets[k + 1];
                }
            }
            // Rahmen horizontal: Vorschubbox ∪ Tinte; an indented line of point text keeps the
            // unindented start, so the object keeps its position (re 60 section 3)
            let (mut lo, mut hi) = (x0.min(x0 - indent), x0 + w);
            for g in &glyphs[g0..] {
                if g.is_tab {
                    continue;
                }
                let o = g.font.outline(g.glyph_id);
                let k = g.size / g.font.units_per_em() as f64;
                let tan = if g.faux_italic {
                    self.opts.italic_shear / g.h_scale
                } else {
                    0.0
                };
                let Some((ix0, ix1)) = o.ink_x_sheared(tan) else {
                    continue;
                };
                let (el, er) = if g.faux_bold {
                    (FAUX_BOLD_INK.0 * g.size, FAUX_BOLD_INK.1 * g.size)
                } else {
                    (0.0, 0.0)
                };
                lo = lo.min(g.x + ix0 * k * g.h_scale - el);
                hi = hi.max(g.x + ix1 * k * g.h_scale + er);
            }
            lefts.push(lo);
            rights.push(hi);
            underlines(b, &glyphs[g0..], &mut decorations);
            out_lines.push(LineLayout {
                chars,
                glyphs: g0..glyphs.len(),
                baseline: y - base_shift,
                start: x0,
                width: w,
                align_width: wa,
                ascent: ext.0,
                descent: ext.1,
                carets,
                paragraph_start: para_start,
            });
            last_base = y - base_shift;
        }
        let asc = first_ext.0;
        let (desc, emb) = (last_ext.1, last_ext.2);
        let mut frame_exact = [
            lefts.iter().copied().fold(f64::INFINITY, f64::min),
            y_anchor - asc,
            rights.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            y + desc,
        ];
        let mut frame = Rect::new(
            (frame_exact[0] + 1e-6).floor() as f32,
            (y_anchor - (asc - 1e-9).ceil()) as f32,
            (frame_exact[2] - 1e-6).ceil() as f32,
            (frame_exact[3] - 1e-6).ceil() as f32,
        );
        if fixed {
            // TW-214 (re 63 section 2.3, B): the horizontal frame ends at the descent (`head.yMin`)
            // below the unrounded last baseline, rounded up; one line is `ceil((d + desc)·s)`
            // (Arial 12/15/30/60 pt -> 13/16/32/63, Myriad Pro 60 -> 58, Times 60 -> 61), each
            // further line adds leading·s (Arial 12 pt, three lines: 42). Corpus of stored fixed
            // frames: bottom exact 630/676 (before, with the em box rounded: 305/676)
            let bottom = if vertical { y + emb } else { last_base + desc };
            frame_exact = [b.bounds.left as f64, y_anchor, b.bounds.right as f64, bottom];
            // vertical text keeps the em-box bottom, rounded
            let bottom = if vertical {
                bottom.round()
            } else {
                (bottom - 1e-6).ceil()
            };
            // a frame of width 0 (script `addNewText` with a zero-width rect) shows 10 px wide,
            // without ink (re 63 section 2.2: object 14 px wide incl. the 2 px margins)
            let right = if !vertical && b.bounds.right <= b.bounds.left {
                frame_exact[2] = b.bounds.left as f64 + ZERO_WIDTH_FRAME;
                b.bounds.left + ZERO_WIDTH_FRAME as f32
            } else {
                b.bounds.right
            };
            frame = Rect::new(b.bounds.left, b.bounds.top, right, bottom as f32);
        }
        if too_narrow {
            glyphs.clear();
            decorations.clear();
            for l in &mut out_lines {
                l.glyphs = 0..0;
            }
        }
        let mut lay = TextLayout {
            kind: b.kind,
            orientation: b.orientation,
            frame,
            frame_exact,
            lines: out_lines,
            glyphs,
            decorations,
            antialias: b.antialias,
            hinting: self.opts.hinting,
            missing_fonts: std::mem::take(&mut self.missing),
            char_count: n_chars,
        };
        if on_path {
            self.place_on_path(&mut lay, first_para_end);
        } else if vertical {
            self.make_vertical(&mut lay);
        }
        lay
    }

    /// Text auf Pfad: Glyphen entlang der ersten Pfadkontur platzieren (A, siehe Moduldoku).
    fn place_on_path(&self, lay: &mut TextLayout, first_para_end: usize) {
        let Some(tp) = self.block.path.as_ref() else {
            return;
        };
        let Some(c) = tp.contours.first() else { return };
        let sampler = path::PathSampler::new(c, tp.reversed);
        // Laufweite entlang einer um PATH_TEXT_OFFSET zur Glyphenoberseite versetzten Kurve
        // (Laufzeit phase5 `pt_kreis*`: Radius der Verteilung = Pfadradius + 4,3 px, unabhängig
        // von Radius, Schriftgröße und Grundlinienversatz, innen −4,3; Gerade unverändert).
        let delta = match tp.mode {
            PathTextMode::Rotate | PathTextMode::SkewHorizontal => PATH_TEXT_OFFSET,
            PathTextMode::Vertical | PathTextMode::SkewVertical => PATH_TEXT_OFFSET_UPRIGHT,
        };
        let ocum = sampler.offset_cum(delta);
        let len = ocum.last().copied().unwrap_or(0.0);
        let Some(line) = lay.lines.first().cloned() else {
            return;
        };
        let start = match self.alignment_of_first() {
            Alignment::Center => (len - line.align_width) / 2.0,
            Alignment::Right => len - line.align_width,
            _ => 0.0,
        } + tp.offset;
        let x_line0 = line.start;
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        lay.decorations.clear();
        for g in lay.glyphs.iter_mut() {
            let k = g.size / g.font.units_per_em() as f64;
            let core = g.advance - g.tracking;
            let half = core / 2.0;
            let s_mid = start + (g.x - x_line0) + half;
            g.visible = !g.is_tab && s_mid >= 0.0 && s_mid <= len && g.char_index < first_para_end;
            let Some((p, th)) = sampler.at(sampler.path_s(&ocum, s_mid)) else {
                continue;
            };
            let (sn, cs) = th.sin_cos();
            // Bildachsen der Glyphe: t = Laufrichtung (x), nrm = „unten“ (−y) im Dokument
            let (t, nrm) = match tp.mode {
                PathTextMode::Rotate => ([cs, sn], [-sn, cs]),
                PathTextMode::Vertical => ([1.0, 0.0], [0.0, 1.0]),
                PathTextMode::SkewVertical => ([cs, sn], [0.0, 1.0]),
                PathTextMode::SkewHorizontal => ([1.0, 0.0], [-sn, cs]),
            };
            let tan_it = if g.faux_italic {
                self.opts.italic_shear
            } else {
                0.0
            };
            let bls = g.y - line.baseline; // −BLSf
            g.transform = Affine {
                a: t[0] * k * g.h_scale,
                b: t[1] * k * g.h_scale,
                c: t[0] * k * tan_it - nrm[0] * k,
                d: t[1] * k * tan_it - nrm[1] * k,
                e: p[0] - t[0] * half + nrm[0] * bls,
                f: p[1] - t[1] * half + nrm[1] * bls,
            };
            if !g.visible {
                continue;
            }
            // Rahmen: Box aus Tinte (x) und em-Ober-/Unterlänge (y) im Glyphensystem
            let o = g.font.outline(g.glyph_id);
            let upm = g.font.units_per_em() as f64;
            let (ix0, ix1) = o.ink.map(|b| (b[0], b[1])).unwrap_or((0.0, core / k));
            let (ya, yd) = (g.font.ascent() * upm, -g.font.descent() * upm);
            for q in [[ix0, yd], [ix1, yd], [ix0, ya], [ix1, ya]] {
                let r = g.transform.apply(q);
                lo[0] = lo[0].min(r[0]);
                lo[1] = lo[1].min(r[1]);
                hi[0] = hi[0].max(r[0]);
                hi[1] = hi[1].max(r[1]);
            }
        }
        if lo[0].is_finite() {
            lay.frame_exact = [lo[0], lo[1], hi[0], hi[1]];
            lay.frame = Rect::new(
                (lo[0] + 1e-6).floor() as f32,
                (lo[1] + 1e-6).floor() as f32,
                (hi[0] - 1e-6).ceil() as f32,
                (hi[1] - 1e-6).ceil() as f32,
            );
        }
    }

    fn alignment_of_first(&self) -> Alignment {
        self.attrs(0).alignment
    }

    /// Vertikaler Text (A, Laufzeit T5): jede Zeile wird zu einer Spalte, die Glyphen um 90° im
    /// Uhrzeigersinn gedreht. Spaltenfolge rechts→links (`ORIi 1`) bzw. links→rechts (`ORIi 3`).
    fn make_vertical(&self, lay: &mut TextLayout) {
        let b = self.block;
        let (ax, ay) = (b.bounds.left as f64, b.bounds.top as f64);
        let rtl_cols = b.orientation == crate::model::Orientation::VerticalRtl;
        let base0 = lay.lines.first().map(|l| l.baseline).unwrap_or(ay);
        let mut fr = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        lay.decorations.clear();
        for li in 0..lay.lines.len() {
            let line = lay.lines[li].clone();
            let off = line.baseline - base0;
            let col_x = if rtl_cols { ax - off } else { ax + off };
            let y0 = match self.alignment_of_line(&line, lay) {
                Alignment::Center => ay - line.align_width / 2.0,
                Alignment::Right => ay - line.align_width,
                _ => ay,
            };
            let mut size: f64 = 0.0;
            for gi in line.glyphs.clone() {
                let g = &mut lay.glyphs[gi];
                size = size.max(g.size);
                let k = g.size / g.font.units_per_em() as f64;
                let asc = g.font.ascent();
                let bx = col_x + (1.0 - 2.0 * asc) * g.size / 2.0 + (line.baseline - g.y);
                let gy = y0 + (g.x - line.start);
                let tan = if g.faux_italic {
                    self.opts.italic_shear
                } else {
                    0.0
                };
                g.transform = Affine {
                    a: 0.0,
                    b: k * g.h_scale,
                    c: k,
                    d: k * tan,
                    e: bx,
                    f: gy,
                };
            }
            if size == 0.0 {
                size = self.attrs(0).size;
            }
            fr[0] = fr[0].min(col_x - size / 2.0);
            fr[2] = fr[2].max(col_x + size / 2.0);
            fr[1] = fr[1].min(y0);
            fr[3] = fr[3].max(y0 + line.width);
        }
        if fr[0].is_finite() {
            lay.frame_exact = fr;
            lay.frame = Rect::new(
                (fr[0] + 1e-6).floor() as f32,
                (fr[1] + 1e-6).floor() as f32,
                (fr[2] - 1e-6).ceil() as f32,
                (fr[3] - 1e-6).ceil() as f32,
            );
        }
    }

    fn alignment_of_line(&self, line: &LineLayout, lay: &TextLayout) -> Alignment {
        let run = lay
            .glyphs
            .get(line.glyphs.start)
            .filter(|_| !line.glyphs.is_empty())
            .map(|g| g.run)
            .unwrap_or(0);
        self.attrs(run).alignment
    }

    /// Text in Pfad (A): Zeilen laufen in den waagerechten Spannen der Form von oben nach unten;
    /// Zeilen, deren em-Band nicht mehr in die Form passt, sind unsichtbar.
    fn in_path(self, lines: Vec<RawLine>, n_chars: usize) -> TextLayout {
        let polys = self
            .block
            .path
            .as_ref()
            .map(|p| path::polygons(&p.contours))
            .unwrap_or_default();
        let Some((ytop, ybot)) = path::y_range(&polys) else {
            // keine Form: wie Punkttext am Benutzerrahmen
            return self.position(lines, n_chars);
        };
        // Schätzung für den Umbruch: gleichmäßiger Zeilenabstand der ersten Zeile
        // erste Grundlinie = Oberkante der Form + lateinische Oberlänge (A, `t2_frames`: Rahmen oben/unten
        // exakt); eine Zeile passt, solange ihr Band [Grundlinie − Oberlänge, Grundlinie + Unterlänge]
        // in der Form liegt
        let first = lines.first().map(|l| l.items.as_slice()).unwrap_or(&[]);
        let ra = self.roman_ascent(first, 0);
        let (_, desc, _) = self.line_extents(first, 0);
        let step = first
            .iter()
            .map(|it| self.attrs(it.run).line_advance())
            .fold(f64::NEG_INFINITY, f64::max);
        let step = if step.is_finite() {
            step.max(1.0)
        } else {
            self.attrs(0).line_advance().max(1.0)
        };
        let span_at = |base: f64, asc: f64, desc: f64| -> Option<(f64, f64)> {
            if base + desc > ybot + 1e-6 {
                return None;
            }
            path::band_span(&polys, base - asc, base + desc).map(|(l, r)| (l, r - l))
        };
        let first_base = ytop + ra;
        let wrapped = self.wrap(lines, &|i| span_at(first_base + i as f64 * step, ra, desc));
        // als Punkttext an der ersten Grundlinie setzen, dann jede Zeile in ihre Spanne schieben
        let mut block = self.block.clone();
        block.kind = FrameKind::Point;
        block.bounds = Rect::new(0.0, first_base as f32, 0.0, 0.0);
        let inner = Builder {
            block: &block,
            opts: self.opts,
            run_fonts: self.run_fonts.clone(),
            missing: self.missing.clone(),
            default_attrs: CharAttrs::default(),
        };
        let mut lay = inner.position(wrapped, n_chars);
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for line in lay.lines.iter_mut() {
            let gl = &lay.glyphs[line.glyphs.clone()];
            let ra_line = gl
                .iter()
                .map(|g| g.font.roman_ascent() * g.size)
                .fold(ra, f64::max);
            let span = span_at(line.baseline, ra_line, line.descent.max(0.0));
            let align = lay
                .glyphs
                .get(line.glyphs.start)
                .map(|g| block.runs[g.run].attrs.alignment)
                .unwrap_or_default();
            let (dx, visible) = match span {
                Some((l, w)) => {
                    let x0 = match align {
                        Alignment::Center => l + (w - line.align_width) / 2.0,
                        Alignment::Right => l + w - line.align_width,
                        _ => l,
                    };
                    (x0 - line.start, true)
                }
                None => (0.0, false),
            };
            line.start += dx;
            for c in line.carets.iter_mut() {
                *c += dx;
            }
            for g in &mut lay.glyphs[line.glyphs.clone()] {
                g.x += dx;
                g.transform = g.transform.translated(dx, 0.0);
                g.visible = visible && !g.is_tab;
                if g.visible {
                    if let Some(r) = g.ink_bounds() {
                        lo[0] = lo[0].min(r.left as f64);
                        hi[0] = hi[0].max(r.right as f64);
                    }
                    lo[0] = lo[0].min(g.x);
                    hi[0] = hi[0].max(g.x + g.advance);
                    lo[1] = lo[1].min(g.y - g.font.ascent() * g.size);
                    hi[1] = hi[1].max(g.y + g.font.descent() * g.size);
                }
            }
        }
        lay.decorations.clear();
        for line in &lay.lines {
            let gs: Vec<PositionedGlyph> = lay.glyphs[line.glyphs.clone()]
                .iter()
                .filter(|g| g.visible)
                .cloned()
                .collect();
            underlines(&block, &gs, &mut lay.decorations);
        }
        if lo[0].is_finite() {
            lay.frame_exact = [lo[0], lo[1], hi[0], hi[1]];
            lay.frame = Rect::new(
                (lo[0] + 1e-6).floor() as f32,
                (lo[1] + 1e-6).floor() as f32,
                (hi[0] - 1e-6).ceil() as f32,
                (hi[1] - 1e-6).ceil() as f32,
            );
        }
        lay.kind = FrameKind::InPath;
        lay
    }
}

/// Unterstreichungen einer Zeile (09 §6.5, A, gemessen an `t4_faux.fw.png`): Mitte = Grundlinie −
/// `post.underlinePosition`·k + 0,1·PTSf, Stärke = `post.underlineThickness`·k; von der ersten bis
/// zum Vorschubende der letzten unterstrichenen Glyphe (ohne deren Laufweite).
fn underlines(block: &TextBlock, glyphs: &[PositionedGlyph], out: &mut Vec<Decoration>) {
    let ul = |g: &PositionedGlyph| block.runs.get(g.run).is_some_and(|r| r.attrs.underline);
    let mut i = 0;
    while i < glyphs.len() {
        let g = &glyphs[i];
        if !ul(g) {
            i += 1;
            continue;
        }
        let mut j = i + 1;
        while j < glyphs.len()
            && ul(&glyphs[j])
            && Arc::ptr_eq(&glyphs[j].font, &g.font)
            && glyphs[j].size == g.size
            && glyphs[j].y == g.y
        {
            j += 1;
        }
        let upm = g.font.units_per_em() as f64;
        let k = g.size / upm;
        let (pos, th) = g
            .font
            .underline_metrics()
            .map(|(p, t)| (p as f64, t as f64))
            .unwrap_or((-upm / 10.0, upm / 20.0));
        let c = g.y - pos * k + UNDERLINE_EXTRA_DEPTH * g.size;
        let h = (th * k).max(0.0) / 2.0;
        let last = &glyphs[j - 1];
        let (x0, x1) = (g.x, last.x + last.advance - last.tracking);
        out.push(Decoration {
            quad: [[x0, c - h], [x1, c - h], [x1, c + h], [x0, c + h]],
            color: g.color,
        });
        i = j;
    }
}

/// Blocksatz: `extra` px auf die inneren Leerzeichen verteilen (ohne Leerzeichen am Zeilenende).
fn justify_line(items: &mut [Item], order: &[usize], xs: &mut [f64], extra: f64) {
    if extra <= 0.0 {
        return;
    }
    let mut last = items.len();
    while last > 0 && items[last - 1].ch.is_whitespace() {
        last -= 1;
    }
    let spaces: Vec<usize> = (0..last).filter(|&i| items[i].ch == ' ').collect();
    if spaces.is_empty() {
        return;
    }
    let per = extra / spaces.len() as f64;
    for &i in &spaces {
        items[i].adv += per;
    }
    let mut x = 0.0;
    for &i in order {
        xs[i] = x;
        x += items[i].adv;
    }
}

/// Setzt einen Abschnitt mit rustybuzz. `None`, wenn die Schrift nicht lesbar ist.
fn shape_segment(seg: &[Item], rtl: bool) -> Option<Vec<Item>> {
    let first = seg.first()?;
    let face = rustybuzz::Face::from_slice(first.font.data(), first.font.index())?;
    let text: String = seg.iter().map(|i| i.ch).collect();
    let mut buf = rustybuzz::UnicodeBuffer::new();
    buf.push_str(&text);
    buf.set_direction(if rtl {
        rustybuzz::Direction::RightToLeft
    } else {
        rustybuzz::Direction::LeftToRight
    });
    let kern_tag = rustybuzz::ttf_parser::Tag::from_bytes(b"kern");
    let features = [rustybuzz::Feature::new(kern_tag, first.kern_on as u32, ..)];
    let gb = rustybuzz::shape(&face, &features, buf);
    // Byte-Offset → Index im Abschnitt
    let byte_to_idx: Vec<(usize, usize)> = text
        .char_indices()
        .enumerate()
        .map(|(i, (b, _))| (b, i))
        .collect();
    let idx_of = |cluster: u32| -> usize {
        byte_to_idx
            .iter()
            .rev()
            .find(|(b, _)| *b <= cluster as usize)
            .map(|(_, i)| *i)
            .unwrap_or(0)
    };
    let infos = gb.glyph_infos();
    let poss = gb.glyph_positions();
    let mut clusters: Vec<usize> = infos.iter().map(|g| idx_of(g.cluster)).collect();
    clusters.push(seg.len());
    let mut out = Vec::with_capacity(infos.len());
    for (k, (gi, gp)) in infos.iter().zip(poss).enumerate() {
        let ci = clusters[k];
        // Clusterlänge: Abstand zum nächsten anderen Cluster (in logischer Richtung)
        let mut next = seg.len();
        for &c in clusters.iter().take(infos.len()) {
            if c > ci && c < next {
                next = c;
            }
        }
        let src = &seg[ci.min(seg.len() - 1)];
        let same_cluster_before = clusters[..k].contains(&ci);
        out.push(Item {
            ch: src.ch,
            char_index: src.char_index,
            char_len: if same_cluster_before {
                0
            } else {
                (next - ci).max(1)
            },
            run: src.run,
            font: src.font.clone(),
            gid: gi.glyph_id as u16,
            kern_on: src.kern_on,
            tab: false,
            hb_advance: Some((gp.x_advance, gp.x_offset, gp.y_offset)),
            adv: 0.0,
            trk: 0.0,
        });
    }
    Some(out)
}
