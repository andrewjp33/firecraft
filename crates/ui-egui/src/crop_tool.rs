//! The Crop tool and Edit > Crop Selected Bitmap: a crop box on the canvas, as in Fireworks MX
//! (measured 2026-11-16, docs/behavior/crop.md).
//!
//! * Dragging outside the box draws a new one. Where the drag began isn't limited, but the
//!   corner being dragged stays on the canvas.
//! * The eight handles and the four sides resize it, also past the canvas (the canvas then
//!   grows), and turn it over when dragged past the opposite side. Dragging inside the box
//!   moves it.
//! * Double-clicking inside or pressing Enter crops; the Crop tool stays chosen. Escape drops
//!   the box and chooses the Pointer tool; choosing another tool drops it too, except the Hand
//!   and Zoom tools, which keep it.
//! * The box is marching ants with small black handles; the outside isn't dimmed.
//! * Choosing the Crop tool with pixels selected makes a box of the selection's size. The arrow
//!   keys move the box (Shift: 10 px).
//! * Crop Selected Bitmap puts the box around the bitmap; cropping trims the bitmap (undo step
//!   "Crop Image") and goes back to the Pointer tool.

use egui::{Color32, CursorIcon, Pos2, Rect, Stroke, pos2, vec2};
use firecraft_engine::{CropRect, DocState, ElementRef, Tool};

use crate::FireCraftApp;

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
        edit.rect = Some(match drag {
            Drag::New((sx, sy)) => {
                // The dragged corner stays on the canvas (Fireworks MX); the start doesn't.
                let ex = (x.round() as i64).clamp(0, cw);
                let ey = (y.round() as i64).clamp(0, ch);
                normalized(sx, sy, ex, ey)
            }
            Drag::Move((fx, fy), r) => {
                let (dx, dy) = ((x - fx).round() as i64, (y - fy).round() as i64);
                (r.0 + dx, r.1 + dy, r.2 + dx, r.3 + dy)
            }
            Drag::Handle((hx, hy), r) => {
                let (mut l, mut t, mut rr, mut b) = r;
                let (x, y) = (x.round() as i64, y.round() as i64);
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

    // Cursor: resize arrows over the handles, a move cursor inside the box.
    if let (Some(r), Some(q)) = (edit.rect, resp.hover_pos())
        && edit.drag.is_none()
    {
        if let Some((hx, hy)) = handle_at(r, q) {
            ctx.set_cursor_icon(match (hx, hy) {
                (0, _) => CursorIcon::ResizeVertical,
                (_, 0) => CursorIcon::ResizeHorizontal,
                (a, b) if a == b => CursorIcon::ResizeNwSe,
                _ => CursorIcon::ResizeNeSw,
            });
        } else if inside(r, q) {
            ctx.set_cursor_icon(CursorIcon::Move);
        }
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
        let hr = Rect::from_center_size(c.floor() + vec2(0.5, 0.5), vec2(HANDLE, HANDLE));
        p.rect_filled(hr, 0.0, Color32::BLACK);
    }
    ctx.request_repaint_after(std::time::Duration::from_millis(120));
}

/// A rectangle drawn as marching ants (4 px black, 4 px white, moving with `phase`).
fn ants(p: &egui::Painter, r: Rect, phase: i64) {
    let o = vec2(0.5, 0.5);
    let (a, b) = (r.min.floor() + o, r.max.floor() + o);
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
