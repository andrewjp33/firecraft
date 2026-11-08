//! Document -> Fireworks tree + tiles.
//!
//! Where a document or element was read from a Fireworks PNG, its original node is the template:
//! FireCraft rewrites the fields it models and leaves everything else (effects, textures,
//! presets, export settings) as Fireworks wrote it. New documents get the minimal tree shape
//! observed in Fireworks MX output (see docs/fireworks-png.md).

use firecraft_doc::{
    Bitmap, Contour, Document, Element, ElementKind, Layer, MaskMode, Path, RectShape, SymbolKind,
};
use firecraft_fwpng::{Node, Tile};
use firecraft_raster::transform::Affine;
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
    /// Mask tiles already written per texture (shared by every path using it, as in Fireworks).
    texture_tiles: Vec<(std::sync::Arc<Vec<u8>>, u32, Vec<u32>)>,
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
        texture_tiles: Vec::new(),
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
        // Fireworks keeps opacity as a byte and saves floor(byte * 1000 / 255) (50% -> 501).
        let byte = (el.opacity.clamp(0.0, 100.0) as f64 * 255.0 / 100.0).round() as i64;
        put(&mut n, Node::int("OPA", byte * 1000 / 255));
    }
    mx_order(&mut n);
    n
}

/// Put a path's or bitmap's children in Fireworks MX's order (verified with
/// tools/fwmx-oracle: MX reads a path's opacity, blend mode and effects only in its header,
/// and ignores them after the points):
/// path `VISb LCKb PATv BRPi PTTi FOTb EFLv RNDi TOXf TOYf BLDi OPAi PCFb PSX..PFY PBLv PCLv
/// PRIv ... OBNs`, bitmap `VISb LCKb XOF YOF XLC YLC WPX HPX TSZ TILv ... BLDi OPAi EFLv OBNs`.
/// Tags not listed keep their place relative to their neighbours.
fn mx_order(n: &mut Node) {
    const PATH: [&str; 22] = [
        "VIS", "LCK", "PAT", "BRP", "PTT", "FOT", "EFL", "RND", "TOX", "TOY", "BLD", "OPA", "PCF", "PSX",
        "PSY", "PEX", "PEY", "PFX", "PFY", "PBL", "PCL", "PRI",
    ];
    const IMAGE: [&str; 10] = [
        "VIS", "LCK", "XOF", "YOF", "XLC", "YLC", "WPX", "HPX", "TSZ", "TIL",
    ];
    const IMAGE_END: [&str; 4] = ["BLD", "OPA", "EFL", "OBN"];
    let is_path = n.is("PTH");
    let rank = |c: &Node| -> usize {
        if is_path {
            match PATH.iter().position(|t| c.is(t)) {
                Some(i) => i * 2,
                None if c.is("OBN") => 1000,
                None => PATH.len() * 2 + 1, // after PRIv (DTAv, POPi, EOFb, AGMb...)
            }
        } else {
            match (
                IMAGE.iter().position(|t| c.is(t)),
                IMAGE_END.iter().position(|t| c.is(t)),
            ) {
                (Some(i), _) => i * 2,
                (_, Some(i)) => 100 + i,
                _ => IMAGE.len() * 2 + 1,
            }
        }
    };
    if is_path || n.is("IMG") {
        let mut kids = std::mem::take(&mut n.children);
        kids.sort_by_key(|c| rank(c));
        n.children = kids;
    }
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

/// A texture node (`TXBv`/`TXFv { INMs MSKv { WPXi HPXi TSZi{100} TILv } }`): mask tiles of
/// 256 x 256 coverage bytes, written once per texture and shared by every path using it.
fn texture_node(tag: &str, t: &firecraft_doc::Texture, w: &mut Writer) -> Node {
    const TS: u32 = 256;
    let known = w
        .texture_tiles
        .iter()
        .find(|(m, width, _)| (std::sync::Arc::ptr_eq(m, &t.mask) || **m == *t.mask) && *width == t.width)
        .map(|e| e.2.clone());
    let ids = known.unwrap_or_else(|| {
        let mut ids = Vec::new();
        for ty in (0..t.height).step_by(TS as usize) {
            for tx in (0..t.width).step_by(TS as usize) {
                let mut data = vec![0u8; (TS * TS) as usize];
                for y in 0..TS.min(t.height - ty) {
                    for x in 0..TS.min(t.width - tx) {
                        if let (Some(d), Some(v)) = (
                            data.get_mut((y * TS + x) as usize),
                            t.mask.get(((ty + y) * t.width + tx + x) as usize),
                        ) {
                            *d = *v;
                        }
                    }
                }
                let id = w.tile_id();
                // Fireworks marks mask tiles with flags 1 and refuses a file whose texture
                // tiles lack it (verified with tools/fwmx-oracle).
                let mut tile = Tile::new(id, data);
                tile.flags = 1;
                w.tiles.push(tile);
                ids.push(id);
            }
        }
        w.texture_tiles.push((t.mask.clone(), t.width, ids.clone()));
        ids
    });
    Node::vector(
        tag,
        vec![
            Node::string("INM", &t.name),
            Node::vector(
                "MSK",
                vec![
                    Node::int("WPX", t.width as i64),
                    Node::int("HPX", t.height as i64),
                    Node::int("TSZ", TS as i64),
                    Node::vector(
                        "TIL",
                        ids.into_iter().map(|id| Node::int("TID", id as i64)).collect(),
                    ),
                ],
            ),
        ],
    )
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
    if !unchanged {
        n.children.retain(|c| !c.is("PCL"));
        put(&mut n, pbl);
    }
    // A rectangle primitive (PRIv): Fireworks rebuilds the shape from this record, not from
    // the points, so it is written from the model; a plain path has none.
    match &p.rect_in_step() {
        Some(r) => write_rect(&mut n, r, w.origin),
        None => n.children.retain(|c| !c.is("PRI")),
    }
    // Texture origin (`TOXf`/`TOYf`, after `RNDi` as Fireworks writes it).
    let (tx, ty) = firecraft_doc::stroke_fw::texture_origin(p);
    for (tag, v) in [("TOX", tx + w.origin.0), ("TOY", ty + w.origin.1)] {
        match n.children.iter().position(|c| c.is(tag)) {
            Some(i) => n.children[i] = Node::float(tag, v),
            None => {
                let at = n
                    .children
                    .iter()
                    .position(|c| c.is("RND") || c.is("TOX"))
                    .map_or(n.children.len(), |i| i + 1);
                n.children.insert(at, Node::float(tag, v));
            }
        }
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
                if let Some(fpl) = attrs.child_mut("FPL") {
                    write_fill_kind(fpl, f);
                    put(fpl, Node::int("FTB", f.texture_amount.min(1000) as i64));
                    put(fpl, Node::int("FSM", if f.texture_transparent { 1 } else { 2 }));
                }
                if let Some(t) = &f.texture {
                    let txf = texture_node("TXF", t, w);
                    match attrs.children.iter().position(|c| c.is("TXF")) {
                        Some(i) => attrs.children[i] = txf,
                        None => {
                            // After TXBv (Fireworks' order: BPL, FPL, TXB, TXF).
                            let at = attrs
                                .children
                                .iter()
                                .rposition(|c| c.is("TXB") || c.is("BPL") || c.is("FPL"))
                                .map_or(attrs.children.len(), |i| i + 1);
                            attrs.children.insert(at, txf);
                        }
                    }
                }
            }
            None => attrs.children.retain(|c| !c.is("FPL")),
        }
        match &p.stroke {
            Some(s) => {
                put(attrs, Node::int("BCL", to_argb(s.color)));
                // The file's brush preset, in step with FireCraft's stroke settings; new
                // strokes get FireCraft's pencil.
                let mut bpl = s
                    .brush
                    .clone()
                    .or_else(|| attrs.child("BPL").cloned())
                    .unwrap_or_else(|| firecraft_doc::stroke_fw::pencil_preset(s));
                firecraft_doc::stroke_fw::sync_preset(&mut bpl, s);
                put(attrs, bpl);
                if let Some(t) = &s.texture {
                    let txb = texture_node("TXB", t, w);
                    match attrs.children.iter().position(|c| c.is("TXB")) {
                        Some(i) => attrs.children[i] = txb,
                        None => {
                            let at = attrs
                                .children
                                .iter()
                                .rposition(|c| c.is("BPL") || c.is("FPL"))
                                .map_or(attrs.children.len(), |i| i + 1);
                            attrs.children.insert(at, txb);
                        }
                    }
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
        // Fireworks' order (others after, as they were).
        const ORDER: [&str; 6] = ["BCL", "FCL", "BPL", "FPL", "TXB", "TXF"];
        attrs
            .children
            .sort_by_key(|c| ORDER.iter().position(|t| c.is(t)).unwrap_or(ORDER.len()));
    }
    // Stroke placement (`BRPi`: 0 inside, 1 centred, 2 outside).
    if let Some(st) = &p.stroke {
        put(&mut n, Node::int("BRP", st.placement.min(2) as i64));
    }
    // Gradient handles follow the object when it is moved or scaled.
    if let Some((s, e, f)) = gradient_handles {
        let (ox, oy) = w.origin;
        let handles = [
            ("PSX", s.0 + ox),
            ("PSY", s.1 + oy),
            ("PEX", e.0 + ox),
            ("PEY", e.1 + oy),
            ("PFX", f.0 + ox),
            ("PFY", f.1 + oy),
        ];
        for (tag, v) in handles {
            put(&mut n, Node::float(tag, v));
        }
        // Fireworks MX ignores the handles without this flag.
        put(&mut n, Node::bool("PCF", true));
        // A rectangle repeats them in its primitive record, which is the one MX uses.
        if let Some(pri) = n.child_mut("PRI") {
            for (tag, v) in handles {
                put(pri, Node::float(tag, v));
            }
            put(pri, Node::bool("PCF", true));
        }
    }
    n
}

/// Write a rectangle primitive's record (`PRIv`, after the points, as Fireworks MX does) and
/// mark the path as one (`PTTi` 1). The record's points (transform origin `TOXf`/`TOYf`, fill
/// handles `PSXf`..`PFYf`, also kept on the path) follow the change of matrix; a new record
/// puts them at the matrix origin, as Fireworks does.
fn write_rect(n: &mut Node, r: &RectShape, origin: (f64, f64)) {
    let m = Affine::translate(origin.0, origin.1).then(&r.matrix);
    let old = n.child("PRI").map(|p| crate::read::matrix_of(p.child("MTX")));
    let delta = old.and_then(|o| o.invert()).map(|inv| m.then(&inv));
    let (zx, zy) = m.apply(0.0, 0.0);
    let move_points = |node: &mut Node| {
        for (tx, ty) in [("TOX", "TOY"), ("PSX", "PSY"), ("PEX", "PEY"), ("PFX", "PFY")] {
            let (x, y) = match (&delta, node.float_of(tx), node.float_of(ty)) {
                (Some(d), Some(x), Some(y)) => d.apply(x, y),
                (None, Some(_), Some(_)) => (zx, zy),
                _ => continue,
            };
            put(node, Node::float(tx, x));
            put(node, Node::float(ty, y));
        }
    };
    let mut pri = n.child("PRI").cloned().unwrap_or_else(|| {
        Node::vector(
            "PRI",
            vec![
                Node::bool("PCF", true),
                Node::float("PSX", 0.0),
                Node::float("PSY", 0.0),
                Node::float("PEX", 0.0),
                Node::float("PEY", 0.0),
                Node::float("PFX", 0.0),
                Node::float("PFY", 0.0),
            ],
        )
    });
    move_points(&mut pri);
    put(
        &mut pri,
        Node::vector(
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
        ),
    );
    put(
        &mut pri,
        Node::vector(
            "RCT",
            vec![
                Node::float("RDS", r.roundness),
                Node::float("LFT", r.left),
                Node::float("TOP", r.top),
                Node::float("RIT", r.right),
                Node::float("BOT", r.bottom),
            ],
        ),
    );
    if n.child("PRI").is_none() {
        // The path's own copy of the points, after RNDi (Fireworks' order).
        let at = n
            .children
            .iter()
            .position(|c| c.is("RND"))
            .map_or(n.children.len(), |i| i + 1);
        let fresh = [
            Node::float("TOX", zx),
            Node::float("TOY", zy),
            Node::bool("PCF", true),
            Node::float("PSX", zx),
            Node::float("PSY", zy),
            Node::float("PEX", zx),
            Node::float("PEY", zy),
            Node::float("PFX", zx),
            Node::float("PFY", zy),
        ];
        let fresh: Vec<Node> = fresh
            .into_iter()
            .filter(|f| n.children.iter().all(|c| c.tag != f.tag))
            .collect();
        for (k, node) in fresh.into_iter().enumerate() {
            n.children.insert(at + k, node);
        }
        n.children.push(pri);
    } else {
        move_points(n);
        put(n, pri);
    }
    put(n, Node::int("PTT", 1));
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
/// A gradient type's Fireworks number (`FSHi`), solid fills 0.
fn fill_shape(category: &str) -> i64 {
    match category {
        "fc_Linear" => 2,
        "fc_Circular" => 3,
        "fc_Conical" => 4,
        "fc_Satin" => 5,
        "fc_Pinch" => 7,
        "fc_Folds" => 8,
        "fc_Elliptical" => 9,
        "fc_Rectangular" => 10,
        "fc_Bars" => 12,
        "fc_Ripple" => 13,
        "fc_Waves" => 14,
        _ => 0,
    }
}

/// Write what kind of fill this is into its preset (`FPLv`): category, edge, gradient ramp.
fn write_fill_kind(fpl: &mut Node, f: &firecraft_doc::Fill) {
    let changed = fpl.str_of("CAT") != Some(f.category.as_str());
    put(fpl, Node::string("CAT", &f.category));
    if changed {
        // Fireworks' names when the category is changed.
        let name = if f.gradient.is_some() {
            "Solid"
        } else {
            "fn_Normal"
        };
        put(fpl, Node::string("INM", name));
        put(fpl, Node::string("UNM", name));
    }
    put(fpl, Node::int("FEF", f.feather as i64));
    put(fpl, Node::int("FET", i64::from(f.anti_alias || f.feather > 0)));
    match &f.gradient {
        Some(g) => {
            put(fpl, Node::int("FSH", fill_shape(&f.category)));
            let ramp = |tag: &str, stops: &[(f32, i64)]| {
                let mut kids = vec![Node::int("FNC", stops.len() as i64)];
                kids.extend(stops.iter().map(|(p, c)| {
                    Node::vector("FGI", vec![Node::float("FGP", *p as f64), Node::int("FGC", *c)])
                }));
                Node::vector(tag, kids)
            };
            let colors: Vec<(f32, i64)> = g
                .colors
                .iter()
                .map(|(p, c)| (*p, to_argb(Rgba { a: 255, ..*c })))
                .collect();
            let default_opacity = [(0.0, 255), (1.0, 255)];
            let stops: &[(f32, u8)] = if g.opacity.is_empty() {
                &default_opacity
            } else {
                &g.opacity
            };
            // The opacity ramp keeps opacity in each stop's alpha byte.
            let opacity: Vec<(f32, i64)> = stops
                .iter()
                .map(|(p, a)| (*p, to_argb(Rgba::new(0, 0, 0, *a))))
                .collect();
            let name = fpl
                .child("FGL")
                .and_then(|l| l.str_of("INM"))
                .unwrap_or("cn_BlackWhite")
                .to_owned();
            put(
                fpl,
                Node::vector(
                    "FGL",
                    vec![
                        Node::string("CAT", "cc_GradientColors"),
                        Node::string("INM", &name),
                        ramp("FGV", &colors),
                        Node::vector("FGY", vec![ramp("FG0", &colors), ramp("FG1", &opacity)]),
                    ],
                ),
            );
        }
        None => {
            put(fpl, Node::int("FSH", 0));
            fpl.children.retain(|c| !c.is("FGL"));
        }
    }
}

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
                feather: 0,
                gradient: None,
                texture: None,
                texture_amount: 0,
                texture_transparent: false,
            }),
            stroke: None,
            rect: None,
            pcl: None,
            texture_origin: None,
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
    use firecraft_doc::{Document, Element, ElementKind, Path, RectShape};
    use firecraft_fwpng::Node;
    use firecraft_raster::Rgba;
    use firecraft_raster::transform::Affine;

    fn rect_path(roundness: f64) -> Element {
        let r = RectShape::new(10.0, 20.0, 110.0, 80.0, roundness);
        Element::new(ElementKind::Path(Path {
            contours: vec![r.contour()],
            rect: Some(r),
            ..Path::default()
        }))
    }

    fn round_trip(doc: &Document) -> Document {
        crate::open(&crate::save_fireworks_png(doc).expect("saved"), None).expect("opened")
    }

    fn first(doc: &Document) -> &Element {
        doc.layers
            .iter()
            .flat_map(|l| l.elements(0))
            .next()
            .expect("an object")
    }

    fn pri(doc: &Document) -> Node {
        let (tree, _) = crate::fireworks_tree(doc);
        fn find(n: &Node) -> Option<Node> {
            if n.is("PRI") {
                return Some(n.clone());
            }
            n.children.iter().find_map(find)
        }
        find(&tree).expect("a PRIv")
    }

    #[test]
    fn rectangles_round_trip_as_primitives() {
        let mut doc = Document::new(200, 200, 72.0, Some(Rgba::WHITE));
        if let Some(cell) = doc.layers[0].elements_mut(0) {
            cell.push(rect_path(0.3));
        }
        let back = round_trip(&doc);
        let ElementKind::Path(p) = &first(&back).kind else {
            panic!("a path")
        };
        let r = p.rect.expect("still a rectangle");
        assert!((r.roundness - 0.3).abs() < 1e-9 && p.contours[0].points.len() == 8);
        // Moved and rotated: the record's matrix follows, the points still match it.
        let mut moved = back.clone();
        if let Some(e) = moved
            .layers
            .iter_mut()
            .find_map(|l| l.elements_mut(0).filter(|c| !c.is_empty()))
            .and_then(|c| c.first_mut())
        {
            e.translate(316.0, 119.0);
            e.transform(&Affine {
                a: 0.0,
                b: 1.0,
                c: -1.0,
                d: 0.0,
                ..Affine::IDENTITY
            });
        }
        let again = round_trip(&moved);
        let pts = |e: &Element| match &e.kind {
            ElementKind::Path(p) => (
                p.rect,
                p.contours[0]
                    .points
                    .iter()
                    .map(|q| (q.x, q.y))
                    .collect::<Vec<_>>(),
            ),
            _ => (None, vec![]),
        };
        let ((ra, pa), (rb, pb)) = (pts(first(&again)), pts(first(&moved)));
        assert_eq!(ra, rb);
        assert!(
            pa.iter()
                .zip(&pb)
                .all(|(a, b)| (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6)
        );
        assert!(pri(&moved).child("TOX").is_none());
        let mtx = pri(&moved).child("MTX").cloned().expect("matrix");
        assert_eq!(
            (mtx.float_of("M01"), mtx.float_of("M10")),
            (Some(1.0), Some(-1.0))
        );
    }

    #[test]
    fn out_of_step_record_is_repaired_on_open() {
        // Points moved without the record (as saved by an earlier FireCraft).
        let mut doc = Document::new(800, 600, 72.0, Some(Rgba::WHITE));
        let mut e = rect_path(0.0);
        if let ElementKind::Path(p) = &mut e.kind {
            for pt in p.contours.iter_mut().flat_map(|c| c.points.iter_mut()) {
                pt.x += 316.0;
                pt.y += 119.0;
            }
        }
        if let Some(cell) = doc.layers[0].elements_mut(0) {
            cell.push(e);
        }
        let back = round_trip(&doc);
        let ElementKind::Path(p) = &first(&back).kind else {
            panic!("a path")
        };
        let r = p.rect.expect("repaired, still a rectangle");
        assert_eq!(r.matrix.apply(10.0, 20.0), (326.0, 139.0));
        let mtx = pri(&back).child("MTX").cloned().expect("matrix");
        assert_eq!(
            (mtx.float_of("M20"), mtx.float_of("M21")),
            (Some(316.0), Some(119.0))
        );
    }

    #[test]
    fn edited_points_make_a_plain_path() {
        let mut doc = Document::new(200, 200, 72.0, Some(Rgba::WHITE));
        let mut e = rect_path(0.0);
        if let ElementKind::Path(p) = &mut e.kind {
            p.contours[0].points[2].x += 30.0;
            p.rect = None;
        }
        if let Some(cell) = doc.layers[0].elements_mut(0) {
            cell.push(e);
        }
        let back = round_trip(&doc);
        let ElementKind::Path(p) = &first(&back).kind else {
            panic!("a path")
        };
        assert!(p.rect.is_none());
    }
}
