//! Fireworks tree -> Document.

use std::collections::HashMap;

use firecraft_doc::{
    Bitmap, BezierPoint, Contour, Document, Element, ElementKind, Fill, Frame, Layer, Path, Stroke, Text,
};
use firecraft_fwpng::{FwPng, Node, Tile};
use firecraft_raster::{Rgba, RgbaImage};

use crate::{Error, Result};

/// Image tile edge in pixels (`TSZi{80}`).
pub(crate) const IMAGE_TILE: u32 = 128;

/// Fireworks colours are ARGB integers.
pub(crate) fn argb(v: u32) -> Rgba {
    Rgba::new((v >> 16) as u8, (v >> 8) as u8, v as u8, (v >> 24) as u8)
}

struct Ctx<'a> {
    tiles: &'a HashMap<u32, Tile>,
    /// Canvas origin in document space (root `XLCf`/`YLCf`).
    origin: (f64, f64),
}

pub fn document_from_fireworks(fw: &FwPng) -> Result<Document> {
    let root = fw.tree()?.ok_or_else(|| Error::Unsupported("no document tree".into()))?;
    let tiles = fw.tiles()?;
    let ctx = Ctx { tiles: &tiles, origin: (root.float_of("XLC").unwrap_or(0.0), root.float_of("YLC").unwrap_or(0.0)) };

    let width = root.float_of("WID").unwrap_or(1.0).round().clamp(1.0, 65_535.0) as u32;
    let height = root.float_of("HIT").unwrap_or(1.0).round().clamp(1.0, 65_535.0) as u32;
    let bg = root.u32_of("BGC").map(argb).unwrap_or(Rgba::WHITE);
    let frame_count = root.int_of("FRC").unwrap_or(1).clamp(1, 10_000) as usize;

    let mut doc = Document::new(width, height, root.float_of("RES").unwrap_or(72.0), (bg.a > 0).then_some(bg));
    doc.resolution_cm = root.int_of("RSU") == Some(1);
    doc.frames = (1..=frame_count).map(|i| Frame { name: format!("Frame {i}"), ..Frame::default() }).collect();
    doc.layers = root
        .child("LYL")
        .map(|lyl| lyl.children_tagged("LAY").map(|l| read_layer(l, &ctx)).collect())
        .unwrap_or_default();
    if doc.layers.is_empty() {
        doc.layers.push(Layer::new("Layer 1"));
    }
    doc.current_layer = root
        .child("LYL")
        .and_then(|lyl| lyl.children_tagged("LAY").position(|l| l.bool_of("MCL") == Some(true)))
        .or_else(|| doc.layers.iter().position(|l| !l.web))
        .unwrap_or(0);
    doc.raw_secondary = fw.secondary_tree()?;
    doc.raw_tiles = std::sync::Arc::new(tiles.clone());
    doc.raw = Some(root);
    Ok(doc)
}

fn read_layer(n: &Node, ctx: &Ctx) -> Layer {
    let mut layer = Layer::new(n.str_of("LNM").unwrap_or("Layer"));
    layer.web = n.bool_of("URL").unwrap_or(false);
    layer.shared = n.bool_of("MSL").unwrap_or(false);
    layer.disclosed = n.bool_of("DIS").unwrap_or(true);
    layer.visible = n.child("VIF").and_then(|v| v.bool_of("VIS")).unwrap_or(true);
    layer.cells = n
        .child("CLL")
        .map(|cll| {
            cll.children_tagged("CEL")
                .map(|cel| {
                    layer.locked |= cel.bool_of("LCK").unwrap_or(false);
                    cel.child("ELM").map(|elm| read_elements(elm, ctx, 0)).unwrap_or_default()
                })
                .collect()
        })
        .unwrap_or_default();
    if layer.cells.is_empty() {
        layer.cells.push(Vec::new());
    }
    layer.raw = Some(n.clone());
    layer
}

const MAX_GROUP_DEPTH: usize = 64;

/// `ELMv` lists objects top first (as the Layers panel shows them); the model keeps them
/// bottom first (drawing order), so the list is reversed here and in the writer.
fn read_elements(elm: &Node, ctx: &Ctx, depth: usize) -> Vec<Element> {
    elm.children.iter().rev().filter_map(|c| read_element(c, ctx, depth)).collect()
}

fn read_element(n: &Node, ctx: &Ctx, depth: usize) -> Option<Element> {
    let kind = match n.tag_str() {
        "IMG" => ElementKind::Bitmap(read_bitmap(n, ctx)?),
        "PTH" => ElementKind::Path(read_path(n, ctx)),
        "TXT" => ElementKind::Text(read_text(n, ctx)),
        "GRP" if depth < MAX_GROUP_DEPTH => {
            ElementKind::Group(n.child("ELM").map(|e| read_elements(e, ctx, depth + 1)).unwrap_or_default())
        }
        other => ElementKind::Other {
            kind: match other {
                "URL" => "Web Object".into(),
                "ALS" => "Symbol".into(),
                t => t.to_owned(),
            },
            cached: n.child("IMG").and_then(|i| read_bitmap(i, ctx)),
        },
    };
    let mut el = Element::new(kind);
    el.raw = Some(n.clone());
    if let Some(b) = n.int_of("BLD") {
        el.blend = firecraft_raster::BlendMode::from_fw_code(b);
    }
    if let Some(o) = n.int_of("OPA") {
        el.opacity = o.clamp(0, 100) as u8;
    }
    Some(el)
}

/// Assemble an `IMGv` (or `MSKv`-shaped) element's tiles into an image.
fn read_bitmap(n: &Node, ctx: &Ctx) -> Option<Bitmap> {
    let w = n.int_of("WPX")?.clamp(0, 65_535) as u32;
    let h = n.int_of("HPX")?.clamp(0, 65_535) as u32;
    let tsz = n.int_of("TSZ").unwrap_or(IMAGE_TILE as i64).clamp(1, 4096) as u32;
    let x = (n.float_of("XLC").unwrap_or(0.0) + n.float_of("XOF").unwrap_or(0.0) - ctx.origin.0).round() as i64;
    let y = (n.float_of("YLC").unwrap_or(0.0) + n.float_of("YOF").unwrap_or(0.0) - ctx.origin.1).round() as i64;
    let mut image = RgbaImage::new(w, h, Rgba::TRANSPARENT);
    let cols = w.div_ceil(tsz).max(1);
    let ids: Vec<u32> = n.child("TIL").map(|t| t.children.iter().filter_map(Node::as_u32).collect()).unwrap_or_default();
    for (i, id) in ids.iter().enumerate() {
        let Some(tile) = ctx.tiles.get(id) else { continue };
        let (tx, ty) = ((i as u32 % cols) * tsz, (i as u32 / cols) * tsz);
        for (k, px) in tile.data.chunks_exact(4).enumerate() {
            let (px_x, px_y) = (tx + k as u32 % tsz, ty + k as u32 / tsz);
            if px_x < w && px_y < h {
                image.set(px_x as i64, px_y as i64, Rgba::new(px[1], px[2], px[3], px[0]));
            }
        }
    }
    Some(Bitmap { x, y, image })
}

fn read_path(n: &Node, ctx: &Ctx) -> Path {
    let (ox, oy) = ctx.origin;
    let contours = n
        .child("PBL")
        .map(|pbl| {
            pbl.children_tagged("PBP")
                .map(|pbp| Contour {
                    closed: pbp.bool_of("ISC").unwrap_or(false),
                    points: pbp
                        .children_tagged("PBT")
                        .map(|p| {
                            // Control points are absolute; a missing one coincides with the anchor.
                            let (ax, ay) = (p.float_of("XLC").unwrap_or(0.0), p.float_of("YLC").unwrap_or(0.0));
                            let rel = |tx: &str, ty: &str| {
                                (p.float_of(tx).map(|v| v - ax).unwrap_or(0.0), p.float_of(ty).map(|v| v - ay).unwrap_or(0.0))
                            };
                            BezierPoint { x: ax - ox, y: ay - oy, pred: rel("XPC", "YPC"), succ: rel("XSC", "YSC") }
                        })
                        .collect(),
                })
                .collect()
        })
        .unwrap_or_default();
    let attrs = n.child("PAT");
    let fill = attrs.and_then(|a| {
        let preset = a.child("FPL")?;
        Some(Fill {
            color: argb(a.u32_of("FCL").unwrap_or(0xFF00_0000)),
            category: preset.str_of("CAT").unwrap_or("fc_Solid").to_owned(),
            anti_alias: preset.int_of("FET").unwrap_or(1) != 0,
        })
    });
    let stroke = attrs.and_then(|a| {
        let preset = a.child("BPL")?;
        Some(Stroke {
            color: argb(a.u32_of("BCL").unwrap_or(0xFF00_0000)),
            category: preset.str_of("CAT").unwrap_or("bc_Pencil").to_owned(),
            name: preset.str_of("INM").unwrap_or_default().to_owned(),
            diameter: preset.int_of("BDI").unwrap_or(1).clamp(1, 1000) as f64,
            softness: preset.int_of("BSE").unwrap_or(0).clamp(0, 100) as u8,
        })
    });
    Path { contours, fill, stroke }
}

fn read_text(n: &Node, ctx: &Ctx) -> Text {
    let text: String = n
        .child("TFS")
        .map(|t| t.children.iter().filter_map(Node::as_str).collect::<Vec<_>>().join(""))
        .unwrap_or_default();
    Text {
        text,
        font: n.str_of("FON").unwrap_or("Arial").to_owned(),
        size: n.float_of("PTS").unwrap_or(12.0),
        color: n.child("PAT").and_then(|a| a.u32_of("FCL")).map(argb).unwrap_or(Rgba::BLACK),
        bold: n.bool_of("BOL").unwrap_or(false),
        italic: n.bool_of("ITL").unwrap_or(false),
        underline: n.bool_of("UND").unwrap_or(false),
        cached: n.child("IMG").and_then(|i| read_bitmap(i, ctx)),
    }
}
