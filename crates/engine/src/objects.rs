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
        refs.sort();
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
        }) = sel.target.and_then(|t| self.element(t))
        else {
            return None;
        };
        let local = sel.on(src);
        let (bx, by, bw, bh) = local.bounds()?;
        let full = src.image.extract_masked(&local);
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
        // A mask can be copied, but stays (Modify > Mask > Delete Mask removes it).
        if self.pixels.is_none() && self.selection.iter().any(ElementRef::is_mask) {
            return None;
        }
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
        let clip = self.prepared(clip);
        self.edit("Paste", |s| {
            s.adopt(&clip);
            let f = s.frame();
            if let Some(cell) = s.doc.layers.get_mut(layer).and_then(|l| l.elements_mut(f)) {
                let start = cell.len();
                cell.extend(clip.elements.iter().cloned());
                s.selection = (start..cell.len())
                    .map(|index| ElementRef::new(layer, index))
                    .collect();
            }
            s.doc.current_layer = layer;
            s.pixels = None;
        });
    }

    /// The clipboard made ready for this document: symbols and tiles renumbered where their
    /// ids clash with this document's.
    pub(crate) fn prepared(&self, clip: &Clipboard) -> Clipboard {
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
        // In a Symbol Editor, an instance that would make the edited symbol contain itself
        // (Symbol 3 pasted into Symbol 2, which Symbol 3 is made of): as Fireworks MX does, the
        // clipboard's symbols that lead back to it are pasted as copies (same names, new ids,
        // contents as copied), so nothing contains itself (verified: mechscene2_loop.png).
        if let Some(link) = self.symbol_link {
            // The clipboard's symbols, with the document's for any nested ones it doesn't carry.
            let mut used = Vec::new();
            clip.elements
                .iter()
                .for_each(|e| crate::symbols::instances_in(e, &mut used));
            let mut k = 0;
            while let Some(&id) = used.get(k) {
                k += 1;
                if !clip.symbols.iter().any(|s| s.id == id)
                    && let Some(s) = self.doc.symbols.iter().find(|s| s.id == id)
                {
                    clip.symbols.push(s.clone());
                }
                if let Some(sym) = clip.symbols.iter().find(|s| s.id == id) {
                    for e in sym.layers.iter().flat_map(|l| l.cells.iter().flatten()) {
                        crate::symbols::instances_in(e, &mut used);
                    }
                }
            }
            let looping: Vec<u32> = clip
                .symbols
                .iter()
                .map(|s| s.id)
                .filter(|&id| crate::symbols::symbol_reaches(&clip.symbols, id, link.symbol))
                .collect();
            let mut next = self
                .doc
                .symbols
                .iter()
                .chain(clip.symbols.iter())
                .map(|x| x.id)
                .max()
                .unwrap_or(0)
                + 1;
            for from in looping {
                let to = next;
                next += 1;
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
            firecraft_doc::refresh_nested_instance_bounds(&mut clip.symbols);
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
        clip
    }

    /// Add a prepared clipboard's tiles and symbols to the document (inside an edit).
    pub(crate) fn adopt(&mut self, clip: &Clipboard) {
        if !clip.tiles.is_empty() {
            let tiles = std::sync::Arc::make_mut(&mut self.doc.raw_tiles);
            for t in &clip.tiles {
                tiles.entry(t.id).or_insert_with(|| t.clone());
            }
        }
        for sym in &clip.symbols {
            if !self.doc.symbols.iter().any(|x| x.id == sym.id) {
                std::sync::Arc::make_mut(&mut self.doc.symbols).push(sym.clone());
            }
        }
    }

    /// Edit > Duplicate (`offset` = true: 10 px right and down) and Edit > Clone (in place):
    /// copies just above the topmost selected object of each layer, selected.
    pub fn duplicate(&mut self, offset: bool) {
        let refs = self.stacking_selection();
        // A mask has no copies of its own (an object has one mask).
        if refs.is_empty() || self.pixels.is_some() || refs.iter().any(ElementRef::is_mask) {
            return;
        }
        let label = if offset { "Duplicate" } else { "Clone" };
        self.edit(label, |s| {
            let mut selection = Vec::new();
            // Per list: a layer's objects, or a group's members (Subselection).
            for (first, mine) in by_list(&refs) {
                let Some(cell) = s.list_mut(first) else {
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
                selection.extend((at..at + n).map(|index| first.sibling(index)));
            }
            s.selection = selection;
        });
    }

    /// Copy each selected object to just above itself (Alt+drag, Fireworks MX), without an undo
    /// step; the copies become the selection. Nothing happens with a pixel selection.
    fn copy_each_above(&mut self) {
        let refs = self.stacking_selection();
        if refs.is_empty() || self.pixels.is_some() {
            return;
        }
        let mut selection = Vec::new();
        for (first, mine) in by_list(&refs) {
            let Some(cell) = self.list_mut(first) else {
                continue;
            };
            // Bottom first: the k-th original has k copies below it once they are in.
            for (k, &i) in mine.iter().enumerate() {
                let at = i + k;
                if let Some(copy) = cell.get(at).cloned() {
                    cell.insert(at + 1, copy);
                    selection.push(first.sibling(at + 1));
                }
            }
        }
        self.selection = selection;
        self.touch();
    }

    /// Pointer drag, live: start moving the selection, or with `duplicate` (Alt held, Fireworks
    /// MX) copy each selected object to just above itself and move the copies, the originals
    /// staying where they are. One undo step ("Move" or "Duplicate") when it is committed.
    pub fn begin_drag(&mut self, duplicate: bool) {
        // A selected mask moves, Alt or not: an object has one mask.
        let mask = self.selection.iter().any(ElementRef::is_mask);
        if duplicate && self.pixels.is_none() && !mask {
            self.begin_live_edit("Duplicate");
            self.copy_each_above();
        } else {
            self.begin_live_edit("Move");
        }
    }

    /// Alt pressed or released during a live Pointer drag: undo the drag so far and redo it the
    /// other way, `originals` being the objects the drag started with and (`dx`, `dy`) how far it
    /// has gone.
    pub fn switch_drag(&mut self, duplicate: bool, originals: &[ElementRef], dx: f64, dy: f64) {
        self.cancel_live_edit();
        self.selection = originals.to_vec();
        self.begin_drag(duplicate);
        self.move_selection_live(dx, dy);
    }

    /// Alt+drag without live dragging: copies of the selected objects, each just above its
    /// original, moved by (`dx`, `dy`); one undo step, "Duplicate" (Fireworks MX).
    pub fn duplicate_moved(&mut self, dx: f64, dy: f64) {
        if self.selection.is_empty() || self.pixels.is_some() {
            return;
        }
        self.begin_drag(true);
        self.move_selection_live(dx, dy);
        self.commit_live_edit();
    }

    /// Whether `refs` (none a mask) are all in one list: a layer's objects, a group's members, or
    /// a masked object's content.
    pub(crate) fn one_list(&self, refs: &[ElementRef]) -> bool {
        let Some(first) = refs.first() else { return false };
        refs.iter()
            .all(|r| !r.is_mask() && r.layer == first.layer && r.parent() == first.parent())
    }

    /// Take `refs` (stacking order, all in one list) out of their list; returns them (bottom
    /// first) and where something replacing them goes: the topmost one's place, after the
    /// removal.
    pub(crate) fn take_list(&mut self, refs: &[ElementRef]) -> Option<(Vec<Element>, ElementRef)> {
        let top = *refs.last()?;
        let list = self.list_mut(top)?;
        let mut at: Vec<usize> = refs.iter().map(ElementRef::position).filter(|&i| i < list.len()).collect();
        at.sort_unstable();
        at.dedup();
        let mut taken: Vec<Element> = at.iter().rev().map(|&i| list.remove(i)).collect();
        taken.reverse();
        let place = top.position() + 1 - at.len();
        Some((taken, top.sibling(place)))
    }

    /// Insert `e` at `at` (a place in a layer's objects or inside an object).
    pub(crate) fn insert_in_list(&mut self, at: ElementRef, e: Element) -> bool {
        let i = at.position();
        match self.list_mut(at) {
            Some(list) if i <= list.len() => {
                list.insert(i, e);
                true
            }
            _ => false,
        }
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
        // Objects inside a mask or group (chosen in the Layers panel or with the Subselection
        // tool): grouped where they are, if they share one list.
        if self.selection_nested() {
            if !self.one_list(&refs) {
                return;
            }
            self.edit("Group", |s| {
                if let Some((children, at)) = s.take_list(&refs)
                    && s.insert_in_list(at, Element::new(ElementKind::Group(children)))
                {
                    s.selection = vec![at];
                }
            });
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
                s.selection = vec![ElementRef::new(top.layer, at)];
            }
            s.doc.current_layer = top.layer;
        });
    }

    /// Whether Ungroup has something to do.
    pub fn can_ungroup(&self) -> bool {
        // Groups, and masked objects (Fireworks MX ungroups a masked object into its content
        // and its mask).
        let ungroupable =
            |r: &ElementRef| matches!(self.element(*r).map(|e| &e.kind), Some(ElementKind::Group(_) | ElementKind::Masked { .. }));
        if self.selection_nested() {
            // One inside a mask or another group.
            return matches!(self.selection.as_slice(), [r] if !r.is_mask() && ungroupable(r));
        }
        self.selection.iter().any(ungroupable)
    }

    /// Modify > Ungroup: selected groups are replaced by their objects (selected) in place.
    pub fn ungroup(&mut self) {
        if !self.can_ungroup() {
            return;
        }
        if let [r] = self.selection.as_slice()
            && r.is_nested()
        {
            let r = *r;
            self.edit("Ungroup", |s| {
                let at = r.position();
                let Some(list) = s.list_mut(r) else { return };
                let Some(children) = list.get(at).cloned().and_then(ungrouped) else {
                    return;
                };
                let n = children.len();
                list.splice(at..=at, children);
                s.selection = (at..at + n).map(|i| r.sibling(i)).collect();
            });
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
                    let sel = selected.contains(&ElementRef::new(li, i));
                    let parts = if sel { ungrouped(e.clone()) } else { None };
                    match parts {
                        Some(children) => {
                            for c in children {
                                selection.push(ElementRef::new(li, cell.len()));
                                cell.push(c);
                            }
                        }
                        None => {
                            if sel {
                                selection.push(ElementRef::new(li, cell.len()));
                            }
                            cell.push(e);
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
            let mut selection = Vec::new();
            // Per list: a layer's objects, or a group's members (Subselection).
            for (first, mine) in by_list(&refs) {
                let Some(cell) = s.list_mut(first) else {
                    continue;
                };
                let mut items: Vec<(Element, bool)> = std::mem::take(cell)
                    .into_iter()
                    .enumerate()
                    .map(|(i, e)| (e, mine.contains(&i)))
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
                        selection.push(first.sibling(i));
                    }
                    cell.push(e);
                }
            }
            s.selection = selection;
        });
    }

    /// Modify > Align. Several objects line up with each other (their combined bounds); a
    /// single object lines up with the canvas. Distribute spaces three or more objects evenly
    /// between the outermost ones (equal gaps, in the order of their centres; the ones between
    /// land on whole pixels).
    pub fn align(&mut self, how: Align) {
        let refs = self.stacking_selection();
        let items: Vec<(ElementRef, (f64, f64, f64, f64))> = refs
            .iter()
            .filter_map(|r| Some((*r, crate::align_panel::tidy_bounds(self.element(*r)?.bounds()?))))
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
                let last = sorted.len() - 1;
                for (i, (r, b)) in sorted.iter().enumerate() {
                    let (p, size) = pos(b);
                    // Fireworks MX puts the objects between the ends on whole pixels, rounded
                    // down from where equal gaps would put them (measured: 94.5 -> 94,
                    // 66.67 -> 66, 133.33 -> 133).
                    let to = if i == 0 || i == last { at } else { (at + 1e-9).floor() };
                    let d = to - p;
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
                    e.move_by(dx, dy);
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
        if refs.is_empty() || self.selection_nested() || self.doc.layers.get(layer).is_none_or(|l| l.locked) {
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
                s.selection = (at..at + n).map(|index| ElementRef::new(layer, index)).collect();
            }
            s.doc.current_layer = layer;
        });
    }

    /// Layers panel: move the selected objects into a list, inserted at `index` of it as it is
    /// now: the objects of `layer` (`parent` None), or those inside the object `parent` (a
    /// group's members, a masked object's content, a group mask's objects). Objects can leave
    /// and enter groups and masks this way. A group left empty goes; a mask left with no
    /// content becomes an ordinary object (as if broken apart). They stay selected.
    pub fn move_selection_into(&mut self, layer: usize, parent: Option<ElementRef>, index: usize) {
        // Masks stay with their objects; an object inside another selected one moves with it.
        let sel = self.stacking_selection();
        let refs: Vec<ElementRef> = sel
            .iter()
            .copied()
            .filter(|r| !r.is_mask() && !sel.iter().any(|a| is_inside(*r, *a)))
            .collect();
        if refs.is_empty() || self.pixels.is_some() || self.doc.layers.get(layer).is_none_or(|l| l.locked) {
            return;
        }
        // Not into itself, nor into something inside it.
        if let Some(p) = parent
            && (refs.contains(&p) || refs.iter().any(|r| is_inside(p, *r)) || self.element(p).is_none())
        {
            return;
        }
        // Dropping the selection where it is changes nothing.
        let same_list = refs.iter().all(|r| r.layer == layer && r.parent() == parent);
        if same_list {
            let before = refs.iter().filter(|r| r.position() < index).count();
            let at = index - before;
            if refs.iter().enumerate().all(|(k, r)| r.position() == at + k) {
                return;
            }
        }
        let moved: Vec<Element> = refs.iter().filter_map(|r| self.element(*r).cloned()).collect();
        if moved.len() != refs.len() {
            return;
        }
        let label = if same_list {
            "Reorder"
        } else if refs.iter().any(|r| r.layer != layer) {
            "Move to Layer"
        } else {
            "Move"
        };
        let f = self.frame();
        let mut layers: Vec<usize> = refs.iter().map(|r| r.layer).chain([layer]).collect();
        layers.sort_unstable();
        layers.dedup();
        self.edit(label, |s| {
            let mut tree = Tree {
                refs: &refs,
                dest: (layer, parent, index),
                moved: &moved,
                selection: Vec::new(),
            };
            for li in layers {
                let Some(cell) = s.doc.layers.get_mut(li).and_then(|l| l.elements_mut(f)) else {
                    continue;
                };
                let old = std::mem::take(cell);
                *cell = tree.list(&old, li, None, None);
            }
            s.selection = tree.selection;
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
                if let Some(t) = &mut p.target {
                    t.layer = map(t.layer);
                }
            }
            if let Some(t) = &mut s.bitmap_edit {
                t.layer = map(t.layer);
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

/// What Ungroup turns an object into: a group's members, or a masked object's content and mask
/// (see `masks::apart`); None for anything else.
fn ungrouped(e: Element) -> Option<Vec<Element>> {
    match e.kind {
        ElementKind::Group(children) => Some(children),
        ElementKind::Masked { .. } => crate::masks::apart(&e),
        _ => None,
    }
}

/// Whether `inner` is an object somewhere inside `outer`.
fn is_inside(inner: ElementRef, outer: ElementRef) -> bool {
    let mut p = inner.parent();
    while let Some(x) = p {
        if x == outer {
            return true;
        }
        p = x.parent();
    }
    false
}

/// Rebuilding a layer's objects with some moved elsewhere ([`DocState::move_selection_into`]).
struct Tree<'a> {
    /// The objects that move, as they are addressed now.
    refs: &'a [ElementRef],
    /// Where they go: layer, the object whose list it is (None: the layer's), index.
    dest: (usize, Option<ElementRef>, usize),
    moved: &'a [Element],
    /// Where they ended up.
    selection: Vec<ElementRef>,
}

impl Tree<'_> {
    /// A list rebuilt: `old` is the list of `owner` (None: layer `li`'s objects) as addressed
    /// now, `new_owner` the same list's address afterwards.
    fn list(&mut self, old: &[Element], li: usize, owner: Option<ElementRef>, new_owner: Option<ElementRef>) -> Vec<Element> {
        let at = |o: Option<ElementRef>, i: usize| match o {
            Some(p) => p.child(i),
            None => Some(ElementRef::new(li, i)),
        };
        let mut out = Vec::with_capacity(old.len() + self.moved.len());
        for i in 0..=old.len() {
            if self.dest == (li, owner, i) {
                for m in self.moved {
                    if let Some(r) = at(new_owner, out.len()) {
                        self.selection.push(r);
                    }
                    out.push(m.clone());
                }
            }
            let Some(e) = old.get(i) else { continue };
            let Some(was) = at(owner, i) else {
                out.push(e.clone());
                continue;
            };
            if self.refs.contains(&was) {
                continue;
            }
            let now = at(new_owner, out.len());
            if let Some(e) = self.element(e, was, now) {
                out.push(e);
            }
        }
        out
    }

    /// An element with the lists inside it rebuilt; None for a group left empty, the mask alone
    /// for a masked object left with no content.
    fn element(&mut self, e: &Element, was: ElementRef, now: Option<ElementRef>) -> Option<Element> {
        let mut e = e.clone();
        match &mut e.kind {
            ElementKind::Group(children) => {
                let before = children.len();
                *children = self.list(children, was.layer, Some(was), now);
                if children.is_empty() && before > 0 {
                    return None;
                }
            }
            ElementKind::Masked { content, mask, .. } => {
                *content = self.list(content, was.layer, Some(was), now);
                if let Some(m) = was.mask() {
                    // A group mask left empty hides everything, but stays the mask.
                    **mask = self
                        .element(mask, m, now.and_then(ElementRef::mask))
                        .unwrap_or_else(|| Element::new(ElementKind::Group(Vec::new())));
                }
                if content.is_empty() {
                    let mut m = (**mask).clone();
                    m.visible = true;
                    return Some(m);
                }
            }
            _ => {}
        }
        Some(e)
    }
}

/// References grouped by the list they are in (a layer's objects, or one group's members): a
/// reference of each list and the positions selected in it, in order.
fn by_list(refs: &[ElementRef]) -> Vec<(ElementRef, Vec<usize>)> {
    let mut out: Vec<(ElementRef, Vec<usize>)> = Vec::new();
    for r in refs {
        let key = |x: &ElementRef| (x.layer, x.parent());
        match out.iter_mut().find(|(f, _)| key(f) == key(r)) {
            Some((_, v)) => v.push(r.position()),
            None => out.push((*r, vec![r.position()])),
        }
    }
    for (_, v) in &mut out {
        v.sort_unstable();
        v.dedup();
    }
    out
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
        d.selection = idx.iter().map(|&index| ElementRef::new(0, index)).collect();
    }

    /// e0 alone, then a group of e1, e2, e3 (the group is selected).
    fn grouped() -> DocState {
        let mut d = doc(&[1, 2, 3, 4]);
        sel(&mut d, &[1, 2, 3]);
        d.group();
        d
    }

    fn member_names(d: &DocState) -> Vec<String> {
        match d.element(ElementRef::new(0, 1)).map(|e| &e.kind) {
            Some(ElementKind::Group(c)) => c.iter().map(|e| e.label()).collect(),
            _ => vec![],
        }
    }

    #[test]
    fn subselection_reaches_group_members() {
        let mut d = grouped();
        // The pick goes inside the group: e2 is the group's second member (x 20..24).
        let m = d.hit_deep(21.0, 6.0).expect("a member");
        assert_eq!(
            (m.top(), m.position(), m.is_nested()),
            (ElementRef::new(0, 1), 1, true)
        );
        assert_eq!(d.hit_test(21.0, 6.0), Some(ElementRef::new(0, 1)));
        // Moving the member moves only it; the group stays.
        d.selection = vec![m];
        d.move_selection(0.0, 10.0, "Move");
        assert_eq!(d.element(m).and_then(|e| e.bounds()).map(|b| b.1), Some(15.0));
        assert_eq!(names(&d), ["e0", "Group"]);
        // Arrange and Duplicate act within the group.
        d.arrange(Arrange::BringToFront);
        assert_eq!(member_names(&d), ["e1", "e3", "e2"]);
        assert_eq!(d.selection[0].position(), 2);
        d.duplicate(false);
        assert_eq!(member_names(&d), ["e1", "e3", "e2", "e2"]);
        // Superselect: the group; Subselect: its members.
        d.superselect();
        assert_eq!(d.selection, [ElementRef::new(0, 1)]);
        d.subselect();
        assert_eq!(d.selection.len(), 4);
        assert!(!d.can_ungroup());
    }

    /// e0, then e2 masked by e1 (a bitmap mask), selected none.
    fn masked() -> DocState {
        let mut d = doc(&[1, 2, 3]);
        let mut cell = d.doc.layers[0].elements_mut(0).expect("cell").clone();
        let content = cell.remove(2);
        let mask = cell.remove(1);
        cell.push(Element::new(ElementKind::Masked {
            content: vec![content],
            mask: Box::new(mask),
            mode: firecraft_doc::MaskMode::Grayscale,
            cache: None,
            cache_key: 0,
            flags: Default::default(),
        }));
        *d.doc.layers[0].elements_mut(0).expect("cell") = cell;
        d
    }

    #[test]
    fn a_mask_can_be_selected_and_moved_alone() {
        let mut d = masked();
        let obj = ElementRef::new(0, 1);
        let m = obj.mask().expect("mask ref");
        assert!(m.is_mask() && m.in_mask() && !obj.is_mask());
        assert_eq!(m.parent(), Some(obj));
        assert_eq!(d.element(m).map(|e| e.label()), Some("e1".into()));
        // A member of the content isn't the mask.
        let c = obj.child(0).expect("content");
        assert_eq!(d.element(c).map(|e| e.label()), Some("e2".into()));
        assert!(!c.is_mask());
        // Moving the mask leaves the content where it was.
        d.selection = vec![m];
        d.move_selection(0.0, 7.0, "Move");
        assert_eq!(d.element(m).and_then(|e| e.bounds()).map(|b| b.1), Some(12.0));
        assert_eq!(d.element(c).and_then(|e| e.bounds()).map(|b| b.1), Some(5.0));
        // Delete, Cut, Duplicate and Alt-drag don't take the mask out of its object.
        d.delete();
        d.duplicate(true);
        assert!(d.cut().is_none());
        d.begin_drag(true);
        d.move_selection_live(1.0, 0.0);
        d.commit_live_edit();
        assert_eq!(d.selection, [m]);
        assert_eq!(names(&d), ["e0", "e2 (masked)"]);
        assert_eq!(d.element(m).and_then(|e| e.bounds()).map(|b| b.0), Some(11.0));
        // Undo puts the mask back.
        d.undo();
        d.undo();
        assert_eq!(d.element(m).and_then(|e| e.bounds()).map(|b| (b.0, b.1)), Some((10.0, 5.0)));
    }

    #[test]
    fn layers_drags_reach_inside_groups_and_masks() {
        // Reorder inside a group.
        let mut d = grouped();
        let g = ElementRef::new(0, 1);
        d.selection = vec![g.child(0).expect("member")];
        d.move_selection_into(0, Some(g), 3);
        assert_eq!(member_names(&d), ["e2", "e3", "e1"]);
        assert_eq!(d.selection, [g.child(2).expect("member")]);
        d.undo();
        assert_eq!(member_names(&d), ["e1", "e2", "e3"]);
        // Out of the group, onto the layer above it; then the rest: the empty group goes.
        d.selection = vec![g.child(2).expect("member")];
        d.move_selection_into(0, None, 2);
        assert_eq!(names(&d), ["e0", "Group", "e3"]);
        d.selection = vec![g.child(0).expect("member"), g.child(1).expect("member")];
        d.move_selection_into(0, None, 0);
        assert_eq!(names(&d), ["e1", "e2", "e0", "e3"]);
        assert_eq!(d.selection, [ElementRef::new(0, 0), ElementRef::new(0, 1)]);

        // Into a mask's content, above what it masks.
        let mut d = masked();
        let o = ElementRef::new(0, 1);
        d.selection = vec![ElementRef::new(0, 0)];
        d.move_selection_into(0, Some(o), 1);
        let o = ElementRef::new(0, 0);
        assert_eq!(names(&d), ["Masked Group"]);
        assert_eq!(d.element(o.child(1).expect("c")).map(|e| e.label()), Some("e0".into()));
        // Not into itself.
        d.selection = vec![o];
        d.move_selection_into(0, Some(o), 0);
        assert_eq!(names(&d), ["Masked Group"]);
        // Dragging all the content out leaves the mask an ordinary object.
        d.selection = vec![o.child(0).expect("c"), o.child(1).expect("c")];
        d.move_selection_into(0, None, 0);
        assert_eq!(names(&d), ["e2", "e0", "e1"]);
    }

    #[test]
    fn group_symbol_and_ungroup_inside_a_mask() {
        // e0, then e2 masked by e1; e0 moved into the mask: content [e2, e0].
        let mut d = masked();
        d.selection = vec![ElementRef::new(0, 0)];
        d.move_selection_into(0, Some(ElementRef::new(0, 1)), 1);
        let o = ElementRef::new(0, 0);
        let content = |d: &DocState| match d.element(o).map(|e| &e.kind) {
            Some(ElementKind::Masked { content, .. }) => content.iter().map(|e| e.label()).collect::<Vec<_>>(),
            _ => vec![],
        };
        let both = vec![o.child(0).expect("c"), o.child(1).expect("c")];
        d.selection = both.clone();
        d.group();
        assert_eq!(content(&d), ["Group"]);
        assert_eq!(d.selection, [o.child(0).expect("c")]);
        assert!(d.can_ungroup());
        d.ungroup();
        assert_eq!(content(&d), ["e2", "e0"]);
        assert_eq!(d.selection, both);
        // F8: a symbol in their place, inside the mask.
        d.convert_to_symbol("inside", firecraft_doc::SymbolKind::Graphic).expect("symbol");
        assert_eq!(content(&d), ["inside"]);
        assert_eq!(names(&d), ["inside (masked)"]);
        // Break Apart there: the objects again, in place, selected (the masked instance
        // selected counts as its symbol too).
        for sel in [o.child(0).expect("c"), o] {
            d.selection = vec![sel];
            assert!(d.can_break_apart());
            d.break_apart();
            assert_eq!(content(&d), ["e2", "e0"]);
            assert_eq!(d.selection.len(), 2);
            d.undo();
        }
        // Objects from two lists: nothing.
        d.undo();
        d.selection = vec![o.child(0).expect("c"), o];
        d.group();
        assert_eq!(content(&d), ["e2", "e0"]);
    }

    #[test]
    fn deleting_members_keeps_the_group_until_empty() {
        let mut d = grouped();
        let g = ElementRef::new(0, 1);
        d.selection = vec![g.child(0).expect("member")];
        d.delete();
        assert_eq!(member_names(&d), ["e2", "e3"]);
        d.selection = vec![g.child(0).expect("member"), g.child(1).expect("member")];
        d.delete();
        assert_eq!(names(&d), ["e0"]);
        d.undo();
        assert_eq!(member_names(&d), ["e2", "e3"]);
    }

    #[test]
    fn group_and_ungroup_keep_stacking() {
        let mut d = doc(&[1, 2, 3]);
        sel(&mut d, &[2, 0]);
        d.group();
        assert_eq!(names(&d), ["e1", "Group"]);
        assert_eq!(d.selection, [ElementRef::new(0, 1)]);
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
        assert_eq!(d.selection, [ElementRef::new(0, 3)]);
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
            d.element(ElementRef::new(0, 1)).and_then(Element::bounds),
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
        assert_eq!(other.selection, [ElementRef::new(0, 1)]);
        // Cut removes it and records "Cut".
        let n = names(&d).len();
        assert!(d.cut().is_some());
        assert_eq!(names(&d).len(), n - 1);
        assert_eq!(d.history.next_undo(), Some("Cut"));
    }

    /// Measured in Fireworks MX: Alt+drag puts each copy just above its own original, moved;
    /// the copies stay selected; one undo step "Duplicate".
    #[test]
    fn alt_drag_copies_each_object_above_itself() {
        let mut d = doc(&[1, 2, 3, 4]);
        sel(&mut d, &[0, 2]);
        d.begin_drag(true);
        d.move_selection_live(5.0, 7.0);
        d.commit_live_edit();
        assert_eq!(names(&d), ["e0", "e0", "e1", "e2", "e2", "e3"]);
        assert_eq!(d.selection, [ElementRef::new(0, 1), ElementRef::new(0, 4)]);
        let x = |d: &DocState, i| d.element(ElementRef::new(0, i)).and_then(Element::bounds).map(|b| (b.0, b.1));
        assert_eq!((x(&d, 0), x(&d, 1)), (Some((0.0, 5.0)), Some((5.0, 12.0))));
        assert_eq!((x(&d, 3), x(&d, 4)), (Some((20.0, 5.0)), Some((25.0, 12.0))));
        assert_eq!(d.history.next_undo(), Some("Duplicate"));
        d.undo();
        assert_eq!(names(&d), ["e0", "e1", "e2", "e3"]);

        // Alt released during the drag: a plain move after all.
        sel(&mut d, &[1]);
        let originals = d.selection.clone();
        d.begin_drag(true);
        d.move_selection_live(3.0, 0.0);
        d.switch_drag(false, &originals, 3.0, 0.0);
        d.commit_live_edit();
        assert_eq!(names(&d), ["e0", "e1", "e2", "e3"]);
        assert_eq!(d.history.next_undo(), Some("Move"));
        assert_eq!(x(&d, 1), Some((13.0, 5.0)));

        // Without live dragging.
        sel(&mut d, &[3]);
        d.duplicate_moved(0.0, 10.0);
        assert_eq!(names(&d), ["e0", "e1", "e2", "e3", "e3"]);
        assert_eq!(d.selection, [ElementRef::new(0, 4)]);
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
            .filter_map(|i| d.element(ElementRef::new(0, i))?.bounds())
            .collect();
        assert!(b.iter().all(|b| b.1 == 5.0), "{b:?}");
        d.align(Align::DistributeWidths);
        let xs: Vec<f64> = (0..3)
            .filter_map(|i| Some(d.element(ElementRef::new(0, i))?.bounds()?.0))
            .collect();
        assert_eq!(xs, [0.0, 20.0, 40.0]);
        // One object aligns with the canvas.
        sel(&mut d, &[1]);
        d.align(Align::Right);
        assert_eq!(
            d.element(ElementRef::new(0, 1))
                .and_then(Element::bounds)
                .map(|b| b.0),
            Some(96.0)
        );
    }

    #[test]
    fn turns_are_pixel_exact_with_every_interpolation() {
        use firecraft_raster::transform::Resample;
        for method in Resample::ALL {
            let mut d = DocState::new(Document::new(50, 50, 72.0, None));
            d.interpolation = method;
            let mut img = RgbaImage::new(3, 2, Rgba::TRANSPARENT);
            for (i, (x, y)) in [(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)].into_iter().enumerate() {
                img.set(x, y, Rgba::new(40 * i as u8, 0, 0, 255));
            }
            d.add_element(Element::new(ElementKind::Bitmap(Bitmap { x: 10, y: 10, image: img.clone() })), "Add");
            d.turn(Turn::Rotate90Cw);
            d.turn(Turn::FlipHorizontal);
            d.turn(Turn::FlipHorizontal);
            d.turn(Turn::Rotate90Ccw);
            match &d.element(ElementRef::new(0, 0)).map(|e| e.kind.clone()) {
                Some(ElementKind::Bitmap(b)) => assert_eq!((b.x, b.y, &b.image), (10, 10, &img), "{method:?}"),
                _ => panic!("not a bitmap"),
            }
        }
    }

    #[test]
    fn scaling_follows_the_interpolation_preference() {
        use firecraft_raster::transform::{Affine, Resample};
        let scaled = |method| {
            let mut d = DocState::new(Document::new(50, 50, 72.0, None));
            d.interpolation = method;
            let mut img = RgbaImage::new(4, 4, Rgba::new(0, 0, 0, 255));
            img.set(1, 1, Rgba::new(255, 255, 255, 255));
            d.add_element(Element::new(ElementKind::Bitmap(Bitmap { x: 0, y: 0, image: img })), "Add");
            let originals = vec![(ElementRef::new(0, 0), d.element(ElementRef::new(0, 0)).unwrap().clone())];
            let m = Affine { a: 2.5, d: 2.5, ..Affine::IDENTITY };
            d.transform_from(&originals, &m, "Scale");
            match &d.element(ElementRef::new(0, 0)).unwrap().kind {
                ElementKind::Bitmap(b) => b.image.clone(),
                _ => panic!(),
            }
        };
        let near = scaled(Resample::NearestNeighbor);
        // Nearest neighbour keeps only the two colours; the others blend.
        assert!(near.pixels().iter().all(|p| p.r == 0 || p.r == 255));
        for m in [Resample::Bicubic, Resample::Bilinear, Resample::Soft] {
            assert_ne!(scaled(m), near, "{m:?}");
        }
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
        let bitmap = |d: &DocState| match &d.element(ElementRef::new(0, 0)).map(|e| e.kind.clone()) {
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
        d.rename_object(ElementRef::new(0, 0), "first");
        assert_eq!(d.doc.layers[0].elements(0)[0].label(), "first");
    }
}
