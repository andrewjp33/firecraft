//! The Pen and Subselection tools on the canvas (Fireworks MX).
//!
//! Pen: click for corner points, drag for curve points; click the first point to close the
//! path, click the last point again (or double-click, Enter, Escape) to finish it. On a
//! selected path: click an end point of an open path to continue it, a segment to add a point,
//! a curve point to make it a corner, a corner point to delete it.
//!
//! Subselection: shows the points of the selected paths. Click a point to select it (Shift:
//! add/remove), drag to move the selected points, drag a handle to bend a curve (Alt: just that
//! handle), Alt+drag a point to pull handles out of it, drag a box to select points. Clicking an
//! object selects it inside groups (the innermost member under the pointer), and dragging its
//! body moves it. Delete removes the selected points, arrow keys nudge them.

use egui::{Color32, Pos2, Rect, Stroke, pos2, vec2};
use firecraft_doc::{BezierPoint, Path};
use firecraft_engine::paths::{self, PointRef};
use firecraft_engine::{DocState, ElementRef, Tool, ToolOptions};

use crate::theme;

type Pt = (f64, f64);

/// Screen distance (points) within which a point, handle or segment is hit.
const HIT: f32 = 5.0;

/// The tools' state (one for the app; it follows the active document).
#[derive(Default)]
pub struct PathEdit {
    doc: u64,
    /// The path whose points are selected, and which.
    target: Option<ElementRef>,
    points: Vec<PointRef>,
    /// Pen: the open contour being drawn, extended at its end.
    pen: Option<(ElementRef, usize)>,
    drag: Option<Drag>,
    /// The document's revision when this state was last in step with it.
    revision: u64,
}

enum Drag {
    /// Moving the selected points of the target path (from the path as it was).
    Points { start: Pt, orig: Path, moved: bool },
    /// One handle of a point (`succ`: the outgoing one); Alt leaves the other handle alone.
    Handle {
        pt: PointRef,
        succ: bool,
        orig: Path,
        moved: bool,
    },
    /// Pulling handles out of a point: the Pen placing a curve point, or Alt+drag with the
    /// Subselection tool.
    Pull {
        pt: PointRef,
        at: Pt,
        live_edit: bool,
        moved: bool,
    },
    /// Subselection on a path's body: moving the object.
    Object { start: Pt, applied: Pt },
    /// Subselection on empty canvas: a box selecting points.
    Marquee { start: Pt },
}

impl PathEdit {
    /// Stop drawing with the Pen (another tool was chosen).
    pub fn end_pen(&mut self) {
        self.pen = None;
    }

    /// Bring the state in step with the document: a different document, an outside change
    /// (undo, another tool) or a changed selection drop what no longer applies.
    fn sync(&mut self, d: &DocState) {
        if self.doc != d.id {
            *self = Self {
                doc: d.id,
                revision: d.revision,
                ..Self::default()
            };
            return;
        }
        if self.drag.is_none() && self.revision != d.revision {
            self.pen = None;
        }
        let valid = |r: ElementRef| d.path(r).is_some() && d.selection.contains(&r);
        if self.target.is_none_or(|t| !valid(t)) {
            self.target = None;
            self.points.clear();
        }
        if let Some(p) = self.target.and_then(|t| d.path(t)) {
            self.points.retain(|q| {
                p.contours
                    .get(q.contour)
                    .is_some_and(|c| q.index < c.points.len())
            });
        }
        if self.pen.is_some_and(|(r, c)| {
            !valid(r) || d.path(r).and_then(|p| p.contours.get(c)).is_none_or(|c| c.closed)
        }) {
            self.pen = None;
        }
    }
}

fn point(p: &Path, r: PointRef) -> Option<&BezierPoint> {
    p.contours.get(r.contour)?.points.get(r.index)
}

fn point_mut(p: &mut Path, r: PointRef) -> Option<&mut BezierPoint> {
    p.contours.get_mut(r.contour)?.points.get_mut(r.index)
}

/// The selected paths (their points are shown).
fn shown(d: &DocState) -> Vec<ElementRef> {
    d.selection
        .iter()
        .copied()
        .filter(|r| d.path(*r).is_some())
        .collect()
}

/// Constrain `p` to 45-degree steps from `from` (Shift).
fn constrain(from: Pt, p: Pt) -> Pt {
    let (dx, dy) = (p.0 - from.0, p.1 - from.1);
    let len = dx.hypot(dy);
    let a = (dy.atan2(dx) / std::f64::consts::FRAC_PI_4).round() * std::f64::consts::FRAC_PI_4;
    (from.0 + len * a.cos(), from.1 + len * a.sin())
}

/// Delete the selected points (Delete key with points selected). Whether it did.
pub fn delete(edit: &mut PathEdit, d: &mut DocState) -> bool {
    edit.sync(d);
    let (Some(r), false) = (edit.target, edit.points.is_empty()) else {
        return false;
    };
    let pts = std::mem::take(&mut edit.points);
    d.delete_points(r, &pts);
    edit.revision = d.revision;
    true
}

/// Nudge the selected points (arrow keys). Whether it did.
pub fn nudge(edit: &mut PathEdit, d: &mut DocState, dx: f64, dy: f64) -> bool {
    edit.sync(d);
    let (Some(r), false) = (edit.target, edit.points.is_empty()) else {
        return false;
    };
    let pts = edit.points.clone();
    d.path_edit(r, "Move Points", |p| {
        for q in &pts {
            if let Some(b) = point_mut(p, *q) {
                b.x += dx;
                b.y += dy;
            }
        }
        p.rect = None;
    });
    edit.revision = d.revision;
    true
}

#[allow(clippy::too_many_arguments)]
pub fn canvas(
    edit: &mut PathEdit,
    d: &mut DocState,
    tool: Tool,
    painter: &egui::Painter,
    resp: &egui::Response,
    (origin, z): (Pos2, f32),
    opts: &ToolOptions,
    (cursors, highlight): (crate::cursors::CursorStyle, bool),
) {
    edit.sync(d);
    let ctx = resp.ctx.clone();
    let to_screen = |q: Pt| pos2(origin.x + q.0 as f32 * z, origin.y + q.1 as f32 * z);
    let to_doc = |q: Pos2| (((q.x - origin.x) / z) as f64, ((q.y - origin.y) / z) as f64);
    let near = |a: Pt, s: Pos2| to_screen(a).distance(s) <= HIT;
    let (mods, pressed, down, released, pointer) = ctx.input(|i| {
        (
            i.modifiers,
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.primary_released(),
            i.pointer.interact_pos(),
        )
    });
    let pressed = pressed && resp.hovered();

    if tool == Tool::Pen
        && edit.pen.is_some()
        && ctx.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape))
    {
        edit.pen = None;
    }

    // ---- Press ----------------------------------------------------------------------------
    if pressed
        && edit.drag.is_none()
        && let Some(s) = pointer
    {
        let at = to_doc(s);
        match tool {
            Tool::Pen => pen_press(edit, d, at, s, mods, opts, &near),
            _ => subselect_press(edit, d, at, s, mods, &near),
        }
    }

    // ---- Drag -----------------------------------------------------------------------------
    if down && let Some(s) = pointer {
        let cur = to_doc(s);
        // The path being edited (a placeholder when there is none: path changes then do
        // nothing).
        let r = edit
            .target
            .or(edit.pen.map(|p| p.0))
            .unwrap_or(ElementRef::new(usize::MAX, 0));
        let alt = mods.alt;
        match &mut edit.drag {
            Some(Drag::Points { start, orig, moved }) => {
                let (mut dx, mut dy) = (cur.0 - start.0, cur.1 - start.1);
                if mods.shift {
                    // Shift: horizontal or vertical only.
                    if dx.abs() > dy.abs() {
                        dy = 0.0;
                    } else {
                        dx = 0.0;
                    }
                }
                if dx != 0.0 || dy != 0.0 || *moved {
                    *moved = true;
                    let mut p = orig.clone();
                    for q in &edit.points {
                        if let Some(b) = point_mut(&mut p, *q) {
                            b.x += dx;
                            b.y += dy;
                        }
                    }
                    p.rect = None;
                    d.path_live(r, |x| *x = p);
                }
            }
            Some(Drag::Handle {
                pt,
                succ,
                orig,
                moved,
            }) => {
                if let Some(b) = point(orig, *pt) {
                    *moved = true;
                    let v = (cur.0 - b.x, cur.1 - b.y);
                    let other = if *succ { b.pred } else { b.succ };
                    // A smooth point keeps its handles in line (each keeps its length).
                    let len = other.0.hypot(other.1);
                    let vlen = v.0.hypot(v.1);
                    let mirrored = if alt || len == 0.0 || vlen == 0.0 {
                        other
                    } else {
                        (-v.0 / vlen * len, -v.1 / vlen * len)
                    };
                    let (pt, succ) = (*pt, *succ);
                    let mut p = orig.clone();
                    if let Some(q) = point_mut(&mut p, pt) {
                        if succ {
                            (q.succ, q.pred) = (v, mirrored);
                        } else {
                            (q.pred, q.succ) = (v, mirrored);
                        }
                    }
                    p.rect = None;
                    d.path_live(r, |x| *x = p);
                }
            }
            Some(Drag::Pull {
                pt,
                at,
                live_edit,
                moved,
            }) => {
                let v = (cur.0 - at.0, cur.1 - at.1);
                if *moved || to_screen(*at).distance(s) > 2.0 {
                    if !*moved && *live_edit {
                        d.begin_live_edit("Convert Point");
                    }
                    *moved = true;
                    let pt = *pt;
                    d.path_live(r, |p| {
                        if let Some(q) = point_mut(p, pt) {
                            q.succ = v;
                            q.pred = (-v.0, -v.1);
                        }
                        p.rect = None;
                    });
                }
            }
            Some(Drag::Object { start, applied }) => {
                let target = ((cur.0 - start.0).round(), (cur.1 - start.1).round());
                if target != *applied {
                    d.move_selection_live(target.0 - applied.0, target.1 - applied.1);
                    *applied = target;
                }
            }
            Some(Drag::Marquee { .. }) | None => {}
        }
    }
    // A box with no path selected still selects nothing but shows.
    if down && let (Some(s), Some(Drag::Marquee { start })) = (pointer, &edit.drag) {
        let r = Rect::from_two_pos(to_screen(*start), s);
        painter.rect_stroke(
            r,
            0.0,
            Stroke::new(1.0, theme::SELECT_BLUE),
            egui::StrokeKind::Inside,
        );
    }

    // ---- Release --------------------------------------------------------------------------
    if (released || !down)
        && let Some(drag) = edit.drag.take()
    {
        match drag {
            Drag::Points { moved: false, .. } | Drag::Handle { moved: false, .. } => d.discard_live_edit(),
            Drag::Object { applied, .. } if applied == (0.0, 0.0) => d.cancel_live_edit(),
            Drag::Marquee { start } => {
                let end = pointer.map_or(start, to_doc);
                let (x0, x1) = (start.0.min(end.0), start.0.max(end.0));
                let (y0, y1) = (start.1.min(end.1), start.1.max(end.1));
                let tiny = to_screen(start).distance(to_screen(end)) < 3.0;
                if tiny {
                    // A click on empty canvas deselects (Shift keeps the selection).
                    if !mods.shift {
                        edit.points.clear();
                        edit.target = None;
                        d.deselect();
                    }
                } else {
                    // Points inside the box, in the first shown path that has any.
                    let found = shown(d).into_iter().find_map(|r| {
                        let p = d.path(r)?;
                        let pts: Vec<PointRef> = p
                            .contours
                            .iter()
                            .enumerate()
                            .flat_map(|(ci, c)| {
                                c.points.iter().enumerate().filter_map(move |(i, b)| {
                                    (b.x >= x0 && b.x <= x1 && b.y >= y0 && b.y <= y1).then_some(PointRef {
                                        contour: ci,
                                        index: i,
                                    })
                                })
                            })
                            .collect();
                        (!pts.is_empty()).then_some((r, pts))
                    });
                    match found {
                        Some((r, pts)) => {
                            if mods.shift && edit.target == Some(r) {
                                for q in pts {
                                    if !edit.points.contains(&q) {
                                        edit.points.push(q);
                                    }
                                }
                            } else {
                                edit.target = Some(r);
                                edit.points = pts;
                            }
                        }
                        None if !mods.shift => edit.points.clear(),
                        None => {}
                    }
                }
            }
            _ => {}
        }
    }

    // Cursor: the Subselection arrow; the Pen a crosshair (set by the canvas).
    if tool == Tool::Subselect && (resp.hovered() || resp.dragged()) {
        crate::cursors::set(&ctx, crate::cursors::Cur::Subselect, cursors);
    }
    edit.revision = d.revision;
    // Mouse highlight: the member a click would select (inside groups), in red.
    if highlight
        && tool == Tool::Subselect
        && edit.drag.is_none()
        && resp.hovered()
        && let Some(h) = pointer
    {
        let at = to_doc(h);
        if let Some(r) = d.hit_deep(at.0, at.1)
            && !d.selection.contains(&r)
        {
            crate::canvas::mouse_highlight(painter, d, r, &|x, y| to_screen((x, y)));
        }
    }
    overlay(
        edit,
        d,
        tool,
        painter,
        pointer.filter(|_| resp.hovered()),
        &to_screen,
        mods.shift,
    );
}

#[allow(clippy::too_many_arguments)]
fn pen_press(
    edit: &mut PathEdit,
    d: &mut DocState,
    at: Pt,
    s: Pos2,
    mods: egui::Modifiers,
    opts: &ToolOptions,
    near: &dyn Fn(Pt, Pos2) -> bool,
) {
    // Drawing: close on the first point, finish on the last, else add a point.
    if let Some((r, ci)) = edit.pen
        && let Some(c) = d.path(r).and_then(|p| p.contours.get(ci)).cloned()
    {
        let (first, last) = (c.points.first().copied(), c.points.last().copied());
        if let Some(l) = last
            && near((l.x, l.y), s)
        {
            edit.pen = None;
            return;
        }
        if let Some(f) = first
            && c.points.len() >= 2
            && near((f.x, f.y), s)
        {
            d.pen_close(r, ci);
            edit.pen = None;
            edit.target = Some(r);
            edit.points = vec![PointRef {
                contour: ci,
                index: 0,
            }];
            edit.drag = Some(Drag::Pull {
                pt: PointRef {
                    contour: ci,
                    index: 0,
                },
                at: (f.x, f.y),
                live_edit: false,
                moved: false,
            });
            return;
        }
        let p = match last {
            Some(l) if mods.shift => constrain((l.x, l.y), at),
            _ => at,
        };
        if let Some(pt) = d.pen_append(r, ci, p.0, p.1) {
            edit.target = Some(r);
            edit.points = vec![pt];
            edit.drag = Some(Drag::Pull {
                pt,
                at: p,
                live_edit: false,
                moved: false,
            });
        }
        return;
    }
    // On the selected path: continue it, or add, convert or delete a point.
    if let [r] = d.selection[..]
        && let Some(p) = d.path(r).cloned()
    {
        let hit_point = p.contours.iter().enumerate().find_map(|(ci, c)| {
            c.points
                .iter()
                .position(|b| near((b.x, b.y), s))
                .map(|i| PointRef {
                    contour: ci,
                    index: i,
                })
        });
        if let Some(pt) = hit_point
            && let Some(c) = p.contours.get(pt.contour)
        {
            let end = !c.closed && (pt.index == 0 || pt.index + 1 == c.points.len());
            if end && c.points.len() >= 2 {
                // Continue drawing from this end (the first point: reverse the contour).
                if pt.index == 0 {
                    d.path_edit(r, "Continue Path", |p| {
                        if let Some(c) = p.contours.get_mut(pt.contour) {
                            paths::reverse(c);
                        }
                        p.rect = None;
                    });
                }
                edit.pen = Some((r, pt.contour));
                edit.target = Some(r);
                edit.points.clear();
            } else if point(&p, pt).is_some_and(paths::is_curve) {
                d.make_corner(r, pt);
            } else {
                d.delete_points(r, &[pt]);
                edit.points.clear();
            }
            return;
        }
        let tol = (HIT as f64) / d.zoom as f64;
        if let Some((ci, seg, t, _)) = paths::nearest_segment(&p, at.0, at.1, tol) {
            if let Some(pt) = d.insert_point(r, ci, seg, t) {
                edit.target = Some(r);
                edit.points = vec![pt];
            }
            return;
        }
    }
    // Elsewhere: a new path.
    if let Some(r) = d.pen_start(at.0, at.1, opts) {
        let pt = PointRef { contour: 0, index: 0 };
        edit.pen = Some((r, 0));
        edit.target = Some(r);
        edit.points = vec![pt];
        edit.drag = Some(Drag::Pull {
            pt,
            at,
            live_edit: false,
            moved: false,
        });
    }
}

fn subselect_press(
    edit: &mut PathEdit,
    d: &mut DocState,
    at: Pt,
    s: Pos2,
    mods: egui::Modifiers,
    near: &dyn Fn(Pt, Pos2) -> bool,
) {
    // A handle of a selected point.
    if let Some(r) = edit.target
        && let Some(p) = d.path(r).cloned()
    {
        for &pt in &edit.points {
            let Some(b) = point(&p, pt) else { continue };
            for (succ, h) in [(true, b.succ), (false, b.pred)] {
                if h != (0.0, 0.0) && near((b.x + h.0, b.y + h.1), s) {
                    d.begin_live_edit("Move Handle");
                    edit.drag = Some(Drag::Handle {
                        pt,
                        succ,
                        orig: p,
                        moved: false,
                    });
                    return;
                }
            }
        }
    }
    // A point of a shown path.
    for r in shown(d) {
        let Some(p) = d.path(r).cloned() else { continue };
        let hit = p.contours.iter().enumerate().find_map(|(ci, c)| {
            c.points
                .iter()
                .position(|b| near((b.x, b.y), s))
                .map(|i| PointRef {
                    contour: ci,
                    index: i,
                })
        });
        let Some(pt) = hit else { continue };
        if edit.target != Some(r) {
            edit.points.clear();
        }
        edit.target = Some(r);
        if mods.alt {
            // Pull handles out of the point.
            edit.points = vec![pt];
            let at = point(&p, pt).map_or(at, |b| (b.x, b.y));
            edit.drag = Some(Drag::Pull {
                pt,
                at,
                live_edit: true,
                moved: false,
            });
            return;
        }
        if mods.shift {
            if let Some(k) = edit.points.iter().position(|q| *q == pt) {
                edit.points.remove(k);
                return;
            }
            edit.points.push(pt);
        } else if !edit.points.contains(&pt) {
            edit.points = vec![pt];
        }
        d.begin_live_edit("Move Points");
        edit.drag = Some(Drag::Points {
            start: at,
            orig: p,
            moved: false,
        });
        return;
    }
    // An object, inside groups down to the innermost member (Fireworks: the Subselection tool
    // selects group members): select it (a path shows its points) and drag to move it.
    if let Some(hit) = d.hit_deep(at.0, at.1) {
        if !d.selection.contains(&hit) {
            d.selection = if mods.shift {
                [d.selection.clone(), vec![hit]].concat()
            } else {
                vec![hit]
            };
            d.pixels = None;
        }
        if edit.target != Some(hit) {
            edit.points.clear();
            edit.target = d.path(hit).is_some().then_some(hit);
        }
        d.begin_live_edit("Move");
        edit.drag = Some(Drag::Object {
            start: at,
            applied: (0.0, 0.0),
        });
        return;
    }
    edit.drag = Some(Drag::Marquee { start: at });
}

/// The selected paths' outlines and points; handles of the selected points; the Pen's next
/// segment.
fn overlay(
    edit: &PathEdit,
    d: &DocState,
    tool: Tool,
    painter: &egui::Painter,
    hover: Option<Pos2>,
    to_screen: &dyn Fn(Pt) -> Pos2,
    shift: bool,
) {
    let blue = theme::SELECT_BLUE;
    let line = Stroke::new(1.0, blue);
    // Selected objects that aren't paths (a bitmap or text member, a group): their bounds.
    for r in &d.selection {
        if d.path(*r).is_none()
            && let Some((x, y, w, h)) = d.element(*r).and_then(|e| e.bounds())
        {
            let rect = Rect::from_two_pos(to_screen((x, y)), to_screen((x + w, y + h)));
            painter.rect_stroke(rect, 0.0, line, egui::StrokeKind::Outside);
        }
    }
    for r in shown(d) {
        let Some(p) = d.path(r) else { continue };
        for c in &p.contours {
            let pts: Vec<Pos2> = c.flatten(0.0, 0.0).into_iter().map(to_screen).collect();
            if pts.len() >= 2 {
                if c.closed {
                    painter.add(egui::Shape::closed_line(pts, line));
                } else {
                    painter.add(egui::Shape::line(pts, line));
                }
            }
        }
        let selected = |pt: PointRef| edit.target == Some(r) && edit.points.contains(&pt);
        // Handles of the selected points (and the point being drawn).
        for (ci, c) in p.contours.iter().enumerate() {
            for (i, b) in c.points.iter().enumerate() {
                let pt = PointRef {
                    contour: ci,
                    index: i,
                };
                if !selected(pt) {
                    continue;
                }
                let a = to_screen((b.x, b.y));
                for h in [b.pred, b.succ] {
                    if h != (0.0, 0.0) {
                        let e = to_screen((b.x + h.0, b.y + h.1));
                        painter.line_segment([a, e], line);
                        painter.circle_filled(e, 2.5, blue);
                    }
                }
            }
        }
        // Points: hollow squares, filled when selected (as in Fireworks MX).
        for (ci, c) in p.contours.iter().enumerate() {
            for (i, b) in c.points.iter().enumerate() {
                let a = to_screen((b.x, b.y));
                let sq = Rect::from_center_size(a, vec2(5.0, 5.0));
                if selected(PointRef {
                    contour: ci,
                    index: i,
                }) {
                    painter.rect_filled(sq, 0.0, blue);
                } else {
                    painter.rect_filled(sq, 0.0, Color32::WHITE);
                    painter.rect_stroke(sq, 0.0, line, egui::StrokeKind::Inside);
                }
            }
        }
    }
    // The Pen's next segment, from the last point to the pointer.
    if tool == Tool::Pen
        && edit.drag.is_none()
        && let (Some((r, ci)), Some(h)) = (edit.pen, hover)
        && let Some(l) = d
            .path(r)
            .and_then(|p| p.contours.get(ci))
            .and_then(|c| c.points.last())
    {
        let to = if shift {
            let (hx, hy) = (
                (h.x - to_screen((0.0, 0.0)).x) as f64,
                (h.y - to_screen((0.0, 0.0)).y) as f64,
            );
            let z = (to_screen((1.0, 0.0)).x - to_screen((0.0, 0.0)).x) as f64;
            to_screen(constrain((l.x, l.y), (hx / z, hy / z)))
        } else {
            h
        };
        painter.line_segment(
            [to_screen((l.x, l.y)), to],
            Stroke::new(1.0, blue.gamma_multiply(0.6)),
        );
    }
}
