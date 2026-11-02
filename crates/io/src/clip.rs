//! Clipboard data shared with Fireworks MX.
//!
//! Fireworks MX copies objects to the system clipboard as a format named "PNG" (on Linux under
//! Wine: the X11 target `image/png`) holding a complete Fireworks PNG: a document cropped to
//! the copied objects, whose root origin (`XLCf`/`YLCf`) is their original top-left. It pastes
//! the same format back as editable objects (checked with Fireworks MX: paths, names and
//! effects survive both ways). Other applications see an ordinary PNG of the objects.

use firecraft_doc::{Document, Element, Layer, Symbol};
use firecraft_fwpng::{Node, Tile};

use crate::{Error, Result};

/// A Fireworks PNG holding `elements` (canvas coordinates; bottom first) and the symbols they
/// use, cropped to `bounds` (x, y, w, h) as Fireworks does.
pub fn clipboard_png(
    elements: &[Element],
    symbols: &[Symbol],
    tiles: &[Tile],
    bounds: (f64, f64, f64, f64),
) -> Result<Vec<u8>> {
    let (x, y, w, h) = bounds;
    // Whole pixels; the tolerance keeps float noise (9.9999999) from adding a pixel.
    const EPS: f64 = 1e-6;
    let (ox, oy) = ((x + EPS).floor(), (y + EPS).floor());
    let (cw, ch) = (
        ((x + w - EPS).ceil() - ox).max(1.0) as u32,
        ((y + h - EPS).ceil() - oy).max(1.0) as u32,
    );
    let mut doc = Document::new(cw, ch, 72.0, None);
    let mut root = crate::write::new_root();
    put(&mut root, Node::float("XLC", ox));
    put(&mut root, Node::float("YLC", oy));
    doc.raw = Some(root);
    doc.symbols = std::sync::Arc::new(symbols.to_vec());
    // The writer keeps the ones the objects refer to.
    doc.raw_tiles = std::sync::Arc::new(tiles.iter().map(|t| (t.id, t.clone())).collect());
    let mut layer = Layer::new("Layer 1");
    layer.cells = vec![
        elements
            .iter()
            .map(|e| {
                let mut e = e.clone();
                e.translate(-ox, -oy);
                e
            })
            .collect(),
    ];
    doc.layers.retain(|l| l.web);
    doc.layers.insert(0, layer);
    crate::save_fireworks_png(&doc)
}

/// The objects in a clipboard PNG from Fireworks (or FireCraft), in canvas coordinates of the
/// document they were copied from (bottom first), with the symbols they use. A plain PNG
/// (from another application) is one bitmap at (0, 0).
pub fn read_clipboard_png(bytes: &[u8]) -> Result<(Vec<Element>, Vec<Symbol>, Vec<Tile>)> {
    let fw = firecraft_fwpng::FwPng::read(bytes)?;
    if !fw.is_fireworks() {
        let img = crate::decode_png(bytes)?;
        let el = Element::new(firecraft_doc::ElementKind::Bitmap(firecraft_doc::Bitmap {
            x: 0,
            y: 0,
            image: img,
        }));
        return Ok((vec![el], Vec::new(), Vec::new()));
    }
    let root = fw
        .tree()?
        .ok_or_else(|| Error::Unsupported("no document tree".into()))?;
    let origin = (
        root.float_of("XLC").unwrap_or(0.0),
        root.float_of("YLC").unwrap_or(0.0),
    );
    let doc = crate::document_from_fireworks(&fw)?;
    let mut out = Vec::new();
    for layer in doc.layers.iter().filter(|l| !l.web) {
        for e in layer.elements(0) {
            let mut e = e.clone();
            e.translate(origin.0, origin.1);
            out.push(e);
        }
    }
    Ok((
        out,
        doc.symbols.to_vec(),
        doc.raw_tiles.values().cloned().collect(),
    ))
}

fn put(n: &mut Node, child: Node) {
    match n.children.iter_mut().find(|c| c.tag == child.tag) {
        Some(slot) => *slot = child,
        None => n.children.push(child),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firecraft_doc::{Bitmap, ElementKind};
    use firecraft_raster::{Rgba, RgbaImage};

    #[test]
    fn round_trips_objects_at_their_place() {
        let mut e = Element::new(ElementKind::Bitmap(Bitmap {
            x: 37,
            y: 12,
            image: RgbaImage::new(5, 4, Rgba::new(1, 2, 3, 255)),
        }));
        e.name = Some("piece".into());
        let png = clipboard_png(std::slice::from_ref(&e), &[], &[], (37.0, 12.0, 5.0, 4.0)).expect("written");
        let (back, _, _) = read_clipboard_png(&png).expect("read");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name.as_deref(), Some("piece"));
        assert_eq!(back[0].bounds(), Some((37.0, 12.0, 5.0, 4.0)));
        // Other applications see the objects' picture.
        let flat = crate::decode_png(&png).expect("plain png");
        assert_eq!((flat.width(), flat.height()), (5, 4));
    }
}

#[cfg(test)]
mod nine_tests {
    use firecraft_doc::{Bitmap, Document, Element, ElementKind, Layer, Symbol, SymbolKind};
    use firecraft_raster::{Rgba, RgbaImage};

    #[test]
    fn nine_slice_guides_round_trip_as_fireworks_cs_writes_them() {
        let mut doc = Document::new(200, 100, 72.0, Some(Rgba::WHITE));
        let mut layer = Layer::new("Layer 1");
        layer.cells = vec![vec![Element::new(ElementKind::Bitmap(Bitmap {
            x: -30,
            y: -20,
            image: RgbaImage::new(60, 40, Rgba::new(1, 2, 3, 255)),
        }))]];
        let sym = Symbol {
            id: 0x1234,
            name: "nine".into(),
            kind: SymbolKind::Graphic,
            layers: vec![layer, Layer::web_layer()],
            frames: 1,
            nine: Some((-10.0, 10.0, -7.0, 7.0)),
            raw: None,
        };
        doc.symbols = std::sync::Arc::new(vec![sym]);
        let bytes = crate::save_fireworks_png(&doc).expect("saved");
        let back = crate::open(&bytes, None).expect("opened");
        assert_eq!(
            back.symbols.first().and_then(|s| s.nine),
            Some((-10.0, 10.0, -7.0, 7.0))
        );
        // NGSv right after GDSv, with NGCf top, bottom, left, right.
        let (tree, _) = crate::fireworks_tree(&back);
        let mst = tree
            .child("MSL")
            .and_then(|m| m.child("MST"))
            .expect("symbol node");
        let tags: Vec<String> = mst
            .children
            .iter()
            .map(|c| String::from_utf8_lossy(&c.tag).into_owned())
            .collect();
        let gds = tags.iter().position(|t| t == "GDS").expect("GDS");
        assert_eq!(tags.get(gds + 1).map(String::as_str), Some("NGS"));
        let ngc: Vec<f64> = mst
            .child("NGS")
            .expect("NGS")
            .children
            .iter()
            .filter(|c| c.is("NGC"))
            .filter_map(|c| c.text.parse().ok())
            .collect();
        assert_eq!(ngc, [-7.0, 7.0, -10.0, 10.0]);
    }
}
