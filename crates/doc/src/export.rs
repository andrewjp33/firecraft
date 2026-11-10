//! Export settings (Fireworks' Optimize panel): the format a document exports to and its options.
//!
//! Fireworks MX keeps them in the document (`EXPv`; field codes measured with the oracle, see
//! docs/behavior/export.md). WebP and JPEG XL are FireCraft's additions.

use firecraft_fwpng::Node;
use firecraft_raster::Rgba;

/// The export file format (the Optimize panel's format menu).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ExportFormat {
    #[default]
    Gif,
    AnimatedGif,
    Jpeg,
    Png8,
    Png24,
    Png32,
    WebP,
    JpegXl,
}

impl ExportFormat {
    pub const ALL: [ExportFormat; 8] = [
        Self::Gif,
        Self::AnimatedGif,
        Self::Jpeg,
        Self::Png8,
        Self::Png24,
        Self::Png32,
        Self::WebP,
        Self::JpegXl,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Gif => "GIF",
            Self::AnimatedGif => "Animated GIF",
            Self::Jpeg => "JPEG",
            Self::Png8 => "PNG 8",
            Self::Png24 => "PNG 24",
            Self::Png32 => "PNG 32",
            Self::WebP => "WebP",
            Self::JpegXl => "JPEG XL",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Gif | Self::AnimatedGif => "gif",
            Self::Jpeg => "jpg",
            Self::Png8 | Self::Png24 | Self::Png32 => "png",
            Self::WebP => "webp",
            Self::JpegXl => "jxl",
        }
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::Gif | Self::AnimatedGif => "image/gif",
            Self::Jpeg => "image/jpeg",
            Self::Png8 | Self::Png24 | Self::Png32 => "image/png",
            Self::WebP => "image/webp",
            Self::JpegXl => "image/jxl",
        }
    }

    /// Exports through a colour palette (GIF, PNG 8).
    pub fn indexed(self) -> bool {
        matches!(self, Self::Gif | Self::AnimatedGif | Self::Png8)
    }

    /// FireCraft's own formats, with Quality and Alpha Quality.
    pub fn modern(self) -> bool {
        matches!(self, Self::WebP | Self::JpegXl)
    }
}

/// How an indexed export picks its colours (Fireworks MX's palette menu).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PaletteKind {
    Adaptive,
    /// Adaptive, with colours close to a Web 216 colour snapped to it.
    #[default]
    WebAdaptive,
    Web216,
    Exact,
    Windows,
    Macintosh,
    Grayscale,
    BlackWhite,
    Uniform,
}

impl PaletteKind {
    pub const ALL: [PaletteKind; 9] = [
        Self::Adaptive,
        Self::WebAdaptive,
        Self::Web216,
        Self::Exact,
        Self::Windows,
        Self::Macintosh,
        Self::Grayscale,
        Self::BlackWhite,
        Self::Uniform,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Adaptive => "Adaptive",
            Self::WebAdaptive => "WebSnap Adaptive",
            Self::Web216 => "Web 216",
            Self::Exact => "Exact",
            Self::Windows => "Windows",
            Self::Macintosh => "Macintosh",
            Self::Grayscale => "Grayscale",
            Self::BlackWhite => "Black & White",
            Self::Uniform => "Uniform",
        }
    }
}

/// Transparency of an indexed export.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Transparency {
    #[default]
    None,
    /// The canvas colour (or a chosen colour) becomes the transparent index.
    Index,
    /// Transparent where the image is: one transparent index for GIF (pixels under half
    /// opacity), a palette with alpha for PNG 8.
    Alpha,
}

impl Transparency {
    pub const ALL: [Transparency; 3] = [Self::None, Self::Index, Self::Alpha];

    pub fn label(self) -> &'static str {
        match self {
            Self::None => "No Transparency",
            Self::Index => "Index Transparency",
            Self::Alpha => "Alpha Transparency",
        }
    }
}

/// A document's export settings.
#[derive(Clone, Debug, PartialEq)]
pub struct ExportSettings {
    pub format: ExportFormat,
    pub palette: PaletteKind,
    /// Maximum colours of an indexed export (2..256).
    pub colors: u16,
    /// WebSnap Adaptive: how close (per channel) a colour must be to snap to Web 216.
    pub websnap_tolerance: u8,
    /// Diffusion dither, percent (0 = none).
    pub dither: u8,
    pub transparency: Transparency,
    /// Colour under semi-transparent pixels where the format can't keep them. `None`: the
    /// canvas colour, or white when the canvas is transparent.
    pub matte: Option<Rgba>,
    pub interlaced: bool,
    /// Animated GIF: times to play, 0 = forever.
    pub loops: u16,
    /// JPEG quality 0..100.
    pub jpeg_quality: u8,
    /// JPEG smoothing 0..8 (blurs before compressing).
    pub jpeg_smoothing: u8,
    pub progressive: bool,
    /// JPEG: full-resolution colour (no 4:2:0 subsampling), as Fireworks MX writes JPEGs.
    pub jpeg_full_color: bool,
    /// WebP / JPEG XL quality 0..100.
    pub quality: u8,
    /// WebP / JPEG XL quality of the alpha channel 0..100.
    pub alpha_quality: u8,
    /// WebP / JPEG XL: lossless.
    pub lossless: bool,
    /// The record read from the file (`EXPv`), kept for the fields FireCraft doesn't model.
    pub raw: Option<Node>,
}

impl Default for ExportSettings {
    /// A new document in Fireworks MX: GIF, WebSnap Adaptive, 128 colours, no dither, no
    /// transparency (read with the oracle from `dom.exportOptions`).
    fn default() -> Self {
        Self {
            format: ExportFormat::Gif,
            palette: PaletteKind::WebAdaptive,
            colors: 128,
            websnap_tolerance: 14,
            dither: 0,
            transparency: Transparency::None,
            matte: None,
            interlaced: false,
            loops: 0,
            jpeg_quality: 80,
            jpeg_smoothing: 0,
            progressive: false,
            jpeg_full_color: true,
            quality: 80,
            alpha_quality: 100,
            lossless: false,
            raw: None,
        }
    }
}

/// The Optimize panel's saved settings (Fireworks MX's presets, and FireCraft's for its own
/// formats).
pub const PRESETS: [&str; 10] = [
    "GIF Web 216",
    "GIF WebSnap 256",
    "GIF WebSnap 128",
    "GIF Adaptive 256",
    "JPEG - Better Quality",
    "JPEG - Smaller File",
    "Animated GIF WebSnap 128",
    "PNG 32",
    "WebP",
    "JPEG XL",
];

impl ExportSettings {
    /// A preset by name (see [`PRESETS`]), from the default settings.
    pub fn preset(name: &str) -> Option<Self> {
        let d = Self::default();
        let gif = |palette, colors| Self {
            format: ExportFormat::Gif,
            palette,
            colors,
            ..Self::default()
        };
        Some(match name {
            "GIF Web 216" => gif(PaletteKind::Web216, 256),
            "GIF WebSnap 256" => gif(PaletteKind::WebAdaptive, 256),
            "GIF WebSnap 128" => gif(PaletteKind::WebAdaptive, 128),
            "GIF Adaptive 256" => gif(PaletteKind::Adaptive, 256),
            "JPEG - Better Quality" => Self {
                format: ExportFormat::Jpeg,
                jpeg_quality: 80,
                ..d
            },
            "JPEG - Smaller File" => Self {
                format: ExportFormat::Jpeg,
                jpeg_quality: 60,
                jpeg_smoothing: 2,
                ..d
            },
            "Animated GIF WebSnap 128" => Self {
                format: ExportFormat::AnimatedGif,
                ..d
            },
            "PNG 32" => Self {
                format: ExportFormat::Png32,
                ..d
            },
            "WebP" => Self {
                format: ExportFormat::WebP,
                quality: 80,
                alpha_quality: 100,
                ..d
            },
            "JPEG XL" => Self {
                format: ExportFormat::JpegXl,
                quality: 85,
                alpha_quality: 100,
                ..d
            },
            _ => return None,
        })
    }

    /// The preset these settings equal, if any (ignoring options the format doesn't use).
    pub fn preset_name(&self) -> Option<&'static str> {
        PRESETS
            .iter()
            .copied()
            .find(|n| Self::preset(n).is_some_and(|p| p.same_effect(self)))
    }

    /// Whether `other` exports the same way (only the options its format uses count).
    pub fn same_effect(&self, other: &Self) -> bool {
        if self.format != other.format {
            return false;
        }
        match self.format {
            ExportFormat::Gif | ExportFormat::AnimatedGif | ExportFormat::Png8 => {
                self.palette == other.palette
                    && self.colors == other.colors
                    && self.dither == other.dither
                    && self.transparency == other.transparency
                    && self.interlaced == other.interlaced
                    && (self.format != ExportFormat::AnimatedGif || self.loops == other.loops)
            }
            ExportFormat::Jpeg => {
                self.jpeg_quality == other.jpeg_quality
                    && self.jpeg_smoothing == other.jpeg_smoothing
                    && self.progressive == other.progressive
            }
            ExportFormat::Png24 | ExportFormat::Png32 => self.interlaced == other.interlaced,
            ExportFormat::WebP | ExportFormat::JpegXl => {
                self.quality == other.quality
                    && self.alpha_quality == other.alpha_quality
                    && self.lossless == other.lossless
            }
        }
    }
}
