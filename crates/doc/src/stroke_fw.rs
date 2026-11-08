//! Path strokes drawn by Fireworks' brush engine (`firecraft-stroke`, adapted from FirePixel):
//! FireCraft's path, stroke and the original Fireworks nodes in, stroke pixels out.
//!
//! The brush comes from the path's brush preset (`BPLv`) when the file has one, else the
//! "1-Pixel Anti-Aliased" pencil FireCraft writes for new strokes; FireCraft's own stroke
//! settings (colour, diameter, softness, anti-aliasing) override it. The outline is Fireworks'
//! stored polyline (`PCLv`, with pen pressure and velocity) while the path is unchanged,
//! otherwise the path flattened the way Fireworks does it.

use firecraft_fwpng::Node;
use firecraft_raster::{Rgba, RgbaImage};
use firecraft_stroke::attrs::{
    BrushParams, BrushType, Feedback, PathContour, PointDynamics, Stroke as FwStroke, StrokePlacement,
    StrokeTexture, TipColoringMode, TipShape, TipSpacingMode, VectorPath,
};
use firecraft_stroke::support::color::Argb;
use firecraft_stroke::support::geom::{BezierContour, BezierNode, IRect, Point, Vec2};

use crate::element::{Contour, Path, Stroke};

/// The brush preset FireCraft writes for new strokes: Fireworks' "1-Pixel Anti-Aliased" pencil
/// with the stroke's diameter, softness and anti-aliasing.
pub fn pencil_preset(stroke: &Stroke) -> Node {
    let i = |t: &str, v: i64| Node::int(t, v);
    Node::vector(
        "BPL",
        vec![
            Node::string("CAT", "bc_Pencil"),
            Node::string("INM", "bn_1-Pixel Anti-Aliased"),
            Node::bool("RDO", false),
            i("BAN", 0),
            i("BAS", 1000),
            i("BDI", stroke.diameter.round().clamp(1.0, 1000.0) as i64),
            i("BMM", 15),
            i("BMS", 10),
            i("BSE", stroke.softness.min(100) as i64 * 10),
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
            Node::bool("BIA", stroke.anti_alias),
        ],
    )
}

/// Bring a brush preset in step with FireCraft's stroke settings (diameter, softness,
/// anti-aliasing), changing a field only when the stroke's value differs from what it holds.
pub fn sync_preset(bpl: &mut Node, stroke: &Stroke) {
    let dia = stroke.diameter.round().clamp(1.0, 1000.0) as i64;
    if bpl.int_of("BDI") != Some(dia) {
        put(bpl, Node::int("BDI", dia));
    }
    if bpl.int_of("BSE").map(|v| (v.clamp(0, 1000) / 10) as u8) != Some(stroke.softness.min(100)) {
        put(bpl, Node::int("BSE", stroke.softness.min(100) as i64 * 10));
    }
    if bpl.bool_of("BIA") != Some(stroke.anti_alias) {
        put(bpl, Node::bool("BIA", stroke.anti_alias));
    }
}

fn put(parent: &mut Node, node: Node) {
    match parent.children.iter_mut().find(|c| c.tag == node.tag) {
        Some(slot) => *slot = node,
        None => parent.children.push(node),
    }
}

const SENSE_SOURCES: &[u8] = b"PSHVR";
const SENSE_TARGETS: &[u8] = b"ZAOBRHLS";

/// A brush preset (`BPLv`) as the engine's parameters (FirePixel's `read_brush` +
/// `brush_params`).
pub fn brush_params(bpl: &Node) -> BrushParams {
    let g = |t: &str| bpl.int_of(t).unwrap_or(0).max(0) as u32;
    let mut p = BrushParams {
        diameter: g("BDI"),
        angle: g("BAN"),
        min_size: g("BMS"),
        aspect: g("BAS"),
        max_count: g("BMM"),
        softness: g("BSE"),
        shape: if g("BSH") == 1 {
            TipShape::Circle
        } else {
            TipShape::Square
        },
        bell_curve: g("BSF") == 1,
        kind: if g("BRT") == 0 {
            BrushType::Natural
        } else {
            BrushType::Simple
        },
        feedback: Feedback::from_u32(g("BFB")),
        tip_spacing_mode: match g("BSM") {
            1 => TipSpacingMode::Diagonal,
            2 => TipSpacingMode::Circular,
            _ => TipSpacingMode::Random,
        },
        tip_coloring_mode: match g("BCM") {
            1 => TipColoringMode::Uniform,
            2 => TipColoringMode::Complementary,
            3 => TipColoringMode::Hue,
            4 => TipColoringMode::Shadow,
            _ => TipColoringMode::Random,
        },
        alpha_remap: g("BEF"),
        spacing: g("BSP"),
        flow_rate: g("BFR"),
        tip_count: g("BNT"),
        tip_spacing: g("BTS"),
        blackness: g("BBK"),
        concentration: g("BCN"),
        texture_blend: g("BTB"),
        texture_edge: g("BTE"),
        antialiased: bpl.bool_of("BIA").unwrap_or(false),
        dashes: Vec::new(),
        sense: [[0; 5]; 8],
    };
    let dashes = g("NDI").min(3) as usize;
    for (on, off) in [("DO1", "DF1"), ("DO2", "DF2"), ("DO3", "DF3")]
        .into_iter()
        .take(dashes)
    {
        p.dashes.push((g(on), g(off)));
    }
    // Sensitivities: `S<source><target>i` entries (pressure, speed, ... -> size, angle, ...).
    for c in &bpl.children {
        let t = c.tag;
        if t[0] == b'S'
            && let (Some(si), Some(ti)) = (
                SENSE_SOURCES.iter().position(|s| *s == t[1]),
                SENSE_TARGETS.iter().position(|s| *s == t[2]),
            )
            && let Ok(v) = i64::from_str_radix(c.text.trim(), 16)
            && v > 0
            && let Some(row) = p.sense.get_mut(ti)
            && let Some(slot) = row.get_mut(si)
        {
            *slot = BrushParams::sense_from_permille(v as u32);
        }
    }
    p
}

/// How far a stroke can reach beyond the path's outline, in pixels (FirePixel's
/// `stroke_margin`: conservative; a natural brush with scatter throws tips about 10 diameters).
pub fn margin(stroke: &Stroke) -> f64 {
    let b = brush_for(stroke, None);
    let fw = FwStroke::new(b.clone(), Argb(0));
    let placement = StrokePlacement::from_u32(stroke.placement as u32);
    let dp = firecraft_stroke::stroke::effective_diameter(&fw, placement);
    let w = firecraft_stroke::brush::tip_width(dp, b.shape == TipShape::Circle);
    let mut m = w / 2.0 + 3.0;
    if b.kind == BrushType::Natural {
        // Multiple tips spread over `BTSi` percent of the diameter.
        m += b.tip_spacing as f64 * dp / 100.0 + 2.0;
        // Only a brush with scatter throws dabs far from the line.
        if b.row_active(firecraft_stroke::attrs::SenseTarget::Scatter) {
            m += 10.0 * dp;
        }
    }
    if placement == StrokePlacement::Inside {
        m = m.min(w + 3.0);
    }
    m.clamp(0.0, 100_000.0)
}

/// The engine's brush for a FireCraft stroke: the file's preset (or FireCraft's pencil) with the
/// stroke's own settings.
fn brush_for(stroke: &Stroke, raw: Option<&Node>) -> BrushParams {
    let mut bpl = stroke
        .brush
        .clone()
        .or_else(|| {
            raw.and_then(|r| r.child("PAT"))
                .and_then(|a| a.child("BPL"))
                .cloned()
        })
        .unwrap_or_else(|| pencil_preset(stroke));
    sync_preset(&mut bpl, stroke);
    brush_params(&bpl)
}

/// The path's contours for the engine, offset by `off`: Fireworks' stored polyline while it
/// belongs to the current outline, else the outline flattened as Fireworks does.
fn contours(path: &Path, off: (f64, f64)) -> Vec<PathContour> {
    if let Some(pcl) = path.pcl.as_ref().filter(|p| p.source == path.contours) {
        return pcl
            .lines
            .iter()
            .filter(|l| !l.points.is_empty())
            .map(|l| PathContour {
                points: l
                    .points
                    .iter()
                    .map(|q| Vec2::new(q.x + off.0, q.y + off.1))
                    .collect(),
                closed: l.closed,
                dynamics: l
                    .points
                    .iter()
                    .map(|q| PointDynamics {
                        pressure: q.pressure,
                        velocity: q.velocity,
                        seed: q.seed,
                    })
                    .collect(),
            })
            .collect();
    }
    path.contours
        .iter()
        .filter(|c| !c.points.is_empty())
        .map(|c| PathContour::from_bezier(&bezier(c, off), None))
        .collect()
}

fn bezier(c: &Contour, off: (f64, f64)) -> BezierContour {
    let p = |x: f64, y: f64| Point::new((x + off.0) as f32, (y + off.1) as f32);
    BezierContour::new(
        c.points
            .iter()
            .map(|b| {
                BezierNode::new(
                    p(b.x, b.y),
                    p(b.x + b.pred.0, b.y + b.pred.1),
                    p(b.x + b.succ.0, b.y + b.succ.1),
                )
            })
            .collect(),
        c.closed,
    )
}

/// Where `path`'s texture is anchored: its texture origin, else the top-left of its points.
pub fn texture_origin(path: &Path) -> (f64, f64) {
    path.texture_origin.unwrap_or_else(|| {
        let pts = path.contours.iter().flat_map(|c| c.points.iter());
        let x = pts.clone().map(|p| p.x).fold(f64::INFINITY, f64::min);
        let y = pts.map(|p| p.y).fold(f64::INFINITY, f64::min);
        if x.is_finite() && y.is_finite() {
            (x, y)
        } else {
            (0.0, 0.0)
        }
    })
}

/// Draw `path`'s stroke onto `layer` (whose pixel (0, 0) is canvas point -`off`), as Fireworks
/// does. `raw` is the path's original Fireworks node, if any.
pub fn draw(layer: &mut RgbaImage, path: &Path, stroke: &Stroke, raw: Option<&Node>, off: (f64, f64)) {
    let (w, h) = (layer.width(), layer.height());
    if w == 0 || h == 0 || !(stroke.diameter > 0.0) {
        return;
    }
    let flag = |t: &str| raw.and_then(|r| r.int_of(t));
    let vp = VectorPath {
        contours: contours(path, off),
        even_odd: raw.and_then(|r| r.bool_of("EOF")).unwrap_or(true),
        agm: raw.and_then(|r| r.bool_of("AGM")).unwrap_or(false),
        fill: None,
        stroke: None,
        fill_over_stroke: raw.and_then(|r| r.bool_of("FOT")).unwrap_or(false),
        placement: StrokePlacement::from_u32(stroke.placement as u32),
        ptt: flag("PTT") == Some(1),
        texture_offset: {
            let (x, y) = texture_origin(path);
            Vec2::new(x + off.0, y + off.1)
        },
        random_seed: flag("RND").unwrap_or(0) as u32,
    };
    let c = stroke.color;
    let mut fw = FwStroke::new(
        brush_for(stroke, raw),
        Argb(((c.a as u32) << 24) | ((c.r as u32) << 16) | ((c.g as u32) << 8) | c.b as u32),
    );
    fw.texture = stroke.texture.as_ref().and_then(|t| {
        let r = IRect::new(0, 0, t.width as i32, t.height as i32);
        firecraft_stroke::support::raster::Mask::from_raw(r, t.mask.to_vec())
            .ok()
            .map(|m| StrokeTexture {
                mask: std::sync::Arc::new(m),
            })
    });
    let rect = IRect::new(0, 0, w as i32, h as i32);
    let px = firecraft_stroke::stroke::stroke_layer(&vp, &fw, rect);
    for (k, p) in px.iter().enumerate() {
        if p[3] == 0 {
            continue;
        }
        let (x, y) = ((k % w as usize) as i64, (k / w as usize) as i64);
        if let Some(d) = layer.get(x, y) {
            layer.set(x, y, d.over(Rgba::new(p[0], p[1], p[2], 255), p[3]));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::element::BezierPoint;

    /// A white "1-Pixel Anti-Aliased" pencil line on black, measured in Fireworks MX
    /// (docs/behavior/strokes.md): 191 in the row a line through pixel centres covers; 112/111
    /// on a row boundary, the last pixel before the end 64; 1/16-px steps (y + 1/16: 183/16).
    #[test]
    fn one_pixel_pencil_matches_fireworks_mx() {
        let at = |y: f64| {
            let path = Path {
                contours: vec![Contour {
                    points: vec![BezierPoint::corner(20.0, y), BezierPoint::corner(60.0, y)],
                    closed: false,
                }],
                ..Path::default()
            };
            let stroke = Stroke {
                color: Rgba::WHITE,
                category: "bc_Pencil".into(),
                name: "bn_1-Pixel Anti-Aliased".into(),
                diameter: 1.0,
                softness: 0,
                anti_alias: true,
                brush: None,
                placement: 1,
                texture: None,
            };
            let mut img = RgbaImage::new(80, 10, Rgba::BLACK);
            draw(&mut img, &path, &stroke, None, (0.0, 0.0));
            img
        };
        let r = |img: &RgbaImage, x: i64, y: i64| img.get(x, y).map(|p| p.r);
        let c = at(4.5);
        assert_eq!((r(&c, 40, 4), r(&c, 40, 3)), (Some(191), Some(0)));
        let b = at(5.0);
        assert_eq!((r(&b, 40, 4), r(&b, 40, 5)), (Some(111), Some(112)));
        assert_eq!((r(&b, 59, 4), r(&b, 60, 4)), (Some(64), Some(0)));
        let q = at(4.5625);
        assert_eq!((r(&q, 40, 4), r(&q, 40, 5)), (Some(183), Some(16)));
    }
}
