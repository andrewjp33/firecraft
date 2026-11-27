//! The Crop tool and Edit > Crop Selected Bitmap: a crop box on the canvas, as in Fireworks MX
//! (measured 2026-11-16, docs/behavior/crop.md).
//!
//! * Dragging outside the box draws a new one, kept on the canvas: the corner being dragged
//!   stays on it, and so does where the drag began, unless both ends lie past the same side
//!   of the canvas (a drag from -20,-20 to -40,-40 gives -20,-20 to 0,0).
//! * The eight handles and the four sides resize it, also past the canvas (the canvas then
//!   grows), and turn it over when dragged past the opposite side. Dragging inside the box
//!   moves it. While resizing or moving, a side that ends up to 5 screen pixels past the
//!   canvas snaps to the canvas's edge; Ctrl turns this off.
//! * Double-clicking inside or pressing Enter crops; the Crop tool stays chosen. Escape drops
//!   the box and chooses the Pointer tool; choosing another tool drops it too, except the Hand
//!   and Zoom tools, which keep it.
//! * The box is marching ants with small black handles; the outside isn't dimmed.
//! * Choosing the Crop tool with pixels selected makes a box of the selection's size. The arrow
//!   keys move the box (Shift: 10 px).
//! * Crop Selected Bitmap puts the box around the bitmap; cropping trims the bitmap (undo step
//!   "Crop Image") and goes back to the Pointer tool.

use egui::{Color32, Pos2, Rect, Stroke, pos2, vec2};
use firecraft_engine::{CropRect, DocState, ElementRef, Tool};

use crate::FireCraftApp;
use crate::cursors::{self, Cur, CursorStyle};

/// What the box crops.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CropTarget {
    Document,
    /// Edit > Crop Selected Bitmap.
    Bitmap(ElementRef),
}

/// The crop box being edited in the active document.
#[derive(Clone, Debug, PartialEq)]
pub struct CropEdit {
    doc: u64,
    target: CropTarget,
    /// Canvas pixels (left, top, right, bottom); `None` until one is drawn.
    pub rect: Option<CropRect>,
    drag: Option<Drag>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// A new box from this canvas point.
    New((i64, i64)),
    /// Moving the box from where it was when the drag began.
    Move((f64, f64), CropRect),
    /// One of the eight handles: which sides it moves (-1 left/top, 1 right/bottom, 0 neither).
    Handle((i8, i8), CropRect),
}

/// The handles: corners and side midpoints.
const HANDLES: [(i8, i8); 8] = [(-1, -1), (0, -1), (1, -1), (1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0)];
/// Handle size and how near the pointer must be to take one (screen points).
const HANDLE: f32 = 5.0;
const GRAB: f32 = 5.0;
/// How near a side of the box the pointer must be to drag that side (screen points).
const SIDE_GRAB: f32 = 3.0;
/// How far past the canvas a side of the box may be let go and still snap to the canvas's
/// edge (screen points; Fireworks MX: 5 at 100% and at 200%, measured 2026-11-26).
const SNAP: f64 = 5.0;

/// The canvas edge (0 or `size`) that canvas coordinate `v` snaps to, when it lies just past
/// it: up to [`SNAP`] screen points outside, at zoom `z`. Inside the canvas nothing snaps.
fn snap(v: f64, size: i64, z: f64) -> Option<i64> {
    if v < 0.0 && -v * z <= SNAP {
        Some(0)
    } else if v > size as f64 && (v - size as f64) * z <= SNAP {
        Some(size)
    } else {
        None
    }
}

/// One axis of a new box drawn from `start` to `end` over a canvas `size` long: the dragged
/// end stays on the canvas, and so does the start, unless both lie past the same edge (the
/// start then stays put, so the box doesn't vanish). Measured in Fireworks MX 2026-11-26:
/// -20 to 320 over 300 gave 0 to 300, -20 to -40 gave -20 to 0, 320 to 370 gave 300 to 320.
fn new_side(start: i64, end: f64, size: i64) -> (i64, i64) {
    let e = (end.round() as i64).clamp(0, size);
    let s = start.clamp(0, size);
    (if s == e { start } else { s }, e)
}

fn handle_pos(r: CropRect, (hx, hy): (i8, i8)) -> (f64, f64) {
    let pick = |a: i64, b: i64, k: i8| match k {
        -1 => a as f64,
        1 => b as f64,
        _ => (a + b) as f64 / 2.0,
    };
    (pick(r.0, r.2, hx), pick(r.1, r.3, hy))
}

/// Left/right and top/bottom in order.
fn normalized(l: i64, t: i64, r: i64, b: i64) -> CropRect {
    (l.min(r), t.min(b), l.max(r), t.max(b))
}

impl CropEdit {
    fn new(doc: u64, target: CropTarget, rect: Option<CropRect>) -> Self {
        Self {
            doc,
            target,
            rect,
            drag: None,
        }
    }
}

/// Edit > Crop Selected Bitmap: a crop box around the bitmap being edited or selected, with
/// the Crop tool.
pub fn begin_bitmap_crop(app: &mut FireCraftApp) {
    let Some(d) = app.session.active_doc() else { return };
    let Some(r) = d.crop_bitmap_target() else { return };
    let rect = d.bitmap_rect(r);
    app.crop = Some(CropEdit::new(d.id, CropTarget::Bitmap(r), rect));
    app.session.tool = Tool::Crop;
}

/// Crop with the box (Enter, double-click). `true` if the Pointer tool should be chosen.
fn commit(crop: &mut Option<CropEdit>, d: &mut DocState) -> bool {
    let Some(edit) = crop.take() else { return false };
    match (edit.target, edit.rect) {
        (CropTarget::Document, Some(r)) => {
            d.crop_document(r);
            false
        }
        (CropTarget::Bitmap(b), Some(r)) => {
            d.crop_bitmap(b, r);
            true
        }
        (CropTarget::Bitmap(_), None) => true,
        (CropTarget::Document, None) => false,
    }
}

/// Input and overlay on the canvas of the active document. Returns `true` when the Pointer tool
/// should be chosen (Escape, or a bitmap crop finished).
pub fn canvas(
    crop: &mut Option<CropEdit>,
    d: &mut DocState,
    p: &egui::Painter,
    resp: &egui::Response,
    (origin, z): (Pos2, f32),
    keys_free: bool,
    style: CursorStyle,
) -> bool {
    let ctx = resp.ctx.clone();
    let to_screen = |x: f64, y: f64| pos2(origin.x + x as f32 * z, origin.y + y as f32 * z);
    let to_doc = |q: Pos2| (((q.x - origin.x) / z) as f64, ((q.y - origin.y) / z) as f64);
    // A box belongs to one document; a bitmap crop ends when its bitmap is gone (undo).
    if crop.as_ref().is_some_and(|c| {
        c.doc != d.id
            || matches!(c.target, CropTarget::Bitmap(b) if d.bitmap_rect(b).is_none())
    }) {
        *crop = None;
    }
    if crop.is_none() {
        // Choosing the Crop tool with pixels selected turns the selection into a crop box of
        // the same size (Fireworks MX); bitmap editing ends, as choosing the Pointer ends it.
        let from_pixels = d.pixels.as_ref().and_then(|p| {
            let (x, y, w, h) = p.mask.bounds()?;
            let (l, t) = (p.x + i64::from(x), p.y + i64::from(y));
            Some((l, t, l + i64::from(w), t + i64::from(h)))
        });
        if from_pixels.is_some() {
            d.exit_bitmap_mode();
        }
        *crop = Some(CropEdit::new(d.id, CropTarget::Document, from_pixels));
    }
    let Some(edit) = crop.as_mut() else { return false };

    // Keys: Enter crops, Escape cancels (and takes the Pointer tool, as Fireworks MX does).
    if keys_free {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            *crop = None;
            return true;
        }
        if edit.rect.is_some() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
            return commit(crop, d);
        }
    }

    let (cw, ch) = (i64::from(d.doc.width), i64::from(d.doc.height));
    let screen_rect = |r: CropRect| Rect::from_min_max(to_screen(r.0 as f64, r.1 as f64), to_screen(r.2 as f64, r.3 as f64));
    // A handle, or else a side of the box (sides move like their middle handles, as in
    // Fireworks MX).
    let handle_at = |r: CropRect, q: Pos2| {
        // Corners first, so a small box's corners stay reachable.
        let mut order = HANDLES;
        order.sort_by_key(|h| (h.0 == 0 || h.1 == 0) as u8);
        order
            .into_iter()
            .find(|&h| {
                let (x, y) = handle_pos(r, h);
                let c = to_screen(x, y);
                (q.x - c.x).abs() <= GRAB && (q.y - c.y).abs() <= GRAB
            })
            .or_else(|| {
                let s = screen_rect(r);
                let near = |a: f32, b: f32| (a - b).abs() <= SIDE_GRAB;
                let across = (s.left()..=s.right()).contains(&q.x);
                let down = (s.top()..=s.bottom()).contains(&q.y);
                if down && near(q.x, s.left()) {
                    Some((-1, 0))
                } else if down && near(q.x, s.right()) {
                    Some((1, 0))
                } else if across && near(q.y, s.top()) {
                    Some((0, -1))
                } else if across && near(q.y, s.bottom()) {
                    Some((0, 1))
                } else {
                    None
                }
            })
    };
    let inside = |r: CropRect, q: Pos2| screen_rect(r).contains(q);

    if resp.double_clicked()
        && let (Some(r), Some(q)) = (edit.rect, resp.interact_pointer_pos())
        && inside(r, q)
    {
        return commit(crop, d);
    }

    if resp.drag_started()
        && let Some(q) = ctx.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos())
    {
        let (x, y) = to_doc(q);
        let handle = edit.rect.and_then(|r| Some((handle_at(r, q)?, r)));
        edit.drag = Some(match (handle, edit.rect) {
            (Some((h, r)), _) => Drag::Handle(h, r),
            (None, Some(r)) if inside(r, q) => Drag::Move((x, y), r),
            _ => Drag::New((x.round() as i64, y.round() as i64)),
        });
    }
    if let (Some(drag), Some(q)) = (edit.drag, resp.interact_pointer_pos())
        && resp.dragged()
    {
        let (x, y) = to_doc(q);
        // Sides let go just past the canvas snap to its edges, unless Ctrl is held.
        let free = ctx.input(|i| i.modifiers.ctrl || i.modifiers.command);
        let zf = f64::from(z);
        let place = |v: f64, size: i64| {
            let snapped = if free { None } else { snap(v, size, zf) };
            snapped.unwrap_or(v.round() as i64)
        };
        // How far a moved box goes along one axis: as far as the pointer, unless one of its
        // sides then snaps.
        let shift = |d: f64, a: i64, b: i64, size: i64| {
            let snapped = (!free)
                .then(|| {
                    snap(a as f64 + d, size, zf)
                        .map(|e| e - a)
                        .or_else(|| snap(b as f64 + d, size, zf).map(|e| e - b))
                })
                .flatten();
            snapped.unwrap_or(d.round() as i64)
        };
        edit.rect = Some(match drag {
            Drag::New((sx, sy)) => {
                let (l, r) = new_side(sx, x, cw);
                let (t, b) = new_side(sy, y, ch);
                normalized(l, t, r, b)
            }
            Drag::Move((fx, fy), r) => {
                let dx = shift(x - fx, r.0, r.2, cw);
                let dy = shift(y - fy, r.1, r.3, ch);
                (r.0 + dx, r.1 + dy, r.2 + dx, r.3 + dy)
            }
            Drag::Handle((hx, hy), r) => {
                let (mut l, mut t, mut rr, mut b) = r;
                let (x, y) = (place(x, cw), place(y, ch));
                match hx {
                    -1 => l = x,
                    1 => rr = x,
                    _ => {}
                }
                match hy {
                    -1 => t = y,
                    1 => b = y,
                    _ => {}
                }
                normalized(l, t, rr, b)
            }
        });
    }
    if resp.drag_stopped() {
        edit.drag = None;
        // A drag that made no area leaves no box.
        if edit.rect.is_some_and(|r| r.2 <= r.0 || r.3 <= r.1) {
            edit.rect = None;
        }
    }

    // Cursor: resize arrows over the handles, a move cursor inside the box, also while they
    // are dragged; elsewhere the crop cursor the canvas has set.
    let resize = |(hx, hy): (i8, i8)| match (hx, hy) {
        (0, _) => Cur::ResizeV,
        (_, 0) => Cur::ResizeH,
        (a, b) if a == b => Cur::ResizeNwSe,
        _ => Cur::ResizeNeSw,
    };
    let cur = match edit.drag {
        Some(Drag::Handle(h, _)) => Some(resize(h)),
        Some(Drag::Move(..)) => Some(Cur::Move),
        Some(Drag::New(_)) => None,
        None => edit.rect.zip(resp.hover_pos()).and_then(|(r, q)| match handle_at(r, q) {
            Some(h) => Some(resize(h)),
            None => inside(r, q).then_some(Cur::Move),
        }),
    };
    if let Some(cur) = cur
        && (resp.hovered() || resp.dragged())
    {
        cursors::set(&ctx, cur, style);
    }

    overlay(crop, d, p, (origin, z));
    false
}

/// Arrow keys: move the crop box of the active document by (`dx`, `dy`) canvas pixels.
/// `false` if it has none.
pub fn nudge(app: &mut FireCraftApp, dx: i64, dy: i64) -> bool {
    let Some(id) = app.session.active_doc().map(|d| d.id) else { return false };
    let Some(edit) = app.crop.as_mut().filter(|c| c.doc == id && c.drag.is_none()) else {
        return false;
    };
    let Some(r) = edit.rect else { return false };
    edit.rect = Some((r.0 + dx, r.1 + dy, r.2 + dx, r.3 + dy));
    true
}

/// Draw the crop box of document `d`, if it has one (also while the Hand or Zoom tool is in
/// use, which keep it).
pub fn overlay(crop: &Option<CropEdit>, d: &DocState, p: &egui::Painter, (origin, z): (Pos2, f32)) {
    let Some(r) = crop.as_ref().filter(|c| c.doc == d.id).and_then(|c| c.rect) else {
        return;
    };
    let to_screen = |x: i64, y: i64| pos2(origin.x + x as f32 * z, origin.y + y as f32 * z);
    let ctx = p.ctx();
    let phase = (ctx.input(|i| i.time) * 8.0) as i64;
    ants(p, Rect::from_min_max(to_screen(r.0, r.1), to_screen(r.2, r.3)), phase);
    for h in HANDLES {
        let (x, y) = handle_pos(r, h);
        let c = pos2(origin.x + x as f32 * z, origin.y + y as f32 * z);
        // 3 screen pixels before the box's edge and 2 after, as Fireworks MX draws them at
        // any zoom (measured 2026-11-26).
        let hr = Rect::from_min_size(c.floor() - vec2(3.0, 3.0), vec2(HANDLE, HANDLE));
        p.rect_filled(hr, 0.0, Color32::BLACK);
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(120));
}

/// A rectangle drawn as marching ants (4 px black, 4 px white, moving with `phase`), on its
/// outermost screen pixels: the right and bottom sides on the last pixel inside, as Fireworks
/// MX draws them (a box the size of the canvas runs along the canvas's own edge pixels).
fn ants(p: &egui::Painter, r: Rect, phase: i64) {
    let o = vec2(0.5, 0.5);
    let a = r.min.floor() + o;
    let b = (r.max.ceil() - vec2(1.0, 1.0) + o).max(a);
    let corners = [a, pos2(b.x, a.y), b, pos2(a.x, b.y), a];
    let mut along = 0.0_f32;
    for (&s, &e) in corners.iter().zip(corners.iter().skip(1)) {
        let len = (e - s).length();
        let dir = if len > 0.0 { (e - s) / len } else { vec2(0.0, 0.0) };
        let mut t = 0.0;
        while t < len {
            let k = ((along + t) as i64 + phase).rem_euclid(8);
            let step = (4 - k % 4) as f32;
            let end = (t + step).min(len);
            let color = if k < 4 { Color32::BLACK } else { Color32::WHITE };
            p.line_segment([s + dir * t, s + dir * end], Stroke::new(1.0, color));
            t = end;
        }
        along += len;
    }
}

/// The Property inspector's crop box fields (Fireworks MX shows W, H, X and Y under the
/// Document inspector while a box is up).
pub fn inspector(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let Some(d) = app.session.active_doc() else { return };
    let id = d.id;
    ui.separator();
    ui.vertical(|ui| {
        let edit = app.crop.as_mut().filter(|c| c.doc == id);
        let Some(edit) = edit.filter(|c| c.rect.is_some()) else {
            ui.weak("Drag on the canvas to draw a crop box.");
            return;
        };
        let Some((l, t, r, b)) = edit.rect else { return };
        let (mut x, mut y, mut w, mut h) = (l, t, r - l, b - t);
        let mut changed = false;
        egui::Grid::new("crop_pi").spacing(vec2(6.0, 2.0)).show(ui, |ui| {
            ui.label("W:");
            changed |= ui.add(egui::DragValue::new(&mut w).range(1..=32_000)).changed();
            ui.label("X:");
            changed |= ui.add(egui::DragValue::new(&mut x).range(-32_000..=32_000)).changed();
            ui.end_row();
            ui.label("H:");
            changed |= ui.add(egui::DragValue::new(&mut h).range(1..=32_000)).changed();
            ui.label("Y:");
            changed |= ui.add(egui::DragValue::new(&mut y).range(-32_000..=32_000)).changed();
            ui.end_row();
        });
        if changed {
            edit.rect = Some((x, y, x + w, y + h));
        }
        let what = match edit.target {
            CropTarget::Document => "Crop the document",
            CropTarget::Bitmap(_) => "Crop the bitmap",
        };
        ui.weak(format!("{what}: Enter or double-click."))
            .on_hover_text("Escape removes the box. Dragging past the canvas makes it bigger.");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// New boxes, as drawn in Fireworks MX on a 300 x 200 canvas (2026-11-26).
    #[test]
    fn new_box_stays_on_the_canvas() {
        let draw = |(sx, sy): (i64, i64), (ex, ey): (f64, f64)| {
            let (l, r) = new_side(sx, ex, 300);
            let (t, b) = new_side(sy, ey, 200);
            normalized(l, t, r, b)
        };
        assert_eq!(draw((-20, -20), (320.0, 220.0)), (0, 0, 300, 200));
        assert_eq!(draw((-20, -20), (100.0, 80.0)), (0, 0, 100, 80));
        assert_eq!(draw((320, 220), (100.0, 100.0)), (100, 100, 300, 200));
        assert_eq!(draw((-20, -20), (-40.0, -40.0)), (-20, -20, 0, 0));
        assert_eq!(draw((-20, -20), (100.0, -40.0)), (0, -20, 100, 0));
        assert_eq!(draw((-20, 60), (-40.0, 120.0)), (-20, 60, 0, 120));
        assert_eq!(draw((320, 60), (370.0, 120.0)), (300, 60, 320, 120));
    }

    /// Sides snap up to 5 screen pixels past the canvas, never inside it (Fireworks MX).
    #[test]
    fn sides_snap_just_past_the_canvas() {
        assert_eq!(snap(305.0, 300, 1.0), Some(300));
        assert_eq!(snap(306.0, 300, 1.0), None);
        assert_eq!(snap(299.0, 300, 1.0), None);
        assert_eq!(snap(-5.0, 300, 1.0), Some(0));
        assert_eq!(snap(-6.0, 300, 1.0), None);
        assert_eq!(snap(3.0, 300, 1.0), None);
        // At 200% the distance is on screen: 2 canvas pixels snap, 3 don't.
        assert_eq!(snap(302.0, 300, 2.0), Some(300));
        assert_eq!(snap(303.0, 300, 2.0), None);
    }
}
