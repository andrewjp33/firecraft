//! The FireCraft session: open documents, the active tool and its options, selections,
//! undo history and the commands every menu item, shortcut and script goes through.
//!
//! The UI is a thin shell over this crate: it renders state and turns input into calls here.

pub mod align_panel;
mod canvas;
pub use canvas::Anchor;
mod crop;
pub use crop::CropRect;
mod fills;
mod filters;
pub use filters::filter_image;
mod animation;
mod history;
mod masks;
pub use masks::{MaskPaste, MaskRemoval};
mod objects;
mod paint;
pub mod paths;
mod symbols;
pub mod text;
mod tools;

pub use fills::{
    FillHandles, GRADIENT_FILLS, GRADIENT_PRESETS, PATTERN, WEB_DITHER, default_handles, fill_label,
    new_gradient, new_pattern, opaque_ramp, preset_colors, preset_name, web_dither_tile,
};
pub use animation::{AnimateSettings, PathPoint};
pub use history::{DEFAULT_UNDO_STEPS, History, MAX_UNDO_STEPS, Selected};
pub use objects::{Align, Arrange, Clipboard, DUPLICATE_OFFSET, Turn};
pub use paint::PaintTool;
pub use paths::PointRef;

/// The whole pixels a box (x, y, w, h, canvas coordinates) covers: from the pixel holding its
/// left/top edge to the one holding its right/bottom edge, partly covered ones included. What the
/// Property inspector's W, H, X and Y show and the selection box outlines: 50.1 wide from x 10
/// covers 51 pixels; 20 wide from x 10.5 covers 21 (pixels 10 to 30). A hair of float noise
/// (bounds of 9.999999999999996) doesn't count as covering another pixel.
pub fn covered_pixels((x, y, w, h): (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    const EPS: f64 = 1e-6;
    let (l, t) = ((x + EPS).floor(), (y + EPS).floor());
    let (r, b) = ((x + w - EPS).ceil().max(l), (y + h - EPS).ceil().max(t));
    (l, t, r - l, b - t)
}
pub use symbols::SymbolLink;
pub use tools::{PaintOptions, Tool, ToolOptions};

use firecraft_doc::{
    BezierPoint, Bitmap, Contour, Document, Element, ElementKind, Fill, Layer, Path, RectShape, Stroke,
};
use firecraft_raster::mask::Combine;
use firecraft_raster::nine::{self, Insets};
use firecraft_raster::wand::magic_wand;
use firecraft_raster::{Mask, Rgba, RgbaImage};

/// An element address: layer index, element index (in the current frame), and for an object
/// inside groups (the Subselection tool) the child indices down to it. Ordered by stacking
/// (bottom first).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ElementRef {
    pub layer: usize,
    pub index: usize,
    pub sub: SubPath,
}

/// How deep a reference reaches into nested groups.
pub const MAX_SUB_DEPTH: usize = 8;

/// The child index that stands for a masked object's mask (its content are 0, 1, ...).
const MASK_SLOT: u16 = u16::MAX;

/// Child indices from a top-level object into groups (a group's children, a masked group's
/// content), outermost first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SubPath {
    len: u8,
    idx: [u16; MAX_SUB_DEPTH],
}

impl SubPath {
    pub fn as_slice(&self) -> &[u16] {
        self.idx.get(..self.len as usize).unwrap_or(&[])
    }
}

impl ElementRef {
    /// A top-level object.
    pub const fn new(layer: usize, index: usize) -> Self {
        Self {
            layer,
            index,
            sub: SubPath {
                len: 0,
                idx: [0; MAX_SUB_DEPTH],
            },
        }
    }

    /// Whether this is an object inside a group.
    pub fn is_nested(&self) -> bool {
        self.sub.len > 0
    }

    /// Child `i` of this object (a group's member), if not too deep.
    pub fn child(self, i: usize) -> Option<Self> {
        let mut r = self;
        let slot = r.sub.idx.get_mut(r.sub.len as usize)?;
        *slot = u16::try_from(i).ok()?;
        r.sub.len += 1;
        Some(r)
    }

    /// The mask of this (masked) object, as Fireworks CS selects it from the Layers panel's
    /// mask thumbnail.
    pub fn mask(self) -> Option<Self> {
        let mut r = self;
        let slot = r.sub.idx.get_mut(r.sub.len as usize)?;
        *slot = MASK_SLOT;
        r.sub.len += 1;
        Some(r)
    }

    /// Whether this is a masked object's mask.
    pub fn is_mask(&self) -> bool {
        self.sub.as_slice().last() == Some(&MASK_SLOT)
    }

    /// Whether this is a mask or an object inside one.
    pub fn in_mask(&self) -> bool {
        self.sub.as_slice().contains(&MASK_SLOT)
    }

    /// The group this object is in (`None` for a top-level object).
    pub fn parent(self) -> Option<Self> {
        if self.sub.len == 0 {
            return None;
        }
        let mut r = self;
        r.sub.len -= 1;
        if let Some(slot) = r.sub.idx.get_mut(r.sub.len as usize) {
            *slot = 0;
        }
        Some(r)
    }

    /// Whether this object is somewhere inside `outer` (its group, mask or one around them).
    pub fn is_within(&self, outer: ElementRef) -> bool {
        let mut p = self.parent();
        while let Some(x) = p {
            if x == outer {
                return true;
            }
            p = x.parent();
        }
        false
    }

    /// The top-level object this one is in (itself when top-level).
    pub fn top(self) -> Self {
        Self::new(self.layer, self.index)
    }

    /// Its index in its own list (the layer's objects, or its group's children).
    pub fn position(&self) -> usize {
        match self.sub.as_slice().last() {
            Some(&i) => i as usize,
            None => self.index,
        }
    }

    /// The object at index `i` of the same list.
    pub fn sibling(self, i: usize) -> Self {
        match self.parent() {
            Some(p) => p.child(i).unwrap_or(self),
            None => Self::new(self.layer, i),
        }
    }
}

/// Child `i` of `e` along a reference: a member, or for [`MASK_SLOT`] a masked object's mask.
fn step(e: &Element, i: u16) -> Option<&Element> {
    match (&e.kind, i) {
        (ElementKind::Masked { mask, .. }, MASK_SLOT) => Some(mask),
        _ => members(e)?.get(i as usize),
    }
}

fn step_mut(e: &mut Element, i: u16) -> Option<&mut Element> {
    if i == MASK_SLOT {
        return match &mut e.kind {
            ElementKind::Masked { mask, .. } => Some(mask),
            _ => None,
        };
    }
    members_mut(e)?.get_mut(i as usize)
}

/// A group's children (or a masked group's content): the objects the Subselection tool reaches.
pub fn members(e: &Element) -> Option<&Vec<Element>> {
    match &e.kind {
        ElementKind::Group(c) => Some(c),
        ElementKind::Masked { content, .. } => Some(content),
        _ => None,
    }
}

/// Whether object `el` covers canvas point (`x`, `y`) (within 2 pixels).
fn element_hit(el: &Element, x: f64, y: f64, ctx: &firecraft_doc::RenderCtx) -> bool {
    const TOL: i64 = 2;
    let (px, py) = (x.floor() as i64, y.floor() as i64);
    // Perspective symbols: their whole shape is drawn once and kept, so the pixel test is
    // exact and cheap, and their bounds can't be trusted (drawn the Fireworks MX way, folds7's
    // reach 50 pixels beyond the exact projection's).
    if let ElementKind::Instance { matrix, .. } = &el.kind
        && !matrix.is_affine()
    {
        let mut solo = el.clone();
        solo.opacity = 100.0;
        solo.blend = firecraft_raster::BlendMode::Normal;
        return firecraft_doc::perspective_hit(&solo, px, py, TOL, ctx);
    }
    // Where the object can have pixels (its stroke and effects reach beyond its geometry).
    let near = || {
        el.visual_bounds(ctx).is_some_and(|(bx, by, bw, bh)| {
            let t = TOL as f64;
            x >= bx - t && y >= by - t && x < bx + bw.max(1.0) + t && y < by + bh.max(1.0) + t
        })
    };
    match &el.kind {
        // Symbols, groups and anything with effects can be slow to draw (a blur draws its whole
        // source for any one pixel): test a kept rendering of the whole object.
        ElementKind::Instance { .. } | ElementKind::Group(_) | ElementKind::Masked { .. } => {
            solo_kept_hit(el, px, py, TOL, ctx, near)
        }
        _ if el.effects.iter().any(|e| e.visible()) => solo_kept_hit(el, px, py, TOL, ctx, near),
        _ if !near() => false,
        // Bitmaps: the pixel under the pointer.
        ElementKind::Bitmap(b) => b.image.get(px - b.x, py - b.y).is_some_and(|p| p.a > 0),
        // Plain paths and text: render the object alone around the point.
        _ => solo_probe(el, px, py, TOL, ctx),
    }
}

/// Whether `el`, drawn alone (full opacity, Normal) around a point, has pixels within `tol` of it.
fn solo_probe(el: &Element, px: i64, py: i64, tol: i64, ctx: &firecraft_doc::RenderCtx) -> bool {
    let n = (2 * tol + 1) as u32;
    // A path without effects or a feathered fill draws each pixel on its own: draw just the probe
    // (drawing it whole cost 95 ms per mouse movement on spectrum_wave.png's canvas-sized
    // textured gradient, which made the Pointer's hover highlight, and so every repaint and zoom,
    // sluggish).
    if let ElementKind::Path(p) = &el.kind
        && el.effects.iter().all(|e| e.active().is_none())
        && p.fill.as_ref().is_none_or(|f| f.feather == 0)
    {
        return el
            .rasterize(px - tol, py - tol, n, n, ctx)
            .is_some_and(|(img, x, y)| {
                (0..n as i64).any(|v| (0..n as i64).any(|u| img.get(u - x, v - y).is_some_and(|q| q.a > 0)))
            });
    }
    let mut probe = firecraft_raster::RgbaImage::new(n, n, Rgba::TRANSPARENT);
    let mut solo = el.clone();
    solo.opacity = 100.0;
    solo.blend = firecraft_raster::BlendMode::Normal;
    solo.render_onto(&mut probe, px - tol, py - tol, ctx);
    probe.pixels().iter().any(|p| p.a > 0)
}

/// [`firecraft_doc::kept_hit`] for an object drawn alone, or [`solo_probe`] if its extent is
/// unknown.
fn solo_kept_hit(
    el: &Element,
    px: i64,
    py: i64,
    tol: i64,
    ctx: &firecraft_doc::RenderCtx,
    near: impl Fn() -> bool,
) -> bool {
    firecraft_doc::kept_hit(el, px, py, tol, ctx, &near).unwrap_or_else(|| near() && solo_probe(el, px, py, tol, ctx))
}

/// Give an object a new effect list. A text still showing the image Fireworks stored with it has
/// its old effects drawn into that image, so it is drawn afresh first.
fn set_element_effects(e: &mut Element, list: &[firecraft_doc::EffectEntry]) {
    if let ElementKind::Text(t) = &mut e.kind
        && t.baked_effects
        && !t.edited
        && e.effects.as_slice() != list
    {
        text::refresh(t);
    }
    e.effects = list.to_vec();
}

fn members_mut(e: &mut Element) -> Option<&mut Vec<Element>> {
    match &mut e.kind {
        ElementKind::Group(c) => Some(c),
        ElementKind::Masked { content, .. } => Some(content),
        _ => None,
    }
}

/// Selected pixels (marquee, Magic Wand) and the bitmap they apply to. The mask is anchored to
/// the canvas, not to the bitmap: in Fireworks MX, picking another bitmap in the Layers panel
/// keeps the marquee where it is and applies it to that bitmap ("Edit Bitmap").
#[derive(Clone, Debug, PartialEq)]
pub struct PixelSelection {
    /// The bitmap being edited; `None` for a marquee drawn on empty canvas with no bitmap being
    /// edited (Fireworks MX: filling or painting in it makes a new bitmap).
    pub target: Option<ElementRef>,
    /// Canvas position of the mask's top-left pixel.
    pub x: i64,
    pub y: i64,
    pub mask: Mask,
}

impl PixelSelection {
    /// The selection as the bitmap at canvas (`bx`, `by`), `w` x `h` pixels, sees it: a mask of
    /// the bitmap's size in its own pixel coordinates.
    pub fn local(&self, bx: i64, by: i64, w: u32, h: u32) -> Mask {
        let mut m = Mask::new(w, h);
        let (dx, dy) = (bx - self.x, by - self.y);
        // Only the rows and columns the two have in common.
        let x0 = (-dx).max(0);
        let x1 = (w as i64).min(self.mask.width() as i64 - dx);
        let y0 = (-dy).max(0);
        let y1 = (h as i64).min(self.mask.height() as i64 - dy);
        for y in y0..y1 {
            for x in x0..x1 {
                m.set(x, y, self.mask.get(x + dx, y + dy));
            }
        }
        m
    }

    /// [`local`](Self::local) for a bitmap.
    pub fn on(&self, b: &Bitmap) -> Mask {
        self.local(b.x, b.y, b.image.width(), b.image.height())
    }

    /// Grow the mask (unselected) so it reaches over the canvas rectangle `l, t, r, b`.
    pub fn cover(&mut self, (l, t, r, b): (i64, i64, i64, i64)) {
        let (sl, st) = (self.x, self.y);
        let (sr, sb) = (sl + self.mask.width() as i64, st + self.mask.height() as i64);
        let (nl, nt, nr, nb) = (sl.min(l), st.min(t), sr.max(r), sb.max(b));
        if (nl, nt, nr, nb) == (sl, st, sr, sb) {
            return;
        }
        self.mask = self.local(nl, nt, (nr - nl) as u32, (nb - nt) as u32);
        self.x = nl;
        self.y = nt;
    }

    /// Combine `mask`, placed at canvas (`x`, `y`), into this selection.
    pub fn combine(&mut self, x: i64, y: i64, mask: &Mask, mode: Combine) {
        if mode == Combine::Add {
            self.cover((x, y, x + mask.width() as i64, y + mask.height() as i64));
        }
        let placed = PixelSelection {
            target: self.target,
            x,
            y,
            mask: mask.clone(),
        }
        .local(self.x, self.y, self.mask.width(), self.mask.height());
        self.mask.combine(&placed, mode);
    }
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
    /// The live edit in progress: its undo step is recorded when it is committed.
    live_edit: Option<LiveEdit>,
    /// The Brush, Pencil or Eraser stroke being drawn.
    live_paint: Option<paint::LivePaint>,
    /// The bitmap double-clicked into bitmap mode (Fireworks MX: "Edit Bitmap"), while it has no
    /// pixel selection yet; it counts while it stays selected.
    pub bitmap_edit: Option<ElementRef>,
    /// Image stamps of the bitmaps Insert > Empty Bitmap made: a bitmap still holding one is
    /// untouched (every write gives an image a new stamp), and goes away once deselected.
    empty_bitmaps: Vec<u64>,
    /// Bumped on every change; the UI re-renders the canvas when it moves.
    pub revision: u64,
    /// How bitmaps are resampled when objects are scaled, rotated, skewed or distorted
    /// (Fireworks MX's Preferences > General > Interpolation, `EditTransformMode`; the app sets
    /// it from its preferences).
    pub interpolation: firecraft_raster::transform::Resample,
}

/// A live edit in progress (see [`DocState::begin_live_edit`]).
#[derive(Clone, Debug)]
struct LiveEdit {
    label: String,
    before: Document,
    selected: Selected,
    dirty: bool,
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
            live_edit: None,
            live_paint: None,
            empty_bitmaps: Vec::new(),
            bitmap_edit: None,
            revision: 0,
            interpolation: firecraft_raster::transform::Resample::Bicubic,
        }
    }

    pub fn frame(&self) -> usize {
        self.doc.current_frame
    }

    pub fn element(&self, r: ElementRef) -> Option<&Element> {
        let mut e = self
            .doc
            .layers
            .get(r.layer)?
            .elements(self.frame())
            .get(r.index)?;
        for &i in r.sub.as_slice() {
            e = step(e, i)?;
        }
        Some(e)
    }

    fn element_mut(&mut self, r: ElementRef) -> Option<&mut Element> {
        let f = self.frame();
        let mut e = self
            .doc
            .layers
            .get_mut(r.layer)?
            .elements_mut(f)?
            .get_mut(r.index)?;
        for &i in r.sub.as_slice() {
            e = step_mut(e, i)?;
        }
        Some(e)
    }

    /// The list object `r` is in (its layer's objects, or its group's children).
    pub(crate) fn list_mut(&mut self, r: ElementRef) -> Option<&mut Vec<Element>> {
        // A mask is no list's member: deleting, arranging or grouping leave it alone.
        if r.is_mask() {
            return None;
        }
        match r.parent() {
            Some(p) => members_mut(self.element_mut(p)?),
            None => {
                let f = self.frame();
                self.doc.layers.get_mut(r.layer)?.elements_mut(f)
            }
        }
    }

    /// Whether the selection has objects inside groups (commands that restructure layers, such
    /// as Group or Convert to Symbol, then do nothing).
    pub fn selection_nested(&self) -> bool {
        self.selection.iter().any(ElementRef::is_nested)
    }

    /// Record an undo step, then apply `f`.
    pub fn edit(&mut self, label: &str, f: impl FnOnce(&mut DocState)) {
        self.commit_live_edit();
        let selected = self.selected_now();
        self.history.push(label, &self.doc, selected);
        f(self);
        self.touch();
    }

    /// What is selected now, as an undo step keeps it.
    fn selected_now(&self) -> Selected {
        self.history.selected(&self.selection, self.pixels.as_ref(), self.bitmap_edit)
    }

    /// An undo step that changes only what is selected (Fireworks MX records "Select Pixels",
    /// "Edit Bitmap", "Deselect"): `f` changes the selection (or the current layer); undo brings
    /// back the one before. Nothing is recorded if nothing changed.
    pub(crate) fn select_step(&mut self, label: &str, f: impl FnOnce(&mut DocState)) {
        self.commit_live_edit();
        let before = self.selected_now();
        let layer = self.doc.current_layer;
        self.history.push(label, &self.doc, before.clone());
        f(self);
        if self.selected_now() == before && self.doc.current_layer == layer {
            self.history.discard_last();
            return;
        }
        // The document's content is unchanged: redraw, but nothing to save.
        self.revision = self.revision.wrapping_add(1);
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
        self.commit_live_edit();
        let now = self.selected_now();
        if let Some((d, sel)) = self.history.undo(&self.doc, now) {
            self.doc = d;
            self.restore_selected(sel);
        }
    }

    pub fn redo(&mut self) {
        self.commit_live_edit();
        let now = self.selected_now();
        if let Some((d, sel)) = self.history.redo(&self.doc, now) {
            self.doc = d;
            self.restore_selected(sel);
        }
    }

    /// After undo or redo: the step's pixel selection comes back with the bitmap it applied to;
    /// without one, the objects selected now stay selected (those that still exist).
    fn restore_selected(&mut self, sel: Selected) {
        self.pixels = sel.pixels.map(|p| (*p).clone());
        self.bitmap_edit = sel.editing;
        if self.pixels.is_some() || self.bitmap_edit.is_some() {
            self.selection = sel.objects;
        }
        self.after_history();
    }

    /// Drop references the document no longer has.
    fn after_history(&mut self) {
        let sel = std::mem::take(&mut self.selection);
        self.selection = sel.into_iter().filter(|r| self.element(*r).is_some()).collect();
        let target_ok = |s: &Self, r: ElementRef| matches!(s.element(r).map(|e| &e.kind), Some(ElementKind::Bitmap(_)));
        if self.pixels.as_ref().is_some_and(|p| p.target.is_some_and(|t| !target_ok(self, t))) {
            self.pixels = None;
        }
        if self.bitmap_edit.is_some_and(|r| !target_ok(self, r)) {
            self.bitmap_edit = None;
        }
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
        let ctx = firecraft_doc::RenderCtx {
            mx_perspective: self.doc.mx_perspective,
            symbols: &self.doc.symbols,
            frame: self.frame(),
            depth: 0,
            math: self.doc.color_math,
            chain: [0; firecraft_doc::MAX_SYMBOL_DEPTH as usize],
            canvas_textures: false,
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
                if element_hit(el, x, y, &ctx) {
                    out.push(ElementRef::new(li, ei));
                }
            }
        }
        out
    }

    /// The Subselection tool's pick: the object under the point, inside groups down to the
    /// innermost member (the topmost member at each level).
    /// The selected object a click at (`x`, `y`) lands on, if it is part of the top-level object
    /// `hit` belongs to (an object chosen inside a mask or group in the Layers panel, its mask
    /// included). A click there keeps the selection rather than choosing the masked object or
    /// another member around it. A selected group whose member was hit doesn't count, so the
    /// Subselection tool still selects the member (Fireworks MX).
    pub fn selected_under(&self, x: f64, y: f64, hit: ElementRef) -> Option<ElementRef> {
        let ctx = firecraft_doc::RenderCtx {
            mx_perspective: self.doc.mx_perspective,
            symbols: &self.doc.symbols,
            frame: self.frame(),
            math: self.doc.color_math,
            ..firecraft_doc::RenderCtx::EMPTY
        };
        self.selection.iter().copied().find(|s| {
            s.top() == hit.top()
                && !hit.is_within(*s)
                && self.element(*s).is_some_and(|e| element_hit(e, x, y, &ctx))
        })
    }

    /// Whether a click at (`x`, `y`) that found `hit` lands on a selected object (see
    /// [`selected_under`](Self::selected_under)).
    pub fn click_keeps_selection(&self, x: f64, y: f64, hit: ElementRef) -> bool {
        self.selected_under(x, y, hit).is_some()
    }

    /// What moving or transforming the selection moves: each selected object, but for one in a
    /// masked object's content while its mask is linked, that whole masked object (mask and all
    /// its content move together; only unlinked content moves alone). A selected mask moves
    /// alone (that is editing the mask).
    pub fn motion_refs(&self) -> Vec<ElementRef> {
        let mut out: Vec<ElementRef> = Vec::new();
        for s in &self.selection {
            let mut cur = *s;
            while let Some(p) = cur.parent() {
                let linked = !cur.is_mask()
                    && matches!(&self.element(p).map(|e| &e.kind), Some(ElementKind::Masked { flags, .. }) if flags.linked);
                if !linked {
                    break;
                }
                cur = p;
            }
            if !out.contains(&cur) {
                out.push(cur);
            }
        }
        let all = out.clone();
        out.retain(|r| !all.iter().any(|o| r.is_within(*o)));
        out
    }

    /// The Subselection tool's pick (measured in Fireworks MX on cone3): into groups down to the
    /// member under the point, and at a masked object its mask (a second click stays there).
    pub fn hit_sub(&self, x: f64, y: f64) -> Option<ElementRef> {
        let ctx = firecraft_doc::RenderCtx {
            mx_perspective: self.doc.mx_perspective,
            symbols: &self.doc.symbols,
            frame: self.frame(),
            math: self.doc.color_math,
            ..firecraft_doc::RenderCtx::EMPTY
        };
        let mut r = self.hit_test(x, y)?;
        loop {
            match &self.element(r)?.kind {
                ElementKind::Masked { .. } => return r.mask().or(Some(r)),
                ElementKind::Group(children) => {
                    let next = children
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(_, c)| c.visible && !c.locked && element_hit(c, x, y, &ctx))
                        .and_then(|(i, _)| r.child(i));
                    match next {
                        Some(n) => r = n,
                        None => return Some(r),
                    }
                }
                _ => return Some(r),
            }
        }
    }

    pub fn hit_deep(&self, x: f64, y: f64) -> Option<ElementRef> {
        let ctx = firecraft_doc::RenderCtx {
            mx_perspective: self.doc.mx_perspective,
            symbols: &self.doc.symbols,
            frame: self.frame(),
            depth: 0,
            math: self.doc.color_math,
            chain: [0; firecraft_doc::MAX_SYMBOL_DEPTH as usize],
            canvas_textures: false,
        };
        let mut r = self.hit_test(x, y)?;
        'down: loop {
            let Some(children) = self.element(r).and_then(members) else {
                break;
            };
            for (i, c) in children.iter().enumerate().rev() {
                if c.visible
                    && !c.locked
                    && element_hit(c, x, y, &ctx)
                    && let Some(next) = r.child(i)
                {
                    r = next;
                    continue 'down;
                }
            }
            break;
        }
        Some(r)
    }

    /// Select Behind: select the object below the currently selected one at this point (the
    /// topmost if the selection isn't under the point), wrapping around.
    pub fn select_behind(&mut self, x: f64, y: f64) {
        match self.behind_target(x, y) {
            Some(h) => {
                self.selection = vec![h];
                self.pixels = None;
            }
            None => self.deselect(),
        }
    }

    /// What a Select Behind click at this point would select: the object below the selected
    /// one (the topmost if the selection isn't under the point), wrapping around.
    pub fn behind_target(&self, x: f64, y: f64) -> Option<ElementRef> {
        let hits = self.hits_at(x, y);
        let next = match self.selection.as_slice() {
            [one] => hits
                .iter()
                .position(|h| h == one)
                .map_or(0, |k| (k + 1) % hits.len().max(1)),
            _ => 0,
        };
        hits.get(next).copied()
    }

    /// Whether canvas point (`x`, `y`) is inside the selection's bounding box (pressing there
    /// with the Pointer drags the selection, even where another object lies on top).
    pub fn selection_contains(&self, x: f64, y: f64) -> bool {
        // An empty bitmap covers the canvas but shows no box: a press elsewhere deselects it.
        if self.selection.iter().all(|r| self.is_empty_bitmap(*r)) {
            return false;
        }
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
        if let Some(r) = self.pixels.as_ref().and_then(|p| p.target)
            && let Some(ElementKind::Bitmap(b)) = self.element(r).map(|e| &e.kind)
        {
            let mut mask = Mask::new(b.image.width(), b.image.height());
            mask.data_mut().iter_mut().for_each(|v| *v = 255);
            let all = PixelSelection {
                target: Some(r),
                x: b.x,
                y: b.y,
                mask,
            };
            self.select_step("Select All", |s| s.pixels = Some(all));
            return;
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
                    .map(move |(ei, _)| ElementRef::new(li, ei))
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

    /// Select > Superselect: the groups the selected members are in (Fireworks MX).
    pub fn superselect(&mut self) {
        let mut sel: Vec<ElementRef> = self.selection.iter().map(|r| r.parent().unwrap_or(*r)).collect();
        sel.sort();
        sel.dedup();
        self.selection = sel;
        self.pixels = None;
    }

    /// Select > Subselect: the members of the selected groups (Fireworks MX); other objects
    /// stay selected.
    pub fn subselect(&mut self) {
        let mut sel = Vec::new();
        for r in self.selection.clone() {
            match self.element(r).and_then(members) {
                Some(children) if !children.is_empty() => {
                    sel.extend((0..children.len()).filter_map(|i| r.child(i)));
                }
                _ => sel.push(r),
            }
        }
        self.selection = sel;
        self.pixels = None;
    }

    /// Select > Select Inverse (pixel selections): everything else on the canvas and the
    /// bitmap being edited.
    pub fn select_inverse(&mut self) {
        let Some(mut sel) = self.pixels.clone() else { return };
        let (w, h) = (self.doc.width as i64, self.doc.height as i64);
        sel.cover((0, 0, w, h));
        if let Some(ElementKind::Bitmap(b)) = sel.target.and_then(|t| self.element(t)).map(|e| &e.kind) {
            sel.cover((b.x, b.y, b.x + b.image.width() as i64, b.y + b.image.height() as i64));
        }
        sel.mask.invert();
        self.select_step("Select Inverse", |s| s.pixels = Some(sel));
    }

    /// Select > Feather.
    pub fn feather_pixels(&mut self, radius: u32) {
        let Some(mut sel) = self.pixels.clone() else { return };
        // Room for the soft edge to spread into.
        let pad = radius.min(250) as i64;
        let (l, t) = (sel.x - pad, sel.y - pad);
        let (r, b) = (
            sel.x + sel.mask.width() as i64 + pad,
            sel.y + sel.mask.height() as i64 + pad,
        );
        sel.cover((l, t, r, b));
        sel.mask.feather(radius);
        self.select_step("Feather", |s| s.pixels = Some(sel));
    }

    /// Select > Deselect, clicking empty canvas: nothing selected. Ending a pixel selection is
    /// an undo step, as in Fireworks MX ("Deselect").
    pub fn deselect(&mut self) {
        if self.pixels.is_some() {
            self.select_step("Deselect", |s| {
                s.selection.clear();
                s.pixels = None;
            });
            return;
        }
        self.selection.clear();
    }

    /// End bitmap editing, as choosing the Pointer tool or pressing Escape does in Fireworks MX:
    /// the pixel selection goes (one undo step, "Exit Bitmap Mode") and the bitmap that was
    /// being edited stays selected. An empty bitmap (Insert > Empty Bitmap) nothing was put
    /// into is deselected, so it goes away (on its own that is no undo step, as in Fireworks).
    /// `false` if no bitmap was being edited.
    pub fn exit_bitmap_mode(&mut self) -> bool {
        if !self.in_bitmap_mode() {
            return false;
        }
        let target = self
            .pixels
            .as_ref()
            .and_then(|p| p.target)
            .or(self.bitmap_edit.filter(|r| self.selection.contains(r)));
        let before = target.and_then(|t| self.element(t).cloned());
        // The edited bitmap shrinks to its pixels even if none changed (Fireworks MX, measured
        // 2026-11-15: a Magic Wand selection on a 200x150 bitmap whose pixels cover 10,10
        // 170x120, then Escape, left it 10,10 170x120). Emptied, it goes. The tool stays.
        let leave = |s: &mut Self| {
            s.pixels = None;
            s.bitmap_edit = None;
            if let Some(t) = target {
                s.shrink_bitmap(t);
            }
            let empty: Vec<ElementRef> =
                s.selection.iter().copied().filter(|r| s.is_empty_bitmap(*r)).collect();
            s.selection.retain(|r| !empty.contains(r));
        };
        if self.pixels.is_some() || self.bitmap_edit.is_some() {
            self.select_step("Exit Bitmap Mode", leave);
        } else {
            leave(self);
        }
        if target.is_some_and(|t| self.element(t).cloned() != before) {
            self.dirty = true;
        }
        self.drop_empty_bitmaps();
        true
    }

    /// Whether a bitmap is being edited (Fireworks MX's bitmap mode: the document window's
    /// stop button is red): pixels are selected, a bitmap was double-clicked, or an empty bitmap
    /// from Insert > Empty Bitmap is selected.
    pub fn in_bitmap_mode(&self) -> bool {
        self.pixels.is_some()
            || self.bitmap_edit.is_some_and(|r| self.selection.contains(&r))
            || self.selection.iter().any(|r| self.is_empty_bitmap(*r))
    }

    /// Pointer double-click on bitmap `r`: edit it (Fireworks MX: bitmap mode, the Marquee tool
    /// chosen by the caller; undo step "Edit Bitmap"). `false` if `r` isn't a bitmap.
    pub fn enter_bitmap_mode(&mut self, r: ElementRef) -> bool {
        if !matches!(self.element(r).map(|e| &e.kind), Some(ElementKind::Bitmap(_))) {
            return false;
        }
        self.select_step("Edit Bitmap", |s| {
            if s.pixels.as_ref().is_some_and(|p| p.target != Some(r)) {
                s.pixels = None;
            }
            s.bitmap_edit = Some(r);
            s.selection = vec![r];
            s.doc.current_layer = r.layer;
        });
        true
    }

    /// End the pixel selection only (a click with a marquee tool, or inside the selection with
    /// the Magic Wand): one undo step, recorded as "Select Pixels" as in Fireworks MX.
    pub fn deselect_pixels(&mut self) {
        if self.pixels.is_some() {
            self.select_step("Select Pixels", |s| s.pixels = None);
        }
    }

    /// Layers panel: pick bitmap `r` while pixels are selected. As in Fireworks MX the
    /// selection stays where it is and now applies to `r` (undo step "Edit Bitmap"). `false`
    /// if there is no pixel selection or `r` isn't a bitmap (it is then selected as usual).
    pub fn edit_bitmap(&mut self, r: ElementRef) -> bool {
        let editing = self.pixels.is_some() || self.bitmap_edit.is_some_and(|b| self.selection.contains(&b));
        if !editing || !matches!(self.element(r).map(|e| &e.kind), Some(ElementKind::Bitmap(_))) {
            return false;
        }
        self.select_step("Edit Bitmap", |s| {
            if let Some(p) = s.pixels.as_mut() {
                p.target = Some(r);
            }
            if s.bitmap_edit.is_some() {
                s.bitmap_edit = Some(r);
            }
            s.selection = vec![r];
            s.doc.current_layer = r.layer;
        });
        true
    }

    /// Layers panel: make layer `i` current. With pixels selected this ends the pixel
    /// selection, as an undo step ("Layer Change", measured in Fireworks MX).
    pub fn set_current_layer(&mut self, i: usize) {
        if self.doc.layers.get(i).is_none() {
            return;
        }
        if self.pixels.is_some() {
            self.select_step("Layer Change", |s| {
                s.pixels = None;
                s.doc.current_layer = i;
            });
        } else {
            self.doc.current_layer = i;
        }
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
        let (bx, by) = (b.x, b.y);
        self.combine_pixels("Magic Wand Tool", Some(target), bx, by, mask, combine);
    }

    /// The pixel-grid box a marquee dragged over `rect` (canvas coordinates, any two corners)
    /// selects: (left, top, right, bottom) on whole pixels, each corner at the grid line nearest
    /// the pointer, as the Crop tool's box. The canvas shows this box while the marquee is
    /// dragged, so what will be selected is visible before the button is let go.
    pub fn marquee_box(rect: (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
        let (ax, ay, bx, by) = (rect.0.round(), rect.1.round(), rect.2.round(), rect.3.round());
        (ax.min(bx), ay.min(by), ax.max(bx), ay.max(by))
    }

    /// Rectangular or elliptical marquee in canvas coordinates.
    pub fn marquee(&mut self, rect: (f64, f64, f64, f64), oval: bool, opts: &ToolOptions, combine: Combine) {
        let (x0, y0, x1, y1) = Self::marquee_box(rect);
        // The bitmap being edited, else one under the marquee; on empty canvas with no bitmap
        // being edited, none: filling or painting in the marquee then makes a new bitmap
        // (Fireworks MX).
        let target = match &self.pixels {
            Some(p) => p.target,
            None => self
                .pixel_target((x0 + x1) / 2.0, (y0 + y1) / 2.0)
                .or_else(|| self.pixel_target(x0, y0)),
        };
        // The mask covers the marquee (and room for a feathered edge), wherever the bitmap is:
        // it stays put when another bitmap is picked for editing.
        let pad = match opts.marquee_edge {
            firecraft_raster::Edge::Feather(r) => r.min(250) as i64,
            _ => 0,
        } + 1;
        // Whole pixels, as Fireworks MX: at 100% a drag from the pixel at canvas x 20 to the one
        // at 60 selects pixels 20 to 59, and a rectangle's edges stay hard even with Anti-Alias
        // (measured 2026-11-15: only alpha 0 and 255; a second fill meeting the first leaves no
        // seam). Only an oval's outline is anti-aliased.
        let (ml, mt) = (x0 as i64 - pad, y0 as i64 - pad);
        let (mr, mb) = (x1 as i64 + pad, y1 as i64 + pad);
        let mut mask = Mask::new((mr - ml).max(1) as u32, (mb - mt).max(1) as u32);
        let (ox, oy) = (ml as f64, mt as f64);
        let (cx, cy, rx, ry) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (x1 - x0) / 2.0, (y1 - y0) / 2.0);
        let aa = oval && opts.marquee_edge != firecraft_raster::Edge::Hard;
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
        self.combine_pixels("Select Pixels", target, ml, mt, mask, combine);
    }

    /// Whether canvas point (`x`, `y`) is inside the pixel selection.
    pub fn pixels_contain(&self, x: f64, y: f64) -> bool {
        self.pixels.as_ref().is_some_and(|p| {
            p.mask.get(x.floor() as i64 - p.x, y.floor() as i64 - p.y) > 0
        })
    }

    /// Drag the marquee (a marquee tool or the Magic Wand pressed inside it): the selection
    /// moves, the pixels stay. One undo step, "Move" (Fireworks MX).
    pub fn move_pixel_selection(&mut self, dx: i64, dy: i64) {
        if self.pixels.is_none() || (dx == 0 && dy == 0) {
            return;
        }
        self.select_step("Move", |s| {
            if let Some(p) = s.pixels.as_mut() {
                p.x += dx;
                p.y += dy;
            }
        });
    }

    /// A new marquee or wand selection (`mask` at canvas `x`, `y`) combined with the current
    /// one; one undo step named `label` ("Select Pixels", "Magic Wand Tool": Fireworks MX).
    fn combine_pixels(
        &mut self,
        label: &str,
        target: Option<ElementRef>,
        x: i64,
        y: i64,
        mask: Mask,
        combine: Combine,
    ) {
        let next = match self.pixels.clone() {
            Some(mut sel) if combine != Combine::Replace => {
                sel.target = target;
                sel.combine(x, y, &mask, combine);
                sel
            }
            _ => PixelSelection { target, x, y, mask },
        };
        let next = (!next.mask.is_empty()).then_some(next);
        self.select_step(label, |s| {
            s.pixels = next;
            s.selection = target.into_iter().collect();
            if let Some(t) = target {
                s.doc.current_layer = t.layer;
            }
        });
    }

    /// Edit > Clear / Delete key: remove selected pixels, else delete selected objects.
    pub fn delete(&mut self) {
        self.delete_as("Delete");
    }

    /// [`delete`](Self::delete) recorded as `label` ("Delete", "Cut").
    pub(crate) fn delete_as(&mut self, label: &str) {
        if let Some(sel) = self.pixels.clone() {
            // A marquee with no bitmap yet: nothing to delete.
            let Some(target) = sel.target else { return };
            self.edit(label, |s| {
                if let Some(Element {
                    kind: ElementKind::Bitmap(b),
                    ..
                }) = s.element_mut(target)
                {
                    let mask = sel.on(b);
                    b.image.clear_masked(&mask);
                }
                s.shrink_bitmap(target);
            });
            return;
        }
        if self.selection.iter().all(ElementRef::is_mask) {
            return;
        }
        // Last first, so earlier references stay valid.
        let mut refs = self.selection.clone();
        refs.sort_by(|a, b| b.cmp(a));
        self.edit(label, |s| {
            for r in refs {
                let mut r = Some(r);
                while let Some(cur) = r.take() {
                    let Some(list) = s.list_mut(cur) else { break };
                    if cur.position() < list.len() {
                        list.remove(cur.position());
                    }
                    // A group left empty goes too (as in Fireworks).
                    if list.is_empty()
                        && let Some(p) = cur.parent()
                        && matches!(s.element(p).map(|e| &e.kind), Some(ElementKind::Group(_)))
                    {
                        r = Some(p);
                    }
                }
            }
            s.selection.clear();
        });
    }

    /// Edit > Insert > Bitmap Via Cut (`cut` = true) or Bitmap Via Copy, and the canvas context
    /// menu's New Bitmap Via Cut/Copy: the selected pixels of the bitmap being edited become a
    /// new bitmap right above it. As in Fireworks MX (measured 2026-11-15) the marquee stays and
    /// now applies to the new bitmap, which is selected; one undo step, "New Bitmap Via Cut" or
    /// "New Bitmap Via Copy".
    pub fn bitmap_via(&mut self, cut: bool) {
        let Some(sel) = self.pixels.clone() else { return };
        let Some(target) = sel.target else { return };
        let Some(Element {
            kind: ElementKind::Bitmap(src),
            ..
        }) = self.element(target)
        else {
            return;
        };
        let local = sel.on(src);
        let Some((bx, by, bw, bh)) = local.bounds() else {
            return;
        };
        let full = src.image.extract_masked(&local);
        let mut piece = RgbaImage::new(bw, bh, Rgba::TRANSPARENT);
        piece.draw(&full, -(bx as i64), -(by as i64), 255);
        let new = Element::new(ElementKind::Bitmap(Bitmap {
            x: src.x + bx as i64,
            y: src.y + by as i64,
            image: piece,
        }));
        let label = if cut { "New Bitmap Via Cut" } else { "New Bitmap Via Copy" };
        self.edit(label, |s| {
            if cut {
                if let Some(Element {
                    kind: ElementKind::Bitmap(b),
                    ..
                }) = s.element_mut(target)
                {
                    b.image.clear_masked(&local);
                }
                s.shrink_bitmap(target);
            }
            let f = s.frame();
            if let Some(cell) = s
                .doc
                .layers
                .get_mut(target.layer)
                .and_then(|l| l.elements_mut(f))
            {
                let at = (target.index + 1).min(cell.len());
                cell.insert(at, new);
                let r = ElementRef::new(target.layer, at);
                s.selection = vec![r];
                s.doc.current_layer = target.layer;
                if let Some(p) = s.pixels.as_mut() {
                    p.target = Some(r);
                }
            }
        });
    }

    /// Edit > Insert > Empty Bitmap, the Layers panel's New Bitmap Image: a transparent bitmap
    /// the size of the canvas at the top of the current layer, selected, to paint or fill. As in
    /// Fireworks MX it is no undo step, and deselected while still empty it goes away again
    /// ([`drop_empty_bitmaps`](Self::drop_empty_bitmaps)).
    pub fn insert_empty_bitmap(&mut self) {
        self.commit_live_edit();
        let image = RgbaImage::new(self.doc.width.max(1), self.doc.height.max(1), Rgba::TRANSPARENT);
        let stamp = image.stamp();
        let layer = self.writable_layer();
        let f = self.frame();
        let Some(cell) = self.doc.layers.get_mut(layer).and_then(|l| l.elements_mut(f)) else {
            return;
        };
        cell.push(Element::new(ElementKind::Bitmap(Bitmap { x: 0, y: 0, image })));
        let r = ElementRef::new(layer, cell.len() - 1);
        self.selection = vec![r];
        self.doc.current_layer = layer;
        // A marquee stays and now applies to the new bitmap.
        if let Some(p) = self.pixels.as_mut() {
            p.target = Some(r);
        }
        self.empty_bitmaps.push(stamp);
        self.touch();
    }

    /// Whether `r` is a bitmap from [`insert_empty_bitmap`](Self::insert_empty_bitmap) that
    /// nothing has been painted, filled or pasted into yet.
    pub fn is_empty_bitmap(&self, r: ElementRef) -> bool {
        !self.empty_bitmaps.is_empty()
            && matches!(
                self.element(r).map(|e| &e.kind),
                Some(ElementKind::Bitmap(b)) if self.empty_bitmaps.contains(&b.image.stamp())
            )
    }

    /// Remove the empty bitmaps that are no longer selected (Fireworks MX: "If the empty bitmap
    /// is deselected before any pixels have been drawn [...] it disappears"). No undo step, as in
    /// Fireworks. The UI calls this once per update.
    pub fn drop_empty_bitmaps(&mut self) {
        // Bitmap mode from a double-click ends when the bitmap is no longer selected.
        if self.bitmap_edit.is_some_and(|r| !self.selection.contains(&r)) {
            self.bitmap_edit = None;
        }
        if self.empty_bitmaps.is_empty() || self.live_paint.is_some() || self.live_edit.is_some() {
            return;
        }
        let f = self.frame();
        let in_use = |r: ElementRef| {
            self.selection.iter().any(|s| s.layer == r.layer && s.index == r.index)
                || self.pixels.as_ref().is_some_and(|p| p.target == Some(r))
        };
        let mut gone = Vec::new();
        for (li, layer) in self.doc.layers.iter().enumerate() {
            for (ei, el) in layer.elements(f).iter().enumerate() {
                if let ElementKind::Bitmap(b) = &el.kind
                    && self.empty_bitmaps.contains(&b.image.stamp())
                    && !in_use(ElementRef::new(li, ei))
                {
                    gone.push((li, ei));
                }
            }
        }
        if gone.is_empty() {
            return;
        }
        // Last first, so the earlier positions stay valid; references above each one move down.
        for &(li, ei) in gone.iter().rev() {
            if let Some(cell) = self.doc.layers.get_mut(li).and_then(|l| l.elements_mut(f)) {
                cell.remove(ei);
            }
            let shift = |r: &mut ElementRef| {
                if r.layer == li && r.index > ei {
                    r.index -= 1;
                }
            };
            self.selection.iter_mut().for_each(shift);
            if let Some(p) = self.pixels.as_mut() {
                if let Some(t) = &mut p.target {
                    shift(t);
                }
            }
            if let Some(t) = &mut self.bitmap_edit {
                shift(t);
            }
        }
        // The document is as it was before the insert: redraw, but it is not a change.
        self.revision = self.revision.wrapping_add(1);
    }

    /// After pixels were taken out of bitmap `r` (Delete, Cut, Bitmap Via Cut): its bounds shrink
    /// to the pixels left, as Fireworks MX's do when bitmap editing ends (measured 2026-11-15:
    /// red 10,10 100x80 minus its left 50 columns became 60,10 50x80). Emptied completely, it
    /// stays while it is being edited and goes away once it isn't (MX removes it at "Exit Bitmap
    /// Mode"), like an empty bitmap from Insert > Empty Bitmap.
    pub(crate) fn shrink_bitmap(&mut self, r: ElementRef) {
        let Some(ElementKind::Bitmap(b)) = self.element(r).map(|e| &e.kind) else {
            return;
        };
        if paint::opaque_bounds(&b.image).is_none() {
            let stamp = b.image.stamp();
            self.empty_bitmaps.push(stamp);
            return;
        }
        self.trim_bitmap(r, None);
    }

    /// Shrink bitmap `r` to its non-transparent pixels (a pixel selection is anchored to the
    /// canvas, so it stays where it is).
    /// `keep` (canvas left, top, right, bottom) stays whatever it holds.
    pub(crate) fn trim_bitmap(&mut self, r: ElementRef, keep: Option<(i64, i64, i64, i64)>) {
        let Some(Element {
            kind: ElementKind::Bitmap(b),
            ..
        }) = self.element_mut(r)
        else {
            return;
        };
        let (w, h) = (b.image.width() as i64, b.image.height() as i64);
        let opaque = paint::opaque_bounds(&b.image);
        // `keep` in the bitmap's own pixels.
        let keep = keep.map(|(l, t, r, bb)| (l - b.x, t - b.y, r - b.x, bb - b.y));
        let Some((l, t, rr, bb)) = (match (opaque, keep) {
            (Some(o), Some(k)) => Some((o.0.min(k.0), o.1.min(k.1), o.2.max(k.2), o.3.max(k.3))),
            (o, k) => o.or(k),
        }) else {
            return;
        };
        if (l, t, rr, bb) == (0, 0, w, h) {
            return;
        }
        let mut crop = RgbaImage::new((rr - l) as u32, (bb - t) as u32, Rgba::TRANSPARENT);
        crop.copy_from(&b.image, -l, -t);
        b.x += l;
        b.y += t;
        b.image = crop;
    }

    /// Move the selected objects.
    pub fn move_selection(&mut self, dx: f64, dy: f64, label: &str) {
        if self.selection.is_empty() || (dx == 0.0 && dy == 0.0) {
            return;
        }
        let refs = self.motion_refs();
        self.edit(label, |s| {
            for r in refs {
                if let Some(e) = s.element_mut(r) {
                    e.move_by(dx, dy);
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
        for r in self.motion_refs() {
            if let Some(e) = self.element_mut(r) {
                e.move_by(dx, dy);
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
        let method = self.interpolation;
        for (r, orig) in originals {
            if let Some(e) = self.element_mut(*r) {
                let mut t = orig.clone();
                t.transform_by(m, method);
                *e = t;
            }
        }
        self.touch();
    }

    /// How `objects` look now (effects included, before opacity and blending), for previewing
    /// a transform cheaply: each object's pixels and their canvas position.
    pub fn bake(&self, objects: &[(ElementRef, Element)]) -> Vec<(ElementRef, Element, RgbaImage, i64, i64)> {
        let ctx = firecraft_doc::RenderCtx {
            mx_perspective: self.doc.mx_perspective,
            symbols: &self.doc.symbols,
            frame: self.doc.current_frame,
            depth: 0,
            math: self.doc.color_math,
            chain: [0; firecraft_doc::MAX_SYMBOL_DEPTH as usize],
            canvas_textures: false,
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
        let method = self.interpolation;
        self.edit(label, |s| {
            for (r, orig) in items {
                if let Some(e) = s.element_mut(r) {
                    let mut t = orig;
                    t.transform_by(&m, method);
                    // Edited text is drawn again through its new matrix rather than resampled.
                    if let ElementKind::Text(text) = &mut t.kind
                        && text.edited
                    {
                        crate::text::refresh(text);
                    }
                    *e = t;
                }
            }
        });
    }

    /// The object that paint edits (fill, stroke, text) change for `r`: a masked object holding
    /// one object stands for that object, through nested ones, as in Fireworks MX (whose masked
    /// object is the object with a mask attached).
    pub fn paint_ref(&self, r: ElementRef) -> ElementRef {
        let mut cur = r;
        while let Some(ElementKind::Masked { content, .. }) = self.element(cur).map(|e| &e.kind) {
            match (content.len(), cur.child(0)) {
                (1, Some(c)) => cur = c,
                _ => break,
            }
        }
        cur
    }

    /// [`paint_ref`](Self::paint_ref) of each selected object.
    pub fn paint_refs(&self) -> Vec<ElementRef> {
        let mut out: Vec<ElementRef> = self.selection.iter().map(|r| self.paint_ref(*r)).collect();
        out.dedup();
        out
    }

    /// The object whose effects (and the Property inspector's effects list) count for `r`: the
    /// only object in a masked object's content, without effects of its own, shares the masked
    /// object's (Fireworks keeps a masked object's effects on its mask group), up through
    /// nested ones.
    pub fn effect_ref(&self, r: ElementRef) -> ElementRef {
        let mut cur = r;
        while let Some(p) = cur.parent() {
            let sole = !cur.is_mask()
                && self.element(cur).is_some_and(|e| e.effects.is_empty())
                && matches!(self.element(p).map(|e| &e.kind), Some(ElementKind::Masked { content, .. }) if content.len() == 1);
            if !sole {
                break;
            }
            cur = p;
        }
        cur
    }

    fn effect_refs(&self) -> Vec<ElementRef> {
        let mut out: Vec<ElementRef> = self.selection.iter().map(|r| self.effect_ref(*r)).collect();
        out.sort();
        out.dedup();
        out
    }

    /// The first selected object's Live Effects (what the Property inspector lists).
    pub fn selected_effects(&self) -> Option<Vec<firecraft_doc::EffectEntry>> {
        self.element(self.effect_ref(*self.selection.first()?)).map(|e| e.effects.clone())
    }

    /// The first selected object's pixels as they reach effect `index` of its list (the
    /// effects before it applied): what Levels' and Curves' histograms and Auto look at.
    pub fn effect_input(&self, index: usize) -> Option<RgbaImage> {
        let r = self.effect_ref(*self.selection.first()?);
        let mut e = self.element(r)?.clone();
        e.effects.truncate(index);
        self.bake(&[(r, e)]).into_iter().next().map(|(_, _, img, _, _)| img)
    }

    /// Replace the selected objects' effect lists (one undo step), as Fireworks' Property
    /// inspector does for every selected object.
    pub fn set_effects(&mut self, list: Vec<firecraft_doc::EffectEntry>, label: &str) {
        if self.selection.is_empty() {
            return;
        }
        let refs = self.effect_refs();
        self.edit(label, |s| {
            for r in refs {
                if let Some(e) = s.element_mut(r) {
                    set_element_effects(e, &list);
                }
            }
        });
    }

    /// Start a live edit (a drag, an effect's settings popup): later `..._live` changes apply
    /// at once, and [`commit_live_edit`](Self::commit_live_edit) records the undo step when it
    /// ends. As in Fireworks, the state appears in History on mouse up.
    /// Whether a live edit is under way.
    pub fn in_live_edit(&self) -> bool {
        self.live_edit.is_some()
    }

    pub fn begin_live_edit(&mut self, label: &str) {
        self.commit_live_edit();
        self.live_edit = Some(LiveEdit {
            label: label.to_owned(),
            before: self.doc.clone(),
            selected: self.selected_now(),
            dirty: self.dirty,
        });
    }

    /// End the live edit in progress: one undo step, even if the value ended where it began (as
    /// in Fireworks, which records the gesture).
    pub fn commit_live_edit(&mut self) {
        if let Some(e) = self.live_edit.take() {
            self.history.push(&e.label, &e.before, e.selected);
            self.dirty = true;
        }
    }

    /// Cancel the live edit in progress: the document returns to how it was when it began, with
    /// no undo step left behind.
    pub fn cancel_live_edit(&mut self) {
        if let Some(e) = self.live_edit.take() {
            self.doc = e.before;
            self.after_history();
            self.dirty = e.dirty;
        }
    }

    /// End a live edit that changed nothing, with no undo step.
    pub fn discard_live_edit(&mut self) {
        self.live_edit = None;
    }

    pub fn set_effects_live(&mut self, list: Vec<firecraft_doc::EffectEntry>) {
        let refs = self.effect_refs();
        for r in refs {
            if let Some(e) = self.element_mut(r) {
                set_element_effects(e, &list);
            }
        }
        self.touch();
    }

    /// Copies of the selected objects (for [`transform_from`](Self::transform_from)).
    /// What a transform changes, as it is now: the selected objects, a linked masked object
    /// whole for its content ([`motion_refs`](Self::motion_refs)).
    pub fn selected_elements(&self) -> Vec<(ElementRef, Element)> {
        self.motion_refs()
            .into_iter()
            .filter_map(|r| Some((r, self.element(r)?.clone())))
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
            pcl: None,
            texture_origin: None,
            contours: vec![Contour { points, closed: true }],
            fill: opts.new_fill((x0, y0, x1 - x0, y1 - y0)),
            stroke: opts.stroke.map(|color| opts.new_stroke(color)),
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
            stroke: Some(opts.new_stroke(opts.stroke.unwrap_or(Rgba::BLACK))),
            rect: None,
            pcl: None,
            texture_origin: None,
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
                s.selection = vec![ElementRef::new(layer, cell.len() - 1)];
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

    /// Pointer drag on empty canvas (rubber band from (`x0`, `y0`) to (`x1`, `y1`)): select every
    /// object whose bounds the band overlaps, as Fireworks MX does (measured 2026-11-15: a band
    /// over part of an object selects it, even over a corner of its box with none of its pixels,
    /// e.g. a sparse diagonal of dots; a band touching only its glow doesn't). `toggle` (Shift)
    /// adds the objects that weren't selected and removes those that were. Hidden or locked
    /// objects and layers are skipped. Not an undo step.
    pub fn select_in_rect(&mut self, (x0, y0, x1, y1): (f64, f64, f64, f64), toggle: bool) {
        let (l, t, r, b) = (x0.min(x1), y0.min(y1), x0.max(x1), y0.max(y1));
        let f = self.frame();
        let mut hits = Vec::new();
        for (li, layer) in self.doc.layers.iter().enumerate() {
            if !layer.visible || layer.locked || layer.web {
                continue;
            }
            for (ei, el) in layer.elements(f).iter().enumerate() {
                if !el.visible || el.locked {
                    continue;
                }
                if let Some((bx, by, bw, bh)) = el.bounds()
                    && bx < r
                    && bx + bw > l
                    && by < b
                    && by + bh > t
                {
                    hits.push(ElementRef::new(li, ei));
                }
            }
        }
        if toggle {
            for h in hits {
                if let Some(k) = self.selection.iter().position(|s| *s == h) {
                    self.selection.remove(k);
                } else {
                    self.selection.push(h);
                }
            }
        } else {
            self.selection = hits;
        }
        if let Some(top) = self.selection.iter().max() {
            self.doc.current_layer = top.layer;
        }
        self.pixels = None;
        self.revision = self.revision.wrapping_add(1);
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

    fn apply_fill_color(s: &mut DocState, refs: &[ElementRef], color: Option<Rgba>) {
        for &r in refs {
            // Text: the characters' colour (and a gradient's, as for a path). Text always has a
            // fill, so No Fill leaves it.
            if s.editable_text_at(r).is_some() {
                if let Some(c) = color {
                    s.change_text(r, |t| {
                        let n = crate::text::char_count(t);
                        crate::text::format(t, 0..n, &|run| run.color = c);
                        t.runs.iter_mut().for_each(|run| run.color = c);
                        if let Some(f) = &mut t.fill {
                            f.color = c;
                        }
                    });
                }
                continue;
            }
            if let Some(Element {
                kind: ElementKind::Path(p),
                ..
            }) = s.element_mut(r)
            {
                match (color, &mut p.fill) {
                    (None, f) => *f = None,
                    (Some(c), Some(f)) => {
                        f.color = c;
                        if f.dither.is_some() {
                            f.dither = Some(fills::dither_tile(c, f.dither_transparent));
                        }
                    }
                    (Some(c), f) => *f = Some(fills::solid_fill(c)),
                }
            }
        }
    }

    fn apply_stroke_color(s: &mut DocState, refs: &[ElementRef], color: Option<Rgba>) {
        for &r in refs {
            let Some(e) = s.element_mut(r) else { continue };
            let Some(slot) = e.stroke_slot_mut() else { continue };
            match (color, slot) {
                (None, st) => *st = None,
                (Some(c), Some(st)) => st.color = c,
                (Some(c), st) => {
                    *st = Some(Stroke::from_preset(
                        c,
                        firecraft_doc::brush_presets::default_index(),
                        1.0,
                    ))
                }
            }
            if let ElementKind::Text(t) = &mut e.kind {
                crate::text::restyle(t);
            }
        }
    }

    /// The selected paths' stroke and fill colours, in one History step.
    pub fn set_colors(&mut self, label: &str, stroke: Option<Rgba>, fill: Option<Rgba>) {
        let refs = self.paint_refs();
        self.edit(label, |s| {
            Self::apply_stroke_color(s, &refs, stroke);
            Self::apply_fill_color(s, &refs, fill);
        });
    }

    /// The selected paths' fill colour (`None`: no fill; a path without fill gets a solid one).
    pub fn set_fill_color(&mut self, color: Option<Rgba>) {
        let refs = self.paint_refs();
        self.edit(if color.is_some() { "Fill Color" } else { "No Fill" }, |s| {
            Self::apply_fill_color(s, &refs, color);
        });
    }

    /// The selected paths' stroke colour (`None`: no stroke; a path without stroke gets the
    /// 1-Pixel Anti-Aliased pencil).
    pub fn set_stroke_color(&mut self, color: Option<Rgba>) {
        let refs = self.paint_refs();
        self.edit(
            if color.is_some() {
                "Stroke Color"
            } else {
                "No Stroke"
            },
            |s| {
                Self::apply_stroke_color(s, &refs, color);
            },
        );
    }

    /// Change the stroke of the selected paths that have one (`live`: part of a slider drag
    /// begun with `begin_live_edit`, no undo step of its own).
    pub fn edit_stroke(&mut self, label: &str, live: bool, f: impl Fn(&mut Stroke)) {
        let apply = |s: &mut DocState| {
            for r in s.paint_refs() {
                let Some(e) = s.element_mut(r) else { continue };
                let Some(Some(st)) = e.stroke_slot_mut() else {
                    continue;
                };
                f(st);
                if let ElementKind::Text(t) = &mut e.kind {
                    crate::text::restyle(t);
                }
            }
        };
        if live {
            apply(self);
            self.touch();
        } else {
            self.edit(label, apply);
        }
    }

    /// Change the fill of the selected paths that have one (`live`: part of a slider drag begun
    /// with `begin_live_edit`, no undo step of its own).
    pub fn edit_fill(&mut self, label: &str, live: bool, f: impl Fn(&mut Fill)) {
        let apply = |s: &mut DocState| {
            for r in s.paint_refs() {
                let Some(e) = s.element_mut(r) else { continue };
                let Some(Some(fl)) = e.fill_slot_mut() else {
                    continue;
                };
                f(fl);
                if let ElementKind::Text(t) = &mut e.kind {
                    crate::text::restyle(t);
                }
            }
        };
        if live {
            apply(self);
            self.touch();
        } else {
            self.edit(label, apply);
        }
    }

    /// The first selected text object's or filled path's "Fill over stroke" (`FOTb`), if one is
    /// selected. Fireworks MX offers it for any path, but it only matters with a fill; a path
    /// keeps its setting while it has none (it's stored in its record).
    pub fn selected_fill_over_stroke(&self) -> Option<bool> {
        self.selection.iter().find_map(|r| {
            let e = self.element(*r)?;
            match &e.kind {
                ElementKind::Text(t) => Some(t.fill_over_stroke),
                ElementKind::Path(p) if p.fill.is_some() => {
                    Some(e.raw.as_ref().and_then(|n| n.bool_of("FOT")).unwrap_or(false))
                }
                _ => None,
            }
        })
    }

    /// "Fill over stroke" (Fireworks MX: draw the fill colour over the stroke) for the selected
    /// paths and text objects.
    pub fn set_fill_over_stroke(&mut self, on: bool) {
        let refs = self.paint_refs();
        self.edit("Fill over Stroke", |s| {
            for r in refs {
                let Some(e) = s.element_mut(r) else { continue };
                match &mut e.kind {
                    ElementKind::Text(t) => {
                        t.fill_over_stroke = on;
                        crate::text::refresh(t);
                    }
                    ElementKind::Path(_) => {
                        let raw = e
                            .raw
                            .get_or_insert_with(|| firecraft_fwpng::Node::vector("PTH", Vec::new()));
                        let flag = firecraft_fwpng::Node::bool("FOT", on);
                        match raw.children.iter_mut().find(|c| c.is("FOT")) {
                            Some(c) => *c = flag,
                            None => raw.children.push(flag),
                        }
                    }
                    _ => {}
                }
            }
        });
    }

    /// The selected paths' stroke preset (Property inspector stroke category); paths without a
    /// stroke get one in the stroke colour of the tools, as Fireworks does.
    pub fn set_stroke_preset(&mut self, index: usize, opts: &ToolOptions) {
        let refs = self.paint_refs();
        let color = opts.stroke.unwrap_or(Rgba::BLACK);
        self.edit("Stroke", |s| {
            for r in refs {
                let Some(e) = s.element_mut(r) else { continue };
                let Some(slot) = e.stroke_slot_mut() else { continue };
                match slot {
                    Some(st) => st.set_preset(index),
                    None => {
                        let d = firecraft_doc::brush_presets::BRUSH_PRESETS
                            .get(index)
                            .map_or(1.0, |b| b.diameter as f64);
                        *slot = Some(Stroke::from_preset(color, index, d));
                    }
                }
                // Textured presets need a texture (Fireworks always keeps one).
                if let Some(st) = slot
                    && st.texture.is_none()
                {
                    st.texture.clone_from(&opts.stroke_texture);
                }
                if let ElementKind::Text(t) = &mut e.kind {
                    crate::text::restyle(t);
                }
            }
        });
    }

    /// Set opacity (0..=100) on selected objects.
    /// The selected objects' opacity. `live`: part of a value drag (no undo step of its own).
    pub fn set_opacity(&mut self, opacity: f32, live: bool) {
        let apply = |s: &mut DocState| {
            for r in s.selection.clone() {
                if let Some(e) = s.element_mut(r) {
                    e.opacity = opacity.clamp(0.0, 100.0);
                }
            }
        };
        if live {
            apply(self);
            self.touch();
        } else {
            self.edit("Opacity", apply);
        }
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
    pub fn set_mx_perspective(&mut self, on: bool) {
        if self.doc.mx_perspective != on {
            self.edit("Perspective", |s| s.doc.mx_perspective = on);
        }
    }

    pub fn set_color_math(&mut self, math: firecraft_raster::ColorMath) {
        if self.doc.color_math != math {
            self.edit("Color Math", |s| s.doc.color_math = math);
        }
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
    use crate::PaintTool;

    fn undo_steps(d: &DocState) -> usize {
        d.history.labels().count()
    }

    #[test]
    fn a_dragged_value_is_one_undo_step_on_release() {
        let mut d = DocState::new(Document::new(50, 50, 72.0, Some(firecraft_raster::Rgba::WHITE)));
        d.add_shape((5.0, 5.0, 20.0, 20.0), false, &ToolOptions::default());
        let opacity = |d: &DocState| d.element(d.selection[0]).map(|e| e.opacity);
        let steps = undo_steps(&d);
        d.begin_live_edit("Opacity");
        for v in [90.0, 80.0, 70.0, 60.0] {
            d.set_opacity(v, true);
        }
        assert_eq!(opacity(&d), Some(60.0));
        assert_eq!(undo_steps(&d), steps, "nothing is recorded while dragging");
        d.commit_live_edit();
        assert_eq!(undo_steps(&d), steps + 1);
        d.undo();
        assert_eq!(opacity(&d), Some(100.0));
        // Typing a value is one step too.
        d.set_opacity(40.0, false);
        assert_eq!(undo_steps(&d), steps + 1);
    }

    #[test]
    fn a_drag_back_to_the_same_value_still_records_a_step() {
        // As in Fireworks MX: the gesture is recorded.
        let mut d = DocState::new(Document::new(50, 50, 72.0, Some(firecraft_raster::Rgba::WHITE)));
        d.add_shape((5.0, 5.0, 20.0, 20.0), false, &ToolOptions::default());
        let steps = undo_steps(&d);
        d.begin_live_edit("Opacity");
        d.set_opacity(50.0, true);
        d.set_opacity(100.0, true);
        d.commit_live_edit();
        assert_eq!(undo_steps(&d), steps + 1);
    }

    #[test]
    fn undo_or_another_edit_during_a_live_edit_commits_it_first() {
        let mut d = DocState::new(Document::new(50, 50, 72.0, Some(firecraft_raster::Rgba::WHITE)));
        d.add_shape((5.0, 5.0, 20.0, 20.0), false, &ToolOptions::default());
        let opacity = |d: &DocState| d.element(d.selection[0]).map(|e| e.opacity);
        d.begin_live_edit("Opacity");
        d.set_opacity(60.0, true);
        d.undo();
        assert_eq!(opacity(&d), Some(100.0));
        d.begin_live_edit("Opacity");
        d.set_opacity(60.0, true);
        d.set_opacity(30.0, false);
        let labels: Vec<&str> = d.history.labels().collect();
        assert_eq!(labels.iter().filter(|l| **l == "Opacity").count(), 2);
        d.undo();
        assert_eq!(opacity(&d), Some(60.0));
    }

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
    fn an_empty_bitmap_covers_the_canvas_and_goes_when_deselected_unused() {
        // Measured in Fireworks MX (Edit > Insert > Empty Bitmap on a 120 x 80 canvas): an image
        // at 0,0 120x80 on the current layer, selected, no undo step; leaving it empty removes it.
        let mut d = DocState::new(Document::new(120, 80, 72.0, Some(Rgba::WHITE)));
        d.add_shape((5.0, 5.0, 20.0, 20.0), false, &ToolOptions::default());
        let steps = undo_steps(&d);
        d.insert_empty_bitmap();
        assert_eq!(undo_steps(&d), steps, "no undo step");
        let r = d.selection[0];
        assert_eq!((r.layer, r.index), (0, 1), "on top of the current layer");
        let Some(ElementKind::Bitmap(b)) = d.element(r).map(|e| &e.kind) else {
            panic!("not a bitmap")
        };
        assert_eq!((b.x, b.y, b.image.width(), b.image.height()), (0, 0, 120, 80));
        assert!(b.image.pixels().iter().all(|p| p.a == 0));
        assert!(d.is_empty_bitmap(r));
        assert!(!d.selection_contains(60.0, 40.0), "a press on the canvas doesn't drag it");
        d.drop_empty_bitmaps();
        assert_eq!(d.doc.layers[0].elements(0).len(), 2, "kept while selected");
        d.selection.clear();
        d.drop_empty_bitmaps();
        assert_eq!(d.doc.layers[0].elements(0).len(), 1, "gone once deselected");
        assert_eq!(undo_steps(&d), steps);
    }

    #[test]
    fn dropping_an_empty_bitmap_keeps_other_references_right() {
        let mut d = DocState::new(Document::new(60, 40, 72.0, Some(Rgba::WHITE)));
        d.insert_empty_bitmap();
        d.add_shape((5.0, 5.0, 20.0, 20.0), false, &ToolOptions::default());
        // The rectangle went above the empty bitmap and is selected now.
        assert_eq!(d.selection[0].index, 1);
        d.drop_empty_bitmaps();
        assert_eq!(d.doc.layers[0].elements(0).len(), 1);
        assert_eq!(d.selection[0].index, 0);
        assert!(matches!(
            d.element(d.selection[0]).map(|e| &e.kind),
            Some(ElementKind::Path(_))
        ));
    }

    #[test]
    fn an_empty_bitmap_goes_on_an_editable_layer() {
        let mut d = DocState::new(Document::new(60, 40, 72.0, Some(Rgba::WHITE)));
        d.new_layer();
        let top = d.doc.current_layer;
        d.doc.layers[top].locked = true;
        d.insert_empty_bitmap();
        assert_ne!(d.selection[0].layer, top);
        assert_eq!(d.doc.current_layer, d.selection[0].layer);
    }

    /// Two overlapping bitmaps on two layers, as in the Fireworks MX measurement: red at
    /// 10,10 100x80 on Layer 1, blue at 80,50 100x80 on Layer 2, on a 200 x 150 canvas.
    fn red_and_blue() -> (DocState, ElementRef, ElementRef) {
        let mut d = DocState::new(Document::new(200, 150, 72.0, None));
        let bitmap = |x, y, c| {
            Element::new(ElementKind::Bitmap(Bitmap {
                x,
                y,
                image: RgbaImage::new(100, 80, c),
            }))
        };
        d.add_element(bitmap(10, 10, Rgba::new(255, 0, 0, 255)), "Add");
        let red = d.selection[0];
        d.new_layer();
        d.add_element(bitmap(80, 50, Rgba::new(0, 0, 255, 255)), "Add");
        let blue = d.selection[0];
        (d, red, blue)
    }

    fn pixel(d: &DocState, r: ElementRef, x: i64, y: i64) -> Rgba {
        match d.element(r).map(|e| &e.kind) {
            Some(ElementKind::Bitmap(b)) => b.image.get(x - b.x, y - b.y).unwrap_or(Rgba::TRANSPARENT),
            _ => panic!("not a bitmap"),
        }
    }

    #[test]
    fn a_marquee_carries_over_to_the_bitmap_picked_in_the_layers_panel() {
        // Fireworks MX (2026-11-15): marquee on red ("Select Pixels"), click blue's row in the
        // Layers panel ("Edit Bitmap": the marquee stays), Clear removes blue's pixels inside it.
        let (mut d, red, blue) = red_and_blue();
        d.selection = vec![red];
        d.marquee((60.0, 30.0, 140.0, 110.0), false, &ToolOptions::default(), Combine::Replace);
        assert_eq!(d.history.next_undo(), Some("Select Pixels"));
        assert!(d.edit_bitmap(blue));
        assert_eq!(d.history.next_undo(), Some("Edit Bitmap"));
        assert_eq!(d.pixels.as_ref().and_then(|p| p.target), Some(blue));
        assert_eq!(d.doc.current_layer, blue.layer);
        d.delete();
        assert_eq!(pixel(&d, blue, 100, 70).a, 0, "blue cleared inside");
        assert_eq!(pixel(&d, blue, 150, 120).a, 255, "blue kept outside");
        assert_eq!(pixel(&d, red, 70, 40).a, 255, "red untouched");
        assert!(d.pixels.is_some(), "the marquee stays after Clear");
        // The same selection on red next.
        assert!(d.edit_bitmap(red));
        d.delete();
        assert_eq!(pixel(&d, red, 70, 40).a, 0);
        assert_eq!(pixel(&d, red, 20, 20).a, 255);
    }

    #[test]
    fn undo_brings_back_the_marquee_and_the_bitmap_it_was_on() {
        let (mut d, red, blue) = red_and_blue();
        d.selection = vec![red];
        d.marquee((60.0, 30.0, 140.0, 110.0), false, &ToolOptions::default(), Combine::Replace);
        let marquee = d.pixels.clone();
        d.edit_bitmap(blue);
        d.delete();
        // Undo Delete: the pixels come back and the marquee is still on blue.
        d.undo();
        assert_eq!(pixel(&d, blue, 100, 70).a, 255);
        assert_eq!(d.pixels.as_ref().and_then(|p| p.target), Some(blue));
        // Undo Edit Bitmap: back on red, same marquee.
        d.undo();
        assert_eq!(d.pixels.as_ref().and_then(|p| p.target), Some(red));
        assert_eq!(d.pixels.as_ref().map(|p| &p.mask), marquee.as_ref().map(|p| &p.mask));
        assert_eq!(d.selection, vec![red]);
        // Undo Select Pixels: no marquee.
        d.undo();
        assert!(d.pixels.is_none());
        // And redo all the way.
        d.redo();
        d.redo();
        d.redo();
        assert_eq!(pixel(&d, blue, 100, 70).a, 0);
        assert_eq!(d.pixels.as_ref().and_then(|p| p.target), Some(blue));
    }

    #[test]
    fn picking_the_bitmap_already_edited_or_a_layer_with_a_marquee() {
        let (mut d, red, _) = red_and_blue();
        d.selection = vec![red];
        d.marquee((60.0, 30.0, 140.0, 110.0), false, &ToolOptions::default(), Combine::Replace);
        let steps = undo_steps(&d);
        assert!(d.edit_bitmap(red));
        assert_eq!(undo_steps(&d), steps, "no step when nothing changes");
        // Clicking a layer's name ends the marquee (Fireworks MX: "Layer Change").
        d.set_current_layer(1);
        assert!(d.pixels.is_none());
        assert_eq!(d.history.next_undo(), Some("Layer Change"));
        d.undo();
        assert!(d.pixels.is_some());
        assert_eq!(d.doc.current_layer, 0);
    }

    #[test]
    fn deselecting_pixels_is_an_undo_step() {
        let (mut d, red, _) = red_and_blue();
        d.selection = vec![red];
        d.marquee((20.0, 20.0, 40.0, 40.0), false, &ToolOptions::default(), Combine::Replace);
        d.deselect();
        assert!(d.pixels.is_none());
        assert_eq!(d.history.next_undo(), Some("Deselect"));
        d.undo();
        assert!(d.pixels.is_some());
    }

    #[test]
    fn a_marquee_reaches_past_the_bitmap_it_was_drawn_on() {
        // Drawn over red, but covering part of blue that red doesn't: that part counts on blue.
        let (mut d, red, blue) = red_and_blue();
        d.selection = vec![red];
        d.marquee((100.0, 60.0, 160.0, 100.0), false, &ToolOptions::default(), Combine::Replace);
        d.edit_bitmap(blue);
        d.delete();
        assert_eq!(pixel(&d, blue, 150, 90).a, 0, "beyond red's right edge");
    }

    #[test]
    fn adding_to_a_marquee_grows_it() {
        let (mut d, red, _) = red_and_blue();
        d.selection = vec![red];
        let o = ToolOptions::default();
        d.marquee((20.0, 20.0, 30.0, 30.0), false, &o, Combine::Replace);
        d.marquee((60.0, 60.0, 70.0, 70.0), false, &o, Combine::Add);
        let sel = d.pixels.clone().unwrap();
        let local = sel.local(0, 0, 200, 150);
        assert_eq!(local.get(25, 25), 255);
        assert_eq!(local.get(65, 65), 255);
        assert_eq!(local.get(45, 45), 0);
        d.marquee((20.0, 20.0, 30.0, 30.0), false, &o, Combine::Subtract);
        let local = d.pixels.as_ref().unwrap().local(0, 0, 200, 150);
        assert_eq!(local.get(25, 25), 0);
        assert_eq!(local.get(65, 65), 255);
    }

    fn fill_options() -> ToolOptions {
        ToolOptions {
            fill: Some(Rgba::new(0, 0, 255, 255)),
            stroke: Some(Rgba::new(0, 0, 0, 255)),
            ..ToolOptions::default()
        }
    }

    #[test]
    fn a_marquee_on_empty_canvas_fills_into_a_new_bitmap() {
        // Fireworks MX (2026-11-15), nothing selected: a marquee from 120,10 to 190,45 on empty
        // canvas ("Select Pixels"), then the Paint Bucket inside it ("Fill"): a new bitmap on
        // top of the current layer, being edited, the marquee still there.
        let (mut d, red, blue) = red_and_blue();
        d.selection.clear();
        let o = fill_options();
        d.marquee((120.0, 10.0, 190.0, 45.0), false, &o, Combine::Replace);
        let sel = d.pixels.clone().expect("a marquee without a bitmap");
        assert_eq!(sel.target, None);
        assert_eq!(d.history.next_undo(), Some("Select Pixels"));
        let before = d.doc.layers[blue.layer].elements(0).len();
        d.paint_bucket(150.0, 25.0, &o);
        let cell = d.doc.layers[blue.layer].elements(0);
        assert_eq!(cell.len(), before + 1, "a new bitmap on the current layer");
        let new = ElementRef::new(blue.layer, cell.len() - 1);
        let Some(ElementKind::Bitmap(b)) = d.element(new).map(|e| &e.kind) else {
            panic!("not a bitmap")
        };
        assert_eq!((b.x, b.y, b.image.width(), b.image.height()), (120, 10, 70, 35));
        assert!(b.image.pixels().iter().all(|p| *p == Rgba::new(0, 0, 255, 255)));
        assert_eq!(d.pixels.as_ref().and_then(|p| p.target), Some(new));
        assert_eq!(d.selection, vec![new]);
        assert_eq!(pixel(&d, red, 20, 20).a, 255, "red untouched");
        // Undo the fill: no new bitmap, the marquee is back without one.
        d.undo();
        assert_eq!(d.doc.layers[blue.layer].elements(0).len(), before);
        assert_eq!(d.pixels.as_ref().map(|p| p.target), Some(None));
    }

    #[test]
    fn a_marquee_off_the_edited_bitmap_fills_into_it() {
        // Fireworks MX: red being edited, a marquee in an empty area (10,100 to 60,140), Paint
        // Bucket: the fill goes into red (MX grows it to the canvas while editing).
        let (mut d, red, _) = red_and_blue();
        d.selection = vec![red];
        let o = fill_options();
        d.marquee((10.0, 100.0, 60.0, 140.0), false, &o, Combine::Replace);
        assert_eq!(d.pixels.as_ref().and_then(|p| p.target), Some(red));
        let objects = d.doc.layers.iter().map(|l| l.elements(0).len()).sum::<usize>();
        d.paint_bucket(30.0, 120.0, &o);
        assert_eq!(d.doc.layers.iter().map(|l| l.elements(0).len()).sum::<usize>(), objects);
        let Some(ElementKind::Bitmap(b)) = d.element(red).map(|e| &e.kind) else {
            panic!("not a bitmap")
        };
        // Red's old rectangle and the filled area.
        assert_eq!((b.x, b.y, b.image.width(), b.image.height()), (10, 10, 100, 130));
        assert_eq!(pixel(&d, red, 30, 120), Rgba::new(0, 0, 255, 255));
        assert_eq!(pixel(&d, red, 20, 20), Rgba::new(255, 0, 0, 255));
        assert_eq!(pixel(&d, red, 80, 120).a, 0, "outside the marquee");
    }

    #[test]
    fn a_stroke_in_a_marquee_on_empty_canvas_makes_a_bitmap_inside_it() {
        let mut d = DocState::new(Document::new(100, 80, 72.0, None));
        let o = fill_options();
        d.marquee((20.0, 20.0, 40.0, 40.0), false, &o, Combine::Replace);
        assert!(d.begin_paint(PaintTool::Pencil, 5.0, 30.0, &o));
        d.extend_paint(90.0, 30.0, false);
        d.end_paint();
        let r = d.selection[0];
        let Some(ElementKind::Bitmap(b)) = d.element(r).map(|e| &e.kind) else {
            panic!("not a bitmap")
        };
        assert_eq!((b.x, b.y, b.image.height()), (20, 30, 1), "only inside the marquee");
        assert_eq!(b.image.width(), 20);
        assert_eq!(d.pixels.as_ref().and_then(|p| p.target), Some(r));
    }

    #[test]
    fn exiting_bitmap_mode_ends_the_marquee_and_keeps_the_bitmap_selected() {
        // Fireworks MX (2026-11-15): with a marquee on red, choosing the Pointer tool or pressing
        // Escape removes the marquee, leaves red selected, and records "Exit Bitmap Mode".
        let (mut d, red, _) = red_and_blue();
        d.selection = vec![red];
        d.marquee((20.0, 20.0, 40.0, 40.0), false, &ToolOptions::default(), Combine::Replace);
        assert!(d.exit_bitmap_mode());
        assert!(d.pixels.is_none());
        assert_eq!(d.selection, vec![red]);
        assert_eq!(d.history.next_undo(), Some("Exit Bitmap Mode"));
        d.undo();
        assert!(d.pixels.is_some(), "undo brings the marquee back");
        // Nothing being edited: nothing happens, nothing recorded.
        d.exit_bitmap_mode();
        let steps = undo_steps(&d);
        assert!(!d.exit_bitmap_mode());
        assert_eq!(undo_steps(&d), steps);
    }

    #[test]
    fn exiting_bitmap_mode_removes_an_unused_empty_bitmap() {
        // Fireworks MX: Insert > Empty Bitmap, then leaving bitmap mode: the empty image goes,
        // with no undo step.
        let mut d = DocState::new(Document::new(60, 40, 72.0, None));
        let steps = undo_steps(&d);
        d.insert_empty_bitmap();
        assert!(d.exit_bitmap_mode());
        assert!(d.selection.is_empty());
        assert!(d.doc.layers[0].elements(0).is_empty());
        assert_eq!(undo_steps(&d), steps);
    }

    #[test]
    fn rectangle_marquees_are_whole_pixels_and_overlapping_fills_leave_no_seam() {
        // Fireworks MX (2026-11-15), Anti-Alias on: a marquee from canvas 20,20 to 60,50 and the
        // Paint Bucket fill exactly pixels 20..59 x 20..49, opaque to the edge; a second marquee
        // 40,35 to 90,70 filled the same way meets the first without a seam. Zoomed in, the
        // pointer is between grid lines: each corner goes to the nearest one, as the Crop
        // tool's.
        let mut d = DocState::new(Document::new(200, 150, 72.0, None));
        let o = ToolOptions {
            fill: Some(Rgba::new(153, 153, 153, 255)),
            ..ToolOptions::default()
        };
        assert_ne!(o.marquee_edge, firecraft_raster::Edge::Hard, "anti-aliased by default");
        d.marquee((20.3, 19.6, 60.4, 49.7), false, &o, Combine::Replace);
        d.paint_bucket(30.0, 30.0, &o);
        d.marquee((89.6, 70.3, 40.2, 35.4), false, &o, Combine::Replace);
        d.paint_bucket(80.0, 60.0, &o);
        let img = d.doc.render(0);
        let a = |x: i64, y: i64| img.get(x, y).map_or(0, |p| p.a);
        assert_eq!((a(19, 30), a(20, 30), a(59, 30), a(60, 30)), (0, 255, 255, 0));
        // Where the second fill meets the first (MX: 60,40 and 60,50 opaque).
        assert_eq!((a(59, 40), a(60, 40), a(60, 50), a(45, 35)), (255, 255, 255, 255));
        assert_eq!((a(30, 19), a(30, 20), a(30, 49), a(30, 50)), (0, 255, 255, 0));
        assert_eq!((a(89, 60), a(90, 60), a(45, 69), a(45, 70)), (255, 0, 255, 0));
        assert!(img.pixels().iter().all(|p| p.a == 0 || p.a == 255), "no soft edges or seams");
    }

    #[test]
    fn marquee_box_takes_the_nearest_grid_lines_in_any_drag_direction() {
        assert_eq!(DocState::marquee_box((20.4, 30.6, 10.5, 5.2)), (11.0, 5.0, 20.0, 31.0));
        // The box the canvas shows is the one selected.
        let (mut d, red, _) = red_and_blue();
        d.selection = vec![red];
        d.marquee((20.4, 30.6, 10.5, 5.2), false, &ToolOptions::default(), Combine::Replace);
        assert!(d.pixels_contain(11.5, 30.5) && d.pixels_contain(19.5, 10.5));
        assert!(!d.pixels_contain(10.5, 20.0) && !d.pixels_contain(20.5, 20.0) && !d.pixels_contain(15.0, 31.5));
    }

    #[test]
    fn a_rectangle_scaled_to_whole_pixels_keeps_hard_edges() {
        // The Scale tool lands on whole pixels by default; a sub-pixel width (Ctrl) softens the
        // right edge.
        let mut d = DocState::new(Document::new(120, 80, 72.0, None));
        let o = ToolOptions {
            fill: Some(Rgba::new(0, 0, 255, 255)),
            stroke: None,
            ..ToolOptions::default()
        };
        d.add_shape((10.0, 10.0, 50.0, 40.0), false, &o);
        let from = d.selection_bounds().unwrap();
        assert!((from.0 - 10.0).abs() < 1e-6 && (from.2 - 40.0).abs() < 1e-6);
        let start = d.selected_elements();
        let hard = |d: &DocState| d.doc.render(0).pixels().iter().all(|p| p.a == 0 || p.a == 255);
        d.resize_from(&start, from, (61.0, 37.0), false);
        assert!(hard(&d), "whole-pixel size: no soft edges");
        d.resize_from(&start, from, (61.4, 37.0), false);
        assert!(!hard(&d), "sub-pixel size: soft edge");
        // A whole-pixel move keeps the sub-pixel look; it doesn't snap the edge back.
        let before: Vec<u8> = d.doc.render(0).pixels().iter().map(|p| p.a).collect();
        d.move_selection(3.0, 2.0, "Transform");
        let after = d.doc.render(0);
        assert_eq!(after.get(10 + 3 + 61, 25 + 2).map(|p| p.a), before.get(25 * 120 + 10 + 61).copied());
    }

    #[test]
    fn covered_pixels_counts_partly_covered_ones() {
        assert_eq!(covered_pixels((10.0, 10.0, 50.0, 30.0)), (10.0, 10.0, 50.0, 30.0));
        assert_eq!(covered_pixels((10.0, 10.0, 50.1, 30.0)).2, 51.0);
        assert_eq!(covered_pixels((10.5, 10.0, 20.0, 30.0)), (10.0, 10.0, 21.0, 30.0));
        assert_eq!(covered_pixels((10.0, 9.999999999999996, 40.000000000000014, 30.000000000000018)), (10.0, 10.0, 40.0, 30.0));
        assert_eq!(covered_pixels((-3.5, 0.0, 1.0, 0.2)), (-4.0, 0.0, 2.0, 1.0));
    }

    #[test]
    fn inspector_width_and_height_scale_from_the_top_left() {
        let (mut d, red, blue) = red_and_blue();
        d.selection = vec![red];
        let start = d.selected_elements();
        let from = d.element(red).and_then(|e| e.bounds()).unwrap();
        // A drag on W: each step scales the originals, one undo step at the end.
        d.begin_live_edit("Transform");
        d.resize_from(&start, from, (150.0, from.3), true);
        d.resize_from(&start, from, (50.0, from.3), true);
        d.commit_live_edit();
        assert_eq!(d.element(red).and_then(|e| e.bounds()), Some((10.0, 10.0, 50.0, 80.0)));
        assert_eq!(d.history.next_undo(), Some("Transform"));
        // Typed H on both objects: their joint box keeps its top-left corner.
        d.selection = vec![red, blue];
        let from = d.selection_bounds().unwrap();
        assert_eq!(from, (10.0, 10.0, 170.0, 120.0));
        let now = d.selected_elements();
        d.resize_from(&now, from, (170.0, 60.0), false);
        assert_eq!(d.selection_bounds(), Some((10.0, 10.0, 170.0, 60.0)));
        assert_eq!(d.element(blue).and_then(|e| e.bounds()), Some((80.0, 30.0, 100.0, 40.0)));
        // Below one pixel nothing happens.
        d.resize_from(&now, from, (0.0, 60.0), false);
        assert_eq!(d.selection_bounds(), Some((10.0, 10.0, 170.0, 60.0)));
    }

    #[test]
    fn dragging_the_marquee_moves_it_not_the_pixels() {
        // Fireworks MX: pressed inside the marquee with a marquee tool or the Magic Wand and
        // dragged, the marquee moves ("Move"); the pixels stay.
        let (mut d, red, _) = red_and_blue();
        d.selection = vec![red];
        d.marquee((20.0, 20.0, 40.0, 40.0), false, &ToolOptions::default(), Combine::Replace);
        assert!(d.pixels_contain(25.0, 25.0));
        assert!(!d.pixels_contain(45.0, 25.0));
        let before = d.doc.clone();
        d.move_pixel_selection(20, 10);
        assert_eq!(d.history.next_undo(), Some("Move"));
        assert_eq!(d.doc, before, "pixels untouched");
        assert!(d.pixels_contain(45.0, 35.0));
        assert!(!d.pixels_contain(25.0, 25.0));
        d.undo();
        assert!(d.pixels_contain(25.0, 25.0));
    }

    #[test]
    fn a_marquee_click_and_the_wand_record_fireworks_labels() {
        let (mut d, red, _) = red_and_blue();
        d.selection = vec![red];
        let o = ToolOptions::default();
        d.magic_wand(20.0, 20.0, &o, Combine::Replace);
        assert_eq!(d.history.next_undo(), Some("Magic Wand Tool"));
        // A click with a marquee tool (or the wand inside the selection) ends it: "Select Pixels".
        d.deselect_pixels();
        assert!(d.pixels.is_none());
        assert_eq!(d.history.next_undo(), Some("Select Pixels"));
    }

    #[test]
    fn a_new_bitmap_image_takes_over_the_marquee() {
        let (mut d, red, _) = red_and_blue();
        d.selection = vec![red];
        let o = fill_options();
        d.marquee((20.0, 20.0, 40.0, 40.0), false, &o, Combine::Replace);
        d.insert_empty_bitmap();
        let new = d.selection[0];
        assert_ne!(new, red);
        assert_eq!(d.pixels.as_ref().and_then(|p| p.target), Some(new));
        // Filling goes into the new bitmap, trimmed to the marquee; red is untouched.
        d.paint_bucket(30.0, 30.0, &o);
        let Some(ElementKind::Bitmap(b)) = d.element(new).map(|e| &e.kind) else {
            panic!("not a bitmap")
        };
        assert_eq!((b.x, b.y, b.image.width(), b.image.height()), (20, 20, 20, 20));
        assert_eq!(pixel(&d, red, 30, 30), Rgba::new(255, 0, 0, 255));
    }

    #[test]
    fn new_bitmap_via_cut_and_copy_keep_the_marquee_on_the_new_bitmap() {
        // Fireworks MX (2026-11-15): red edited, marquee 30,20 to 90,70 over red and blue,
        // context menu > New Bitmap Via Cut: red's pixels in the marquee become a new bitmap
        // right above red, being edited, the marquee still there; blue untouched.
        let (mut d, red, blue) = red_and_blue();
        d.selection = vec![red];
        d.marquee((30.0, 20.0, 90.0, 70.0), false, &ToolOptions::default(), Combine::Replace);
        d.bitmap_via(true);
        assert_eq!(d.history.next_undo(), Some("New Bitmap Via Cut"));
        let new = ElementRef::new(red.layer, red.index + 1);
        assert_eq!(d.selection, vec![new]);
        assert_eq!(d.pixels.as_ref().and_then(|p| p.target), Some(new));
        let Some(ElementKind::Bitmap(b)) = d.element(new).map(|e| &e.kind) else {
            panic!("not a bitmap")
        };
        // Red covers 10..110 x 10..90: the cut is red's part of the marquee.
        assert_eq!((b.x, b.y, b.image.width(), b.image.height()), (30, 20, 60, 50));
        assert_eq!(pixel(&d, red, 50, 40).a, 0, "cut out of red");
        assert_eq!(pixel(&d, blue, 85, 60).a, 255, "blue untouched");
        // Copy from the new bitmap: the marquee follows again, the source keeps its pixels.
        d.bitmap_via(false);
        assert_eq!(d.history.next_undo(), Some("New Bitmap Via Copy"));
        assert_eq!(pixel(&d, new, 50, 40).a, 255);
        let newer = ElementRef::new(red.layer, red.index + 2);
        assert_eq!(d.pixels.as_ref().and_then(|p| p.target), Some(newer));
    }

    fn bounds_of(d: &DocState, r: ElementRef) -> (i64, i64, u32, u32) {
        match d.element(r).map(|e| &e.kind) {
            Some(ElementKind::Bitmap(b)) => (b.x, b.y, b.image.width(), b.image.height()),
            _ => panic!("not a bitmap"),
        }
    }

    #[test]
    fn deleting_selected_pixels_shrinks_the_bitmap_to_what_is_left() {
        // Fireworks MX (2026-11-15): red at 10,10 100x80, marquee 0,0 to 60,100, Delete
        // ("Delete"): red ends up 60,10 50x80.
        let (mut d, red, _) = red_and_blue();
        d.selection = vec![red];
        d.marquee((0.0, 0.0, 60.0, 100.0), false, &ToolOptions::default(), Combine::Replace);
        d.delete();
        assert_eq!(d.history.next_undo(), Some("Delete"));
        assert_eq!(bounds_of(&d, red), (60, 10, 50, 80));
        assert_eq!(pixel(&d, red, 60, 10), Rgba::new(255, 0, 0, 255));
        assert!(d.pixels.is_some(), "the marquee stays");
        d.undo();
        assert_eq!(bounds_of(&d, red), (10, 10, 100, 80));
        // A hole in the middle leaves the bounds as they are.
        d.marquee((40.0, 30.0, 60.0, 50.0), false, &ToolOptions::default(), Combine::Replace);
        d.delete();
        assert_eq!(bounds_of(&d, red), (10, 10, 100, 80));
        assert_eq!(pixel(&d, red, 50, 40).a, 0);
    }

    #[test]
    fn cutting_selected_pixels_shrinks_the_bitmap_too() {
        let (mut d, red, _) = red_and_blue();
        d.selection = vec![red];
        d.marquee((0.0, 0.0, 200.0, 50.0), false, &ToolOptions::default(), Combine::Replace);
        d.delete_as("Cut");
        assert_eq!(bounds_of(&d, red), (10, 50, 100, 40));
        d.undo();
        d.bitmap_via(true);
        assert_eq!(bounds_of(&d, red), (10, 50, 100, 40), "Bitmap Via Cut's source");
    }

    #[test]
    fn a_bitmap_emptied_by_delete_goes_when_bitmap_editing_ends() {
        // Fireworks MX: all of blue selected and deleted; at Exit Bitmap Mode blue is gone and
        // nothing is selected. Undo brings it back.
        let (mut d, _, blue) = red_and_blue();
        d.selection = vec![blue];
        d.marquee((0.0, 0.0, 200.0, 150.0), false, &ToolOptions::default(), Combine::Replace);
        d.delete();
        d.drop_empty_bitmaps();
        assert_eq!(d.doc.layers[blue.layer].elements(0).len(), 1, "kept while being edited");
        assert!(d.exit_bitmap_mode());
        assert!(d.doc.layers[blue.layer].elements(0).is_empty());
        assert!(d.selection.is_empty());
        d.undo();
        assert_eq!(d.doc.layers[blue.layer].elements(0).len(), 1);
        d.undo();
        assert_eq!(pixel(&d, blue, 100, 70).a, 255, "undo Delete: blue is back");
    }

    /// The flat red/blue picture as Fireworks MX opens it: one 200x150 bitmap at 0,0 whose
    /// pixels cover 10,10 170x120 (transparent margins around them).
    fn flat_red_and_blue() -> (DocState, ElementRef) {
        let mut img = RgbaImage::new(200, 150, Rgba::TRANSPARENT);
        for y in 0..150 {
            for x in 0..200 {
                if (80..180).contains(&x) && (50..130).contains(&y) {
                    img.set(x, y, Rgba::new(0, 0, 255, 255));
                } else if (10..110).contains(&x) && (10..90).contains(&y) {
                    img.set(x, y, Rgba::new(255, 0, 0, 255));
                }
            }
        }
        let d = DocState::new(Document::from_image(img, None));
        (d, ElementRef::new(0, 0))
    }

    #[test]
    fn strokes_shrink_a_bitmap_to_its_pixels_margins_included() {
        // Fireworks MX (2026-11-15): a Brush or an Eraser stroke inside red, then leaving bitmap
        // editing, turned the 200x150 bitmap into 10,10 170x120 ("Brush Tool", "Eraser Tool").
        let o = fill_options();
        for tool in [PaintTool::Pencil, PaintTool::Eraser] {
            let (mut d, r) = flat_red_and_blue();
            d.selection = vec![r];
            assert_eq!(bounds_of(&d, r), (0, 0, 200, 150));
            assert!(d.begin_paint(tool, 40.0, 40.0, &o));
            d.extend_paint(60.0, 40.0, false);
            d.end_paint();
            assert_eq!(bounds_of(&d, r), (10, 10, 170, 120), "{tool:?}");
        }
        let (mut d, r) = flat_red_and_blue();
        d.selection = vec![r];
        d.begin_paint(PaintTool::Eraser, 40.0, 40.0, &o);
        d.end_paint();
        assert_eq!(d.history.next_undo(), Some("Eraser Tool"));
    }

    #[test]
    fn exit_bitmap_mode_shrinks_the_edited_bitmap_even_unchanged() {
        // Fireworks MX: a Magic Wand click on red, then Escape: 10,10 170x120 ("Exit Bitmap
        // Mode"); undo gives the margins back.
        let (mut d, r) = flat_red_and_blue();
        d.selection = vec![r];
        d.magic_wand(50.0, 40.0, &ToolOptions::default(), Combine::Replace);
        assert_eq!(bounds_of(&d, r), (0, 0, 200, 150));
        d.dirty = false;
        assert!(d.exit_bitmap_mode());
        assert_eq!(bounds_of(&d, r), (10, 10, 170, 120));
        assert!(d.dirty);
        assert_eq!(d.history.next_undo(), Some("Exit Bitmap Mode"));
        d.undo();
        assert_eq!(bounds_of(&d, r), (0, 0, 200, 150));
    }

    #[test]
    fn a_fill_shrinks_the_bitmap_and_is_called_fill() {
        let (mut d, r) = flat_red_and_blue();
        d.selection = vec![r];
        d.paint_bucket(50.0, 40.0, &fill_options());
        assert_eq!(d.history.next_undo(), Some("Fill"));
        assert_eq!(bounds_of(&d, r), (10, 10, 170, 120));
    }

    #[test]
    fn a_pointer_band_selects_what_it_overlaps() {
        // Fireworks MX (2026-11-15): a band from 2,2 to 115,95 selects red (inside) and blue
        // (only partly covered); 2,2 to 60,60 selects red; a Shift band toggles.
        let (mut d, red, blue) = red_and_blue();
        d.select_in_rect((2.0, 2.0, 115.0, 95.0), false);
        assert_eq!(d.selection.len(), 2);
        assert!(d.selection.contains(&red) && d.selection.contains(&blue));
        d.select_in_rect((2.0, 2.0, 60.0, 60.0), false);
        assert_eq!(d.selection, vec![red]);
        d.select_in_rect((193.0, 146.0, 83.0, 61.0), true);
        assert_eq!(d.selection, vec![blue], "red toggled off, blue on");
        // A band over empty canvas only selects nothing.
        d.select_in_rect((185.0, 5.0, 195.0, 40.0), false);
        assert!(d.selection.is_empty());
    }

    #[test]
    fn a_band_over_an_empty_corner_of_the_box_selects_sparse_pixels() {
        // glowpx.png: a diagonal of single pixels in a 129x169 bitmap at 704,423. MX selects it
        // with a band in the empty top-left corner of its box.
        let mut img = RgbaImage::new(129, 169, Rgba::TRANSPARENT);
        for i in 0..20 {
            img.set(i * 6, 168 - i * 8, Rgba::new(0, 0, 0, 255));
        }
        let mut d = DocState::new(Document::new(1920, 1080, 72.0, None));
        d.add_element(
            Element::new(ElementKind::Bitmap(Bitmap { x: 704, y: 423, image: img })),
            "Add",
        );
        let r = d.selection[0];
        d.selection.clear();
        d.select_in_rect((706.0, 424.0, 736.0, 456.0), false);
        assert_eq!(d.selection, vec![r]);
        // Left of the box (where only a glow would reach): nothing.
        d.select_in_rect((688.0, 568.0, 700.0, 600.0), false);
        assert!(d.selection.is_empty());
    }

    #[test]
    fn double_click_marquee_delete_exit_flow() {
        // Fireworks MX (2026-11-15): Pointer double-click on red -> bitmap mode, Marquee tool
        // ("Edit Bitmap"); marquee and Delete ("Delete"); the stop button -> "Exit Bitmap Mode",
        // red still selected with its hole, the tool unchanged.
        let (mut d, red, blue) = red_and_blue();
        assert!(!d.in_bitmap_mode());
        assert!(d.enter_bitmap_mode(red));
        assert_eq!(d.history.next_undo(), Some("Edit Bitmap"));
        assert!(d.in_bitmap_mode());
        assert_eq!(d.selection, vec![red]);
        assert_eq!(d.doc.current_layer, red.layer);
        // The marquee applies to the bitmap being edited.
        d.marquee((0.0, 0.0, 60.0, 100.0), false, &ToolOptions::default(), Combine::Replace);
        assert_eq!(d.pixels.as_ref().and_then(|p| p.target), Some(red));
        d.delete();
        assert_eq!(bounds_of(&d, red), (60, 10, 50, 80));
        assert!(d.exit_bitmap_mode());
        assert!(!d.in_bitmap_mode());
        assert_eq!(d.selection, vec![red]);
        assert_eq!(d.history.next_undo(), Some("Exit Bitmap Mode"));
        // Undo walks back into bitmap mode.
        d.undo();
        assert!(d.in_bitmap_mode() && d.pixels.is_some());
        // Double-clicked but nothing done: exiting is still one step, and only bitmaps count.
        let (mut d, red, _) = red_and_blue();
        d.enter_bitmap_mode(red);
        assert!(d.exit_bitmap_mode());
        assert!(!d.in_bitmap_mode());
        assert!(!d.enter_bitmap_mode(ElementRef::new(5, 0)));
        let _ = blue;
    }

    #[test]
    fn bitmap_mode_ends_when_the_bitmap_is_no_longer_selected() {
        let (mut d, red, blue) = red_and_blue();
        d.enter_bitmap_mode(red);
        d.selection = vec![blue];
        d.drop_empty_bitmaps();
        assert!(!d.in_bitmap_mode());
        assert_eq!(d.bitmap_edit, None);
        // And picking another bitmap in the Layers panel while editing moves the editing there.
        d.enter_bitmap_mode(red);
        assert!(d.edit_bitmap(blue));
        assert_eq!(d.bitmap_edit, Some(blue));
        assert!(d.in_bitmap_mode());
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
        assert_eq!(d.hit_test(30.0, 20.0), Some(ElementRef::new(0, 0)));
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
        assert_eq!(d.hit_test(45.0, 45.0), Some(ElementRef::new(0, 0)));
        assert_eq!(d.hit_test(5.0, 5.0), Some(ElementRef::new(0, 1)));
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
        assert_eq!(d.selection, vec![ElementRef::new(0, 1)]);
        d.select_behind(45.0, 45.0);
        assert_eq!(d.selection, vec![ElementRef::new(0, 0)]);
        d.select_behind(45.0, 45.0);
        assert_eq!(d.selection, vec![ElementRef::new(0, 1)]);
        assert!(d.selection_contains(50.0, 50.0));
    }
}
