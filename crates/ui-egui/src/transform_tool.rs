//! Scale, Skew and Distort tools (Fireworks MX's transform tools, sharing the Scale slot).
//!
//! A box with eight handles surrounds the selection. Drag inside it to move, outside it to
//! rotate (Shift: 15 degree steps), or a handle:
//! - Scale: corners and edges resize (Shift keeps proportions, Alt scales about the centre);
//! - Skew: edges slide along themselves, corners pinch symmetrically (perspective);
//! - Distort: corners and edges move freely.
//!
//! Each release applies one transform (box -> quadrilateral, perspective where needed) to the
//! objects as they were when the edit began, so repeated adjustments resample bitmaps once.

use egui::{Color32, Pos2, Rect, Stroke, StrokeKind, pos2, vec2};
use firecraft_doc::Element;
use firecraft_engine::{DocState, ElementRef, Tool};
use firecraft_raster::transform::Affine;

use crate::{FireCraftApp, theme};

type Pt = (f64, f64);

/// The selection being transformed.
#[derive(Clone, Debug)]
pub struct TransformEdit {
    doc: u64,
    refs: Vec<ElementRef>,
    /// Document revision after our last change (another change starts a new edit).
    revision: u64,
    originals: Vec<(ElementRef, Element)>,
    /// The originals' bounds.
    rect: (f64, f64, f64, f64),
    /// Where the box's corners are now: top-left, top-right, bottom-right, bottom-left.
    quad: [Pt; 4],
    /// Pivot (rotation centre) in box coordinates, (0.5, 0.5) = the middle; it keeps its place
    /// in the box as the box moves and changes shape.
    pivot_uv: Pt,
    drag: Option<Drag>,
}

#[derive(Clone, Copy, Debug)]
struct Drag {
    handle: Handle,
    start_quad: [Pt; 4],
    /// The pivot when the drag began (document coordinates).
    pivot: Pt,
    start: Pt,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Handle {
    Corner(usize),
    /// 0 top, 1 right, 2 bottom, 3 left.
    Edge(usize),
    Move,
    Rotate,
    Pivot,
}

fn corners(r: (f64, f64, f64, f64)) -> [Pt; 4] {
    let (x, y, w, h) = r;
    [(x, y), (x + w, y), (x + w, y + h), (x, y + h)]
}

/// The current edit for the selection, started fresh when the selection or document changed.
fn sync<'a>(edit: &'a mut Option<TransformEdit>, d: &DocState) -> Option<&'a mut TransformEdit> {
    let rect = d.selection_bounds()?;
    let stale = edit
        .as_ref()
        .is_none_or(|e| e.doc != d.id || e.refs != d.selection || e.revision != d.revision);
    if stale {
        *edit = Some(TransformEdit {
            doc: d.id,
            refs: d.selection.clone(),
            revision: d.revision,
            originals: d.selected_elements(),
            rect,
            quad: corners(rect),
            pivot_uv: (0.5, 0.5),
            drag: None,
        });
    }
    edit.as_mut()
}

fn mid(a: Pt, b: Pt) -> Pt {
    ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0)
}

fn add(a: Pt, b: Pt) -> Pt {
    (a.0 + b.0, a.1 + b.1)
}

fn sub(a: Pt, b: Pt) -> Pt {
    (a.0 - b.0, a.1 - b.1)
}

fn dot(a: Pt, b: Pt) -> f64 {
    a.0 * b.0 + a.1 * b.1
}

fn scale(a: Pt, k: f64) -> Pt {
    (a.0 * k, a.1 * k)
}

fn centre(q: &[Pt; 4]) -> Pt {
    (
        q.iter().map(|p| p.0).sum::<f64>() / 4.0,
        q.iter().map(|p| p.1).sum::<f64>() / 4.0,
    )
}

/// Point-in-quad (convex or not) by crossing count.
fn inside(q: &[Pt; 4], p: Pt) -> bool {
    let mut c = false;
    for i in 0..4 {
        let (a, b) = (q[i], q[(i + 1) % 4]);
        if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) * (b.0 - a.0) / (b.1 - a.1) {
            c = !c;
        }
    }
    c
}

/// Edge `e`'s corner indices.
fn edge(e: usize) -> (usize, usize) {
    (e % 4, (e + 1) % 4)
}

/// The new box for a drag.
fn dragged(tool: Tool, drag: &Drag, p: Pt, shift: bool, alt: bool) -> [Pt; 4] {
    let q = drag.start_quad;
    let delta = sub(p, drag.start);
    match drag.handle {
        Handle::Move => q.map(|c| add(c, delta)),
        Handle::Pivot => q,
        Handle::Rotate => {
            let c = drag.pivot;
            let a0 = (drag.start.1 - c.1).atan2(drag.start.0 - c.0);
            let mut da = (p.1 - c.1).atan2(p.0 - c.0) - a0;
            if shift {
                let step = 15f64.to_radians();
                da = (da / step).round() * step;
            }
            let (s, co) = da.sin_cos();
            q.map(|v| {
                let r = sub(v, c);
                add(c, (r.0 * co - r.1 * s, r.0 * s + r.1 * co))
            })
        }
        Handle::Corner(k) | Handle::Edge(k) if tool == Tool::Scale => {
            scale_box(&q, drag.handle, k, p, shift, alt)
        }
        Handle::Corner(k) if tool == Tool::Skew => {
            // Pinch: the corner moves along one side, its partner on that side moves opposite.
            let (u, v) = (sub(q[1], q[0]), sub(q[3], q[0]));
            let (lu, lv) = (dot(u, u).sqrt().max(1e-9), dot(v, v).sqrt().max(1e-9));
            let (du, dv) = (dot(delta, u) / lu, dot(delta, v) / lv);
            let mut out = q;
            if du.abs() >= dv.abs() {
                let partner = [1, 0, 3, 2][k];
                out[k] = add(q[k], scale(u, du / lu));
                out[partner] = add(q[partner], scale(u, -du / lu));
            } else {
                let partner = [3, 2, 1, 0][k];
                out[k] = add(q[k], scale(v, dv / lv));
                out[partner] = add(q[partner], scale(v, -dv / lv));
            }
            out
        }
        Handle::Edge(e) if tool == Tool::Skew => {
            // Slide the edge along itself.
            let (a, b) = edge(e);
            let dir = sub(q[b], q[a]);
            let l = dot(dir, dir).max(1e-9);
            let shift_along = scale(dir, dot(delta, dir) / l);
            let mut out = q;
            out[a] = add(q[a], shift_along);
            out[b] = add(q[b], shift_along);
            out
        }
        Handle::Corner(k) => {
            let mut out = q;
            out[k] = add(q[k], delta);
            out
        }
        Handle::Edge(e) => {
            let (a, b) = edge(e);
            let mut out = q;
            out[a] = add(q[a], delta);
            out[b] = add(q[b], delta);
            out
        }
    }
}

/// Scale in the box's own frame (it stays a parallelogram while scaling and rotating).
fn scale_box(q: &[Pt; 4], handle: Handle, k: usize, p: Pt, shift: bool, alt: bool) -> [Pt; 4] {
    let (o, u, v) = (q[0], sub(q[1], q[0]), sub(q[3], q[0]));
    let det = u.0 * v.1 - u.1 * v.0;
    if det.abs() < 1e-9 {
        return *q;
    }
    // Pointer in box coordinates (0..1 across the box).
    let r = sub(p, o);
    let (s, t) = ((r.0 * v.1 - r.1 * v.0) / det, (u.0 * r.1 - u.1 * r.0) / det);
    let unit = [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
    let at = |a: f64, b: f64| add(o, add(scale(u, a), scale(v, b)));
    if let Handle::Corner(_) = handle {
        let c = unit[k];
        // The fixed point: the opposite corner, or the centre with Alt.
        let (fx, fy) = if alt { (0.5, 0.5) } else { (1.0 - c.0, 1.0 - c.1) };
        let (mut nx, mut ny) = (s, t);
        if shift {
            // Keep proportions: the larger relative change wins.
            let (kx, ky) = ((nx - fx) / (c.0 - fx), (ny - fy) / (c.1 - fy));
            let f = if kx.abs() > ky.abs() { kx } else { ky };
            nx = fx + (c.0 - fx) * f;
            ny = fy + (c.1 - fy) * f;
        }
        // The corner opposite the dragged one (mirrored about the centre with Alt).
        let (ox, oy) = if alt { (1.0 - nx, 1.0 - ny) } else { (fx, fy) };
        // Each corner takes the dragged corner's x/y where it shares that side, else the
        // opposite corner's (dragging past the far side mirrors naturally).
        return unit.map(|j| at(if j.0 == c.0 { nx } else { ox }, if j.1 == c.1 { ny } else { oy }));
    }
    let (mut x0, mut y0, mut x1, mut y1) = (0.0, 0.0, 1.0, 1.0);
    if let Handle::Edge(e) = handle {
        match e {
            0 => y0 = t,
            1 => x1 = s,
            2 => y1 = t,
            _ => x0 = s,
        }
        if alt {
            match e {
                0 => y1 = 1.0 - y0,
                1 => x0 = 1.0 - x1,
                2 => y0 = 1.0 - y1,
                _ => x1 = 1.0 - x0,
            }
        }
    }
    [at(x0, y0), at(x1, y0), at(x1, y1), at(x0, y1)]
}

/// Box coordinates (0..1) <-> document coordinates through the box's current shape.
fn box_map(q: &[Pt; 4]) -> Option<Affine> {
    Affine::rect_to_quad((0.0, 0.0, 1.0, 1.0), *q)
}

fn pivot_of(edit: &TransformEdit) -> Pt {
    box_map(&edit.quad).map_or_else(
        || centre(&edit.quad),
        |m| m.apply(edit.pivot_uv.0, edit.pivot_uv.1),
    )
}

/// What a press at screen point `q` would grab.
fn pick(
    edit: &TransformEdit,
    q: Pos2,
    to_screen: &dyn Fn(Pt) -> Pos2,
    to_doc: &dyn Fn(Pos2) -> Pt,
    shift: bool,
) -> Handle {
    let is_inside = inside(&edit.quad, to_doc(q));
    // Handles grab within 6 px inside the box but only on their square outside it, where
    // dragging rotates (as in Fireworks).
    let near = |pt: Pt| {
        let dist = to_screen(pt).distance(q);
        dist <= if is_inside { 6.0 } else { 4.0 }
    };
    let pivot = near(pivot_of(edit));
    // Shift: the handle under the pivot rather than the pivot.
    if pivot && !shift {
        return Handle::Pivot;
    }
    if let Some(k) = (0..4).find(|&k| near(edit.quad[k])) {
        return Handle::Corner(k);
    }
    if let Some(e) = (0..4).find(|&e| near(mid(edit.quad[edge(e).0], edit.quad[edge(e).1]))) {
        return Handle::Edge(e);
    }
    if pivot {
        return Handle::Pivot;
    }
    if is_inside { Handle::Move } else { Handle::Rotate }
}

/// Snap points for the pivot in box coordinates: corners, edge midpoints and the centre.
const SNAPS: [Pt; 9] = [
    (0.0, 0.0),
    (0.5, 0.0),
    (1.0, 0.0),
    (1.0, 0.5),
    (1.0, 1.0),
    (0.5, 1.0),
    (0.0, 1.0),
    (0.0, 0.5),
    (0.5, 0.5),
];

/// The pivot at document point `p` in box coordinates, snapped to the nearest snap point
/// within 8 screen pixels unless `free` (Ctrl).
fn pivot_at(edit: &TransformEdit, p: Pt, to_screen: &dyn Fn(Pt) -> Pos2, free: bool) -> Option<Pt> {
    let m = box_map(&edit.quad)?;
    let uv = m.invert()?.apply(p.0, p.1);
    if free {
        return Some(uv);
    }
    let here = to_screen(p);
    let best = SNAPS
        .iter()
        .map(|&s| (s, to_screen(m.apply(s.0, s.1)).distance(here)))
        .min_by(|a, b| a.1.total_cmp(&b.1));
    Some(match best {
        Some((snap, dist)) if dist <= 8.0 => snap,
        _ => uv,
    })
}

/// The resize cursor closest to screen direction `dir` (y down).
fn resize_cursor(dir: egui::Vec2) -> egui::CursorIcon {
    let a = dir.y.atan2(dir.x).to_degrees().rem_euclid(180.0);
    match ((a + 22.5) / 45.0) as i32 % 4 {
        0 => egui::CursorIcon::ResizeHorizontal,
        1 => egui::CursorIcon::ResizeNwSe,
        2 => egui::CursorIcon::ResizeVertical,
        _ => egui::CursorIcon::ResizeNeSw,
    }
}

/// Cursor for a handle, showing what dragging it does.
fn cursor_for(
    tool: Tool,
    h: Handle,
    edit: &TransformEdit,
    to_screen: &dyn Fn(Pt) -> Pos2,
) -> egui::CursorIcon {
    let q = &edit.quad;
    let c = to_screen(centre(q));
    match h {
        Handle::Move => egui::CursorIcon::Move,
        Handle::Rotate => egui::CursorIcon::None, // drawn: a rotate arrow
        Handle::Pivot => egui::CursorIcon::Crosshair,
        Handle::Corner(_) | Handle::Edge(_) if tool == Tool::Distort => egui::CursorIcon::Crosshair,
        Handle::Edge(e) if tool == Tool::Skew => {
            // Sliding along the edge.
            let (a, b) = edge(e);
            resize_cursor(to_screen(q[b]) - to_screen(q[a]))
        }
        Handle::Corner(_) if tool == Tool::Skew => resize_cursor(to_screen(q[1]) - to_screen(q[0])),
        Handle::Corner(k) => resize_cursor(to_screen(q[k]) - c),
        Handle::Edge(e) => resize_cursor(to_screen(mid(q[edge(e).0], q[edge(e).1])) - c),
    }
}

/// A rotate cursor: a three-quarter circle with an arrowhead, drawn at the pointer.
fn paint_rotate_cursor(p: &egui::Painter, at: Pos2) {
    let r = 7.0;
    let pts: Vec<Pos2> = (0..=18)
        .map(|i| {
            let a = (-200.0 + i as f32 * 14.0).to_radians();
            at + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    let end = *pts.last().unwrap_or(&at);
    for (w, col) in [(3.5, Color32::WHITE), (1.5, Color32::BLACK)] {
        p.add(egui::Shape::line(pts.clone(), Stroke::new(w, col)));
        p.add(egui::Shape::convex_polygon(
            vec![
                end + vec2(-4.0, -1.0),
                end + vec2(3.0, -3.0),
                end + vec2(1.0, 4.0),
            ],
            col,
            Stroke::new(w - 1.5, col),
        ));
    }
}

pub fn canvas(
    transform: &mut Option<TransformEdit>,
    d: &mut DocState,
    tool: Tool,
    p: &egui::Painter,
    resp: &egui::Response,
    origin: Pos2,
    z: f32,
) {
    let to_screen = |q: Pt| pos2(origin.x + q.0 as f32 * z, origin.y + q.1 as f32 * z);
    let to_doc = |q: Pos2| (((q.x - origin.x) / z) as f64, ((q.y - origin.y) / z) as f64);
    let mods = resp.ctx.input(|i| i.modifiers);

    // Clicking selects objects, as with the Pointer.
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let (x, y) = to_doc(pos);
            let keep = transform.as_ref().is_some_and(|e| inside(&e.quad, (x, y)));
            if !keep {
                match d.hit_test(x, y) {
                    Some(hit) => d.selection = vec![hit],
                    None => d.deselect(),
                }
                d.pixels = None;
            }
        }
    }
    let Some(edit) = sync(transform, d) else { return };

    if resp.drag_started() {
        if let Some(q) = resp
            .ctx
            .input(|i| i.pointer.press_origin())
            .or(resp.interact_pointer_pos())
        {
            let handle = pick(edit, q, &to_screen, &to_doc, mods.shift);
            edit.drag = Some(Drag {
                handle,
                start_quad: edit.quad,
                pivot: pivot_of(edit),
                start: to_doc(q),
            });
        }
    }
    if let (Some(drag), Some(q)) = (edit.drag, resp.interact_pointer_pos()) {
        if drag.handle == Handle::Pivot {
            // Move the pivot (snapping to the box's corners, edge midpoints and centre; Ctrl:
            // free); it's stored in box coordinates.
            if let Some(uv) = pivot_at(edit, to_doc(q), &to_screen, mods.ctrl || mods.command) {
                edit.pivot_uv = uv;
            }
        } else {
            edit.quad = dragged(tool, &drag, to_doc(q), mods.shift, mods.alt);
        }
    }
    // Cursor: what a drag here does (or is doing).
    if let Some(hover) = resp.hover_pos().or(resp.interact_pointer_pos()) {
        let h = edit.drag.map_or_else(
            || pick(edit, hover, &to_screen, &to_doc, mods.shift),
            |d| d.handle,
        );
        let icon = cursor_for(tool, h, edit, &to_screen);
        resp.ctx.set_cursor_icon(icon);
        if icon == egui::CursorIcon::None {
            paint_rotate_cursor(p, hover);
        }
    }
    if resp.drag_stopped() {
        if let Some(drag) = edit.drag.take() {
            let label = match (drag.handle, tool) {
                (Handle::Move, _) => "Move",
                (Handle::Rotate, _) => "Rotate",
                (_, Tool::Skew) => "Skew",
                (_, Tool::Distort) => "Distort",
                _ => "Scale",
            };
            if edit.quad != drag.start_quad {
                if let Some(m) = Affine::rect_to_quad(edit.rect, edit.quad) {
                    let originals = edit.originals.clone();
                    d.transform_from(&originals, &m, label);
                    edit.revision = d.revision;
                }
            }
        }
    }

    // Overlay: the box, its handles and centre.
    let pts: Vec<Pos2> = edit.quad.iter().map(|&c| to_screen(c)).collect();
    p.add(egui::Shape::closed_line(
        pts.clone(),
        Stroke::new(1.0, theme::HANDLE_BLUE),
    ));
    let handle = |c: Pos2| {
        let r = Rect::from_center_size(c, vec2(7.0, 7.0));
        p.rect_filled(r, 0.0, Color32::WHITE);
        p.rect_stroke(r, 0.0, Stroke::new(1.0, theme::HANDLE_BLUE), StrokeKind::Inside);
    };
    for k in 0..4 {
        handle(pts[k]);
        handle(to_screen(mid(edit.quad[edge(k).0], edit.quad[edge(k).1])));
    }
    // The pivot (drag it to rotate about another point).
    let c = to_screen(pivot_of(edit));
    p.circle_filled(c, 3.5, Color32::WHITE);
    p.circle_stroke(c, 3.5, Stroke::new(1.0, theme::HANDLE_BLUE));
}

pub fn inspector(app: &mut FireCraftApp, ui: &mut egui::Ui, tool: Tool) {
    ui.vertical(|ui| {
        ui.set_width(150.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), egui::Sense::hover());
            crate::icons::paint(
                ui.painter(),
                Rect::from_center_size(r.center(), vec2(20.0, 20.0)),
                tool,
                true,
            );
            ui.label(egui::RichText::new(tool.label()).size(11.0));
        });
    });
    ui.separator();
    ui.vertical(|ui| {
        ui.label(match tool {
            Tool::Skew => "Drag a side handle to slant; a corner handle to taper (perspective).",
            Tool::Distort => "Drag any corner or side handle freely.",
            _ => "Drag a handle to resize (Shift: keep proportions, Alt: from the centre).",
        });
        ui.weak("Drag inside the box to move, outside it to rotate (Shift: 15 degree steps) about the pivot. Drag the pivot to move it: it snaps to the box's corners, sides and centre (Ctrl: no snapping; Shift: grab the handle under the pivot).");
        let has = app.session.active_doc().is_some_and(|d| !d.selection.is_empty());
        if !has {
            ui.weak("Select one or more objects first.");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drag(handle: Handle, q: [Pt; 4], start: Pt) -> Drag {
        Drag {
            handle,
            start_quad: q,
            pivot: centre(&q),
            start,
        }
    }

    #[test]
    fn scale_skew_distort_shapes() {
        let q = corners((0.0, 0.0, 100.0, 50.0));
        // Scale: bottom-right corner to (200, 100) doubles the box.
        let s = dragged(
            Tool::Scale,
            &drag(Handle::Corner(2), q, (100.0, 50.0)),
            (200.0, 100.0),
            false,
            false,
        );
        assert_eq!(s, [(0.0, 0.0), (200.0, 0.0), (200.0, 100.0), (0.0, 100.0)]);
        // Skew: top edge dragged right by 20 slides only the top.
        let k = dragged(
            Tool::Skew,
            &drag(Handle::Edge(0), q, (50.0, 0.0)),
            (70.0, 3.0),
            false,
            false,
        );
        assert_eq!(k, [(20.0, 0.0), (120.0, 0.0), (100.0, 50.0), (0.0, 50.0)]);
        // Skew: top-right corner dragged right by 10 widens the top symmetrically.
        let t = dragged(
            Tool::Skew,
            &drag(Handle::Corner(1), q, (100.0, 0.0)),
            (110.0, 0.0),
            false,
            false,
        );
        assert_eq!(t, [(-10.0, 0.0), (110.0, 0.0), (100.0, 50.0), (0.0, 50.0)]);
        // Distort: one corner moves alone.
        let d = dragged(
            Tool::Distort,
            &drag(Handle::Corner(3), q, (0.0, 50.0)),
            (15.0, 70.0),
            false,
            false,
        );
        assert_eq!(d, [(0.0, 0.0), (100.0, 0.0), (100.0, 50.0), (15.0, 70.0)]);
        // Move.
        let m = dragged(
            Tool::Scale,
            &drag(Handle::Move, q, (10.0, 10.0)),
            (15.0, 12.0),
            false,
            false,
        );
        assert_eq!(m[0], (5.0, 2.0));
        // Rotate 90 degrees about a pivot at the top-left corner.
        let mut r = drag(Handle::Rotate, q, (10.0, 0.0));
        r.pivot = (0.0, 0.0);
        let rot = dragged(Tool::Scale, &r, (0.0, 10.0), false, false);
        assert!(
            (rot[1].0).abs() < 1e-9 && (rot[1].1 - 100.0).abs() < 1e-9,
            "{rot:?}"
        );
        assert!(rot[0].0.abs() < 1e-9 && rot[0].1.abs() < 1e-9);
    }

    #[test]
    fn pivot_snaps_unless_free() {
        let rect = (0.0, 0.0, 100.0, 50.0);
        let edit = TransformEdit {
            doc: 0,
            refs: Vec::new(),
            revision: 0,
            originals: Vec::new(),
            rect,
            quad: corners(rect),
            pivot_uv: (0.5, 0.5),
            drag: None,
        };
        let to_screen = |p: Pt| pos2(p.0 as f32, p.1 as f32);
        // 3 px from the top-right corner: snaps there.
        assert_eq!(pivot_at(&edit, (97.0, 2.0), &to_screen, false), Some((1.0, 0.0)));
        // Ctrl: stays where it was dropped.
        let free = pivot_at(&edit, (97.0, 2.0), &to_screen, true).unwrap();
        assert!((free.0 - 0.97).abs() < 1e-9 && (free.1 - 0.04).abs() < 1e-9);
        // Far from every snap point: free.
        let mid = pivot_at(&edit, (30.0, 20.0), &to_screen, false).unwrap();
        assert!((mid.0 - 0.3).abs() < 1e-9);
    }
}
