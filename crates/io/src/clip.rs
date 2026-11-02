//! Clipboard data shared with Fireworks MX.
//!
//! Fireworks MX copies objects to the system clipboard as a format named "PNG" (on Linux under
//! Wine: the X11 target `image/png`) holding a complete Fireworks PNG: a document cropped to
//! the copied objects, whose root origin (`XLCf`/`YLCf`) is their original top-left. It pastes
//! the same format back as editable objects (checked with Fireworks MX: paths, names and
//! effects survive both ways). Other applications see an ordinary PNG of the objects.

use firecraft_doc::{Document, Element, Layer, Symbol};
use firecraft_fwpng::Node;

use crate::{Error, Result};

/// A Fireworks PNG holding `elements` (canvas coordinates; bottom first) and the symbols they
/// use, cropped to `bounds` (x, y, w, h) as Fireworks does.
pub fn clipboard_png(
    elements: &[Element],
    symbols: &[Symbol],
    bounds: (f64, f64, f64, f64),
) -> Result<Vec<u8>> {
    let (x, y, w, h) = bounds;
    let (ox, oy) = (x.floor(), y.floor());
    let (cw, ch) = (
        ((x + w).ceil() - ox).max(1.0) as u32,
        ((y + h).ceil() - oy).max(1.0) as u32,
    );
    let mut doc = Document::new(cw, ch, 72.0, None);
    let mut root = crate::write::new_root();
    put(&mut root, Node::float("XLC", ox));
    put(&mut root, Node::float("YLC", oy));
    doc.raw = Some(root);
    doc.symbols = std::sync::Arc::new(symbols.to_vec());
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
pub fn read_clipboard_png(bytes: &[u8]) -> Result<(Vec<Element>, Vec<Symbol>)> {
    let fw = firecraft_fwpng::FwPng::read(bytes)?;
    if !fw.is_fireworks() {
        let img = crate::decode_png(bytes)?;
        let el = Element::new(firecraft_doc::ElementKind::Bitmap(firecraft_doc::Bitmap {
            x: 0,
            y: 0,
            image: img,
        }));
        return Ok((vec![el], Vec::new()));
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
    Ok((out, doc.symbols.to_vec()))
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
        let png = clipboard_png(std::slice::from_ref(&e), &[], (37.0, 12.0, 5.0, 4.0)).expect("written");
        let (back, _) = read_clipboard_png(&png).expect("read");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name.as_deref(), Some("piece"));
        assert_eq!(back[0].bounds(), Some((37.0, 12.0, 5.0, 4.0)));
        // Other applications see the objects' picture.
        let flat = crate::decode_png(&png).expect("plain png");
        assert_eq!((flat.width(), flat.height()), (5, 4));
    }
}
