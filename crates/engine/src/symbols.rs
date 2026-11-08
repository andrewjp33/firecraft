//! Symbols (Modify > Symbol, the Library panel): Convert to Symbol, Break Apart, instances,
//! renaming/duplicating/deleting symbols, and the Symbol Editor link.
//!
//! As in Fireworks MX, a new symbol's content is centred on the symbol's origin (a 50x40
//! rectangle at (10, 10) becomes -25..25 x -20..20) and the instance's matrix moves it back.
//! Editing a symbol opens it as its own document (Fireworks MX's Symbol Editor window); every
//! change there is written back to the symbol, so instances update as you edit.

use std::sync::Arc;

use firecraft_doc::{Document, Element, ElementKind, Frame, Layer, Symbol, SymbolKind};
use firecraft_raster::transform::Affine;
use instant::SystemTime;

use crate::{DocState, ElementRef, Session};

/// A document that is a symbol being edited.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SymbolLink {
    /// The document the symbol belongs to (`DocState::id`): always the real document, also for
    /// a symbol opened from inside another Symbol Editor, so every edit reaches it.
    pub parent: u64,
    pub symbol: u32,
    /// Symbol space -> editor canvas.
    pub offset: (f64, f64),
    /// The editor's revision last written back.
    pub synced: u64,
    /// Where Done returns: the document, or the Symbol Editor it was opened from.
    pub back: u64,
    /// The document's revision whose symbols this editor holds (its copy is refreshed when the
    /// document's symbols change, e.g. by another Symbol Editor).
    pub parent_seen: u64,
}

/// Least room around a symbol's content in its editor (at least the content's own size is
/// added on each side, so there's space to work in all around it).
const EDITOR_MARGIN: f64 = 500.0;

/// 9-slice guides at a rectangle's thirds: (left, right, top, bottom).
pub fn thirds(x: f64, y: f64, w: f64, h: f64) -> (f64, f64, f64, f64) {
    (
        (x + w / 3.0).round(),
        (x + 2.0 * w / 3.0).round(),
        (y + h / 3.0).round(),
        (y + 2.0 * h / 3.0).round(),
    )
}

/// A fresh, unused symbol id (Fireworks uses arbitrary 32-bit ids).
fn new_symbol_id(doc: &Document) -> u32 {
    let mut seed = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0x5eed_1234, |d| {
            d.subsec_nanos() ^ (d.as_secs() as u32).rotate_left(13)
        });
    loop {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        let id = seed & 0x7fff_ffff;
        if id != 0 && !doc.symbols.iter().any(|s| s.id == id) {
            return id;
        }
    }
}

/// Point every instance of `id` (in `els`, recursively) at `f`.
fn for_instances(els: &mut [Element], id: u32, f: &mut dyn FnMut(&mut Element)) {
    for e in els {
        let is = matches!(&e.kind, ElementKind::Instance { symbol, .. } if *symbol == id);
        if is {
            f(e);
            continue;
        }
        match &mut e.kind {
            ElementKind::Group(children) => for_instances(children, id, f),
            ElementKind::Masked { content, .. } => for_instances(content, id, f),
            _ => {}
        }
    }
}

/// Every cell of every layer (the document's and its symbols').
fn all_cells(doc: &mut Document) -> Vec<&mut Vec<Element>> {
    let mut out: Vec<&mut Vec<Element>> = Vec::new();
    for l in &mut doc.layers {
        out.extend(l.cells.iter_mut());
    }
    for s in Arc::make_mut(&mut doc.symbols) {
        for l in &mut s.layers {
            out.extend(l.cells.iter_mut());
        }
    }
    out
}

/// The symbols `e` shows instances of (also inside groups and masks).
pub(crate) fn instances_in(e: &Element, out: &mut Vec<u32>) {
    match &e.kind {
        ElementKind::Instance { symbol, .. } => {
            if !out.contains(symbol) {
                out.push(*symbol);
            }
        }
        ElementKind::Group(children) => children.iter().for_each(|c| instances_in(c, out)),
        ElementKind::Masked { content, mask, .. } => {
            content.iter().for_each(|c| instances_in(c, out));
            instances_in(mask, out);
        }
        _ => {}
    }
}

/// Whether symbol `from` is `target` or contains an instance of it, directly or through other
/// symbols.
pub(crate) fn symbol_reaches(symbols: &[Symbol], from: u32, target: u32) -> bool {
    let mut seen = vec![from];
    let mut todo = vec![from];
    while let Some(id) = todo.pop() {
        if id == target {
            return true;
        }
        let Some(sym) = symbols.iter().find(|s| s.id == id) else {
            continue;
        };
        let mut used = Vec::new();
        for e in sym.layers.iter().flat_map(|l| l.cells.iter().flatten()) {
            instances_in(e, &mut used);
        }
        for u in used {
            if !seen.contains(&u) {
                seen.push(u);
                todo.push(u);
            }
        }
    }
    false
}

/// After a symbol's content changed: its instances' cached bounds and names.
fn refresh_instances(doc: &mut Document, id: u32) {
    let Some(sym) = doc.symbols.iter().find(|s| s.id == id) else {
        return;
    };
    let (content, name) = (sym.content_bounds(), sym.name.clone());
    for cell in all_cells(doc) {
        for_instances(cell, id, &mut |e| {
            if let ElementKind::Instance {
                matrix,
                bounds,
                name: n,
                ..
            } = &mut e.kind
            {
                *bounds = content.map(|(x, y, w, h)| matrix.map_rect(x, y, w, h));
                *n = name.clone();
            }
        });
    }
    // Symbols containing it changed size too: refresh nested instances, then every instance
    // from its symbol's (possibly new) content.
    firecraft_doc::refresh_nested_instance_bounds(Arc::make_mut(&mut doc.symbols).as_mut_slice());
    let info: Vec<(u32, Option<(f64, f64, f64, f64)>)> =
        doc.symbols.iter().map(|s| (s.id, s.content_bounds())).collect();
    for cell in all_cells(doc) {
        for (sid, content) in &info {
            for_instances(cell, *sid, &mut |e| {
                if let ElementKind::Instance { matrix, bounds, .. } = &mut e.kind {
                    *bounds = content.map(|(x, y, w, h)| matrix.map_rect(x, y, w, h));
                }
            });
        }
    }
}

impl DocState {
    /// The symbol an object is an instance of.
    pub fn instance_symbol(&self, r: ElementRef) -> Option<u32> {
        match &self.element(r)?.kind {
            ElementKind::Instance { symbol, .. } => Some(*symbol),
            _ => None,
        }
    }

    /// Modify > Symbol > Convert to Symbol (F8): the selected objects become a symbol and are
    /// replaced by an instance of it (where the topmost of them was). Returns the symbol's id.
    pub fn convert_to_symbol(&mut self, name: &str, kind: SymbolKind) -> Option<u32> {
        self.convert_to_symbol_with(name, kind, false)
    }

    /// Convert to Symbol, optionally with 9-slice scaling guides (Fireworks CS; placed at the
    /// content's thirds, as Fireworks does).
    pub fn convert_to_symbol_with(&mut self, name: &str, kind: SymbolKind, nine: bool) -> Option<u32> {
        let refs = self.stacking_selection();
        let top = *refs.last()?;
        if self.pixels.is_some() || self.selection_nested() {
            return None;
        }
        let (x, y, w, h) = self.selection_bounds()?;
        // The content's centre becomes the symbol's origin (whole pixels keep bitmaps sharp).
        let (cx, cy) = ((x + w / 2.0).round(), (y + h / 2.0).round());
        let id = new_symbol_id(&self.doc);
        let name = if name.trim().is_empty() {
            "Symbol".to_owned()
        } else {
            name.trim().to_owned()
        };
        self.edit("Convert to Symbol", |s| {
            let mut content = s.take(&refs);
            content.iter_mut().for_each(|e| e.translate(-cx, -cy));
            let mut layer = Layer::new("Layer 1");
            // Buttons have four states (Up, Over, Down, Over While Down); all start as Up.
            let frames = if kind == SymbolKind::Button { 4 } else { 1 };
            layer.cells = vec![content; frames];
            let mut web = Layer::web_layer();
            web.cells = vec![Vec::new(); frames];
            let sym = Symbol {
                id,
                name: name.clone(),
                kind,
                layers: vec![layer, web],
                frames,
                nine: nine.then(|| thirds(x - cx, y - cy, w, h)),
                raw: None,
            };
            Arc::make_mut(&mut s.doc.symbols).push(sym);
            let instance = Element::new(ElementKind::Instance {
                symbol: id,
                name,
                matrix: Affine::translate(cx, cy),
                bounds: Some((x, y, w, h)),
            });
            let below = refs
                .iter()
                .filter(|r| r.layer == top.layer && r.index < top.index)
                .count();
            let f = s.frame();
            if let Some(cell) = s.doc.layers.get_mut(top.layer).and_then(|l| l.elements_mut(f)) {
                let at = (top.index - below).min(cell.len());
                cell.insert(at, instance);
                s.selection = vec![ElementRef::new(top.layer, at)];
            }
            s.doc.current_layer = top.layer;
        });
        Some(id)
    }

    /// An instance's outline: its symbol's content rectangle under the instance's matrix (a
    /// distorted instance has a distorted outline), corners clockwise from top-left. Also the
    /// symbol-space rectangle it comes from.
    pub fn instance_quad(&self, r: ElementRef) -> Option<([(f64, f64); 4], (f64, f64, f64, f64))> {
        let ElementKind::Instance { symbol, matrix, .. } = &self.element(r)?.kind else {
            return None;
        };
        let (x, y, w, h) = self
            .doc
            .symbols
            .iter()
            .find(|s| s.id == *symbol)?
            .content_bounds()?;
        let q = [
            matrix.apply(x, y),
            matrix.apply(x + w, y),
            matrix.apply(x + w, y + h),
            matrix.apply(x, y + h),
        ];
        Some((q, (x, y, w, h)))
    }

    /// The rectangle around an instance's outline (x, y, w, h): what Fireworks draws dotted.
    pub fn instance_rect(&self, r: ElementRef) -> Option<(f64, f64, f64, f64)> {
        let (q, _) = self.instance_quad(r)?;
        let x0 = q.iter().map(|p| p.0).fold(f64::MAX, f64::min);
        let y0 = q.iter().map(|p| p.1).fold(f64::MAX, f64::min);
        let x1 = q.iter().map(|p| p.0).fold(f64::MIN, f64::max);
        let y1 = q.iter().map(|p| p.1).fold(f64::MIN, f64::max);
        Some((x0, y0, x1 - x0, y1 - y0))
    }

    /// Whether Break Apart has something to do.
    pub fn can_break_apart(&self) -> bool {
        !self.selection_nested() && self.selection.iter().any(|r| self.instance_symbol(*r).is_some())
    }

    /// Modify > Symbol > Break Apart: selected instances become ordinary objects (the symbol's,
    /// placed as the instance shows them); the symbol stays in the Library.
    pub fn break_apart(&mut self) {
        let refs = self.stacking_selection();
        if !self.can_break_apart() {
            return;
        }
        let ctx = firecraft_doc::RenderCtx {
            symbols: &self.doc.symbols,
            frame: self.doc.current_frame,
            depth: 0,
            math: self.doc.color_math,
            chain: [0; firecraft_doc::MAX_SYMBOL_DEPTH as usize],
        };
        // Each instance's objects, in place (instances' own opacity/blend/effects are dropped,
        // as Fireworks does).
        let parts: Vec<(ElementRef, Vec<Element>)> = refs
            .iter()
            .filter_map(|r| {
                let e = self.element(*r)?;
                matches!(e.kind, ElementKind::Instance { .. }).then(|| (*r, e.pass_through_children(&ctx)))
            })
            .collect();
        self.edit("Break Apart", |s| {
            let f = s.frame();
            let mut selection: Vec<ElementRef> = refs
                .iter()
                .filter(|r| s.instance_symbol(**r).is_none())
                .copied()
                .collect();
            // Bottom-most last so earlier indices stay valid.
            for (r, objects) in parts.into_iter().rev() {
                if let Some(cell) = s.doc.layers.get_mut(r.layer).and_then(|l| l.elements_mut(f))
                    && r.index < cell.len()
                {
                    let n = objects.len();
                    cell.splice(r.index..=r.index, objects);
                    // Shift selected objects above this one in the same layer.
                    for sel in &mut selection {
                        if sel.layer == r.layer && sel.index > r.index {
                            sel.index = sel.index + n - 1;
                        }
                    }
                    selection.extend((r.index..r.index + n).map(|index| ElementRef::new(r.layer, index)));
                }
            }
            selection.sort();
            s.selection = selection;
        });
    }

    /// Library: place an instance of `id` with its origin at (`x`, `y`) on the current layer,
    /// selected.
    /// In a Symbol Editor: whether an instance of `id` here would make the edited symbol contain
    /// itself (`id` is that symbol, or contains it through other symbols). Placing one would
    /// recurse endlessly.
    pub fn would_nest_itself(&self, id: u32) -> bool {
        self.symbol_link
            .is_some_and(|l| symbol_reaches(&self.doc.symbols, id, l.symbol))
    }

    /// Place an instance of symbol `id` with its origin at (`x`, `y`); nothing if that would put
    /// the edited symbol inside itself ([`DocState::would_nest_itself`]).
    pub fn insert_instance(&mut self, id: u32, x: f64, y: f64) {
        if self.would_nest_itself(id) {
            return;
        }
        let Some(sym) = self.doc.symbols.iter().find(|s| s.id == id) else {
            return;
        };
        let matrix = Affine::translate(x.round(), y.round());
        let bounds = sym
            .content_bounds()
            .map(|(bx, by, bw, bh)| matrix.map_rect(bx, by, bw, bh));
        let el = Element::new(ElementKind::Instance {
            symbol: id,
            name: sym.name.clone(),
            matrix,
            bounds,
        });
        self.add_element(el, "Insert Instance");
    }

    /// Library: turn a symbol's 9-slice scaling guides on (at its content's thirds) or off.
    pub fn set_symbol_nine(&mut self, id: u32, on: bool) {
        let Some(sym) = self.doc.symbols.iter().find(|s| s.id == id) else {
            return;
        };
        if sym.nine.is_some() == on {
            return;
        }
        let guides = sym.content_bounds().map(|(x, y, w, h)| thirds(x, y, w, h));
        self.edit(if on { "Enable 9-slice" } else { "Disable 9-slice" }, |s| {
            if let Some(sym) = Arc::make_mut(&mut s.doc.symbols).iter_mut().find(|x| x.id == id) {
                sym.nine = if on { guides } else { None };
            }
        });
    }

    /// Library: rename a symbol (its instances show the new name).
    pub fn rename_symbol(&mut self, id: u32, name: &str) {
        let name = name.trim();
        if name.is_empty()
            || self
                .doc
                .symbols
                .iter()
                .find(|s| s.id == id)
                .is_none_or(|s| s.name == name)
        {
            return;
        }
        let name = name.to_owned();
        self.edit("Rename Symbol", |s| {
            if let Some(sym) = Arc::make_mut(&mut s.doc.symbols).iter_mut().find(|x| x.id == id) {
                sym.name = name;
            }
            refresh_instances(&mut s.doc, id);
        });
    }

    /// Library: a copy of a symbol ("name copy"); returns its id.
    pub fn duplicate_symbol(&mut self, id: u32) -> Option<u32> {
        let mut copy = self.doc.symbols.iter().find(|s| s.id == id)?.clone();
        let new = new_symbol_id(&self.doc);
        copy.id = new;
        copy.name = format!("{} copy", copy.name);
        // A new symbol for Fireworks: its node is written fresh.
        copy.raw = None;
        self.edit("Duplicate Symbol", |s| {
            Arc::make_mut(&mut s.doc.symbols).push(copy)
        });
        Some(new)
    }

    /// Library: delete a symbol and its instances (in the document and in other symbols).
    pub fn delete_symbol(&mut self, id: u32) {
        if !self.doc.symbols.iter().any(|s| s.id == id) {
            return;
        }
        self.edit("Delete Symbol", |s| {
            Arc::make_mut(&mut s.doc.symbols).retain(|x| x.id != id);
            fn strip(els: &mut Vec<Element>, id: u32) {
                els.retain(|e| !matches!(&e.kind, ElementKind::Instance { symbol, .. } if *symbol == id));
                for e in els {
                    match &mut e.kind {
                        ElementKind::Group(children) => strip(children, id),
                        ElementKind::Masked { content, .. } => strip(content, id),
                        _ => {}
                    }
                }
            }
            for cell in all_cells(&mut s.doc) {
                strip(cell, id);
            }
            s.selection.clear();
            s.pixels = None;
        });
    }

    /// The symbol's content as an editable document for the Symbol Editor, and the offset from
    /// symbol space to its canvas.
    pub fn symbol_document(&self, id: u32) -> Option<(Document, (f64, f64))> {
        let sym = self.doc.symbols.iter().find(|s| s.id == id)?;
        let (bx, by, bw, bh) = sym.content_bounds().unwrap_or((-50.0, -50.0, 100.0, 100.0));
        let margin = EDITOR_MARGIN.max(bw.max(bh));
        let offset = ((margin - bx).round(), (margin - by).round());
        let (w, h) = (
            (bw + 2.0 * margin).ceil() as u32,
            (bh + 2.0 * margin).ceil() as u32,
        );
        let mut doc = Document::new(w, h, self.doc.resolution, None);
        doc.layers = sym
            .layers
            .iter()
            .map(|l| {
                let mut l = l.clone();
                l.cells
                    .iter_mut()
                    .flatten()
                    .for_each(|e| e.translate(offset.0, offset.1));
                l
            })
            .collect();
        if !doc.layers.iter().any(|l| l.web) {
            doc.layers.push(Layer::web_layer());
        }
        doc.frames = (0..sym.frames.max(1))
            .map(|i| Frame {
                name: format!("Frame {}", i + 1),
                ..Frame::default()
            })
            .collect();
        // Instances inside the symbol need the other symbols.
        doc.symbols = self.doc.symbols.clone();
        doc.color_math = self.doc.color_math;
        doc.raw_tiles = self.doc.raw_tiles.clone();
        doc.path = Some(format!("Symbol: {}", sym.name));
        Some((doc, offset))
    }

    /// Write the Symbol Editor's content back into symbol `id` (live, without an undo step in
    /// this document: the editing is undone in the Symbol Editor).
    pub fn set_symbol_content(&mut self, id: u32, editor: &Document, offset: (f64, f64)) {
        self.set_symbol_content_with(id, editor, offset, None);
    }

    /// [`set_symbol_content`](Self::set_symbol_content), also setting the 9-slice guides when
    /// given (`Some(None)` turns them off).
    pub fn set_symbol_content_with(
        &mut self,
        id: u32,
        editor: &Document,
        offset: (f64, f64),
        editor_nine: Option<Option<(f64, f64, f64, f64)>>,
    ) {
        let Some(sym) = Arc::make_mut(&mut self.doc.symbols)
            .iter_mut()
            .find(|s| s.id == id)
        else {
            return;
        };
        sym.layers = editor
            .layers
            .iter()
            .map(|l| {
                let mut l = l.clone();
                l.cells
                    .iter_mut()
                    .flatten()
                    .for_each(|e| e.translate(-offset.0, -offset.1));
                l
            })
            .collect();
        sym.frames = editor.frames.len().max(1);
        // Guides moved in the editor (symbol space).
        if let Some(g) = editor_nine {
            sym.nine = g;
        }
        // Tiles the edited objects refer to (pasted textures...).
        for (k, t) in editor.raw_tiles.iter() {
            if !self.doc.raw_tiles.contains_key(k) {
                Arc::make_mut(&mut self.doc.raw_tiles).insert(*k, t.clone());
            }
        }
        refresh_instances(&mut self.doc, id);
        self.mark_changed();
    }
}

impl Session {
    /// Open symbol `id` of document `doc` (index) in the Symbol Editor (or switch to it if it's
    /// already open).
    pub fn edit_symbol(&mut self, doc: usize, id: u32) {
        let Some(from) = self.docs.get(doc) else { return };
        let back = from.id;
        // From inside a Symbol Editor: the symbol still belongs to the real document.
        let doc = match from.symbol_link {
            Some(l) => match self.docs.iter().position(|d| d.id == l.parent) {
                Some(p) => p,
                None => return,
            },
            None => doc,
        };
        let Some(parent) = self.docs.get(doc) else { return };
        let (pid, parent_rev) = (parent.id, parent.revision);
        if let Some(i) = self
            .docs
            .iter()
            .position(|d| d.symbol_link.is_some_and(|l| l.parent == pid && l.symbol == id))
        {
            self.active = i;
            return;
        }
        let Some((editor, offset)) = parent.symbol_document(id) else {
            return;
        };
        let content = parent
            .doc
            .symbols
            .iter()
            .find(|s| s.id == id)
            .and_then(Symbol::content_bounds);
        let nine = parent
            .doc
            .symbols
            .iter()
            .find(|s| s.id == id)
            .and_then(|s| s.nine);
        self.push(editor);
        if let Some(d) = self.docs.last_mut() {
            d.symbol_link = Some(SymbolLink {
                parent: pid,
                symbol: id,
                offset,
                synced: d.revision,
                back,
                parent_seen: parent_rev,
            });
            d.symbol_nine = nine;
            // Open on the content (and the symbol's origin), not the whole canvas.
            d.view_focus = Some(match content {
                Some((x, y, w, h)) => {
                    let (x0, y0) = (x.min(0.0), y.min(0.0));
                    let (x1, y1) = ((x + w).max(0.0), (y + h).max(0.0));
                    (x0 + offset.0, y0 + offset.1, x1 - x0, y1 - y0)
                }
                None => (offset.0 - 100.0, offset.1 - 100.0, 200.0, 200.0),
            });
        }
    }

    /// The Symbol Editor's Done: write it back, close it and return to its document.
    pub fn finish_symbol_edit(&mut self, editor: u64) {
        self.sync_symbols();
        let Some(i) = self.docs.iter().position(|d| d.id == editor) else {
            return;
        };
        let link = self.docs.get(i).and_then(|d| d.symbol_link);
        self.close(i);
        // Back to the Symbol Editor it was opened from (if still open), else the document.
        let to = link.and_then(|l| {
            self.docs
                .iter()
                .position(|d| d.id == l.back)
                .or_else(|| self.docs.iter().position(|d| d.id == l.parent))
        });
        if let Some(p) = to {
            self.active = p;
        }
    }

    /// Write changed Symbol Editors back to their symbols (call once per frame). Editors whose
    /// document has closed or whose symbol was deleted are closed.
    pub fn sync_symbols(&mut self) {
        let mut close = Vec::new();
        for i in 0..self.docs.len() {
            let Some(link) = self.docs.get(i).and_then(|d| d.symbol_link) else {
                continue;
            };
            let Some(p) = self.docs.iter().position(|d| d.id == link.parent) else {
                close.push(i);
                continue;
            };
            if self
                .docs
                .get(p)
                .is_none_or(|d| !d.doc.symbols.iter().any(|s| s.id == link.symbol))
            {
                close.push(i);
                continue;
            }
            let Some(editor) = self.docs.get(i) else { continue };
            if editor.revision != link.synced {
                let (content, revision, guides) = (editor.doc.clone(), editor.revision, editor.symbol_nine);
                let mut parent_rev = link.parent_seen;
                if let Some(parent) = self.docs.get_mut(p) {
                    let changed_before = parent.revision != link.parent_seen;
                    parent.set_symbol_content_with(link.symbol, &content, link.offset, Some(guides));
                    // Symbols made inside the editor (Convert to Symbol there) join the document.
                    let new: Vec<Symbol> = content
                        .symbols
                        .iter()
                        .filter(|s| !parent.doc.symbols.iter().any(|q| q.id == s.id))
                        .cloned()
                        .collect();
                    if !new.is_empty() {
                        Arc::make_mut(&mut parent.doc.symbols).extend(new);
                        parent.mark_changed();
                    }
                    // Its own write doesn't make the editor's copy stale.
                    if !changed_before {
                        parent_rev = parent.revision;
                    }
                }
                if let Some(editor) = self.docs.get_mut(i) {
                    editor.symbol_link = Some(SymbolLink {
                        synced: revision,
                        parent_seen: parent_rev,
                        ..link
                    });
                    // Saved with its document, not on its own.
                    editor.dirty = false;
                }
            }
        }
        // Editors whose document's symbols changed (another Symbol Editor wrote back, undo, the
        // Library): take the document's symbols, so nested instances show the change.
        for i in 0..self.docs.len() {
            let Some(link) = self.docs.get(i).and_then(|d| d.symbol_link) else {
                continue;
            };
            let Some(parent) = self.docs.iter().find(|d| d.id == link.parent) else {
                continue;
            };
            if parent.revision == link.parent_seen {
                continue;
            }
            let (symbols, tiles, rev) = (
                parent.doc.symbols.clone(),
                parent.doc.raw_tiles.clone(),
                parent.revision,
            );
            if let Some(editor) = self.docs.get_mut(i) {
                let same = Arc::ptr_eq(&editor.doc.symbols, &symbols);
                if !same {
                    editor.doc.symbols = symbols;
                    editor.doc.raw_tiles = tiles;
                    editor.mark_changed();
                }
                let dirty = editor.dirty && !same;
                editor.symbol_link = Some(SymbolLink {
                    parent_seen: rev,
                    synced: if same { link.synced } else { editor.revision },
                    ..link
                });
                if dirty {
                    editor.dirty = false;
                }
            }
        }
        for i in close.into_iter().rev() {
            self.close(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ToolOptions;
    use firecraft_raster::Rgba;

    fn near(a: Option<(f64, f64, f64, f64)>, b: (f64, f64, f64, f64)) -> bool {
        a.is_some_and(|a| {
            [a.0 - b.0, a.1 - b.1, a.2 - b.2, a.3 - b.3]
                .iter()
                .all(|d| d.abs() < 1e-6)
        })
    }

    fn doc() -> DocState {
        let mut d = DocState::new(Document::new(300, 200, 72.0, Some(Rgba::WHITE)));
        let opts = ToolOptions {
            fill: Some(Rgba::new(200, 40, 40, 255)),
            stroke: None,
            ..Default::default()
        };
        d.add_shape((10.0, 10.0, 60.0, 50.0), false, &opts);
        d.add_shape((100.0, 20.0, 150.0, 70.0), true, &opts);
        d
    }

    #[test]
    fn convert_and_break_apart_keep_the_picture() {
        let mut d = doc();
        let before = d.doc.render(0);
        d.selection = vec![ElementRef::new(0, 0), ElementRef::new(0, 1)];
        let id = d
            .convert_to_symbol("thing", SymbolKind::Graphic)
            .expect("converted");
        assert_eq!(d.doc.layers[0].elements(0).len(), 1);
        let sym = d.doc.symbols.iter().find(|s| s.id == id).expect("symbol");
        // Centred on the origin, as Fireworks does: bounds 10..150 x 10..70 -> centre (80, 40).
        assert!(
            near(sym.content_bounds(), (-70.0, -30.0, 140.0, 60.0)),
            "{:?}",
            sym.content_bounds()
        );
        assert!(
            near(d.selection_bounds(), (10.0, 10.0, 140.0, 60.0)),
            "{:?}",
            d.selection_bounds()
        );
        assert_eq!(d.doc.render(0), before);
        d.break_apart();
        assert_eq!(d.doc.layers[0].elements(0).len(), 2);
        assert_eq!(d.doc.render(0), before);
        assert!(
            d.doc.symbols.iter().any(|s| s.id == id),
            "the symbol stays in the Library"
        );
    }

    #[test]
    fn symbol_editor_round_trip_updates_instances() {
        let mut s = Session::default();
        s.push(doc().doc);
        let d = s.active_doc_mut().expect("doc");
        d.selection = vec![ElementRef::new(0, 0)];
        let id = d
            .convert_to_symbol("box", SymbolKind::Graphic)
            .expect("converted");
        d.insert_instance(id, 200.0, 150.0);
        s.edit_symbol(0, id);
        assert_eq!(s.active, 1);
        // Move the content right by 5 in the editor: both instances move.
        let ed = s.active_doc_mut().expect("editor");
        ed.select_all();
        ed.move_selection(5.0, 0.0, "Move");
        s.sync_symbols();
        let parent = &s.docs[0];
        let xs: Vec<f64> = parent.doc.layers[0]
            .elements(0)
            .iter()
            .filter_map(|e| e.bounds().map(|b| b.0))
            .collect();
        let has = |v: f64| xs.iter().any(|x| (x - v).abs() < 1e-6);
        assert!(has(15.0) && has(180.0), "{xs:?}");
        // Deleting the symbol closes its editor.
        s.docs[0].delete_symbol(id);
        s.sync_symbols();
        assert_eq!(s.docs.len(), 1);
    }
    /// In a symbol's editor, an instance of a symbol containing it can't be pasted or dropped
    /// (it would contain itself); others can.
    #[test]
    fn a_symbol_cannot_be_placed_inside_itself() {
        let mut s = Session::default();
        s.push(doc().doc);
        let d = s.active_doc_mut().expect("doc");
        d.add_shape((150.0, 20.0, 190.0, 60.0), false, &ToolOptions::default());
        d.selection = vec![ElementRef::new(0, 0)];
        let inner = d.convert_to_symbol("inner", SymbolKind::Graphic).expect("inner");
        let other = {
            let n = d.doc.layers[0].elements(0).len();
            d.selection = vec![ElementRef::new(0, n - 1)];
            d.convert_to_symbol("other", SymbolKind::Graphic).expect("other")
        };
        let at = d.doc.layers[0]
            .elements(0)
            .iter()
            .position(|e| matches!(&e.kind, ElementKind::Instance { symbol, .. } if *symbol == inner))
            .expect("inner instance");
        d.selection = vec![ElementRef::new(0, at)];
        let outer = d.convert_to_symbol("outer", SymbolKind::Graphic).expect("outer");
        // Copy an outer instance, then edit the inner symbol.
        let top = d.doc.layers[0].elements(0).len();
        d.selection = (0..top)
            .map(|i| ElementRef::new(0, i))
            .filter(|r| d.instance_symbol(*r) == Some(outer))
            .collect();
        let clip = d.copy().expect("copied");
        s.edit_symbol(0, inner);
        let ed = s.active_doc_mut().expect("inner editor");
        let n = ed.doc.layers[0].elements(0).len();
        assert!(ed.would_nest_itself(outer) && ed.would_nest_itself(inner));
        assert!(!ed.would_nest_itself(other));
        // Dropping from the Library is refused (Fireworks: "a symbol can not contain a copy of
        // itself"); other symbols can be placed.
        ed.insert_instance(outer, 0.0, 0.0);
        ed.insert_instance(inner, 0.0, 0.0);
        assert_eq!(ed.doc.layers[0].elements(0).len(), n);
        ed.insert_instance(other, 0.0, 0.0);
        assert_eq!(ed.doc.layers[0].elements(0).len(), n + 1);
        // Pasting goes in as copies of the symbols that lead back (as Fireworks MX does):
        // "outer" and "inner" again, with new ids, and nothing contains itself.
        ed.paste(&clip);
        let els = ed.doc.layers[0].elements(0);
        assert_eq!(els.len(), n + 2);
        let pasted = match &els[n + 1].kind {
            ElementKind::Instance { symbol, .. } => *symbol,
            _ => panic!("not an instance"),
        };
        assert_ne!(pasted, outer);
        let name = |id: u32| ed.doc.symbols.iter().find(|s| s.id == id).map(|s| s.name.clone());
        assert_eq!(name(pasted).as_deref(), Some("outer"));
        assert!(!symbol_reaches(&ed.doc.symbols, pasted, inner));
        assert_eq!(ed.doc.symbols.iter().filter(|s| s.name == "inner").count(), 2);
        s.sync_symbols();
        let root = &s.docs[0];
        assert!(
            !root
                .doc
                .symbols
                .iter()
                .any(|x| symbol_reaches_strict(&root.doc.symbols, x.id))
        );
    }

    /// Whether symbol `id` contains itself (through at least one instance).
    fn symbol_reaches_strict(symbols: &[Symbol], id: u32) -> bool {
        let Some(sym) = symbols.iter().find(|s| s.id == id) else {
            return false;
        };
        let mut used = Vec::new();
        for e in sym.layers.iter().flat_map(|l| l.cells.iter().flatten()) {
            instances_in(e, &mut used);
        }
        used.iter().any(|&u| symbol_reaches(symbols, u, id))
    }

    /// Symbol 2 edited from inside Symbol 3's editor (mechscene2.png): the change reaches the
    /// document, the outer editor shows it, Done returns to the outer editor, and reopening
    /// keeps it.
    #[test]
    fn nested_symbol_edit_reaches_the_document() {
        let mut s = Session::default();
        s.push(doc().doc);
        let d = s.active_doc_mut().expect("doc");
        d.selection = vec![ElementRef::new(0, 0)];
        let inner = d.convert_to_symbol("inner", SymbolKind::Graphic).expect("inner");
        d.selection = vec![ElementRef::new(0, 0)];
        let outer = d.convert_to_symbol("outer", SymbolKind::Graphic).expect("outer");
        let bounds = |d: &DocState, id: u32| {
            d.doc
                .symbols
                .iter()
                .find(|s| s.id == id)
                .and_then(Symbol::content_bounds)
        };
        let before = bounds(&s.docs[0], inner).expect("inner bounds");
        let outer_before = bounds(&s.docs[0], outer).expect("outer bounds");
        s.edit_symbol(0, outer);
        let a = s.active;
        s.edit_symbol(a, inner);
        let b = s.active;
        assert_ne!(a, b);
        let ed = s.active_doc_mut().expect("inner editor");
        ed.select_all();
        ed.move_selection(5.0, 0.0, "Move");
        s.sync_symbols();
        // The document has it, and the outer editor's copy too.
        let moved = (before.0 + 5.0, before.1, before.2, before.3);
        assert_eq!(bounds(&s.docs[0], inner), Some(moved));
        assert_eq!(bounds(&s.docs[a], inner), Some(moved));
        assert!(bounds(&s.docs[0], outer).is_some_and(|b| (b.0 - outer_before.0 - 5.0).abs() < 1e-9));
        // Done returns to the outer editor, then to the document.
        let b_id = s.docs[b].id;
        s.finish_symbol_edit(b_id);
        assert_eq!(s.active, a);
        let a_id = s.docs[a].id;
        s.finish_symbol_edit(a_id);
        assert_eq!(s.active, 0);
        assert_eq!(bounds(&s.docs[0], inner), Some(moved));
        // Reopened, the outer editor shows the edited inner symbol.
        s.edit_symbol(0, outer);
        let r = s.active;
        assert_eq!(bounds(&s.docs[r], inner), Some(moved));
    }
}
