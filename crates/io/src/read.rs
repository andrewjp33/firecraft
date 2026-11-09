//! Fireworks tree -> Document.

use std::collections::HashMap;

use firecraft_doc::{
    BezierPoint, Bitmap, Contour, Document, Element, ElementKind, Fill, Frame, Layer, MaskMode, Path,
    PolyLine, PolyPoint, RectShape, StoredPolyline, Stroke, Symbol, Text, TextRun,
};
use firecraft_fwpng::{FwPng, Node, Tile};
use firecraft_raster::gradient::{Gradient, GradientKind};
use firecraft_raster::pattern::PatternFill;
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
    let root = fw
        .tree()?
        .ok_or_else(|| Error::Unsupported("no document tree".into()))?;
    let tiles = fw.tiles()?;
    // Symbol definitions (`MSLv` > `MSTv`) first: instances need their names and bounds. Symbols
    // have their own coordinate space (no canvas origin).
    let sym_ctx = Ctx {
        tiles: &tiles,
        origin: (0.0, 0.0),
        symbols: HashMap::new(),
    };
    let mut symbols: Vec<Symbol> = root
        .child("MSL")
        .map(|msl| {
            msl.children_tagged("MST")
                .filter_map(|m| read_symbol(m, &sym_ctx))
                .collect()
        })
        .unwrap_or_default();
    // Instances inside symbols were read before the symbols' bounds were known.
    firecraft_doc::refresh_nested_instance_bounds(&mut symbols);
    let symbol_info = symbols
        .iter()
        .map(|s| (s.id, (s.name.clone(), symbol_bounds(s))))
        .collect();
    let ctx = Ctx {
        tiles: &tiles,
        origin: (
            root.float_of("XLC").unwrap_or(0.0),
            root.float_of("YLC").unwrap_or(0.0),
        ),
        symbols: symbol_info,
    };

    let width = root.float_of("WID").unwrap_or(1.0).round().clamp(1.0, 65_535.0) as u32;
    let height = root.float_of("HIT").unwrap_or(1.0).round().clamp(1.0, 65_535.0) as u32;
    let bg = root.u32_of("BGC").map(argb).unwrap_or(Rgba::WHITE);
    let frame_count = root.int_of("FRC").unwrap_or(1).clamp(1, 10_000) as usize;

    let mut doc = Document::new(
        width,
        height,
        root.float_of("RES").unwrap_or(72.0),
        (bg.a > 0).then_some(bg),
    );
    doc.resolution_cm = root.int_of("RSU") == Some(1);
    doc.color_math = if root.int_of("FCM") == Some(1) {
        ColorMath::Precise
    } else {
        ColorMath::Classic
    };
    // Files from Fireworks keep its perspective; FireCraft marks its pixel-correct warp (FCPi).
    doc.mx_perspective = root.int_of("FCP") != Some(1);
    // Frame delays (1/100 s) and export flags: EXPv > FNLv > FNMv{FNDi, VISb}.
    let fnm: Vec<&Node> = root
        .child("EXP")
        .and_then(|e| e.child("FNL"))
        .map(|l| l.children_tagged("FNM").collect())
        .unwrap_or_default();
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
    let vis: Vec<bool> = n
        .child("VIF")
        .map(|v| v.children_tagged("VIS").filter_map(Node::as_bool).collect())
        .unwrap_or_default();
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
                    cel.child("ELM")
                        .map(|elm| read_elements(elm, ctx, 0))
                        .unwrap_or_default()
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
    elm.children
        .iter()
        .rev()
        .filter_map(|c| read_element(c, ctx, depth))
        .collect()
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
    el.effects = n
        .child("EFL")
        .map(firecraft_doc::effects::read_list)
        .unwrap_or_default();
    // Object name (Layers panel / Property inspector), `OBNs`.
    el.name = n.str_of("OBN").filter(|s| !s.is_empty()).map(str::to_owned);
    // Hidden objects (eye off in the Layers panel).
    el.visible = n.bool_of("VIS").unwrap_or(true);
    // Per-object lock (Fireworks CS; MX files don't have it).
    el.locked = n.bool_of("LCK").unwrap_or(false);
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
    let layers = m
        .child("LYL")
        .map(|l| l.children_tagged("LAY").map(|l| read_layer(l, ctx)).collect())
        .unwrap_or_default();
    Some(Symbol {
        id,
        name: m.str_of("UNM").unwrap_or("Symbol").to_owned(),
        kind: match m.int_of("MSY") {
            Some(1) => firecraft_doc::SymbolKind::Button,
            Some(2) => firecraft_doc::SymbolKind::Animation,
            _ => firecraft_doc::SymbolKind::Graphic,
        },
        layers,
        frames: m.int_of("FRC").unwrap_or(1).clamp(1, 10_000) as usize,
        // NGSv: four NGCf guides, top y, bottom y, left x, right x (read from Fireworks CS
        // files; FireCraft keeps the node as read when saving).
        // (FNGv: the same guides kept by FireCraft where CS can't take them.)
        nine: m.child("NGS").or_else(|| m.child("FNG")).and_then(|n| {
            let v: Vec<f64> = n
                .children
                .iter()
                .filter(|c| c.is("NGC"))
                .filter_map(|c| c.text.trim().parse().ok())
                .collect();
            match v.as_slice() {
                [top, bottom, left, right] => Some((
                    left.min(*right),
                    left.max(*right),
                    top.min(*bottom),
                    top.max(*bottom),
                )),
                _ => None,
            }
        }),
        raw: Some(m.clone()),
    })
}

/// Union of the symbol's frame-1 content bounds.
fn symbol_bounds(s: &Symbol) -> Option<(f64, f64, f64, f64)> {
    let bs: Vec<_> = s
        .layers
        .iter()
        .filter(|l| !l.web)
        .flat_map(|l| l.elements(0))
        .filter_map(|e| e.bounds())
        .collect();
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
    let doc_space = matrix_of(n.child("MTX"));
    let matrix = Affine::translate(-ctx.origin.0, -ctx.origin.1).then(&doc_space);
    let (name, sb) = ctx
        .symbols
        .get(&symbol)
        .cloned()
        .unwrap_or_else(|| ("Symbol".into(), None));
    let bounds = sb.map(|b| matrix.map_rect(b.0, b.1, b.2, b.3));
    ElementKind::Instance {
        symbol,
        name,
        matrix,
        bounds,
    }
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
            let Some(mask) = n
                .child("MRM")
                .and_then(|m| m.children.first())
                .and_then(|m| read_element(m, ctx, depth + 1))
            else {
                // A bitmap mask has no MRMv: the first (top) ELMv child is the mask itself, stored
                // inverted (Fireworks keeps 255 - value; measured: a white mask is saved black).
                if kids.len() >= 2 && kids.first().is_some_and(|k| k.is("IMG")) {
                    if let Some(mut mask) = read_element(kids.remove(0), ctx, depth + 1) {
                        if let ElementKind::Bitmap(b) = &mut mask.kind {
                            invert_mask_bitmap(&mut b.image);
                        }
                        let content = kids
                            .iter()
                            .rev()
                            .filter_map(|c| read_element(c, ctx, depth + 1))
                            .collect();
                        let mode = if grayscale {
                            MaskMode::Grayscale
                        } else {
                            MaskMode::Alpha
                        };
                        return ElementKind::Masked {
                            content,
                            mask: Box::new(mask),
                            mode,
                            cache: None,
                        };
                    }
                }
                return plain_group(n, ctx, depth);
            };
            // The cache is an IMGv on top whose bounds match the mask's.
            let mut cache = None;
            if let (Some(first), Some(mb)) = (kids.first(), mask.bounds()) {
                if first.is("IMG") {
                    if let Some(b) = read_bitmap(first, ctx) {
                        let close = |a: f64, b: f64| (a - b).abs() <= 1.5;
                        if close(b.x as f64, mb.0)
                            && close(b.y as f64, mb.1)
                            && close(b.image.width() as f64, mb.2)
                            && close(b.image.height() as f64, mb.3)
                        {
                            let mut e = Element::new(ElementKind::Bitmap(b));
                            e.raw = Some((*first).clone());
                            cache = Some(Box::new(e));
                            kids.remove(0);
                        }
                    }
                }
            }
            let content = kids
                .iter()
                .rev()
                .filter_map(|c| read_element(c, ctx, depth + 1))
                .collect();
            let mode = if grayscale {
                MaskMode::Grayscale
            } else {
                MaskMode::Alpha
            };
            ElementKind::Masked {
                content,
                mask: Box::new(mask),
                mode,
                cache,
            }
        }
        2 if kids.len() >= 2 => {
            let Some(mask) = read_element(kids.remove(0), ctx, depth + 1) else {
                return plain_group(n, ctx, depth);
            };
            let content = kids
                .iter()
                .rev()
                .filter_map(|c| read_element(c, ctx, depth + 1))
                .collect();
            ElementKind::Masked {
                content,
                mask: Box::new(mask),
                mode: MaskMode::PathOutline,
                cache: None,
            }
        }
        _ => plain_group(n, ctx, depth),
    }
}

/// Fireworks stores bitmap masks with inverted colour channels (alpha unchanged); the same
/// function converts both ways.
pub(crate) fn invert_mask_bitmap(img: &mut RgbaImage) {
    for p in img.pixels_mut() {
        *p = Rgba::new(255 - p.r, 255 - p.g, 255 - p.b, p.a);
    }
}

fn plain_group(n: &Node, ctx: &Ctx, depth: usize) -> ElementKind {
    ElementKind::Group(
        n.child("ELM")
            .map(|e| read_elements(e, ctx, depth + 1))
            .unwrap_or_default(),
    )
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
    let ids: Vec<u32> = n
        .child("TIL")
        .map(|t| t.children.iter().filter_map(Node::as_u32).collect())
        .unwrap_or_default();
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
        return Some(Bitmap {
            x: x + xof as i64,
            y: y + yof as i64,
            image: visible,
        });
    }
    Some(Bitmap { x, y, image })
}

fn read_path(n: &Node, ctx: &Ctx) -> Path {
    let (ox, oy) = ctx.origin;
    let contours: Vec<Contour> = n
        .child("PBL")
        .map(|pbl| {
            pbl.children_tagged("PBP")
                .map(|pbp| Contour {
                    closed: pbp.bool_of("ISC").unwrap_or(false),
                    points: pbp
                        .children_tagged("PBT")
                        .map(|p| {
                            // Control points are absolute; a missing one coincides with the anchor.
                            let (ax, ay) =
                                (p.float_of("XLC").unwrap_or(0.0), p.float_of("YLC").unwrap_or(0.0));
                            let rel = |tx: &str, ty: &str| {
                                (
                                    p.float_of(tx).map(|v| v - ax).unwrap_or(0.0),
                                    p.float_of(ty).map(|v| v - ay).unwrap_or(0.0),
                                )
                            };
                            BezierPoint {
                                x: ax - ox,
                                y: ay - oy,
                                pred: rel("XPC", "YPC"),
                                succ: rel("XSC", "YSC"),
                            }
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
            feather: preset.int_of("FEF").unwrap_or(0).clamp(0, 1024) as u16,
            pattern: (category == "fc_Pattern")
                .then(|| read_pattern(preset, n, ctx))
                .flatten(),
            dither: (category == "fc_WebDither")
                .then(|| read_web_dither(preset))
                .flatten(),
            dither_transparent: preset.bool_of("FDT").unwrap_or(false),
            gradient: read_gradient(&category, preset, n, ctx),
            category,
            texture: a.child("TXF").and_then(|t| read_texture(t, ctx)),
            texture_amount: preset.int_of("FTB").unwrap_or(0).clamp(0, 1000) as u16,
            // FSMi 1 = transparent; anything else (2 is the default) lightens the colour.
            texture_transparent: preset.int_of("FSM") == Some(1),
        })
    });
    let stroke = attrs.and_then(|a| {
        let preset = a.child("BPL")?;
        Some(Stroke {
            color: argb(a.u32_of("BCL").unwrap_or(0xFF00_0000)),
            category: preset.str_of("CAT").unwrap_or("bc_Pencil").to_owned(),
            name: preset.str_of("INM").unwrap_or_default().to_owned(),
            diameter: preset.int_of("BDI").unwrap_or(1).clamp(1, 1000) as f64,
            // BSEi is in tenths of a percent (1000 = 100%).
            softness: (preset.int_of("BSE").unwrap_or(0).clamp(0, 1000) / 10) as u8,
            anti_alias: preset.bool_of("BIA").unwrap_or(true),
            brush: Some(preset.clone()),
            placement: n.int_of("BRP").unwrap_or(1).clamp(0, 2) as u8,
            texture: a.child("TXB").and_then(|t| read_texture(t, ctx)),
        })
    });
    let rect = read_rect(n, &contours, ctx);
    let pcl = read_pcl(n, &contours, ctx);
    let texture_origin = match (n.float_of("TOX"), n.float_of("TOY")) {
        (Some(x), Some(y)) => Some((x - ctx.origin.0, y - ctx.origin.1)),
        _ => None,
    };
    Path {
        contours,
        fill,
        stroke,
        rect,
        pcl,
        texture_origin,
    }
}

/// A texture (`TXBv`/`TXFv { INMs [UNMs] MSKv }`): its name and 8-bit mask, whose tiles
/// (`TSZi{100}`) are 256 x 256 coverage bytes laid out row-major over `WPXi` x `HPXi`.
fn read_texture(n: &Node, ctx: &Ctx) -> Option<firecraft_doc::Texture> {
    let name = n.str_of("INM").unwrap_or_default();
    let m = n.child("MSK")?;
    let w = m.int_of("WPX")?.clamp(0, 16_384) as u32;
    let h = m.int_of("HPX")?.clamp(0, 16_384) as u32;
    let tsz = m.int_of("TSZ").unwrap_or(256).clamp(1, 4096) as u32;
    let cols = w.div_ceil(tsz).max(1);
    let mut data = vec![255u8; w as usize * h as usize];
    let ids = m
        .child("TIL")
        .map(|t| t.children.iter().filter_map(Node::as_u32).collect::<Vec<_>>())
        .unwrap_or_default();
    let mut found = false;
    for (i, id) in ids.iter().enumerate() {
        let Some(tile) = ctx.tiles.get(id) else { continue };
        found = true;
        let (tx, ty) = ((i as u32 % cols) * tsz, (i as u32 / cols) * tsz);
        for y in 0..tsz.min(h.saturating_sub(ty)) {
            for x in 0..tsz.min(w.saturating_sub(tx)) {
                if let (Some(v), Some(d)) = (
                    tile.data.get((y * tsz + x) as usize),
                    data.get_mut(((ty + y) * w + tx + x) as usize),
                ) {
                    *d = *v;
                }
            }
        }
    }
    if !found {
        return None;
    }
    firecraft_doc::Texture::new(name, w, h, data)
}

/// Fireworks' flattened outline (`PCLv { PPLv* { ISCb, PPTv* { XLCf YLCf PRSf VELf RNDi } } }`,
/// document coordinates): what its brushes stroke, with pen pressure and velocity per point.
fn read_pcl(n: &Node, contours: &[Contour], ctx: &Ctx) -> Option<StoredPolyline> {
    let (ox, oy) = ctx.origin;
    let lines: Vec<PolyLine> = n
        .child("PCL")?
        .children_tagged("PPL")
        .map(|l| PolyLine {
            closed: l.bool_of("ISC").unwrap_or(false),
            points: l
                .children_tagged("PPT")
                .map(|p| PolyPoint {
                    x: p.float_of("XLC").unwrap_or(0.0) - ox,
                    y: p.float_of("YLC").unwrap_or(0.0) - oy,
                    pressure: p.float_of("PRS").unwrap_or(0.0) as f32,
                    velocity: p.float_of("VEL").unwrap_or(0.0) as f32,
                    seed: p.u32_of("RND").unwrap_or(0),
                })
                .collect(),
        })
        .collect();
    (!lines.is_empty()).then(|| StoredPolyline {
        source: contours.to_vec(),
        lines,
    })
}

/// A Fireworks matrix (`MTXv`, row vectors) as an `Affine`.
pub(crate) fn matrix_of(m: Option<&Node>) -> Affine {
    let g = |t: &str, d: f64| m.and_then(|m| m.float_of(t)).unwrap_or(d);
    Affine {
        a: g("M00", 1.0),
        b: g("M01", 0.0),
        g: g("M02", 0.0),
        c: g("M10", 0.0),
        d: g("M11", 1.0),
        h: g("M12", 0.0),
        e: g("M20", 0.0),
        f: g("M21", 0.0),
        i: g("M22", 1.0),
    }
}

/// A rectangle primitive (`PRIv`: base box `RCTv` with roundness `RDSf`, matrix `MTXv`). If the
/// record doesn't match the path's points (files saved by FireCraft before it kept primitives
/// in step), it is fitted to them; if it can't be, the path is read as a plain path.
fn read_rect(n: &Node, contours: &[Contour], ctx: &Ctx) -> Option<RectShape> {
    let pri = n.child("PRI")?;
    let r = pri.child("RCT")?;
    let (l, t, rt, b) = (
        r.float_of("LFT")?,
        r.float_of("TOP")?,
        r.float_of("RIT")?,
        r.float_of("BOT")?,
    );
    // (Possibly with perspective: Fireworks keeps a distorted rectangle a primitive.)
    let doc_space = matrix_of(pri.child("MTX"));
    let mut shape = RectShape::new(l, t, rt, b, r.float_of("RDS").unwrap_or(0.0));
    shape.matrix = Affine::translate(-ctx.origin.0, -ctx.origin.1).then(&doc_space);
    let path = Path {
        contours: contours.to_vec(),
        rect: Some(shape),
        ..Path::default()
    };
    path.rect_in_step()
}

/// Gradient stops (`FGLv`: colour ramp `FG0`/`FGV`, opacity ramp `FG1`) and handles
/// (`PSXf`.. on the path, document coordinates).
fn read_gradient(category: &str, preset: &Node, path: &Node, ctx: &Ctx) -> Option<Gradient> {
    let kind = GradientKind::from_category(category)?;
    let fgl = preset.child("FGL")?;
    let stops = |n: &Node| -> Vec<(f32, Rgba)> {
        let mut v: Vec<(f32, Rgba)> = n
            .children_tagged("FGI")
            .map(|i| {
                (
                    i.float_of("FGP").unwrap_or(0.0).clamp(0.0, 1.0) as f32,
                    argb(i.u32_of("FGC").unwrap_or(0xFF00_0000)),
                )
            })
            .collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        v
    };
    let fgy = fgl.child("FGY");
    let colors = fgy
        .and_then(|y| y.child("FG0"))
        .or_else(|| fgl.child("FGV"))
        .map(stops)
        .unwrap_or_default();
    // The opacity ramp keeps opacity in each stop's alpha byte.
    let opacity = fgy
        .and_then(|y| y.child("FG1"))
        .map(|n| stops(n).into_iter().map(|(p, c)| (p, c.a)).collect())
        .unwrap_or_default();
    let (ox, oy) = ctx.origin;
    let pt = |x: &str, y: &str| {
        (
            path.float_of(x).unwrap_or(0.0) - ox,
            path.float_of(y).unwrap_or(0.0) - oy,
        )
    };
    Some(Gradient {
        kind,
        colors,
        opacity,
        start: pt("PSX", "PSY"),
        end: pt("PEX", "PEY"),
        side: pt("PFX", "PFY"),
    })
}

/// A pattern fill: its image is stored in the file (`FPTv`), with the handles on the path.
fn read_pattern(preset: &Node, path: &Node, ctx: &Ctx) -> Option<PatternFill> {
    let fpt = preset.child("FPT")?;
    let image = read_bitmap(fpt.child("IMG")?, ctx)?.image;
    let (ox, oy) = ctx.origin;
    let pt = |x: &str, y: &str| {
        (
            path.float_of(x).unwrap_or(0.0) - ox,
            path.float_of(y).unwrap_or(0.0) - oy,
        )
    };
    Some(PatternFill {
        name: fpt.str_of("INM").unwrap_or_default().to_owned(),
        image,
        start: pt("PSX", "PSY"),
        end: pt("PEX", "PEY"),
        side: pt("PFX", "PFY"),
    })
}

/// A Web Dither fill's 2 x 2 tile.
fn read_web_dither(preset: &Node) -> Option<[Rgba; 4]> {
    let c = |t: &str| preset.u32_of(t).map(argb);
    Some([c("FD1")?, c("FD2")?, c("FD3")?, c("FD4")?])
}

/// A run's attributes: `base`, changed by whatever `n` (or a run's preceding nodes) sets.
fn text_attrs(nodes: &[&Node], base: &TextRun) -> TextRun {
    let mut r = base.clone();
    for n in nodes {
        let t = String::from_utf8_lossy(&n.tag).to_string();
        match t.as_str() {
            "FON" => r.font = n.as_str().unwrap_or(&r.font).to_owned(),
            "PTS" => r.size = n.as_float().unwrap_or(r.size),
            "BOL" => r.bold = n.as_bool().unwrap_or(r.bold),
            "ITL" => r.italic = n.as_bool().unwrap_or(r.italic),
            "UND" => r.underline = n.as_bool().unwrap_or(r.underline),
            "HSC" => r.h_scale = n.as_float().unwrap_or(r.h_scale),
            "RKN" => r.tracking = n.as_float().unwrap_or(r.tracking),
            "BLS" => r.baseline_shift = n.as_float().unwrap_or(r.baseline_shift),
            "LED" => r.leading = n.as_float().unwrap_or(r.leading),
            "LDM" => r.leading_px = n.as_int().unwrap_or(0) != 0,
            "JST" => {
                let v = n.as_int().unwrap_or(0);
                r.align = if v & 0x0f >= 4 {
                    (v & 0x10) as u8
                } else {
                    v.clamp(0, 4) as u8
                };
            }
            "PIN" => r.indent = n.as_float().unwrap_or(r.indent),
            "PSB" => r.space_before = n.as_float().unwrap_or(r.space_before),
            "PSA" => r.space_after = n.as_float().unwrap_or(r.space_after),
            "FCL" => r.color = n.as_u32().map_or(r.color, argb),
            _ => {}
        }
    }
    r
}

/// How far Fireworks MX has moved a text object since it was typed: it leaves the block where it
/// was and keeps the move in the matrix. Only plain moves (a scaled or rotated object keeps its
/// matrix), and only MX's records: no Fireworks CS file here has shown what its block does.
pub(crate) fn text_move(n: &Node) -> Option<(f64, f64)> {
    let m = matrix_of(n.child("MTX"));
    let plain = [m.a - 1.0, m.b, m.g, m.c, m.d - 1.0, m.h, m.i - 1.0]
        .iter()
        .all(|v| v.abs() < 1e-9);
    (plain && n.child("LFA").is_none() && (m.e != 0.0 || m.f != 0.0)).then_some((m.e, m.f))
}

fn read_text(n: &Node, ctx: &Ctx) -> Text {
    let base = TextRun {
        text: String::new(),
        font: "Arial".into(),
        size: 12.0,
        color: n
            .child("PAT")
            .and_then(|a| a.u32_of("FCL"))
            .map(argb)
            .unwrap_or(Rgba::BLACK),
        bold: false,
        italic: false,
        underline: false,
        h_scale: 1.0,
        tracking: 0.0,
        baseline_shift: 0.0,
        leading: 1.0,
        leading_px: false,
        align: 0,
        indent: 0.0,
        space_before: 0.0,
        space_after: 0.0,
    };
    let own: Vec<&Node> = n.children.iter().collect();
    let initial = text_attrs(&own, &base);
    // `TFSv`: attribute changes, then the characters they apply to.
    let mut runs = Vec::new();
    let mut current = initial.clone();
    let mut pending: Vec<&Node> = Vec::new();
    for c in n.child("TFS").map_or(&[][..], |t| &t.children) {
        if c.is("TRN") {
            current = text_attrs(&pending, &current);
            pending.clear();
            runs.push(TextRun {
                text: c.as_str().unwrap_or_default().to_owned(),
                ..current.clone()
            });
        } else {
            pending.push(c);
        }
    }
    // Fireworks CS: the runs name the PostScript font (`CFTs`), the object its family (`FONs`).
    if let (Some(ps), Some(family)) = (n.str_of("CFT"), n.str_of("FON")) {
        for r in &mut runs {
            if r.font == ps {
                r.font = family.to_owned();
            }
        }
    }
    let (ox, oy) = ctx.origin;
    let edge = |t: &str| n.float_of(t).unwrap_or(0.0);
    // Fireworks CS: `TFTi` says what kind of block it is, and `LFAf`.. where.
    let kind = n.int_of("TFT");
    let anchor = (kind == Some(0)).then(|| (edge("LFA") - ox, edge("TOA") - oy));
    let frame = (kind == Some(1)).then(|| {
        (
            edge("LFA") - ox,
            edge("TOA") - oy,
            edge("RIA") - ox,
            edge("BOA") - oy,
        )
    });
    Text {
        text: runs.iter().map(|r| r.text.as_str()).collect(),
        font: initial.font.clone(),
        size: initial.size,
        color: initial.color,
        bold: initial.bold,
        italic: initial.italic,
        underline: initial.underline,
        runs,
        rect: frame.unwrap_or_else(|| {
            let (dx, dy) = text_move(n).unwrap_or_default();
            (
                edge("LFT") - ox + dx,
                edge("TOP") - oy + dy,
                edge("RIT") - ox + dx,
                edge("BOT") - oy + dy,
            )
        }),
        anchor,
        on_path: matches!(kind, Some(2 | 3)),
        auto_kern: n.bool_of("ATK").unwrap_or(true),
        anti_alias: n.int_of("TAA").unwrap_or(0).clamp(0, 2) as u8,
        // FireCraft extension (Fireworks MX ignores unknown nodes).
        auto_width: n.int_of("FAW") == Some(1),
        cs_layout: n.child("LFA").is_some(),
        edited: false,
        cached: n.child("IMG").and_then(|i| read_bitmap(i, ctx)),
    }
}

/// A text object read from its record alone, for tests.
#[cfg(test)]
pub(crate) fn text_from_node(n: &Node) -> Text {
    read_text(
        n,
        &Ctx {
            tiles: &HashMap::new(),
            origin: (0.0, 0.0),
            symbols: HashMap::new(),
        },
    )
}
