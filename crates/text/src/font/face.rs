//! [`Font`]: eine geladene Schrift (Daten im Speicher) mit den Metriken, die der Satz des Originals
//! braucht.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use ttf_parser::{Face, GlyphId, Tag};

use super::outline::{Collector, GlyphOutline};
use super::sfnt::{Bytes, FontNames, KernTables, base_ideo, parse_names};
use crate::TextError;

/// Gemessene Oberlängen (Anteil der Schriftgröße) aus den gerundeten Korpus-Rahmen, nach Familie
/// (09 §5.3, Python-Referenz `layout.ASCENT_EMPIRICAL`). Sie gehen den Regeln vor und sind für
/// byte-genaue Altrahmen nötig (Arial 0,857986 statt Regelwert 0,858154).
const ASCENT_MEASURED: &[(&str, f64)] = &[
    ("myriad pro", 0.837006),
    ("arial", 0.857986),
    ("times new roman", 0.826),
    ("verdana", 0.863495),
];

/// Eine geladene Schrift. Unveränderlich und günstig über `Arc` teilbar; Konturen werden bei Bedarf
/// gelesen und zwischengespeichert.
pub struct Font {
    data: Arc<Vec<u8>>,
    index: u32,
    path: Option<PathBuf>,
    names: FontNames,
    units_per_em: u16,
    bbox: [i16; 4],
    cap_height: Option<i16>,
    hhea_ascender: i16,
    ideo: Option<i16>,
    underline: Option<(i16, i16)>,
    num_glyphs: u16,
    typo_ascender: Option<i16>,
    /// `OS/2.usWinAscent + usWinDescent`: the cell height Windows sizes a font by when asked for a
    /// positive height.
    win_cell: Option<u32>,
    kern: OnceLock<KernTables>,
    roman_ascent: OnceLock<f64>,
    outlines: Mutex<HashMap<u16, Arc<GlyphOutline>>>,
    /// Gehintete Umrisse je (Glyphe, ppem-Bits, Art) in Pixeln der Rasterauflösung.
    hinted: Mutex<HintedCache>,
    /// Hinting-Instanzen je (ppem-Bits, Art) (das `prep`-Programm läuft einmal je Größe).
    hinters: Mutex<HinterCache>,
}

/// Höchstzahl zwischengespeicherter gehinteter Umrisse bzw. Hinting-Instanzen je Schrift.
const MAX_HINTED: usize = 16_384;
const MAX_HINTERS: usize = 64;

type HintedCache = HashMap<(u16, u32, u8), Option<Arc<GlyphOutline>>>;
type HinterCache = HashMap<(u32, u8), Option<Arc<skrifa::outline::HintingInstance>>>;

/// Hinting-Art für [`Font::hinted_outline`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HintStyle {
    /// Volles Hinting für Schwarzweiß-Rasterung (beide Richtungen, `FT_LOAD_TARGET_MONO`).
    Mono,
    /// Glattes Hinting (`FT_LOAD_TARGET_NORMAL`).
    Normal,
    /// Leichtes Hinting (nur senkrecht, `FT_LOAD_TARGET_LIGHT`).
    Light,
}

impl HintStyle {
    fn code(self) -> u8 {
        self as u8
    }

    fn target(self) -> skrifa::outline::Target {
        use skrifa::outline::{SmoothMode, Target};
        match self {
            HintStyle::Mono => Target::Mono,
            HintStyle::Normal => Target::Smooth {
                mode: SmoothMode::Normal,
                symmetric_rendering: true,
                preserve_linear_metrics: false,
            },
            HintStyle::Light => Target::Smooth {
                mode: SmoothMode::Light,
                symmetric_rendering: true,
                preserve_linear_metrics: false,
            },
        }
    }
}

/// Stift für skrifa: sammelt Konturen in Pixeln (y nach oben).
#[derive(Default)]
struct SkPen(Collector);

impl skrifa::outline::OutlinePen for SkPen {
    fn move_to(&mut self, x: f32, y: f32) {
        ttf_parser::OutlineBuilder::move_to(&mut self.0, x, y);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        ttf_parser::OutlineBuilder::line_to(&mut self.0, x, y);
    }
    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        ttf_parser::OutlineBuilder::quad_to(&mut self.0, cx0, cy0, x, y);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        ttf_parser::OutlineBuilder::curve_to(&mut self.0, cx0, cy0, cx1, cy1, x, y);
    }
    fn close(&mut self) {
        ttf_parser::OutlineBuilder::close(&mut self.0);
    }
}

impl std::fmt::Debug for Font {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Font")
            .field("ps_name", &self.names.ps_name)
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl Font {
    /// Schrift aus Bytes (Index in einer Sammlung `.ttc`).
    pub fn from_data(data: Vec<u8>, index: u32) -> Result<Self, TextError> {
        Self::build(Arc::new(data), index, None)
    }

    /// Schrift aus einer Datei.
    pub fn from_file(path: &Path, index: u32) -> Result<Self, TextError> {
        let data =
            std::fs::read(path).map_err(|e| TextError::Io(path.display().to_string(), e.to_string()))?;
        Self::build(Arc::new(data), index, Some(path.to_path_buf()))
    }

    fn build(data: Arc<Vec<u8>>, index: u32, path: Option<PathBuf>) -> Result<Self, TextError> {
        let face = Face::parse(&data, index).map_err(|e| TextError::BadFont(e.to_string()))?;
        let raw = face.raw_face();
        let names = raw
            .table(Tag::from_bytes(b"name"))
            .map(parse_names)
            .unwrap_or_default();
        let bb = face.global_bounding_box();
        let upm = face.units_per_em().max(1);
        let cap_height = raw.table(Tag::from_bytes(b"OS/2")).and_then(|o| {
            let b = Bytes(o);
            if b.u16(0)? >= 2 && o.len() >= 90 {
                b.i16(88)
            } else {
                None
            }
        });
        let hhea_ascender = raw
            .table(Tag::from_bytes(b"hhea"))
            .and_then(|h| Bytes(h).i16(4))
            .unwrap_or(0);
        let typo_ascender = raw.table(Tag::from_bytes(b"OS/2")).and_then(|o| Bytes(o).i16(68));
        let win_cell = raw.table(Tag::from_bytes(b"OS/2")).and_then(|o| {
            let (a, d) = (Bytes(o).i16(74)? as u16, Bytes(o).i16(76)? as u16);
            (a > 0 || d > 0).then_some(u32::from(a) + u32::from(d))
        });
        let ideo = raw.table(Tag::from_bytes(b"BASE")).and_then(base_ideo);
        let underline = face.underline_metrics().map(|m| (m.position, m.thickness));
        let num_glyphs = face.number_of_glyphs();
        Ok(Self {
            index,
            path,
            names,
            units_per_em: upm,
            bbox: [bb.x_min, bb.y_min, bb.x_max, bb.y_max],
            cap_height,
            hhea_ascender,
            ideo,
            underline,
            num_glyphs,
            typo_ascender,
            win_cell,
            kern: OnceLock::new(),
            roman_ascent: OnceLock::new(),
            outlines: Mutex::new(HashMap::new()),
            hinted: Mutex::new(HashMap::new()),
            hinters: Mutex::new(HashMap::new()),
            data,
        })
    }

    fn face(&self) -> Option<Face<'_>> {
        Face::parse(&self.data, self.index).ok()
    }

    /// Rohdaten der Schriftdatei (für rustybuzz).
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Index in der Sammlung.
    pub fn index(&self) -> u32 {
        self.index
    }

    /// Dateipfad, falls aus einer Datei geladen.
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// PostScript-Name (Name-ID 6).
    pub fn ps_name(&self) -> &str {
        &self.names.ps_name
    }

    /// Familie (Name-ID 16, sonst 1).
    pub fn family(&self) -> &str {
        &self.names.family
    }

    /// Stil (Name-ID 17, sonst 2).
    pub fn subfamily(&self) -> &str {
        &self.names.subfamily
    }

    /// Einheiten je em (`head.unitsPerEm`).
    pub fn units_per_em(&self) -> u16 {
        self.units_per_em
    }

    /// Schrift-Bounding-Box `head` (xMin, yMin, xMax, yMax).
    pub fn bbox(&self) -> [i16; 4] {
        self.bbox
    }

    /// Anzahl der Glyphen.
    pub fn num_glyphs(&self) -> u16 {
        self.num_glyphs
    }

    /// Glyphen-ID eines Zeichens über `cmap`; 0 (`.notdef`), wenn die Schrift es nicht hat.
    pub fn glyph_index(&self, ch: char) -> u16 {
        self.face()
            .and_then(|f| f.glyph_index(ch))
            .map(|g| g.0)
            .unwrap_or(0)
    }

    /// Hat die Schrift eine Glyphe für das Zeichen?
    pub fn has_glyph(&self, ch: char) -> bool {
        self.face().and_then(|f| f.glyph_index(ch)).is_some()
    }

    /// Vorschub (`hmtx`) in Font-Einheiten.
    pub fn advance(&self, gid: u16) -> u16 {
        self.face()
            .and_then(|f| f.glyph_hor_advance(GlyphId(gid)))
            .unwrap_or(0)
    }

    /// Paar-Kerning in Font-Einheiten (GPOS `kern`-Feature, sonst `kern`-Tabelle; 09 §5.6).
    pub fn kerning(&self, g1: u16, g2: u16) -> i32 {
        self.kern
            .get_or_init(|| {
                let face = self.face();
                let raw = face.as_ref().map(|f| f.raw_face());
                KernTables::parse(
                    raw.and_then(|r| r.table(Tag::from_bytes(b"GPOS"))),
                    raw.and_then(|r| r.table(Tag::from_bytes(b"kern"))),
                )
            })
            .get(g1, g2) as i32
    }

    /// How many pixels a font unit of kerning is worth at `size` in Fireworks MX: Windows' kerning
    /// pairs for a font sized by its cell (`usWinAscent + usWinDescent`), not its em (measured on
    /// Arial 12-36: pairs of -152 units give -2 px at 36, not -3).
    pub fn kerning_scale(&self, size: f64) -> f64 {
        let units = self
            .win_cell
            .map_or(f64::from(self.units_per_em), f64::from)
            .max(1.0);
        size / units
    }

    /// The `kern` table's kerning alone (Fireworks MX, through Windows' GDI, ignores GPOS).
    pub fn legacy_kerning(&self, g1: u16, g2: u16) -> i32 {
        self.kern
            .get_or_init(|| {
                let face = self.face();
                let raw = face.as_ref().map(|f| f.raw_face());
                KernTables::parse(
                    raw.and_then(|r| r.table(Tag::from_bytes(b"GPOS"))),
                    raw.and_then(|r| r.table(Tag::from_bytes(b"kern"))),
                )
            })
            .get_legacy(g1, g2) as i32
    }

    /// Umriss einer Glyphe (Font-Einheiten, y nach oben), zwischengespeichert.
    pub fn outline(&self, gid: u16) -> Arc<GlyphOutline> {
        let mut cache = self.outlines.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(o) = cache.get(&gid) {
            return o.clone();
        }
        let mut col = Collector::default();
        if let Some(f) = self.face() {
            let _ = f.outline_glyph(GlyphId(gid), &mut col);
        }
        let o = Arc::new(GlyphOutline::new(col.into_contours()));
        cache.insert(gid, o.clone());
        o
    }

    /// Gehinteter Umriss einer Glyphe bei `ppem` Pixeln je em, in **Pixeln** (y nach oben),
    /// zwischengespeichert. `None`, wenn die Schrift kein Hinting erlaubt oder die Glyphe fehlt
    /// (dann gilt der ungehintete Umriss).
    pub fn hinted_outline(&self, gid: u16, ppem: f32, style: HintStyle) -> Option<Arc<GlyphOutline>> {
        if !(ppem.is_finite() && ppem > 0.0 && ppem <= 16384.0) {
            return None;
        }
        let key = (gid, ppem.to_bits(), style.code());
        if let Some(o) = self.hinted.lock().unwrap_or_else(|e| e.into_inner()).get(&key) {
            return o.clone();
        }
        let r = self.compute_hinted(gid, ppem, style).map(Arc::new);
        let mut cache = self.hinted.lock().unwrap_or_else(|e| e.into_inner());
        // viele Größen (z. B. beim stufenlosen Zoomen): Zwischenspeicher begrenzen
        if cache.len() >= MAX_HINTED {
            cache.clear();
        }
        cache.insert(key, r.clone());
        r
    }

    fn compute_hinted(&self, gid: u16, ppem: f32, style: HintStyle) -> Option<GlyphOutline> {
        use skrifa::MetadataProvider;
        use skrifa::instance::{LocationRef, Size};
        use skrifa::outline::{DrawSettings, HintingInstance, HintingOptions};
        let font = skrifa::FontRef::from_index(&self.data, self.index).ok()?;
        let outlines = font.outline_glyphs();
        let ikey = (ppem.to_bits(), style.code());
        let inst = {
            let mut hs = self.hinters.lock().unwrap_or_else(|e| e.into_inner());
            if hs.len() >= MAX_HINTERS && !hs.contains_key(&ikey) {
                hs.clear();
            }
            hs.entry(ikey)
                .or_insert_with(|| {
                    let opts = HintingOptions {
                        engine: skrifa::outline::Engine::Interpreter,
                        target: style.target(),
                    };
                    HintingInstance::new(&outlines, Size::new(ppem), LocationRef::default(), opts)
                        .ok()
                        .map(Arc::new)
                })
                .clone()?
        };
        let glyph = outlines.get(skrifa::GlyphId::new(gid as u32))?;
        let mut pen = SkPen::default();
        glyph.draw(DrawSettings::hinted(&inst, false), &mut pen).ok()?;
        Some(GlyphOutline::new(pen.0.into_contours()))
    }

    /// Unterlänge für den Textrahmen als Anteil der Schriftgröße: `−head.yMin / unitsPerEm`
    /// (09 §5.3, B).
    pub fn descent(&self) -> f64 {
        -(self.bbox[1] as f64) / self.units_per_em as f64
    }

    /// Ideographische Grundlinie aus `BASE` in Font-Einheiten.
    pub fn ideo_baseline(&self) -> Option<i16> {
        self.ideo
    }

    /// Versalhöhe (`OS/2.sCapHeight`, ab Version 2).
    pub fn cap_height(&self) -> Option<i16> {
        self.cap_height
    }

    /// Oberlänge für den Textrahmen = Oberkante der em-Box als Anteil der Schriftgröße (09 §5.3):
    /// gemessene Werte der Korpusfamilien zuerst; mit `BASE` `1 + ideo/upm` (B); ohne `BASE` die auf
    /// der Versalhöhe zentrierte em-Box `(capHeight/2 + upm/2)/upm` (A); zuletzt `hhea.ascender`.
    pub fn ascent(&self) -> f64 {
        let fam = self.names.family.to_lowercase();
        if let Some((_, v)) = ASCENT_MEASURED.iter().find(|(f, _)| *f == fam) {
            return *v;
        }
        self.ascent_rule()
    }

    /// Oberlänge nur nach Regel (ohne gemessene Ausnahmen).
    pub fn ascent_rule(&self) -> f64 {
        let upm = self.units_per_em as f64;
        if let Some(ib) = self.ideo {
            return 1.0 + ib as f64 / upm;
        }
        if let Some(ch) = self.cap_height.filter(|&c| c != 0) {
            return (ch as f64 / 2.0 + upm / 2.0) / upm;
        }
        self.hhea_ascender as f64 / upm
    }

    /// Lateinische Oberlänge als Anteil der Schriftgröße: Oberkante der Glyphe „d“ (sonst „l“, „h“,
    /// `OS/2.sTypoAscender`, `hhea.ascender`). Das Original setzt damit die erste Grundlinie von
    /// Rahmentext und Text in Pfad (A: Myriad Pro 20 px → 14,2 px unter der Oberkante; Testdateien
    /// T1/T2/T8 per Pixelvergleich, Rahmen von `t2` „in Pfad“ oben/unten exakt).
    pub fn roman_ascent(&self) -> f64 {
        *self.roman_ascent.get_or_init(|| {
            let upm = self.units_per_em as f64;
            for ch in ['d', 'l', 'h'] {
                if self.has_glyph(ch)
                    && let Some(b) = self.outline(self.glyph_index(ch)).ink
                {
                    return b[3] / upm;
                }
            }
            match self.typo_ascender {
                Some(t) if t > 0 => t as f64 / upm,
                _ => self.hhea_ascender as f64 / upm,
            }
        })
    }

    /// Unterstreichung (`post.underlinePosition`, `underlineThickness`) in Font-Einheiten.
    pub fn underline_metrics(&self) -> Option<(i16, i16)> {
        self.underline
    }
}
