//! Tools and their options (what the Property inspector edits when a tool is active).

use firecraft_raster::{AntiAliasStyle, Edge, Rgba, WandOptions};

/// The Tools panel, in Fireworks MX's sections.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Tool {
    // Select
    #[default]
    Pointer,
    /// Fireworks MX's Select Behind tool (the Pointer's flyout): each click on the same spot
    /// selects the next object down.
    SelectBehind,
    Subselect,
    Scale,
    Skew,
    Distort,
    /// Fireworks CS5's 9-slice Scaling tool (shares the Scale tool's slot).
    NineSlice,
    Crop,
    // Bitmap
    Marquee,
    OvalMarquee,
    Lasso,
    MagicWand,
    Brush,
    Pencil,
    Eraser,
    Blur,
    RubberStamp,
    Eyedropper,
    PaintBucket,
    // Vector
    Line,
    Pen,
    Rectangle,
    Ellipse,
    Text,
    Freeform,
    Knife,
    // Web
    Hotspot,
    Slice,
    // View
    Hand,
    Zoom,
}

impl Tool {
    /// Name shown in the Property inspector.
    pub fn label(self) -> &'static str {
        match self {
            Tool::Pointer => "Pointer tool",
            Tool::SelectBehind => "Select Behind tool",
            Tool::Subselect => "Subselection tool",
            Tool::Scale => "Scale tool",
            Tool::Skew => "Skew tool",
            Tool::Distort => "Distort tool",
            Tool::NineSlice => "9-slice Scaling tool",
            Tool::Crop => "Crop tool",
            Tool::Marquee => "Marquee tool",
            Tool::OvalMarquee => "Oval Marquee tool",
            Tool::Lasso => "Lasso tool",
            Tool::MagicWand => "Magic Wand tool",
            Tool::Brush => "Brush tool",
            Tool::Pencil => "Pencil tool",
            Tool::Eraser => "Eraser tool",
            Tool::Blur => "Blur tool",
            Tool::RubberStamp => "Rubber Stamp tool",
            Tool::Eyedropper => "Eyedropper tool",
            Tool::PaintBucket => "Paint Bucket tool",
            Tool::Line => "Line tool",
            Tool::Pen => "Pen tool",
            Tool::Rectangle => "Rectangle tool",
            Tool::Ellipse => "Ellipse tool",
            Tool::Text => "Text tool",
            Tool::Freeform => "Freeform tool",
            Tool::Knife => "Knife tool",
            Tool::Hotspot => "Rectangle Hotspot tool",
            Tool::Slice => "Slice tool",
            Tool::Hand => "Hand tool",
            Tool::Zoom => "Zoom tool",
        }
    }

    /// Single-key shortcut (Fireworks MX defaults).
    pub fn shortcut(self) -> Option<char> {
        Some(match self {
            Tool::Pointer | Tool::SelectBehind => 'V',
            Tool::Subselect => 'A',
            Tool::Scale | Tool::Skew | Tool::Distort | Tool::NineSlice => 'Q',
            Tool::Crop => 'C',
            Tool::Marquee | Tool::OvalMarquee => 'M',
            Tool::Lasso => 'L',
            Tool::MagicWand => 'W',
            Tool::Brush => 'B',
            Tool::Pencil => 'Y',
            Tool::Eraser => 'E',
            Tool::Blur => 'R',
            Tool::RubberStamp => 'S',
            Tool::Eyedropper => 'I',
            Tool::PaintBucket => 'K',
            Tool::Line => 'N',
            Tool::Pen => 'P',
            Tool::Rectangle | Tool::Ellipse => 'U',
            Tool::Text => 'T',
            Tool::Freeform => 'O',
            Tool::Knife => 'Y',
            Tool::Hotspot => 'J',
            Tool::Slice => 'K',
            Tool::Hand => 'H',
            Tool::Zoom => 'Z',
        })
    }

    /// Implemented in this build (others are visible but inactive).
    pub fn implemented(self) -> bool {
        matches!(
            self,
            Tool::Pointer
                | Tool::SelectBehind
                | Tool::Subselect
                | Tool::Pen
                | Tool::Scale
                | Tool::Skew
                | Tool::Distort
                | Tool::NineSlice
                | Tool::Marquee
                | Tool::OvalMarquee
                | Tool::MagicWand
                | Tool::Line
                | Tool::Rectangle
                | Tool::Ellipse
                | Tool::Hand
                | Tool::Zoom
                | Tool::Eyedropper
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolOptions {
    pub tolerance: u8,
    pub wand_edge: Edge,
    pub aa_style: AntiAliasStyle,
    pub marquee_edge: Edge,
    /// Stroke colour (`None` = no stroke), as the Tools panel's colour boxes.
    pub stroke: Option<Rgba>,
    pub fill: Option<Rgba>,
    pub stroke_size: u32,
    /// Rectangle tool: corner roundness, 0..=1 (Property inspector "Rectangle roundness", %).
    pub roundness: f64,
    /// Stroke preset for new strokes (index into `firecraft_doc::brush_presets`) and its edge
    /// softness (percent).
    pub stroke_preset: usize,
    pub stroke_softness: u8,
}

impl ToolOptions {
    /// A new stroke with the tool settings, in `color`.
    pub fn new_stroke(&self, color: Rgba) -> firecraft_doc::Stroke {
        let mut s = firecraft_doc::Stroke::from_preset(color, self.stroke_preset, self.stroke_size as f64);
        s.softness = self.stroke_softness.min(100);
        s
    }
}

impl Default for ToolOptions {
    fn default() -> Self {
        Self {
            tolerance: 32,
            wand_edge: Edge::AntiAlias,
            aa_style: AntiAliasStyle::Smooth,
            marquee_edge: Edge::AntiAlias,
            stroke: Some(Rgba::BLACK),
            fill: Some(Rgba::new(0x99, 0x99, 0x99, 255)),
            stroke_size: 1,
            roundness: 0.0,
            stroke_preset: firecraft_doc::brush_presets::default_index(),
            stroke_softness: 0,
        }
    }
}

impl ToolOptions {
    pub fn wand(&self) -> WandOptions {
        WandOptions {
            tolerance: self.tolerance,
            edge: self.wand_edge,
            aa_style: self.aa_style,
            contiguous: true,
        }
    }
}
