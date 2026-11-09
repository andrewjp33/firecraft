//! Text für Firepixel: Schriftverwaltung, Satz und Rasterung nach dem Original.
//!
//! Grundlage: Spezifikation `re\09-text.md` (inkl. Laufzeitergebnisse der Testrunde 11.10.2026) mit
//! den Python-Referenzen `re\text\layout.py` (Satz/Rahmen), `glaettung.py` (Tonkurven) und
//! `raster.py` (Punktabtastung). Unabhängig von `fp-model`: fp-render bzw. der Text-Editor füllen
//! die Eingabe-Strukturen aus [`model`] aus dem Dokument.
//!
//! # Überblick
//!
//! | Modul | Inhalt |
//! |---|---|
//! | [`model`] | Eingabe: [`TextBlock`] (Rahmenart `TFTi`, Benutzerrahmen, Läufe, Auto-Kerning, Story-Kerning-Art, Orientierung, Glättung, Text auf/in Pfad), [`CharAttrs`] je Lauf, Enums mit den Tag-Werten, [`resolve_runs`] für die Vererbung in `TFSv` |
//! | [`font`] | [`FontDb`]: Systemschriften zur Laufzeit (keine eingebetteten Schriften), Auflösung nach PostScript-/Familien-/Stilname, Rückfall; [`Font`]: Metriken (asc aus `BASE`/Versalhöhe, desc aus `head.yMin`), GPOS-/`kern`-Paarkerning, Konturen |
//! | [`mod@layout`] | [`layout()`]: Breitenmodell, Zeilen, Ausrichtung/Blocksatz, Tabulatoren, Rahmen fester Breite, Text auf/in Pfad, vertikal/RTL; Ergebnis [`TextLayout`] mit Zeilen, Glyphen, Rahmen und Cursor-/Auswahl-Abfragen |
//! | [`render`] | Deckungsmaske ([`render::coverage`]), Bild mit Laufarben ([`render::render`]), Glyphenkonturen als `crate::support::BezierContour` ([`render::glyph_contours`]) |
//! | [`antialias`] | Tonkurven glatt/scharf/stark/benutzerdefiniert (abgeschnittene Bézier-Kurve) |
//! | [`raster`] | Punktabtastung N×N (Non-Zero), Faux-Fett-Verbreiterung |
//!
//! # Beispiel
//!
//! ```no_run
//! use firecraft_text::{layout, render, CharAttrs, FontDb, LayoutOptions, TextBlock, TextRun};
//!
//! let fonts = FontDb::system();
//! let attrs = CharAttrs { font: "ArialMT".into(), size: 20.0, ..CharAttrs::default() };
//! let block = TextBlock::point(10.0, 40.0, vec![TextRun::new("Hallo", attrs)]);
//! let lay = layout(&block, &fonts, &LayoutOptions::default());
//! println!("Rahmen {:?}", lay.frame);           // LFTf/TOPf/RITf/BOTf
//! let img = render::render(&lay, None);          // RGBA in Dokumentkoordinaten
//! let hit = lay.hit_test(25.0, 35.0);            // Caret-Index für den Editor
//! # let _ = (img, hit);
//! ```
//!
//! # Konventionen
//!
//! - Dokumentpixel, y nach unten; Schriftgrößen in px (`PTSf`, bei 72 ppi = pt).
//! - Zeichenindizes zählen Unicode-Skalare im Gesamttext (Steuerzeichen `\r`, `\u{3}`, `\t`
//!   eingeschlossen).
//! - Kein `unsafe`, keine Panik auf Eingaben: fehlende oder defekte Schriften → Rückfall-Schrift,
//!   ohne jede Schrift ein leeres Layout.
//! - Hinting (FX-T, 09 §6.4): Glyphen werden wie in der Schriftmaschine des Originals in der
//!   N-fachen Auflösung der Überabtastung gehintet (TrueType-Interpreter bzw. PostScript-Hinter
//!   über `skrifa`, volles Hinting „Mono“), die Grundlinie liegt auf ganzen Pixeln, der waagerechte
//!   Ursprung auf 1/N px. Abschaltbar über [`LayoutOptions::hinting`] (Python-Referenz: ungehintet).

pub mod antialias;
pub mod font;
pub mod layout;
pub mod model;
pub mod outline_hint;
pub mod raster;
pub mod render;
pub mod support;

pub use font::{FaceInfo, Font, FontDb, ResolvedFont};
pub use layout::{
    Caret, Decoration, HitResult, LayoutOptions, LineLayout, PositionedGlyph, Shaping, TextLayout, layout,
    layout_simple,
};
pub use model::{
    Alignment, AntiAlias, AntiAliasMode, CharAttrs, CharAttrsPatch, FrameKind, KerningKind, LeadingMode,
    Orientation, PathTextMode, StoryKernRun, TextBlock, TextPath, TextRun, resolve_runs,
};

/// Fehler von fp-text (nur beim expliziten Laden von Schriften; der Satz selbst fällt zurück).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TextError {
    /// Datei nicht lesbar.
    #[error("Could not read the font {0}: {1}")]
    Io(String, String),
    /// Keine gültige OpenType-/TrueType-Schrift.
    #[error("Not a usable font: {0}")]
    BadFont(String),
}
