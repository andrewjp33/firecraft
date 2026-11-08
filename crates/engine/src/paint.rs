//! Painting on bitmaps: the Brush, Pencil and Eraser (a live stroke) and the Paint Bucket.
//!
//! Brush and Pencil strokes come from the same brush engine as path strokes
//! (`firecraft_doc::stroke_fw`). Every update renders only the area the new points can change;
//! a pixel's result depends only on the point sequence, not on the pieces it was rendered in.
//!
//! How the stroke is composed and the Eraser's dabs follow FirePixel's measurements
//! (`fp-engine/src/paint.rs` and `commands/bitmap.rs`, Apache-2.0; see ATTRIBUTION.md).

use firecraft_doc::element::PolyPoint;
#[cfg(test)]
use firecraft_doc::element::{PolyLine, StoredPolyline};
#[cfg(test)]
use firecraft_doc::{BezierPoint, Contour, Path};
use firecraft_doc::{Bitmap, Element, ElementKind, Stroke, stroke_fw};
use firecraft_raster::blend::BlendMode;
use firecraft_raster::mask::Combine;
use firecraft_raster::wand::magic_wand;
use firecraft_raster::{Mask, Rgba, RgbaImage, WandOptions};
use firecraft_stroke::brush;

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
#[derive(Clone, Debug)]
struct Erase {
    dab: Dab,
    /// 0..=1.
    opacity: f64,
}

/// The Eraser's tip: Fireworks' brush tip with the Edge as its softness, with the bell curve when
/// round (single dabs of sizes 7 to 60 at Edge 2 to 100, measured in Fireworks MX, match exactly).
/// Computed once per stroke.
#[derive(Clone, Debug)]
struct Dab {
    /// Cells per side.
    cells: i64,
    half: f64,
    round: bool,
    /// Cover of each cell, row by row.
    cover: std::sync::Arc<Vec<f64>>,
}

impl Dab {
    fn new(size: f64, round: bool, edge: f64) -> Dab {
        let dp = size.max(1.0).round();
        // A round tip has one more row and column, on its far side.
        let cells = dp as i64 + i64::from(round);
        let rr = (dp * 8.0) as i64;
        let s = brush::softness_byte(dp, (edge.clamp(0.0, 100.0) * 10.0) as u32) as i64;
        let bell = brush::bell_table();
        let at = |i: i64| (2 * i + 1 - dp as i64) * 8;
        let cover = (0..cells)
            .flat_map(|y| (0..cells).map(move |x| (x, y)))
            .map(|(x, y)| brush::tip_value(at(x), at(y), rr, s, round, round, &bell) as f64 / 255.0)
            .collect();
        Dab {
            cells,
            half: dp / 2.0,
            round,
            cover: std::sync::Arc::new(cover),
        }
    }

    /// The top-left cell of the dab at mouse pixel `p`. Odd sizes centre on that pixel; even
    /// ones on its left/top edge when round, on its right/bottom edge when square (measured).
    fn origin(&self, p: (f64, f64)) -> (i64, i64) {
        let start = |v: f64| {
            if self.round {
                (v - self.half).ceil() as i64
            } else {
                (v - self.half).floor() as i64 + 1
            }
        };
        (start(p.0), start(p.1))
    }

    fn bounds(&self, p: (f64, f64)) -> Area {
        let (sx, sy) = self.origin(p);
        (sx, sy, sx + self.cells, sy + self.cells)
    }

    /// Call `f(x, y, cover)` for each cell the dab at `p` covers.
    fn each(&self, p: (f64, f64), mut f: impl FnMut(i64, i64, f64)) {
        let (sx, sy) = self.origin(p);
        for (i, &c) in self.cover.iter().enumerate() {
            if c > 0.0 {
                let i = i as i64;
                f(sx + i % self.cells, sy + i / self.cells, c);
            }
        }
    }
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
    #[cfg(test)]
    stroke: Option<Stroke>,
    erase: Option<Erase>,
    seed: u32,
    /// Canvas coordinates.
    points: Vec<(f64, f64)>,
    painted: Option<Area>,
    /// An area to redraw at the next update (what a restarted stroke covered).
    pending: Option<Area>,
    /// The stroke's ink so far (Brush, Pencil).
    ink: Option<stroke_fw::LiveDraw>,
    /// Eraser: what's left of each buffer pixel so far, and the dab spacing carried over.
    keep: Vec<f32>,
    carry: f64,
    /// How many `points` have been drawn.
    fed: usize,
    /// Start again from the first point at the next update.
    restart: bool,
}

impl LivePaint {
    fn buffer_area(&self) -> Area {
        (0, 0, self.base.width() as i64, self.base.height() as i64)
    }

    /// Draw the points not yet drawn; the buffer area that changed.
    fn advance(&mut self) -> Option<Area> {
        let mut area = self.pending.take();
        // A lone click drew a dab that a drag's first dab replaces.
        if self.ink.is_some() && self.fed == 1 && self.points.len() > 1 && !self.restart {
            self.restart = true;
            area = self.painted.map(|p| union(area, p)).or(area);
        }
        if self.restart {
            self.restart = false;
            self.fed = 0;
            self.carry = 0.0;
            self.keep.iter_mut().for_each(|k| *k = 1.0);
            if let Some(ink) = &mut self.ink {
                ink.reset();
            }
        }
        if self.fed >= self.points.len() {
            return area;
        }
        let new = if self.ink.is_some() {
            self.feed_stroke()
        } else {
            self.feed_eraser()
        };
        self.fed = self.points.len();
        match (area, new) {
            (Some(a), Some(n)) => Some(union(Some(a), n)),
            (a, n) => a.or(n),
        }
    }

    fn feed_stroke(&mut self) -> Option<Area> {
        let (ox, oy) = (self.origin.0 as f64, self.origin.1 as f64);
        let from = self.fed.saturating_sub(1);
        let seed = self.seed;
        let mut pts: Vec<PolyPoint> = self
            .points
            .iter()
            .enumerate()
            .skip(from)
            .map(|(i, p)| PolyPoint {
                // Through the middle of the pixels under the mouse (measured in Fireworks MX).
                x: p.0.floor() + 0.5 - ox,
                y: p.1.floor() + 0.5 - oy,
                pressure: 1.0,
                velocity: PAINT_VELOCITY,
                seed: point_seed(seed, i),
            })
            .collect();
        // A lone click is a zero-length line.
        if let [only] = pts[..] {
            pts.push(PolyPoint {
                seed: point_seed(seed, 1),
                ..only
            });
        }
        self.ink.as_mut()?.feed(&pts)
    }

    fn feed_eraser(&mut self) -> Option<Area> {
        let e = self.erase.clone()?;
        let (ox, oy) = (self.origin.0 as f64, self.origin.1 as f64);
        let centres: Vec<(f64, f64)> = self
            .points
            .iter()
            .skip(self.fed.saturating_sub(1))
            .map(|p| (p.0.floor() - ox, p.1.floor() - oy))
            .collect();
        let mut stamps = Vec::new();
        if self.fed == 0 {
            stamps.extend(centres.first().copied());
        }
        // One dab per pixel along the stroke.
        for (&(ax, ay), &(bx, by)) in centres.iter().zip(centres.iter().skip(1)) {
            let len = (bx - ax).hypot(by - ay);
            if len <= 0.0 {
                continue;
            }
            let mut t = 1.0 - self.carry;
            while t <= len {
                let f = t / len;
                stamps.push((ax + (bx - ax) * f, ay + (by - ay) * f));
                t += 1.0;
            }
            self.carry = (self.carry + len) % 1.0;
        }
        let (w, h) = (self.base.width() as i64, self.base.height() as i64);
        let mut area = None;
        for s in stamps {
            let (l, t, r, b) = e.dab.bounds(s);
            let clipped = (l.max(0), t.max(0), r.min(w), b.min(h));
            if clipped.2 > clipped.0 && clipped.3 > clipped.1 {
                area = Some(union(area, clipped));
            }
            let keep = &mut self.keep;
            e.dab.each(s, |x, y, c| {
                if x >= 0
                    && y >= 0
                    && x < w
                    && y < h
                    && let Some(k) = keep.get_mut((y * w + x) as usize)
                {
                    *k *= 1.0 - c as f32;
                }
            });
        }
        area
    }

    #[cfg(test)]
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
    #[cfg(test)]
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
        let Some(ink) = &self.ink else { return out };
        let before = self.keep_alpha.then(|| out.clone());
        let mut layer = ink.layer(area);
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
        let (l, t, r, _) = area;
        let (aw, bw) = ((r - l) as usize, self.base.width() as usize);
        // Opacity applies to the whole stroke: overlapping dabs don't add up past it.
        for (i, px) in out.pixels_mut().iter_mut().enumerate() {
            let (x, y) = (l as usize + i % aw, t as usize + i / aw);
            let inside = self
                .clip
                .as_ref()
                .map_or(1.0, |m| m.get(x as i64, y as i64) as f64 / 255.0);
            let left = self.keep.get(y * bw + x).copied().unwrap_or(1.0) as f64;
            let cover = (1.0 - left) * e.opacity * inside;
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
        let (bw, bh) = (base.width(), base.height());
        let ink = stroke
            .as_ref()
            .map(|s| stroke_fw::LiveDraw::new(s, bw, bh, (-(origin.0 as f64), -(origin.1 as f64))));
        let keep = if erase.is_some() {
            vec![1.0; bw as usize * bh as usize]
        } else {
            Vec::new()
        };
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
            #[cfg(test)]
            stroke,
            erase,
            seed,
            points: vec![(x, y)],
            painted: None,
            pending: None,
            ink,
            keep,
            carry: 0.0,
            fed: 0,
            restart: false,
        });
        self.flush_paint();
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
            live.pending = live.painted.map(|p| union(live.pending, p)).or(live.pending);
            live.restart = true;
            return;
        }
        live.points.push((x, y));
    }

    /// Bring the bitmap up to date with the points given so far (once per screen update: a
    /// fast mouse delivers several points per frame).
    pub fn flush_paint(&mut self) {
        if let Some(area) = self.live_paint.as_mut().and_then(LivePaint::advance) {
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
        self.flush_paint();
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
        self.commit_live_edit();
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

    /// A wandering path that doubles back over itself.
    fn scribble() -> Vec<(f64, f64)> {
        (0..60)
            .map(|i| {
                let t = i as f64 * 0.37;
                (
                    120.0 + t.sin() * 70.0 * (t * 0.31).cos(),
                    90.0 + (t * 1.7).cos() * 50.0,
                )
            })
            .collect()
    }

    #[test]
    fn live_strokes_equal_drawing_the_whole_stroke() {
        // Added a few points at a time against one full render, for every preset.
        let pts = scribble();
        for (i, p) in firecraft_doc::brush_presets::BRUSH_PRESETS.iter().enumerate() {
            let o = ToolOptions {
                stroke_size: 30,
                stroke_preset: i,
                ..options()
            };
            let mut a = DocState::new(Document::new(240, 180, 72.0, Some(Rgba::WHITE)));
            a.begin_paint(PaintTool::Brush, pts[0].0, pts[0].1, &o);
            for (n, q) in pts.iter().enumerate().skip(1) {
                a.extend_paint(q.0, q.1, false);
                if n % 3 == 0 {
                    a.flush_paint();
                }
            }
            a.flush_paint();
            let live = a.live_paint.as_ref().expect("painting");
            let area = live.buffer_area();
            let shown = live.ink.as_ref().expect("ink").layer(area);
            let stroke = live.stroke.as_ref().expect("stroke");
            let mut whole = RgbaImage::new(240, 180, Rgba::TRANSPARENT);
            stroke_fw::draw(&mut whole, &live.path(stroke), stroke, None, (0.0, 0.0));
            let differ = shown
                .pixels()
                .iter()
                .zip(whole.pixels())
                .filter(|(a, b)| a.a != b.a || (a.a != 0 && a != b))
                .count();
            assert_eq!(differ, 0, "{} / {}", p.category, p.name);
            a.end_paint();
        }
    }

    #[test]
    fn a_click_then_drag_equals_a_drag() {
        let o = ToolOptions {
            stroke_size: 20,
            ..options()
        };
        let image = |flush_first: bool| {
            let mut d = doc();
            d.begin_paint(PaintTool::Brush, 20.0, 20.0, &o);
            if flush_first {
                d.flush_paint();
            }
            d.extend_paint(60.0, 40.0, false);
            d.end_paint();
            bitmap(&d, d.selection[0]).image
        };
        assert_eq!(image(true), image(false));
    }

    #[test]
    fn live_eraser_does_not_depend_on_how_it_is_updated() {
        let pts = scribble();
        let run = |every: usize| {
            let mut o = options();
            o.paint.eraser_size = 21;
            o.paint.eraser_edge = 40;
            let mut d = DocState::new(Document::new(240, 180, 72.0, Some(Rgba::WHITE)));
            let r = add_bitmap(&mut d, 0, 0, 240, 180, RED);
            d.begin_paint(PaintTool::Eraser, pts[0].0, pts[0].1, &o);
            for (n, q) in pts.iter().enumerate().skip(1) {
                d.extend_paint(q.0, q.1, false);
                if n % every == 0 {
                    d.flush_paint();
                }
            }
            d.end_paint();
            bitmap(&d, r).image
        };
        assert_eq!(run(1), run(1000));
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

    /// What a single click at x = 53 leaves of an opaque bitmap, along the row through the dab.
    fn dab_row(round: bool, size: u32, edge: u8, from: i64, to: i64) -> Vec<u8> {
        let mut d = doc();
        let mut o = options();
        o.paint.eraser_size = size;
        o.paint.eraser_round = round;
        o.paint.eraser_edge = edge;
        let r = add_bitmap(&mut d, 0, 0, 100, 80, RED);
        d.begin_paint(PaintTool::Eraser, 53.0, 40.0, &o);
        d.end_paint();
        let b = bitmap(&d, r);
        (from..to)
            .map(|x| b.image.get(x, 40).map_or(0, |p| p.a))
            .collect()
    }

    /// What a stroke along row 40 leaves in the rows from `from`.
    fn stroke_rows(round: bool, edge: u8, from: i64, to: i64) -> Vec<u8> {
        let mut d = doc();
        let mut o = options();
        o.paint.eraser_round = round;
        o.paint.eraser_edge = edge;
        let r = add_bitmap(&mut d, 0, 0, 100, 80, RED);
        line(&mut d, PaintTool::Eraser, (20.0, 40.0), (80.0, 40.0), &o);
        let b = bitmap(&d, r);
        (from..to)
            .map(|y| b.image.get(50, y).map_or(0, |p| p.a))
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
    fn square_eraser_dab_matches_fireworks_mx() {
        // Alpha left along the centre row, measured in Fireworks MX, from x = 46.
        let row = |size, edge, n: usize, want: &[u8]| {
            assert_close(&dab_row(false, size, edge, 46, 46 + n as i64), want, 1);
        };
        row(13, 10, 3, &[0xFF, 0x45, 0]);
        row(13, 25, 3, &[0xFF, 0xB0, 0x13]);
        row(13, 50, 5, &[0xFF, 0xD7, 0x89, 0x3A, 0]);
        row(13, 75, 7, &[0xFF, 0xE4, 0xB0, 0x7C, 0x47, 0x13, 0]);
        row(13, 100, 8, &[0xFF, 0xEB, 0xC4, 0x9C, 0x75, 0x4E, 0x27, 0]);
        row(13, 0, 3, &[0xFF, 0, 0]);
        // Size 30, centred between columns 53 and 54.
        assert_close(&dab_row(false, 30, 50, 38, 42), &[0xFF, 0xEE, 0xCC, 0xAA], 1);
        assert_close(&dab_row(false, 30, 100, 52, 56), &[0x19, 0x08, 0x08, 0x19], 1);
    }

    #[test]
    fn round_eraser_dab_matches_fireworks_mx() {
        // The far side has one more cell (x = 60 for size 13).
        let row = |edge, want: &[u8]| {
            assert_close(&dab_row(true, 13, edge, 46, 54), want, 1);
            assert_close(&dab_row(true, 13, edge, 60, 61), &[tail(edge)], 1);
        };
        fn tail(edge: u8) -> u8 {
            match edge {
                2 => 0xF6,
                10 => 0xF7,
                25 => 0xF8,
                50 => 0xF9,
                _ => 0xFA,
            }
        }
        row(2, &[0xFF, 0xA9, 0, 0, 0, 0, 0, 0]);
        row(10, &[0xFF, 0xBE, 0, 0, 0, 0, 0, 0]);
        row(25, &[0xFF, 0xD5, 0x43, 0, 0, 0, 0, 0]);
        row(50, &[0xFF, 0xE5, 0xA9, 0x16, 0, 0, 0, 0]);
        row(75, &[0xFF, 0xED, 0xCC, 0x8A, 0x1E, 0, 0, 0]);
        row(100, &[0xFF, 0xF1, 0xDD, 0xB8, 0x82, 0x46, 0x12, 0]);
        // Even sizes centre between columns 52 and 53.
        assert_close(
            &dab_row(true, 30, 100, 50, 56),
            &[0x1A, 0x09, 0, 0, 0x09, 0x1A],
            1,
        );
        assert_close(&dab_row(true, 7, 50, 49, 53), &[0xFF, 0xC9, 0x35, 0], 1);
        assert_close(&dab_row(true, 60, 100, 47, 52), &[0x22, 0x16, 0x0E, 0x05, 0], 1);
    }

    #[test]
    fn eraser_strokes_match_fireworks_mx() {
        // Rows above and below a stroke along row 40 (size 13), measured in Fireworks MX.
        assert_close(&stroke_rows(false, 100, 33, 37), &[0xFF, 0x58, 0x0C, 0x02], 3);
        assert_close(&stroke_rows(false, 50, 33, 36), &[0xFF, 0x1D, 0x01], 3);
        assert_close(&stroke_rows(false, 0, 33, 36), &[0xFF, 0, 0], 0);
        assert_close(&stroke_rows(true, 2, 33, 36), &[0xFF, 0x31, 0], 3);
        assert_close(&stroke_rows(true, 2, 46, 49), &[0x31, 0xDF, 0xFF], 3);
        assert_close(&stroke_rows(true, 50, 33, 36), &[0xFF, 0x96, 0x20], 3);
        assert_close(&stroke_rows(true, 50, 45, 48), &[0x20, 0x96, 0xE7], 3);
        assert_close(
            &stroke_rows(true, 100, 34, 39),
            &[0xB8, 0x6D, 0x25, 0x07, 0x01],
            3,
        );
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
