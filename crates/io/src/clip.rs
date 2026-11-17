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
/// `res` is the source document's resolution: Fireworks MX's clipboard PNG carries it
/// (`RESf`/`RSUi`, and `pHYs` by the same rule as its files: 150 ppi gave 150, 0 and 5905).
pub fn clipboard_png(
    elements: &[Element],
    symbols: &[Symbol],
    tiles: &[Tile],
    bounds: (f64, f64, f64, f64),
    res: crate::Resolution,
) -> Result<Vec<u8>> {
    let (x, y, w, h) = bounds;
    // Whole pixels; the tolerance keeps float noise (9.9999999) from adding a pixel.
    const EPS: f64 = 1e-6;
    let (ox, oy) = ((x + EPS).floor(), (y + EPS).floor());
    let (cw, ch) = (
        ((x + w - EPS).ceil() - ox).max(1.0) as u32,
        ((y + h - EPS).ceil() - oy).max(1.0) as u32,
    );
    let mut doc = Document::new(cw, ch, res.0, None);
    doc.resolution_cm = res.1;
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

/// What a paste brings in.
pub enum Pasted {
    /// Objects from Fireworks (or FireCraft), at their places, with the symbols and tiles they
    /// use.
    Objects(Vec<Element>, Vec<Symbol>, Vec<Tile>),
    /// A picture from another application (a screenshot, a browser image...): Fireworks MX
    /// pastes it as a bitmap object centred on the canvas.
    Image(firecraft_raster::RgbaImage),
}

/// The system clipboard's data: a Fireworks PNG (objects), or a picture as a PNG, a DIB
/// (`CF_DIB`/`CF_DIBV5`), a BMP file or a JPEG.
pub fn read_clipboard(bytes: &[u8]) -> Result<Pasted> {
    if bytes.starts_with(b"\x89PNG") {
        if firecraft_fwpng::FwPng::read(bytes).is_ok_and(|fw| fw.is_fireworks()) {
            let (e, s, t) = read_clipboard_png(bytes)?;
            return Ok(Pasted::Objects(e, s, t));
        }
        return crate::decode_png(bytes).map(Pasted::Image);
    }
    if bytes.starts_with(b"BM") {
        return crate::dib::decode_bmp(bytes).map(Pasted::Image);
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return crate::textures::decode_image(bytes).map(Pasted::Image);
    }
    crate::dib::decode_dib(bytes).map(Pasted::Image)
}

/// Where Fireworks MX puts a pasted picture of `size` on a canvas of `canvas` size: centred,
/// halves rounded away from zero (measured 2026-11-16: 101 x 61 on 300 x 200 at (100, 70),
/// 303 x 203 at (-2, -2), 500 x 400 at (-100, -100); the canvas doesn't grow).
pub fn pasted_image_origin(canvas: (u32, u32), size: (u32, u32)) -> (i64, i64) {
    let c = |canvas: u32, size: u32| ((f64::from(canvas) - f64::from(size)) / 2.0).round() as i64;
    (c(canvas.0, size.0), c(canvas.1, size.1))
}

/// The size (pixels) and resolution of the picture in clipboard data, as File > New in
/// Fireworks MX starts from it (measured 2026-11-16 by opening File > New with a picture on the
/// clipboard):
/// * a Fireworks PNG: its document's resolution and unit (`RESf`, `RSUi`): 2540 Pixels/Inch,
///   28.9899 Pixels/cm, though their `pHYs` say 1000 and 28 px/cm;
/// * another PNG: `pHYs` as pixels/cm (5906 px/m -> 59.06 Pixels/cm); without `pHYs`
///   28.34 Pixels/cm (Fireworks' 72 ppi as `pHYs` writes it: 2834 px/m).
///
/// A DIB's pixels/metre is taken the same way (unmeasured: under Wine, MX only reads `PNG`);
/// other formats give their size at 28.34 Pixels/cm.
pub fn clipboard_image_info(bytes: &[u8]) -> Option<(u32, u32, crate::Resolution)> {
    let default = (f64::from(crate::png_ppm((72.0, false))) / 100.0, true);
    let be = |at: usize| -> Option<u32> { Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?)) };
    let le = |b: &[u8], at: usize| -> Option<u32> { Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?)) };
    let per_cm = |per_metre: u32| (per_metre > 0).then(|| (f64::from(per_metre) / 100.0, true));
    if bytes.starts_with(b"\x89PNG") {
        let (w, h) = (be(16)?, be(20)?);
        if let Ok(fw) = firecraft_fwpng::FwPng::read(bytes)
            && fw.is_fireworks()
            && let Ok(Some(root)) = fw.tree()
            && let Some(res) = root.float_of("RES")
        {
            return Some((w, h, (res, root.int_of("RSU") == Some(1))));
        }
        // pHYs (unit 1 = metre) somewhere before the image data.
        let mut at = 8;
        let mut res = None;
        while let (Some(len), Some(kind)) = (be(at), bytes.get(at + 4..at + 8)) {
            if kind == b"IDAT" {
                break;
            }
            if kind == b"pHYs" && bytes.get(at + 16) == Some(&1) {
                res = be(at + 8).and_then(per_cm);
            }
            at += 12 + len as usize;
        }
        return Some((w, h, res.unwrap_or(default)));
    }
    let dib = if bytes.starts_with(b"BM") { bytes.get(14..)? } else { bytes };
    if !bytes.starts_with(&[0xff, 0xd8, 0xff]) && le(dib, 0).is_some_and(|n| n >= 40) {
        let (w, h) = (le(dib, 4)? as i32, le(dib, 8)? as i32);
        let res = le(dib, 24).and_then(per_cm).unwrap_or(default);
        return (w > 0 && h != 0).then_some((w.unsigned_abs(), h.unsigned_abs(), res));
    }
    let (w, h) = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    Some((w, h, default))
}

/// A copied image file (from a file manager) as pasted objects: a Fireworks PNG's objects
/// (frame 1 of its visible layers) or a flat image's bitmap, placed so the file's canvas is
/// centred on a canvas of `canvas` size, as a pasted picture is. Fireworks MX doesn't paste
/// files (Edit > Paste stays disabled with only a file on the clipboard, checked 2026-11-16).
pub fn file_as_paste(bytes: &[u8], name: &str, canvas: (u32, u32)) -> Result<(Vec<Element>, Vec<Symbol>, Vec<Tile>)> {
    let doc = crate::open(bytes, Some(name.to_owned()))?;
    let (dx, dy) = pasted_image_origin(canvas, (doc.width, doc.height));
    let elements: Vec<Element> = doc
        .layers
        .iter()
        .filter(|l| !l.web && l.visible)
        .flat_map(|l| l.elements(0).iter().cloned())
        .map(|mut e| {
            e.translate(dx as f64, dy as f64);
            e
        })
        .collect();
    if elements.is_empty() {
        return Err(Error::Unsupported(format!("{name} has nothing to paste")));
    }
    Ok((elements, doc.symbols.to_vec(), doc.raw_tiles.values().cloned().collect()))
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
        let png = clipboard_png(std::slice::from_ref(&e), &[], &[], (37.0, 12.0, 5.0, 4.0), (150.0, false)).expect("written");
        let (back, _, _) = read_clipboard_png(&png).expect("read");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].name.as_deref(), Some("piece"));
        assert_eq!(back[0].bounds(), Some((37.0, 12.0, 5.0, 4.0)));
        // Other applications see the objects' picture.
        let flat = crate::decode_png(&png).expect("plain png");
        assert_eq!((flat.width(), flat.height()), (5, 4));
        // It carries the document's resolution, as Fireworks MX's does.
        assert_eq!(clipboard_image_info(&png), Some((5, 4, (150.0, false))));
        let doc = crate::open(&png, None).expect("opens");
        assert_eq!((doc.resolution, doc.resolution_cm), (150.0, false));
    }

    #[test]
    fn picture_size_and_resolution_from_headers() {
        let img = RgbaImage::new(123, 45, Rgba::new(0, 90, 200, 255));
        // A plain PNG: pHYs as pixels/cm.
        let png = crate::encode_png(&img, 150.0).expect("png");
        assert_eq!(clipboard_image_info(&png), Some((123, 45, (59.05, true))));
        // A screenshot DIB without a resolution: Fireworks' 72 ppi, 28.34 px/cm.
        let mut dib = vec![0u8; 40];
        dib[0] = 40;
        dib[4..8].copy_from_slice(&1920i32.to_le_bytes());
        dib[8..12].copy_from_slice(&1080i32.to_le_bytes());
        assert_eq!(clipboard_image_info(&dib), Some((1920, 1080, (28.34, true))));
        // A top-down DIB's height is negative.
        dib[8..12].copy_from_slice(&(-1080i32).to_le_bytes());
        dib[24..28].copy_from_slice(&3780u32.to_le_bytes());
        assert_eq!(clipboard_image_info(&dib), Some((1920, 1080, (37.8, true))));
        assert_eq!(clipboard_image_info(b"nothing"), None);
    }

    #[test]
    fn a_copied_file_pastes_centred() {
        let img = RgbaImage::new(101, 61, Rgba::new(200, 30, 30, 255));
        let png = crate::encode_png(&img, 72.0).expect("png");
        let (els, _, _) = file_as_paste(&png, "odd.png", (300, 200)).expect("pasted");
        assert_eq!(els.len(), 1);
        assert_eq!(els[0].bounds(), Some((100.0, 70.0, 101.0, 61.0)));
        assert!(file_as_paste(b"not a picture", "x.png", (300, 200)).is_err());
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

#[cfg(test)]
mod cs_compat_tests {
    use firecraft_doc::{Bitmap, Document, Element, ElementKind, Layer, Symbol, SymbolKind};
    use firecraft_fwpng::Node;
    use firecraft_raster::transform::Affine;
    use firecraft_raster::{Rgba, RgbaImage};

    fn symbol(id: u32) -> Symbol {
        let mut layer = Layer::new("Layer 1");
        layer.cells = vec![vec![Element::new(ElementKind::Bitmap(Bitmap {
            x: -30,
            y: -20,
            image: RgbaImage::new(60, 40, Rgba::new(9, 9, 9, 255)),
        }))]];
        Symbol {
            id,
            name: format!("s{id}"),
            kind: SymbolKind::Graphic,
            layers: vec![layer, Layer::web_layer()],
            frames: 1,
            nine: Some((-10.0, 10.0, -7.0, 7.0)),
            raw: None,
        }
    }

    #[test]
    fn object_lock_and_visibility_round_trip() {
        let mut doc = Document::new(100, 100, 72.0, Some(Rgba::WHITE));
        let bitmap = |x| {
            Element::new(ElementKind::Bitmap(Bitmap {
                x,
                y: 0,
                image: RgbaImage::new(8, 8, Rgba::new(1, 2, 3, 255)),
            }))
        };
        if let Some(cell) = doc.layers[0].elements_mut(0) {
            let mut locked = bitmap(0);
            locked.locked = true;
            let mut hidden = bitmap(20);
            hidden.locked = true;
            hidden.visible = false;
            cell.extend([locked, hidden, bitmap(40)]);
        }
        let back = crate::open(&crate::save_fireworks_png(&doc).expect("saved"), None).expect("opened");
        let flags: Vec<(bool, bool)> = back.layers[0]
            .elements(0)
            .iter()
            .map(|e| (e.locked, e.visible))
            .collect();
        assert_eq!(flags, vec![(true, true), (true, false), (false, true)]);
        // Exactly the two locked objects are written locked.
        fn count(n: &Node) -> usize {
            usize::from(n.is("LCK") && n.text.trim() == "1") + n.children.iter().map(count).sum::<usize>()
        }
        let (tree, _) = crate::fireworks_tree(&back);
        assert_eq!(count(&tree), 2);
    }

    #[test]
    fn cs_markers_dropped_and_skewed_nine_slices_hidden_from_cs() {
        let mut doc = Document::new(300, 200, 72.0, Some(Rgba::WHITE));
        doc.symbols = std::sync::Arc::new(vec![symbol(1), symbol(2)]);
        let inst = |id, m: Affine| {
            Element::new(ElementKind::Instance {
                symbol: id,
                name: String::new(),
                matrix: m,
                bounds: None,
                anim: firecraft_doc::Animation::default(),
            })
        };
        let rotated = Affine {
            a: 0.0,
            b: 2.0,
            c: -2.0,
            d: 0.0,
            e: 100.0,
            f: 100.0,
            ..Affine::IDENTITY
        };
        let perspective = Affine {
            g: 0.001,
            e: 200.0,
            f: 100.0,
            ..Affine::IDENTITY
        };
        if let Some(cell) = doc.layers[0].elements_mut(0) {
            cell.push(inst(1, rotated));
            cell.push(inst(2, perspective));
        }
        // As if read from a Fireworks CS file.
        let mut root = crate::write::new_root();
        root.children.push(Node::int("VRS", 10));
        root.children.push(Node::int("VRM", 1));
        doc.raw = Some(root);
        let back = crate::open(&crate::save_fireworks_png(&doc).expect("saved"), None).expect("opened");
        let (tree, _) = crate::fireworks_tree(&back);
        assert!(tree.child("VRS").is_none() && tree.child("VRM").is_none());
        let msts: Vec<&Node> = tree
            .child("MSL")
            .expect("symbols")
            .children
            .iter()
            .filter(|c| c.is("MST"))
            .collect();
        // Rotated only: CS's node. Perspective: FireCraft's own, which CS ignores.
        assert!(msts[0].child("NGS").is_some() && msts[0].child("FNG").is_none());
        assert!(msts[1].child("NGS").is_none() && msts[1].child("FNG").is_some());
        assert!(
            back.symbols
                .iter()
                .all(|s| s.nine == Some((-10.0, 10.0, -7.0, 7.0)))
        );
    }
}
