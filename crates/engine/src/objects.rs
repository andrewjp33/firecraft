//! Object commands: the clipboard, Duplicate/Clone, Group/Ungroup, Arrange, Align, Transform
//! (flip, rotate, numeric), and reordering and renaming from the Layers panel.
//!
//! Selections are kept in stacking order where it matters (bottom first): layers bottom to top,
//! then objects bottom to top within a layer.

use std::collections::BTreeSet;

use firecraft_doc::{Bitmap, Document, Element, ElementKind, Symbol};
use firecraft_raster::transform::Affine;
use firecraft_raster::{Rgba, RgbaImage};

use crate::{DocState, ElementRef};

/// Copied objects, with the symbols their instances use (so they render when pasted into
/// another document) and the tiles their original Fireworks data refers to (textures, cached
/// images: Fireworks drops objects whose tiles are missing).
#[derive(Clone, Debug, Default)]
pub struct Clipboard {
    /// Bottom first.
    pub elements: Vec<Element>,
    pub symbols: Vec<Symbol>,
    pub tiles: Vec<firecraft_fwpng::Tile>,
}

/// Modify > Arrange.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrange {
    BringToFront,
    BringForward,
    SendBackward,
    SendToBack,
}

/// Modify > Align (Fireworks' names: "Center Vertical" lines centres up on a vertical axis).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    CenterVertical,
    Right,
    Top,
    CenterHorizontal,
    Bottom,
    DistributeWidths,
    DistributeHeights,
}

/// Modify > Transform commands with a fixed result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Turn {
    Rotate180,
    Rotate90Cw,
    Rotate90Ccw,
    FlipHorizontal,
    FlipVertical,
}

/// Duplicate's offset from the originals (Fireworks MX: 10 px right and down).
pub const DUPLICATE_OFFSET: f64 = 10.0;

impl DocState {
    /// The selected objects in stacking order (bottom first).
    pub fn stacking_selection(&self) -> Vec<ElementRef> {
        let mut refs = self.selection.clone();
        refs.sort_by_key(|r| (r.layer, r.index));
        refs.dedup();
        refs.retain(|r| self.element(*r).is_some());
        refs
    }

    /// The selected pixels as a new bitmap object at their place (Copy with a pixel selection,
    /// Bitmap via Copy).
    pub(crate) fn selected_pixels(&self) -> Option<Element> {
        let sel = self.pixels.as_ref()?;
        let Some(Element {
            kind: ElementKind::Bitmap(src),
            ..
        }) = self.element(sel.target)
        else {
            return None;
        };
        let (bx, by, bw, bh) = sel.mask.bounds()?;
        let full = src.image.extract_masked(&sel.mask);
        let mut piece = RgbaImage::new(bw, bh, Rgba::TRANSPARENT);
        piece.draw(&full, -(bx as i64), -(by as i64), 255);
        Some(Element::new(ElementKind::Bitmap(Bitmap {
            x: src.x + bx as i64,
            y: src.y + by as i64,
            image: piece,
        })))
    }

    /// Edit > Copy: the selected pixels (as a bitmap) or the selected objects.
    pub fn copy(&self) -> Option<Clipboard> {
        let elements: Vec<Element> = match self.pixels {
            Some(_) => vec![self.selected_pixels()?],
            None => self
                .stacking_selection()
                .iter()
                .filter_map(|r| self.element(*r).cloned())
                .collect(),
        };
        if elements.is_empty() {
            return None;
        }
        let symbols = used_symbols(&elements, &self.doc);
        let mut ids = Vec::new();
        elements
            .iter()
            .for_each(|e| firecraft_doc::tile_refs(e, &mut ids));
        for sym in &symbols {
            sym.layers
                .iter()
                .flat_map(|l| l.cells.iter().flatten())
                .for_each(|e| firecraft_doc::tile_refs(e, &mut ids));
        }
        ids.sort_unstable();
        ids.dedup();
        let tiles = ids
            .iter()
            .filter_map(|id| self.doc.raw_tiles.get(id).cloned())
            .collect();
        Some(Clipboard {
            elements,
            symbols,
            tiles,
        })
    }

    /// Edit > Cut: copy, then delete what was copied.
    pub fn cut(&mut self) -> Option<Clipboard> {
        let clip = self.copy()?;
        self.delete_as("Cut");
        Some(clip)
    }

    /// Edit > Paste: the clipboard's objects on top of the current layer, at their original
    /// places, selected.
    pub fn paste(&mut self, clip: &Clipboard) {
        if clip.elements.is_empty() {
            return;
        }
        let layer = self.writable_layer();
        let mut clip = clip.clone();
        // Symbols from elsewhere (another document, Fireworks) may use ids this document gives
        // to other symbols: renumber those. The same id and name is taken as the same symbol.
        let mut next = self.doc.symbols.iter().map(|x| x.id).max().unwrap_or(0) + 1;
        let clashes: Vec<(u32, u32)> = clip
            .symbols
            .iter()
            .filter(|sym| {
                self.doc
                    .symbols
                    .iter()
                    .any(|x| x.id == sym.id && x.name != sym.name)
            })
            .map(|sym| {
                next += 1;
                (sym.id, next - 1)
            })
            .collect();
        for &(from, to) in &clashes {
            clip.elements.iter_mut().for_each(|e| renumber(e, from, to));
            for sym in &mut clip.symbols {
                if sym.id == from {
                    sym.id = to;
                }
                for layer in &mut sym.layers {
                    layer
                        .cells
                        .iter_mut()
                        .flatten()
                        .for_each(|e| renumber(e, from, to));
                }
            }
        }
        // Tiles: keep identical ones, renumber those whose ids this document uses otherwise.
        let mut next_tile = self
            .doc
            .raw_tiles
            .keys()
            .chain(clip.tiles.iter().map(|t| &t.id))
            .copied()
            .max()
            .unwrap_or(0x0100_0000)
            .saturating_add(0x10)
            & !0xF;
        let mut tile_map = std::collections::HashMap::new();
        for t in &mut clip.tiles {
            if self
                .doc
                .raw_tiles
                .get(&t.id)
                .is_some_and(|mine| mine.data != t.data)
            {
                tile_map.insert(t.id, next_tile);
                t.id = next_tile;
                next_tile += 0x10;
            }
        }
        if !tile_map.is_empty() {
            clip.elements
                .iter_mut()
                .for_each(|e| firecraft_doc::remap_tiles(e, &tile_map));
            for sym in &mut clip.symbols {
                sym.layers
                    .iter_mut()
                    .flat_map(|l| l.cells.iter_mut().flatten())
                    .for_each(|e| firecraft_doc::remap_tiles(e, &tile_map));
            }
        }
        self.edit("Paste", |s| {
            if !clip.tiles.is_empty() {
                let tiles = std::sync::Arc::make_mut(&mut s.doc.raw_tiles);
                for t in &clip.tiles {
                    tiles.entry(t.id).or_insert_with(|| t.clone());
                }
            }
            for sym in &clip.symbols {
                if !s.doc.symbols.iter().any(|x| x.id == sym.id) {
                    std::sync::Arc::make_mut(&mut s.doc.symbols).push(sym.clone());
                }
            }
            let f = s.frame();
            if let Some(cell) = s.doc.layers.get_mut(layer).and_then(|l| l.elements_mut(f)) {
                let start = cell.len();
                cell.extend(clip.elements.iter().cloned());
                s.selection = (start..cell.len())
                    .map(|index| ElementRef { layer, index })
                    .collect();
            }
            s.doc.current_layer = layer;
            s.pixels = None;
        });
    }

    /// Edit > Duplicate (`offset` = true: 10 px right and down) and Edit > Clone (in place):
    /// copies just above the topmost selected object of each layer, selected.
    pub fn duplicate(&mut self, offset: bool) {
        let refs = self.stacking_selection();
        if refs.is_empty() || self.pixels.is_some() {
            return;
        }
        let label = if offset { "Duplicate" } else { "Clone" };
        self.edit(label, |s| {
            let f = s.frame();
            let mut selection = Vec::new();
            let layers: BTreeSet<usize> = refs.iter().map(|r| r.layer).collect();
            for li in layers {
                let mine: Vec<usize> = refs.iter().filter(|r| r.layer == li).map(|r| r.index).collect();
                let Some(cell) = s.doc.layers.get_mut(li).and_then(|l| l.elements_mut(f)) else {
                    continue;
                };
                let mut copies: Vec<Element> = mine.iter().filter_map(|&i| cell.get(i).cloned()).collect();
                if offset {
                    copies
                        .iter_mut()
                        .for_each(|e| e.translate(DUPLICATE_OFFSET, DUPLICATE_OFFSET));
                }
                let at = mine.iter().max().map_or(cell.len(), |&i| i + 1).min(cell.len());
                let n = copies.len();
                cell.splice(at..at, copies);
                selection.extend((at..at + n).map(|index| ElementRef { layer: li, index }));
            }
            s.selection = selection;
        });
    }

    /// Remove `refs` (stacking order) from the document, returning them in the same order.
    pub(crate) fn take(&mut self, refs: &[ElementRef]) -> Vec<Element> {
        let f = self.frame();
        let mut out = Vec::with_capacity(refs.len());
        for r in refs.iter().rev() {
            if let Some(cell) = self.doc.layers.get_mut(r.layer).and_then(|l| l.elements_mut(f))
                && r.index < cell.len()
            {
                out.push(cell.remove(r.index));
            }
        }
        out.reverse();
        out
    }

    /// Modify > Group: the selected objects become one group, placed where the topmost of them
    /// was.
    pub fn group(&mut self) {
        let refs = self.stacking_selection();
        let Some(&top) = refs.last() else { return };
        if self.pixels.is_some() {
            return;
        }
        self.edit("Group", |s| {
            let children = s.take(&refs);
            // Objects removed below the topmost one in its layer shift it down.
            let below = refs
                .iter()
                .filter(|r| r.layer == top.layer && r.index < top.index)
                .count();
            let at = top.index - below;
            let f = s.frame();
            if let Some(cell) = s.doc.layers.get_mut(top.layer).and_then(|l| l.elements_mut(f)) {
                let at = at.min(cell.len());
                cell.insert(at, Element::new(ElementKind::Group(children)));
                s.selection = vec![ElementRef {
                    layer: top.layer,
                    index: at,
                }];
            }
            s.doc.current_layer = top.layer;
        });
    }

    /// Whether Ungroup has something to do.
    pub fn can_ungroup(&self) -> bool {
        self.selection
            .iter()
            .any(|r| matches!(self.element(*r).map(|e| &e.kind), Some(ElementKind::Group(_))))
    }

    /// Modify > Ungroup: selected groups are replaced by their objects (selected) in place.
    pub fn ungroup(&mut self) {
        if !self.can_ungroup() {
            return;
        }
        let selected: BTreeSet<ElementRef> = self.selection.iter().copied().collect();
        self.edit("Ungroup", |s| {
            let f = s.frame();
            let mut selection = Vec::new();
            for (li, layer) in s.doc.layers.iter_mut().enumerate() {
                let Some(cell) = layer.elements_mut(f) else {
                    continue;
                };
                if !selected.iter().any(|r| r.layer == li) {
                    continue;
                }
                let old = std::mem::take(cell);
                for (i, e) in old.into_iter().enumerate() {
                    let sel = selected.contains(&ElementRef { layer: li, index: i });
                    match e.kind {
                        ElementKind::Group(children) if sel => {
                            for c in children {
                                selection.push(ElementRef {
                                    layer: li,
                                    index: cell.len(),
                                });
                                cell.push(c);
                            }
                        }
                        kind => {
                            if sel {
                                selection.push(ElementRef {
                                    layer: li,
                                    index: cell.len(),
                                });
                            }
                            cell.push(Element { kind, ..e });
                        }
                    }
                }
            }
            s.selection = selection;
        });
    }

    /// Modify > Arrange: move the selected objects up or down within their layers.
    pub fn arrange(&mut self, how: Arrange) {
        let refs = self.stacking_selection();
        if refs.is_empty() {
            return;
        }
        let label = match how {
            Arrange::BringToFront => "Bring to Front",
            Arrange::BringForward => "Bring Forward",
            Arrange::SendBackward => "Send Backward",
            Arrange::SendToBack => "Send to Back",
        };
        self.edit(label, |s| {
            let f = s.frame();
            let mut selection = Vec::new();
            let layers: BTreeSet<usize> = refs.iter().map(|r| r.layer).collect();
            for li in layers {
                let Some(cell) = s.doc.layers.get_mut(li).and_then(|l| l.elements_mut(f)) else {
                    continue;
                };
                let mut items: Vec<(Element, bool)> = std::mem::take(cell)
                    .into_iter()
                    .enumerate()
                    .map(|(i, e)| (e, refs.contains(&ElementRef { layer: li, index: i })))
                    .collect();
                match how {
                    // Stable partitions keep the selected objects' own order.
                    Arrange::BringToFront => items.sort_by_key(|(_, sel)| *sel),
                    Arrange::SendToBack => items.sort_by_key(|(_, sel)| !*sel),
                    Arrange::BringForward => {
                        for i in (0..items.len().saturating_sub(1)).rev() {
                            let sel = |k: usize| items.get(k).is_some_and(|x| x.1);
                            if sel(i) && !sel(i + 1) {
                                items.swap(i, i + 1);
                            }
                        }
                    }
                    Arrange::SendBackward => {
                        for i in 1..items.len() {
                            let sel = |k: usize| items.get(k).is_some_and(|x| x.1);
                            if sel(i) && !sel(i - 1) {
                                items.swap(i, i - 1);
                            }
                        }
                    }
                }
                for (i, (e, sel)) in items.into_iter().enumerate() {
                    if sel {
                        selection.push(ElementRef { layer: li, index: i });
                    }
                    cell.push(e);
                }
            }
            s.selection = selection;
        });
    }

    /// Modify > Align. Several objects line up with each other (their combined bounds); a
    /// single object lines up with the canvas. Distribute spaces three or more objects evenly
    /// between the outermost ones (equal gaps).
    pub fn align(&mut self, how: Align) {
        let refs = self.stacking_selection();
        let items: Vec<(ElementRef, (f64, f64, f64, f64))> = refs
            .iter()
            .filter_map(|r| Some((*r, self.element(*r)?.bounds()?)))
            .collect();
        if items.is_empty() {
            return;
        }
        let area = if items.len() == 1 {
            (0.0, 0.0, self.doc.width as f64, self.doc.height as f64)
        } else {
            union(items.iter().map(|(_, b)| *b))
        };
        let (ax, ay, aw, ah) = area;
        let mut moves: Vec<(ElementRef, f64, f64)> = Vec::new();
        match how {
            Align::DistributeWidths | Align::DistributeHeights => {
                if items.len() < 3 {
                    return;
                }
                let horizontal = how == Align::DistributeWidths;
                let pos = |b: &(f64, f64, f64, f64)| if horizontal { (b.0, b.2) } else { (b.1, b.3) };
                let mut sorted = items.clone();
                sorted.sort_by(|a, b| {
                    let (pa, sa) = pos(&a.1);
                    let (pb, sb) = pos(&b.1);
                    (pa + sa / 2.0).total_cmp(&(pb + sb / 2.0))
                });
                let total: f64 = sorted.iter().map(|(_, b)| pos(b).1).sum();
                let (start, span) = if horizontal { (ax, aw) } else { (ay, ah) };
                let gap = (span - total) / (sorted.len() - 1) as f64;
                let mut at = start;
                for (r, b) in &sorted {
                    let (p, size) = pos(b);
                    let d = at - p;
                    moves.push(if horizontal { (*r, d, 0.0) } else { (*r, 0.0, d) });
                    at += size + gap;
                }
            }
            _ => {
                for (r, (x, y, w, h)) in &items {
                    let (dx, dy) = match how {
                        Align::Left => (ax - x, 0.0),
                        Align::CenterVertical => (ax + aw / 2.0 - (x + w / 2.0), 0.0),
                        Align::Right => (ax + aw - (x + w), 0.0),
                        Align::Top => (0.0, ay - y),
                        Align::CenterHorizontal => (0.0, ay + ah / 2.0 - (y + h / 2.0)),
                        Align::Bottom => (0.0, ay + ah - (y + h)),
                        _ => (0.0, 0.0),
                    };
                    moves.push((*r, dx, dy));
                }
            }
        }
        moves.retain(|(_, dx, dy)| dx.abs() > 1e-9 || dy.abs() > 1e-9);
        if moves.is_empty() {
            return;
        }
        self.edit("Align", |s| {
            for (r, dx, dy) in moves {
                if let Some(e) = s.element_mut(r) {
                    e.translate(dx, dy);
                }
            }
        });
    }

    /// Modify > Transform > Rotate 180°/90° CW/90° CCW, Flip Horizontal/Vertical, about the
    /// selection's centre. Whole-pixel results stay whole-pixel (bitmaps are moved, not
    /// resampled).
    pub fn turn(&mut self, how: Turn) {
        let Some((x, y, w, h)) = self.selection_bounds() else {
            return;
        };
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let (lin, label) = match how {
            Turn::Rotate180 => (
                Affine {
                    a: -1.0,
                    d: -1.0,
                    ..Affine::IDENTITY
                },
                "Rotate 180°",
            ),
            Turn::Rotate90Cw => (
                Affine {
                    a: 0.0,
                    b: 1.0,
                    c: -1.0,
                    d: 0.0,
                    ..Affine::IDENTITY
                },
                "Rotate 90° CW",
            ),
            Turn::Rotate90Ccw => (
                Affine {
                    a: 0.0,
                    b: -1.0,
                    c: 1.0,
                    d: 0.0,
                    ..Affine::IDENTITY
                },
                "Rotate 90° CCW",
            ),
            Turn::FlipHorizontal => (
                Affine {
                    a: -1.0,
                    ..Affine::IDENTITY
                },
                "Flip Horizontal",
            ),
            Turn::FlipVertical => (
                Affine {
                    d: -1.0,
                    ..Affine::IDENTITY
                },
                "Flip Vertical",
            ),
        };
        let m = about(lin, cx, cy);
        // Keep the move whole pixels so the top-left stays on the pixel grid.
        let (nx, ny, _, _) = m.map_rect(x, y, w, h);
        let snap = Affine::translate(((nx - x).round()) - (nx - x), ((ny - y).round()) - (ny - y));
        let m = snap.then(&m);
        let originals = self.selected_elements();
        self.transform_from(&originals, &m, label);
    }

    /// Modify > Transform > Numeric Transform: scale (x, y as fractions) and rotate (degrees,
    /// clockwise) about the selection's centre.
    pub fn numeric_transform(&mut self, sx: f64, sy: f64, degrees: f64) {
        let Some((x, y, w, h)) = self.selection_bounds() else {
            return;
        };
        if !(sx.is_finite() && sy.is_finite() && degrees.is_finite()) || sx == 0.0 || sy == 0.0 {
            return;
        }
        let (s, c) = degrees.to_radians().sin_cos();
        // y points down: a clockwise turn on screen.
        let lin = Affine {
            a: c * sx,
            b: s * sx,
            c: -s * sy,
            d: c * sy,
            ..Affine::IDENTITY
        };
        let m = about(lin, x + w / 2.0, y + h / 2.0);
        let originals = self.selected_elements();
        self.transform_from(&originals, &m, "Transform");
    }

    /// Layers panel: move the selected objects to `layer`, inserted at `index` of that layer
    /// as it is now (before the move); they keep their stacking order and stay selected.
    pub fn move_selection_to(&mut self, layer: usize, index: usize) {
        let refs = self.stacking_selection();
        if refs.is_empty() || self.doc.layers.get(layer).is_none_or(|l| l.locked) {
            return;
        }
        // Dropping the selection onto itself changes nothing.
        let f = self.frame();
        let before = refs
            .iter()
            .filter(|r| r.layer == layer && r.index < index)
            .count();
        let at = index - before;
        let n = refs.len();
        let unchanged =
            refs.iter().all(|r| r.layer == layer) && refs.iter().enumerate().all(|(k, r)| r.index == at + k);
        if unchanged {
            return;
        }
        let label = if refs.iter().all(|r| r.layer == layer) {
            "Reorder"
        } else {
            "Move to Layer"
        };
        self.edit(label, |s| {
            let moved = s.take(&refs);
            if let Some(cell) = s.doc.layers.get_mut(layer).and_then(|l| l.elements_mut(f)) {
                let at = at.min(cell.len());
                cell.splice(at..at, moved);
                s.selection = (at..at + n).map(|index| ElementRef { layer, index }).collect();
            }
            s.doc.current_layer = layer;
        });
    }

    /// Layers panel: move layer `from` so it ends up at position `to` (indices bottom first;
    /// the Web Layer stays on top).
    pub fn move_layer(&mut self, from: usize, to: usize) {
        let n = self.doc.layers.len();
        let web_top = self.doc.layers.iter().position(|l| l.web).unwrap_or(n);
        if from >= n || self.doc.layers.get(from).is_some_and(|l| l.web) {
            return;
        }
        let to = to.min(web_top.saturating_sub(1)).min(n - 1);
        if to == from {
            return;
        }
        self.edit("Move Layer", |s| {
            let l = s.doc.layers.remove(from);
            s.doc.layers.insert(to, l);
            // Layer indices shift: remap the selection and the current layer.
            let map = |i: usize| -> usize {
                if i == from {
                    to
                } else if from < to && i > from && i <= to {
                    i - 1
                } else if to < from && i >= to && i < from {
                    i + 1
                } else {
                    i
                }
            };
            for r in &mut s.selection {
                r.layer = map(r.layer);
            }
            if let Some(p) = &mut s.pixels {
                p.target.layer = map(p.target.layer);
            }
            s.doc.current_layer = map(s.doc.current_layer);
        });
    }

    /// Rename a layer (Layers panel double-click).
    pub fn rename_layer(&mut self, i: usize, name: &str) {
        let name = name.trim();
        if name.is_empty() || self.doc.layers.get(i).is_none_or(|l| l.name == name) {
            return;
        }
        let name = name.to_owned();
        self.edit("Rename Layer", |s| {
            if let Some(l) = s.doc.layers.get_mut(i) {
                l.name = name;
            }
        });
    }

    /// Rename an object (empty: back to its kind's name).
    pub fn rename_object(&mut self, r: ElementRef, name: &str) {
        let name = Some(name.trim().to_owned()).filter(|n| !n.is_empty());
        if self.element(r).is_none_or(|e| e.name == name) {
            return;
        }
        self.edit("Rename Object", |s| {
            if let Some(e) = s.element_mut(r) {
                e.name = name;
            }
        });
    }
}

/// Point instances of symbol `from` at `to` instead.
fn renumber(e: &mut Element, from: u32, to: u32) {
    match &mut e.kind {
        ElementKind::Instance { symbol, .. } if *symbol == from => *symbol = to,
        ElementKind::Group(children) => children.iter_mut().for_each(|c| renumber(c, from, to)),
        ElementKind::Masked {
            content, mask, cache, ..
        } => {
            content.iter_mut().for_each(|c| renumber(c, from, to));
            renumber(mask, from, to);
            if let Some(c) = cache {
                renumber(c, from, to);
            }
        }
        _ => {}
    }
}

/// `lin` (a linear map) about the point (`cx`, `cy`).
fn about(lin: Affine, cx: f64, cy: f64) -> Affine {
    Affine::translate(cx, cy)
        .then(&lin)
        .then(&Affine::translate(-cx, -cy))
}

fn union(mut it: impl Iterator<Item = (f64, f64, f64, f64)>) -> (f64, f64, f64, f64) {
    let Some(first) = it.next() else {
        return (0.0, 0.0, 0.0, 0.0);
    };
    let (x0, y0, x1, y1) = it.fold(
        (first.0, first.1, first.0 + first.2, first.1 + first.3),
        |a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.0 + b.2), a.3.max(b.1 + b.3)),
    );
    (x0, y0, x1 - x0, y1 - y0)
}

/// The symbols `elements` use, directly or through other symbols.
fn used_symbols(elements: &[Element], doc: &Document) -> Vec<Symbol> {
    fn ids(e: &Element, out: &mut Vec<u32>) {
        match &e.kind {
            ElementKind::Instance { symbol, .. } => out.push(*symbol),
            ElementKind::Group(children) => children.iter().for_each(|c| ids(c, out)),
            ElementKind::Masked {
                content, mask, cache, ..
            } => {
                content.iter().for_each(|c| ids(c, out));
                ids(mask, out);
                if let Some(c) = cache {
                    ids(c, out);
                }
            }
            _ => {}
        }
    }
    let mut todo = Vec::new();
    elements.iter().for_each(|e| ids(e, &mut todo));
    let mut found: Vec<Symbol> = Vec::new();
    while let Some(id) = todo.pop() {
        if found.iter().any(|s| s.id == id) {
            continue;
        }
        if let Some(sym) = doc.symbols.iter().find(|s| s.id == id) {
            for layer in &sym.layers {
                for cell in &layer.cells {
                    cell.iter().for_each(|e| ids(e, &mut todo));
                }
            }
            found.push(sym.clone());
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document with one bitmap per colour (1-wide columns 10 px apart), selected none.
    fn doc(colours: &[u8]) -> DocState {
        let mut d = DocState::new(Document::new(100, 100, 72.0, Some(Rgba::WHITE)));
        for (i, &c) in colours.iter().enumerate() {
            let img = RgbaImage::new(4, 3, Rgba::new(c, 0, 0, 255));
            let mut e = Element::new(ElementKind::Bitmap(Bitmap {
                x: 10 * i as i64,
                y: 5,
                image: img,
            }));
            e.name = Some(format!("e{i}"));
            d.add_element(e, "Add");
        }
        d.selection.clear();
        d
    }

    fn names(d: &DocState) -> Vec<String> {
        d.doc.layers[0].elements(0).iter().map(|e| e.label()).collect()
    }

    fn sel(d: &mut DocState, idx: &[usize]) {
        d.selection = idx.iter().map(|&index| ElementRef { layer: 0, index }).collect();
    }

    #[test]
    fn group_and_ungroup_keep_stacking() {
        let mut d = doc(&[1, 2, 3]);
        sel(&mut d, &[2, 0]);
        d.group();
        assert_eq!(names(&d), ["e1", "Group"]);
        assert_eq!(d.selection, [ElementRef { layer: 0, index: 1 }]);
        assert!(d.can_ungroup());
        d.ungroup();
        assert_eq!(names(&d), ["e1", "e0", "e2"]);
        assert_eq!(d.selection.len(), 2);
        d.undo();
        d.undo();
        assert_eq!(names(&d), ["e0", "e1", "e2"]);
    }

    #[test]
    fn arrange_moves_within_the_layer() {
        let mut d = doc(&[1, 2, 3, 4]);
        sel(&mut d, &[0]);
        d.arrange(Arrange::BringForward);
        assert_eq!(names(&d), ["e1", "e0", "e2", "e3"]);
        d.arrange(Arrange::BringToFront);
        assert_eq!(names(&d), ["e1", "e2", "e3", "e0"]);
        assert_eq!(d.selection, [ElementRef { layer: 0, index: 3 }]);
        sel(&mut d, &[1, 2]);
        d.arrange(Arrange::SendToBack);
        assert_eq!(names(&d), ["e2", "e3", "e1", "e0"]);
        d.arrange(Arrange::BringForward);
        assert_eq!(names(&d), ["e1", "e2", "e3", "e0"]);
        sel(&mut d, &[3]);
        d.arrange(Arrange::SendBackward);
        assert_eq!(names(&d), ["e1", "e2", "e0", "e3"]);
    }

    #[test]
    fn duplicate_clone_and_paste() {
        let mut d = doc(&[1, 2]);
        sel(&mut d, &[0]);
        d.duplicate(true);
        assert_eq!(names(&d), ["e0", "e0", "e1"]);
        assert_eq!(
            d.element(ElementRef { layer: 0, index: 1 })
                .and_then(Element::bounds),
            Some((10.0, 15.0, 4.0, 3.0))
        );
        d.duplicate(false);
        assert_eq!(names(&d), ["e0", "e0", "e0", "e1"]);
        // Copy from this document, paste into another: on top, same place.
        sel(&mut d, &[3]);
        let clip = d.copy().expect("copied");
        let mut other = doc(&[9]);
        other.paste(&clip);
        assert_eq!(names(&other), ["e0", "e1"]);
        assert_eq!(other.selection, [ElementRef { layer: 0, index: 1 }]);
        // Cut removes it and records "Cut".
        let n = names(&d).len();
        assert!(d.cut().is_some());
        assert_eq!(names(&d).len(), n - 1);
        assert_eq!(d.history.next_undo(), Some("Cut"));
    }

    #[test]
    fn align_and_distribute() {
        let mut d = doc(&[1, 2, 3]);
        // Spread them unevenly first: x = 0, 10, 40.
        sel(&mut d, &[2]);
        d.move_selection(20.0, 7.0, "Move");
        sel(&mut d, &[0, 1, 2]);
        d.align(Align::Top);
        let b: Vec<_> = (0..3)
            .filter_map(|i| d.element(ElementRef { layer: 0, index: i })?.bounds())
            .collect();
        assert!(b.iter().all(|b| b.1 == 5.0), "{b:?}");
        d.align(Align::DistributeWidths);
        let xs: Vec<f64> = (0..3)
            .filter_map(|i| Some(d.element(ElementRef { layer: 0, index: i })?.bounds()?.0))
            .collect();
        assert_eq!(xs, [0.0, 20.0, 40.0]);
        // One object aligns with the canvas.
        sel(&mut d, &[1]);
        d.align(Align::Right);
        assert_eq!(
            d.element(ElementRef { layer: 0, index: 1 })
                .and_then(Element::bounds)
                .map(|b| b.0),
            Some(96.0)
        );
    }

    #[test]
    fn turns_are_pixel_exact() {
        let mut d = DocState::new(Document::new(50, 50, 72.0, None));
        let mut img = RgbaImage::new(3, 2, Rgba::TRANSPARENT);
        for (i, (x, y)) in [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)]
            .into_iter()
            .enumerate()
        {
            img.set(x, y, Rgba::new(40 * i as u8, 0, 0, 255));
        }
        d.add_element(
            Element::new(ElementKind::Bitmap(Bitmap {
                x: 10,
                y: 10,
                image: img.clone(),
            })),
            "Add",
        );
        let bitmap = |d: &DocState| match &d
            .element(ElementRef { layer: 0, index: 0 })
            .map(|e| e.kind.clone())
        {
            Some(ElementKind::Bitmap(b)) => b.clone(),
            _ => panic!("not a bitmap"),
        };
        d.turn(Turn::Rotate90Cw);
        let b = bitmap(&d);
        assert_eq!((b.image.width(), b.image.height()), (2, 3));
        // Clockwise: the old bottom-left pixel is now top-left.
        assert_eq!(b.image.get(0, 0), img.get(0, 1));
        assert_eq!(b.image.get(1, 0), img.get(0, 0));
        assert_eq!(b.image.get(1, 2), img.get(2, 0));
        d.turn(Turn::Rotate90Ccw);
        let b = bitmap(&d);
        assert_eq!((b.x, b.y), (10, 10));
        assert_eq!(b.image.pixels(), img.pixels());
        d.turn(Turn::FlipHorizontal);
        let b = bitmap(&d);
        assert_eq!((b.x, b.y, b.image.get(0, 0)), (10, 10, img.get(2, 0)));
    }

    #[test]
    fn layers_panel_moves() {
        let mut d = doc(&[1, 2, 3]);
        d.new_layer();
        let top = d.doc.current_layer;
        sel(&mut d, &[0, 2]);
        d.move_selection_to(top, 0);
        assert_eq!(names(&d), ["e1"]);
        assert_eq!(
            d.doc.layers[top]
                .elements(0)
                .iter()
                .map(|e| e.label())
                .collect::<Vec<_>>(),
            ["e0", "e2"]
        );
        assert_eq!(d.selection.len(), 2);
        d.move_layer(top, 0);
        assert_eq!(d.doc.layers[0].elements(0).len(), 2);
        assert!(d.selection.iter().all(|r| r.layer == 0));
        d.rename_layer(0, "Moved");
        assert_eq!(d.doc.layers[0].name, "Moved");
        d.rename_object(ElementRef { layer: 0, index: 0 }, "first");
        assert_eq!(d.doc.layers[0].elements(0)[0].label(), "first");
    }
}
