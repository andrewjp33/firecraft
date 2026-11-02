//! The FireCraft document model, shaped after Fireworks MX:
//!
//! * a **document** has a canvas (size, resolution, colour) and an ordered list of **layers**
//!   (index 0 is the bottom layer, drawn first);
//! * each layer holds one **cell** per **frame** (animation state), and a cell holds the
//!   layer's **elements** in that frame. A layer can be shared across frames, in which case
//!   every frame shows cell 0;
//! * elements are bitmaps, vector paths, text, groups, or anything else (kept as raw
//!   Fireworks tree data so it survives a save even before FireCraft understands it).
//!
//! The model is plain data. Editing goes through `firecraft-engine`; file formats live in
//! `firecraft-io`.

pub mod element;

mod cache;
pub mod effects;
pub use cache::{Dirty, RenderCache};
pub use effects::{EffectEntry, EffectParam};
pub use element::{BezierPoint, Bitmap, Contour, Element, ElementKind, Fill, MaskMode, Path, Stroke, Text};
use firecraft_fwpng::Node;
use firecraft_raster::{ColorMath, Rgba, RgbaImage};

/// Canvas colour: Fireworks offers white, transparent or a custom colour.
pub type CanvasColor = Option<Rgba>;

/// A symbol definition (Modify > Symbol): a small document of its own that instances show.
#[derive(Clone, Debug, PartialEq)]
pub struct Symbol {
    pub id: u32,
    pub name: String,
    pub layers: Vec<Layer>,
    /// Animated symbols have several frames; an instance shows frame (document frame mod count).
    pub frames: usize,
}

/// Symbols may contain instances of other symbols; deeper nesting is ignored (cycle guard).
pub const MAX_SYMBOL_DEPTH: u32 = 16;

/// What rendering needs besides the element itself.
#[derive(Clone, Copy, Debug)]
pub struct RenderCtx<'a> {
    pub symbols: &'a [Symbol],
    pub frame: usize,
    pub depth: u32,
    pub math: ColorMath,
}

impl RenderCtx<'_> {
    pub const EMPTY: RenderCtx<'static> = RenderCtx {
        symbols: &[],
        frame: 0,
        depth: 0,
        math: ColorMath::Classic,
    };

    pub fn deeper(&self) -> Self {
        Self {
            depth: self.depth + 1,
            ..*self
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Frame {
    pub name: String,
    /// Delay in 1/100 s (Fireworks' Frames panel unit).
    pub delay: u32,
    /// Include when exporting (the frame's "export" checkbox).
    pub export: bool,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            name: "Frame 1".into(),
            delay: 7,
            export: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    /// Per-frame visibility (Fireworks keeps one flag per frame); empty = visible in all frames.
    pub frame_visible: Vec<bool>,
    pub locked: bool,
    /// Expanded in the Layers panel.
    pub disclosed: bool,
    /// Same contents in every frame ("Share Across Frames").
    pub shared: bool,
    /// The Web Layer holds slices and hotspots; Fireworks keeps exactly one, on top.
    pub web: bool,
    /// One entry per frame (or a single entry when `shared`).
    pub cells: Vec<Vec<Element>>,
    /// Original Fireworks node, for properties FireCraft does not model yet.
    pub raw: Option<Node>,
}

impl Layer {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            visible: true,
            frame_visible: Vec::new(),
            locked: false,
            disclosed: true,
            shared: false,
            web: false,
            cells: vec![Vec::new()],
            raw: None,
        }
    }

    pub fn web_layer() -> Self {
        Self {
            web: true,
            shared: true,
            ..Self::new("Web Layer")
        }
    }

    /// Shown in `frame`.
    pub fn visible_in(&self, frame: usize) -> bool {
        self.visible && self.frame_visible.get(frame).copied().unwrap_or(true)
    }

    /// Elements shown in `frame` (bottom to top).
    pub fn elements(&self, frame: usize) -> &[Element] {
        let i = if self.shared { 0 } else { frame };
        self.cells
            .get(i)
            .or_else(|| self.cells.first())
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub fn elements_mut(&mut self, frame: usize) -> Option<&mut Vec<Element>> {
        let i = if self.shared { 0 } else { frame };
        if self.cells.len() <= i {
            self.cells.resize_with(i + 1, Vec::new);
        }
        self.cells.get_mut(i)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub width: u32,
    pub height: u32,
    /// Pixels per inch (Fireworks stores the value in the unit chosen by `resolution_cm`).
    pub resolution: f64,
    /// Resolution was entered as pixels/cm.
    pub resolution_cm: bool,
    pub canvas: CanvasColor,
    /// Bottom to top.
    pub layers: Vec<Layer>,
    pub frames: Vec<Frame>,
    pub current_layer: usize,
    pub current_frame: usize,
    /// The whole original Fireworks tree (preset lists, grid, guides, export settings...).
    /// Used as the template when saving so unmodelled data is preserved.
    pub raw: Option<Node>,
    pub raw_secondary: Option<Node>,
    /// Pixel tiles of the original file, by id. Raw nodes kept above (presets, textures, text
    /// caches, `mkBS`) reference them, so they are written back with the document.
    pub raw_tiles: std::sync::Arc<std::collections::HashMap<u32, firecraft_fwpng::Tile>>,
    /// The symbol library (shared: undo snapshots don't copy it).
    pub symbols: std::sync::Arc<Vec<Symbol>>,
    /// Absolute path or display name; `None` for "Untitled".
    pub path: Option<String>,
    /// Fireworks-exact or double-precision colour formulas (a FireCraft extension, saved in a
    /// node Fireworks MX ignores).
    pub color_math: ColorMath,
}

impl Document {
    /// A new document like File > New: a "Layer 1" plus the Web Layer.
    pub fn new(width: u32, height: u32, resolution: f64, canvas: CanvasColor) -> Self {
        Self {
            width,
            height,
            resolution,
            resolution_cm: false,
            canvas,
            layers: vec![Layer::new("Layer 1"), Layer::web_layer()],
            frames: vec![Frame::default()],
            current_layer: 0,
            current_frame: 0,
            raw: None,
            raw_secondary: None,
            raw_tiles: Default::default(),
            symbols: Default::default(),
            path: None,
            color_math: ColorMath::Classic,
        }
    }

    /// A document holding a single bitmap (opening a flat image, as Fireworks does).
    pub fn from_image(img: RgbaImage, name: Option<String>) -> Self {
        let mut doc = Self::new(img.width(), img.height(), 72.0, None);
        doc.layers[0].name = "Background".into();
        if let Some(cell) = doc.layers[0].elements_mut(0) {
            cell.push(Element::new(ElementKind::Bitmap(Bitmap {
                x: 0,
                y: 0,
                image: img,
            })));
        }
        doc.path = name;
        doc
    }

    pub fn title(&self) -> String {
        match &self.path {
            Some(p) => p.rsplit(['/', '\\']).next().unwrap_or(p).to_owned(),
            None => "Untitled-1.png".into(),
        }
    }

    /// Flatten the given frame to an image (canvas colour underneath).
    pub fn render(&self, frame: usize) -> RgbaImage {
        let mut out = RgbaImage::new(self.width, self.height, self.canvas.unwrap_or(Rgba::TRANSPARENT));
        let ctx = RenderCtx {
            symbols: &self.symbols,
            frame,
            depth: 0,
            math: self.color_math,
        };
        for layer in self.layers.iter().filter(|l| l.visible_in(frame)) {
            for el in layer.elements(frame) {
                el.render_onto(&mut out, 0, 0, &ctx);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_document_has_layer_and_web_layer() {
        let d = Document::new(500, 500, 72.0, Some(Rgba::WHITE));
        assert_eq!(d.layers.len(), 2);
        assert!(d.layers[1].web);
        assert_eq!(d.render(0).get(10, 10), Some(Rgba::WHITE));
    }

    #[test]
    fn shared_layer_shows_cell_zero_everywhere() {
        let mut l = Layer::new("a");
        l.shared = true;
        l.cells[0].push(Element::new(ElementKind::Bitmap(Bitmap {
            x: 0,
            y: 0,
            image: RgbaImage::new(1, 1, Rgba::BLACK),
        })));
        assert_eq!(l.elements(5).len(), 1);
    }
}
