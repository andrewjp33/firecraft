//! Painting on bitmaps: the Brush, Pencil and Eraser (a live stroke) and the Paint Bucket.
//!
//! Brush and Pencil strokes come from the same brush engine as path strokes
//! (`firecraft_doc::stroke_fw`). Every update renders only the area the new points can change;
//! a pixel's result depends only on the point sequence, not on the pieces it was rendered in.
//!
//! How the stroke is composed and the Eraser's dabs follow FirePixel's measurements
//! (`fp-engine/src/paint.rs` and `commands/bitmap.rs`, Apache-2.0; see ATTRIBUTION.md).

use firecraft_doc::element::{PolyLine, PolyPoint, StoredPolyline};
use firecraft_doc::{BezierPoint, Bitmap, Contour, Element, ElementKind, Path, Stroke, stroke_fw};
use firecraft_raster::blend::BlendMode;
use firecraft_raster::mask::Combine;
use firecraft_raster::wand::magic_wand;
use firecraft_raster::{Mask, Rgba, RgbaImage, WandOptions};

use crate::{DocState, ElementRef, ToolOptions};

/// The tools that paint a stroke by dragging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaintTool {
    Brush,
    Pencil,
    Eraser,
}

impl PaintTool {
    fn label(self) -> &'static str {
        match self {
            PaintTool::Brush => "Brush",
            PaintTool::Pencil => "Pencil",
            PaintTool::Eraser => "Eraser",
        }
    }
}

/// Velocity Fireworks gives points without pointer timing (`VELf`, as FirePixel measured).
const PAINT_VELOCITY: f32 = 0.125;

/// A rectangle in stroke-buffer pixels: left, top, right, bottom (exclusive).
type Area = (i64, i64, i64, i64);

fn union(a: Option<Area>, b: Area) -> Area {
    a.map_or(b, |a| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))
}

/// The Eraser's settings for one stroke.
#[derive(Clone, Copy, Debug)]
struct Erase {
    dab: Dab,
    /// 0..=1.
    opacity: f64,
}

/// The Eraser's tip. Square (measured in Fireworks MX, single dabs at sizes 13 and 30): exactly
/// `size` cells, the cover rising linearly from the outer edge over `Edge`% of the half width.
/// Round (FirePixel's measurements on strokes): an edgeless tip is hard; otherwise full cover up
/// to a core radius, then an exponential fall-off.
#[derive(Clone, Copy, Debug)]
struct Dab {
    cells: i64,
    half: f64,
    round: bool,
    core: f64,
    width: f64,
    /// Round with no Edge.
    hard: bool,
    /// Square: how far in from the outer edge the cover reaches 100%.
    ramp: f64,
}

impl Dab {
    fn new(size: f64, round: bool, edge: f64) -> Dab {
        let size = size.max(1.0);
        let e = edge.clamp(0.0, 100.0);
        let r = size / 2.0;
        // Measured at diameters 13 and 16; smaller and larger tips scale.
        let scale = if r < 6.5 {
            r / 6.5
        } else if r > 8.0 {
            r / 8.0
        } else {
            1.0
        };
        Dab {
            cells: size.ceil() as i64,
            half: r,
            round,
            // FirePixel's curves over the Edge, with the constants solved from a Fireworks MX
            // stroke (size 13, Edge 2: the two soft rows beside it keep alpha 49 and 223).
            core: (r - (0.88116 + 0.020826 * e + 0.000_047_83 * e * e) * scale).max(0.0),
            width: ((0.40113 + 0.003_605_8 * e + 0.000_019_961 * e * e) * scale).max(0.05),
            hard: round && e <= 0.0,
            ramp: r * e / 100.0,
        }
    }

    /// The top-left cell of the dab at mouse pixel `p`, and its centre: size 13 at row 126
    /// centres on the middle of that row and covers rows 120..=133 (measured in Fireworks MX).
    fn origin(&self, p: (f64, f64)) -> (i64, i64, f64, f64) {
        let sx = (p.0 - self.half).floor() as i64 + 1;
        let sy = (p.1 - self.half).floor() as i64 + 1;
        (sx, sy, sx as f64 + self.half, sy as f64 + self.half)
    }

    fn bounds(&self, p: (f64, f64)) -> Area {
        let (sx, sy, _, _) = self.origin(p);
        (sx, sy, sx + self.cells + 1, sy + self.cells + 1)
    }

    /// Call `f(x, y, cover)` for each cell the dab at `p` visibly covers.
    fn each(&self, p: (f64, f64), mut f: impl FnMut(i64, i64, f64)) {
        let (sx, sy, cx, cy) = self.origin(p);
        if !self.round {
            for y in sy..sy + self.cells {
                for x in sx..sx + self.cells {
                    let d = (x as f64 + 0.5 - cx).abs().max((y as f64 + 0.5 - cy).abs());
                    let cover = if self.ramp > 0.0 {
                        ((self.half - d) / self.ramp).min(1.0)
                    } else {
                        1.0
                    };
                    if cover > 0.0 {
                        f(x, y, cover);
                    }
                }
            }
            return;
        }
        if self.hard {
            for y in sy..sy + self.cells {
                for x in sx..sx + self.cells {
                    let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
                    if dx.hypot(dy) <= self.half + 1e-4 {
                        f(x, y, 1.0);
                    }
                }
            }
            return;
        }
        for y in sy..=sy + self.cells {
            for x in sx..=sx + self.cells {
                let (dx, dy) = (x as f64 + 0.5 - cx, y as f64 + 0.5 - cy);
                let d = dx.hypot(dy);
                let w = if d <= self.core {
                    1.0
                } else {
                    (-(d - self.core) / self.width).exp()
                };
                if w >= 0.5 / 255.0 {
                    f(x, y, w);
                }
            }
        }
    }
}

/// Dab positions along `points`, one per pixel (the first point always).
fn dab_points(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let Some(first) = points.first() else {
        return Vec::new();
    };
    let mut stamps = vec![*first];
    let mut carry = 0.0;
    for (&(ax, ay), &(bx, by)) in points.iter().zip(points.iter().skip(1)) {
        let len = (bx - ax).hypot(by - ay);
        if len <= 0.0 {
            continue;
        }
        let mut t = 1.0 - carry;
        while t <= len {
            let f = t / len;
            stamps.push((ax + (bx - ax) * f, ay + (by - ay) * f));
            t += 1.0;
        }
        carry = (carry + len) % 1.0;
    }
    stamps
}

/// The end of a Shift-constrained line from `a` towards `b`: horizontal, vertical or diagonal.
fn constrained(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let m = dx.abs().max(dy.abs());
    if m == 0.0 {
        return a;
    }
    let k = (dy.atan2(dx) / std::f64::consts::FRAC_PI_4).round();
    let (s, c) = (k * std::f64::consts::FRAC_PI_4).sin_cos();
    let unit = |v: f64| {
        if v > 0.5 {
            1.0
        } else if v < -0.5 {
            -1.0
        } else {
            0.0
        }
    };
    (a.0 + unit(c) * m, a.1 + unit(s) * m)
}

/// A stroke in progress.
#[derive(Clone, Debug)]
pub struct LivePaint {
    target: ElementRef,
    new_bitmap: bool,
    /// Canvas position of the buffer's top-left pixel.
    origin: (i64, i64),
    /// The buffer before the stroke.
    base: RgbaImage,
    /// The bitmap's rectangle in the buffer before the stroke (none for a new bitmap).
    old_area: Option<Area>,
    /// The pixel selection, in buffer coordinates: painting stays inside it.
    clip: Option<Mask>,
    keep_alpha: bool,
    opacity: u8,
    blend: BlendMode,
    stroke: Option<Stroke>,
    erase: Option<Erase>,
    seed: u32,
    /// Canvas coordinates.
    points: Vec<(f64, f64)>,
    painted: Option<Area>,
}

impl LivePaint {
    fn buffer_area(&self) -> Area {
        (0, 0, self.base.width() as i64, self.base.height() as i64)
    }

    fn margin(&self) -> i64 {
        match (&self.stroke, &self.erase) {
            (Some(s), _) => stroke_fw::margin(s).ceil() as i64 + 2,
            (_, Some(e)) => e.dab.cells + 2,
            _ => 2,
        }
    }

    /// The buffer area the points `from..` can change.
    fn reach(&self, from: usize) -> Option<Area> {
        let m = self.margin();
        let (ox, oy) = self.origin;
        let pts = self.points.get(from..)?;
        let mut a = None;
        for &(x, y) in pts {
            let (px, py) = (x.floor() as i64 - ox, y.floor() as i64 - oy);
            a = Some(union(a, (px - m, py - m, px + m + 1, py + m + 1)));
        }
        let (l, t, r, b) = a?;
        let (w, h) = (self.base.width() as i64, self.base.height() as i64);
        let c = (l.max(0), t.max(0), r.min(w), b.min(h));
        (c.2 > c.0 && c.3 > c.1).then_some(c)
    }

    /// The stroke points as drawn: a lone click is a zero-length line, and a stroke runs through
    /// the middle of the pixels under the mouse (measured in Fireworks MX: a level line stays
    /// one full row).
    fn drawn_points(&self) -> Vec<(f64, f64)> {
        let mut pts = self.points.clone();
        if pts.len() == 1 {
            pts.extend_from_slice(&self.points);
        }
        for p in &mut pts {
            *p = (p.0.floor() + 0.5, p.1.floor() + 0.5);
        }
        pts
    }

    /// `stroke` as a path whose stored outline is exactly the stroke's points.
    fn path(&self, stroke: &Stroke) -> Path {
        let pts = self.drawn_points();
        let contours = vec![Contour {
            points: pts.iter().map(|p| BezierPoint::corner(p.0, p.1)).collect(),
            closed: false,
        }];
        let line = PolyLine {
            points: pts
                .iter()
                .enumerate()
                .map(|(i, p)| PolyPoint {
                    x: p.0,
                    y: p.1,
                    pressure: 1.0,
                    velocity: PAINT_VELOCITY,
                    seed: point_seed(self.seed, i),
                })
                .collect(),
            closed: false,
        };
        Path {
            pcl: Some(StoredPolyline {
                source: contours.clone(),
                lines: vec![line],
            }),
            contours,
            fill: None,
            stroke: Some(stroke.clone()),
            rect: None,
            // Anchored to the canvas: the texture must not shift as the stroke grows.
            texture_origin: Some((0.0, 0.0)),
        }
    }

    /// The buffer area `area` with the whole stroke applied.
    fn render(&self, area: Area, math: firecraft_raster::ColorMath) -> RgbaImage {
        let (l, t, r, b) = area;
        let (w, h) = ((r - l) as u32, (b - t) as u32);
        let mut out = RgbaImage::new(w, h, Rgba::TRANSPARENT);
        out.copy_from(&self.base, -l, -t);
        if let Some(e) = &self.erase {
            self.erase_into(&mut out, area, e);
            return out;
        }
        let Some(stroke) = &self.stroke else { return out };
        let before = self.keep_alpha.then(|| out.clone());
        let mut layer = RgbaImage::new(w, h, Rgba::TRANSPARENT);
        let off = (-((self.origin.0 + l) as f64), -((self.origin.1 + t) as f64));
        stroke_fw::draw(&mut layer, &self.path(stroke), stroke, None, off);
        if let Some(clip) = &self.clip {
            for (i, px) in layer.pixels_mut().iter_mut().enumerate() {
                if px.a != 0 {
                    let m = clip.get(l + (i as i64 % w as i64), t + (i as i64 / w as i64)) as u32;
                    px.a = ((px.a as u32 * m + 127) / 255) as u8;
                }
            }
        }
        out.draw_blend_with(&layer, 0, 0, self.opacity, self.blend, math);
        if let Some(before) = before {
            for (px, old) in out.pixels_mut().iter_mut().zip(before.pixels()) {
                *px = kept_alpha(*px, *old);
            }
        }
        out
    }

    fn erase_into(&self, out: &mut RgbaImage, area: Area, e: &Erase) {
        let (l, t, r, b) = area;
        let (ox, oy) = self.origin;
        let w = (r - l) as usize;
        let mut keep = vec![1.0f64; w * (b - t) as usize];
        let centres: Vec<(f64, f64)> = self.points.iter().map(|p| (p.0.floor(), p.1.floor())).collect();
        for s in dab_points(&centres) {
            let p = (s.0 - ox as f64, s.1 - oy as f64);
            let d = e.dab.bounds(p);
            if d.2 <= l || d.3 <= t || d.0 >= r || d.1 >= b {
                continue;
            }
            e.dab.each(p, |x, y, c| {
                if x >= l
                    && y >= t
                    && x < r
                    && y < b
                    && let Some(k) = keep.get_mut((y - t) as usize * w + (x - l) as usize)
                {
                    *k *= 1.0 - c;
                }
            });
        }
        // Opacity applies to the whole stroke: overlapping dabs don't add up past it.
        for (i, px) in out.pixels_mut().iter_mut().enumerate() {
            let inside = self.clip.as_ref().map_or(1.0, |m| {
                m.get(l + (i % w) as i64, t + (i / w) as i64) as f64 / 255.0
            });
            let cover = (1.0 - keep.get(i).copied().unwrap_or(1.0)) * e.opacity * inside;
            if cover > 0.0 {
                px.a = (px.a as f64 * (1.0 - cover)).round() as u8;
            }
        }
    }
}

fn point_seed(seed: u32, i: usize) -> u32 {
    let mut x = seed ^ (i as u32).wrapping_mul(0x9e37_79b9);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x.max(1)
}

/// Preserve Transparency: `new`'s colour with `old`'s opacity; a fully transparent pixel stays
/// exactly as it was.
fn kept_alpha(new: Rgba, old: Rgba) -> Rgba {
    if old.a == 0 { old } else { Rgba { a: old.a, ..new } }
}

fn percent(p: u8) -> u8 {
    ((p.min(100) as u32 * 255 + 50) / 100) as u8
}

impl DocState {
    pub fn is_painting(&self) -> bool {
        self.live_paint.is_some()
    }

    /// The bitmap a stroke pressed at (`x`, `y`) paints into: the one with the pixel selection,
    /// the selected one, the one under the press; `None`: a new bitmap.
    fn paint_target(&self, x: f64, y: f64) -> Option<ElementRef> {
        self.pixels
            .as_ref()
            .map(|p| p.target)
            .or_else(|| self.pixel_target(x, y))
    }

    /// Start a stroke at the canvas point. `false` if there is nothing to paint on or with.
    pub fn begin_paint(&mut self, tool: PaintTool, x: f64, y: f64, opts: &ToolOptions) -> bool {
        if self.live_paint.is_some() {
            return false;
        }
        let paint = &opts.paint;
        let target = self.paint_target(x, y);
        if tool == PaintTool::Eraser && target.is_none() {
            return false;
        }
        let color = match tool {
            PaintTool::Eraser => Rgba::TRANSPARENT,
            PaintTool::Pencil if paint.pencil_auto_erase => {
                let Some(stroke) = opts.stroke else { return false };
                let under = target.and_then(|r| match self.element(r).map(|e| &e.kind) {
                    Some(ElementKind::Bitmap(b)) => {
                        b.image.get(x.floor() as i64 - b.x, y.floor() as i64 - b.y)
                    }
                    _ => None,
                });
                match (under, opts.fill) {
                    (Some(u), Some(fill)) if u == stroke => fill,
                    _ => stroke,
                }
            }
            _ => {
                let Some(c) = opts.stroke else { return false };
                c
            }
        };
        let stroke = match tool {
            PaintTool::Eraser => None,
            PaintTool::Pencil => {
                let name = if paint.pencil_anti_alias {
                    "bn_1-Pixel Anti-Aliased"
                } else {
                    "bn_1-Pixel"
                };
                let preset = firecraft_doc::brush_presets::find("bc_Pencil", name)
                    .unwrap_or_else(firecraft_doc::brush_presets::default_index);
                Some(Stroke::from_preset(color, preset, 1.0))
            }
            PaintTool::Brush => Some(opts.new_stroke(color)),
        };
        let erase = (tool == PaintTool::Eraser).then(|| Erase {
            dab: Dab::new(
                paint.eraser_size.max(1) as f64,
                paint.eraser_round,
                paint.eraser_edge as f64,
            ),
            opacity: paint.eraser_opacity.min(100) as f64 / 100.0,
        });

        self.begin_live_edit(tool.label());
        let (cw, ch) = (self.doc.width as i64, self.doc.height as i64);
        let keep_alpha = paint.preserve_transparency && tool != PaintTool::Eraser;
        let clip = self.pixels.as_ref().map(|p| p.mask.clone());
        let (target, new_bitmap, origin, base, old_area) = match target {
            Some(r) => {
                let Some(ElementKind::Bitmap(b)) = self.element(r).map(|e| &e.kind) else {
                    self.discard_live_edit();
                    return false;
                };
                let (bx, by, bw, bh) = (b.x, b.y, b.image.width() as i64, b.image.height() as i64);
                // Only an unconstrained stroke can make the bitmap bigger.
                if clip.is_some() || keep_alpha || tool == PaintTool::Eraser {
                    (r, false, (bx, by), b.image.clone(), Some((0, 0, bw, bh)))
                } else {
                    let (l, t) = (bx.min(0), by.min(0));
                    let (rr, bb) = ((bx + bw).max(cw), (by + bh).max(ch));
                    let mut buf = RgbaImage::new((rr - l) as u32, (bb - t) as u32, Rgba::TRANSPARENT);
                    buf.copy_from(&b.image, bx - l, by - t);
                    let area = (bx - l, by - t, bx - l + bw, by - t + bh);
                    (r, false, (l, t), buf, Some(area))
                }
            }
            None => {
                let buf = RgbaImage::new(cw.max(1) as u32, ch.max(1) as u32, Rgba::TRANSPARENT);
                let layer = self.writable_layer();
                let f = self.frame();
                let el = Element::new(ElementKind::Bitmap(Bitmap {
                    x: 0,
                    y: 0,
                    image: buf.clone(),
                }));
                let Some(cell) = self.doc.layers.get_mut(layer).and_then(|l| l.elements_mut(f)) else {
                    self.discard_live_edit();
                    return false;
                };
                cell.push(el);
                let r = ElementRef::new(layer, cell.len() - 1);
                self.selection = vec![r];
                self.pixels = None;
                (r, true, (0, 0), buf, None)
            }
        };
        if !new_bitmap {
            self.selection = vec![target];
            self.set_paint_buffer(target, origin, &base);
        }
        let seed = point_seed(
            (self.revision as u32) ^ ((x * 16.0) as u32).rotate_left(11) ^ (y * 16.0) as u32,
            0,
        );
        self.live_paint = Some(LivePaint {
            target,
            new_bitmap,
            origin,
            base,
            old_area,
            clip,
            keep_alpha,
            opacity: if tool == PaintTool::Eraser {
                255
            } else {
                percent(paint.opacity)
            },
            blend: paint.blend,
            stroke,
            erase,
            seed,
            points: vec![(x, y)],
            painted: None,
        });
        self.repaint_from(0);
        true
    }

    fn set_paint_buffer(&mut self, target: ElementRef, origin: (i64, i64), image: &RgbaImage) {
        if let Some(Element {
            kind: ElementKind::Bitmap(b),
            ..
        }) = self.element_mut(target)
        {
            b.x = origin.0;
            b.y = origin.1;
            b.image = image.clone();
        }
    }

    /// Extend the stroke to the canvas point; `constrain` (Shift) makes it one straight
    /// horizontal, vertical or diagonal line from the press.
    pub fn extend_paint(&mut self, x: f64, y: f64, constrain: bool) {
        let Some(live) = self.live_paint.as_mut() else {
            return;
        };
        if live.points.last() == Some(&(x, y)) {
            return;
        }
        if constrain {
            let Some(&start) = live.points.first() else { return };
            let end = constrained(start, (x, y));
            live.points = vec![start, end];
            // The old stroke goes too.
            let old = live.painted;
            let new = live.reach(0);
            if let Some(area) = match (old, new) {
                (Some(o), Some(n)) => Some(union(Some(o), n)),
                (o, n) => o.or(n),
            } {
                self.repaint(area);
            }
            return;
        }
        live.points.push((x, y));
        // From two points back: the previous end point wasn't final.
        let from = live.points.len().saturating_sub(3);
        self.repaint_from(from);
    }

    fn repaint_from(&mut self, from: usize) {
        if let Some(area) = self.live_paint.as_ref().and_then(|l| l.reach(from)) {
            self.repaint(area);
        }
    }

    fn repaint(&mut self, area: Area) {
        let math = self.doc.color_math;
        let Some(live) = self.live_paint.as_mut() else {
            return;
        };
        let (l, t) = (area.0, area.1);
        let part = live.render(area, math);
        live.painted = Some(union(live.painted, area));
        let target = live.target;
        if let Some(Element {
            kind: ElementKind::Bitmap(b),
            ..
        }) = self.element_mut(target)
        {
            b.image.copy_from(&part, l, t);
        }
        self.touch();
    }

    /// Finish the stroke: one undo step; the bitmap shrinks to what it holds.
    pub fn end_paint(&mut self) {
        let Some(live) = self.live_paint.take() else {
            return;
        };
        let Some(Element {
            kind: ElementKind::Bitmap(b),
            ..
        }) = self.element(live.target)
        else {
            self.cancel_live_edit();
            return;
        };
        if b.image == live.base {
            // Nothing changed (say, Preserve Transparency over nothing): no undo step, no
            // empty bitmap left behind.
            self.cancel_live_edit();
            return;
        }
        // Only the buffer's extra room is trimmed: what the bitmap covered before stays.
        let grew = live.new_bitmap || live.old_area != Some(live.buffer_area());
        if grew {
            let image = b.image.clone();
            let opaque = opaque_bounds(&image);
            let keep = match (opaque, live.old_area) {
                (Some(o), Some(a)) => Some(union(Some(a), o)),
                (o, a) => o.or(a),
            };
            let Some((l, t, r, bb)) = keep else {
                self.cancel_live_edit();
                return;
            };
            let mut crop = RgbaImage::new((r - l) as u32, (bb - t) as u32, Rgba::TRANSPARENT);
            crop.copy_from(&image, -l, -t);
            self.set_paint_buffer(live.target, (live.origin.0 + l, live.origin.1 + t), &crop);
        }
        self.touch();
    }

    /// Abandon the stroke (Esc): the document returns to how it was.
    pub fn cancel_paint(&mut self) {
        if self.live_paint.take().is_some() {
            self.cancel_live_edit();
        }
    }

    /// Paint Bucket click at a canvas point: fills the similar pixels connected to it (or the
    /// whole pixel selection with Fill Selection), or the path under the pointer.
    pub fn paint_bucket(&mut self, x: f64, y: f64, opts: &ToolOptions) {
        let Some(color) = opts.fill else { return };
        let hit = self.hit_test(x, y);
        let path_hit =
            hit.filter(|r| matches!(self.element(*r).map(|e| &e.kind), Some(ElementKind::Path(_))));
        if self.pixels.is_none()
            && let Some(r) = path_hit
        {
            self.selection = vec![r];
            self.set_fill_color(Some(color));
            return;
        }
        let Some(target) = self.paint_target(x, y) else {
            return;
        };
        let Some(ElementKind::Bitmap(b)) = self.element(target).map(|e| &e.kind) else {
            return;
        };
        let paint = &opts.paint;
        let selected = self
            .pixels
            .as_ref()
            .filter(|p| p.target == target)
            .map(|p| &p.mask);
        let (px, py) = (x.floor() as i64 - b.x, y.floor() as i64 - b.y);
        // Fireworks trims a bitmap to its opaque area, so what lies beyond it isn't part of it.
        let extent = opaque_bounds(&b.image);
        let mask = match selected {
            Some(sel) if paint.bucket_fill_selection => sel.clone(),
            _ => {
                let inside = extent.is_some_and(|(l, t, r, bb)| px >= l && py >= t && px < r && py < bb);
                if !inside {
                    return;
                }
                let mut m = magic_wand(
                    &b.image,
                    px,
                    py,
                    WandOptions {
                        tolerance: paint.bucket_tolerance,
                        edge: paint.bucket_edge,
                        aa_style: firecraft_raster::AntiAliasStyle::Classic,
                        contiguous: true,
                    },
                );
                if let Some(sel) = selected {
                    m.combine(sel, Combine::Intersect);
                }
                if let Some((l, t, r, bb)) = extent {
                    let w = b.image.width() as i64;
                    for (i, v) in m.data_mut().iter_mut().enumerate() {
                        let (x, y) = (i as i64 % w, i as i64 / w);
                        if x < l || y < t || x >= r || y >= bb {
                            *v = 0;
                        }
                    }
                }
                m
            }
        };
        if mask.is_empty() {
            return;
        }
        let math = self.doc.color_math;
        let opacity = percent(paint.opacity);
        let (blend, keep_alpha) = (paint.blend, paint.preserve_transparency);
        self.edit("Paint Bucket", |s| {
            if let Some(Element {
                kind: ElementKind::Bitmap(b),
                ..
            }) = s.element_mut(target)
            {
                fill_masked(&mut b.image, &mask, color, opacity, blend, keep_alpha, math);
            }
        });
    }
}

/// Paint `color` through `mask` (bitmap-sized).
fn fill_masked(
    image: &mut RgbaImage,
    mask: &Mask,
    color: Rgba,
    opacity: u8,
    blend: BlendMode,
    keep_alpha: bool,
    math: firecraft_raster::ColorMath,
) {
    for (px, &m) in image.pixels_mut().iter_mut().zip(mask.data()) {
        if m == 0 {
            continue;
        }
        let src = Rgba::new(
            color.r,
            color.g,
            color.b,
            ((color.a as u32 * m as u32 + 127) / 255) as u8,
        );
        let old = *px;
        *px = if blend == BlendMode::Normal {
            old.over(src, opacity)
        } else {
            blend.composite_with(old, src, opacity, math)
        };
        if keep_alpha {
            *px = kept_alpha(*px, old);
        }
    }
}

/// The rectangle holding every non-transparent pixel.
fn opaque_bounds(image: &RgbaImage) -> Option<Area> {
    let w = image.width() as i64;
    let mut a = None;
    for (i, p) in image.pixels().iter().enumerate() {
        if p.a != 0 {
            let (x, y) = (i as i64 % w, i as i64 / w);
            a = Some(a.map_or((x, y, x + 1, y + 1), |(l, t, r, b): Area| {
                (l.min(x), t.min(y), r.max(x + 1), b.max(y + 1))
            }));
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use firecraft_doc::Document;

    const RED: Rgba = Rgba::new(255, 0, 0, 255);

    fn doc() -> DocState {
        DocState::new(Document::new(100, 80, 72.0, Some(Rgba::WHITE)))
    }

    fn options() -> ToolOptions {
        ToolOptions {
            stroke: Some(RED),
            fill: Some(Rgba::new(0, 0, 255, 255)),
            ..ToolOptions::default()
        }
    }

    fn add_bitmap(d: &mut DocState, x: i64, y: i64, w: u32, h: u32, c: Rgba) -> ElementRef {
        d.add_element(
            Element::new(ElementKind::Bitmap(Bitmap {
                x,
                y,
                image: RgbaImage::new(w, h, c),
            })),
            "Add",
        );
        d.selection[0]
    }

    fn bitmap(d: &DocState, r: ElementRef) -> Bitmap {
        match d.element(r).map(|e| &e.kind) {
            Some(ElementKind::Bitmap(b)) => b.clone(),
            other => panic!("not a bitmap: {other:?}"),
        }
    }

    fn line(d: &mut DocState, tool: PaintTool, a: (f64, f64), b: (f64, f64), o: &ToolOptions) -> bool {
        let began = d.begin_paint(tool, a.0, a.1, o);
        d.extend_paint(b.0, b.1, false);
        d.end_paint();
        began
    }

    #[test]
    fn brush_without_a_bitmap_makes_a_trimmed_one_and_undoes_in_one_step() {
        let mut d = doc();
        assert!(line(
            &mut d,
            PaintTool::Pencil,
            (10.0, 20.0),
            (30.0, 20.0),
            &options()
        ));
        let r = d.selection[0];
        let b = bitmap(&d, r);
        assert_eq!((b.x, b.y, b.image.height()), (10, 20, 1), "only the row it drew");
        assert!(b.image.width() <= 21);
        assert!(b.image.pixels().iter().any(|p| p.a != 0));
        d.undo();
        assert!(d.element(r).is_none());
    }

    #[test]
    fn a_stroke_on_the_selected_bitmap_extends_it_past_its_edge() {
        let mut d = doc();
        let r = add_bitmap(&mut d, 20, 20, 10, 10, Rgba::new(0, 255, 0, 255));
        let before = d.history.labels().count();
        line(&mut d, PaintTool::Pencil, (25.0, 25.0), (45.0, 25.0), &options());
        assert_eq!(d.history.labels().count(), before + 1);
        let b = bitmap(&d, r);
        assert_eq!((b.x, b.y, b.image.height()), (20, 20, 10), "keeps the old area");
        assert!(b.image.width() > 20, "grew to the right: {}", b.image.width());
        // Still its old content in the corner.
        assert_eq!(b.image.get(0, 0), Some(Rgba::new(0, 255, 0, 255)));
    }

    #[test]
    fn live_preview_equals_one_piece() {
        // Painted point by point against everything at once.
        let pts = [
            (10.0, 10.0),
            (14.3, 12.1),
            (22.7, 18.9),
            (30.1, 15.5),
            (41.0, 33.0),
        ];
        let mut o = options();
        o.stroke_size = 7;
        o.stroke_preset = firecraft_doc::brush_presets::find("bc_Air Brush", "bn_Basic")
            .or_else(|| firecraft_doc::brush_presets::find("bc_Basic", "bn_Soft Rounded"))
            .unwrap_or(0);
        let mut a = doc();
        a.begin_paint(PaintTool::Brush, pts[0].0, pts[0].1, &o);
        for p in &pts[1..] {
            a.extend_paint(p.0, p.1, false);
        }
        let img_a = bitmap(&a, a.selection[0]).image;
        let live = a.live_paint.as_ref().expect("painting");
        let whole = live.render(live.buffer_area(), a.doc.color_math);
        assert_eq!(img_a, whole);
        a.end_paint();
    }

    #[test]
    fn nothing_painted_leaves_nothing_behind() {
        let mut d = doc();
        let mut o = options();
        o.paint.preserve_transparency = true;
        let r = add_bitmap(&mut d, 20, 20, 10, 10, Rgba::TRANSPARENT);
        let steps = d.history.labels().count();
        line(&mut d, PaintTool::Pencil, (22.0, 22.0), (27.0, 22.0), &o);
        assert_eq!(d.history.labels().count(), steps);
        assert_eq!(bitmap(&d, r).image.width(), 10);
        // A stroke entirely off the canvas makes no bitmap.
        let count = d.doc.layers[0].elements(0).len();
        d.deselect();
        line(
            &mut d,
            PaintTool::Pencil,
            (-50.0, -50.0),
            (-40.0, -50.0),
            &options(),
        );
        assert_eq!(d.doc.layers[0].elements(0).len(), count);
    }

    #[test]
    fn painting_stays_inside_a_pixel_selection() {
        let mut d = doc();
        let r = add_bitmap(&mut d, 0, 0, 40, 40, Rgba::new(0, 255, 0, 255));
        let o = options();
        d.marquee((10.0, 10.0, 20.0, 20.0), false, &o, Combine::Replace);
        line(&mut d, PaintTool::Pencil, (0.0, 15.0), (39.0, 15.0), &o);
        let b = bitmap(&d, r);
        assert_eq!(b.image.get(5, 15), Some(Rgba::new(0, 255, 0, 255)), "outside");
        assert_ne!(b.image.get(15, 15), Some(Rgba::new(0, 255, 0, 255)), "inside");
    }

    #[test]
    fn eraser_needs_a_bitmap_and_removes_pixels() {
        let mut d = doc();
        let mut o = options();
        o.paint.eraser_size = 5;
        o.paint.eraser_round = false;
        assert!(!d.begin_paint(PaintTool::Eraser, 10.0, 10.0, &o));
        let r = add_bitmap(&mut d, 0, 0, 40, 40, RED);
        line(&mut d, PaintTool::Eraser, (5.0, 20.0), (35.0, 20.0), &o);
        let b = bitmap(&d, r);
        assert_eq!(b.image.get(20, 20).map(|p| p.a), Some(0));
        assert_eq!(b.image.get(20, 5).map(|p| p.a), Some(255));
        assert_eq!(b.image.width(), 40, "the eraser never resizes");
        // Half-strength erasing removes half, however much the dabs overlap.
        let r = add_bitmap(&mut d, 0, 0, 40, 40, RED);
        o.paint.eraser_opacity = 50;
        line(&mut d, PaintTool::Eraser, (5.0, 20.0), (35.0, 20.0), &o);
        assert_eq!(bitmap(&d, r).image.get(20, 20).map(|p| p.a), Some(128));
    }

    #[test]
    fn hard_square_eraser_covers_exactly_its_size() {
        // FirePixel's measurement: size 13, square, at y 150 clears rows 144..=156.
        let dab = Dab::new(13.0, false, 0.0);
        let mut rows = std::collections::BTreeSet::new();
        dab.each((60.0, 150.0), |_, y, _| {
            rows.insert(y);
        });
        assert_eq!(rows.first(), Some(&144));
        assert_eq!(rows.last(), Some(&156));
    }

    /// What a single click leaves of an opaque bitmap, along the row through the dab.
    fn dab_row(size: u32, edge: u8, from: i64, to: i64) -> Vec<u8> {
        let mut d = doc();
        let mut o = options();
        o.paint.eraser_size = size;
        o.paint.eraser_round = false;
        o.paint.eraser_edge = edge;
        let r = add_bitmap(&mut d, 0, 0, 100, 80, RED);
        d.begin_paint(PaintTool::Eraser, 53.0, 40.0, &o);
        d.end_paint();
        let b = bitmap(&d, r);
        (from..to)
            .map(|x| b.image.get(x, 40).map_or(0, |p| p.a))
            .collect()
    }

    fn assert_close(got: &[u8], want: &[u8], tolerance: i32) {
        assert_eq!(got.len(), want.len());
        for (g, w) in got.iter().zip(want) {
            assert!(
                (*g as i32 - *w as i32).abs() <= tolerance,
                "{got:02X?} vs {want:02X?}"
            );
        }
    }

    #[test]
    fn square_eraser_edge_matches_fireworks_mx() {
        // Single dabs measured in Fireworks MX: alpha left along the centre row, x = 46..=53.
        assert_close(&dab_row(13, 50, 46, 54), &[0xFF, 0xD7, 0x89, 0x3A, 0, 0, 0, 0], 3);
        assert_close(
            &dab_row(13, 100, 46, 54),
            &[0xFF, 0xEB, 0xC4, 0x9C, 0x75, 0x4E, 0x27, 0],
            3,
        );
        assert_close(
            &dab_row(13, 75, 46, 54),
            &[0xFF, 0xE4, 0xB0, 0x7C, 0x47, 0x13, 0, 0],
            3,
        );
        assert_close(&dab_row(13, 25, 46, 49), &[0xFF, 0xB0, 0x13], 3);
        // Size 30 (centred between columns 53 and 54).
        assert_close(&dab_row(30, 50, 38, 42), &[0xFF, 0xEE, 0xCC, 0xAA], 3);
        assert_close(&dab_row(30, 100, 38, 41), &[0xFF, 0xF6, 0xE5], 3);
        assert_close(&dab_row(30, 100, 52, 56), &[0x19, 0x08, 0x08, 0x19], 3);
        // No Edge: hard.
        assert_close(&dab_row(13, 0, 46, 49), &[0xFF, 0, 0], 0);
    }

    #[test]
    fn square_eraser_stroke_matches_fireworks_mx() {
        // A stroke along row 40, measured rows above it: 13 dabs build up at the outer row.
        let rows = |edge: u8| {
            let mut d = doc();
            let mut o = options();
            o.paint.eraser_round = false;
            o.paint.eraser_edge = edge;
            let r = add_bitmap(&mut d, 0, 0, 100, 80, RED);
            line(&mut d, PaintTool::Eraser, (20.0, 40.0), (80.0, 40.0), &o);
            let b = bitmap(&d, r);
            (33..37)
                .map(|y| b.image.get(50, y).map_or(0, |p| p.a))
                .collect::<Vec<u8>>()
        };
        assert_close(&rows(100), &[0xFF, 0x58, 0x0C, 0x02], 3);
        assert_close(&rows(50), &[0xFF, 0x1D, 0x01, 0], 3);
        assert_close(&rows(0), &[0xFF, 0, 0, 0], 0);
    }

    #[test]
    fn pencil_auto_erase_draws_the_fill_colour_on_the_stroke_colour() {
        let mut d = doc();
        let mut o = options();
        o.paint.pencil_auto_erase = true;
        o.paint.pencil_anti_alias = false;
        let r = add_bitmap(&mut d, 0, 0, 20, 20, RED);
        line(&mut d, PaintTool::Pencil, (5.0, 5.0), (10.0, 5.0), &o);
        assert_eq!(bitmap(&d, r).image.get(7, 5), o.fill);
    }

    #[test]
    fn shift_constrains_to_a_straight_line() {
        assert_eq!(constrained((10.0, 10.0), (30.0, 12.0)), (30.0, 10.0));
        assert_eq!(constrained((10.0, 10.0), (12.0, 30.0)), (10.0, 30.0));
        assert_eq!(constrained((10.0, 10.0), (29.0, 31.0)), (31.0, 31.0));
        let mut d = doc();
        let o = options();
        d.begin_paint(PaintTool::Pencil, 10.0, 10.0, &o);
        d.extend_paint(20.0, 40.0, false);
        d.extend_paint(30.0, 11.0, true);
        d.end_paint();
        let b = bitmap(&d, d.selection[0]);
        assert_eq!(b.image.height(), 1, "the wandering part is gone");
    }

    #[test]
    fn bucket_fills_connected_similar_pixels_only() {
        let mut d = doc();
        let o = options();
        let r = add_bitmap(&mut d, 0, 0, 30, 10, Rgba::new(10, 10, 10, 255));
        // A wall at x = 15.
        d.edit("wall", |s| {
            if let Some(Element {
                kind: ElementKind::Bitmap(b),
                ..
            }) = s.element_mut(r)
            {
                for y in 0..10 {
                    b.image.set(15, y, Rgba::new(250, 250, 250, 255));
                }
            }
        });
        d.paint_bucket(3.5, 3.5, &o);
        let b = bitmap(&d, r);
        assert_eq!(b.image.get(5, 5), o.fill);
        assert_eq!(b.image.get(20, 5), Some(Rgba::new(10, 10, 10, 255)));
    }

    #[test]
    fn bucket_fills_a_whole_selection_and_paths() {
        let mut d = doc();
        let mut o = options();
        o.paint.bucket_fill_selection = true;
        let r = add_bitmap(&mut d, 0, 0, 30, 30, Rgba::new(10, 10, 10, 255));
        d.marquee((5.0, 5.0, 10.0, 10.0), false, &o, Combine::Replace);
        d.paint_bucket(7.5, 7.5, &o);
        let b = bitmap(&d, r);
        assert_eq!(b.image.get(7, 7), o.fill);
        assert_eq!(b.image.get(15, 15), Some(Rgba::new(10, 10, 10, 255)));
        d.deselect();
        d.add_shape((40.0, 40.0, 60.0, 60.0), false, &options());
        let path = d.selection[0];
        d.deselect();
        d.paint_bucket(50.0, 50.0, &o);
        match d.element(path).map(|e| &e.kind) {
            Some(ElementKind::Path(p)) => {
                assert_eq!(p.fill.as_ref().map(|f| f.color), o.fill);
            }
            other => panic!("{other:?}"),
        }
    }
}
