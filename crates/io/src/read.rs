//! Fireworks tree -> Document.

use std::collections::HashMap;

use firecraft_doc::{
    MaskMode,
    Bitmap, BezierPoint, Contour, Document, Element, ElementKind, Fill, Frame, Layer, Path, Stroke, Symbol, Text,
};
use firecraft_fwpng::{FwPng, Node, Tile};
use firecraft_raster::gradient::{Gradient, GradientKind};
use firecraft_raster::transform::Affine;
use firecraft_raster::{ColorMath, Rgba, RgbaImage};

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
    /// Symbol id -> (name, content bounds in symbol space), for instances.
    symbols: HashMap<u32, (String, Option<(f64, f64, f64, f64)>)>,
}

pub fn document_from_fireworks(fw: &FwPng) -> Result<Document> {
    let root = fw.tree()?.ok_or_else(|| Error::Unsupported("no document tree".into()))?;
    let tiles = fw.tiles()?;
    // Symbol definitions (`MSLv` > `MSTv`) first: instances need their names and bounds. Symbols
    // have their own coordinate space (no canvas origin).
    let sym_ctx = Ctx { tiles: &tiles, origin: (0.0, 0.0), symbols: HashMap::new() };
    let symbols: Vec<Symbol> = root
        .child("MSL")
        .map(|msl| msl.children_tagged("MST").filter_map(|m| read_symbol(m, &sym_ctx)).collect())
        .unwrap_or_default();
    let symbol_info = symbols.iter().map(|s| (s.id, (s.name.clone(), symbol_bounds(s)))).collect();
    let ctx = Ctx {
        tiles: &tiles,
        origin: (root.float_of("XLC").unwrap_or(0.0), root.float_of("YLC").unwrap_or(0.0)),
        symbols: symbol_info,
    };

    let width = root.float_of("WID").unwrap_or(1.0).round().clamp(1.0, 65_535.0) as u32;
    let height = root.float_of("HIT").unwrap_or(1.0).round().clamp(1.0, 65_535.0) as u32;
    let bg = root.u32_of("BGC").map(argb).unwrap_or(Rgba::WHITE);
    let frame_count = root.int_of("FRC").unwrap_or(1).clamp(1, 10_000) as usize;

    let mut doc = Document::new(width, height, root.float_of("RES").unwrap_or(72.0), (bg.a > 0).then_some(bg));
    doc.resolution_cm = root.int_of("RSU") == Some(1);
    doc.color_math = if root.int_of("FCM") == Some(1) { ColorMath::Precise } else { ColorMath::Classic };
    // Frame delays (1/100 s) and export flags: EXPv > FNLv > FNMv{FNDi, VISb}.
    let fnm: Vec<&Node> = root.child("EXP").and_then(|e| e.child("FNL")).map(|l| l.children_tagged("FNM").collect()).unwrap_or_default();
    doc.frames = (0..frame_count)
        .map(|i| {
            let f = fnm.get(i);
            Frame {
                name: format!("Frame {}", i + 1),
                delay: f.and_then(|f| f.int_of("FND")).unwrap_or(7).clamp(0, 65_535) as u32,
                export: f.and_then(|f| f.bool_of("VIS")).unwrap_or(true),
            }
        })
        .collect();
    doc.layers = root
        .child("LYL")
        .map(|lyl| lyl.children_tagged("LAY").map(|l| read_layer(l, &ctx)).collect())
        .unwrap_or_default();
    if doc.layers.is_empty() {
        doc.layers.push(Layer::new("Layer 1"));
    }
    // The current layer isn't stored (MCLb is per-layer sharing): start on the top normal layer.
    doc.current_layer = doc.layers.iter().rposition(|l| !l.web).unwrap_or(0);
    doc.symbols = std::sync::Arc::new(symbols);
    doc.raw_secondary = fw.secondary_tree()?;
    doc.raw_tiles = std::sync::Arc::new(tiles.clone());
    doc.raw = Some(root);
    Ok(doc)
}

fn read_layer(n: &Node, ctx: &Ctx) -> Layer {
    let mut layer = Layer::new(n.str_of("LNM").unwrap_or("Layer"));
    layer.web = n.bool_of("URL").unwrap_or(false);
    // MCLb{1} = the layer has its own content per frame; MCLb{0} = shared across frames
    // (confirmed with Fireworks MX's layer.sharing on tubes.png; MSLb is something else).
    layer.shared = !n.bool_of("MCL").unwrap_or(true);
    layer.disclosed = n.bool_of("DIS").unwrap_or(true);
    // VIFv: one VISb per frame.
    let vis: Vec<bool> = n.child("VIF").map(|v| v.children_tagged("VIS").filter_map(Node::as_bool).collect()).unwrap_or_default();
    if vis.iter().all(|&v| v == vis.first().copied().unwrap_or(true)) {
        layer.visible = vis.first().copied().unwrap_or(true);
    } else {
        layer.frame_visible = vis;
    }
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
        "GRP" if depth < MAX_GROUP_DEPTH => read_group(n, ctx, depth),
        "ALS" => read_instance(n, ctx),
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
    el.effects = n.child("EFL").map(read_effects).unwrap_or_default();
    // Hidden objects (eye off in the Layers panel).
    el.visible = n.bool_of("VIS").unwrap_or(true);
    if let Some(b) = n.int_of("BLD") {
        el.blend = firecraft_raster::BlendMode::from_fw_code(b);
    }
    // OPAi: tenths of a percent (Fireworks saves 25/50/75% as 250/501/749); absent = 100%.
    if let Some(o) = n.int_of("OPA") {
        el.opacity = (o.clamp(0, 1000) as f32) / 10.0;
    }
    Some(el)
}

/// `MSTv`: name `UNMs`, id `MSNi`, frame count `FRCi`, its own `LYLv`.
fn read_symbol(m: &Node, ctx: &Ctx) -> Option<Symbol> {
    let id = m.u32_of("MSN")?;
    let layers = m.child("LYL").map(|l| l.children_tagged("LAY").map(|l| read_layer(l, ctx)).collect()).unwrap_or_default();
    Some(Symbol {
        id,
        name: m.str_of("UNM").unwrap_or("Symbol").to_owned(),
        layers,
        frames: m.int_of("FRC").unwrap_or(1).clamp(1, 10_000) as usize,
    })
}

/// Union of the symbol's frame-1 content bounds.
fn symbol_bounds(s: &Symbol) -> Option<(f64, f64, f64, f64)> {
    let bs: Vec<_> = s.layers.iter().filter(|l| !l.web).flat_map(|l| l.elements(0)).filter_map(|e| e.bounds()).collect();
    if bs.is_empty() {
        return None;
    }
    let x0 = bs.iter().map(|b| b.0).fold(f64::MAX, f64::min);
    let y0 = bs.iter().map(|b| b.1).fold(f64::MAX, f64::min);
    let x1 = bs.iter().map(|b| b.0 + b.2).fold(f64::MIN, f64::max);
    let y1 = bs.iter().map(|b| b.1 + b.3).fold(f64::MIN, f64::max);
    Some((x0, y0, x1 - x0, y1 - y0))
}

/// `ALSv`: symbol id `MSNi` and placement `MTXv` (symbol space -> document space; FireCraft keeps
/// the matrix in canvas space, i.e. minus the canvas origin).
fn read_instance(n: &Node, ctx: &Ctx) -> ElementKind {
    let symbol = n.u32_of("MSN").unwrap_or(0);
    let m = n.child("MTX");
    let g = |t: &str, d: f64| m.and_then(|m| m.float_of(t)).unwrap_or(d);
    let matrix = Affine { a: g("M00", 1.0), b: g("M01", 0.0), c: g("M10", 0.0), d: g("M11", 1.0), e: g("M20", 0.0) - ctx.origin.0, f: g("M21", 0.0) - ctx.origin.1 };
    let (name, sb) = ctx.symbols.get(&symbol).cloned().unwrap_or_else(|| ("Symbol".into(), None));
    let bounds = sb.map(|b| matrix.map_rect(b.0, b.1, b.2, b.3));
    ElementKind::Instance { symbol, name, matrix, bounds }
}

/// Live Effects: `EFLv` holds one `EPSv` per effect, each a list of `DCEv` key/value pairs.
/// Effects that are switched off (`EffectIsVisible` false) or not rendered yet are skipped here
/// (they stay in the object's raw node, so saving keeps them).
fn read_effects(efl: &Node) -> Vec<firecraft_raster::effects::Effect> {
    efl.children_tagged("EPS")
        .filter_map(|eps| {
            let mut params = std::collections::HashMap::new();
            let mut vectors = std::collections::HashMap::new();
            for dce in eps.children_tagged("DCE") {
                let Some(key) = dce.str_of("DCK") else { continue };
                if let Some(v) = dce.str_of("DCV") {
                    params.insert(key.to_owned(), v.to_owned());
                } else if let Some(list) = dce.children.iter().find(|c| c.kind == firecraft_fwpng::Kind::Vector) {
                    let pts = list
                        .children
                        .iter()
                        .map(|p| (p.float_of("XLC").unwrap_or(0.0) as f32, p.float_of("YLC").unwrap_or(0.0) as f32))
                        .collect();
                    vectors.insert(key.to_owned(), pts);
                }
            }
            if params.get("EffectIsVisible").is_some_and(|v| v == "false") {
                return None;
            }
            firecraft_raster::effects::Effect::parse(&params, &vectors)
        })
        .collect()
}

/// Groups: `GRTi` 0 = plain group, 1 = "mask to image" (mask in `MRMv`, an `IMGv` mask cache
/// first in `ELMv`), 2 = "mask to path" (the mask path is the top object in `ELMv`).
/// `MNAb{1}` = grayscale mask, `0` = alpha channel (inferred; see docs/behavior/masks.md).
fn read_group(n: &Node, ctx: &Ctx, depth: usize) -> ElementKind {
    let elm = n.child("ELM");
    let mut kids: Vec<&Node> = elm.map(|e| e.children.iter().collect()).unwrap_or_default();
    let grayscale = n.bool_of("MNA").unwrap_or(true);
    match n.int_of("GRT").unwrap_or(0) {
        1 => {
            let Some(mask) = n.child("MRM").and_then(|m| m.children.first()).and_then(|m| read_element(m, ctx, depth + 1)) else {
                return plain_group(n, ctx, depth);
            };
            // The cache is an IMGv on top whose bounds match the mask's.
            let mut cache = None;
            if let (Some(first), Some(mb)) = (kids.first(), mask.bounds()) {
                if first.is("IMG") {
                    if let Some(b) = read_bitmap(first, ctx) {
                        let close = |a: f64, b: f64| (a - b).abs() <= 1.5;
                        if close(b.x as f64, mb.0) && close(b.y as f64, mb.1) && close(b.image.width() as f64, mb.2) && close(b.image.height() as f64, mb.3) {
                            let mut e = Element::new(ElementKind::Bitmap(b));
                            e.raw = Some((*first).clone());
                            cache = Some(Box::new(e));
                            kids.remove(0);
                        }
                    }
                }
            }
            let content = kids.iter().rev().filter_map(|c| read_element(c, ctx, depth + 1)).collect();
            let mode = if grayscale { MaskMode::Grayscale } else { MaskMode::Alpha };
            ElementKind::Masked { content, mask: Box::new(mask), mode, cache }
        }
        2 if kids.len() >= 2 => {
            let Some(mask) = read_element(kids.remove(0), ctx, depth + 1) else { return plain_group(n, ctx, depth) };
            let content = kids.iter().rev().filter_map(|c| read_element(c, ctx, depth + 1)).collect();
            ElementKind::Masked { content, mask: Box::new(mask), mode: MaskMode::PathOutline, cache: None }
        }
        _ => plain_group(n, ctx, depth),
    }
}

fn plain_group(n: &Node, ctx: &Ctx, depth: usize) -> ElementKind {
    ElementKind::Group(n.child("ELM").map(|e| read_elements(e, ctx, depth + 1)).unwrap_or_default())
}

/// Assemble an `IMGv` (or `MSKv`-shaped) element's tiles into an image.
fn read_bitmap(n: &Node, ctx: &Ctx) -> Option<Bitmap> {
    let w = n.int_of("WPX")?.clamp(0, 65_535) as u32;
    let h = n.int_of("HPX")?.clamp(0, 65_535) as u32;
    let tsz = n.int_of("TSZ").unwrap_or(IMAGE_TILE as i64).clamp(1, 4096) as u32;
    // The pixel buffer (and its tile grid) starts at XLCf/YLCf and is WPXi x HPXi. XOFf/YOFf crop
    // its top-left: Fireworks' image rectangle is (XLC + XOF, YLC + YOF) .. (XLC + W, YLC + H)
    // (from the bitmap constructor at 0x62acd0 in Fireworks.exe). Hidden pixels are dropped.
    let x = (n.float_of("XLC").unwrap_or(0.0) - ctx.origin.0).round() as i64;
    let y = (n.float_of("YLC").unwrap_or(0.0) - ctx.origin.1).round() as i64;
    let xof = n.float_of("XOF").unwrap_or(0.0).round().clamp(0.0, w as f64) as u32;
    let yof = n.float_of("YOF").unwrap_or(0.0).round().clamp(0.0, h as f64) as u32;
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
    if xof > 0 || yof > 0 {
        let mut visible = RgbaImage::new(w - xof, h - yof, Rgba::TRANSPARENT);
        visible.draw(&image, -(xof as i64), -(yof as i64), 255);
        return Some(Bitmap { x: x + xof as i64, y: y + yof as i64, image: visible });
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
        let category = preset.str_of("CAT").unwrap_or("fc_Solid").to_owned();
        Some(Fill {
            color: argb(a.u32_of("FCL").unwrap_or(0xFF00_0000)),
            anti_alias: preset.int_of("FET").unwrap_or(1) != 0,
            gradient: read_gradient(&category, preset, n, ctx),
            category,
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

/// Gradient stops (`FGLv`: colour ramp `FG0`/`FGV`, opacity ramp `FG1`) and handles
/// (`PSXf`.. on the path, document coordinates).
fn read_gradient(category: &str, preset: &Node, path: &Node, ctx: &Ctx) -> Option<Gradient> {
    let kind = GradientKind::from_category(category)?;
    let fgl = preset.child("FGL")?;
    let stops = |n: &Node| -> Vec<(f32, Rgba)> {
        let mut v: Vec<(f32, Rgba)> = n
            .children_tagged("FGI")
            .map(|i| (i.float_of("FGP").unwrap_or(0.0).clamp(0.0, 1.0) as f32, argb(i.u32_of("FGC").unwrap_or(0xFF00_0000))))
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        v
    };
    let fgy = fgl.child("FGY");
    let colors = fgy.and_then(|y| y.child("FG0")).or_else(|| fgl.child("FGV")).map(stops).unwrap_or_default();
    // The opacity ramp keeps opacity in each stop's alpha byte.
    let opacity = fgy.and_then(|y| y.child("FG1")).map(|n| stops(n).into_iter().map(|(p, c)| (p, c.a)).collect()).unwrap_or_default();
    let (ox, oy) = ctx.origin;
    let pt = |x: &str, y: &str| (path.float_of(x).unwrap_or(0.0) - ox, path.float_of(y).unwrap_or(0.0) - oy);
    Some(Gradient { kind, colors, opacity, start: pt("PSX", "PSY"), end: pt("PEX", "PEY"), side: pt("PFX", "PFY") })
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
