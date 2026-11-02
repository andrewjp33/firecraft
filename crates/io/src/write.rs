//! Document -> Fireworks tree + tiles.
//!
//! Where a document or element was read from a Fireworks PNG, its original node is the template:
//! FireCraft rewrites the fields it models and leaves everything else (effects, textures,
//! presets, export settings) as Fireworks wrote it. New documents get the minimal tree shape
//! observed in Fireworks MX output (see docs/fireworks-png.md).

use firecraft_doc::{Bitmap, Contour, Document, Element, ElementKind, Layer, MaskMode, Path, SymbolKind};
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
    /// Frame count of each symbol (new instances record it, `ANNi`).
    symbol_frames: std::collections::HashMap<u32, usize>,
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
    // A Fireworks CS file's MKBv carries CS-only fields (version VRSi/VRMi, page ids...) that
    // tell CS to read the CS document (PDCv) stored after it. FireCraft writes only MKBv (PDCv
    // would be stale after edits), so drop them: CS then reads the file like an MX one, as it
    // does files from Fireworks MX (otherwise it shows an empty canvas).
    root.children.retain(|c| !CS_ROOT_ONLY.iter().any(|t| c.is(t)));
    let origin = (
        root.float_of("XLC").unwrap_or(0.0),
        root.float_of("YLC").unwrap_or(0.0),
    );
    // New tile ids start above every id of the original file, so kept references stay unique.
    let start = doc.raw_tiles.keys().copied().max().unwrap_or(0).max(0x0100_0000) & !0xF;
    let mut w = Writer {
        tiles: Vec::new(),
        next_id: start,
        origin,
        next_rnd: 0x0060_0000,
        symbol_frames: doc.symbols.iter().map(|s| (s.id, s.frames)).collect(),
    };

    put(&mut root, Node::int("FRC", doc.frames.len().max(1) as i64));
    put(&mut root, Node::float("WID", doc.width as f64));
    put(&mut root, Node::float("HIT", doc.height as f64));
    put(&mut root, Node::float("RES", doc.resolution));
    put(&mut root, Node::int("RSU", i64::from(doc.resolution_cm)));
    put(
        &mut root,
        Node::int("BGC", to_argb(doc.canvas.unwrap_or(Rgba::CLEARED))),
    );
    // FireCraft extension (Fireworks MX ignores unknown nodes): FCMi 1 = high-precision colour math.
    root.children.retain(|c| !c.is("FCM"));
    if doc.color_math == ColorMath::Precise {
        root.children.push(Node::int("FCM", 1));
    }
    let frames = doc.frames.len().max(1);
    let layers = doc
        .layers
        .iter()
        .map(|l| write_layer(l, frames, &mut w))
        .collect();
    put(&mut root, Node::vector("LYL", layers));
    write_symbols(&mut root, doc, &mut w);
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
        if !generated.contains(&id)
            && let Some(t) = doc.raw_tiles.get(&id)
        {
            w.tiles.push(t.clone());
        }
    }
    (root, w.tiles)
}

fn collect_tile_refs(n: &Node, out: &mut Vec<u32>, depth: usize) {
    if n.is("TID") {
        out.extend(n.as_u32());
    }
    if depth < firecraft_fwpng::tree::MAX_DEPTH {
        n.children
            .iter()
            .for_each(|c| collect_tile_refs(c, out, depth + 1));
    }
}

pub(crate) fn new_root() -> Node {
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

/// Root fields only Fireworks CS writes (see `fireworks_tree`).
const CS_ROOT_ONLY: [&str; 8] = ["VRS", "VRM", "PID", "PIL", "ILL", "IMP", "LSM", "ZSL"];

/// Whether any instance of symbol `id` (in the document or in symbols) is skewed or in
/// perspective (its transformed axes aren't perpendicular, or it isn't affine).
fn symbol_skewed(doc: &Document, id: u32) -> bool {
    fn any(els: &[Element], id: u32) -> bool {
        els.iter().any(|e| match &e.kind {
            ElementKind::Instance { symbol, matrix, .. } if *symbol == id => {
                let dot = matrix.a * matrix.c + matrix.b * matrix.d;
                let scale = (matrix.a.hypot(matrix.b) * matrix.c.hypot(matrix.d)).max(1e-12);
                !matrix.is_affine() || (dot / scale).abs() > 1e-6
            }
            ElementKind::Group(children) => any(children, id),
            ElementKind::Masked { content, .. } => any(content, id),
            _ => false,
        })
    }
    doc.layers.iter().flat_map(|l| l.cells.iter()).any(|c| any(c, id))
        || doc
            .symbols
            .iter()
            .flat_map(|s| s.layers.iter().flat_map(|l| l.cells.iter()))
            .any(|c| any(c, id))
}

/// `MSLv`: the symbol definitions (`MSTv`), in symbol space (no canvas origin). A symbol read
/// from a file keeps its original node for what FireCraft doesn't model; a new one gets
/// Fireworks MX's defaults (as Convert to Symbol writes them).
fn write_symbols(root: &mut Node, doc: &Document, w: &mut Writer) {
    if doc.symbols.is_empty() {
        root.children.retain(|c| !c.is("MSL"));
        return;
    }
    let saved = w.origin;
    w.origin = (0.0, 0.0);
    let defaults = new_root();
    let symbols = doc
        .symbols
        .iter()
        .map(|s| {
            let mut n = s.raw.clone().unwrap_or_else(|| {
                let mut kids = vec![
                    Node::string("UNM", ""),
                    Node::int("MSN", 0),
                    Node::float("MOX", 0.0),
                    Node::float("MOY", 0.0),
                    Node::vector("LYL", vec![]),
                ];
                kids.extend(
                    defaults
                        .children
                        .iter()
                        .filter(|c| c.is("GRD") || c.is("GDS"))
                        .cloned(),
                );
                kids.extend([
                    Node::int("FRC", 1),
                    Node::int("OSB", 0),
                    Node::int("OSA", 0),
                    Node::int("BGC", 0x00ff_ffff),
                    Node::bool("ATS", true),
                    Node::bool("IDS", false),
                    Node::bool("DVL", false),
                    Node::bool("IOD", false),
                ]);
                Node::vector("MST", kids)
            });
            put(&mut n, Node::string("UNM", &s.name));
            put(&mut n, Node::int("MSN", i64::from(s.id)));
            let kind = match s.kind {
                SymbolKind::Graphic => None,
                SymbolKind::Button => Some(1),
                SymbolKind::Animation => Some(2),
            };
            match kind {
                // MSYi follows MSNi.
                Some(k) => match n.children.iter().position(|c| c.is("MSY")) {
                    Some(i) => n.children[i] = Node::int("MSY", k),
                    None => {
                        let at = n.children.iter().position(|c| c.is("MSN")).map_or(0, |i| i + 1);
                        n.children.insert(at, Node::int("MSY", k));
                    }
                },
                None => n.children.retain(|c| !c.is("MSY")),
            }
            // 9-slice guides (Fireworks CS's NGSv, after GDSv). Fireworks MX ignores the node
            // (the file opens, the symbol scales plainly); CS reads it. Fireworks CS doesn't
            // allow skewing or distorting a 9-slice instance (it asks to remove the slices) and
            // hangs opening one, so a symbol with such an instance keeps its guides in a
            // FireCraft-only node (FNGv) that MX and CS ignore: CS shows it unsliced.
            n.children.retain(|c| !c.is("FNG"));
            let cs_safe = !symbol_skewed(doc, s.id);
            if let (Some((left, right, top, bottom)), false) = (s.nine, cs_safe) {
                n.children.retain(|c| !c.is("NGS"));
                let fng = Node::vector(
                    "FNG",
                    [top, bottom, left, right]
                        .into_iter()
                        .map(|v| Node::float("NGC", v))
                        .collect(),
                );
                let at = n
                    .children
                    .iter()
                    .position(|c| c.is("GDS"))
                    .map_or(n.children.len(), |i| i + 1);
                n.children.insert(at, fng);
            }
            match s.nine.filter(|_| cs_safe) {
                Some((left, right, top, bottom)) => {
                    let mut ngs = n.child("NGS").cloned().unwrap_or_else(|| {
                        Node::vector(
                            "NGS",
                            vec![
                                Node::bool("NGL", false),
                                Node::int("NGK", 0xff00_00ff_u32 as i64),
                                Node::vector("NGM", vec![]),
                                Node::vector("NGV", vec![]),
                            ],
                        )
                    });
                    ngs.children.retain(|c| !c.is("NGC"));
                    let at = ngs
                        .children
                        .iter()
                        .position(|c| c.is("NGM"))
                        .unwrap_or(ngs.children.len());
                    for (k, v) in [top, bottom, left, right].into_iter().enumerate() {
                        ngs.children.insert(at + k, Node::float("NGC", v));
                    }
                    match n.children.iter().position(|c| c.is("NGS")) {
                        Some(i) => n.children[i] = ngs,
                        None => {
                            let at = n
                                .children
                                .iter()
                                .position(|c| c.is("GDS"))
                                .map_or(n.children.len(), |i| i + 1);
                            n.children.insert(at, ngs);
                        }
                    }
                }
                None => n.children.retain(|c| !c.is("NGS")),
            }
            let frames = s.frames.max(1);
            let layers = s.layers.iter().map(|l| write_layer(l, frames, w)).collect();
            put(&mut n, Node::vector("LYL", layers));
            put(&mut n, Node::int("FRC", frames as i64));
            n
        })
        .collect();
    w.origin = saved;
    let msl = Node::vector("MSL", symbols);
    match root.children.iter().position(|c| c.is("MSL")) {
        Some(i) => root.children[i] = msl,
        // Fireworks writes MSLv after the preset lists, before PATv/EXPv/LYLv.
        None => {
            let at = root
                .children
                .iter()
                .position(|c| c.is("PAT") || c.is("EXP") || c.is("MNF") || c.is("LYL"))
                .unwrap_or(root.children.len());
            root.children.insert(at, msl);
        }
    }
}

fn write_layer(layer: &Layer, frames: usize, w: &mut Writer) -> Node {
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
    let cell_count = if layer.shared { 1 } else { frames };
    let cells = (0..cell_count)
        .map(|f| {
            // Top first on disk (see read_elements).
            let elements = layer
                .elements(f)
                .iter()
                .rev()
                .map(|e| write_element(e, w, 0))
                .collect();
            Node::vector(
                "CEL",
                vec![Node::bool("LCK", layer.locked), Node::vector("ELM", elements)],
            )
        })
        .collect();
    put(&mut n, Node::vector("CLL", cells));
    put(
        &mut n,
        Node::vector(
            "VIF",
            (0..frames)
                .map(|f| Node::bool("VIS", layer.visible_in(f)))
                .collect(),
        ),
    );
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
            put(
                &mut g,
                Node::vector(
                    "ELM",
                    children
                        .iter()
                        .rev()
                        .map(|c| write_element(c, w, depth + 1))
                        .collect(),
                ),
            );
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
        ElementKind::Masked {
            content,
            mask,
            mode,
            cache,
        } if depth < 64 => {
            let mut g = el
                .raw
                .clone()
                .filter(|r| r.is("GRP"))
                .unwrap_or_else(|| Node::vector("GRP", vec![]));
            put(&mut g, Node::bool("MNA", *mode != MaskMode::Alpha));
            put(
                &mut g,
                Node::int("GRT", if *mode == MaskMode::PathOutline { 2 } else { 1 }),
            );
            let mut elm: Vec<Node> = Vec::new();
            // A bitmap mask (no cached render of a separate mask object) is stored inline as
            // the top ELMv child, without MRMv.
            let inline = *mode == MaskMode::PathOutline
                || (cache.is_none() && matches!(mask.kind, ElementKind::Bitmap(_)));
            if inline {
                let mut stored = (**mask).clone();
                if let ElementKind::Bitmap(b) = &mut stored.kind {
                    if *mode != MaskMode::PathOutline {
                        crate::read::invert_mask_bitmap(&mut b.image);
                    }
                }
                elm.push(write_element(&stored, w, depth + 1));
            } else if let Some(c) = cache {
                elm.push(write_element(c, w, depth + 1));
            }
            elm.extend(content.iter().rev().map(|c| write_element(c, w, depth + 1)));
            put(&mut g, Node::vector("ELM", elm));
            if inline {
                g.children.retain(|c| !c.is("MRM"));
            } else {
                put(
                    &mut g,
                    Node::vector("MRM", vec![write_element(mask, w, depth + 1)]),
                );
            }
            g
        }
        ElementKind::Masked { .. } => {
            Node::vector("GRP", vec![Node::int("GRT", 0), Node::vector("ELM", vec![])])
        }
        ElementKind::Instance { symbol, matrix, .. } => {
            let mut n = el.raw.clone().filter(|r| r.is("ALS")).unwrap_or_else(|| {
                // A new instance, with Fireworks MX's defaults (MTXv goes after MSNi).
                let frames = w.symbol_frames.get(symbol).copied().unwrap_or(1);
                Node::vector(
                    "ALS",
                    vec![
                        Node::int("MSN", *symbol as i64),
                        Node::vector("MTX", vec![]),
                        Node::int("XFM", 0),
                        Node::int("ANN", frames as i64),
                        Node::float("ANX", 0.0),
                        Node::float("ANY", 0.0),
                        Node::float("ASC", 100.0),
                    ],
                )
            });
            let (ox, oy) = w.origin;
            let m = firecraft_raster::transform::Affine::translate(ox, oy).then(matrix);
            let mtx = Node::vector(
                "MTX",
                vec![
                    Node::float("M00", m.a),
                    Node::float("M01", m.b),
                    Node::float("M02", m.g),
                    Node::float("M10", m.c),
                    Node::float("M11", m.d),
                    Node::float("M12", m.h),
                    Node::float("M20", m.e),
                    Node::float("M21", m.f),
                    Node::float("M22", m.i),
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
    // Per-object lock `LCKb` (Fireworks CS writes it on every object, after VISb or first;
    // Fireworks MX ignores it). Written when locked, or kept as read.
    match n.children.iter().position(|c| c.is("LCK")) {
        Some(i) => n.children[i] = Node::bool("LCK", el.locked),
        None if el.locked => {
            let at = n.children.iter().position(|c| c.is("VIS")).map_or(0, |i| i + 1);
            n.children.insert(at, Node::bool("LCK", true));
        }
        None => {}
    }
    // Object name `OBNs`: after the object's own data, before blending, effects and opacity.
    match &el.name {
        Some(name) => {
            let obn = Node::string("OBN", name);
            match n.children.iter().position(|c| c.is("OBN")) {
                Some(i) => n.children[i] = obn,
                None => match n
                    .children
                    .iter()
                    .position(|c| c.is("BLD") || c.is("EFL") || c.is("OPA"))
                {
                    Some(i) => n.children.insert(i, obn),
                    None => n.children.push(obn),
                },
            }
        }
        None => n.children.retain(|c| !c.is("OBN")),
    }
    write_effect_list(&mut n, &el.effects);
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
        put(
            &mut n,
            Node::int("OPA", (el.opacity.clamp(0.0, 100.0) * 10.0).round() as i64),
        );
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
                    let Some(p) = b.image.get((tx + x) as i64, (ty + y) as i64) else {
                        continue;
                    };
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
    let mut kids = vec![
        Node::bool("ISC", c.closed),
        Node::float("BSL", 0.0),
        Node::int("PPC", points.len() as i64),
    ];
    kids.extend(points);
    Node::vector("PBP", kids)
}

fn write_path(p: &Path, raw: Option<&Node>, w: &mut Writer) -> Node {
    let mut n = raw.cloned().unwrap_or_else(|| {
        Node::vector(
            "PTH",
            vec![
                Node::vector("PAT", vec![]),
                Node::int("BRP", 1),
                Node::int("PTT", 0),
                Node::bool("FOT", false),
            ],
        )
    });
    if n.child("RND").is_none() {
        w.next_rnd += 1;
        let at = n
            .children
            .iter()
            .position(|c| c.is("FOT"))
            .map(|i| i + 1)
            .unwrap_or(n.children.len());
        n.children.insert(at, Node::int("RND", w.next_rnd));
    }
    let pbl = Node::vector(
        "PBL",
        p.contours.iter().map(|c| contour_node(c, w.origin)).collect(),
    );
    // Geometry the original file had, rebuilt the same way: if unchanged, keep Fireworks' cached
    // stroke polyline (PCLv); otherwise drop it so it is recomputed.
    let unchanged = raw
        .and_then(|r| r.child("PBL"))
        .map(|old| same_geometry(old, &pbl))
        .unwrap_or(false);
    // A Fireworks shape primitive (PRIv: rectangle as a base box and a matrix) is rebuilt from
    // its record, not from the points: keep the record in step with the points.
    if n.child("PRI").is_some() {
        sync_primitive(&mut n, raw.and_then(|r| r.child("PBL")), &pbl, unchanged);
    }
    if !unchanged {
        n.children.retain(|c| !c.is("PCL"));
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
        for (tag, v) in [
            ("PSX", s.0 + ox),
            ("PSY", s.1 + oy),
            ("PEX", e.0 + ox),
            ("PEY", e.1 + oy),
            ("PFX", f.0 + ox),
            ("PFY", f.1 + oy),
        ] {
            put(&mut n, Node::float(tag, v));
        }
    }
    n
}

/// Bring a path's shape primitive (PRIv) in step with its points `pbl`: when the record
/// matched the points as read (`old`), apply the affine transform from those to the new points;
/// when it didn't (a file saved by an earlier FireCraft that moved the points only), fit the
/// transform from the record's own rectangle corners. If no affine transform fits, drop the
/// record (Fireworks then opens a plain path).
fn sync_primitive(n: &mut Node, old: Option<&Node>, pbl: &Node, unchanged: bool) {
    let now = anchors(pbl);
    let corners = n.child("PRI").and_then(primitive_corners);
    let bbox = |pts: &[(f64, f64)]| {
        pts.iter()
            .fold((f64::MAX, f64::MAX, f64::MIN, f64::MIN), |b, &(x, y)| {
                (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y))
            })
    };
    let agree = |pts: &[(f64, f64)]| {
        corners.as_ref().is_some_and(|c| {
            let (a, b) = (bbox(pts), bbox(c));
            [(a.0, b.0), (a.1, b.1), (a.2, b.2), (a.3, b.3)]
                .iter()
                .all(|(u, v)| (u - v).abs() < 0.5)
        })
    };
    let old = old.map(anchors).unwrap_or_default();
    let fit = if agree(&now) && (unchanged || agree(&old)) && old.len() == now.len() {
        if unchanged {
            return;
        }
        fit_affine(&old, &now)
    } else if agree(&old) {
        fit_affine(&old, &now)
    } else {
        // Out of step already (or no matching original): fit from the record's rectangle
        // corners, in any starting corner and direction.
        corners.and_then(|c| {
            (0..4)
                .flat_map(|k| [false, true].map(move |rev| (k, rev)))
                .find_map(|(k, rev)| {
                    let mut c: Vec<_> = (0..4).map(|i| c[(k + i) % 4]).collect();
                    if rev {
                        c[1..].reverse();
                    }
                    fit_affine(&c, &now)
                })
        })
    };
    match fit {
        Some(m) => transform_primitive(n, &m),
        None => n.children.retain(|c| !c.is("PRI")),
    }
}

/// The corners of a primitive's base rectangle (RCTv) through its matrix (MTXv), clockwise
/// from top left, as Fireworks orders a rectangle's points.
fn primitive_corners(pri: &Node) -> Option<Vec<(f64, f64)>> {
    let r = pri.child("RCT")?;
    let (l, t, rt, b) = (
        r.float_of("LFT")?,
        r.float_of("TOP")?,
        r.float_of("RIT")?,
        r.float_of("BOT")?,
    );
    let m = pri.child("MTX");
    let g = |tag: &str, def: f64| m.and_then(|m| m.float_of(tag)).unwrap_or(def);
    let (m00, m01, m02) = (g("M00", 1.0), g("M01", 0.0), g("M02", 0.0));
    let (m10, m11, m12) = (g("M10", 0.0), g("M11", 1.0), g("M12", 0.0));
    let (m20, m21, m22) = (g("M20", 0.0), g("M21", 0.0), g("M22", 1.0));
    [(l, t), (rt, t), (rt, b), (l, b)]
        .iter()
        .map(|&(x, y)| {
            let w = x * m02 + y * m12 + m22;
            (w.abs() > 1e-12).then(|| ((x * m00 + y * m10 + m20) / w, (x * m01 + y * m11 + m21) / w))
        })
        .collect()
}

/// A path list's anchor points (absolute).
fn anchors(pbl: &Node) -> Vec<(f64, f64)> {
    pbl.children_tagged("PBP")
        .flat_map(|c| c.children_tagged("PBT").collect::<Vec<_>>())
        .map(|p| (p.float_of("XLC").unwrap_or(0.0), p.float_of("YLC").unwrap_or(0.0)))
        .collect()
}

/// The affine transform `(a, b, c, d, e, f)` (x' = a x + c y + e, y' = b x + d y + f) taking
/// every point of `from` to the same point of `to`, if there is one.
fn fit_affine(from: &[(f64, f64)], to: &[(f64, f64)]) -> Option<[f64; 6]> {
    if from.len() != to.len() || from.len() < 3 {
        return None;
    }
    // The three points spanning the largest triangle.
    let area = |i: usize, j: usize, k: usize| {
        let (p, q, r) = (from[i], from[j], from[k]);
        ((q.0 - p.0) * (r.1 - p.1) - (r.0 - p.0) * (q.1 - p.1)).abs()
    };
    let n = from.len().min(64);
    let mut best = (0.0, 0, 1, 2);
    for i in 0..n {
        for j in i + 1..n {
            for k in j + 1..n {
                let v = area(i, j, k);
                if v > best.0 {
                    best = (v, i, j, k);
                }
            }
        }
    }
    let (v, i, j, k) = best;
    if v < 1e-6 {
        return None;
    }
    // Solve [x y 1] * [a b; c d; e f] = [x' y'] for the three points.
    let (p, q, r) = (from[i], from[j], from[k]);
    let det = p.0 * (q.1 - r.1) - p.1 * (q.0 - r.0) + (q.0 * r.1 - r.0 * q.1);
    if det.abs() < 1e-12 {
        return None;
    }
    let solve = |u: f64, w: f64, z: f64| {
        let x = (u * (q.1 - r.1) - p.1 * (w - z) + (w * r.1 - z * q.1)) / det;
        let y = (p.0 * (w - z) - u * (q.0 - r.0) + (q.0 * z - r.0 * w)) / det;
        let t = (p.0 * (q.1 * z - r.1 * w) - p.1 * (q.0 * z - r.0 * w) + u * (q.0 * r.1 - r.0 * q.1)) / det;
        (x, y, t)
    };
    let (a, c, e) = solve(to[i].0, to[j].0, to[k].0);
    let (b, d, f) = solve(to[i].1, to[j].1, to[k].1);
    let m = [a, b, c, d, e, f];
    let ok = from.iter().zip(to).all(|(&(x, y), &(tx, ty))| {
        (a * x + c * y + e - tx).abs() < 0.01 && (b * x + d * y + f - ty).abs() < 0.01
    });
    ok.then_some(m)
}

/// Apply affine `m` to a path's shape primitive (PRIv): its matrix (row vectors, M20/M21
/// translation, as Fireworks stores it) and its points (transform origin TOX/TOY, fill
/// handles PSX..PFY, also kept on the path).
fn transform_primitive(n: &mut Node, m: &[f64; 6]) {
    let [a, b, c, d, e, f] = *m;
    let pt = |x: f64, y: f64| (a * x + c * y + e, b * x + d * y + f);
    let move_points = |node: &mut Node| {
        for (tx, ty) in [("TOX", "TOY"), ("PSX", "PSY"), ("PEX", "PEY"), ("PFX", "PFY")] {
            if let (Some(x), Some(y)) = (node.float_of(tx), node.float_of(ty)) {
                let (x, y) = pt(x, y);
                put(node, Node::float(tx, x));
                put(node, Node::float(ty, y));
            }
        }
    };
    if let Some(pri) = n.child_mut("PRI") {
        move_points(pri);
        if let Some(mtx) = pri.child_mut("MTX") {
            let g = |t: &str, def: f64| mtx.float_of(t).unwrap_or(def);
            let old = [
                [g("M00", 1.0), g("M01", 0.0), g("M02", 0.0)],
                [g("M10", 0.0), g("M11", 1.0), g("M12", 0.0)],
                [g("M20", 0.0), g("M21", 0.0), g("M22", 1.0)],
            ];
            let by = [[a, b, 0.0], [c, d, 0.0], [e, f, 1.0]];
            for (r, row) in old.iter().enumerate() {
                for k in 0..3 {
                    let v: f64 = (0..3).map(|t| row[t] * by[t][k]).sum();
                    put(mtx, Node::float(&format!("M{r}{k}"), v));
                }
            }
        }
    }
    move_points(n);
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
                points: vec![
                    BezierPoint::corner(10.0, 10.0),
                    BezierPoint::corner(60.0, 10.0),
                    BezierPoint::corner(35.0, 50.0),
                ],
            }],
            fill: Some(Fill {
                color: Rgba::new(0, 102, 153, 255),
                category: "fc_Solid".into(),
                anti_alias: true,
                gradient: None,
            }),
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
        let ElementKind::Bitmap(b) = &els[0].kind else {
            panic!("bitmap expected")
        };
        assert_eq!(
            b.image,
            match &doc.layers[0].cells[0][0].kind {
                ElementKind::Bitmap(o) => o.image.clone(),
                _ => unreachable!(),
            }
        );
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

/// Live Effects: `EFLv` rebuilt from the object's list (parameters as stored, so an unchanged
/// list comes out identical). New lists go where Fireworks puts them: after `PATv` (paths),
/// before `ELMv` (groups), else last.
fn write_effect_list(n: &mut Node, list: &[firecraft_doc::EffectEntry]) {
    let at = n.children.iter().position(|c| c.is("EFL"));
    match (list.is_empty(), at) {
        (true, Some(i)) => {
            // Keep an empty list as read; drop one whose effects were all removed.
            if n.children.get(i).is_some_and(|c| c.child("EPS").is_some()) {
                n.children.remove(i);
            }
        }
        (true, None) => {}
        (false, Some(i)) => {
            if let Some(slot) = n.children.get_mut(i) {
                let prev = slot.clone();
                *slot = firecraft_doc::effects::write_list(list, Some(&prev));
            }
        }
        (false, None) => {
            let efl = firecraft_doc::effects::write_list(list, None);
            if let Some(i) = n.children.iter().position(|c| c.is("PAT")) {
                n.children.insert(i + 1, efl);
            } else if let Some(i) = n.children.iter().position(|c| c.is("ELM")) {
                n.children.insert(i, efl);
            } else {
                n.children.push(efl);
            }
        }
    }
}

#[cfg(test)]
mod primitive_tests {
    use super::*;

    #[test]
    fn out_of_step_primitive_is_repaired() {
        // Points already moved by (316, 119), record still at the old place (an earlier save).
        let pt = |x: f64, y: f64| Node::vector("PBT", vec![Node::float("XLC", x), Node::float("YLC", y)]);
        let pbl = Node::vector(
            "PBL",
            vec![Node::vector(
                "PBP",
                vec![
                    pt(377.0, 175.0),
                    pt(568.0, 175.0),
                    pt(568.0, 305.0),
                    pt(377.0, 305.0),
                ],
            )],
        );
        let rct = Node::vector(
            "RCT",
            vec![
                Node::float("LFT", 61.0),
                Node::float("TOP", 56.0),
                Node::float("RIT", 252.0),
                Node::float("BOT", 186.0),
            ],
        );
        let mut n = Node::vector(
            "PTH",
            vec![Node::vector("PRI", vec![Node::vector("MTX", vec![]), rct])],
        );
        sync_primitive(&mut n, Some(&pbl), &pbl, true);
        let mtx = n.child("PRI").and_then(|p| p.child("MTX")).expect("matrix");
        assert_eq!(
            (mtx.float_of("M20"), mtx.float_of("M21")),
            (Some(316.0), Some(119.0))
        );
        // In step now: saving again changes nothing.
        let before = n.clone();
        sync_primitive(&mut n, Some(&pbl), &pbl, true);
        assert_eq!(n, before);
    }

    #[test]
    fn affine_fit_and_primitive_matrix() {
        let from = [(61.0, 56.0), (252.0, 56.0), (252.0, 186.0), (61.0, 186.0)];
        let moved: Vec<_> = from.iter().map(|&(x, y)| (x + 316.0, y + 119.0)).collect();
        let m = fit_affine(&from, &moved).expect("a translation");
        assert!(
            m.iter()
                .zip([1.0, 0.0, 0.0, 1.0, 316.0, 119.0])
                .all(|(a, b)| (a - b).abs() < 1e-9)
        );
        // One corner dragged: no affine transform does that.
        let mut bent = moved.clone();
        bent[2].0 += 40.0;
        assert!(fit_affine(&from, &bent).is_none());
        // The primitive's matrix (translation 144,268) and transform origin follow the move.
        let mut n = Node::vector(
            "PTH",
            vec![
                Node::float("TOX", 144.0),
                Node::float("TOY", 268.0),
                Node::vector(
                    "PRI",
                    vec![Node::vector(
                        "MTX",
                        vec![Node::float("M20", 144.0), Node::float("M21", 268.0)],
                    )],
                ),
            ],
        );
        transform_primitive(&mut n, &m);
        let mtx = n.child("PRI").and_then(|p| p.child("MTX")).expect("matrix");
        assert_eq!(
            (mtx.float_of("M20"), mtx.float_of("M21")),
            (Some(460.0), Some(387.0))
        );
        assert_eq!((mtx.float_of("M00"), mtx.float_of("M11")), (Some(1.0), Some(1.0)));
        assert_eq!((n.float_of("TOX"), n.float_of("TOY")), (Some(460.0), Some(387.0)));
    }
}
