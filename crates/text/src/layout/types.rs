//! Ergebnis-Typen des Satzes.

use std::ops::Range;
use std::sync::Arc;

use crate::support::Rect;
use crate::support::Rgba8;

use crate::font::Font;
use crate::model::{AntiAlias, FrameKind, Orientation};
use crate::raster::Affine;

/// Einstellungen des Satzes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayoutOptions {
    /// Glyphenwahl und Vorschübe.
    pub shaping: Shaping,
    /// Standard-Tabulatorraster in px (Story `/31 = 36`, Laufzeit T1: 36 px = ½ Zoll).
    pub tab_interval: f64,
    /// Mindestabstand bis zum nächsten Tabulatorstopp in px (T1: ≤ 2 px, Korpus: > 0,4 px; A: 1 px).
    pub tab_min_gap: f64,
    /// Scherung von Faux-Kursiv als tan(Winkel) (A: tan 10° = 0,1763; Rahmen der Testdatei T4 17/18).
    pub italic_shear: f64,
    /// Glyphen beim Rastern hinten wie die Schriftmaschine des Originals (FX-T, 09 §6.4): volles
    /// Hinting in der N-fachen Auflösung der Überabtastung, Grundlinie auf ganze Pixel gerundet,
    /// waagerechter Ursprung auf 1/N px. `false` = ungehintete Konturen wie die Python-Referenz.
    pub hinting: bool,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            shaping: Shaping::Original,
            tab_interval: 36.0,
            tab_min_gap: 1.0,
            italic_shear: FAUX_ITALIC_SHEAR,
            hinting: true,
        }
    }
}

/// Faux-Kursiv: tan(10°) (A, angepasst an die Rahmen von `t4_faux.fw.png`, 17/18 exakt).
pub const FAUX_ITALIC_SHEAR: f64 = 0.176_326_980_708_464_97;
/// Faux-Fett: Vorschubzuschlag je Glyphe als Anteil von `PTSf`, vor dem Runden (09 §6.5, B).
pub const FAUX_BOLD_ADVANCE: f64 = 0.027;
/// Faux-Fett: Tintenverbreiterung links/rechts für den Rahmen (Anteil von `PTSf`, 09 §6.5).
pub const FAUX_BOLD_INK: (f64, f64) = (1.0 / 72.0, 1.0 / 36.0);
/// Faux-Fett: Konturverbreiterung je Seite beim Rastern (Anteil von `PTSf`, A: Strich ≈ `PTSf/36`).
pub const FAUX_BOLD_STROKE: f64 = 1.0 / 72.0;
/// Unterstreichung: zusätzliche Tiefe unter `post.underlinePosition` als Anteil von `PTSf`
/// (A, gemessen an den Caches von `t4_faux.fw.png`: Arial/Myriad 10–100 px).
pub const UNDERLINE_EXTRA_DEPTH: f64 = 0.1;

/// Glyphenwahl.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shaping {
    /// Original-Breitenmodell (09 §5.6): eine Glyphe je Zeichen über `cmap`, Paar-Kerning der Schrift,
    /// keine Ligaturen. Trifft die gespeicherten Rahmen am besten.
    #[default]
    Original,
    /// rustybuzz (HarfBuzz): Ligaturen, kontextabhängige Formen, komplexe Schriften und RTL. Die
    /// Vorschübe werden danach wie im Original-Modell je Glyphe gerundet.
    Harfbuzz,
}

/// Eine gesetzte Glyphe.
#[derive(Debug, Clone)]
pub struct PositionedGlyph {
    /// Schrift.
    pub font: Arc<Font>,
    /// Glyphen-ID.
    pub glyph_id: u16,
    /// Erstes Zeichen (Index in den Unicode-Skalaren des Gesamttexts).
    pub char_index: usize,
    /// Anzahl der Zeichen dieser Glyphe (> 1 bei Ligaturen).
    pub char_len: usize,
    /// Zeichen (erstes Zeichen des Clusters).
    pub ch: char,
    /// Index des Laufs.
    pub run: usize,
    /// Zeile.
    pub line: usize,
    /// Ursprung x (logisch, vor einer Pfad-/Vertikal-Abbildung).
    pub x: f64,
    /// Grundlinie y inklusive Grundlinienversatz (logisch).
    pub y: f64,
    /// Schriftgröße in px.
    pub size: f64,
    /// Vorschub in px inklusive Laufweite.
    pub advance: f64,
    /// Laufweite hinter der Glyphe in px.
    pub tracking: f64,
    /// Horizontale Skalierung.
    pub h_scale: f64,
    /// Faux-Fett.
    pub faux_bold: bool,
    /// Faux-Kursiv.
    pub faux_italic: bool,
    /// Textfarbe.
    pub color: Rgba8,
    /// Abbildung Font-Einheiten (y oben) → Dokumentpixel.
    pub transform: Affine,
    /// Sichtbar (Text auf Pfad: Glyphenmitte auf dem Pfad; Text in Pfad: Zeile in der Form).
    pub visible: bool,
    /// Tabulator (ohne Tinte).
    pub is_tab: bool,
}

impl PositionedGlyph {
    /// Tinten-Box in Dokumentpixeln (Hülle der abgebildeten Kontur), `None` bei leerer Glyphe.
    pub fn ink_bounds(&self) -> Option<Rect> {
        let o = self.font.outline(self.glyph_id);
        let b = o.ink?;
        let m = &self.transform;
        let pts = [[b[0], b[2]], [b[1], b[2]], [b[0], b[3]], [b[1], b[3]]];
        let mut r: Option<[f64; 4]> = None;
        for p in pts {
            let q = m.apply(p);
            r = Some(match r {
                None => [q[0], q[1], q[0], q[1]],
                Some(r) => [r[0].min(q[0]), r[1].min(q[1]), r[2].max(q[0]), r[3].max(q[1])],
            });
        }
        let e = if self.faux_bold {
            self.size * FAUX_BOLD_STROKE
        } else {
            0.0
        };
        r.map(|r| {
            Rect::new(
                (r[0] - e) as f32,
                (r[1] - e) as f32,
                (r[2] + e) as f32,
                (r[3] + e) as f32,
            )
        })
    }
}

/// Eine Zeile.
#[derive(Debug, Clone, PartialEq)]
pub struct LineLayout {
    /// Zeichenbereich (inklusive abschließendem Umbruchzeichen).
    pub chars: Range<usize>,
    /// Glyphenbereich in [`TextLayout::glyphs`].
    pub glyphs: Range<usize>,
    /// Grundlinie (horizontal) bzw. x der Spalte (vertikal).
    pub baseline: f64,
    /// Zeilenbeginn (horizontal x, vertikal y).
    pub start: f64,
    /// Zeilenbreite = Summe der Vorschübe.
    pub width: f64,
    /// Breite für die Ausrichtung (ohne Laufweite hinter der letzten Glyphe).
    pub align_width: f64,
    /// Größte Oberlänge der Zeile in px (über der Grundlinie, inkl. Grundlinienversatz).
    pub ascent: f64,
    /// Größte Unterlänge der Zeile in px (unter der Grundlinie).
    pub descent: f64,
    /// Caret-Positionen: für jedes Zeichen der Zeile die Lage vor dem Zeichen, dazu das Zeilenende
    /// (Länge `chars.len() + 1`).
    pub carets: Vec<f64>,
    /// Beginnt einen Absatz.
    pub paragraph_start: bool,
}

/// Unterstreichung: Viereck in Dokumentpixeln mit Farbe.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decoration {
    pub quad: [[f64; 2]; 4],
    pub color: Rgba8,
}

/// Ergebnis des Satzes.
#[derive(Debug, Clone)]
pub struct TextLayout {
    /// Objektart.
    pub kind: FrameKind,
    /// Orientierung.
    pub orientation: Orientation,
    /// Layout-Rahmen `LFTf TOPf RITf BOTf` (gerundet wie das Original).
    pub frame: Rect,
    /// Ungerundete Rahmengrenzen (links, oben, rechts, unten).
    pub frame_exact: [f64; 4],
    /// Zeilen.
    pub lines: Vec<LineLayout>,
    /// Glyphen in logischer Reihenfolge.
    pub glyphs: Vec<PositionedGlyph>,
    /// Unterstreichungen.
    pub decorations: Vec<Decoration>,
    /// Kantenglättung (aus dem Textobjekt).
    pub antialias: AntiAlias,
    /// Hinting beim Rastern ([`LayoutOptions::hinting`]).
    pub hinting: bool,
    /// Nicht gefundene Schriftnamen (es wurde eine Ersatzschrift benutzt).
    pub missing_fonts: Vec<String>,
    /// Anzahl der Zeichen des Texts.
    pub char_count: usize,
}

impl TextLayout {
    /// Sichtbare Glyphen.
    pub fn visible_glyphs(&self) -> impl Iterator<Item = &PositionedGlyph> {
        self.glyphs.iter().filter(|g| g.visible)
    }

    /// Zeile, die das Zeichen `char_index` enthält (Zeichen nach dem Ende → letzte Zeile).
    pub fn line_of_char(&self, char_index: usize) -> Option<usize> {
        if self.lines.is_empty() {
            return None;
        }
        Some(
            self.lines
                .iter()
                .position(|l| char_index < l.chars.end)
                .unwrap_or(self.lines.len() - 1),
        )
    }
}
