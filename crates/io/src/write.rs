//! Document -> Fireworks tree + tiles.
//!
//! Where a document or element was read from a Fireworks PNG, its original node is the template:
//! FireCraft rewrites the fields it models and leaves everything else (effects, textures,
//! presets, export settings) as Fireworks wrote it. New documents get the minimal tree shape
//! observed in Fireworks MX output (see docs/fireworks-png.md).

use firecraft_doc::{Bitmap, Contour, Document, Element, ElementKind, Layer, MaskMode, Path};
use firecraft_fwpng::{Node, Tile};
use firecraft_raster::{ColorMath, Rgba};

use crate::read::IMAGE_TILE;

pub(crate) fn to_argb(c: Rgba) -> i64 {
    (((c.a as u32) << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | c.b as u32) as i32 as i64
}

struct Writer {
    tiles: Vec<Tile>,
    next_id: u32,
    origin: (f64, f64),
    /// Next per-object id (`RNDi`); Fireworks refuses paths without one.
    next_rnd: i64,
}

impl Writer {
    fn tile_id(&mut self) -> u32 {
        self.next_id = self.next_id.wrapping_add(0x10);
        self.next_id
    }
}

/// Replace (or append) a child node with the same tag.
fn put(parent: &mut Node, node: Node) {
    match parent.children.iter_mut().find(|c| c.tag == node.tag) {
        Some(slot) => *slot = node,
        None => parent.children.push(node),
    }
}

/// Build the `mkTS` tree and the tiles it references.
pub fn fireworks_tree(doc: &Document) -> (Node, Vec<Tile>) {
    let mut root = doc.raw.clone().unwrap_or_else(new_root);
    let origin = (root.float_of("XLC").unwrap_or(0.0), root.float_of("YLC").unwrap_or(0.0));
    // New tile ids start above every id of the original file, so kept references stay unique.
    let start = doc.raw_tiles.keys().copied().max().unwrap_or(0).max(0x0100_0000) & !0xF;
    let mut w = Writer { tiles: Vec::new(), next_id: start, origin, next_rnd: 0x0060_0000 };

    put(&mut root, Node::int("FRC", doc.frames.len().max(1) as i64));
    put(&mut root, Node::float("WID", doc.width as f64));
    put(&mut root, Node::float("HIT", doc.height as f64));
    put(&mut root, Node::float("RES", doc.resolution));
    put(&mut root, Node::int("RSU", i64::from(doc.resolution_cm)));
    put(&mut root, Node::int("BGC", to_argb(doc.canvas.unwrap_or(Rgba::CLEARED))));
    // FireCraft extension (Fireworks MX ignores unknown nodes): FCMi 1 = high-precision colour math.
    root.children.retain(|c| !c.is("FCM"));
    if doc.color_math == ColorMath::Precise {
        root.children.push(Node::int("FCM", 1));
    }
    let layers =
        doc.layers.iter().map(|l| write_layer(l, doc, &mut w)).collect();
    put(&mut root, Node::vector("LYL", layers));
    // Frame delays/export flags back into EXPv > FNLv (kept as read otherwise).
    if let Some(fnl) = root.child_mut("EXP").and_then(|e| e.child_mut("FNL")) {
        let fnms: Vec<&mut Node> = fnl.children.iter_mut().filter(|c| c.is("FNM")).collect();
        for (f, n) in doc.frames.iter().zip(fnms) {
            put(n, Node::int("FND", f.delay as i64));
            put(n, Node::bool("VIS", f.export));
        }
    }
    // Carry over original tiles still referenced (presets, textures, cached text, mkBS images).
    let mut referenced = Vec::new();
    collect_tile_refs(&root, &mut referenced, 0);
    if let Some(s) = &doc.raw_secondary {
        collect_tile_refs(s, &mut referenced, 0);
    }
    referenced.sort_unstable();
    referenced.dedup();
    let generated: std::collections::HashSet<u32> = w.tiles.iter().map(|t| t.id).collect();
    for id in referenced {
        if !generated.contains(&id) {
            if let Some(t) = doc.raw_tiles.get(&id) {
                w.tiles.push(t.clone());
            }
        }
    }
    (root, w.tiles)
}

fn collect_tile_refs(n: &Node, out: &mut Vec<u32>, depth: usize) {
    if n.is("TID") {
        out.extend(n.as_u32());
    }
    if depth < firecraft_fwpng::tree::MAX_DEPTH {
        n.children.iter().for_each(|c| collect_tile_refs(c, out, depth + 1));
    }
}

fn new_root() -> Node {
    Node::vector(
        "MKB",
        vec![
            Node::int("FRC", 1),
            Node::float("XLC", 0.0),
            Node::float("YLC", 0.0),
            Node::float("WID", 1.0),
            Node::float("HIT", 1.0),
            Node::float("RES", 72.0),
            Node::int("RSU", 0),
            Node::int("BGC", -1),
            Node::vector("LYL", vec![]),
            Node::vector(
                "GRD",
                vec![
                    Node::float("GOX", 0.0),
                    Node::float("GOY", 0.0),
                    Node::float("GSX", 36.0),
                    Node::float("GSY", 36.0),
                    Node::int("GCL", 0xff10_1010_u32 as i32 as i64),
                ],
            ),
            Node::vector(
                "GDS",
                vec![
                    Node::bool("GDL", false),
                    Node::int("GDK", 0xff00_ff00_u32 as i32 as i64),
                    Node::vector("GDH", vec![]),
                    Node::vector("GDV", vec![]),
                ],
            ),
            Node::int("OSB", 0),
            Node::int("OSA", 0),
        ],
    )
}

fn write_layer(layer: &Layer, doc: &Document, w: &mut Writer) -> Node {
    let mut n = layer.raw.clone().unwrap_or_else(|| {
        Node::vector(
            "LAY",
            vec![
                Node::bool("BKG", false),
                Node::bool("URL", false),
                Node::string("LNM", ""),
                Node::bool("MSL", false),
                Node::bool("MCL", false),
                Node::bool("DIS", true),
                Node::vector("CLL", vec![]),
                Node::vector("VIF", vec![]),
            ],
        )
    });
    put(&mut n, Node::bool("URL", layer.web));
    put(&mut n, Node::string("LNM", &layer.name));
    // MSLb: meaning unknown (1 on the Web Layer); kept as read, set for new layers.
    if layer.raw.is_none() {
        put(&mut n, Node::bool("MSL", layer.web));
    }
    put(&mut n, Node::bool("MCL", !layer.shared));
    put(&mut n, Node::bool("DIS", layer.disclosed));
    let cell_count = if layer.shared { 1 } else { doc.frames.len().max(1) };
    let cells = (0..cell_count)
        .map(|f| {
            // Top first on disk (see read_elements).
            let elements = layer.elements(f).iter().rev().map(|e| write_element(e, w, 0)).collect();
            Node::vector("CEL", vec![Node::bool("LCK", layer.locked), Node::vector("ELM", elements)])
        })
        .collect();
    put(&mut n, Node::vector("CLL", cells));
    let frames = doc.frames.len().max(1);
    put(&mut n, Node::vector("VIF", (0..frames).map(|f| Node::bool("VIS", layer.visible_in(f))).collect()));
    n
}

fn write_element(el: &Element, w: &mut Writer, depth: usize) -> Node {
    let mut n = match &el.kind {
        ElementKind::Bitmap(b) => write_bitmap(b, el.raw.as_ref().filter(|r| r.is("IMG")), w),
        ElementKind::Path(p) => write_path(p, el.raw.as_ref().filter(|r| r.is("PTH")), w),
        ElementKind::Group(children) if depth < 64 => {
            let mut g = el
                .raw
                .clone()
                .filter(|r| r.is("GRP"))
                .unwrap_or_else(|| Node::vector("GRP", vec![Node::int("GRT", 0)]));
            put(&mut g, Node::vector("ELM", children.iter().rev().map(|c| write_element(c, w, depth + 1)).collect()));
            g
        }
        // Text and other elements are re-emitted as read. A text element created in FireCraft (no
        // raw node yet) is saved as its rendered bitmap until the text engine lands.
        ElementKind::Text(t) => match (&el.raw, &t.cached) {
            (Some(raw), _) => raw.clone(),
            (None, Some(b)) => write_bitmap(b, None, w),
            (None, None) => Node::vector("GRP", vec![Node::int("GRT", 0), Node::vector("ELM", vec![])]),
        },
        ElementKind::Other { cached, .. } => match (&el.raw, cached) {
            (Some(raw), _) => raw.clone(),
            (None, Some(b)) => write_bitmap(b, None, w),
            (None, None) => Node::vector("GRP", vec![Node::int("GRT", 0), Node::vector("ELM", vec![])]),
        },
        ElementKind::Group(_) => Node::vector("GRP", vec![Node::int("GRT", 0), Node::vector("ELM", vec![])]),
        ElementKind::Masked { content, mask, mode, cache } if depth < 64 => {
            let mut g = el.raw.clone().filter(|r| r.is("GRP")).unwrap_or_else(|| Node::vector("GRP", vec![]));
            put(&mut g, Node::bool("MNA", *mode != MaskMode::Alpha));
            put(&mut g, Node::int("GRT", if *mode == MaskMode::PathOutline { 2 } else { 1 }));
            let mut elm: Vec<Node> = Vec::new();
            if *mode == MaskMode::PathOutline {
                elm.push(write_element(mask, w, depth + 1));
            } else if let Some(c) = cache {
                elm.push(write_element(c, w, depth + 1));
            }
            elm.extend(content.iter().rev().map(|c| write_element(c, w, depth + 1)));
            put(&mut g, Node::vector("ELM", elm));
            if *mode == MaskMode::PathOutline {
                g.children.retain(|c| !c.is("MRM"));
            } else {
                put(&mut g, Node::vector("MRM", vec![write_element(mask, w, depth + 1)]));
            }
            g
        }
        ElementKind::Masked { .. } => Node::vector("GRP", vec![Node::int("GRT", 0), Node::vector("ELM", vec![])]),
        ElementKind::Instance { symbol, matrix, .. } => {
            let mut n = el.raw.clone().filter(|r| r.is("ALS")).unwrap_or_else(|| Node::vector("ALS", vec![Node::int("MSN", *symbol as i64)]));
            let (ox, oy) = w.origin;
            let mtx = Node::vector(
                "MTX",
                vec![
                    Node::float("M00", matrix.a),
                    Node::float("M01", matrix.b),
                    Node::float("M02", 0.0),
                    Node::float("M10", matrix.c),
                    Node::float("M11", matrix.d),
                    Node::float("M12", 0.0),
                    Node::float("M20", matrix.e + ox),
                    Node::float("M21", matrix.f + oy),
                    Node::float("M22", 1.0),
                ],
            );
            put(&mut n, mtx);
            n
        }
    };
    // Visibility: Fireworks writes VISb{0} (first) for hidden objects only.
    if el.visible {
        n.children.retain(|c| !c.is("VIS"));
    } else if n.child("VIS").is_none() {
        n.children.insert(0, Node::bool("VIS", false));
    }
    // Blend mode: Normal is written as an absent tag (as Fireworks does).
    if el.blend != firecraft_raster::BlendMode::Normal {
        let bld = Node::int("BLD", el.blend.fw_code());
        match n.children.iter().position(|c| c.is("BLD")) {
            Some(i) => n.children[i] = bld,
            // Fireworks writes BLDi just before the effect list.
            None => match n.children.iter().position(|c| c.is("EFL")) {
                Some(i) => n.children.insert(i, bld),
                None => n.children.push(bld),
            },
        }
    } else {
        n.children.retain(|c| !c.is("BLD"));
    }
    if el.opacity < 100.0 || n.child("OPA").is_some() {
        put(&mut n, Node::int("OPA", (el.opacity.clamp(0.0, 100.0) * 10.0).round() as i64));
    }
    n
}

fn write_bitmap(b: &Bitmap, raw: Option<&Node>, w: &mut Writer) -> Node {
    let mut n = raw.cloned().unwrap_or_else(|| Node::vector("IMG", vec![]));
    let (iw, ih) = (b.image.width(), b.image.height());
    // The reader already cropped away pixels hidden by XOFf/YOFf, so the offset is 0 here.
    put(&mut n, Node::float("XOF", 0.0));
    put(&mut n, Node::float("YOF", 0.0));
    put(&mut n, Node::float("XLC", b.x as f64 + w.origin.0));
    put(&mut n, Node::float("YLC", b.y as f64 + w.origin.1));
    put(&mut n, Node::int("WPX", iw as i64));
    put(&mut n, Node::int("HPX", ih as i64));
    put(&mut n, Node::int("TSZ", IMAGE_TILE as i64));
    let mut ids = Vec::new();
    for ty in (0..ih.max(1)).step_by(IMAGE_TILE as usize) {
        for tx in (0..iw.max(1)).step_by(IMAGE_TILE as usize) {
            let mut data = vec![0u8; (IMAGE_TILE * IMAGE_TILE * 4) as usize];
            for y in 0..IMAGE_TILE {
                for x in 0..IMAGE_TILE {
                    let Some(p) = b.image.get((tx + x) as i64, (ty + y) as i64) else { continue };
                    let o = ((y * IMAGE_TILE + x) * 4) as usize;
                    if let Some(d) = data.get_mut(o..o + 4) {
                        d.copy_from_slice(&[p.a, p.r, p.g, p.b]);
                    }
                }
            }
            let id = w.tile_id();
            w.tiles.push(Tile::new(id, data));
            ids.push(Node::int("TID", id as i64));
        }
    }
    put(&mut n, Node::vector("TIL", ids));
    n
}

fn contour_node(c: &Contour, origin: (f64, f64)) -> Node {
    let points = c
        .points
        .iter()
        .map(|p| {
            // Absolute coordinates; a control point equal to its anchor is omitted (as Fireworks does).
            let (ax, ay) = (p.x + origin.0, p.y + origin.1);
            let mut kids = vec![Node::float("XLC", ax), Node::float("YLC", ay)];
            if p.pred != (0.0, 0.0) {
                kids.push(Node::float("XPC", ax + p.pred.0));
                kids.push(Node::float("YPC", ay + p.pred.1));
            }
            if p.succ != (0.0, 0.0) {
                kids.push(Node::float("XSC", ax + p.succ.0));
                kids.push(Node::float("YSC", ay + p.succ.1));
            }
            Node::vector("PBT", kids)
        })
        .collect::<Vec<_>>();
    let mut kids = vec![Node::bool("ISC", c.closed), Node::float("BSL", 0.0), Node::int("PPC", points.len() as i64)];
    kids.extend(points);
    Node::vector("PBP", kids)
}

fn write_path(p: &Path, raw: Option<&Node>, w: &mut Writer) -> Node {
    let mut n = raw.cloned().unwrap_or_else(|| {
        Node::vector("PTH", vec![Node::vector("PAT", vec![]), Node::int("BRP", 1), Node::int("PTT", 0), Node::bool("FOT", false)])
    });
    if n.child("RND").is_none() {
        w.next_rnd += 1;
        let at = n.children.iter().position(|c| c.is("FOT")).map(|i| i + 1).unwrap_or(n.children.len());
        n.children.insert(at, Node::int("RND", w.next_rnd));
    }
    let pbl = Node::vector("PBL", p.contours.iter().map(|c| contour_node(c, w.origin)).collect());
    // Geometry the original file had, rebuilt the same way: if unchanged, keep Fireworks' cached
    // stroke polyline (PCLv); otherwise drop it so it is recomputed.
    let unchanged = raw.and_then(|r| r.child("PBL")).map(|old| same_geometry(old, &pbl)).unwrap_or(false);
    if !unchanged {
        n.children.retain(|c| !c.is("PCL"));
    }
    if !unchanged {
        put(&mut n, pbl);
    }
    let mut gradient_handles = None;
    if let Some(attrs) = n.child_mut("PAT") {
        match &p.fill {
            Some(f) => {
                if let Some(g) = &f.gradient {
                    gradient_handles = Some((g.start, g.end, g.side));
                }
                put(attrs, Node::int("FCL", to_argb(f.color)));
                if attrs.child("FPL").is_none() {
                    attrs.children.push(fill_preset());
                }
            }
            None => attrs.children.retain(|c| !c.is("FPL")),
        }
        match &p.stroke {
            Some(s) => {
                put(attrs, Node::int("BCL", to_argb(s.color)));
                if attrs.child("BPL").is_none() {
                    attrs.children.push(brush_preset(s.diameter));
                }
            }
            None => attrs.children.retain(|c| !c.is("BPL")),
        }
        // Fireworks MX refuses a path whose attributes lack either colour, even when there is no
        // stroke or no fill (verified with tools/fwmx-oracle). Order as Fireworks writes: BCL, FCL.
        if attrs.child("FCL").is_none() {
            attrs.children.insert(0, Node::int("FCL", to_argb(Rgba::BLACK)));
        }
        if attrs.child("BCL").is_none() {
            attrs.children.insert(0, Node::int("BCL", to_argb(Rgba::BLACK)));
        }
    }
    // Gradient handles follow the object when it is moved or scaled.
    if let Some((s, e, f)) = gradient_handles {
        let (ox, oy) = w.origin;
        for (tag, v) in [("PSX", s.0 + ox), ("PSY", s.1 + oy), ("PEX", e.0 + ox), ("PEY", e.1 + oy), ("PFX", f.0 + ox), ("PFY", f.1 + oy)] {
            put(&mut n, Node::float(tag, v));
        }
    }
    n
}

fn same_geometry(old: &Node, new: &Node) -> bool {
    let pts = |n: &Node| -> Vec<(i64, i64)> {
        n.children_tagged("PBP")
            .flat_map(|c| c.children_tagged("PBT").collect::<Vec<_>>())
            .map(|p| {
                let r = |t: &str| (p.float_of(t).unwrap_or(0.0) * 100.0).round() as i64;
                (r("XLC"), r("YLC"))
            })
            .collect()
    };
    pts(old) == pts(new)
}

/// Solid fill preset as Fireworks MX writes it.
fn fill_preset() -> Node {
    Node::vector(
        "FPL",
        vec![
            Node::string("CAT", "fc_Solid"),
            Node::string("INM", "fn_Normal"),
            Node::string("UNM", "fn_Normal"),
            Node::bool("RDO", false),
            Node::int("FTB", 0),
            Node::int("FEF", 0),
            Node::int("FSM", 2),
            Node::int("FET", 1),
            Node::bool("FRD", false),
            Node::int("FRR", 0),
            Node::int("FSH", 0),
        ],
    )
}

/// "Pencil / 1-Pixel Anti-Aliased"-style brush preset (field values as in Fireworks MX output).
fn brush_preset(diameter: f64) -> Node {
    let i = |t: &str, v: i64| Node::int(t, v);
    Node::vector(
        "BPL",
        vec![
            Node::string("CAT", "bc_Pencil"),
            Node::string("INM", "bn_1-Pixel Anti-Aliased"),
            Node::bool("RDO", false),
            i("BAN", 0),
            i("BAS", 1000),
            i("BDI", diameter.round().clamp(1.0, 1000.0) as i64),
            i("BMM", 15),
            i("BMS", 10),
            i("BSE", 0),
            i("BSF", 1),
            i("BSH", 1),
            i("BBL", 0),
            i("BBK", 0),
            i("BCN", 1000),
            i("BEF", 0),
            i("BRT", 1),
            i("BFB", 1),
            i("BFR", 0),
            i("BNT", 1),
            i("BSP", 150),
            i("BTB", 0),
            i("BTE", 0),
            i("BTS", 0),
            i("BSM", 0),
            i("BCM", 0),
            Node::bool("BIA", true),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use firecraft_doc::{BezierPoint, Fill};
    use firecraft_raster::RgbaImage;

    fn sample() -> Document {
        let mut img = RgbaImage::new(200, 130, Rgba::WHITE);
        img.set(150, 129, Rgba::new(10, 20, 30, 128));
        let mut doc = Document::from_image(img, None);
        let path = Path {
            contours: vec![Contour {
                closed: true,
                points: vec![BezierPoint::corner(10.0, 10.0), BezierPoint::corner(60.0, 10.0), BezierPoint::corner(35.0, 50.0)],
            }],
            fill: Some(Fill { color: Rgba::new(0, 102, 153, 255), category: "fc_Solid".into(), anti_alias: true, gradient: None }),
            stroke: None,
        };
        doc.layers[0].cells[0].push(Element::new(ElementKind::Path(path)));
        doc
    }

    #[test]
    fn save_then_open_preserves_document() {
        let doc = sample();
        let bytes = crate::save_fireworks_png(&doc).unwrap();
        let back = crate::open(&bytes, None).unwrap();
        assert_eq!((back.width, back.height), (200, 130));
        assert_eq!(back.layers.len(), 2);
        assert!(back.layers[1].web);
        let els = back.layers[0].elements(0);
        assert_eq!(els.len(), 2);
        let ElementKind::Bitmap(b) = &els[0].kind else { panic!("bitmap expected") };
        assert_eq!(b.image, match &doc.layers[0].cells[0][0].kind {
            ElementKind::Bitmap(o) => o.image.clone(),
            _ => unreachable!(),
        });
        assert_eq!(back.render(0), doc.render(0));
        // A second save of the re-opened document is stable.
        let again = crate::save_fireworks_png(&back).unwrap();
        assert_eq!(crate::open(&again, None).unwrap().render(0), doc.render(0));
    }

    #[test]
    fn flat_png_opens_as_one_bitmap() {
        let img = RgbaImage::new(3, 2, Rgba::new(1, 2, 3, 4));
        let png = crate::encode_png(&img, 72.0).unwrap();
        let doc = crate::open(&png, Some("x.png".into())).unwrap();
        assert_eq!(doc.render(0).get(2, 1), Some(Rgba::new(1, 2, 3, 4)));
        assert_eq!(doc.title(), "x.png");
    }
}
