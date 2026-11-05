//! Path editing for the Pen and Subselection tools (Fireworks MX): points of a path, finding
//! the segment under the pointer, adding a point on a segment without changing its shape,
//! deleting points, corner/curve points, and drawing new paths point by point.
//!
//! Editing a path's points makes a rectangle primitive a plain path (`Path::rect` = `None`), as
//! Fireworks does when a rectangle's points are edited.

use firecraft_doc::{BezierPoint, Contour, Element, ElementKind, Fill, Path};

use crate::{DocState, ElementRef, ToolOptions};

/// A point of a path: its contour and index in that contour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PointRef {
    pub contour: usize,
    pub index: usize,
}

type Pt = (f64, f64);

/// The cubic of segment `seg` of contour `c` (from point `seg` to the next), absolute.
pub fn segment(c: &Contour, seg: usize) -> Option<[Pt; 4]> {
    let n = c.points.len();
    let a = c.points.get(seg)?;
    let b = c.points.get((seg + 1) % n.max(1))?;
    Some([
        (a.x, a.y),
        (a.x + a.succ.0, a.y + a.succ.1),
        (b.x + b.pred.0, b.y + b.pred.1),
        (b.x, b.y),
    ])
}

fn cubic_at(q: &[Pt; 4], t: f64) -> Pt {
    let u = 1.0 - t;
    let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    (
        a * q[0].0 + b * q[1].0 + c * q[2].0 + d * q[3].0,
        a * q[0].1 + b * q[1].1 + c * q[2].1 + d * q[3].1,
    )
}

/// The number of segments of a contour.
pub fn segment_count(c: &Contour) -> usize {
    let n = c.points.len();
    if c.closed && n > 1 { n } else { n.saturating_sub(1) }
}

/// The segment nearest to (`x`, `y`) within `tol`: (contour, segment, t, distance).
pub fn nearest_segment(p: &Path, x: f64, y: f64, tol: f64) -> Option<(usize, usize, f64, f64)> {
    let mut best: Option<(usize, usize, f64, f64)> = None;
    for (ci, c) in p.contours.iter().enumerate() {
        for s in 0..segment_count(c) {
            let Some(q) = segment(c, s) else { continue };
            // Coarse samples, then refine around the best one.
            let steps = 48;
            let dist = |t: f64| {
                let (px, py) = cubic_at(&q, t);
                ((px - x).powi(2) + (py - y).powi(2)).sqrt()
            };
            let mut bt = 0.0;
            let mut bd = f64::MAX;
            for k in 0..=steps {
                let t = k as f64 / steps as f64;
                let d = dist(t);
                if d < bd {
                    (bt, bd) = (t, d);
                }
            }
            let mut step = 1.0 / steps as f64;
            for _ in 0..20 {
                step /= 2.0;
                for t in [bt - step, bt + step] {
                    let t = t.clamp(0.0, 1.0);
                    let d = dist(t);
                    if d < bd {
                        (bt, bd) = (t, d);
                    }
                }
            }
            if bd <= tol && best.is_none_or(|b| bd < b.3) {
                best = Some((ci, s, bt, bd));
            }
        }
    }
    best
}

/// Add a point on segment `seg` at parameter `t` without changing the curve (de Casteljau);
/// the new point's index.
pub fn split_segment(c: &mut Contour, seg: usize, t: f64) -> Option<usize> {
    let q = segment(c, seg)?;
    // A straight segment gets a corner point (the segment stays a straight line when it moves).
    if q[1] == q[0] && q[2] == q[3] {
        let at = seg + 1;
        let (x, y) = (q[0].0 + (q[3].0 - q[0].0) * t, q[0].1 + (q[3].1 - q[0].1) * t);
        c.points.insert(at, BezierPoint::corner(x, y));
        return Some(at);
    }

    let lerp = |a: Pt, b: Pt| (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
    let (p01, p12, p23) = (lerp(q[0], q[1]), lerp(q[1], q[2]), lerp(q[2], q[3]));
    let (p012, p123) = (lerp(p01, p12), lerp(p12, p23));
    let m = lerp(p012, p123);
    let n = c.points.len();
    let next = (seg + 1) % n;
    if let Some(a) = c.points.get_mut(seg) {
        a.succ = (p01.0 - a.x, p01.1 - a.y);
    }
    if let Some(b) = c.points.get_mut(next) {
        b.pred = (p23.0 - b.x, p23.1 - b.y);
    }
    let point = BezierPoint {
        x: m.0,
        y: m.1,
        pred: (p012.0 - m.0, p012.1 - m.1),
        succ: (p123.0 - m.0, p123.1 - m.1),
    };
    // Splitting the closing segment of a closed contour appends after the last point.
    let at = seg + 1;
    c.points.insert(at, point);
    Some(at)
}

/// Reverse a contour's direction (its points and their handles).
pub fn reverse(c: &mut Contour) {
    c.points.reverse();
    for p in &mut c.points {
        std::mem::swap(&mut p.pred, &mut p.succ);
    }
}

/// Whether a point has curve handles.
pub fn is_curve(p: &BezierPoint) -> bool {
    p.pred != (0.0, 0.0) || p.succ != (0.0, 0.0)
}

/// Remove points from a path; contours left with fewer than two points go (a closed contour
/// with two points stays closed). Returns whether the path still has points.
pub fn remove_points(p: &mut Path, pts: &[PointRef]) -> bool {
    let mut pts = pts.to_vec();
    pts.sort();
    for r in pts.iter().rev() {
        if let Some(c) = p.contours.get_mut(r.contour)
            && r.index < c.points.len()
        {
            c.points.remove(r.index);
        }
    }
    p.contours.retain(|c| c.points.len() >= 2);
    p.rect = None;
    !p.contours.is_empty()
}

impl DocState {
    /// The path object `r`, if it is one.
    pub fn path(&self, r: ElementRef) -> Option<&Path> {
        match self.element(r).map(|e| &e.kind) {
            Some(ElementKind::Path(p)) => Some(p),
            _ => None,
        }
    }

    /// Change path `r` without an undo step of its own (part of an edit or live edit already
    /// begun); the canvas redraws.
    pub fn path_live(&mut self, r: ElementRef, f: impl FnOnce(&mut Path)) {
        if let Some(Element {
            kind: ElementKind::Path(p),
            ..
        }) = self.element_mut(r)
        {
            f(p);
        }
        self.touch();
    }

    /// Change path `r` as one undo step.
    pub fn path_edit(&mut self, r: ElementRef, label: &str, f: impl FnOnce(&mut Path)) {
        if self.path(r).is_none() {
            return;
        }
        self.edit(label, |s| {
            if let Some(Element {
                kind: ElementKind::Path(p),
                ..
            }) = s.element_mut(r)
            {
                f(p);
            }
        });
    }

    /// Pen tool: start a new open path with one point at (`x`, `y`), using the tool's fill and
    /// stroke (as Fireworks' Pen does); it becomes the selection.
    pub fn pen_start(&mut self, x: f64, y: f64, opts: &ToolOptions) -> Option<ElementRef> {
        let path = Path {
            contours: vec![Contour {
                points: vec![BezierPoint::corner(x, y)],
                closed: false,
            }],
            fill: opts.fill.map(|color| Fill {
                color,
                category: "fc_Solid".into(),
                anti_alias: true,
                gradient: None,
            }),
            stroke: Some(opts.new_stroke(opts.stroke.unwrap_or(firecraft_raster::Rgba::BLACK))),
            rect: None,
            pcl: None,
            texture_origin: None,
        };
        self.add_element(Element::new(ElementKind::Path(path)), "Pen");
        self.selection.first().copied()
    }

    /// Pen tool: add a corner point at the end of open contour `contour`; its reference.
    pub fn pen_append(&mut self, r: ElementRef, contour: usize, x: f64, y: f64) -> Option<PointRef> {
        let n = self.path(r)?.contours.get(contour)?.points.len();
        self.path_edit(r, "Pen", |p| {
            if let Some(c) = p.contours.get_mut(contour) {
                c.points.push(BezierPoint::corner(x, y));
            }
            p.rect = None;
        });
        Some(PointRef { contour, index: n })
    }

    /// Pen tool: close open contour `contour` (clicking its first point).
    pub fn pen_close(&mut self, r: ElementRef, contour: usize) {
        self.path_edit(r, "Close Path", |p| {
            if let Some(c) = p.contours.get_mut(contour) {
                c.closed = true;
            }
        });
    }

    /// Pen tool on a path: add a point on segment `seg` of `contour` at `t`.
    pub fn insert_point(&mut self, r: ElementRef, contour: usize, seg: usize, t: f64) -> Option<PointRef> {
        let mut out = None;
        self.path_edit(r, "Add Point", |p| {
            if let Some(c) = p.contours.get_mut(contour) {
                out = split_segment(c, seg, t).map(|index| PointRef { contour, index });
            }
            p.rect = None;
        });
        out
    }

    /// Pen tool on a curve point: make it a corner (no handles).
    pub fn make_corner(&mut self, r: ElementRef, pt: PointRef) {
        self.path_edit(r, "Convert Point", |p| {
            if let Some(q) = p
                .contours
                .get_mut(pt.contour)
                .and_then(|c| c.points.get_mut(pt.index))
            {
                q.pred = (0.0, 0.0);
                q.succ = (0.0, 0.0);
            }
            p.rect = None;
        });
    }

    /// Delete points of path `r` (Subselection + Delete, or the Pen on a corner point). A path
    /// left without points is deleted.
    pub fn delete_points(&mut self, r: ElementRef, pts: &[PointRef]) {
        let Some(mut p) = self.path(r).cloned() else {
            return;
        };
        if remove_points(&mut p, pts) {
            self.path_edit(r, "Delete Points", |q| *q = p);
        } else {
            self.selection = vec![r];
            self.delete();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve() -> Contour {
        Contour {
            points: vec![
                BezierPoint {
                    x: 0.0,
                    y: 0.0,
                    pred: (0.0, 0.0),
                    succ: (30.0, -40.0),
                },
                BezierPoint {
                    x: 100.0,
                    y: 0.0,
                    pred: (-20.0, 50.0),
                    succ: (0.0, 0.0),
                },
            ],
            closed: false,
        }
    }

    #[test]
    fn splitting_keeps_the_curve() {
        let before = curve();
        let q = segment(&before, 0).expect("segment");
        let mut after = before.clone();
        let at = split_segment(&mut after, 0, 0.3).expect("split");
        assert_eq!(at, 1);
        assert_eq!(after.points.len(), 3);
        // Both halves trace the original: sample the halves against it.
        for k in 0..=10 {
            let s = k as f64 / 10.0;
            let (a, b) = (segment(&after, 0).expect("a"), segment(&after, 1).expect("b"));
            let p = cubic_at(&a, s);
            let o = cubic_at(&q, 0.3 * s);
            assert!((p.0 - o.0).abs() < 1e-9 && (p.1 - o.1).abs() < 1e-9);
            let p = cubic_at(&b, s);
            let o = cubic_at(&q, 0.3 + 0.7 * s);
            assert!((p.0 - o.0).abs() < 1e-9 && (p.1 - o.1).abs() < 1e-9);
        }
    }

    #[test]
    fn nearest_segment_finds_the_curve() {
        let p = Path {
            contours: vec![curve()],
            ..Path::default()
        };
        let q = segment(&p.contours[0], 0).expect("segment");
        let on = cubic_at(&q, 0.6);
        let (c, s, t, d) = nearest_segment(&p, on.0 + 1.0, on.1, 3.0).expect("near");
        assert_eq!((c, s), (0, 0));
        assert!((t - 0.6).abs() < 0.05 && d <= 1.0 + 1e-6);
        assert!(nearest_segment(&p, 50.0, 200.0, 3.0).is_none());
    }

    #[test]
    fn closed_contour_splits_its_closing_segment() {
        let mut c = Contour {
            points: vec![
                BezierPoint::corner(0.0, 0.0),
                BezierPoint::corner(10.0, 0.0),
                BezierPoint::corner(10.0, 10.0),
            ],
            closed: true,
        };
        let at = split_segment(&mut c, 2, 0.5).expect("split");
        assert_eq!(at, 3);
        assert_eq!(c.points[3], BezierPoint::corner(5.0, 5.0));
    }

    #[test]
    fn removing_points_drops_short_contours() {
        let mut p = Path {
            contours: vec![curve()],
            ..Path::default()
        };
        assert!(!remove_points(&mut p, &[PointRef { contour: 0, index: 0 }]));
    }

    #[test]
    fn pen_draws_and_closes_a_path() {
        let doc = firecraft_doc::Document::new(100, 100, 72.0, None);
        let mut d = DocState::new(doc);
        let opts = ToolOptions::default();
        let r = d.pen_start(10.0, 10.0, &opts).expect("path");
        d.pen_append(r, 0, 60.0, 10.0);
        d.pen_append(r, 0, 60.0, 60.0);
        d.pen_close(r, 0);
        let p = d.path(r).expect("a path");
        assert!(p.contours[0].closed && p.contours[0].points.len() == 3);
        // Each step is its own undo step.
        d.undo();
        assert!(!d.path(r).expect("a path").contours[0].closed);
    }
}
