//! Document elements (Fireworks "objects").

use firecraft_fwpng::Node;
use firecraft_raster::effects::Effect;
use firecraft_raster::gradient::Gradient;
use firecraft_raster::nine::{AxisMap, resample};
use firecraft_raster::path::{Pt, fill_polygons_feathered, fill_polygons_with, flatten_cubic};
use firecraft_raster::pattern::{PatternFill, web_dither_at};
use firecraft_raster::transform::{Affine, sample, transform_image};
use firecraft_raster::{BlendMode, Rgba, RgbaImage};

use crate::{MAX_SYMBOL_DEPTH, RenderCtx};

/// A bitmap object. Pixels are in canvas coordinates starting at (`x`, `y`).
#[derive(Clone, Debug, PartialEq)]
pub struct Bitmap {
    pub x: i64,
    pub y: i64,
    pub image: RgbaImage,
}

/// One bezier anchor. Control points are kept *relative* to the anchor here; Fireworks stores
/// them as absolute coordinates (`XPCf/YPCf` predecessor, `XSCf/YSCf` successor) and omits a
/// control point that coincides with its anchor.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BezierPoint {
    pub x: f64,
    pub y: f64,
    pub pred: (f64, f64),
    pub succ: (f64, f64),
}

impl BezierPoint {
    pub fn corner(x: f64, y: f64) -> Self {
        Self {
            x,
            y,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Contour {
    pub points: Vec<BezierPoint>,
    pub closed: bool,
}

impl Contour {
    /// Flatten to a polyline in canvas pixels, offset by (`ox`, `oy`).
    pub fn flatten(&self, ox: f64, oy: f64) -> Vec<Pt> {
        let mut out = Vec::new();
        let Some(first) = self.points.first() else {
            return out;
        };
        out.push((first.x + ox, first.y + oy));
        let n = self.points.len();
        let segs = if self.closed { n } else { n.saturating_sub(1) };
        for i in 0..segs {
            let (Some(a), Some(b)) = (self.points.get(i), self.points.get((i + 1) % n)) else {
                break;
            };
            flatten_cubic(
                (a.x + ox, a.y + oy),
                (a.x + a.succ.0 + ox, a.y + a.succ.1 + oy),
                (b.x + b.pred.0 + ox, b.y + b.pred.1 + oy),
                (b.x + ox, b.y + oy),
                &mut out,
            );
        }
        out
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Fill {
    pub color: Rgba,
    /// Fireworks fill category, e.g. "fc_Solid", "fc_Linear".
    pub category: String,
    pub anti_alias: bool,
    /// Feathered edge: its radius in pixels (`FEFi`), 0 for none.
    pub feather: u16,
    /// Gradient fills (handles in canvas coordinates); `None` for solid fills.
    pub gradient: Option<Gradient>,
    /// Pattern fills (`fc_Pattern`).
    pub pattern: Option<PatternFill>,
    /// Web Dither fills (`fc_WebDither`, `FD1i`..`FD4i`): the 2 x 2 tile, by rows.
    pub dither: Option<[Rgba; 4]>,
    /// Web Dither's Transparent option (`FDTb`): every other pixel is left empty.
    pub dither_transparent: bool,
    /// The fill's texture (`TXFv`; Fireworks keeps one on every path, "Grain" by default).
    pub texture: Option<Texture>,
    /// Texture amount in per mille (`FTBi` in the fill preset).
    pub texture_amount: u16,
    /// "Transparent" (`FSMi` 1): the texture takes coverage away; otherwise (`FSMi` 2) it
    /// lightens the colour.
    pub texture_transparent: bool,
}

impl Fill {
    /// The fill's handles (gradients and patterns), canvas coordinates.
    pub fn handles_mut(&mut self) -> Vec<&mut (f64, f64)> {
        match (&mut self.gradient, &mut self.pattern) {
            (Some(g), _) => vec![&mut g.start, &mut g.end, &mut g.side],
            (_, Some(p)) => vec![&mut p.start, &mut p.end, &mut p.side],
            _ => Vec::new(),
        }
    }
}

/// Below this much perspective Fireworks MX still treats an instance's matrix as a plain scale
/// (measured: 0.00001 scales strokes, 0.0002 doesn't; the value between is a choice).
const MX_PERSPECTIVE_THRESHOLD: f64 = 5e-5;

/// Map `e` through an instance's matrix as Fireworks MX does: strokes scale with a plain matrix
/// but keep their width in perspective (measured on Confetti strokes at 50% and 100%).
fn map_instance_child(e: &mut Element, matrix: &Affine) {
    fn widths(e: &Element, out: &mut Vec<f64>) {
        match &e.kind {
            ElementKind::Path(p) => out.push(p.stroke.as_ref().map_or(0.0, |s| s.diameter)),
            ElementKind::Group(c) => c.iter().for_each(|x| widths(x, out)),
            _ => {}
        }
    }
    fn restore(e: &mut Element, it: &mut std::vec::IntoIter<f64>) {
        match &mut e.kind {
            ElementKind::Path(p) => {
                let width = it.next().unwrap_or(1.0);
                if let Some(s) = &mut p.stroke {
                    s.diameter = width;
                }
            }
            ElementKind::Group(c) => c.iter_mut().for_each(|x| restore(x, it)),
            _ => {}
        }
    }
    let keep = matrix.g.abs() >= MX_PERSPECTIVE_THRESHOLD || matrix.h.abs() >= MX_PERSPECTIVE_THRESHOLD;
    let mut kept = Vec::new();
    if keep {
        widths(e, &mut kept);
    }
    e.transform(matrix);
    if keep {
        restore(e, &mut kept.into_iter());
    }
}

/// Whether an element redraws faithfully at a larger scale: plain paths, whose fills, simple
/// strokes and resizable effects (colour adjustments, blurs, shadows, glows, bevels) can all be
/// drawn bigger. Textures, natural brushes and the plug-in effects change with scale.
fn redraws_at_any_scale(e: &Element) -> bool {
    let ElementKind::Path(p) = &e.kind else {
        return false;
    };
    let fill_ok = p.fill.as_ref().is_none_or(|f| f.texture_amount == 0);
    let stroke_ok = p.stroke.as_ref().is_none_or(|s| {
        s.brush
            .as_ref()
            .is_none_or(|b| b.int_of("BRT") != Some(0) && b.int_of("BTB").unwrap_or(0) == 0)
    });
    let effects_ok = e
        .effects
        .iter()
        .all(|x| x.effect.as_ref().is_none_or(|fx| fx.scaled(1.0).is_some()));
    effects_ok && fill_ok && stroke_ok
}

/// The most a perspective matrix enlarges at the corners of a rectangle in front of its horizon.
fn max_stretch(matrix: &Affine, (x0, y0, x1, y1): (f64, f64, f64, f64)) -> f64 {
    let side = matrix.i.signum();
    let in_front = |x: f64, y: f64| (matrix.g * x + matrix.h * y + matrix.i) * side > 0.0;
    let stretch = |x: f64, y: f64| {
        let p = matrix.apply(x, y);
        let (right, down) = (matrix.apply(x + 1.0, y), matrix.apply(x, y + 1.0));
        ((right.0 - p.0).hypot(right.1 - p.1)).max((down.0 - p.0).hypot(down.1 - p.1))
    };
    [(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
        .into_iter()
        .filter(|(x, y)| in_front(*x, *y))
        .map(|(x, y)| stretch(x, y))
        .filter(|s| s.is_finite())
        .fold(1.0, f64::max)
}

/// How much larger than symbol space to draw so the most enlarged part of the instance has a
/// pixel for each pixel on screen: a whole number up to 32, within a budget of pixels, so a small
/// symbol can go as far as it needs and a large one at least as far as memory allows.
fn warp_detail(matrix: &Affine, (x0, y0, x1, y1): (f64, f64, f64, f64)) -> f64 {
    let most = max_stretch(matrix, (x0, y0, x1, y1));
    let mut detail = most.ceil().clamp(1.0, 32.0);
    while detail > 1.0 && (x1 - x0) * (y1 - y0) * detail * detail > (1 << 24) as f64 {
        detail -= 1.0;
    }
    detail
}

/// An output pixel of a warp over a detailed image: one sample where the image is about pixel
/// sized there, and an average over the pixel where it's denser (the far side of a perspective),
/// which keeps edges anti-aliased.
fn sample_footprint(flat: &RgbaImage, u: f64, v: f64, texel: &impl Fn(f64, f64) -> (f64, f64)) -> Rgba {
    let at = texel(u, v);
    let (right, down) = (texel(u + 1.0, v), texel(u, v + 1.0));
    let across = (right.0 - at.0).abs().max((right.1 - at.1).abs());
    let along = (down.0 - at.0).abs().max((down.1 - at.1).abs());
    let n = (across.max(along).round() as i64).clamp(1, 4);
    if n == 1 {
        return sample(flat, at.0, at.1);
    }
    let (mut sum, mut alpha) = ([0.0f64; 3], 0.0f64);
    for j in 0..n {
        for i in 0..n {
            let offset = |k: i64| (k as f64 + 0.5) / n as f64 - 0.5;
            let (px, py) = texel(u + offset(i), v + offset(j));
            let c = sample(flat, px, py);
            let a = f64::from(c.a);
            alpha += a;
            sum[0] += f64::from(c.r) * a;
            sum[1] += f64::from(c.g) * a;
            sum[2] += f64::from(c.b) * a;
        }
    }
    if alpha <= 0.0 {
        return Rgba::TRANSPARENT;
    }
    let channel = |c: f64| (c / alpha).round().clamp(0.0, 255.0) as u8;
    Rgba::new(
        channel(sum[0]),
        channel(sum[1]),
        channel(sum[2]),
        (alpha / (n * n) as f64).round().clamp(0.0, 255.0) as u8,
    )
}

/// `round(v / 255)` for `v` up to 255 * 255 (Fireworks' rounding).
fn round255(v: u32) -> u32 {
    ((v + 128) + ((v + 128) >> 8)) >> 8
}

/// Apply a fill's texture to its pixels (FirePixel's `paint.rs`, Apache-2.0): weight
/// `w = FTBi * 255 / 1000`, the mask tiled from the texture origin `(ox, oy)` (canvas pixels;
/// `at` = the layer's top-left canvas pixel). Opaque: colour toward white,
/// `f = round((255 - T) w / 255)`, `c' = c + round((255 - c) f / 255)`. Transparent: coverage
/// `s = T + (255 - w) - round(T (255 - w) / 255)`, `a' = round(s a / 255)`.
pub fn apply_fill_texture(layer: &mut RgbaImage, f: &Fill, origin: (f64, f64), at: (i64, i64)) {
    let Some(t) = &f.texture else { return };
    let w = f.texture_amount.min(1000) as u32 * 255 / 1000;
    if w == 0 || t.width == 0 || t.height == 0 {
        return;
    }
    let (tw, th) = (t.width as i64, t.height as i64);
    let (ox, oy) = (origin.0.floor() as i64, origin.1.floor() as i64);
    let (lw, lh) = (layer.width() as i64, layer.height() as i64);
    for ly in 0..lh {
        let v = (ly + at.1 - oy).rem_euclid(th);
        for lx in 0..lw {
            let Some(mut p) = layer.get(lx, ly) else { continue };
            if p.a == 0 {
                continue;
            }
            let u = (lx + at.0 - ox).rem_euclid(tw);
            let tv = t.mask.get((v * tw + u) as usize).copied().unwrap_or(255) as u32;
            if f.texture_transparent {
                let s = tv + (255 - w) - round255(tv * (255 - w));
                p.a = round255(s * p.a as u32).min(255) as u8;
            } else {
                let k = (w * (255 - tv) + 127) / 255;
                let ch = |c: u8| (c as u32 + ((255 - c as u32) * k + 127) / 255).min(255) as u8;
                (p.r, p.g, p.b) = (ch(p.r), ch(p.g), ch(p.b));
            }
            layer.set(lx, ly, p);
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub color: Rgba,
    /// Brush category and name, e.g. "bc_Pencil" / "bn_1-Pixel Anti-Aliased".
    pub category: String,
    pub name: String,
    pub diameter: f64,
    pub softness: u8,
    /// Anti-aliased tip (the brush preset's `BIAb`): placed at sub-pixel positions; otherwise
    /// it snaps to whole pixels.
    pub anti_alias: bool,
    /// The full brush preset (`BPLv`: tip, spacing, natural brush settings, dashes,
    /// sensitivities); `diameter`, `softness` and `anti_alias` above override its fields.
    /// `None`: Fireworks' 1-Pixel Anti-Aliased pencil.
    pub brush: Option<Node>,
    /// Stroke placement (`BRPi` on the path): 0 inside, 1 centred, 2 outside.
    pub placement: u8,
    /// The stroke's texture (`TXBv`), applied by the brush's texture amounts (`BTBi`, edge
    /// texture `BTEi`); kept when they are 0, as Fireworks does.
    pub texture: Option<Texture>,
}

impl Stroke {
    /// A centred stroke of preset `index` (see [`crate::brush_presets`]) with `diameter`.
    pub fn from_preset(color: Rgba, index: usize, diameter: f64) -> Stroke {
        let p = crate::brush_presets::BRUSH_PRESETS
            .get(index)
            .or(crate::brush_presets::BRUSH_PRESETS.first());
        let d = diameter.round().clamp(1.0, 1000.0);
        match p {
            Some(p) => Stroke {
                color,
                category: p.category.into(),
                name: p.name.into(),
                diameter: d,
                softness: (p.softness.clamp(0, 1000) / 10) as u8,
                anti_alias: p.antialiased,
                brush: Some(crate::brush_presets::node(p, d as i32)),
                placement: 1,
                texture: None,
            },
            None => Stroke {
                color,
                category: "bc_Pencil".into(),
                name: "bn_1-Pixel Anti-Aliased".into(),
                diameter: d,
                softness: 0,
                anti_alias: true,
                brush: None,
                placement: 1,
                texture: None,
            },
        }
    }

    /// The texture amount (the brush's `BTBi`, per mille; Fireworks' "Texture" percentage).
    pub fn texture_amount(&self) -> i32 {
        self.brush
            .as_ref()
            .and_then(|b| b.int_of("BTB"))
            .unwrap_or(0)
            .clamp(0, 1000) as i32
    }

    /// Set the texture amount (per mille); the brush keeps its other settings.
    pub fn set_texture_amount(&mut self, permille: i32) {
        let base = self
            .brush
            .clone()
            .unwrap_or_else(|| crate::stroke_fw::pencil_preset(self));
        let mut f = crate::brush_presets::BrushFields::from_node(&base);
        f.texture_blend = permille.clamp(0, 1000);
        self.brush = Some(f.to_node(&base));
    }

    /// Switch to preset `index`, keeping colour, size, placement and texture (as Fireworks' stroke
    /// category menu does).
    pub fn set_preset(&mut self, index: usize) {
        let keep = (self.color, self.diameter, self.placement, self.texture.take());
        *self = Stroke::from_preset(keep.0, index, keep.1);
        self.placement = keep.2;
        self.texture = keep.3;
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub contours: Vec<Contour>,
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
    /// Drawn with the Rectangle tool: Fireworks' rectangle primitive (`PRIv`), which Fireworks
    /// rebuilds from this record rather than from the points. Moves and transforms keep it;
    /// editing points makes the path a plain one (`None`).
    pub rect: Option<RectShape>,
    /// Fireworks' stored flattened outline (`PCLv`, with pen pressure and velocity per point),
    /// valid while the path's contours equal its `source`.
    pub pcl: Option<StoredPolyline>,
    /// Where its textures are anchored (`TOXf`/`TOYf`, canvas pixels); moves and transforms
    /// with the path. `None`: the top-left of its bounds.
    pub texture_origin: Option<(f64, f64)>,
}

/// A texture (a stroke's `TXBv { INMs, MSKv }`): its name and 8-bit mask, tiled across the
/// path from its texture origin. Fireworks files carry the mask, so a document shows its
/// textures without Fireworks' texture folder.
#[derive(Clone, Debug, PartialEq)]
pub struct Texture {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// `width` x `height` coverage values, row by row (shared between copies).
    pub mask: std::sync::Arc<Vec<u8>>,
}

impl Texture {
    /// A texture from a mask; `None` if the sizes don't match or it is empty.
    pub fn new(name: impl Into<String>, width: u32, height: u32, mask: Vec<u8>) -> Option<Texture> {
        (width > 0 && height > 0 && mask.len() == width as usize * height as usize).then(|| Texture {
            name: name.into(),
            width,
            height,
            mask: std::sync::Arc::new(mask),
        })
    }
}

/// One point of Fireworks' stored polyline (`PPTv`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PolyPoint {
    pub x: f64,
    pub y: f64,
    /// Pen pressure (`PRSf`), velocity (`VELf`) and random seed (`RNDi`).
    pub pressure: f32,
    pub velocity: f32,
    pub seed: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PolyLine {
    pub points: Vec<PolyPoint>,
    pub closed: bool,
}

/// Fireworks' flattened outline of a path and the contours it was made from.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredPolyline {
    pub source: Vec<Contour>,
    pub lines: Vec<PolyLine>,
}

impl StoredPolyline {
    /// Move it with its path.
    fn map(&mut self, f: impl Fn(f64, f64) -> (f64, f64)) {
        for q in self.lines.iter_mut().flat_map(|l| l.points.iter_mut()) {
            (q.x, q.y) = f(q.x, q.y);
        }
    }
}

/// A Fireworks rectangle primitive: a base rectangle with rounded corners, placed by `matrix`
/// (base space -> canvas).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectShape {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    /// Corner roundness, 0..=1: the corner radius as a fraction of half the shorter side
    /// (Fireworks' `RDSf`, shown as a percentage).
    pub roundness: f64,
    pub matrix: Affine,
}

impl RectShape {
    /// An axis-aligned rectangle in canvas space.
    pub fn new(left: f64, top: f64, right: f64, bottom: f64, roundness: f64) -> Self {
        Self {
            left: left.min(right),
            top: top.min(bottom),
            right: left.max(right),
            bottom: top.max(bottom),
            roundness: roundness.clamp(0.0, 1.0),
            matrix: Affine::IDENTITY,
        }
    }

    /// The outline as Fireworks builds it: four corners clockwise from top left, or with
    /// rounding eight points, from the left end of the top edge, each corner a quarter circle
    /// (bezier handles 0.5523 of the radius).
    pub fn contour(&self) -> Contour {
        let (l, t, r, b) = (self.left, self.top, self.right, self.bottom);
        let rad = self.roundness.clamp(0.0, 1.0) * (r - l).abs().min((b - t).abs()) / 2.0;
        let k = rad * 0.552_284_75;
        let pts = if rad <= 1e-9 {
            vec![
                BezierPoint::corner(l, t),
                BezierPoint::corner(r, t),
                BezierPoint::corner(r, b),
                BezierPoint::corner(l, b),
            ]
        } else {
            let p = |x, y, pred, succ| BezierPoint { x, y, pred, succ };
            vec![
                p(l + rad, t, (-k, 0.0), (0.0, 0.0)),
                p(r - rad, t, (0.0, 0.0), (k, 0.0)),
                p(r, t + rad, (0.0, -k), (0.0, 0.0)),
                p(r, b - rad, (0.0, 0.0), (0.0, k)),
                p(r - rad, b, (k, 0.0), (0.0, 0.0)),
                p(l + rad, b, (0.0, 0.0), (-k, 0.0)),
                p(l, b - rad, (0.0, k), (0.0, 0.0)),
                p(l, t + rad, (0.0, 0.0), (0.0, -k)),
            ]
        };
        let m = &self.matrix;
        let points = pts
            .into_iter()
            .map(|pt| {
                let (ax, ay) = m.apply(pt.x, pt.y);
                let pr = m.apply(pt.x + pt.pred.0, pt.y + pt.pred.1);
                let su = m.apply(pt.x + pt.succ.0, pt.y + pt.succ.1);
                BezierPoint {
                    x: ax,
                    y: ay,
                    pred: (pr.0 - ax, pr.1 - ay),
                    succ: (su.0 - ax, su.1 - ay),
                }
            })
            .collect();
        Contour { points, closed: true }
    }
}

impl Path {
    /// The rectangle record in step with the points: as it is when its outline matches them,
    /// else fitted to them by an affine transform (points changed without the record), else
    /// none (the points no longer form that rectangle: a plain path).
    pub fn rect_in_step(&self) -> Option<RectShape> {
        let mut r = self.rect?;
        let anchors = |c: &[Contour]| -> Vec<(f64, f64)> {
            c.iter()
                .flat_map(|c| c.points.iter())
                .map(|p| (p.x, p.y))
                .collect()
        };
        let points = anchors(&self.contours);
        let built = anchors(std::slice::from_ref(&r.contour()));
        let close = built.len() == points.len()
            && built
                .iter()
                .zip(&points)
                .all(|(p, q)| (p.0 - q.0).abs() < 0.5 && (p.1 - q.1).abs() < 0.5);
        if close {
            return Some(r);
        }
        let m = Affine::fit(&built, &points, 0.05)?;
        r.matrix = m.then(&r.matrix);
        Some(r)
    }

    /// Set a rectangle primitive's roundness (0..=1) and rebuild its outline.
    pub fn set_roundness(&mut self, roundness: f64) {
        if let Some(r) = &mut self.rect {
            r.roundness = roundness.clamp(0.0, 1.0);
            self.contours = vec![r.contour()];
        }
    }
}

/// Text attached to a path (Fireworks MX's `TAPv`).
#[derive(Clone, Debug, PartialEq)]
pub struct TextPath {
    /// `TAMi`: 0 rotate around the path, 1 vertical, 2 skew vertical, 3 skew horizontal.
    pub mode: u8,
    /// `CHOf`: how far along the path the text starts, in pixels.
    pub offset: f64,
    /// The path (`PTHv`), as it was: Detach from Path puts it back.
    pub path: Box<Element>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    /// The characters with their formatting, in order (`TFSv`).
    pub runs: Vec<TextRun>,
    /// The text block, canvas coordinates: left, top, right, bottom (`LFT`..`BOT`).
    pub rect: (f64, f64, f64, f64),
    /// Auto-kerning (`ATKb`).
    pub auto_kern: bool,
    /// Anti-alias mode (`TAAi`): 0 smooth, 1 crisp, 2 strong.
    pub anti_alias: u8,
    /// Laid out by Fireworks CS's rules (its record's block) rather than Fireworks MX's.
    pub cs_layout: bool,
    /// How the block is drawn on the canvas, as Fireworks MX keeps it (`MTXv`): scaled, rotated,
    /// skewed or distorted text keeps its block and formatting and is drawn through this.
    pub matrix: Affine,
    /// The text's fill when it isn't plain colour: a gradient, pattern or web dither shades the
    /// glyphs, and a texture applies over them. `None`: each run's colour.
    pub fill: Option<Fill>,
    /// A stroke along the characters' outlines (the brush, placement and texture of a path's).
    pub stroke: Option<Stroke>,
    /// `ORIi`: 0 horizontal, 1 vertical with columns right to left, 2 horizontal right to left,
    /// 3 vertical with columns left to right.
    pub orientation: u8,
    /// "Draw fill over stroke" (`FOTb`).
    pub fill_over_stroke: bool,
    /// The block grows with its text instead of wrapping (a click, not a drag, made it).
    pub auto_width: bool,
    /// Fireworks CS point text (`TFTi` 0): its first baseline's start, canvas coordinates
    /// (`LFAf`, `TOAf`). `rect` is then just the area it covers.
    pub anchor: Option<(f64, f64)>,
    /// Text bound to a shape FireCraft can't lay out yet (Fireworks CS text in a path, `TFTi` 3):
    /// shown from its stored image, not editable.
    pub on_path: bool,
    /// Text attached to a path (Text > Attach to Path): it runs along the path's first contour.
    pub path: Option<TextPath>,
    /// Its characters or formatting have changed since the file was read, so the record and its
    /// stored image are rewritten on save.
    pub edited: bool,
    /// The stored image already shows the object's Live Effects: Fireworks draws a text's effects
    /// into its `IMGv` (measured in Fireworks MX: a glowing text's stored image holds the glow).
    /// Until the text is drawn again, its effects aren't applied a second time.
    pub baked_effects: bool,
    pub text: String,
    pub font: String,
    pub size: f64,
    pub color: Rgba,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// Fireworks stores a pre-rendered image of the text; used until FireCraft's own text
    /// engine renders it (a text engine is on the roadmap).
    pub cached: Option<Bitmap>,
}

impl Text {
    /// Move the text by (`dx`, `dy`) canvas pixels: its block, or its matrix once it has one.
    pub fn shift(&mut self, dx: f64, dy: f64) {
        for h in self.fill.iter_mut().flat_map(Fill::handles_mut) {
            *h = (h.0 + dx, h.1 + dy);
        }
        if let Some(tp) = &mut self.path {
            tp.path.translate(dx, dy);
        }
        if self.matrix == Affine::IDENTITY {
            self.rect = (
                self.rect.0 + dx,
                self.rect.1 + dy,
                self.rect.2 + dx,
                self.rect.3 + dy,
            );
            self.anchor = self.anchor.map(|(x, y)| (x + dx, y + dy));
        } else {
            self.matrix = Affine::translate(dx, dy).then(&self.matrix);
        }
    }
}

/// The offset `m` moves by, if that's all it does.
pub fn plain_move(m: &Affine) -> Option<(f64, f64)> {
    let plain = [m.a - 1.0, m.b, m.c, m.d - 1.0, m.g, m.h, m.i - 1.0]
        .iter()
        .all(|v| v.abs() < 1e-9);
    plain.then_some((m.e, m.f))
}

/// A stretch of text with one set of attributes.
#[derive(Clone, Debug, PartialEq)]
pub struct TextRun {
    pub text: String,
    pub font: String,
    pub size: f64,
    pub color: Rgba,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    /// Horizontal scale, 1 = 100% (`HSCf`).
    pub h_scale: f64,
    /// Range kerning, tenths of the size (`RKNf`).
    pub tracking: f64,
    /// Baseline shift in pixels (`BLSf`).
    pub baseline_shift: f64,
    /// Leading (`LEDf`): a multiple of the size, or pixels if `leading_px`.
    pub leading: f64,
    pub leading_px: bool,
    /// Paragraph alignment (`JSTi`): 0 left, 1 center, 2 right, 3 justify, 4 stretch.
    pub align: u8,
    /// First-line indent and space before and after a paragraph, pixels (`PINf`, `PSBf`, `PSAf`).
    pub indent: f64,
    pub space_before: f64,
    pub space_after: f64,
}

/// How a mask hides its content (Fireworks MX element masks).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskMode {
    /// Modify > Mask > "mask to image", grayscale: the mask's luminance (times its alpha).
    Grayscale,
    /// "mask to image", alpha channel: the mask's alpha.
    Alpha,
    /// "mask to path": the mask path's outline clips; its fill and stroke are ignored.
    PathOutline,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ElementKind {
    Bitmap(Bitmap),
    Path(Path),
    Text(Text),
    Group(Vec<Element>),
    /// An object (or objects) with an element mask. `cache` is Fireworks' pre-rendered mask
    /// bitmap, kept so it is written back as read.
    Masked {
        content: Vec<Element>,
        mask: Box<Element>,
        mode: MaskMode,
        cache: Option<Box<Element>>,
    },
    /// An instance of a symbol from the document's library, placed by `matrix` (symbol space ->
    /// canvas). `bounds` is the placed symbol's bounding box, computed when the file is read.
    Instance {
        symbol: u32,
        name: String,
        matrix: Affine,
        bounds: Option<(f64, f64, f64, f64)>,
    },
    /// Slices, hotspots and anything not yet modelled.
    Other {
        kind: String,
        cached: Option<Bitmap>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub name: Option<String>,
    pub visible: bool,
    /// Locked (Fireworks CS's per-object lock, `LCKb`): can't be selected or edited. Fireworks
    /// MX ignores it.
    pub locked: bool,
    /// Percent, 0..=100 (Fireworks stores tenths of a percent in `OPAi`).
    pub opacity: f32,
    pub blend: BlendMode,
    /// Live Effects, in order (as stored; see `effects::EffectEntry`).
    pub effects: Vec<crate::effects::EffectEntry>,
    pub kind: ElementKind,
    /// Original Fireworks node (effects, textures, gradients...), re-emitted on save.
    pub raw: Option<Node>,
}

impl Element {
    /// The fill of a path or text object (`None` for other kinds; `Some(None)`: no fill).
    pub fn fill_slot(&self) -> Option<Option<&Fill>> {
        match &self.kind {
            ElementKind::Path(p) => Some(p.fill.as_ref()),
            ElementKind::Text(t) => Some(t.fill.as_ref()),
            _ => None,
        }
    }

    /// The stroke of a path or text object (`None` for other kinds; `Some(None)`: no stroke).
    pub fn stroke_slot(&self) -> Option<Option<&Stroke>> {
        match &self.kind {
            ElementKind::Path(p) => Some(p.stroke.as_ref()),
            ElementKind::Text(t) => Some(t.stroke.as_ref()),
            _ => None,
        }
    }

    /// [`stroke_slot`](Self::stroke_slot), to change.
    pub fn stroke_slot_mut(&mut self) -> Option<&mut Option<Stroke>> {
        match &mut self.kind {
            ElementKind::Path(p) => Some(&mut p.stroke),
            ElementKind::Text(t) => Some(&mut t.stroke),
            _ => None,
        }
    }

    /// [`fill_slot`](Self::fill_slot), to change.
    pub fn fill_slot_mut(&mut self) -> Option<&mut Option<Fill>> {
        match &mut self.kind {
            ElementKind::Path(p) => Some(&mut p.fill),
            ElementKind::Text(t) => Some(&mut t.fill),
            _ => None,
        }
    }

    pub fn new(kind: ElementKind) -> Self {
        Self {
            name: None,
            visible: true,
            locked: false,
            opacity: 100.0,
            blend: BlendMode::Normal,
            effects: Vec::new(),
            kind,
            raw: None,
        }
    }

    /// The label the Layers panel shows.
    pub fn label(&self) -> String {
        if let Some(n) = &self.name {
            return n.clone();
        }
        match &self.kind {
            ElementKind::Bitmap(_) => "Bitmap".into(),
            ElementKind::Path(p) if p.rect.is_some() => "Rectangle".into(),
            ElementKind::Path(_) => "Path".into(),
            ElementKind::Text(t) => t.text.chars().take(24).collect(),
            ElementKind::Group(_) => "Group".into(),
            ElementKind::Masked { content, .. } => match content.as_slice() {
                [one] => format!("{} (masked)", one.label()),
                _ => "Masked Group".into(),
            },
            ElementKind::Instance { name, .. } => name.clone(),
            ElementKind::Other { kind, .. } => kind.clone(),
        }
    }

    /// Bounding box (x, y, w, h) in canvas pixels, if known.
    pub fn bounds(&self) -> Option<(f64, f64, f64, f64)> {
        match &self.kind {
            ElementKind::Bitmap(b) => Some((
                b.x as f64,
                b.y as f64,
                b.image.width() as f64,
                b.image.height() as f64,
            )),
            ElementKind::Text(Text { cached: Some(b), .. }) | ElementKind::Other { cached: Some(b), .. } => {
                Some((
                    b.x as f64,
                    b.y as f64,
                    b.image.width() as f64,
                    b.image.height() as f64,
                ))
            }
            ElementKind::Path(p) => {
                let pts: Vec<Pt> = p.contours.iter().flat_map(|c| c.flatten(0.0, 0.0)).collect();
                let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                for (x, y) in pts {
                    x0 = x0.min(x);
                    y0 = y0.min(y);
                    x1 = x1.max(x);
                    y1 = y1.max(y);
                }
                (x0 <= x1).then(|| (x0, y0, x1 - x0, y1 - y0))
            }
            // Only what the mask reveals can show: its bounds bound the result.
            ElementKind::Masked { mask, .. } => mask.bounds(),
            ElementKind::Instance { bounds, .. } => *bounds,
            ElementKind::Group(children) => {
                let bs: Vec<_> = children.iter().filter_map(Element::bounds).collect();
                let x0 = bs.iter().map(|b| b.0).fold(f64::MAX, f64::min);
                let y0 = bs.iter().map(|b| b.1).fold(f64::MAX, f64::min);
                let x1 = bs.iter().map(|b| b.0 + b.2).fold(f64::MIN, f64::max);
                let y1 = bs.iter().map(|b| b.1 + b.3).fold(f64::MIN, f64::max);
                (!bs.is_empty()).then(|| (x0, y0, x1 - x0, y1 - y0))
            }
            _ => None,
        }
    }

    /// Move by (`dx`, `dy`) canvas pixels.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        match &mut self.kind {
            ElementKind::Text(t) => {
                t.shift(dx, dy);
                if let Some(b) = &mut t.cached {
                    b.x = (b.x as f64 + dx).round() as i64;
                    b.y = (b.y as f64 + dy).round() as i64;
                }
            }
            ElementKind::Bitmap(b) => {
                b.x += dx.round() as i64;
                b.y += dy.round() as i64;
            }
            ElementKind::Path(p) => {
                for pt in p.contours.iter_mut().flat_map(|c| c.points.iter_mut()) {
                    pt.x += dx;
                    pt.y += dy;
                }
                for h in p.fill.iter_mut().flat_map(Fill::handles_mut) {
                    h.0 += dx;
                    h.1 += dy;
                }
                if let Some(r) = &mut p.rect {
                    r.matrix = Affine::translate(dx, dy).then(&r.matrix);
                }
                if let Some(pcl) = &mut p.pcl {
                    pcl.map(|x, y| (x + dx, y + dy));
                    pcl.source.clone_from(&p.contours);
                }
                if let Some(o) = &mut p.texture_origin {
                    *o = (o.0 + dx, o.1 + dy);
                }
            }
            ElementKind::Other { cached: Some(b), .. } => {
                b.x += dx.round() as i64;
                b.y += dy.round() as i64;
            }
            ElementKind::Group(children) => children.iter_mut().for_each(|c| c.translate(dx, dy)),
            ElementKind::Masked {
                content, mask, cache, ..
            } => {
                content.iter_mut().for_each(|c| c.translate(dx, dy));
                mask.translate(dx, dy);
                if let Some(c) = cache {
                    c.translate(dx, dy);
                }
            }
            ElementKind::Instance { matrix, bounds, .. } => {
                *matrix = Affine::translate(dx, dy).then(matrix);
                if let Some(b) = bounds {
                    b.0 += dx;
                    b.1 += dy;
                }
            }
            _ => {}
        }
    }

    /// Apply an affine transform to the element's geometry: path anchors and handles, gradient
    /// handles and stroke widths are transformed; bitmaps are resampled.
    pub fn transform(&mut self, m: &Affine) {
        let map_bitmap = |b: &mut Bitmap| {
            if let Some((img, x, y)) = transform_image(&b.image, b.x, b.y, m) {
                *b = Bitmap { x, y, image: img };
            }
        };
        match &mut self.kind {
            ElementKind::Text(t) => {
                if let Some(b) = &mut t.cached {
                    map_bitmap(b);
                }
                // As in Fireworks MX, the block and its formatting stay as they were and the
                // transformation joins the text's matrix.
                match plain_move(m) {
                    Some((dx, dy)) => t.shift(dx, dy),
                    // Text on a path follows the path.
                    None if t.path.is_some() => {
                        for h in t.fill.iter_mut().flat_map(Fill::handles_mut) {
                            *h = m.apply(h.0, h.1);
                        }
                        if let Some(tp) = &mut t.path {
                            tp.path.transform(m);
                        }
                        let (l, top, r, b) = t.rect;
                        let (x, y, w, h) = m.map_rect(l, top, r - l, b - top);
                        t.rect = (x, y, x + w, y + h);
                    }
                    None => {
                        for h in t.fill.iter_mut().flat_map(Fill::handles_mut) {
                            *h = m.apply(h.0, h.1);
                        }
                        t.matrix = m.then(&t.matrix);
                    }
                }
            }
            ElementKind::Bitmap(b) => map_bitmap(b),
            ElementKind::Path(p) => {
                for pt in p.contours.iter_mut().flat_map(|c| c.points.iter_mut()) {
                    let (ax, ay) = m.apply(pt.x, pt.y);
                    let pr = m.apply(pt.x + pt.pred.0, pt.y + pt.pred.1);
                    let su = m.apply(pt.x + pt.succ.0, pt.y + pt.succ.1);
                    *pt = BezierPoint {
                        x: ax,
                        y: ay,
                        pred: (pr.0 - ax, pr.1 - ay),
                        succ: (su.0 - ax, su.1 - ay),
                    };
                }
                for h in p.fill.iter_mut().flat_map(Fill::handles_mut) {
                    *h = m.apply(h.0, h.1);
                }
                if let Some(s) = &mut p.stroke {
                    s.diameter = (s.diameter * m.scale()).max(0.1);
                }
                // A rectangle primitive stays one under transforms, perspective included (as in
                // Fireworks: rotated, skewed or distorted rectangles remain rectangles).
                if let Some(r) = &mut p.rect {
                    r.matrix = m.then(&r.matrix);
                }
                // Fireworks transforms its stored polyline with the path (keeping pen data).
                if let Some(pcl) = &mut p.pcl {
                    pcl.map(|x, y| m.apply(x, y));
                    pcl.source.clone_from(&p.contours);
                }
                if let Some(o) = &mut p.texture_origin {
                    *o = m.apply(o.0, o.1);
                }
            }
            ElementKind::Other { cached: Some(b), .. } => map_bitmap(b),
            ElementKind::Group(children) => children.iter_mut().for_each(|c| c.transform(m)),
            ElementKind::Masked {
                content, mask, cache, ..
            } => {
                content.iter_mut().for_each(|c| c.transform(m));
                mask.transform(m);
                if let Some(c) = cache {
                    c.transform(m);
                }
            }
            ElementKind::Instance { matrix, bounds, .. } => {
                *matrix = m.then(matrix);
                if let Some(b) = bounds {
                    *b = m.map_rect(b.0, b.1, b.2, b.3);
                }
            }
            _ => {}
        }
    }

    /// The objects an instance shows in the current frame, placed on the canvas.
    /// A 9-slice symbol's instance: the slices at the instance's stretch along the symbol's own
    /// width and height (the mean length of opposite sides over the symbol's size; for an affine
    /// transform, its transformed axes' lengths), and the rest of the transform (rotation, skew,
    /// flip, perspective) to apply after. So the slices survive every transform (Fireworks CS
    /// asks to drop them instead).
    fn nine_split(&self, ctx: &RenderCtx) -> Option<(AxisMap, AxisMap, Affine)> {
        let ElementKind::Instance { symbol, matrix, .. } = &self.kind else {
            return None;
        };
        let sym = ctx.symbols.iter().find(|s| s.id == *symbol)?;
        let (gl, gr, gt, gb) = sym.nine?;
        let (x, y, w, h) = sym.content_bounds()?;
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        let (x1, y1) = (x + w, y + h);
        let q = [
            matrix.apply(x, y),
            matrix.apply(x1, y),
            matrix.apply(x1, y1),
            matrix.apply(x, y1),
        ];
        let len = |a: (f64, f64), b: (f64, f64)| ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
        let sx = (len(q[0], q[1]) + len(q[3], q[2])) / 2.0 / w;
        let sy = (len(q[0], q[3]) + len(q[1], q[2])) / 2.0 / h;
        if !(sx > 1e-9 && sy > 1e-9 && sx.is_finite() && sy.is_finite()) {
            return None;
        }
        let mx = AxisMap::new(x, x1, gl.clamp(x, x1), gr.clamp(x, x1), x * sx, x1 * sx);
        let my = AxisMap::new(y, y1, gt.clamp(y, y1), gb.clamp(y, y1), y * sy, y1 * sy);
        let unstretch = Affine {
            a: 1.0 / sx,
            d: 1.0 / sy,
            ..Affine::IDENTITY
        };
        Some((mx, my, matrix.then(&unstretch)))
    }

    /// In Fireworks MX mode a perspective instance's contents are mapped one by one (paths and
    /// fill handles) with their effects applied afterwards at canvas size, as Fireworks MX draws
    /// them. 9-slice guides aren't a Fireworks MX feature and keep the warp.
    fn mapped_in_perspective(&self, ctx: &RenderCtx) -> bool {
        ctx.mx_perspective && self.nine_split(ctx).is_none()
    }

    fn instance_elements(&self, ctx: &RenderCtx) -> Option<Vec<Element>> {
        let ElementKind::Instance { matrix, .. } = &self.kind else {
            return None;
        };
        let mut out = self.symbol_elements(ctx)?;
        if let Some((mx, my, rest)) = self.nine_split(ctx) {
            let plain = rest.is_affine()
                && (rest.a - 1.0).abs() < 1e-9
                && rest.b.abs() < 1e-9
                && rest.c.abs() < 1e-9
                && (rest.d - 1.0).abs() < 1e-9;
            out.iter_mut().for_each(|e| {
                e.map_nine(&mx, &my);
                if plain {
                    e.translate(rest.e / rest.i, rest.f / rest.i);
                } else {
                    e.transform(&rest);
                }
            });
            return Some(out);
        }
        out.iter_mut().for_each(|e| map_instance_child(e, matrix));
        Some(out)
    }

    /// Bounds including symbol instances whose bounds weren't known when the file was read
    /// (instances nested inside symbols): computed from the symbol's contents.
    pub fn deep_bounds(&self, ctx: &RenderCtx) -> Option<(f64, f64, f64, f64)> {
        if ctx.depth >= MAX_SYMBOL_DEPTH {
            return None;
        }
        let union = union_rects;
        match &self.kind {
            ElementKind::Instance {
                matrix, bounds: None, ..
            } => {
                let inner = ctx.deeper(self);
                let kids = self.symbol_elements(ctx)?;
                let (x, y, w, h) = union(&mut kids.iter().filter_map(|c| c.deep_bounds(&inner)))?;
                Some(matrix.map_rect(x, y, w, h))
            }
            ElementKind::Group(children) => union(&mut children.iter().filter_map(|c| c.deep_bounds(ctx))),
            _ => self.bounds(),
        }
    }

    /// The objects an instance's symbol shows in the current frame, in symbol space.
    pub(crate) fn symbol_elements(&self, ctx: &RenderCtx) -> Option<Vec<Element>> {
        let ElementKind::Instance { symbol, .. } = &self.kind else {
            return None;
        };
        if ctx.depth >= MAX_SYMBOL_DEPTH || ctx.inside_symbol(*symbol) {
            return None;
        }
        let sym = ctx.symbols.iter().find(|s| s.id == *symbol)?;
        // Buttons show their Up state; animations play along with the document's frames.
        let frame = if sym.kind == crate::SymbolKind::Button {
            0
        } else {
            ctx.frame % sym.frames.max(1)
        };
        let mut out = Vec::new();
        for layer in sym.layers.iter().filter(|l| l.visible_in(frame) && !l.web) {
            out.extend(layer.elements(frame).iter().cloned());
        }
        Some(out)
    }

    /// Apply a 9-slice mapping (see `firecraft_raster::nine`) to the element's geometry. Bitmaps
    /// are resampled; path anchors and control handles are mapped (handles as absolute points,
    /// so curves keep their shape in the fixed corners).
    pub fn map_nine(&mut self, mx: &AxisMap, my: &AxisMap) {
        let map_bitmap = |b: &mut Bitmap| {
            let (x0, y0) = (mx.apply(b.x as f64).round(), my.apply(b.y as f64).round());
            let x1 = mx.apply((b.x + b.image.width() as i64) as f64).round();
            let y1 = my.apply((b.y + b.image.height() as i64) as f64).round();
            let size = ((x1 - x0).max(1.0) as u32, (y1 - y0).max(1.0) as u32);
            b.image = resample(
                &b.image,
                (b.x as f64, b.y as f64),
                mx,
                my,
                (x0 as i64, y0 as i64),
                size,
            );
            b.x = x0 as i64;
            b.y = y0 as i64;
        };
        match &mut self.kind {
            ElementKind::Bitmap(b) => map_bitmap(b),
            ElementKind::Path(p) => {
                p.rect = None;
                p.pcl = None;
                for pt in p.contours.iter_mut().flat_map(|c| c.points.iter_mut()) {
                    let (ax, ay) = (mx.apply(pt.x), my.apply(pt.y));
                    let pred = (mx.apply(pt.x + pt.pred.0) - ax, my.apply(pt.y + pt.pred.1) - ay);
                    let succ = (mx.apply(pt.x + pt.succ.0) - ax, my.apply(pt.y + pt.succ.1) - ay);
                    *pt = BezierPoint {
                        x: ax,
                        y: ay,
                        pred,
                        succ,
                    };
                }
                for h in p.fill.iter_mut().flat_map(Fill::handles_mut) {
                    *h = (mx.apply(h.0), my.apply(h.1));
                }
            }
            ElementKind::Text(Text { cached: Some(b), .. }) | ElementKind::Other { cached: Some(b), .. } => {
                map_bitmap(b)
            }
            ElementKind::Group(children) => children.iter_mut().for_each(|c| c.map_nine(mx, my)),
            ElementKind::Masked {
                content, mask, cache, ..
            } => {
                content.iter_mut().for_each(|c| c.map_nine(mx, my));
                mask.map_nine(mx, my);
                if let Some(c) = cache {
                    c.map_nine(mx, my);
                }
            }
            _ => {}
        }
    }

    /// The element's own pixels before opacity, blend mode and effects, as an image and its
    /// position in `out` coordinates (`out`'s top-left is canvas (`ox`, `oy`), size `w` x `h`).
    pub fn rasterize(
        &self,
        ox: i64,
        oy: i64,
        w: u32,
        h: u32,
        ctx: &RenderCtx,
    ) -> Option<(RgbaImage, i64, i64)> {
        match &self.kind {
            ElementKind::Bitmap(b) => Some((b.image.clone(), b.x - ox, b.y - oy)),
            ElementKind::Text(Text { cached: Some(b), .. }) | ElementKind::Other { cached: Some(b), .. } => {
                Some((b.image.clone(), b.x - ox, b.y - oy))
            }
            ElementKind::Path(p) => {
                // Render only the path's bounds (plus stroke), clipped to the output.
                let (bx, by, bw, bh) = self.bounds()?;
                let feather = p.fill.as_ref().map_or(0.0, |f| f.feather as f64 + 2.0);
                let pad = p
                    .stroke
                    .as_ref()
                    .map_or(2.0, crate::stroke_fw::margin)
                    .max(feather);
                let x0 = ((bx - pad).floor() as i64 - ox).max(0);
                let y0 = ((by - pad).floor() as i64 - oy).max(0);
                let x1 = ((bx + bw + pad).ceil() as i64 - ox).min(w as i64);
                let y1 = ((by + bh + pad).ceil() as i64 - oy).min(h as i64);
                if x1 <= x0 || y1 <= y0 {
                    return None;
                }
                let mut layer = RgbaImage::new((x1 - x0) as u32, (y1 - y0) as u32, Rgba::TRANSPARENT);
                let (fx, fy) = (-((ox + x0) as f64), -((oy + y0) as f64));
                // "Draw fill over stroke" (`FOTb`): the fill goes on a layer of its own, laid over
                // the stroke afterwards.
                let fill_over = p.stroke.is_some()
                    && p.fill.is_some()
                    && self.raw.as_ref().and_then(|r| r.bool_of("FOT")).unwrap_or(false);
                let mut stroke_layer = None;
                if fill_over {
                    let mut st = RgbaImage::new(layer.width(), layer.height(), Rgba::TRANSPARENT);
                    if let Some(s) = &p.stroke {
                        crate::stroke_fw::draw(&mut st, p, s, self.raw.as_ref(), (fx, fy));
                    }
                    stroke_layer = Some(st);
                }
                if let Some(f) = &p.fill {
                    let polys: Vec<Vec<Pt>> = p.contours.iter().map(|c| c.flatten(fx, fy)).collect();
                    // Shade in canvas coordinates (the layer's pixel + its canvas offset).
                    // Fireworks samples gradients at the pixel's integer coordinate (its
                    // top-left corner), not its centre (measured on Folds/Pinch/Waves).
                    let shade = |x: f64, y: f64| {
                        let (cx, cy) = (x - 0.5 - fx, y - 0.5 - fy);
                        match (&f.gradient, &f.pattern, &f.dither) {
                            (Some(g), _, _) => g.color_at(cx, cy),
                            (_, Some(pattern), _) => pattern.color_at(cx, cy),
                            (_, _, Some(tile)) => {
                                let c = web_dither_at(tile, cx, cy);
                                if f.dither_transparent && (cx.round() as i64 + cy.round() as i64) % 2 != 0 {
                                    Rgba { a: 0, ..c }
                                } else {
                                    c
                                }
                            }
                            _ => f.color,
                        }
                    };
                    if f.feather > 0 {
                        fill_polygons_feathered(&mut layer, &polys, f.feather as usize, &shade);
                    } else {
                        fill_polygons_with(&mut layer, &polys, f.anti_alias, &shade);
                    }
                    let origin = if ctx.canvas_textures {
                        (0.0, 0.0)
                    } else {
                        crate::stroke_fw::texture_origin(p)
                    };
                    apply_fill_texture(&mut layer, f, origin, (ox + x0, oy + y0));
                }
                match stroke_layer {
                    Some(mut st) => {
                        for (d, f) in st.pixels_mut().iter_mut().zip(layer.pixels()) {
                            *d = d.over(*f, 255);
                        }
                        layer = st;
                    }
                    None => {
                        if let Some(s) = &p.stroke {
                            crate::stroke_fw::draw(&mut layer, p, s, self.raw.as_ref(), (fx, fy));
                        }
                    }
                }
                Some((layer, x0, y0))
            }
            ElementKind::Group(children) => {
                let mut layer = RgbaImage::new(w, h, Rgba::TRANSPARENT);
                children
                    .iter()
                    .for_each(|c| c.render_onto(&mut layer, ox, oy, ctx));
                Some((layer, 0, 0))
            }
            ElementKind::Instance { matrix, .. }
                if !matrix.is_affine() && !self.mapped_in_perspective(ctx) =>
            {
                // Perspective: render the symbol flat, then warp its pixels (exact for fills and
                // effects, unlike mapping the geometry, which is the Fireworks MX preference). A 9-slice symbol is rendered sliced at the
                // instance's stretch, then warped by the rest of the transform.
                let mut children = self.symbol_elements(ctx)?;
                let nine = self.nine_split(ctx);
                if let Some((mx, my, _)) = &nine {
                    children.iter_mut().for_each(|c| c.map_nine(mx, my));
                }
                let matrix = nine.map_or(*matrix, |(_, _, rest)| rest);
                let matrix = &matrix;
                let inner = ctx.deeper(self);
                // What the contents reach, effects and strokes included.
                let extent = |children: &[Element]| {
                    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                    for (rx, ry, rw, rh) in children.iter().filter_map(|c| c.full_region(&inner)) {
                        x0 = x0.min(rx as f64);
                        y0 = y0.min(ry as f64);
                        x1 = x1.max((rx + i64::from(rw)) as f64);
                        y1 = y1.max((ry + i64::from(rh)) as f64);
                    }
                    (x0 <= x1).then_some((x0, y0, x1, y1))
                };
                let (x0, y0, x1, y1) = extent(&children)?;
                // Vector contents are drawn at the size they're seen, so an enlarged symbol
                // resolves detail instead of magnifying its pixels.
                let detail = if nine.is_none() && children.iter().all(redraws_at_any_scale) {
                    warp_detail(matrix, (x0, y0, x1, y1))
                } else {
                    1.0
                };
                let key = crate::cache::contents_key(&children, &inner, detail);
                let crate::cache::Kept {
                    image: flat,
                    x: fx,
                    y: fy,
                } = crate::cache::warp_source(key, || {
                    let mut children = children.clone();
                    if detail > 1.0 {
                        let up = Affine {
                            a: detail,
                            d: detail,
                            ..Affine::IDENTITY
                        };
                        children.iter_mut().for_each(|c| {
                            c.transform(&up);
                            for x in &mut c.effects {
                                x.effect = x.effect.as_ref().and_then(|e| e.scaled(detail as f32));
                            }
                        });
                    }
                    let (x0, y0, x1, y1) = extent(&children)?;
                    let (fx, fy) = (x0.floor() as i64 - 2, y0.floor() as i64 - 2);
                    let (fw, fh) = (
                        (x1.ceil() as i64 + 2 - fx).max(1),
                        (y1.ceil() as i64 + 2 - fy).max(1),
                    );
                    if fw * fh > 1 << 26 {
                        return None;
                    }
                    let mut flat = RgbaImage::new(fw as u32, fh as u32, Rgba::TRANSPARENT);
                    children
                        .iter()
                        .for_each(|c| c.render_onto(&mut flat, fx, fy, &inner));
                    Some(crate::cache::Kept {
                        image: std::sync::Arc::new(flat),
                        x: fx,
                        y: fy,
                    })
                })?;
                // Inverse-map each output pixel; only points in front of the horizon (w > 0 on the
                // same side as the matrix's own w) are visible.
                let inv = matrix.invert()?;
                let side = matrix.i.signum();
                let texel = |u: f64, v: f64| {
                    let (sx, sy) = inv.apply(u, v);
                    (sx * detail - fx as f64 - 0.5, sy * detail - fy as f64 - 0.5)
                };
                let mut layer = RgbaImage::new(w, h, Rgba::TRANSPARENT);
                for ty in 0..h as i64 {
                    for tx in 0..w as i64 {
                        let (u, v) = ((ox + tx) as f64 + 0.5, (oy + ty) as f64 + 0.5);
                        let (sx, sy) = inv.apply(u, v);
                        if (matrix.g * sx + matrix.h * sy + matrix.i) * side <= 0.0 {
                            continue;
                        }
                        let p = if detail > 1.0 {
                            sample_footprint(flat.as_ref(), u, v, &texel)
                        } else {
                            let (px, py) = texel(u, v);
                            sample(flat.as_ref(), px, py)
                        };
                        if p.a != 0 {
                            layer.set(tx, ty, p);
                        }
                    }
                }
                Some((layer, 0, 0))
            }
            ElementKind::Instance { matrix, .. } => {
                let children = self.instance_elements(ctx)?;
                let mut inner = ctx.deeper(self);
                // Only perspective instances mapped the Fireworks MX way reach here unwarped.
                inner.canvas_textures |= !matrix.is_affine();
                let mut layer = RgbaImage::new(w, h, Rgba::TRANSPARENT);
                children
                    .iter()
                    .for_each(|c| c.render_onto(&mut layer, ox, oy, &inner));
                Some((layer, 0, 0))
            }
            ElementKind::Masked {
                content, mask, mode, ..
            } => {
                let mut layer = RgbaImage::new(w, h, Rgba::TRANSPARENT);
                content
                    .iter()
                    .for_each(|c| c.render_onto(&mut layer, ox, oy, ctx));
                let coverage = mask_coverage(mask, *mode, ox, oy, w, h, ctx);
                for (p, &m) in layer.pixels_mut().iter_mut().zip(coverage.pixels()) {
                    p.a = (p.a as u32 * m.a as u32 / 255) as u8;
                }
                Some((layer, 0, 0))
            }
            _ => None,
        }
    }

    /// Composite this element onto `out` (whose top-left is canvas (`ox`, `oy`)).
    /// Whether drawing passes straight through to the children (a group or symbol instance
    /// with no effects, full opacity and Normal blending): each child blends with what lies
    /// beneath on its own.
    pub fn is_pass_through(&self) -> bool {
        let plain = self.opacity >= 100.0
            && self.blend == BlendMode::Normal
            && self.effects.iter().all(|e| e.active().is_none());
        plain
            && match &self.kind {
                ElementKind::Group(_) => true,
                ElementKind::Instance { matrix, .. } => matrix.is_affine(),
                _ => false,
            }
    }

    /// The children a pass-through element draws (see [`is_pass_through`](Self::is_pass_through)).
    pub fn pass_through_children(&self, ctx: &RenderCtx) -> Vec<Element> {
        match &self.kind {
            ElementKind::Group(children) => children.clone(),
            ElementKind::Instance { .. } => self.instance_elements(ctx).unwrap_or_default(),
            _ => Vec::new(),
        }
    }

    /// The opacity byte used when blending.
    pub fn opacity_byte(&self) -> u8 {
        (self.opacity.clamp(0.0, 100.0) as f64 * 255.0 / 100.0).round() as u8
    }

    /// The object's own pixels after its effects, before opacity and blending, as an image and
    /// its position relative to the output area's top-left (canvas (`ox`, `oy`), `w` x `h`).
    pub fn pixels(&self, ox: i64, oy: i64, w: u32, h: u32, ctx: &RenderCtx) -> Option<(RgbaImage, i64, i64)> {
        let active: Vec<&Effect> = self.effects.iter().filter_map(|e| e.active()).collect();
        let baked = matches!(&self.kind, ElementKind::Text(t) if t.baked_effects && !t.edited);
        if active.is_empty() || baked {
            return self.rasterize(ox, oy, w, h, ctx);
        }
        // Effects can draw outside the object (shadows, glows, blurs) and are affected by the
        // object's pixels beyond the output's edge (a blur near the canvas edge): rasterize the
        // output area grown by the effect margin, and give the object that margin of padding.
        let m = active.iter().map(|e| e.margin()).max().unwrap_or(0) as i64;
        let (ew, eh) = (w as i64 + 2 * m, h as i64 + 2 * m);
        if ew * eh > 1 << 28 {
            return None;
        }
        let (img, x, y) = self.rasterize(ox - m, oy - m, ew as u32, eh as u32, ctx)?;
        let (bx, by) = ((x - m).max(0), (y - m).max(0));
        let bw = ((x + img.width() as i64 + m).min(ew) - bx).max(0) as u32;
        let bh = ((y + img.height() as i64 + m).min(eh) - by).max(0) as u32;
        if bw == 0 || bh == 0 {
            return None;
        }
        let mut buf = RgbaImage::new(bw, bh, Rgba::TRANSPARENT);
        buf.draw(&img, x - bx, y - by, 255);
        for e in &active {
            e.apply_with(&mut buf, ctx.math);
        }
        Some((buf, bx - m, by - m))
    }

    /// How far the object's effects reach beyond it.
    pub fn effect_margin(&self) -> i64 {
        self.effects
            .iter()
            .filter_map(|e| e.active())
            .map(|e| e.margin())
            .max()
            .unwrap_or(0) as i64
    }

    /// The area to render the whole object in: its bounds grown by its stroke and effects,
    /// as (x, y, w, h) in canvas pixels. `None` when unknown or huge (then only the visible
    /// part is rendered).
    pub fn full_region(&self, ctx: &RenderCtx) -> Option<(i64, i64, u32, u32)> {
        const MAX_PIXELS: i64 = 40_000_000;
        let (bx, by, bw, bh) = self.deep_bounds(ctx)?;
        if !(bx.is_finite() && by.is_finite() && bw.is_finite() && bh.is_finite()) {
            return None;
        }
        if let Some(reach) = self.perspective_reach(ctx) {
            return Some(reach);
        }
        let stroke = self.max_stroke().ceil() as i64;
        let m = self.effect_margin() + stroke + 2;
        let (x, y) = (bx.floor() as i64 - m, by.floor() as i64 - m);
        let (w, h) = ((bx + bw).ceil() as i64 + m - x, (by + bh).ceil() as i64 + m - y);
        (w > 0 && h > 0 && w * h <= MAX_PIXELS).then_some((x, y, w as u32, h as u32))
    }

    /// The region of a perspective instance whose contents have strokes or effects, which reach
    /// past the bounds stored for it. Warped reach is mapped through the instance's matrix.
    fn perspective_reach(&self, ctx: &RenderCtx) -> Option<(i64, i64, u32, u32)> {
        const MAX_PIXELS: i64 = 40_000_000;
        let ElementKind::Instance { matrix, .. } = &self.kind else {
            return None;
        };
        if matrix.is_affine() || !self.effects.is_empty() {
            return None;
        }
        let children = self.symbol_elements(ctx)?;
        if children
            .iter()
            .all(|c| c.effect_margin() == 0 && c.max_stroke() == 0.0)
        {
            return None;
        }
        let inner = ctx.deeper(self);
        let (bx, by, bw, bh) = self.deep_bounds(ctx)?;
        let (mut x0, mut y0, mut x1, mut y1) = (bx, by, bx + bw, by + bh);
        if self.mapped_in_perspective(ctx) {
            for c in &children {
                let mut moved = c.clone();
                map_instance_child(&mut moved, matrix);
                if let Some((rx, ry, rw, rh)) = moved.full_region(&inner) {
                    x0 = x0.min(rx as f64);
                    y0 = y0.min(ry as f64);
                    x1 = x1.max((rx + i64::from(rw)) as f64);
                    y1 = y1.max((ry + i64::from(rh)) as f64);
                }
            }
        } else {
            // Warped: what reaches past the contents is enlarged as much as they are. (Mapping the
            // reach itself isn't safe: it can land behind the perspective's horizon.)
            let reach = children
                .iter()
                .map(|c| c.effect_margin() as f64 + c.max_stroke())
                .fold(0.0, f64::max);
            let content = children
                .iter()
                .filter_map(|c| c.deep_bounds(&inner))
                .map(|(x, y, w, h)| (x, y, x + w, y + h))
                .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))?;
            let grow = reach * max_stretch(matrix, content).min(64.0);
            (x0, y0, x1, y1) = (x0 - grow, y0 - grow, x1 + grow, y1 + grow);
        }
        if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) || x0 > x1 {
            return None;
        }
        let (x, y) = (x0.floor() as i64 - 2, y0.floor() as i64 - 2);
        let (w, h) = (x1.ceil() as i64 + 2 - x, y1.ceil() as i64 + 2 - y);
        (w > 0 && h > 0 && w * h <= MAX_PIXELS).then_some((x, y, w as u32, h as u32))
    }

    /// Where the object can have pixels: its bounds grown by how far its stroke and effects
    /// reach (conservative; for hit testing).
    pub fn visual_bounds(&self, ctx: &RenderCtx) -> Option<(f64, f64, f64, f64)> {
        let (bx, by, bw, bh) = self.contents_visual_bounds(ctx)?;
        let m = self.effect_margin() as f64 + 1.0;
        Some((bx - m, by - m, bw + 2.0 * m, bh + 2.0 * m))
    }

    /// Where the object's own drawing reaches, before its own effects: the strokes, and for a
    /// group or symbol instance its members' strokes and effects too (folds7's glow and stroke
    /// inside a symbol reach far beyond its outline). An instance draws its members either moved
    /// onto the canvas (strokes and effects at canvas size) or drawn in the symbol and the
    /// picture transformed, so both reaches count.
    fn contents_visual_bounds(&self, ctx: &RenderCtx) -> Option<(f64, f64, f64, f64)> {
        match &self.kind {
            ElementKind::Instance { matrix, .. } if ctx.depth < MAX_SYMBOL_DEPTH => {
                let inner = ctx.deeper(self);
                let reach = |kids: Option<Vec<Element>>| {
                    kids.and_then(|k| union_rects(&mut k.iter().filter_map(|c| c.visual_bounds(&inner))))
                };
                let drawn_in_symbol = reach(self.symbol_elements(ctx)).map(|(x, y, w, h)| matrix.map_rect(x, y, w, h));
                let on_canvas = if matrix.is_affine() {
                    reach(self.instance_elements(ctx))
                } else {
                    None
                };
                union_rects(&mut drawn_in_symbol.into_iter().chain(on_canvas))
                    .or_else(|| self.deep_bounds(ctx))
            }
            ElementKind::Group(children) => {
                union_rects(&mut children.iter().filter_map(|c| c.visual_bounds(ctx)))
            }
            _ => {
                let (bx, by, bw, bh) = self.deep_bounds(ctx)?;
                let m = self.max_stroke();
                Some((bx - m, by - m, bw + 2.0 * m, bh + 2.0 * m))
            }
        }
    }

    /// Half the widest stroke in the object (strokes reach beyond the geometry's bounds).
    fn max_stroke(&self) -> f64 {
        match &self.kind {
            ElementKind::Path(p) => p
                .stroke
                .as_ref()
                .map_or(0.0, crate::stroke_fw::margin)
                .max(p.fill.as_ref().map_or(0.0, |f| f.feather as f64 + 1.0)),
            ElementKind::Group(children) => children.iter().map(Element::max_stroke).fold(0.0, f64::max),
            ElementKind::Masked { content, mask, .. } => content
                .iter()
                .map(Element::max_stroke)
                .fold(mask.max_stroke(), f64::max),
            _ => 0.0,
        }
    }

    /// Composite this element onto `out` (whose top-left is canvas (`ox`, `oy`)).
    pub fn render_onto(&self, out: &mut RgbaImage, ox: i64, oy: i64, ctx: &RenderCtx) {
        if !self.visible || self.opacity <= 0.0 {
            return;
        }
        if self.is_pass_through() {
            let inner = if matches!(self.kind, ElementKind::Instance { .. }) {
                ctx.deeper(self)
            } else {
                *ctx
            };
            for c in self.pass_through_children(ctx) {
                c.render_onto(out, ox, oy, &inner);
            }
            return;
        }
        // Render the whole object (as Fireworks does: effects see all of it, not just the part
        // on the canvas), falling back to the visible part for huge or unbounded objects.
        let rendered = match self.full_region(ctx) {
            Some((rx, ry, rw, rh)) => self
                .pixels(rx, ry, rw, rh, ctx)
                .map(|(img, x, y)| (img, rx + x - ox, ry + y - oy)),
            None => self.pixels(ox, oy, out.width(), out.height(), ctx),
        };
        if let Some((img, x, y)) = rendered {
            out.draw_blend_with(&img, x, y, self.opacity_byte(), self.blend, ctx.math);
        }
    }
}

/// Per-pixel mask strength (in the alpha channel) of `mask` over the output area.
fn mask_coverage(
    mask: &Element,
    mode: MaskMode,
    ox: i64,
    oy: i64,
    w: u32,
    h: u32,
    ctx: &RenderCtx,
) -> RgbaImage {
    let mut m = RgbaImage::new(w, h, Rgba::TRANSPARENT);
    match mode {
        MaskMode::PathOutline => {
            // The outline alone: render the shape as an opaque fill.
            let mut shape = mask.clone();
            shape.opacity = 100.0;
            shape.blend = BlendMode::Normal;
            shape.visible = true;
            if let ElementKind::Path(p) = &mut shape.kind {
                p.fill = Some(Fill {
                    color: Rgba::WHITE,
                    category: "fc_Solid".into(),
                    anti_alias: true,
                    feather: 0,
                    pattern: None,
                    dither: None,
                    dither_transparent: false,
                    gradient: None,
                    texture: None,
                    texture_amount: 0,
                    texture_transparent: false,
                });
                p.stroke = None;
            }
            shape.render_onto(&mut m, ox, oy, ctx);
        }
        MaskMode::Grayscale | MaskMode::Alpha => {
            let mut shown = mask.clone();
            shown.visible = true;
            shown.render_onto(&mut m, ox, oy, ctx);
            if mode == MaskMode::Grayscale {
                for p in m.pixels_mut() {
                    // Fireworks' luma (Rec. 709 weights x 512), times the mask's own alpha.
                    let luma = (p.r as u32 * 109 + p.g as u32 * 366 + p.b as u32 * 37) >> 9;
                    p.a = (luma * p.a as u32 / 255) as u8;
                }
            }
        }
    }
    m
}

/// Tile ids (`TIDi`) the element's original Fireworks data refers to (textures, cached text
/// images...), including its children's.
pub fn tile_refs(e: &Element, out: &mut Vec<u32>) {
    fn walk(n: &Node, out: &mut Vec<u32>, depth: usize) {
        if n.is("TID") {
            out.extend(n.as_u32());
        }
        if depth < firecraft_fwpng::tree::MAX_DEPTH {
            n.children.iter().for_each(|c| walk(c, out, depth + 1));
        }
    }
    if let Some(r) = &e.raw {
        walk(r, out, 0);
    }
    for_children(e, &mut |c| tile_refs(c, out));
}

/// Renumber tile references in the element's original Fireworks data (and its children's).
pub fn remap_tiles(e: &mut Element, map: &std::collections::HashMap<u32, u32>) {
    fn walk(n: &mut Node, map: &std::collections::HashMap<u32, u32>, depth: usize) {
        if n.is("TID")
            && let Some(to) = n.as_u32().and_then(|id| map.get(&id))
        {
            *n = Node::int("TID", i64::from(*to));
        }
        if depth < firecraft_fwpng::tree::MAX_DEPTH {
            n.children.iter_mut().for_each(|c| walk(c, map, depth + 1));
        }
    }
    if let Some(r) = &mut e.raw {
        walk(r, map, 0);
    }
    for_children_mut(e, &mut |c| remap_tiles(c, map));
}

fn for_children(e: &Element, f: &mut dyn FnMut(&Element)) {
    match &e.kind {
        ElementKind::Group(children) => children.iter().for_each(f),
        ElementKind::Masked {
            content, mask, cache, ..
        } => {
            content.iter().for_each(&mut *f);
            f(mask);
            if let Some(c) = cache {
                f(c);
            }
        }
        _ => {}
    }
}

fn for_children_mut(e: &mut Element, f: &mut dyn FnMut(&mut Element)) {
    match &mut e.kind {
        ElementKind::Group(children) => children.iter_mut().for_each(f),
        ElementKind::Masked {
            content, mask, cache, ..
        } => {
            content.iter_mut().for_each(&mut *f);
            f(mask);
            if let Some(c) = cache {
                f(c);
            }
        }
        _ => {}
    }
}

/// The smallest rectangle (x, y, w, h) holding all of `items`; `None` when there are none.
fn union_rects(items: &mut dyn Iterator<Item = (f64, f64, f64, f64)>) -> Option<(f64, f64, f64, f64)> {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for (bx, by, bw, bh) in items {
        x0 = x0.min(bx);
        y0 = y0.min(by);
        x1 = x1.max(bx + bw);
        y1 = y1.max(by + bh);
    }
    (x0 <= x1).then_some((x0, y0, x1 - x0, y1 - y0))
}
