//! Eingabe-Strukturen: ein Pfad mit seinen Attributen, so wie fp-render sie aus dem Dokumentmodell
//! befüllt (Spezifikation 07 §5.1, §6.1; Dateiformat 02 §5.2). fp-vector hängt bewusst **nicht** von
//! fp-model ab; die Felder tragen deshalb in der Doku den Namen des Datei-Tags.
//!
//! Zuordnung Datei → Struktur (Vorgaben in Klammern, wie der Original-Leser bzw. die Referenz):
//!
//! | Tag | Feld |
//! |---|---|
//! | `PCLv/PPLv/PPTv` (`XLCf`, `YLCf`, `ISCb`, `PRSf`, `VELf`, `RNDi`) | [`VectorPath::contours`] |
//! | `EOFb` (1), `AGMb` (0) | [`VectorPath::even_odd`], [`VectorPath::agm`] |
//! | `FOTb` (0), `BRPi` (1), `PTTi` (0) | [`VectorPath::fill_over_stroke`], [`VectorPath::placement`], [`VectorPath::ptt`] |
//! | `TOXf`/`TOYf` (0) | [`VectorPath::texture_offset`] |
//! | `PATv/FPLv`, `FCLi`, `PSXf…PFYf`, `TXFv` | [`Fill`] |
//! | `PATv/BPLv`, `BCLi` | [`Stroke`], [`BrushParams`] |
//!
//! Alle Koordinaten sind Dokumentpixel in `f64` (y nach unten). Die Referenz rechnet mit den als
//! `double` gelesenen Dezimalwerten der Datei; wer bitgleich zur Referenz rechnen will, übergibt
//! diese Werte, wer bitgleich zum Original rechnen will, die `f32`-Werte des Modells.

use std::sync::Arc;

use crate::flatten::FlattenOptions;
use crate::support::color::Argb;
use crate::support::geom::{BezierContour, Point, Vec2};
use crate::support::raster::{Mask, RgbaImage};

/// Eine abgeflachte Kontur (Polylinie) mit optionalen Stiftdaten je Punkt.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PathContour {
    /// Punkte in Dokumentkoordinaten (`PPTv` `XLCf`/`YLCf`).
    pub points: Vec<Vec2>,
    /// Geschlossen-Flag (`ISCb`). Für die Füllung werden offene Konturen implizit geschlossen.
    pub closed: bool,
    /// Druck/Geschwindigkeit/Startwert je Punkt (`PRSf`, `VELf`, `RNDi`); leer = Vorgaben
    /// (Druck 1, Geschwindigkeit 0, Startwert 0). Nur natürliche Pinsel lesen sie.
    pub dynamics: Vec<PointDynamics>,
}

impl PathContour {
    /// Kontur aus Punkten ohne Stiftdaten.
    pub fn new(points: Vec<Vec2>, closed: bool) -> Self {
        Self {
            points,
            closed,
            dynamics: Vec::new(),
        }
    }

    /// Kontur aus `f32`-Punkten (z. B. `crate::support::geom::Polyline`).
    pub fn from_points(points: &[Point], closed: bool) -> Self {
        Self::new(points.iter().map(|p| Vec2::from(*p)).collect(), closed)
    }

    /// Bézier-Kontur mit der Original-Abflachung (= `PCLv`, 16 §1.4) in eine Polylinie wandeln.
    /// `counts` = `BZCi` je Knoten (Druck-Stützstellen; `None` = überall 1).
    pub fn from_bezier(c: &BezierContour, counts: Option<&[u32]>) -> Self {
        let pl = crate::flatten::flatten_contour(c, counts, &FlattenOptions::ORIGINAL);
        Self::from_points(&pl.points, c.closed)
    }

    /// Stiftdaten des Punkts `i` (Vorgaben, wenn keine vorhanden).
    pub fn dynamics_at(&self, i: usize) -> PointDynamics {
        self.dynamics.get(i).copied().unwrap_or_default()
    }
}

/// Stiftdaten eines Polylinienpunkts (`PPTv`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointDynamics {
    /// Druck 0..1 (`PRSf`, Vorgabe 1).
    pub pressure: f32,
    /// Geschwindigkeit (`VELf` in px/ms, Vorgabe 0,125 = `spd` 32 für Punkte ohne eigene Dynamik,
    /// re\34 §2.2).
    pub velocity: f32,
    /// Zufalls-Startwert (`RNDi`, Vorgabe 0).
    pub seed: u32,
}

impl Default for PointDynamics {
    fn default() -> Self {
        Self {
            pressure: 1.0,
            velocity: 0.125,
            seed: 0,
        }
    }
}

/// Wirksame bzw. gewünschte Strichposition (`BRPi`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u8)]
pub enum StrokePlacement {
    /// 0 = innen.
    Inside = 0,
    /// 1 = mittig (Vorgabe).
    #[default]
    Center = 1,
    /// 2 = außen.
    Outside = 2,
}

impl StrokePlacement {
    /// Aus dem Dateiwert; unbekannte Werte → mittig (wie die Referenz).
    pub fn from_u32(v: u32) -> Self {
        match v {
            0 => Self::Inside,
            2 => Self::Outside,
            _ => Self::Center,
        }
    }
}

/// Ein Pfad mit allen für das Rastern nötigen Attributen.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorPath {
    /// Abgeflachte Konturen in Dokumentkoordinaten.
    pub contours: Vec<PathContour>,
    /// Gerade/ungerade-Regel (`EOFb`, Vorgabe 1). 0 = Windungsregel (Verfahren B).
    pub even_odd: bool,
    /// AGM-Rasterung (`AGMb`); wirkt nur zusammen mit `even_odd` (Verfahren G).
    pub agm: bool,
    /// Füllung (`PATv/FPLv`), `None` = keine.
    pub fill: Option<Fill>,
    /// Strich (`PATv/BPLv`), `None` = keiner.
    pub stroke: Option<Stroke>,
    /// Füllung über dem Strich (`FOTb`).
    pub fill_over_stroke: bool,
    /// Strichposition (`BRPi`); die wirksame Position bestimmt [`crate::stroke::effective_placement`].
    pub placement: StrokePlacement,
    /// Pfad-Flag `CDbPath+0xf8` (= Tag `PTTi = 1`, 07 §6.6): geschlossener, mittiger 1-px-Strich wird innen gezeichnet.
    pub ptt: bool,
    /// Texturursprung (`TOXf`, `TOYf`).
    pub texture_offset: Vec2,
    /// Zufalls-Startwert des Pfads (`PTHv/RNDi`): Startwert für Versätze und Farben mehrerer
    /// Spitzen natürlicher Pinsel (`@789060`).
    pub random_seed: u32,
}

impl Default for VectorPath {
    fn default() -> Self {
        Self {
            contours: Vec::new(),
            even_odd: true,
            agm: false,
            fill: None,
            stroke: None,
            fill_over_stroke: false,
            placement: StrokePlacement::Center,
            ptt: false,
            texture_offset: Vec2::new(0.0, 0.0),
            random_seed: 0,
        }
    }
}

impl VectorPath {
    /// Pfad aus Konturen ohne Füllung und Strich (Vorgaben wie ein neuer Original-Pfad: EOF 1).
    pub fn new(contours: Vec<PathContour>) -> Self {
        Self {
            contours,
            ..Default::default()
        }
    }

    /// Die Punktlisten aller Konturen.
    pub fn polys(&self) -> Vec<&[Vec2]> {
        self.contours.iter().map(|c| c.points.as_slice()).collect()
    }
}

// ------------------------------------------------------------------ Füllung

/// Kante der Füllung (`FETi`/`FEFi`, 07 §4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FillEdge {
    /// Hart (`FETi = 0`): ein Abtastpunkt je Pixelmitte.
    Hard,
    /// Kantengeglättet (`FETi = 1`, `FEFi = 0`): 4×4-Teilabtastung.
    #[default]
    AntiAliased,
    /// Weiche Kante (`FETi = 1`, `FEFi = r > 0`): Dreiecksfilter mit Radius r (höchstens 1024).
    Feather(u32),
    /// Dither edge (Firepixel extension, FB-23; model edge 3 with width `r > 0`): the feather ramp
    /// of radius r, quantised to fully on / off by an ordered 8 × 8 threshold on document pixel
    /// coordinates (no partial coverage). Width 0 is [`FillEdge::Hard`].
    Dither(u32),
}

impl FillEdge {
    /// Aus `FETi`/`FEFi` (der Setter erzwingt `FEFi = 0`, wenn `FETi ≠ 1`; Referenz: `FETi ≠ 0` = AA).
    /// The model value 3 is Firepixel's dither edge (stored in files as a feather plus a marker).
    pub fn from_tags(feti: u32, fefi: u32) -> Self {
        if feti == 0 {
            Self::Hard
        } else if feti == 3 {
            if fefi > 0 {
                Self::Dither(fefi.min(1024))
            } else {
                Self::Hard
            }
        } else if fefi > 0 {
            Self::Feather(fefi.min(1024))
        } else {
            Self::AntiAliased
        }
    }
}

/// Stempelmodus der Füllung (`FSMi`): wirkt nur mit Textur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum StampingMode {
    /// 1 = „blend“: die Textur moduliert die Deckung.
    Blend,
    /// 2 = „blend opaque“ (Vorgabe; andere Werte werden zu 2): die Textur hellt die Farbe auf.
    #[default]
    BlendOpaque,
}

impl StampingMode {
    pub fn from_u32(v: u32) -> Self {
        if v == 1 { Self::Blend } else { Self::BlendOpaque }
    }
}

/// Verlaufsgriffe (`PSXf/PSYf`, `PEXf/PEYf`, `PFXf/PFYf`) in Dokumentkoordinaten.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GradientHandles {
    /// Startpunkt p1.
    pub p1: Vec2,
    /// Endpunkt p2 (u = 100).
    pub p2: Vec2,
    /// Dritter Griff p3 (v = 100).
    pub p3: Vec2,
}

/// Verlaufsform (`FSHi`, Tabelle `@017494c0`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum GradientShape {
    Linear = 2,
    Radial = 3,
    Conical = 4,
    Satin = 5,
    Pinch = 7,
    Folds = 8,
    Elliptical = 9,
    Rectangular = 10,
    Bars = 12,
    Ripple = 13,
    Waves = 14,
    /// Konturverlauf (`FSHi = 3`, `FSXi = 17`).
    Contour = 17,
}

impl GradientShape {
    /// Aus `FSHi`/`FSXi`; `None` für Nicht-Verläufe (0, 15, 16) und unbekannte Werte.
    pub fn from_tags(fshi: u32, fsxi: u32) -> Option<Self> {
        if fsxi == 17 {
            return Some(Self::Contour);
        }
        Some(match fshi {
            2 => Self::Linear,
            3 => Self::Radial,
            4 => Self::Conical,
            5 => Self::Satin,
            7 => Self::Pinch,
            8 => Self::Folds,
            9 => Self::Elliptical,
            10 => Self::Rectangular,
            12 => Self::Bars,
            13 => Self::Ripple,
            14 => Self::Waves,
            _ => return None,
        })
    }

    /// Gespiegelte Wiederholung (`@007e6285`)?
    pub fn mirrored(self) -> bool {
        matches!(
            self,
            Self::Satin | Self::Pinch | Self::Folds | Self::Bars | Self::Ripple
        )
    }
}

/// Ein Verlaufsstopp: Position 0..1 (`FGPf`) und Farbe ARGB (`FGCi`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GradientStop {
    pub pos: f64,
    pub color: Argb,
}

/// Verlauf: Form, Farbstopps (`FG0v`, alt `FGVv`) und Deckkraftstopps (`FG1v`).
#[derive(Debug, Clone, PartialEq)]
pub struct Gradient {
    pub shape: GradientShape,
    /// Farbstopps (liefern RGB); leer = Weiß.
    pub color_stops: Vec<GradientStop>,
    /// Deckkraftstopps (liefern Alpha); leer = Alpha 255.
    pub opacity_stops: Vec<GradientStop>,
    /// Gradient "Dither" (`FGDb`, RE spec 75 §1.5): S-shaped position mapping
    /// `t' = 0.5·t + 0.5·(3t² − 2t³)` plus ±1 noise per colour channel.
    pub dither: bool,
}

/// Art der Füllung.
#[derive(Debug, Clone, PartialEq)]
pub enum FillKind {
    /// Vollton (`FSHi = 0`), Farbe `FCLi` (Vorgabe FF000000); Farb-Alpha wirkt wie Deckkraft.
    Solid(Argb),
    /// Verlauf.
    Gradient(Gradient),
    /// Muster (`FSHi = 15`) mit dem Musterbild (`FPTv/IMGv`; Daten liefert der Aufrufer).
    Pattern(Arc<RgbaImage>),
    /// Web-Dither (`FSHi = 16`) mit den Farben `FD1i…FD4i`.
    WebDither([Argb; 4]),
}

/// Textur der Füllung (`TXFv`, `FTBi`).
#[derive(Debug, Clone, PartialEq)]
pub struct FillTexture {
    /// 8-Bit-Texturmaske; nur Größe und Daten zählen (gekachelt modulo Größe, Ursprung =
    /// [`VectorPath::texture_offset`]). Textur-Daten liefert der Aufrufer (keine Assets des Originals).
    pub mask: Arc<Mask>,
    /// Menge in Promille (`FTBi`).
    pub amount_permille: u32,
}

/// Füllung eines Pfads.
#[derive(Debug, Clone, PartialEq)]
pub struct Fill {
    pub kind: FillKind,
    pub edge: FillEdge,
    /// Griffe für Verlauf, Muster und Web-Dither.
    pub handles: GradientHandles,
    /// Textur (wirkt nur bei `amount_permille > 0`).
    pub texture: Option<FillTexture>,
    /// Stempelmodus (`FSMi`).
    pub stamping: StampingMode,
    /// Transparenz-Dither (`FDTb`): Deckung × Schachbrett.
    pub transparency_dither: bool,
    /// Uniform dither density in per cent (Firepixel extension, FB-38, FB-109): the share of
    /// empty pixels. 1..=100 empties the pixels whose ordered 8 × 8 (Bayer) threshold on document
    /// coordinates lies below the density, so the whole fill becomes an on/off half tone of its
    /// colour over transparency (50 = checkerboard, 100 = no pixel left); 0 leaves the fill
    /// unchanged. Applied before the edge.
    pub dither_density: u8,
}

impl Fill {
    /// Vollton-Füllung mit Kantenglättung.
    pub fn solid(color: Argb) -> Self {
        Self {
            kind: FillKind::Solid(color),
            edge: FillEdge::AntiAliased,
            handles: GradientHandles::default(),
            texture: None,
            stamping: StampingMode::BlendOpaque,
            transparency_dither: false,
            dither_density: 0,
        }
    }
}

// ------------------------------------------------------------------ Strich

/// Spitzenform (`BSHi`): 1 = rund, 0 = eckig.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TipShape {
    Square,
    #[default]
    Circle,
}

/// Pinseltyp (`BRTi`): 0 = natural, 1 = simple.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BrushType {
    Natural,
    #[default]
    Simple,
}

/// Tinten-Rückkopplung (`BFBi`): 0 = none, 1 = brush, 2 = background (zeichnet nichts).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Feedback {
    #[default]
    None,
    Brush,
    Background,
}

impl Feedback {
    pub fn from_u32(v: u32) -> Self {
        match v {
            1 => Self::Brush,
            2 => Self::Background,
            _ => Self::None,
        }
    }
}

/// Anordnung mehrerer Spitzen (`BSMi`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TipSpacingMode {
    #[default]
    Random,
    Diagonal,
    Circular,
}

/// Farbe mehrerer Spitzen (`BCMi`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum TipColoringMode {
    #[default]
    Random,
    Uniform,
    Complementary,
    Hue,
    Shadow,
}

/// Zielzeilen der Empfindlichkeits-Tabelle (`S<Quelle><Ziel>i`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SenseTarget {
    Size = 0,
    Angle = 1,
    Opacity = 2,
    Blackness = 3,
    Scatter = 4,
    Hue = 5,
    Lightness = 6,
    Saturation = 7,
}

/// Quellen der Empfindlichkeits-Tabelle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum SenseSource {
    Pressure = 0,
    Speed = 1,
    Horizontal = 2,
    Vertical = 3,
    Random = 4,
}

/// Pinselparameter (`BPLv`, Block `IDbStdBrushParms`, 07 §6.1). Prozentwerte in Promille wie in
/// der Datei.
#[derive(Debug, Clone, PartialEq)]
pub struct BrushParams {
    /// Durchmesser px (`BDIi`, Vorgabe 1).
    pub diameter: u32,
    /// Winkel 0..359 (`BANi`).
    pub angle: u32,
    /// Mindestgröße ‰ (`BMSi`).
    pub min_size: u32,
    /// Seitenverhältnis ‰ (`BASi`, Vorgabe 1000).
    pub aspect: u32,
    /// Anzahl Größen-/Winkelstufen 0..64 (`BMMi`).
    pub max_count: u32,
    /// Weichheit ‰ (`BSEi`).
    pub softness: u32,
    /// Spitzenform (`BSHi`).
    pub shape: TipShape,
    /// Weichheitskurve Glocke (`BSFi = 1`, Vorgabe) bzw. linear.
    pub bell_curve: bool,
    /// Pinseltyp (`BRTi`).
    pub kind: BrushType,
    /// Rückkopplung (`BFBi`).
    pub feedback: Feedback,
    /// Anordnung mehrerer Spitzen (`BSMi`).
    pub tip_spacing_mode: TipSpacingMode,
    /// Farbe mehrerer Spitzen (`BCMi`).
    pub tip_coloring_mode: TipColoringMode,
    /// alphaRemap 0..5 (`BEFi`).
    pub alpha_remap: u32,
    /// Stempelabstand ‰ von D′ (`BSPi`, Vorgabe 100).
    pub spacing: u32,
    /// Fluss ‰ (`BFRi`).
    pub flow_rate: u32,
    /// Anzahl Spitzen 1..32 (`BNTi`, Vorgabe 1).
    pub tip_count: u32,
    /// Spitzenabstand ‰ (`BTSi`).
    pub tip_spacing: u32,
    /// Schwärze ‰ (`BBKi`).
    pub blackness: u32,
    /// Konzentration = Deckkraft ‰ (`BCNi`, Vorgabe 1000).
    pub concentration: u32,
    /// Texturmenge ‰ (`BTBi`).
    pub texture_blend: u32,
    /// Texturkante ‰ (`BTEi`).
    pub texture_edge: u32,
    /// Kantenglättung (`BIAb`).
    pub antialiased: bool,
    /// Strichmuster (`NDIi` = Länge, `DO1i…DO3i`, `DF1i…DF3i`): (an, aus) in px bzw. Stempeln.
    pub dashes: Vec<(u32, u32)>,
    /// Empfindlichkeiten intern 0..128 (Datei: v·1000/128): Zeile = [`SenseTarget`], Spalte = [`SenseSource`].
    pub sense: [[u8; 5]; 8],
}

impl Default for BrushParams {
    fn default() -> Self {
        Self {
            diameter: 1,
            angle: 0,
            min_size: 0,
            aspect: 1000,
            max_count: 0,
            softness: 0,
            shape: TipShape::Circle,
            bell_curve: true,
            kind: BrushType::Simple,
            feedback: Feedback::None,
            tip_spacing_mode: TipSpacingMode::Random,
            tip_coloring_mode: TipColoringMode::Random,
            alpha_remap: 0,
            spacing: 100,
            flow_rate: 0,
            tip_count: 1,
            tip_spacing: 0,
            blackness: 0,
            concentration: 1000,
            texture_blend: 0,
            texture_edge: 0,
            antialiased: false,
            dashes: Vec::new(),
            sense: [[0; 5]; 8],
        }
    }
}

impl BrushParams {
    /// Dateiwert (Promille) einer Empfindlichkeit → interner Wert 0..128 (`round(v·128/1000)`).
    pub fn sense_from_permille(v: u32) -> u8 {
        ((v.min(1000) as f64 * 128.0 / 1000.0).round()) as u8
    }

    /// Ist irgendeine Empfindlichkeit der Zeile gesetzt (Flag „Zeile aktiv“, 07 §6.1, A)?
    pub fn row_active(&self, t: SenseTarget) -> bool {
        self.sense[t as usize].iter().any(|&v| v != 0)
    }
}

/// Textur eines Strichs (`TXBv`; aktiv bei `BTBi > 0` oder `BTEi > 0`).
#[derive(Debug, Clone, PartialEq)]
pub struct StrokeTexture {
    /// 8-Bit-Texturmaske (Daten vom Aufrufer).
    pub mask: Arc<Mask>,
}

/// Strich eines Pfads.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub brush: BrushParams,
    /// Strichfarbe (`BCLi`); Farb-Alpha wirkt wie Deckkraft.
    pub color: Argb,
    /// Strichtextur (optional).
    pub texture: Option<StrokeTexture>,
}

impl Stroke {
    /// Strich mit Pinsel und Farbe, ohne Textur.
    pub fn new(brush: BrushParams, color: Argb) -> Self {
        Self {
            brush,
            color,
            texture: None,
        }
    }
}
