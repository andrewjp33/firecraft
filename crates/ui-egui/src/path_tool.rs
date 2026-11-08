//! The Pen and Subselection tools on the canvas (Fireworks MX).
//!
//! Pen: click for corner points, drag for curve points (Shift: 45-degree steps; Alt: the
//! outgoing handle only); click the first point to close the path, double-click (or Enter,
//! Escape) to finish it. Ctrl: the Subselection tool for as long as it's held. On the path:
//! click a segment to add a point, a curve point to make it a corner, a corner point to delete
//! it, or drag any point to pull out new handles; click an end point of a selected open path to
//! continue it, and while drawing the end of another open path to join it. Switching to another
//! tool and back resumes the drawing. Behaviour measured in Fireworks MX: docs/behavior/paths.md.
//!
//! Subselection: shows the points of the selected paths. Click a point to select it (Shift:
//! add/remove), drag to move the selected points, drag a handle to bend a curve (Alt: just that
//! handle; the neighbours' near handles show too), Alt+drag a selected point to pull out its
//! outgoing handle, drag a box to select points. Clicking an
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
    /// The canvas scale (points per document pixel) last shown.
    scale: f32,
    /// The Pen's last press: when and where (double-clicks).
    last_press: Option<(f64, Pos2)>,
}

/// What a press on an existing point does if the mouse isn't dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
enum OnClick {
    Nothing,
    MakeCorner,
    Delete,
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
    /// Pulling handles out of a point: the Pen placing or re-shaping a curve point, or Alt+drag
    /// with the Subselection tool.
    Pull {
        pt: PointRef,
        at: Pt,
        /// Start a live edit when the drag begins (none was begun by the press).
        live_edit: bool,
        moved: bool,
        /// Closing a path: only the incoming handle changes.
        closing: bool,
        /// Only the outgoing handle changes (Subselection's Alt+drag).
        succ_only: bool,
        /// The point's incoming handle before the drag.
        pred: Pt,
        on_click: OnClick,
    },
    /// A press whose edit is already made; its History state appears on release.
    Done,
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

/// Constrain `p` to 45-degree steps from `from` (Shift), keeping the larger of the horizontal
/// and vertical distances (as Fireworks MX: a drag of 40, 25 gives 40, 40).
fn constrain(from: Pt, p: Pt) -> Pt {
    let (dx, dy) = (p.0 - from.0, p.1 - from.1);
    let m = dx.abs().max(dy.abs());
    if m == 0.0 {
        return from;
    }
    let (sin, cos) =
        ((dy.atan2(dx) / std::f64::consts::FRAC_PI_4).round() * std::f64::consts::FRAC_PI_4).sin_cos();
    let unit = |v: f64| {
        if v > 0.5 {
            1.0
        } else if v < -0.5 {
            -1.0
        } else {
            0.0
        }
    };
    (from.0 + unit(cos) * m, from.1 + unit(sin) * m)
}

/// The handles shown for the selected points `pts`, as (point, outgoing?): theirs and their
/// neighbours' near ones (Fireworks MX).
fn visible_handles(p: &Path, pts: &[PointRef]) -> Vec<(PointRef, bool)> {
    let mut out: Vec<(PointRef, bool)> = Vec::new();
    let mut add = |pt: PointRef, succ: bool| {
        let shown = point(p, pt).is_some_and(|b| (if succ { b.succ } else { b.pred }) != (0.0, 0.0));
        if shown && !out.contains(&(pt, succ)) {
            out.push((pt, succ));
        }
    };
    for &pt in pts {
        add(pt, true);
        add(pt, false);
        let Some(c) = p.contours.get(pt.contour) else {
            continue;
        };
        let n = c.points.len();
        if n < 2 {
            continue;
        }
        let at = |index: usize| PointRef {
            contour: pt.contour,
            index,
        };
        if pt.index > 0 {
            add(at(pt.index - 1), true);
        } else if c.closed {
            add(at(n - 1), true);
        }
        if pt.index + 1 < n {
            add(at(pt.index + 1), false);
        } else if c.closed {
            add(at(0), false);
        }
    }
    out
}

/// A pull on a point just placed (the press already made the edit).
fn pull(pt: PointRef, at: Pt) -> Drag {
    pull_from(pt, at, (0.0, 0.0), false, OnClick::Nothing)
}

/// A pull on point `pt` at `at` whose incoming handle is `pred`. `live_edit`: the drag begins the
/// edit (the press made none).
fn pull_from(pt: PointRef, at: Pt, pred: Pt, live_edit: bool, on_click: OnClick) -> Drag {
    Drag::Pull {
        pt,
        at,
        live_edit,
        moved: false,
        closing: false,
        succ_only: false,
        pred,
        on_click,
    }
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
    edit.scale = z;
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
    let now = ctx.input(|i| i.time);
    // Ctrl with the Pen: the Subselection tool for as long as it's held.
    let subselecting = tool == Tool::Subselect || mods.ctrl || mods.command;

    if tool == Tool::Pen
        && edit.pen.is_some()
        && ctx.input(|i| i.key_pressed(egui::Key::Enter) || i.key_pressed(egui::Key::Escape))
    {
        edit.pen = None;
    }

    // Where the button went down: the pointer may already be elsewhere by the time this frame
    // is drawn, and a thin stroke is easy to miss.
    let press_at = ctx.input(|i| i.pointer.press_origin()).or(pointer);
    if pressed
        && edit.drag.is_none()
        && let Some(s) = press_at
    {
        let at = to_doc(s);
        if subselecting {
            subselect_press(edit, d, at, s, mods, &near);
        } else {
            pen_press(edit, d, at, s, mods, opts, &near, now);
        }
    }
    if down && let Some(s) = pointer {
        let far = |q: Pt| to_screen(q).distance(s) > 2.0;
        drag_to(edit, d, to_doc(s), mods, &far);
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
    if released || !down {
        let end = pointer.map(to_doc);
        let tiny = |a: Pt, b: Pt| to_screen(a).distance(to_screen(b)) < 3.0;
        release(edit, d, end, mods, &tiny);
    }

    // Cursor: the Subselection arrow; the Pen a crosshair (set by the canvas).
    if subselecting && (resp.hovered() || resp.dragged()) {
        crate::cursors::set(&ctx, crate::cursors::Cur::Subselect, cursors);
    }
    edit.revision = d.revision;
    // Mouse highlight: the member a click would select (inside groups), in red.
    if highlight
        && subselecting
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

/// The mouse moved to `cur` during a drag. `far`: whether the pointer has left a point.
fn drag_to(edit: &mut PathEdit, d: &mut DocState, cur: Pt, mods: egui::Modifiers, far: &dyn Fn(Pt) -> bool) {
    // The path being edited (a placeholder when there is none: path changes then do nothing).
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
            closing,
            succ_only,
            pred,
            ..
        }) => {
            if *moved || far(*at) {
                if !*moved && *live_edit {
                    d.begin_live_edit("Convert Point");
                }
                *moved = true;
                let mut v = (cur.0 - at.0, cur.1 - at.1);
                if mods.shift {
                    v = constrain((0.0, 0.0), v);
                }
                let (pt, closing, keep_pred, pred) = (*pt, *closing, *succ_only || alt, *pred);
                d.path_live(r, |p| {
                    if let Some(q) = point_mut(p, pt) {
                        if closing {
                            q.pred = (-v.0, -v.1);
                        } else {
                            q.succ = v;
                            q.pred = if keep_pred { pred } else { (-v.0, -v.1) };
                        }
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
        Some(Drag::Done | Drag::Marquee { .. }) | None => {}
    }
}

/// The mouse button was released. `tiny`: whether two points are about the same place.
fn release(
    edit: &mut PathEdit,
    d: &mut DocState,
    end: Option<Pt>,
    mods: egui::Modifiers,
    tiny: &dyn Fn(Pt, Pt) -> bool,
) {
    let Some(drag) = edit.drag.take() else { return };
    match drag {
        Drag::Points { moved: false, .. } | Drag::Handle { moved: false, .. } => d.discard_live_edit(),
        Drag::Object {
            applied: (0.0, 0.0), ..
        } => d.cancel_live_edit(),
        Drag::Pull {
            moved: false,
            on_click,
            pt,
            ..
        } => {
            if let Some(r) = edit.target {
                match on_click {
                    OnClick::Nothing => {}
                    OnClick::MakeCorner => d.make_corner(r, pt),
                    OnClick::Delete => {
                        d.delete_points(r, &[pt]);
                        edit.points.clear();
                    }
                }
            }
        }
        Drag::Marquee { start } => {
            let end = end.unwrap_or(start);
            let (x0, x1) = (start.0.min(end.0), start.0.max(end.0));
            let (y0, y1) = (start.1.min(end.1), start.1.max(end.1));
            if tiny(start, end) {
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
    // Whatever the drag was, its History state appears now (a discarded one is already gone).
    d.commit_live_edit();
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
    now: f64,
) {
    let double = edit
        .last_press
        .is_some_and(|(t, q)| now - t < 0.5 && q.distance(s) <= HIT);
    edit.last_press = Some((now, s));
    let drawing = edit
        .pen
        .filter(|(r, c)| d.path(*r).is_some_and(|p| p.contours.get(*c).is_some()));

    if let Some((r, ci)) = drawing
        && let Some(c) = d.path(r).and_then(|p| p.contours.get(ci)).cloned()
    {
        // Double-clicking the last point finishes the path.
        if double && c.points.last().is_some_and(|l| near((l.x, l.y), s)) {
            edit.pen = None;
            return;
        }
        // The first point closes it; a drag shapes the incoming curve.
        if let Some(f) = c.points.first().copied()
            && c.points.len() >= 2
            && near((f.x, f.y), s)
        {
            d.pen_close(r, ci);
            let pt = PointRef {
                contour: ci,
                index: 0,
            };
            edit.pen = None;
            edit.target = Some(r);
            edit.points = vec![pt];
            let mut drag = pull_from(pt, (f.x, f.y), f.pred, false, OnClick::Nothing);
            if let Drag::Pull { closing, .. } = &mut drag {
                *closing = true;
            }
            edit.drag = Some(drag);
            return;
        }
    }

    // On the path being drawn, or the selected one.
    let selected = match d.selection[..] {
        [r] if d.path(r).is_some() => Some(r),
        _ => None,
    };
    if let Some(r) = drawing.map(|p| p.0).or(selected)
        && let Some(p) = d.path(r).cloned()
    {
        let hit = p.contours.iter().enumerate().find_map(|(ci, c)| {
            c.points
                .iter()
                .position(|b| near((b.x, b.y), s))
                .map(|index| PointRef { contour: ci, index })
        });
        if let Some(pt) = hit
            && let (Some(c), Some(b)) = (p.contours.get(pt.contour), point(&p, pt).copied())
        {
            let last = pt.index + 1 == c.points.len();
            let end = !c.closed && (pt.index == 0 || last);
            if drawing.is_none() && end && c.points.len() >= 2 {
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
                return;
            }
            // A drag pulls new handles out of the point; a click converts or deletes it.
            let on_click = if drawing == Some((r, pt.contour)) && last {
                OnClick::Nothing
            } else if paths::is_curve(&b) {
                OnClick::MakeCorner
            } else {
                OnClick::Delete
            };
            edit.target = Some(r);
            edit.points = vec![pt];
            edit.drag = Some(pull_from(pt, (b.x, b.y), b.pred, true, on_click));
            return;
        }
        // While drawing, the end of another open path joins it.
        if let Some((r, ci)) = drawing
            && let Some((other, other_ci, at_start)) = d.open_end_at(|x, y| near((x, y), s), r)
        {
            if let Some((merged, contour)) = d.pen_merge(r, ci, other, other_ci, at_start) {
                edit.pen = Some((merged, contour));
                edit.target = Some(merged);
                let index = d
                    .path(merged)
                    .and_then(|p| p.contours.get(contour))
                    .map_or(0, |c| c.points.len().saturating_sub(1));
                edit.points = vec![PointRef { contour, index }];
            }
            return;
        }
        // A segment gets a new point.
        let tol = (HIT as f64) / edit.scale.max(1e-3) as f64;
        if let Some((ci, seg, t, _)) = paths::nearest_segment(&p, at.0, at.1, tol) {
            if let Some(pt) = d.insert_point(r, ci, seg, t) {
                edit.target = Some(r);
                edit.points = vec![pt];
                edit.drag = Some(Drag::Done);
            }
            return;
        }
    }

    // Still drawing: the next point. (Shift: at 45-degree steps from the last one.)
    if let Some((r, ci)) = drawing {
        let last = d
            .path(r)
            .and_then(|p| p.contours.get(ci))
            .and_then(|c| c.points.last().copied());
        let p = match last {
            Some(l) if mods.shift => constrain((l.x, l.y), at),
            _ => at,
        };
        if let Some(pt) = d.pen_append(r, ci, p.0, p.1) {
            edit.target = Some(r);
            edit.points = vec![pt];
            edit.drag = Some(pull(pt, p));
        }
        return;
    }
    // Otherwise a new path.
    if let Some(r) = d.pen_start(at.0, at.1, opts) {
        let pt = PointRef { contour: 0, index: 0 };
        edit.pen = Some((r, 0));
        edit.target = Some(r);
        edit.points = vec![pt];
        edit.drag = Some(pull(pt, at));
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
        for (pt, succ) in visible_handles(&p, &edit.points) {
            let Some(b) = point(&p, pt) else { continue };
            let h = if succ { b.succ } else { b.pred };
            if near((b.x + h.0, b.y + h.1), s) {
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
        if mods.alt
            && edit.points.contains(&pt)
            && let Some(b) = point(&p, pt)
        {
            // Alt+drag on a selected point: pull out its outgoing handle.
            edit.points = vec![pt];
            let mut drag = pull_from(pt, (b.x, b.y), b.pred, true, OnClick::Nothing);
            if let Drag::Pull { succ_only, .. } = &mut drag {
                *succ_only = true;
            }
            edit.drag = Some(drag);
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
        // Handles of the selected points and their neighbours' near ones.
        if edit.target == Some(r) {
            for (pt, succ) in visible_handles(p, &edit.points) {
                let Some(b) = point(p, pt) else { continue };
                let h = if succ { b.succ } else { b.pred };
                let e = to_screen((b.x + h.0, b.y + h.1));
                painter.line_segment([to_screen((b.x, b.y)), e], line);
                painter.circle_filled(e, 2.5, blue);
            }
        }
        // Points: hollow squares, hollow circles for curve points, filled squares when selected
        // (as in Fireworks MX).
        for (ci, c) in p.contours.iter().enumerate() {
            for (i, b) in c.points.iter().enumerate() {
                let a = to_screen((b.x, b.y));
                let sq = Rect::from_center_size(a, vec2(5.0, 5.0));
                if selected(PointRef {
                    contour: ci,
                    index: i,
                }) {
                    painter.rect_filled(sq, 0.0, blue);
                } else if paths::is_curve(b) {
                    painter.circle_filled(a, 2.5, Color32::WHITE);
                    painter.circle_stroke(a, 2.5, line);
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
        // The curve the next click would make, with the last point's outgoing handle.
        let (a, c1) = (to_screen((l.x, l.y)), to_screen((l.x + l.succ.0, l.y + l.succ.1)));
        let preview = (0..=24)
            .map(|i| {
                let t = i as f32 / 24.0;
                let u = 1.0 - t;
                let mix = |p0: f32, p1: f32, p3: f32| {
                    u * u * u * p0 + 3.0 * u * u * t * p1 + 3.0 * u * t * t * p3 + t * t * t * p3
                };
                pos2(mix(a.x, c1.x, to.x), mix(a.y, c1.y, to.y))
            })
            .collect();
        painter.add(egui::Shape::line(
            preview,
            Stroke::new(1.0, blue.gamma_multiply(0.6)),
        ));
    }
}

#[cfg(test)]
mod tests {
    //! Replays of gestures performed in Fireworks MX, with the point positions and handles it
    //! reported through its scripting API (docs/behavior/paths.md).
    use super::*;
    use firecraft_doc::Document;

    struct Sim {
        edit: PathEdit,
        d: DocState,
        opts: ToolOptions,
        now: f64,
    }

    const NONE: egui::Modifiers = egui::Modifiers::NONE;
    const SHIFT: egui::Modifiers = egui::Modifiers::SHIFT;
    const ALT: egui::Modifiers = egui::Modifiers::ALT;

    impl Sim {
        fn new() -> Self {
            Self {
                edit: PathEdit::default(),
                d: DocState::new(Document::new(400, 300, 72.0, None)),
                opts: ToolOptions::default(),
                now: 0.0,
            }
        }

        fn press(&mut self, tool: Tool, at: Pt, mods: egui::Modifiers) {
            self.edit.sync(&self.d);
            self.edit.scale = 1.0;
            let s = pos2(at.0 as f32, at.1 as f32);
            let near = |q: Pt, s: Pos2| pos2(q.0 as f32, q.1 as f32).distance(s) <= HIT;
            if tool == Tool::Pen && !mods.ctrl {
                pen_press(
                    &mut self.edit,
                    &mut self.d,
                    at,
                    s,
                    mods,
                    &self.opts,
                    &near,
                    self.now,
                );
            } else {
                subselect_press(&mut self.edit, &mut self.d, at, s, mods, &near);
            }
        }

        fn up(&mut self, at: Pt, mods: egui::Modifiers) {
            let tiny = |a: Pt, b: Pt| (a.0 - b.0).hypot(a.1 - b.1) < 3.0;
            release(&mut self.edit, &mut self.d, Some(at), mods, &tiny);
            self.edit.revision = self.d.revision;
            // Clicks far apart in time unless a test says otherwise.
            self.now += 10.0;
        }

        fn click(&mut self, tool: Tool, at: Pt, mods: egui::Modifiers) {
            self.press(tool, at, mods);
            self.up(at, mods);
        }

        fn drag(&mut self, tool: Tool, a: Pt, b: Pt, mods: egui::Modifiers) {
            self.press(tool, a, mods);
            let far = |q: Pt| (q.0 - b.0).hypot(q.1 - b.1) > 2.0;
            drag_to(&mut self.edit, &mut self.d, b, mods, &far);
            self.up(b, mods);
        }

        fn pen(&mut self, at: Pt) {
            self.click(Tool::Pen, at, NONE);
        }

        fn double(&mut self, at: Pt) {
            self.click(Tool::Pen, at, NONE);
            self.now -= 10.0;
            self.click(Tool::Pen, at, NONE);
        }

        /// Each point as (x, y, pred, succ) of the top object.
        fn nodes(&self) -> Vec<(f64, f64, Pt, Pt)> {
            self.nodes_of(self.top())
        }

        fn top(&self) -> ElementRef {
            let n = self.d.doc.layers[self.d.doc.current_layer].elements(0).len();
            ElementRef::new(self.d.doc.current_layer, n - 1)
        }

        fn nodes_of(&self, r: ElementRef) -> Vec<(f64, f64, Pt, Pt)> {
            self.d.path(r).expect("a path").contours[0]
                .points
                .iter()
                .map(|b| (b.x, b.y, b.pred, b.succ))
                .collect()
        }

        fn closed(&self) -> bool {
            self.d.path(self.top()).expect("a path").contours[0].closed
        }
    }

    const ZERO: Pt = (0.0, 0.0);

    fn corner(x: f64, y: f64) -> (f64, f64, Pt, Pt) {
        (x, y, ZERO, ZERO)
    }

    /// The first path of the MX session: two corners and a curve point.
    fn drawn() -> Sim {
        let mut s = Sim::new();
        s.pen((50.0, 50.0));
        s.pen((150.0, 50.0));
        s.drag(Tool::Pen, (150.0, 150.0), (190.0, 150.0), NONE);
        s
    }

    #[test]
    fn clicks_place_corners_and_drags_place_curve_points() {
        assert_eq!(
            drawn().nodes(),
            [
                corner(50.0, 50.0),
                corner(150.0, 50.0),
                (150.0, 150.0, (-40.0, 0.0), (40.0, 0.0))
            ]
        );
    }

    #[test]
    fn drawing_resumes_after_other_tools() {
        let mut s = drawn();
        // Choosing a selection tool and coming back changes nothing.
        s.pen((50.0, 200.0));
        assert_eq!(s.nodes().len(), 4);
        assert_eq!(s.d.doc.layers[s.d.doc.current_layer].elements(0).len(), 1);
    }

    #[test]
    fn a_single_click_on_the_last_point_does_not_finish_the_path() {
        let mut s = drawn();
        s.pen((150.0, 150.0));
        s.pen((50.0, 200.0));
        assert_eq!(s.nodes().len(), 4);
    }

    #[test]
    fn dragging_a_point_pulls_handles_and_clicks_convert_then_delete() {
        let mut s = drawn();
        s.drag(Tool::Pen, (150.0, 50.0), (190.0, 50.0), NONE);
        assert_eq!(s.nodes()[1], (150.0, 50.0, (-40.0, 0.0), (40.0, 0.0)));
        // A curve point can be dragged again.
        s.drag(Tool::Pen, (150.0, 50.0), (150.0, 80.0), NONE);
        assert_eq!(s.nodes()[1], (150.0, 50.0, (0.0, -30.0), (0.0, 30.0)));
        s.pen((150.0, 50.0));
        assert_eq!(s.nodes()[1], corner(150.0, 50.0));
        s.pen((150.0, 50.0));
        assert_eq!(s.nodes().len(), 2);
        assert_eq!(s.nodes()[1].0, 150.0);
        assert_eq!(s.nodes()[1].1, 150.0);
    }

    #[test]
    fn shift_and_alt_while_placing_points() {
        let mut s = drawn();
        s.pen((300.0, 100.0));
        s.drag(Tool::Pen, (300.0, 200.0), (340.0, 225.0), SHIFT);
        let last = s.nodes()[4];
        assert_eq!((last.0, last.1), (300.0, 200.0));
        assert_eq!((last.2, last.3), ((-40.0, -40.0), (40.0, 40.0)));
        s.drag(Tool::Pen, (250.0, 250.0), (300.0, 260.0), ALT);
        assert_eq!(s.nodes()[5], (250.0, 250.0, ZERO, (50.0, 10.0)));
        // Shift places a point at 45-degree steps from the last one.
        let mut s = Sim::new();
        s.pen((200.0, 240.0));
        s.click(Tool::Pen, (260.0, 255.0), SHIFT);
        assert_eq!(s.nodes()[1], corner(260.0, 240.0));
    }

    #[test]
    fn closing_with_a_drag_sets_only_the_incoming_handle() {
        let mut s = drawn();
        s.drag(Tool::Pen, (50.0, 50.0), (80.0, 50.0), NONE);
        assert!(s.closed());
        assert_eq!(s.nodes()[0], (50.0, 50.0, (-30.0, 0.0), ZERO));
        // The next click starts another path.
        s.pen((30.0, 280.0));
        assert_eq!(s.nodes().len(), 1);
    }

    #[test]
    fn ctrl_with_the_pen_moves_points() {
        let mut s = Sim::new();
        s.pen((30.0, 280.0));
        s.pen((120.0, 280.0));
        let ctrl = egui::Modifiers::CTRL;
        s.drag(Tool::Pen, (120.0, 280.0), (130.0, 270.0), ctrl);
        assert_eq!(s.nodes()[1], corner(130.0, 270.0));
        // Still drawing.
        s.pen((200.0, 200.0));
        assert_eq!(s.nodes().len(), 3);
    }

    #[test]
    fn double_click_finishes_and_a_new_click_starts_afresh() {
        let mut s = Sim::new();
        s.pen((30.0, 280.0));
        s.double((130.0, 270.0));
        assert_eq!(s.nodes(), [corner(30.0, 280.0), corner(130.0, 270.0)]);
        s.pen((200.0, 280.0));
        assert_eq!(s.nodes(), [corner(200.0, 280.0)]);
    }

    #[test]
    fn clicking_another_paths_end_joins_it_and_keeps_drawing() {
        let mut s = Sim::new();
        s.pen((30.0, 280.0));
        s.double((130.0, 270.0));
        s.pen((200.0, 280.0));
        s.double((280.0, 280.0));
        s.pen((280.0, 280.0)); // the selected path's end: continue it
        s.pen((130.0, 270.0));
        assert_eq!(s.d.doc.layers[s.d.doc.current_layer].elements(0).len(), 1);
        assert_eq!(
            s.nodes(),
            [
                corner(200.0, 280.0),
                corner(280.0, 280.0),
                corner(130.0, 270.0),
                corner(30.0, 280.0)
            ]
        );
        s.pen((200.0, 240.0));
        assert_eq!(s.nodes().len(), 5);
    }

    #[test]
    fn clicking_a_segment_adds_a_point_even_while_drawing() {
        let mut s = Sim::new();
        s.pen((200.0, 280.0));
        s.pen((280.0, 280.0));
        s.pen((280.0, 200.0));
        s.pen((240.0, 280.0));
        assert_eq!(
            s.nodes(),
            [
                corner(200.0, 280.0),
                corner(240.0, 280.0),
                corner(280.0, 280.0),
                corner(280.0, 200.0)
            ]
        );
    }

    /// The clean curve the handle tests were recorded on.
    fn curve() -> Sim {
        let mut s = Sim::new();
        s.pen((50.0, 100.0));
        s.drag(Tool::Pen, (150.0, 100.0), (150.0, 140.0), NONE);
        s.drag(Tool::Pen, (250.0, 100.0), (290.0, 100.0), NONE);
        s.double((320.0, 200.0));
        s
    }

    #[test]
    fn handles_keep_smooth_points_in_line() {
        let mut s = curve();
        s.click(Tool::Subselect, (150.0, 100.0), NONE);
        s.drag(Tool::Subselect, (150.0, 140.0), (190.0, 140.0), NONE);
        let b = s.nodes()[1];
        assert_eq!(b.3, (40.0, 40.0));
        assert!(
            (b.2.0 + 28.284).abs() < 0.01 && (b.2.1 + 28.284).abs() < 0.01,
            "{b:?}"
        );
        // Alt: this handle only.
        s.drag(Tool::Subselect, (190.0, 140.0), (190.0, 170.0), ALT);
        let c = s.nodes()[1];
        assert_eq!(c.3, (40.0, 70.0));
        assert_eq!(c.2, b.2);
    }

    #[test]
    fn a_neighbours_near_handle_can_be_dragged() {
        let mut s = curve();
        s.click(Tool::Subselect, (150.0, 100.0), NONE);
        // The next point's incoming handle, at 210, 100.
        s.drag(Tool::Subselect, (210.0, 100.0), (210.0, 60.0), NONE);
        let b = s.nodes()[2];
        assert_eq!(b.2, (-40.0, -40.0));
        assert!(
            (b.3.0 - 28.284).abs() < 0.01 && (b.3.1 - 28.284).abs() < 0.01,
            "{b:?}"
        );
    }

    #[test]
    fn alt_on_a_selected_corner_pulls_its_outgoing_handle() {
        let mut s = curve();
        s.click(Tool::Subselect, (320.0, 200.0), NONE);
        s.drag(Tool::Subselect, (320.0, 200.0), (350.0, 230.0), ALT);
        assert_eq!(s.nodes()[3], (320.0, 200.0, ZERO, (30.0, 30.0)));
    }

    #[test]
    fn alt_on_an_unselected_point_just_moves_it() {
        let mut s = curve();
        s.click(Tool::Subselect, (320.0, 200.0), NONE);
        s.edit.points.clear();
        s.drag(Tool::Subselect, (320.0, 200.0), (350.0, 230.0), ALT);
        assert_eq!(s.nodes()[3], corner(350.0, 230.0));
    }

    #[test]
    fn a_point_appears_in_history_on_release() {
        let mut s = Sim::new();
        s.pen((30.0, 30.0));
        let steps = s.d.history.labels().count();
        s.press(Tool::Pen, (100.0, 30.0), NONE);
        assert_eq!(s.d.history.labels().count(), steps, "nothing recorded on press");
        s.up((100.0, 30.0), NONE);
        assert_eq!(s.d.history.labels().count(), steps + 1);
    }
}
