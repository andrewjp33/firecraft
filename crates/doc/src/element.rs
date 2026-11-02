//! Document elements (Fireworks "objects").

use firecraft_fwpng::Node;
use firecraft_raster::effects::Effect;
use firecraft_raster::gradient::Gradient;
use firecraft_raster::nine::{AxisMap, resample};
use firecraft_raster::path::{Pt, fill_polygons, fill_polygons_with, flatten_cubic, stamp_stroke};
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
    /// Gradient fills (handles in canvas coordinates); `None` for solid fills.
    pub gradient: Option<Gradient>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub color: Rgba,
    /// Brush category and name, e.g. "bc_Pencil" / "bn_1-Pixel Anti-Aliased".
    pub category: String,
    pub name: String,
    pub diameter: f64,
    pub softness: u8,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Path {
    pub contours: Vec<Contour>,
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
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
            ElementKind::Path(p) if p.contours.iter().all(|c| c.closed) => "Path".into(),
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
            ElementKind::Bitmap(b) => {
                b.x += dx.round() as i64;
                b.y += dy.round() as i64;
            }
            ElementKind::Path(p) => {
                for pt in p.contours.iter_mut().flat_map(|c| c.points.iter_mut()) {
                    pt.x += dx;
                    pt.y += dy;
                }
                if let Some(g) = p.fill.as_mut().and_then(|f| f.gradient.as_mut()) {
                    g.translate(dx, dy);
                }
            }
            ElementKind::Text(Text { cached: Some(b), .. }) | ElementKind::Other { cached: Some(b), .. } => {
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
                if let Some(g) = p.fill.as_mut().and_then(|f| f.gradient.as_mut()) {
                    for h in [&mut g.start, &mut g.end, &mut g.side] {
                        *h = m.apply(h.0, h.1);
                    }
                }
                if let Some(s) = &mut p.stroke {
                    s.diameter = (s.diameter * m.scale()).max(0.1);
                }
            }
            ElementKind::Text(Text { cached: Some(b), .. }) | ElementKind::Other { cached: Some(b), .. } => {
                map_bitmap(b)
            }
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
        out.iter_mut().for_each(|e| e.transform(matrix));
        Some(out)
    }

    /// Bounds including symbol instances whose bounds weren't known when the file was read
    /// (instances nested inside symbols): computed from the symbol's contents.
    pub fn deep_bounds(&self, ctx: &RenderCtx) -> Option<(f64, f64, f64, f64)> {
        if ctx.depth >= MAX_SYMBOL_DEPTH {
            return None;
        }
        let union = |items: &mut dyn Iterator<Item = (f64, f64, f64, f64)>| {
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for (bx, by, bw, bh) in items {
                x0 = x0.min(bx);
                y0 = y0.min(by);
                x1 = x1.max(bx + bw);
                y1 = y1.max(by + bh);
            }
            (x0 <= x1).then(|| (x0, y0, x1 - x0, y1 - y0))
        };
        match &self.kind {
            ElementKind::Instance {
                matrix, bounds: None, ..
            } => {
                let inner = ctx.deeper();
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
        if ctx.depth >= MAX_SYMBOL_DEPTH {
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
                if let Some(g) = p.fill.as_mut().and_then(|f| f.gradient.as_mut()) {
                    for h in [&mut g.start, &mut g.end, &mut g.side] {
                        *h = (mx.apply(h.0), my.apply(h.1));
                    }
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
                let pad = p.stroke.as_ref().map(|s| s.diameter / 2.0 + 2.0).unwrap_or(2.0);
                let x0 = ((bx - pad).floor() as i64 - ox).max(0);
                let y0 = ((by - pad).floor() as i64 - oy).max(0);
                let x1 = ((bx + bw + pad).ceil() as i64 - ox).min(w as i64);
                let y1 = ((by + bh + pad).ceil() as i64 - oy).min(h as i64);
                if x1 <= x0 || y1 <= y0 {
                    return None;
                }
                let mut layer = RgbaImage::new((x1 - x0) as u32, (y1 - y0) as u32, Rgba::TRANSPARENT);
                let (fx, fy) = (-((ox + x0) as f64), -((oy + y0) as f64));
                if let Some(f) = &p.fill {
                    let polys: Vec<Vec<Pt>> = p.contours.iter().map(|c| c.flatten(fx, fy)).collect();
                    match &f.gradient {
                        // Shade in canvas coordinates (the layer's pixel + its canvas offset).
                        // Fireworks samples gradients at the pixel's integer coordinate (its
                        // top-left corner), not its centre (measured on Folds/Pinch/Waves).
                        Some(g) => fill_polygons_with(&mut layer, &polys, f.anti_alias, &|x, y| {
                            g.color_at(x - 0.5 - fx, y - 0.5 - fy)
                        }),
                        None => fill_polygons(&mut layer, &polys, f.color, f.anti_alias),
                    }
                }
                if let Some(s) = &p.stroke {
                    for c in &p.contours {
                        let mut line = c.flatten(fx, fy);
                        if c.closed {
                            if let Some(&first) = line.first() {
                                line.push(first);
                            }
                        }
                        stamp_stroke(&mut layer, &line, s.color, s.diameter, s.softness);
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
            ElementKind::Instance { matrix, .. } if !matrix.is_affine() => {
                // Perspective: render the symbol flat, then warp its pixels (exact for fills and
                // effects, unlike mapping the geometry). A 9-slice symbol is rendered sliced at the
                // instance's stretch, then warped by the rest of the transform.
                let mut children = self.symbol_elements(ctx)?;
                let nine = self.nine_split(ctx);
                if let Some((mx, my, _)) = &nine {
                    children.iter_mut().for_each(|c| c.map_nine(mx, my));
                }
                let matrix = nine.map_or(*matrix, |(_, _, rest)| rest);
                let matrix = &matrix;
                let inner = ctx.deeper();
                let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
                for c in &children {
                    if let Some((bx, by, bw, bh)) = c.deep_bounds(&inner) {
                        x0 = x0.min(bx);
                        y0 = y0.min(by);
                        x1 = x1.max(bx + bw);
                        y1 = y1.max(by + bh);
                    }
                }
                if x0 > x1 {
                    return None;
                }
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
                // Inverse-map each output pixel; only points in front of the horizon (w > 0 on the
                // same side as the matrix's own w) are visible.
                let inv = matrix.invert()?;
                let side = matrix.i.signum();
                let mut layer = RgbaImage::new(w, h, Rgba::TRANSPARENT);
                for ty in 0..h as i64 {
                    for tx in 0..w as i64 {
                        let (sx, sy) = inv.apply((ox + tx) as f64 + 0.5, (oy + ty) as f64 + 0.5);
                        if (matrix.g * sx + matrix.h * sy + matrix.i) * side <= 0.0 {
                            continue;
                        }
                        let p = sample(&flat, sx - fx as f64 - 0.5, sy - fy as f64 - 0.5);
                        if p.a != 0 {
                            layer.set(tx, ty, p);
                        }
                    }
                }
                Some((layer, 0, 0))
            }
            ElementKind::Instance { .. } => {
                let children = self.instance_elements(ctx)?;
                let inner = ctx.deeper();
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
        (self.opacity.clamp(0.0, 100.0) * 2.55).round() as u8
    }

    /// The object's own pixels after its effects, before opacity and blending, as an image and
    /// its position relative to the output area's top-left (canvas (`ox`, `oy`), `w` x `h`).
    pub fn pixels(&self, ox: i64, oy: i64, w: u32, h: u32, ctx: &RenderCtx) -> Option<(RgbaImage, i64, i64)> {
        let active: Vec<&Effect> = self.effects.iter().filter_map(|e| e.active()).collect();
        if active.is_empty() {
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
        let stroke = self.max_stroke().ceil() as i64;
        let m = self.effect_margin() + stroke + 2;
        let (x, y) = (bx.floor() as i64 - m, by.floor() as i64 - m);
        let (w, h) = ((bx + bw).ceil() as i64 + m - x, (by + bh).ceil() as i64 + m - y);
        (w > 0 && h > 0 && w * h <= MAX_PIXELS).then_some((x, y, w as u32, h as u32))
    }

    /// Half the widest stroke in the object (strokes reach beyond the geometry's bounds).
    fn max_stroke(&self) -> f64 {
        match &self.kind {
            ElementKind::Path(p) => p
                .stroke
                .as_ref()
                .map_or(0.0, |s| s.diameter / 2.0 + s.softness as f64 + 1.0),
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
                ctx.deeper()
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
                    gradient: None,
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
