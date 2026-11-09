//! Eingabe-Strukturen für Satz und Rasterung (unabhängig von `fp-model`).
//!
//! fp-render bzw. der Text-Editor füllen diese Strukturen aus dem Dokumentmodell (`TXTv`,
//! Spezifikation 09 §3). Die Werte entsprechen den Datei-Tags; die Tag-Namen stehen jeweils in der
//! Doku. Alle Längen in Dokumentpixeln (bei 72 ppi = pt), Positionen in Dokumentkoordinaten (y nach
//! unten).
//!
//! Läufe (`TFSv`) erben fehlende Attribute vom vorigen Attributblock; dafür gibt es
//! [`CharAttrsPatch`] und [`resolve_runs`].

use crate::support::Rgba8;
use crate::support::{BezierContour, Rect};

/// Objektart `TFTi` (09 §3.1, Laufzeit T2 bestätigt).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FrameKind {
    /// 0: Punkttext (automatische Größe, kein Umbruch; JS `autoExpand = true`).
    #[default]
    Point,
    /// 1: Rahmen fester Breite mit Umbruch (JS `autoExpand = false`).
    FixedWidth,
    /// 2: Text auf Pfad (`TAPv`).
    OnPath,
    /// 3: Text in Pfad (Fließtext in einer Form, `placeTextInPath`).
    InPath,
}

impl FrameKind {
    /// Aus dem Tag-Wert `TFTi`; unbekannte Werte → Punkttext.
    pub fn from_tfti(v: i64) -> Self {
        match v {
            1 => FrameKind::FixedWidth,
            2 => FrameKind::OnPath,
            3 => FrameKind::InPath,
            _ => FrameKind::Point,
        }
    }

    /// Tag-Wert `TFTi`.
    pub fn to_tfti(self) -> i64 {
        self as i64
    }
}

/// Absatzausrichtung `JSTi` (09 §3.5). Die vertikalen Werte 0x10–0x14 bilden auf dieselben Fälle ab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Alignment {
    /// 0 `left`.
    #[default]
    Left,
    /// 1 `center`.
    Center,
    /// 2 `right`.
    Right,
    /// 3 `justify`: Blocksatz, letzte Zeile des Absatzes linksbündig.
    Justify,
    /// 4 `stretch`: erzwungener Blocksatz (auch die letzte Zeile).
    Stretch,
}

impl Alignment {
    /// From `JSTi` (0-4, vertical 0x10-0x14). "Stretched" (4) loads as left like in the
    /// reference text engine, which never stores it (TW-133, re 50 section 1.2); unknown → left.
    pub fn from_jsti(v: i64) -> Self {
        match v & 0x0f {
            1 => Alignment::Center,
            2 => Alignment::Right,
            3 => Alignment::Justify,
            _ => Alignment::Left,
        }
    }
}

/// Zeilenabstandsmodus `LDMi` (09 §3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LeadingMode {
    /// 0: `LEDf` ist ein Faktor der Schriftgröße.
    #[default]
    Percentage,
    /// 1: `LEDf` in px.
    Exact,
    /// 2: `additional` (nur interne Tabelle, nicht im Korpus; A: Schriftgröße + `LEDf` px).
    Additional,
}

impl LeadingMode {
    /// Aus `LDMi`.
    pub fn from_ldmi(v: i64) -> Self {
        match v {
            1 => LeadingMode::Exact,
            2 => LeadingMode::Additional,
            _ => LeadingMode::Percentage,
        }
    }
}

/// Orientierung `ORIi` (09 §3.5, Laufzeit T5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Orientation {
    /// 0: horizontal links → rechts.
    #[default]
    HorizontalLtr,
    /// 1: vertikal, Zeilen rechts → links.
    VerticalRtl,
    /// 2: horizontal rechts → links.
    HorizontalRtl,
    /// 3: vertikal, Zeilen links → rechts.
    VerticalLtr,
}

impl Orientation {
    /// Aus `ORIi`.
    pub fn from_orii(v: i64) -> Self {
        match v {
            1 => Orientation::VerticalRtl,
            2 => Orientation::HorizontalRtl,
            3 => Orientation::VerticalLtr,
            _ => Orientation::HorizontalLtr,
        }
    }

    /// Vertikal (`ORIi` 1 oder 3, wie `@008a1a20`)?
    pub fn is_vertical(self) -> bool {
        matches!(self, Orientation::VerticalRtl | Orientation::VerticalLtr)
    }
}

/// Kantenglättungsmodus `TAAi` (09 §3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AntiAliasMode {
    /// 0 „Glatt“: 4×4, Tabelle `@01b4c088`.
    #[default]
    Smooth,
    /// 1 „Scharf“: 4×4, Tabelle `@01b4c09c`.
    Crisp,
    /// 2 „Stark“: eigener Pfad im Original (hier: 4×4 und empirische Tabelle, siehe [`crate::antialias`]).
    Strong,
    /// 3 „System“: wird beim Laden und im Renderer zu „Scharf“.
    System,
    /// 4 „Benutzerdefiniert“: N = `TOSi`, kubische Bézier-Kurve aus `TA2i`/`TA1i`.
    Custom,
}

impl AntiAliasMode {
    /// Aus `TAAi`; unbekannt → glatt.
    pub fn from_taai(v: i64) -> Self {
        match v {
            1 => AntiAliasMode::Crisp,
            2 => AntiAliasMode::Strong,
            3 => AntiAliasMode::System,
            4 => AntiAliasMode::Custom,
            _ => AntiAliasMode::Smooth,
        }
    }

    /// Wirksamer Modus: „System“ (3) wird wie im Original zu „Scharf“ (B, `@0095ff90`).
    pub fn effective(self) -> Self {
        if self == AntiAliasMode::System {
            AntiAliasMode::Crisp
        } else {
            self
        }
    }
}

/// Kantenglättung des Textobjekts (`TAAi TA1i TA2i TOSi` und `PATv/FPLv/FETi`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AntiAlias {
    /// `TAAi`.
    pub mode: AntiAliasMode,
    /// `TA1i` Schärfe = Bézier-Stützwert P2 (Vorgabe 192).
    pub sharpness: u8,
    /// `TA2i` Stärke = Bézier-Stützwert P1 (Vorgabe 64).
    pub strength: u8,
    /// `TOSi` Überabtastung 4/8/16 (nur bei „benutzerdefiniert“ wirksam, Vorgabe 8).
    pub oversample: u8,
    /// Füllkante geglättet (`FETi = 1`). `false` = „Keine Kantenglättung“: 1 Abtastpunkt je Pixel.
    pub enabled: bool,
}

impl Default for AntiAlias {
    fn default() -> Self {
        Self {
            mode: AntiAliasMode::Smooth,
            sharpness: 192,
            strength: 64,
            oversample: 8,
            enabled: true,
        }
    }
}

impl AntiAlias {
    /// Aus den Tag-Werten; `TA1i`/`TA2i` werden wie im Original auf 255 begrenzt (`@008bed00`).
    pub fn from_tags(taai: i64, ta1i: i64, ta2i: i64, tosi: i64, feti: Option<i64>) -> Self {
        Self {
            mode: AntiAliasMode::from_taai(taai),
            sharpness: ta1i.clamp(0, 255) as u8,
            strength: ta2i.clamp(0, 255) as u8,
            oversample: tosi.clamp(0, 255) as u8,
            enabled: feti != Some(0),
        }
    }
}

/// Kerning-Art eines Stil-Laufs der ATE-Story (`/11`, 09 §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KerningKind {
    /// 0: kein Kerning.
    Off,
    /// 1: Metrik (Paar-Kerning der Schrift).
    Metric,
    /// 2: optisch (ATE-intern, nicht nachgebaut → wie Metrik).
    Optical,
}

impl KerningKind {
    /// Aus dem Story-Wert `/11`.
    pub fn from_value(v: i64) -> Self {
        match v {
            0 => KerningKind::Off,
            2 => KerningKind::Optical,
            _ => KerningKind::Metric,
        }
    }

    /// Paar-Kerning anwenden?
    pub fn is_on(self) -> bool {
        self != KerningKind::Off
    }
}

/// Stil-Lauf der ATE-Story für die Kerning-Art: `len` Zeichen mit Art `kind` (`None` = nicht gesetzt,
/// dann gilt `ATKb`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoryKernRun {
    pub len: usize,
    pub kind: Option<KerningKind>,
}

/// Zeichen- und Absatzattribute eines Laufs (`TFSv`-Attributblock, vollständig aufgelöst).
#[derive(Debug, Clone, PartialEq)]
pub struct CharAttrs {
    /// `FONs` im Lauf: PostScript-Name der Schrift („MyriadPro-Regular“).
    pub font: String,
    /// `PTSf`: Schriftgröße in px.
    pub size: f64,
    /// `FCLi`: Textfarbe.
    pub color: Rgba8,
    /// `BOLb`: Faux-Fett.
    pub bold: bool,
    /// `ITLb`: Faux-Kursiv.
    pub italic: bool,
    /// `UNDb`: unterstrichen.
    pub underline: bool,
    /// `HSCf`: horizontale Skalierung (1 = 100 %).
    pub h_scale: f64,
    /// `KRNf`: manuelles Kerning (nur durchgereicht, 09 §3.4).
    pub kerning: f64,
    /// `RKNf`: Laufweite; Abstand hinter jeder Glyphe = `RKNf · PTSf / 10` px.
    pub tracking: f64,
    /// `BLSf`: Grundlinienversatz in px, positiv hebt an.
    pub baseline_shift: f64,
    /// `LEDf`: Zeilenabstand (Faktor bzw. px je [`LeadingMode`]).
    pub leading: f64,
    /// `LDMi`.
    pub leading_mode: LeadingMode,
    /// `JSTi` (Absatzattribut).
    pub alignment: Alignment,
    /// `PINf`: Erstzeileneinzug in px (Absatzattribut).
    pub indent: f64,
    /// `PSBf`: Abstand vor dem Absatz in px.
    pub space_before: f64,
    /// `PSAf`: Abstand nach dem Absatz in px.
    pub space_after: f64,
}

impl Default for CharAttrs {
    fn default() -> Self {
        Self {
            font: "MyriadPro-Regular".into(),
            size: 12.0,
            color: Rgba8::new(0, 0, 0, 255),
            bold: false,
            italic: false,
            underline: false,
            h_scale: 1.0,
            kerning: 0.0,
            tracking: 0.0,
            baseline_shift: 0.0,
            leading: 1.0,
            leading_mode: LeadingMode::Percentage,
            alignment: Alignment::Left,
            indent: 0.0,
            space_before: 0.0,
            space_after: 0.0,
        }
    }
}

impl CharAttrs {
    /// Zeilenabstand in px für diese Attribute (09 §5.1).
    pub fn line_advance(&self) -> f64 {
        match self.leading_mode {
            LeadingMode::Percentage => self.leading * self.size,
            LeadingMode::Exact => self.leading,
            LeadingMode::Additional => self.size + self.leading,
        }
    }

    /// Laufweite hinter jeder Glyphe in px (`RKNf · PTSf / 10`).
    pub fn tracking_px(&self) -> f64 {
        self.tracking * self.size / 10.0
    }
}

/// Ein Attributblock aus `TFSv` mit nur den gesetzten Tags; fehlende erben (09 §3.3).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CharAttrsPatch {
    pub font: Option<String>,
    pub size: Option<f64>,
    pub color: Option<Rgba8>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub h_scale: Option<f64>,
    pub kerning: Option<f64>,
    pub tracking: Option<f64>,
    pub baseline_shift: Option<f64>,
    pub leading: Option<f64>,
    pub leading_mode: Option<LeadingMode>,
    pub alignment: Option<Alignment>,
    pub indent: Option<f64>,
    pub space_before: Option<f64>,
    pub space_after: Option<f64>,
}

impl CharAttrsPatch {
    /// Gesetzte Werte auf `a` anwenden.
    pub fn apply(&self, a: &mut CharAttrs) {
        if let Some(v) = &self.font {
            a.font.clone_from(v);
        }
        macro_rules! set {
            ($($f:ident),*) => { $( if let Some(v) = self.$f { a.$f = v; } )* };
        }
        set!(
            size,
            color,
            bold,
            italic,
            underline,
            h_scale,
            kerning,
            tracking,
            baseline_shift,
            leading,
            leading_mode,
            alignment,
            indent,
            space_before,
            space_after
        );
    }
}

/// Ein Lauf: Text mit einheitlichen Attributen. Steuerzeichen bleiben im Text: `\r` Absatzende,
/// `\u{3}` Zeilenumbruch im Absatz, `\n` Zeilenumbruch, `\t` Tabulator.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub attrs: CharAttrs,
}

impl TextRun {
    pub fn new(text: impl Into<String>, attrs: CharAttrs) -> Self {
        Self {
            text: text.into(),
            attrs,
        }
    }
}

/// Löst `TFSv`-Attributblöcke zu vollständigen Läufen auf: Start mit `initial` (Kopf-Attribute),
/// jeder Block überschreibt nur seine gesetzten Werte.
pub fn resolve_runs(initial: &CharAttrs, blocks: &[(CharAttrsPatch, String)]) -> Vec<TextRun> {
    let mut cur = initial.clone();
    blocks
        .iter()
        .map(|(p, t)| {
            p.apply(&mut cur);
            TextRun {
                text: t.clone(),
                attrs: cur.clone(),
            }
        })
        .collect()
}

/// Text-auf-Pfad-Modus `TAMi` (09 §3.5, Laufzeit T3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PathTextMode {
    /// 0 `rotate`: Glyphen entlang der Tangente gedreht.
    #[default]
    Rotate,
    /// 1 `vertical`: Glyphen aufrecht.
    Vertical,
    /// 2 `skew vertical`: Senkrechte bleiben senkrecht, Grundlinie folgt der Tangente.
    SkewVertical,
    /// 3 `skew horizontal`: Waagerechte bleiben waagerecht, Senkrechte stehen quer zur Tangente.
    SkewHorizontal,
}

impl PathTextMode {
    /// Aus `TAMi`.
    pub fn from_tami(v: i64) -> Self {
        match v {
            1 => PathTextMode::Vertical,
            2 => PathTextMode::SkewVertical,
            3 => PathTextMode::SkewHorizontal,
            _ => PathTextMode::Rotate,
        }
    }
}

/// Pfad für Text auf Pfad (`TFTi 2`) bzw. in Pfad (`TFTi 3`): `TAPv { TAMi CHOf PTHv }`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextPath {
    /// `TAMi`.
    pub mode: PathTextMode,
    /// `CHOf`: Versatz entlang des Pfads in px.
    pub offset: f64,
    /// Konturen von `TAPv/PTHv` in Dokumentkoordinaten. Text auf Pfad nutzt die erste Kontur.
    pub contours: Vec<BezierContour>,
    /// Richtung umkehren (`reversePathTextDirection`, sonst schon in den Daten umgedreht).
    pub reversed: bool,
}

/// Ein Textobjekt als Eingabe für [`crate::layout()`].
#[derive(Debug, Clone, PartialEq)]
pub struct TextBlock {
    /// `TFTi`.
    pub kind: FrameKind,
    /// Benutzerrahmen `LFAf TOAf RIAf BOAf`. Punkttext: (`left`, `top`) = Anker (Ausrichtungspunkt
    /// auf der ersten Grundlinie), `right`/`bottom` unbenutzt. Rahmentext: Rahmen (Umbruchbreite =
    /// `right − left`). Text auf Pfad: unbenutzt (ursprünglicher Anker). Text in Pfad: unbenutzt.
    pub bounds: Rect,
    /// Läufe in Textreihenfolge.
    pub runs: Vec<TextRun>,
    /// `ATKb`: Auto-Kerning (gilt, wenn keine Story-Kerning-Art vorliegt).
    pub auto_kern: bool,
    /// Kerning-Art je Stil-Lauf der ATE-Story (`aTXT`, `/11`); hat Vorrang vor `auto_kern`.
    pub story_kerning: Option<Vec<StoryKernRun>>,
    /// `ORIi`.
    pub orientation: Orientation,
    /// Kantenglättung.
    pub antialias: AntiAlias,
    /// `TAPv` (nur bei `OnPath`/`InPath`).
    pub path: Option<TextPath>,
}

impl Default for TextBlock {
    fn default() -> Self {
        Self {
            kind: FrameKind::Point,
            bounds: Rect::default(),
            runs: Vec::new(),
            auto_kern: true,
            story_kerning: None,
            orientation: Orientation::HorizontalLtr,
            antialias: AntiAlias::default(),
            path: None,
        }
    }
}

impl TextBlock {
    /// Punkttext mit Anker (x, erste Grundlinie y).
    pub fn point(x: f32, y: f32, runs: Vec<TextRun>) -> Self {
        Self {
            kind: FrameKind::Point,
            bounds: Rect::new(x, y, 0.0, 0.0),
            runs,
            ..Self::default()
        }
    }

    /// Rahmentext fester Breite.
    pub fn fixed(bounds: Rect, runs: Vec<TextRun>) -> Self {
        Self {
            kind: FrameKind::FixedWidth,
            bounds,
            runs,
            ..Self::default()
        }
    }

    /// Gesamter Text (Verkettung aller Läufe).
    pub fn text(&self) -> String {
        self.runs.iter().map(|r| r.text.as_str()).collect()
    }

    /// Anzahl der Zeichen (Unicode-Skalare) des gesamten Texts.
    pub fn char_count(&self) -> usize {
        self.runs.iter().map(|r| r.text.chars().count()).sum()
    }
}
