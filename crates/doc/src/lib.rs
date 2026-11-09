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

pub mod brush_presets;
pub mod element;
pub mod stroke_fw;

mod cache;
pub mod effects;
pub use cache::{Dirty, RenderCache};
pub use effects::{EffectEntry, EffectParam};
pub use element::{
    BezierPoint, Bitmap, Contour, Element, ElementKind, Fill, MaskMode, Path, PolyLine, PolyPoint, RectShape,
    StoredPolyline, Stroke, Text, Texture, remap_tiles, tile_refs,
};
use firecraft_fwpng::Node;
use firecraft_raster::{ColorMath, Rgba, RgbaImage};

/// Canvas colour: Fireworks offers white, transparent or a custom colour.
pub type CanvasColor = Option<Rgba>;

/// What a symbol is for (Fireworks MX's Symbol Properties: `MSYi`, absent for Graphic).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SymbolKind {
    #[default]
    Graphic,
    /// `MSYi` 1: four frames (Up, Over, Down, Over While Down).
    Button,
    /// `MSYi` 2.
    Animation,
}

impl SymbolKind {
    pub fn label(self) -> &'static str {
        match self {
            SymbolKind::Graphic => "Graphic",
            SymbolKind::Button => "Button",
            SymbolKind::Animation => "Animation",
        }
    }
}

/// A symbol definition (Modify > Symbol): a small document of its own that instances show.
/// Its content is in symbol space, centred on the origin when Fireworks creates it.
#[derive(Clone, Debug, PartialEq)]
pub struct Symbol {
    pub id: u32,
    pub name: String,
    pub kind: SymbolKind,
    pub layers: Vec<Layer>,
    /// Animated symbols have several frames; an instance shows frame (document frame mod count).
    pub frames: usize,
    /// 9-slice scaling guides (Fireworks CS, `NGSv`): left x, right x, top y, bottom y in
    /// symbol space. Scaled instances keep the corners' size, stretch the edges one way and the
    /// middle both ways.
    pub nine: Option<(f64, f64, f64, f64)>,
    /// Original Fireworks node (`MSTv`), for properties FireCraft does not model yet.
    pub raw: Option<Node>,
}

impl Symbol {
    /// The symbol's content bounds in frame 1, in symbol space (what an instance's cached
    /// bounds are mapped from).
    pub fn content_bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let bs: Vec<(f64, f64, f64, f64)> = self
            .layers
            .iter()
            .filter(|l| !l.web)
            .flat_map(|l| l.elements(0).iter().filter_map(Element::bounds))
            .collect();
        let x0 = bs.iter().map(|b| b.0).fold(f64::MAX, f64::min);
        let y0 = bs.iter().map(|b| b.1).fold(f64::MAX, f64::min);
        let x1 = bs.iter().map(|b| b.0 + b.2).fold(f64::MIN, f64::max);
        let y1 = bs.iter().map(|b| b.1 + b.3).fold(f64::MIN, f64::max);
        (!bs.is_empty()).then(|| (x0, y0, x1 - x0, y1 - y0))
    }
}

/// Give instances nested inside symbols their bounds (each symbol's content mapped through the
/// instance's matrix), innermost symbols first: a symbol made only of other symbols' instances
/// then has content bounds too (its instances get a selection outline and transform handles).
pub fn refresh_nested_instance_bounds(symbols: &mut [Symbol]) {
    fn walk(
        els: &mut [Element],
        info: &std::collections::HashMap<u32, Option<(f64, f64, f64, f64)>>,
    ) -> bool {
        let mut changed = false;
        for e in els {
            match &mut e.kind {
                ElementKind::Instance {
                    symbol,
                    matrix,
                    bounds,
                    ..
                } => {
                    let b = info
                        .get(symbol)
                        .copied()
                        .flatten()
                        .map(|(x, y, w, h)| matrix.map_rect(x, y, w, h));
                    if b.is_some() && *bounds != b {
                        *bounds = b;
                        changed = true;
                    }
                }
                ElementKind::Group(children) => changed |= walk(children, info),
                ElementKind::Masked { content, mask, .. } => {
                    changed |= walk(content, info);
                    changed |= walk(std::slice::from_mut(mask.as_mut()), info);
                }
                _ => {}
            }
        }
        changed
    }
    for _ in 0..MAX_SYMBOL_DEPTH {
        let info: std::collections::HashMap<u32, Option<(f64, f64, f64, f64)>> =
            symbols.iter().map(|s| (s.id, s.content_bounds())).collect();
        let mut changed = false;
        for s in symbols.iter_mut() {
            for l in &mut s.layers {
                for cell in &mut l.cells {
                    changed |= walk(cell, &info);
                }
            }
        }
        if !changed {
            break;
        }
    }
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
    /// Draw perspective symbol instances the way Fireworks MX does (see [`Document`]).
    pub mx_perspective: bool,
    /// The symbols being drawn around this point, outermost first (`depth` of them): a symbol
    /// that contains itself, directly or through others, is drawn once, not recursively.
    pub chain: [u32; MAX_SYMBOL_DEPTH as usize],
}

impl RenderCtx<'_> {
    pub const EMPTY: RenderCtx<'static> = RenderCtx {
        symbols: &[],
        frame: 0,
        depth: 0,
        mx_perspective: false,
        math: ColorMath::Classic,
        chain: [0; MAX_SYMBOL_DEPTH as usize],
    };

    /// Inside `el` (an instance: its symbol joins the chain).
    pub fn deeper(&self, el: &Element) -> Self {
        let mut c = *self;
        if let (ElementKind::Instance { symbol, .. }, Some(slot)) =
            (&el.kind, c.chain.get_mut(self.depth as usize))
        {
            *slot = *symbol;
        }
        c.depth = self.depth + 1;
        c
    }

    /// Whether symbol `id` is already being drawn further out.
    pub fn inside_symbol(&self, id: u32) -> bool {
        let n = (self.depth as usize).min(self.chain.len());
        self.chain.get(..n).is_some_and(|c| c.contains(&id))
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
    /// Perspective symbol instances: `false` warps the symbol's pixels (true perspective), `true`
    /// maps its paths and fill handles instead, as Fireworks MX draws them. Per document, like
    /// `color_math`: files from Fireworks open with `true` (docs/behavior/symbols.md).
    pub mx_perspective: bool,
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
            mx_perspective: false,
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
            mx_perspective: self.mx_perspective,
            symbols: &self.symbols,
            frame,
            depth: 0,
            math: self.color_math,
            chain: [0; crate::MAX_SYMBOL_DEPTH as usize],
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
    /// Symbols that contain each other (A holds 8 B's, B holds 8 A's) draw each symbol once
    /// per chain instead of recursing 8^16 times (pasting Symbol 3 into Symbol 2 froze the app).
    #[test]
    fn symbols_containing_each_other_render_quickly() {
        use firecraft_raster::transform::Affine;
        let sym = |id: u32, other: u32| {
            let mut l = Layer::new("Layer 1");
            if let Some(c) = l.elements_mut(0) {
                c.push(Element::new(ElementKind::Bitmap(Bitmap {
                    x: 0,
                    y: 0,
                    image: RgbaImage::new(4, 4, Rgba::BLACK),
                })));
                for k in 0..8 {
                    c.push(Element::new(ElementKind::Instance {
                        symbol: other,
                        name: String::new(),
                        matrix: Affine::translate(5.0 * k as f64, 5.0),
                        bounds: None,
                    }));
                }
            }
            Symbol {
                id,
                name: format!("S{id}"),
                kind: SymbolKind::Graphic,
                layers: vec![l],
                frames: 1,
                nine: None,
                raw: None,
            }
        };
        let mut doc = Document::new(100, 100, 72.0, Some(Rgba::WHITE));
        let mut symbols = vec![sym(1, 2), sym(2, 1)];
        refresh_nested_instance_bounds(&mut symbols);
        doc.symbols = std::sync::Arc::new(symbols);
        if let Some(c) = doc.layers[0].elements_mut(0) {
            c.push(Element::new(ElementKind::Instance {
                symbol: 1,
                name: String::new(),
                matrix: Affine::translate(10.0, 10.0),
                bounds: None,
            }));
        }
        let t = std::time::Instant::now();
        let img = doc.render(0);
        let mut cache = RenderCache::default();
        let cached = doc.render_cached(0, &mut cache);
        assert!(t.elapsed().as_secs() < 10, "took {:?}", t.elapsed());
        // A's bitmap and the B's inside it are drawn.
        assert_eq!(img.get(11, 11).map(|p| p.r), Some(0));
        assert_eq!(img.get(16, 16).map(|p| p.r), Some(0));
        assert_eq!(img, cached);
    }

    /// Fill textures (FirePixel's formulas, checked against Fireworks MX with
    /// fill_texture_probe): opaque lightens toward white where the mask is 0, transparent takes
    /// coverage away there; mask 255 leaves the fill as it is.
    #[test]
    fn fill_texture_modes() {
        let tex = element::Texture::new("t", 2, 1, vec![0, 255]);
        let mut f = Fill {
            color: Rgba::new(200, 0, 0, 255),
            category: "fc_Solid".into(),
            anti_alias: true,
            feather: 0,
            pattern: None,
            dither: None,
            dither_transparent: false,
            gradient: None,
            texture: tex,
            texture_amount: 1000,
            texture_transparent: false,
        };
        let red = Rgba::new(200, 0, 0, 255);
        let mut img = RgbaImage::new(2, 1, red);
        element::apply_fill_texture(&mut img, &f, (0.0, 0.0), (0, 0));
        assert_eq!((img.get(0, 0), img.get(1, 0)), (Some(Rgba::WHITE), Some(red)));
        f.texture_transparent = true;
        let mut img = RgbaImage::new(2, 1, red);
        element::apply_fill_texture(&mut img, &f, (0.0, 0.0), (0, 0));
        assert_eq!((img.get(0, 0).map(|p| p.a), img.get(1, 0)), (Some(0), Some(red)));
    }

    #[test]
    fn symbols_of_symbols_get_bounds() {
        use firecraft_raster::transform::Affine;
        let sym = |id: u32, e: Element| {
            let mut l = Layer::new("Layer 1");
            if let Some(c) = l.elements_mut(0) {
                c.push(e);
            }
            Symbol {
                id,
                name: format!("S{id}"),
                kind: SymbolKind::Graphic,
                layers: vec![l],
                frames: 1,
                nine: None,
                raw: None,
            }
        };
        let inst = |symbol: u32, dx: f64| {
            Element::new(ElementKind::Instance {
                symbol,
                name: String::new(),
                matrix: Affine::translate(dx, 0.0),
                bounds: None,
            })
        };
        let bitmap = Element::new(ElementKind::Bitmap(Bitmap {
            x: 0,
            y: 0,
            image: RgbaImage::new(10, 20, Rgba::BLACK),
        }));
        // 3 holds an instance of 2, which holds an instance of 1 (read in that order).
        let mut symbols = vec![sym(3, inst(2, 100.0)), sym(2, inst(1, 5.0)), sym(1, bitmap)];
        assert_eq!(symbols[0].content_bounds(), None);
        refresh_nested_instance_bounds(&mut symbols);
        assert_eq!(symbols[1].content_bounds(), Some((5.0, 0.0, 10.0, 20.0)));
        assert_eq!(symbols[0].content_bounds(), Some((105.0, 0.0, 10.0, 20.0)));
    }

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
