//! The FireCraft session: open documents, the active tool and its options, selections,
//! undo history and the commands every menu item, shortcut and script goes through.
//!
//! The UI is a thin shell over this crate: it renders state and turns input into calls here.

mod history;
mod objects;
pub mod paths;
mod symbols;
mod tools;

pub use history::{DEFAULT_UNDO_STEPS, History, MAX_UNDO_STEPS};
pub use objects::{Align, Arrange, Clipboard, DUPLICATE_OFFSET, Turn};
pub use paths::PointRef;
pub use symbols::SymbolLink;
pub use tools::{Tool, ToolOptions};

use firecraft_doc::{
    BezierPoint, Bitmap, Contour, Document, Element, ElementKind, Fill, Layer, Path, RectShape, Stroke,
};
use firecraft_raster::mask::Combine;
use firecraft_raster::nine::{self, Insets};
use firecraft_raster::wand::magic_wand;
use firecraft_raster::{Mask, Rgba, RgbaImage};

/// An element address: layer index, element index (in the current frame). Ordered by stacking
/// (bottom first).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ElementRef {
    pub layer: usize,
    pub index: usize,
}

/// A pixel selection over one bitmap element (Fireworks MX selects pixels of a bitmap object).
#[derive(Clone, Debug, PartialEq)]
pub struct PixelSelection {
    pub target: ElementRef,
    /// In the bitmap's own pixel coordinates.
    pub mask: Mask,
}

/// One open document and its editing state.
#[derive(Clone, Debug)]
pub struct DocState {
    /// Unique for the life of the process (UI caches key on it).
    pub id: u64,
    pub doc: Document,
    pub history: History,
    /// Selected objects (Pointer tool).
    pub selection: Vec<ElementRef>,
    pub pixels: Option<PixelSelection>,
    pub zoom: f32,
    pub dirty: bool,
    /// This document is a symbol open in the Symbol Editor.
    pub symbol_link: Option<SymbolLink>,
    /// The region to show when the document first appears (x, y, w, h): the Symbol Editor
    /// shows the symbol's content, not its whole canvas.
    pub view_focus: Option<(f64, f64, f64, f64)>,
    /// Symbol Editor: the symbol's 9-slice guides (left, right, top, bottom; symbol space),
    /// edited here and written back with the content.
    pub symbol_nine: Option<(f64, f64, f64, f64)>,
    /// `dirty` when the live edit in progress began (restored if it's cancelled).
    live_dirty: bool,
    /// Bumped on every change; the UI re-renders the canvas when it moves.
    pub revision: u64,
}

impl DocState {
    pub fn new(doc: Document) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            doc,
            history: History::default(),
            selection: Vec::new(),
            pixels: None,
            // Negative: fit the window on first show (at most 100%).
            zoom: -1.0,
            dirty: false,
            symbol_link: None,
            view_focus: None,
            symbol_nine: None,
            live_dirty: false,
            revision: 0,
        }
    }

    pub fn frame(&self) -> usize {
        self.doc.current_frame
    }

    pub fn element(&self, r: ElementRef) -> Option<&Element> {
        self.doc.layers.get(r.layer)?.elements(self.frame()).get(r.index)
    }

    fn element_mut(&mut self, r: ElementRef) -> Option<&mut Element> {
        let f = self.frame();
        self.doc
            .layers
            .get_mut(r.layer)?
            .elements_mut(f)?
            .get_mut(r.index)
    }

    /// Record an undo step, then apply `f`.
    pub fn edit(&mut self, label: &str, f: impl FnOnce(&mut DocState)) {
        self.history.push(label, &self.doc);
        f(self);
        self.touch();
    }

    /// Record that the document changed outside an undoable edit (the canvas redraws).
    pub fn mark_changed(&mut self) {
        self.touch();
    }

    fn touch(&mut self) {
        self.dirty = true;
        self.revision = self.revision.wrapping_add(1);
    }

    pub fn undo(&mut self) {
        if let Some(d) = self.history.undo(&self.doc) {
            self.doc = d;
            self.after_history();
        }
    }

    pub fn redo(&mut self) {
        if let Some(d) = self.history.redo(&self.doc) {
            self.doc = d;
            self.after_history();
        }
    }

    fn after_history(&mut self) {
        self.selection.retain(|r| {
            self.doc
                .layers
                .get(r.layer)
                .is_some_and(|l| r.index < l.elements(self.doc.current_frame).len())
        });
        self.pixels = None;
        self.touch();
    }

    /// Topmost visible element containing canvas point (`x`, `y`), searching layers top-down.
    pub fn hit_test(&self, x: f64, y: f64) -> Option<ElementRef> {
        self.hits_at(x, y).into_iter().next()
    }

    /// Every visible object drawn at canvas point (`x`, `y`), topmost first. Objects are hit on
    /// their own pixels (within 2 px for thin strokes), not their bounding boxes, as in
    /// Fireworks; hidden objects and locked or hidden layers are skipped.
    pub fn hits_at(&self, x: f64, y: f64) -> Vec<ElementRef> {
        const TOL: i64 = 2;
        let (px, py) = (x.floor() as i64, y.floor() as i64);
        let ctx = firecraft_doc::RenderCtx {
            symbols: &self.doc.symbols,
            frame: self.frame(),
            depth: 0,
            math: self.doc.color_math,
        };
        let mut out = Vec::new();
        for (li, layer) in self.doc.layers.iter().enumerate().rev() {
            if !layer.visible_in(self.frame()) || layer.locked {
                continue;
            }
            for (ei, el) in layer.elements(self.frame()).iter().enumerate().rev() {
                // Hidden and locked objects can't be picked.
                if !el.visible || el.locked {
                    continue;
                }
                let near = el.deep_bounds(&ctx).is_some_and(|(bx, by, bw, bh)| {
                    let t = TOL as f64;
                    x >= bx - t && y >= by - t && x < bx + bw.max(1.0) + t && y < by + bh.max(1.0) + t
                });
                if !near {
                    continue;
                }
                let hit = match &el.kind {
                    // Bitmaps: the pixel under the pointer.
                    ElementKind::Bitmap(b) => b.image.get(px - b.x, py - b.y).is_some_and(|p| p.a > 0),
                    // Everything else: render the object alone around the point.
                    _ => {
                        let n = (2 * TOL + 1) as u32;
                        let mut probe = firecraft_raster::RgbaImage::new(n, n, Rgba::TRANSPARENT);
                        let mut solo = el.clone();
                        solo.opacity = 100.0;
                        solo.blend = firecraft_raster::BlendMode::Normal;
                        solo.render_onto(&mut probe, px - TOL, py - TOL, &ctx);
                        probe.pixels().iter().any(|p| p.a > 0)
                    }
                };
                if hit {
                    out.push(ElementRef { layer: li, index: ei });
                }
            }
        }
        out
    }

    /// Select Behind: select the object below the currently selected one at this point (the
    /// topmost if the selection isn't under the point), wrapping around.
    pub fn select_behind(&mut self, x: f64, y: f64) {
        let hits = self.hits_at(x, y);
        let next = match self.selection.as_slice() {
            [one] => hits
                .iter()
                .position(|h| h == one)
                .map_or(0, |k| (k + 1) % hits.len().max(1)),
            _ => 0,
        };
        match hits.get(next) {
            Some(&h) => {
                self.selection = vec![h];
                self.pixels = None;
            }
            None => self.deselect(),
        }
    }

    /// Whether canvas point (`x`, `y`) is inside the selection's bounding box (pressing there
    /// with the Pointer drags the selection, even where another object lies on top).
    pub fn selection_contains(&self, x: f64, y: f64) -> bool {
        self.selection_bounds()
            .is_some_and(|(bx, by, bw, bh)| x >= bx && y >= by && x < bx + bw && y < by + bh)
    }

    /// The bitmap a pixel tool acts on at canvas point: the selected bitmap if any, else the
    /// topmost bitmap under the point.
    pub fn pixel_target(&self, x: f64, y: f64) -> Option<ElementRef> {
        let is_bitmap =
            |r: &ElementRef| matches!(self.element(*r).map(|e| &e.kind), Some(ElementKind::Bitmap(_)));
        if let Some(r) = self.selection.iter().find(|r| is_bitmap(r)) {
            return Some(*r);
        }
        self.hit_test(x, y).filter(is_bitmap)
    }

    // ---- Commands ------------------------------------------------------------------------

    pub fn select_all(&mut self) {
        let f = self.frame();
        if let Some(r) = self.pixels.as_ref().map(|p| p.target) {
            if let Some(ElementKind::Bitmap(b)) = self.element(r).map(|e| &e.kind) {
                let mut m = Mask::new(b.image.width(), b.image.height());
                m.data_mut().iter_mut().for_each(|v| *v = 255);
                self.pixels = Some(PixelSelection { target: r, mask: m });
                return;
            }
        }
        self.selection = self
            .doc
            .layers
            .iter()
            .enumerate()
            .filter(|(_, l)| l.visible && !l.locked && !l.web)
            .flat_map(|(li, l)| {
                l.elements(f)
                    .iter()
                    .enumerate()
                    .filter(|(_, e)| e.visible && !e.locked)
                    .map(move |(ei, _)| ElementRef { layer: li, index: ei })
            })
            .collect();
    }

    /// Show/hide an object (Layers panel eye). One undo step.
    pub fn toggle_object_visible(&mut self, r: ElementRef) {
        let Some(on) = self.element(r).map(|e| !e.visible) else {
            return;
        };
        self.edit(if on { "Show Object" } else { "Hide Object" }, |s| {
            if let Some(e) = s.element_mut(r) {
                e.visible = on;
            }
            if !on {
                s.selection.retain(|x| *x != r);
            }
        });
    }

    /// Lock/unlock an object (Fireworks CS's per-object lock; Layers panel). A locked object
    /// can't be selected, so locking deselects it. One undo step.
    pub fn toggle_object_locked(&mut self, r: ElementRef) {
        let Some(on) = self.element(r).map(|e| !e.locked) else {
            return;
        };
        self.edit(if on { "Lock Object" } else { "Unlock Object" }, |s| {
            if let Some(e) = s.element_mut(r) {
                e.locked = on;
            }
            if on {
                s.selection.retain(|x| *x != r);
            }
        });
    }

    /// Select > Select Inverse (pixel selections).
    pub fn select_inverse(&mut self) {
        if let Some(p) = &mut self.pixels {
            p.mask.invert();
        }
    }

    /// Select > Feather.
    pub fn feather_pixels(&mut self, radius: u32) {
        if let Some(p) = &mut self.pixels {
            p.mask.feather(radius);
        }
    }

    pub fn deselect(&mut self) {
        self.selection.clear();
        self.pixels = None;
    }

    /// Magic wand click at canvas point.
    pub fn magic_wand(&mut self, x: f64, y: f64, opts: &ToolOptions, combine: Combine) {
        let Some(target) = self.pixel_target(x, y) else {
            return;
        };
        let Some(ElementKind::Bitmap(b)) = self.element(target).map(|e| &e.kind) else {
            return;
        };
        let (px, py) = ((x - b.x as f64).floor() as i64, (y - b.y as f64).floor() as i64);
        let mask = magic_wand(&b.image, px, py, opts.wand());
        self.combine_pixels(target, mask, combine);
    }

    /// Rectangular or elliptical marquee in canvas coordinates.
    pub fn marquee(&mut self, rect: (f64, f64, f64, f64), oval: bool, opts: &ToolOptions, combine: Combine) {
        let (x0, y0, x1, y1) = (
            rect.0.min(rect.2),
            rect.1.min(rect.3),
            rect.0.max(rect.2),
            rect.1.max(rect.3),
        );
        let Some(target) = self
            .pixel_target((x0 + x1) / 2.0, (y0 + y1) / 2.0)
            .or_else(|| self.pixel_target(x0, y0))
        else {
            return;
        };
        let Some(ElementKind::Bitmap(b)) = self.element(target).map(|e| &e.kind) else {
            return;
        };
        let mut mask = Mask::new(b.image.width(), b.image.height());
        let (ox, oy) = (b.x as f64, b.y as f64);
        let (cx, cy, rx, ry) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0) / 2.0, (y1 - y0) / 2.0);
        let aa = opts.marquee_edge != firecraft_raster::Edge::Hard;
        let ss = if aa { 4 } else { 1 };
        for py in 0..mask.height() as i64 {
            for px in 0..mask.width() as i64 {
                let mut n = 0;
                for sy in 0..ss {
                    for sx in 0..ss {
                        let x = ox + px as f64 + (sx as f64 + 0.5) / ss as f64;
                        let y = oy + py as f64 + (sy as f64 + 0.5) / ss as f64;
                        let inside = if oval {
                            rx > 0.0 && ry > 0.0 && ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) <= 1.0
                        } else {
                            x >= x0 && x < x1 && y >= y0 && y < y1
                        };
                        n += i32::from(inside);
                    }
                }
                mask.set(px, py, (n * 255 / (ss * ss)) as u8);
            }
        }
        if let firecraft_raster::Edge::Feather(r) = opts.marquee_edge {
            mask.feather(r);
        }
        self.combine_pixels(target, mask, combine);
    }

    fn combine_pixels(&mut self, target: ElementRef, mask: Mask, combine: Combine) {
        match &mut self.pixels {
            Some(sel) if sel.target == target && combine != Combine::Replace => {
                sel.mask.combine(&mask, combine)
            }
            _ => self.pixels = Some(PixelSelection { target, mask }),
        }
        if self.pixels.as_ref().is_some_and(|p| p.mask.is_empty()) {
            self.pixels = None;
        }
        self.selection = vec![target];
    }

    /// Edit > Clear / Delete key: remove selected pixels, else delete selected objects.
    pub fn delete(&mut self) {
        self.delete_as("Delete");
    }

    /// [`delete`](Self::delete) recorded as `label` ("Delete", "Cut").
    pub(crate) fn delete_as(&mut self, label: &str) {
        if let Some(sel) = self.pixels.clone() {
            let pixel_label = if label == "Delete" { "Delete Pixels" } else { label };
            self.edit(pixel_label, |s| {
                if let Some(Element {
                    kind: ElementKind::Bitmap(b),
                    ..
                }) = s.element_mut(sel.target)
                {
                    b.image.clear_masked(&sel.mask);
                }
            });
            return;
        }
        if self.selection.is_empty() {
            return;
        }
        let mut refs = self.selection.clone();
        refs.sort_by(|a, b| (b.layer, b.index).cmp(&(a.layer, a.index)));
        self.edit(label, |s| {
            let f = s.frame();
            for r in refs {
                if let Some(cell) = s.doc.layers.get_mut(r.layer).and_then(|l| l.elements_mut(f)) {
                    if r.index < cell.len() {
                        cell.remove(r.index);
                    }
                }
            }
            s.selection.clear();
        });
    }

    /// Edit > Insert > Bitmap via Cut (`cut` = true) or Bitmap via Copy: the selected pixels
    /// become a new bitmap object above the source.
    pub fn bitmap_via(&mut self, cut: bool) {
        let Some(sel) = self.pixels.clone() else { return };
        let Some(Element {
            kind: ElementKind::Bitmap(src),
            ..
        }) = self.element(sel.target)
        else {
            return;
        };
        let Some((bx, by, bw, bh)) = sel.mask.bounds() else {
            return;
        };
        let full = src.image.extract_masked(&sel.mask);
        let mut piece = RgbaImage::new(bw, bh, Rgba::TRANSPARENT);
        piece.draw(&full, -(bx as i64), -(by as i64), 255);
        let new = Element::new(ElementKind::Bitmap(Bitmap {
            x: src.x + bx as i64,
            y: src.y + by as i64,
            image: piece,
        }));
        let label = if cut { "Bitmap via Cut" } else { "Bitmap via Copy" };
        self.edit(label, |s| {
            if cut {
                if let Some(Element {
                    kind: ElementKind::Bitmap(b),
                    ..
                }) = s.element_mut(sel.target)
                {
                    b.image.clear_masked(&sel.mask);
                }
            }
            let f = s.frame();
            if let Some(cell) = s
                .doc
                .layers
                .get_mut(sel.target.layer)
                .and_then(|l| l.elements_mut(f))
            {
                let at = (sel.target.index + 1).min(cell.len());
                cell.insert(at, new);
                s.selection = vec![ElementRef {
                    layer: sel.target.layer,
                    index: at,
                }];
            }
            s.pixels = None;
        });
    }

    /// Move the selected objects.
    pub fn move_selection(&mut self, dx: f64, dy: f64, label: &str) {
        if self.selection.is_empty() || (dx == 0.0 && dy == 0.0) {
            return;
        }
        let refs = self.selection.clone();
        self.edit(label, |s| {
            for r in refs {
                if let Some(e) = s.element_mut(r) {
                    e.translate(dx, dy);
                }
            }
        });
    }

    /// Move the selected objects as part of a live edit (no undo step of its own; see
    /// [`begin_live_edit`](Self::begin_live_edit)).
    pub fn move_selection_live(&mut self, dx: f64, dy: f64) {
        if self.selection.is_empty() || (dx == 0.0 && dy == 0.0) {
            return;
        }
        for r in self.selection.clone() {
            if let Some(e) = self.element_mut(r) {
                e.translate(dx, dy);
            }
        }
        self.touch();
    }

    /// [`transform_from`](Self::transform_from) as part of a live edit (no undo step of its own).
    pub fn transform_from_live(
        &mut self,
        originals: &[(ElementRef, Element)],
        m: &firecraft_raster::transform::Affine,
    ) {
        for (r, orig) in originals {
            if let Some(e) = self.element_mut(*r) {
                let mut t = orig.clone();
                t.transform(m);
                *e = t;
            }
        }
        self.touch();
    }

    /// How `objects` look now (effects included, before opacity and blending), for previewing
    /// a transform cheaply: each object's pixels and their canvas position.
    pub fn bake(&self, objects: &[(ElementRef, Element)]) -> Vec<(ElementRef, Element, RgbaImage, i64, i64)> {
        let ctx = firecraft_doc::RenderCtx {
            symbols: &self.doc.symbols,
            frame: self.doc.current_frame,
            depth: 0,
            math: self.doc.color_math,
        };
        objects
            .iter()
            .filter_map(|(r, e)| {
                let (rx, ry, rw, rh) = e.full_region(&ctx)?;
                let (img, x, y) = e.pixels(rx, ry, rw, rh, &ctx)?;
                Some((*r, e.clone(), img, rx + x, ry + y))
            })
            .collect()
    }

    /// Show `baked` objects (from [`bake`](Self::bake)) transformed by `m`, as bitmaps of their
    /// look, during a live edit: a fast preview of Scale/Skew/Distort/rotate (the real
    /// transform replaces it when the drag ends).
    pub fn preview_transform(
        &mut self,
        baked: &[(ElementRef, Element, RgbaImage, i64, i64)],
        m: &firecraft_raster::transform::Affine,
    ) {
        for (r, orig, img, x, y) in baked {
            let Some((img, x, y)) = firecraft_raster::transform::transform_image(img, *x, *y, m) else {
                continue;
            };
            let mut e = Element::new(firecraft_doc::ElementKind::Bitmap(firecraft_doc::Bitmap {
                x,
                y,
                image: img,
            }));
            e.name = orig.name.clone();
            e.visible = orig.visible;
            e.opacity = orig.opacity;
            e.blend = orig.blend;
            if let Some(slot) = self.element_mut(*r) {
                *slot = e;
            }
        }
        self.touch();
    }

    /// Scale/Skew/Distort/rotate: replace the selected objects with `originals` (copies taken
    /// when the transform began) transformed by `m`, so repeated adjustments resample once.
    pub fn transform_from(
        &mut self,
        originals: &[(ElementRef, Element)],
        m: &firecraft_raster::transform::Affine,
        label: &str,
    ) {
        if originals.is_empty() {
            return;
        }
        let items = originals.to_vec();
        let m = *m;
        self.edit(label, |s| {
            for (r, orig) in items {
                if let Some(e) = s.element_mut(r) {
                    let mut t = orig;
                    t.transform(&m);
                    *e = t;
                }
            }
        });
    }

    /// The first selected object's Live Effects (what the Property inspector lists).
    pub fn selected_effects(&self) -> Option<Vec<firecraft_doc::EffectEntry>> {
        self.element(*self.selection.first()?).map(|e| e.effects.clone())
    }

    /// Replace the selected objects' effect lists (one undo step), as Fireworks' Property
    /// inspector does for every selected object.
    pub fn set_effects(&mut self, list: Vec<firecraft_doc::EffectEntry>, label: &str) {
        if self.selection.is_empty() {
            return;
        }
        let refs = self.selection.clone();
        self.edit(label, |s| {
            for r in refs {
                if let Some(e) = s.element_mut(r) {
                    e.effects = list.clone();
                }
            }
        });
    }

    /// Start a live edit (an effect's settings popup): records one undo step now; later
    /// [`set_effects_live`](Self::set_effects_live) calls change the document without more.
    pub fn begin_live_edit(&mut self, label: &str) {
        self.history.push(label, &self.doc);
        self.live_dirty = self.dirty;
    }

    /// Cancel the live edit in progress: the document returns to how it was when it began, with
    /// no undo step left behind.
    pub fn cancel_live_edit(&mut self) {
        if let Some(doc) = self.history.take_last() {
            self.doc = doc;
            self.after_history();
            self.dirty = self.live_dirty;
        }
    }

    /// End a live edit that changed nothing: remove its undo step.
    pub fn discard_live_edit(&mut self) {
        self.history.discard_last();
    }

    pub fn set_effects_live(&mut self, list: Vec<firecraft_doc::EffectEntry>) {
        let refs = self.selection.clone();
        for r in refs {
            if let Some(e) = self.element_mut(r) {
                e.effects = list.clone();
            }
        }
        self.touch();
    }

    /// Copies of the selected objects (for [`transform_from`](Self::transform_from)).
    pub fn selected_elements(&self) -> Vec<(ElementRef, Element)> {
        self.selection
            .iter()
            .filter_map(|r| Some((*r, self.element(*r)?.clone())))
            .collect()
    }

    /// Add a rectangle or ellipse path from a drag rectangle, using the tool's fill and stroke.
    pub fn add_shape(&mut self, rect: (f64, f64, f64, f64), ellipse: bool, opts: &ToolOptions) {
        let (x0, y0, x1, y1) = (
            rect.0.min(rect.2),
            rect.1.min(rect.3),
            rect.0.max(rect.2),
            rect.1.max(rect.3),
        );
        if x1 - x0 < 1.0 || y1 - y0 < 1.0 {
            return;
        }
        // Rectangles are Fireworks rectangle primitives (with roundness).
        let rect = (!ellipse).then(|| RectShape::new(x0, y0, x1, y1, opts.roundness));
        let points = if let Some(r) = &rect {
            r.contour().points
        } else {
            // Four-point bezier ellipse (kappa = 0.5523).
            let (cx, cy, rx, ry) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0) / 2.0, (y1 - y0) / 2.0);
            let k = 0.552_284_75;
            vec![
                BezierPoint {
                    x: cx,
                    y: y0,
                    pred: (-rx * k, 0.0),
                    succ: (rx * k, 0.0),
                },
                BezierPoint {
                    x: x1,
                    y: cy,
                    pred: (0.0, -ry * k),
                    succ: (0.0, ry * k),
                },
                BezierPoint {
                    x: cx,
                    y: y1,
                    pred: (rx * k, 0.0),
                    succ: (-rx * k, 0.0),
                },
                BezierPoint {
                    x: x0,
                    y: cy,
                    pred: (0.0, ry * k),
                    succ: (0.0, -ry * k),
                },
            ]
        };
        let path = Path {
            rect,
            contours: vec![Contour { points, closed: true }],
            fill: opts.fill.map(|color| Fill {
                color,
                category: "fc_Solid".into(),
                anti_alias: true,
                gradient: None,
            }),
            stroke: opts.stroke.map(|color| Stroke {
                color,
                category: "bc_Pencil".into(),
                name: "bn_1-Pixel Anti-Aliased".into(),
                diameter: opts.stroke_size as f64,
                softness: 0,
            }),
        };
        self.add_element(
            Element::new(ElementKind::Path(path)),
            if ellipse { "Ellipse" } else { "Rectangle" },
        );
    }

    /// Add a straight line path.
    pub fn add_line(&mut self, a: (f64, f64), b: (f64, f64), opts: &ToolOptions) {
        let path = Path {
            contours: vec![Contour {
                points: vec![BezierPoint::corner(a.0, a.1), BezierPoint::corner(b.0, b.1)],
                closed: false,
            }],
            fill: None,
            stroke: Some(Stroke {
                color: opts.stroke.unwrap_or(Rgba::BLACK),
                category: "bc_Pencil".into(),
                name: "bn_1-Pixel Anti-Aliased".into(),
                diameter: opts.stroke_size as f64,
                softness: 0,
            }),
            rect: None,
        };
        self.add_element(Element::new(ElementKind::Path(path)), "Line");
    }

    /// Insert above the current selection in the current layer.
    pub fn add_element(&mut self, el: Element, label: &str) {
        let layer = self.writable_layer();
        self.edit(label, |s| {
            let f = s.frame();
            if let Some(cell) = s.doc.layers.get_mut(layer).and_then(|l| l.elements_mut(f)) {
                cell.push(el);
                s.selection = vec![ElementRef {
                    layer,
                    index: cell.len() - 1,
                }];
            }
            s.pixels = None;
        });
    }

    /// The current layer, or the first editable normal layer.
    fn writable_layer(&mut self) -> usize {
        let cur = self.doc.current_layer;
        if self.doc.layers.get(cur).is_some_and(|l| !l.web && !l.locked) {
            return cur;
        }
        if let Some(i) = self.doc.layers.iter().position(|l| !l.web && !l.locked) {
            return i;
        }
        self.doc.layers.insert(0, Layer::new("Layer 1"));
        0
    }

    pub fn new_layer(&mut self) {
        let n = self.doc.layers.iter().filter(|l| !l.web).count() + 1;
        let at = self
            .doc
            .layers
            .iter()
            .position(|l| l.web)
            .unwrap_or(self.doc.layers.len())
            .min(self.doc.current_layer + 1);
        let frames = self.doc.frames.len();
        self.edit("New Layer", |s| {
            let mut l = Layer::new(format!("Layer {n}"));
            l.cells = vec![Vec::new(); frames.max(1)];
            s.doc.layers.insert(at, l);
            s.doc.current_layer = at;
        });
    }

    pub fn delete_layer(&mut self, i: usize) {
        if self.doc.layers.get(i).is_none_or(|l| l.web)
            || self.doc.layers.iter().filter(|l| !l.web).count() <= 1
        {
            return;
        }
        self.edit("Delete Layer", |s| {
            s.doc.layers.remove(i);
            s.doc.current_layer = s.doc.current_layer.min(s.doc.layers.len().saturating_sub(1));
            s.selection.clear();
        });
    }

    /// Union of the selected objects' bounds (x, y, w, h).
    pub fn selection_bounds(&self) -> Option<(f64, f64, f64, f64)> {
        let bs: Vec<_> = self
            .selection
            .iter()
            .filter_map(|r| self.element(*r)?.bounds())
            .collect();
        if bs.is_empty() {
            return None;
        }
        let x0 = bs.iter().map(|b| b.0).fold(f64::MAX, f64::min);
        let y0 = bs.iter().map(|b| b.1).fold(f64::MAX, f64::min);
        let x1 = bs.iter().map(|b| b.0 + b.2).fold(f64::MIN, f64::max);
        let y1 = bs.iter().map(|b| b.1 + b.3).fold(f64::MIN, f64::max);
        Some((x0, y0, x1 - x0, y1 - y0))
    }

    /// 9-slice Scaling tool (Fireworks CS5 behaviour, destructive): scale the selection from its
    /// bounds to `to` keeping the four guide insets fixed. The result is ordinary objects, so the
    /// document stays Fireworks MX compatible.
    pub fn nine_slice_scale(&mut self, to: (f64, f64, f64, f64), insets: Insets) {
        let Some(from) = self.selection_bounds() else {
            return;
        };
        if to.2 < 1.0 || to.3 < 1.0 || to.2 > 32_000.0 || to.3 > 32_000.0 {
            return;
        }
        let (mx, my) = nine::maps(from, to, insets);
        let refs = self.selection.clone();
        self.edit("9-slice Scale", |s| {
            for r in refs {
                if let Some(e) = s.element_mut(r) {
                    e.map_nine(&mx, &my);
                }
            }
        });
    }

    /// Set opacity (0..=100) on selected objects.
    pub fn set_opacity(&mut self, opacity: f32) {
        let refs = self.selection.clone();
        self.edit("Opacity", |s| {
            for r in refs {
                if let Some(e) = s.element_mut(r) {
                    e.opacity = opacity.clamp(0.0, 100.0);
                }
            }
        });
    }

    /// Property inspector "Rectangle roundness" (0..=1) of the selected rectangles. `live`: part
    /// of a drag begun with `begin_live_edit` (no undo step of its own).
    pub fn set_roundness(&mut self, roundness: f64, live: bool) {
        let apply = |s: &mut DocState| {
            for r in s.selection.clone() {
                if let Some(Element {
                    kind: ElementKind::Path(p),
                    ..
                }) = s.element_mut(r)
                {
                    p.set_roundness(roundness);
                }
            }
        };
        if live {
            apply(self);
            self.touch();
        } else {
            self.edit("Rectangle Roundness", apply);
        }
    }

    /// Set the blend mode of the selected objects.
    pub fn set_blend(&mut self, mode: firecraft_raster::BlendMode) {
        let refs = self.selection.clone();
        self.edit("Blend Mode", |s| {
            for r in refs {
                if let Some(e) = s.element_mut(r) {
                    e.blend = mode;
                }
            }
        });
    }

    /// Modify > Canvas > Canvas Color.
    pub fn set_canvas(&mut self, canvas: Option<Rgba>) {
        self.edit("Canvas Color", |s| s.doc.canvas = canvas);
    }

    /// Fireworks-exact or high-precision colour formulas for this document.
    pub fn set_color_math(&mut self, math: firecraft_raster::ColorMath) {
        if self.doc.color_math != math {
            self.edit("Color Math", |s| s.doc.color_math = math);
        }
    }

    /// Modify > Canvas > Canvas Size (anchored top-left).
    pub fn set_canvas_size(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 || w > 32_000 || h > 32_000 {
            return;
        }
        self.edit("Canvas Size", |s| {
            s.doc.width = w;
            s.doc.height = h;
        });
    }
}

/// The whole application state independent of the UI.
#[derive(Debug, Default)]
pub struct Session {
    pub docs: Vec<DocState>,
    pub active: usize,
    pub tool: Tool,
    pub options: ToolOptions,
    pub untitled: u32,
}

impl Session {
    pub fn active_doc(&self) -> Option<&DocState> {
        self.docs.get(self.active)
    }
    pub fn active_doc_mut(&mut self) -> Option<&mut DocState> {
        self.docs.get_mut(self.active)
    }

    pub fn new_document(&mut self, w: u32, h: u32, res: f64, canvas: Option<Rgba>) {
        self.untitled += 1;
        let mut d = Document::new(w, h, res, canvas);
        d.path = Some(format!("Untitled-{}.png", self.untitled));
        self.push(d);
    }

    pub fn push(&mut self, doc: Document) {
        self.docs.push(DocState::new(doc));
        self.active = self.docs.len() - 1;
    }

    pub fn open_bytes(&mut self, bytes: &[u8], name: String) -> Result<(), firecraft_io::Error> {
        let doc = firecraft_io::open(bytes, Some(name))?;
        self.push(doc);
        Ok(())
    }

    pub fn close(&mut self, i: usize) {
        if i < self.docs.len() {
            self.docs.remove(i);
            self.active = self.active.min(self.docs.len().saturating_sub(1));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelling_a_live_edit_restores_the_document_without_an_undo_step() {
        let mut d = DocState::new(Document::new(50, 50, 72.0, Some(firecraft_raster::Rgba::WHITE)));
        d.add_shape((5.0, 5.0, 20.0, 20.0), false, &ToolOptions::default());
        let (before, steps) = (d.doc.clone(), d.history.labels().count());
        d.dirty = false;
        d.begin_live_edit("Add Effect");
        let fx = firecraft_doc::EffectEntry::from_pairs(&[
            ("EffectIsVisible", "true"),
            ("EffectMoaID", "{d2541291-70d6-11d1-8c8000a024cdc039}"),
        ]);
        d.set_effects_live(vec![fx]);
        assert_ne!(d.doc, before);
        d.cancel_live_edit();
        assert_eq!(d.doc, before);
        assert_eq!(d.history.labels().count(), steps);
        assert!(!d.dirty && !d.history.can_redo());
    }

    fn doc_with_disc() -> DocState {
        let mut img = RgbaImage::new(100, 100, Rgba::WHITE);
        for y in 0..100 {
            for x in 0..100 {
                if (x as f64 - 49.5).powi(2) + (y as f64 - 49.5).powi(2) < 900.0 {
                    img.set(x, y, Rgba::new(255, 0, 0, 255));
                }
            }
        }
        DocState::new(Document::from_image(img, None))
    }

    #[test]
    fn wand_delete_undo() {
        let mut d = doc_with_disc();
        let opts = ToolOptions::default();
        d.magic_wand(50.0, 50.0, &opts, Combine::Replace);
        assert!(d.pixels.is_some());
        d.delete();
        assert_eq!(d.doc.render(0).get(50, 50).map(|p| p.a), Some(0));
        d.undo();
        assert_eq!(d.doc.render(0).get(50, 50), Some(Rgba::new(255, 0, 0, 255)));
        d.redo();
        assert_eq!(d.doc.render(0).get(50, 50).map(|p| p.a), Some(0));
    }

    #[test]
    fn bitmap_via_cut_makes_new_object() {
        let mut d = doc_with_disc();
        d.magic_wand(50.0, 50.0, &ToolOptions::default(), Combine::Replace);
        d.bitmap_via(true);
        let els = d.doc.layers[0].elements(0);
        assert_eq!(els.len(), 2);
        let ElementKind::Bitmap(b) = &els[1].kind else {
            panic!()
        };
        assert_eq!((b.x, b.y), (19, 19));
        // The composite is unchanged by a cut-to-new-object.
        assert_eq!(d.doc.render(0).get(50, 50), Some(Rgba::new(255, 0, 0, 255)));
    }

    #[test]
    fn nine_slice_keeps_rounded_corners() {
        let mut d = DocState::new(Document::new(300, 200, 72.0, Some(Rgba::WHITE)));
        d.add_shape((10.0, 10.0, 70.0, 40.0), true, &ToolOptions::default());
        let before = d.selection_bounds().unwrap();
        d.nine_slice_scale(
            (10.0, 10.0, 200.0, 30.0),
            Insets {
                left: 20.0,
                right: 20.0,
                top: 10.0,
                bottom: 10.0,
            },
        );
        let after = d.selection_bounds().unwrap();
        assert!(
            (after.2 - 200.0).abs() < 1.0 && (after.3 - 30.0).abs() < 1.0,
            "{before:?} -> {after:?}"
        );
        d.undo();
        assert_eq!(d.selection_bounds(), Some(before));
    }

    #[test]
    fn history_limit_and_navigation() {
        let mut d = DocState::new(Document::new(100, 100, 72.0, Some(Rgba::WHITE)));
        d.history.set_limit(10);
        for i in 0..15 {
            d.add_shape(
                (i as f64, 0.0, i as f64 + 5.0, 5.0),
                false,
                &ToolOptions::default(),
            );
        }
        assert_eq!(d.history.labels().count(), 10);
        let count = |d: &DocState| d.doc.layers[0].elements(0).len();
        assert_eq!(count(&d), 15);
        for _ in 0..4 {
            d.undo();
        }
        assert_eq!(count(&d), 11);
        assert_eq!(d.history.redo_labels().count(), 4);
        d.redo();
        d.redo();
        assert_eq!(count(&d), 13);
        assert_eq!(d.history.labels().count() + d.history.redo_labels().count(), 10);
    }

    #[test]
    fn shapes_and_selection() {
        let mut d = DocState::new(Document::new(200, 200, 72.0, Some(Rgba::WHITE)));
        d.add_shape((10.0, 10.0, 60.0, 40.0), false, &ToolOptions::default());
        assert_eq!(d.hit_test(30.0, 20.0), Some(ElementRef { layer: 0, index: 0 }));
        d.move_selection(100.0, 0.0, "Move");
        assert_eq!(d.hit_test(30.0, 20.0), None);
        assert!(d.hit_test(130.0, 20.0).is_some());
        d.delete();
        assert!(d.doc.layers[0].elements(0).is_empty());
    }

    /// Two overlapping objects: a canvas-sized bitmap that is transparent except a corner, over
    /// a small opaque square. Clicks hit pixels, not bounding boxes; Select Behind cycles.
    #[test]
    fn hits_by_pixels_and_select_behind_cycles() {
        let mut doc = Document::new(100, 100, 72.0, Some(Rgba::WHITE));
        let mut square = RgbaImage::new(20, 20, Rgba::new(255, 0, 0, 255));
        square.set(0, 0, Rgba::new(255, 0, 0, 255));
        let mut overlay = RgbaImage::new(100, 100, Rgba::TRANSPARENT);
        overlay.set(5, 5, Rgba::new(0, 0, 255, 255));
        for (img, x, y) in [(square, 40, 40), (overlay, 0, 0)] {
            doc.layers[0]
                .elements_mut(0)
                .unwrap()
                .push(Element::new(ElementKind::Bitmap(firecraft_doc::Bitmap {
                    x,
                    y,
                    image: img,
                })));
        }
        let mut d = DocState::new(doc);
        // Inside the overlay's box but on its transparent part: the square underneath.
        assert_eq!(d.hit_test(45.0, 45.0), Some(ElementRef { layer: 0, index: 0 }));
        assert_eq!(d.hit_test(5.0, 5.0), Some(ElementRef { layer: 0, index: 1 }));
        assert_eq!(d.hit_test(80.0, 80.0), None);
        // A hidden object is never hit.
        d.doc.layers[0].elements_mut(0).unwrap()[0].visible = false;
        assert_eq!(d.hit_test(45.0, 45.0), None);
        d.doc.layers[0].elements_mut(0).unwrap()[0].visible = true;
        // Make the overlay cover the square's spot too, then cycle with Select Behind.
        if let ElementKind::Bitmap(b) = &mut d.doc.layers[0].elements_mut(0).unwrap()[1].kind {
            b.image.set(45, 45, Rgba::new(0, 0, 255, 255));
        }
        assert_eq!(d.hits_at(45.0, 45.0).len(), 2);
        d.select_behind(45.0, 45.0);
        assert_eq!(d.selection, vec![ElementRef { layer: 0, index: 1 }]);
        d.select_behind(45.0, 45.0);
        assert_eq!(d.selection, vec![ElementRef { layer: 0, index: 0 }]);
        d.select_behind(45.0, 45.0);
        assert_eq!(d.selection, vec![ElementRef { layer: 0, index: 1 }]);
        assert!(d.selection_contains(50.0, 50.0));
    }
}
