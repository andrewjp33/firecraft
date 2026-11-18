//! Window > Align: Fireworks MX's Align panel (a Flash panel shipped with MX that drives the
//! document through Fireworks' script API). Align, Distribute, Match Size and Space for the
//! selected objects, each against the selection's bounds or, with To Canvas, the canvas; and in
//! Anchors mode, lining up the selected points of a path. Measured in Fireworks MX under Wine
//! (docs/behavior/align.md).

use firecraft_doc::{Element, ElementKind};
use firecraft_raster::transform::Affine;

use crate::paths::PointRef;
use crate::{Align, DocState, ElementRef};

/// An edge or centre line of the Align panel's Align and Distribute rows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    /// Centres on one vertical line ("Align horizontal center").
    HorizCenter,
    Right,
    Top,
    /// Centres on one horizontal line ("Align vertical center").
    VertCenter,
    Bottom,
}

impl Edge {
    pub const ALL: [Self; 6] = [
        Self::Left,
        Self::HorizCenter,
        Self::Right,
        Self::Top,
        Self::VertCenter,
        Self::Bottom,
    ];

    fn horizontal(self) -> bool {
        matches!(self, Self::Left | Self::HorizCenter | Self::Right)
    }

    /// The coordinate of this edge of a box (x, y, w, h).
    fn of(self, b: (f64, f64, f64, f64)) -> f64 {
        let (x, y, w, h) = b;
        match self {
            Self::Left => x,
            Self::HorizCenter => x + w / 2.0,
            Self::Right => x + w,
            Self::Top => y,
            Self::VertCenter => y + h / 2.0,
            Self::Bottom => y + h,
        }
    }

    /// Where a box of size (`w`, `h`) starts (left or top) to put this edge at `at`.
    fn start_for(self, at: f64, w: f64, h: f64) -> f64 {
        match self {
            Self::Left | Self::Top => at,
            Self::HorizCenter => at - w / 2.0,
            Self::VertCenter => at - h / 2.0,
            Self::Right => at - w,
            Self::Bottom => at - h,
        }
    }
}

/// The Match Size row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchSize {
    Width,
    Height,
    Both,
}

/// The Space row: equal gaps between the objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Space {
    Vertical,
    Horizontal,
}

/// Anchors mode: which part of each selected point lines up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PointPart {
    /// The point itself, its handles moving with it.
    #[default]
    Anchor,
    /// Only the incoming (preceding) handles.
    Preceding,
    /// Only the outgoing (succeeding) handles.
    Succeeding,
}

type Bounds = (f64, f64, f64, f64);

impl DocState {
    /// The selected objects with their bounds, in selection order.
    fn panel_items(&self) -> Vec<(ElementRef, Bounds)> {
        let mut seen = Vec::new();
        self.selection
            .iter()
            .filter(|r| {
                let new = !seen.contains(*r);
                seen.push(**r);
                new
            })
            .filter_map(|r| Some((*r, tidy_bounds(self.element(*r)?.bounds()?))))
            .collect()
    }

    /// What the panel lines objects up against: the canvas, or the selection's bounds.
    fn panel_reference(&self, items: &[(ElementRef, Bounds)], to_canvas: bool) -> Bounds {
        if to_canvas {
            (0.0, 0.0, self.doc.width as f64, self.doc.height as f64)
        } else {
            union(items.iter().map(|(_, b)| *b))
        }
    }

    /// Where `r`'s top-left corner ends up when moved to (`x`, `y`), as Fireworks'
    /// `moveSelectionTo` places it: paths keep fractional positions, bitmaps go to the nearest
    /// whole pixel (halves round up: measured 152.5 -> 153, 10.5 -> 11, 82.5 -> 83).
    fn landing(&self, r: ElementRef, x: f64, y: f64) -> (f64, f64) {
        let whole = self.element(r).is_some_and(|e| {
            matches!(
                e.kind,
                ElementKind::Bitmap(_) | ElementKind::Other { cached: Some(_), .. }
            )
        });
        if whole {
            ((x + 0.5).floor(), (y + 0.5).floor())
        } else {
            (x, y)
        }
    }

    /// Move each object to its new top-left corner, as one undo step (none when nothing moves).
    fn panel_moves(&mut self, label: &str, moves: Vec<(ElementRef, Bounds, f64, f64)>) {
        let moves: Vec<(ElementRef, f64, f64)> = moves
            .into_iter()
            .map(|(r, b, x, y)| {
                let (x, y) = self.landing(r, x, y);
                (r, x - b.0, y - b.1)
            })
            .filter(|(_, dx, dy)| *dx != 0.0 || *dy != 0.0)
            .collect();
        if moves.is_empty() {
            return;
        }
        self.edit(label, |s| {
            for (r, dx, dy) in moves {
                if let Some(e) = s.element_mut(r) {
                    e.translate(dx, dy);
                }
            }
        });
    }

    /// Align panel > Align: every selected object puts its `edge` on the same edge of the
    /// selection's bounds (or the canvas). Unlike Modify > Align, one object on its own lines up
    /// with the canvas only with To Canvas on.
    pub fn panel_align(&mut self, edge: Edge, to_canvas: bool) {
        let items = self.panel_items();
        if items.is_empty() {
            return;
        }
        let refb = self.panel_reference(&items, to_canvas);
        let at = edge.of(refb);
        let moves = items
            .into_iter()
            .map(|(r, b)| {
                let v = edge.start_for(at, b.2, b.3);
                if edge.horizontal() { (r, b, v, b.1) } else { (r, b, b.0, v) }
            })
            .collect();
        self.panel_moves("Align", moves);
    }

    /// Align panel > Distribute: the objects' `edge`s evenly spaced, in the order of that edge.
    /// Without To Canvas the first and last objects stay and the others are spread between
    /// them; with To Canvas the first goes to the canvas's start and the last to its end (one
    /// object goes to the start).
    pub fn panel_distribute(&mut self, edge: Edge, to_canvas: bool) {
        let mut items = self.panel_items();
        let (Some(_), n) = (items.first(), items.len()) else {
            return;
        };
        items.sort_by(|a, b| edge.of(a.1).total_cmp(&edge.of(b.1)));
        let refb = self.panel_reference(&items, to_canvas);
        let (f, l) = (items[0].1, items[n - 1].1);
        let (rx, ry, rw, rh) = refb;
        let (start, span) = if to_canvas {
            match edge {
                Edge::Left => (0.0, rx + rw - l.2),
                Edge::Top => (0.0, ry + rh - l.3),
                Edge::Right => (rx + f.2, rx + rw - (rx + f.2)),
                Edge::Bottom => (ry + f.3, ry + rh - (ry + f.3)),
                Edge::HorizCenter => (rx + f.2 / 2.0, rx + rw - l.2 / 2.0 - (rx + f.2 / 2.0)),
                Edge::VertCenter => (ry + f.3 / 2.0, ry + rh - l.3 / 2.0 - (ry + f.3 / 2.0)),
            }
        } else {
            (edge.of(f), edge.of(l) - edge.of(f))
        };
        let step = span / (if n == 1 { 1 } else { n - 1 }) as f64;
        let moves = items
            .into_iter()
            .enumerate()
            .map(|(i, (r, b))| {
                let v = edge.start_for(start + i as f64 * step, b.2, b.3);
                if edge.horizontal() { (r, b, v, b.1) } else { (r, b, b.0, v) }
            })
            .collect();
        self.panel_moves("Distribute", moves);
    }

    /// Align panel > Match Size: every selected object scaled to the widest width and/or the
    /// tallest height among them (with To Canvas, the canvas's size if that is larger), its
    /// top-left corner staying where it is. Bitmaps are resampled with the Interpolation
    /// preference.
    pub fn match_size(&mut self, how: MatchSize, to_canvas: bool) {
        let items = self.panel_items();
        if items.is_empty() {
            return;
        }
        let mut mw = items.iter().map(|(_, b)| b.2).fold(0.0, f64::max);
        let mut mh = items.iter().map(|(_, b)| b.3).fold(0.0, f64::max);
        if to_canvas {
            mw = mw.max(self.doc.width as f64);
            mh = mh.max(self.doc.height as f64);
        }
        let (do_w, do_h) = match how {
            MatchSize::Width => (true, false),
            MatchSize::Height => (false, true),
            MatchSize::Both => (true, true),
        };
        let method = self.interpolation;
        let mut changes: Vec<(ElementRef, Element)> = Vec::new();
        for (r, (x, y, w, h)) in items {
            // A line has no width (or height) to scale.
            let sx = if do_w && w > 0.0 { mw / w } else { 1.0 };
            let sy = if do_h && h > 0.0 { mh / h } else { 1.0 };
            if sx == 1.0 && sy == 1.0 {
                continue;
            }
            let m = Affine::translate(x, y)
                .then(&Affine {
                    a: sx,
                    d: sy,
                    ..Affine::IDENTITY
                })
                .then(&Affine::translate(-x, -y));
            let Some(mut e) = self.element(r).cloned() else {
                continue;
            };
            e.transform_with(&m, method);
            if let ElementKind::Text(text) = &mut e.kind
                && text.edited
            {
                crate::text::refresh(text);
            }
            changes.push((r, e));
        }
        if changes.is_empty() {
            return;
        }
        self.edit("Match Size", |s| {
            for (r, e) in changes {
                if let Some(slot) = s.element_mut(r) {
                    *slot = e;
                }
            }
        });
    }

    /// Align panel > Space: equal gaps between the objects. Without To Canvas this is Modify >
    /// Align > Distribute Widths/Heights; with To Canvas the objects (in order of their left or
    /// top edges) fill the canvas from edge to edge with equal gaps (one object goes to the
    /// start).
    pub fn space(&mut self, how: Space, to_canvas: bool) {
        if !to_canvas {
            self.align(match how {
                Space::Horizontal => Align::DistributeWidths,
                Space::Vertical => Align::DistributeHeights,
            });
            return;
        }
        let horizontal = how == Space::Horizontal;
        let mut items = self.panel_items();
        let n = items.len();
        if n == 0 {
            return;
        }
        let size = |b: &Bounds| if horizontal { b.2 } else { b.3 };
        let pos = |b: &Bounds| if horizontal { b.0 } else { b.1 };
        items.sort_by(|a, b| pos(&a.1).total_cmp(&pos(&b.1)));
        let total: f64 = items.iter().map(|(_, b)| size(b)).sum();
        let span = if horizontal {
            self.doc.width as f64
        } else {
            self.doc.height as f64
        };
        let gap = (span - total) / (if n == 1 { 1 } else { n - 1 }) as f64;
        // Each object follows where the previous one actually ended up (a bitmap moved to a
        // whole pixel).
        let mut at = 0.0;
        let mut moves = Vec::new();
        for (r, b) in items {
            let (x, y) = if horizontal { (at, b.1) } else { (b.0, at) };
            let (lx, ly) = self.landing(r, x, y);
            at = if horizontal { lx } else { ly } + size(&b) + gap;
            moves.push((r, b, x, y));
        }
        self.panel_moves("Space", moves);
    }

    /// Align panel in Anchors mode: line up the selected points `pts` of path `r` (two or more).
    /// Left/Right/Top/Bottom use the outermost point (for handle parts, the point whose handle
    /// is outermost, and that handle's position); the centres use the middle between the
    /// outermost points' anchors, for handles too (as Fireworks MX's panel does).
    pub fn align_points(&mut self, r: ElementRef, pts: &[PointRef], edge: Edge, part: PointPart) {
        let Some(p) = self.path(r) else {
            return;
        };
        let get = |q: &PointRef| p.contours.get(q.contour)?.points.get(q.index).copied();
        let sel: Vec<_> = pts.iter().filter_map(get).collect();
        if sel.len() < 2 {
            return;
        }
        let horizontal = edge.horizontal();
        // A point's coordinate for sorting (the anchor or one of its handles), absolute.
        let key = |b: &firecraft_doc::BezierPoint| {
            let (ax, ay) = (b.x, b.y);
            let (ox, oy) = match part {
                PointPart::Anchor => (0.0, 0.0),
                PointPart::Preceding => b.pred,
                PointPart::Succeeding => b.succ,
            };
            if horizontal { ax + ox } else { ay + oy }
        };
        let anchor = |b: &firecraft_doc::BezierPoint| if horizontal { b.x } else { b.y };
        let min = sel.iter().min_by(|a, b| key(a).total_cmp(&key(b)));
        let max = sel.iter().max_by(|a, b| key(a).total_cmp(&key(b)));
        let (Some(min), Some(max)) = (min, max) else {
            return;
        };
        // The target for the anchor, and for the chosen handle (absolute).
        let (to_anchor, to_handle) = match edge {
            Edge::Left | Edge::Top => (anchor(min), key(min)),
            Edge::Right | Edge::Bottom => (anchor(max), key(max)),
            Edge::HorizCenter | Edge::VertCenter => {
                let c = anchor(min) + (anchor(max) - anchor(min)) / 2.0;
                (c, c)
            }
        };
        let pts = pts.to_vec();
        let before = p.clone();
        let mut after = before.clone();
        for q in &pts {
            let Some(b) = after
                .contours
                .get_mut(q.contour)
                .and_then(|c| c.points.get_mut(q.index))
            else {
                continue;
            };
            let (a, h) = match part {
                PointPart::Anchor => (if horizontal { &mut b.x } else { &mut b.y }, None),
                PointPart::Preceding => {
                    let base = if horizontal { b.x } else { b.y };
                    (
                        if horizontal { &mut b.pred.0 } else { &mut b.pred.1 },
                        Some(base),
                    )
                }
                PointPart::Succeeding => {
                    let base = if horizontal { b.x } else { b.y };
                    (
                        if horizontal { &mut b.succ.0 } else { &mut b.succ.1 },
                        Some(base),
                    )
                }
            };
            match h {
                // Handles are kept relative to their point: the point moves, they follow.
                None => *a = to_anchor,
                Some(base) => *a = to_handle - base,
            }
        }
        if after.contours == before.contours {
            return;
        }
        after.rect = None;
        self.path_edit(r, "Align Points", |p| *p = after);
    }
}

/// Bounds with the noise of flattening curves removed (a corner at 20 can come back as
/// 19.999999999999993), so halves round as they should and objects already in place don't move.
pub(crate) fn tidy_bounds(b: Bounds) -> Bounds {
    let t = |v: f64| (v * 1e6).round() / 1e6;
    (t(b.0), t(b.1), t(b.2), t(b.3))
}

fn union(mut it: impl Iterator<Item = Bounds>) -> Bounds {
    let Some(first) = it.next() else {
        return (0.0, 0.0, 0.0, 0.0);
    };
    let (x0, y0, x1, y1) = it.fold(
        (first.0, first.1, first.0 + first.2, first.1 + first.3),
        |a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.0 + b.2), a.3.max(b.1 + b.3)),
    );
    (x0, y0, x1 - x0, y1 - y0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use firecraft_doc::{BezierPoint, Bitmap, Contour, Document, Path};
    use firecraft_raster::{Rgba, RgbaImage};

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Element {
        let c = |x, y| BezierPoint::corner(x, y);
        Element::new(ElementKind::Path(Path {
            contours: vec![Contour {
                points: vec![c(x, y), c(x + w, y), c(x + w, y + h), c(x, y + h)],
                closed: true,
            }],
            ..Path::default()
        }))
    }

    fn bitmap(x: i64, y: i64, w: u32, h: u32) -> Element {
        Element::new(ElementKind::Bitmap(Bitmap {
            x,
            y,
            image: RgbaImage::new(w, h, Rgba::new(200, 0, 0, 255)),
        }))
    }

    /// A 400x300 document with `els` (bottom first), all selected.
    fn doc(els: Vec<Element>) -> DocState {
        let mut d = DocState::new(Document::new(400, 300, 72.0, None));
        let n = els.len();
        for e in els {
            d.add_element(e, "Add");
        }
        d.selection = (0..n).map(|i| ElementRef::new(0, i)).collect();
        d
    }

    fn boxes(d: &DocState) -> Vec<(f64, f64, f64, f64)> {
        d.doc.layers[0]
            .elements(0)
            .iter()
            .filter_map(|e| e.bounds().map(tidy_bounds))
            .collect()
    }

    /// Measured in Fireworks MX (2026-11-18): three rectangles, then a bitmap with two.
    #[test]
    fn align_horizontal_center_like_fireworks() {
        let mut d = doc(vec![
            rect(20.0, 30.0, 60.0, 40.0),
            rect(150.0, 100.0, 41.0, 75.0),
            rect(260.0, 60.0, 85.0, 33.0),
        ]);
        d.panel_align(Edge::HorizCenter, false);
        let xs: Vec<f64> = boxes(&d).iter().map(|b| b.0).collect();
        assert_eq!(xs, [152.5, 162.0, 140.0]);
        assert_eq!(d.history.next_undo(), Some("Align"));

        // A bitmap goes to the nearest whole pixel, halves up (152.5 -> 153; 10.5 -> 11).
        let mut d = doc(vec![
            bitmap(20, 30, 60, 40),
            rect(150.0, 100.0, 41.0, 75.0),
            rect(260.0, 60.0, 85.0, 33.0),
        ]);
        d.panel_align(Edge::HorizCenter, false);
        assert_eq!(boxes(&d)[0].0, 153.0);
        let mut d = doc(vec![bitmap(20, 30, 60, 40), bitmap(0, 100, 81, 75)]);
        d.panel_align(Edge::HorizCenter, false);
        assert_eq!(boxes(&d)[0].0, 11.0);
        d.panel_align(Edge::VertCenter, false);
        assert_eq!(boxes(&d).iter().map(|b| b.1).collect::<Vec<_>>(), [83.0, 65.0]);
    }

    #[test]
    fn one_object_aligns_only_to_the_canvas() {
        let mut d = doc(vec![rect(20.0, 30.0, 60.0, 40.0)]);
        d.panel_align(Edge::Right, false);
        assert_eq!(boxes(&d)[0].0, 20.0);
        assert!(d.history.next_undo() != Some("Align"));
        d.panel_align(Edge::Right, true);
        assert_eq!(boxes(&d)[0].0, 340.0);
        d.panel_align(Edge::VertCenter, true);
        assert_eq!(boxes(&d)[0].1, 130.0);
    }

    #[test]
    fn match_size_keeps_the_top_left_corner() {
        let mut d = doc(vec![
            bitmap(153, 30, 60, 40),
            rect(162.0, 100.0, 41.0, 75.0),
            rect(140.0, 60.0, 85.0, 33.0),
        ]);
        d.match_size(MatchSize::Width, false);
        assert_eq!(
            boxes(&d),
            [
                (153.0, 30.0, 85.0, 40.0),
                (162.0, 100.0, 85.0, 75.0),
                (140.0, 60.0, 85.0, 33.0)
            ]
        );
        assert_eq!(d.history.next_undo(), Some("Match Size"));
        d.match_size(MatchSize::Height, true);
        assert!(boxes(&d).iter().all(|b| b.3 == 300.0));
    }

    /// Fireworks MX, To Canvas on: Distribute left edges, then Space vertically.
    #[test]
    fn distribute_and_space_to_canvas() {
        let mut d = doc(vec![
            bitmap(153, 30, 85, 40),
            rect(162.0, 100.0, 85.0, 75.0),
            rect(140.0, 60.0, 85.0, 33.0),
        ]);
        d.panel_distribute(Edge::Left, true);
        let xs: Vec<f64> = boxes(&d).iter().map(|b| b.0).collect();
        assert_eq!(xs, [158.0, 315.0, 0.0]);
        d.space(Space::Vertical, true);
        let ys: Vec<f64> = boxes(&d).iter().map(|b| b.1).collect();
        assert_eq!(ys, [0.0, 225.0, 116.0]);
    }

    #[test]
    fn distribute_between_the_outer_objects() {
        let mut d = doc(vec![
            rect(0.0, 0.0, 10.0, 10.0),
            rect(30.0, 20.0, 10.0, 10.0),
            rect(200.0, 40.0, 30.0, 10.0),
        ]);
        d.panel_distribute(Edge::Right, false);
        // Right edges 10 .. 230: the middle one's at 120.
        assert_eq!(boxes(&d)[1].0, 110.0);
        d.panel_distribute(Edge::HorizCenter, false);
        assert_eq!(boxes(&d)[1].0, 105.0);
    }

    /// Modify > Align > Distribute Widths (and Space without To Canvas), measured in Fireworks
    /// MX: equal gaps in the order of the centres, the objects between on whole pixels.
    #[test]
    fn space_without_canvas_is_distribute_widths() {
        let mut d = doc(vec![
            rect(0.0, 0.0, 10.0, 10.0),
            rect(50.0, 20.0, 21.0, 10.0),
            rect(200.0, 40.0, 10.0, 10.0),
        ]);
        d.space(Space::Horizontal, false);
        assert_eq!(boxes(&d)[1].0, 94.0);
        let mut d = doc(vec![
            rect(0.0, 0.0, 10.0, 10.0),
            rect(50.0, 20.0, 40.0, 10.0),
            rect(60.0, 40.0, 10.0, 10.0),
            rect(200.0, 60.0, 20.0, 10.0),
        ]);
        d.space(Space::Horizontal, false);
        let xs: Vec<f64> = boxes(&d).iter().map(|b| b.0).collect();
        assert_eq!(xs, [0.0, 113.0, 56.0, 200.0]);
    }

    #[test]
    fn align_points_and_handles() {
        let mut p = rect(0.0, 0.0, 100.0, 50.0);
        if let ElementKind::Path(path) = &mut p.kind {
            path.contours[0].points[1].pred = (-10.0, 0.0);
            path.contours[0].points[2].succ = (5.0, 7.0);
        }
        let mut d = doc(vec![p]);
        let r = ElementRef::new(0, 0);
        let pts = [
            PointRef { contour: 0, index: 1 },
            PointRef { contour: 0, index: 3 },
        ];
        let point = |d: &DocState, i: usize| d.path(r).expect("path").contours[0].points[i];
        // Left: both points go to x = 0 (the leftmost), handles move along.
        d.align_points(r, &pts, Edge::Left, PointPart::Anchor);
        assert_eq!((point(&d, 1).x, point(&d, 1).pred), (0.0, (-10.0, 0.0)));
        assert_eq!(point(&d, 3).x, 0.0);
        assert!(d.path(r).expect("path").rect.is_none());
        d.undo();
        // Vertical centre: halfway between the anchors (y 0 and 50).
        d.align_points(r, &pts, Edge::VertCenter, PointPart::Anchor);
        assert_eq!((point(&d, 1).y, point(&d, 3).y), (25.0, 25.0));
        d.undo();
        // Outgoing handles to the bottom-most outgoing handle (point 2's at y 57).
        let pts = [
            PointRef { contour: 0, index: 1 },
            PointRef { contour: 0, index: 2 },
        ];
        d.align_points(r, &pts, Edge::Bottom, PointPart::Succeeding);
        assert_eq!(point(&d, 1).succ, (0.0, 57.0));
        assert_eq!(point(&d, 2).succ, (5.0, 7.0));
        assert_eq!(point(&d, 1).y, 0.0);
        assert_eq!(d.history.next_undo(), Some("Align Points"));
        // One point alone does nothing.
        let before = d.path(r).cloned();
        d.align_points(r, &pts[..1], Edge::Top, PointPart::Anchor);
        assert_eq!(d.path(r).cloned(), before);
    }
}
