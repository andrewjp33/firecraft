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

use crate::{DocState, ElementRef, Session};

/// A document that is a symbol being edited.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SymbolLink {
    /// The document the symbol belongs to (`DocState::id`).
    pub parent: u64,
    pub symbol: u32,
    /// Symbol space -> editor canvas.
    pub offset: (f64, f64),
    /// The editor's revision last written back.
    pub synced: u64,
}

/// Room around a symbol's content in its editor.
const EDITOR_MARGIN: f64 = 40.0;

/// A fresh, unused symbol id (Fireworks uses arbitrary 32-bit ids).
fn new_symbol_id(doc: &Document) -> u32 {
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
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
        let refs = self.stacking_selection();
        let top = *refs.last()?;
        if self.pixels.is_some() {
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
                s.selection = vec![ElementRef {
                    layer: top.layer,
                    index: at,
                }];
            }
            s.doc.current_layer = top.layer;
        });
        Some(id)
    }

    /// Whether Break Apart has something to do.
    pub fn can_break_apart(&self) -> bool {
        self.selection.iter().any(|r| self.instance_symbol(*r).is_some())
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
                    selection.extend((r.index..r.index + n).map(|index| ElementRef {
                        layer: r.layer,
                        index,
                    }));
                }
            }
            selection.sort();
            s.selection = selection;
        });
    }

    /// Library: place an instance of `id` with its origin at (`x`, `y`) on the current layer,
    /// selected.
    pub fn insert_instance(&mut self, id: u32, x: f64, y: f64) {
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
        let offset = ((EDITOR_MARGIN - bx).round(), (EDITOR_MARGIN - by).round());
        let (w, h) = (
            (bw + 2.0 * EDITOR_MARGIN).ceil().max(100.0) as u32,
            (bh + 2.0 * EDITOR_MARGIN).ceil().max(100.0) as u32,
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
        let Some(parent) = self.docs.get(doc) else { return };
        let pid = parent.id;
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
        self.push(editor);
        if let Some(d) = self.docs.last_mut() {
            d.symbol_link = Some(SymbolLink {
                parent: pid,
                symbol: id,
                offset,
                synced: d.revision,
            });
        }
    }

    /// The Symbol Editor's Done: write it back, close it and return to its document.
    pub fn finish_symbol_edit(&mut self, editor: u64) {
        self.sync_symbols();
        let Some(i) = self.docs.iter().position(|d| d.id == editor) else {
            return;
        };
        let parent = self.docs.get(i).and_then(|d| d.symbol_link).map(|l| l.parent);
        self.close(i);
        if let Some(p) = parent.and_then(|p| self.docs.iter().position(|d| d.id == p)) {
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
            if editor.revision == link.synced {
                continue;
            }
            let (content, revision) = (editor.doc.clone(), editor.revision);
            if let Some(parent) = self.docs.get_mut(p) {
                parent.set_symbol_content(link.symbol, &content, link.offset);
            }
            if let Some(editor) = self.docs.get_mut(i) {
                editor.symbol_link = Some(SymbolLink {
                    synced: revision,
                    ..link
                });
                // Saved with its document, not on its own.
                editor.dirty = false;
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
        d.selection = vec![
            ElementRef { layer: 0, index: 0 },
            ElementRef { layer: 0, index: 1 },
        ];
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
        d.selection = vec![ElementRef { layer: 0, index: 0 }];
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
}
