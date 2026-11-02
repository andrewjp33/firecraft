//! 9-slice Scaling tool UI (Fireworks CS5-style, destructive).
//!
//! Select objects, drag the four dashed guides to mark the corners that must keep their size,
//! then drag the bottom-right handle (or type W/H in the Property inspector and Apply). The
//! result is ordinary paths/bitmaps, so the file stays Fireworks MX compatible.

use egui::{Color32, Pos2, Rect, Shape, Stroke, StrokeKind, pos2, vec2};
use firecraft_engine::DocState;
use firecraft_raster::nine::Insets;

use crate::{FireCraftApp, View, theme};

const GUIDE: Color32 = Color32::from_rgb(0, 120, 220);

/// Guides and target size being edited for the current selection.
#[derive(Clone, Debug, PartialEq)]
pub struct NineEdit {
    /// Which document/selection this belongs to (reset when the selection changes).
    key: (u64, [i64; 4]),
    pub insets: Insets,
    pub width: f64,
    pub height: f64,
    drag: Option<Drag>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Left,
    Right,
    Top,
    Bottom,
    Resize,
}

fn key(d: &DocState, b: (f64, f64, f64, f64)) -> (u64, [i64; 4]) {
    (
        d.id,
        [
            b.0.round() as i64,
            b.1.round() as i64,
            b.2.round() as i64,
            b.3.round() as i64,
        ],
    )
}

/// Current edit state for the selection, created with default (one-third) guides.
fn sync<'a>(
    nine: &'a mut Option<NineEdit>,
    d: &DocState,
) -> Option<(&'a mut NineEdit, (f64, f64, f64, f64))> {
    let b = d.selection_bounds()?;
    let k = key(d, b);
    if nine.as_ref().is_none_or(|n| n.key != k) {
        *nine = Some(NineEdit {
            key: k,
            insets: Insets::thirds(b.2, b.3),
            width: b.2.round(),
            height: b.3.round(),
            drag: None,
        });
    }
    nine.as_mut().map(|n| (n, b))
}

pub fn canvas(
    nine: &mut Option<NineEdit>,
    d: &mut DocState,
    _view: &mut View,
    p: &egui::Painter,
    resp: &egui::Response,
    origin: Pos2,
    z: f32,
) {
    let to_screen = |x: f64, y: f64| pos2(origin.x + x as f32 * z, origin.y + y as f32 * z);
    let to_doc = |q: Pos2| (((q.x - origin.x) / z) as f64, ((q.y - origin.y) / z) as f64);

    // Clicking selects objects, as with the Pointer.
    if resp.clicked()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let (x, y) = to_doc(pos);
        match d.hit_test(x, y) {
            Some(hit) => d.selection = vec![hit],
            None => d.deselect(),
        }
        d.pixels = None;
    }
    let Some((edit, b)) = sync(nine, d) else { return };
    let (x, y, w, h) = b;
    let ins = edit.insets;
    let gx1 = to_screen(x + ins.left, y).x;
    let gx2 = to_screen(x + w - ins.right, y).x;
    let gy1 = to_screen(x, y + ins.top).y;
    let gy2 = to_screen(x, y + h - ins.bottom).y;
    let target = Rect::from_min_size(
        to_screen(x, y),
        vec2(edit.width as f32 * z, edit.height as f32 * z),
    );
    let handle = Rect::from_center_size(target.right_bottom(), vec2(8.0, 8.0));

    if resp.drag_started() {
        // Where the button went down (the current position has already moved past the threshold).
        if let Some(q) = resp
            .ctx
            .input(|i| i.pointer.press_origin())
            .or(resp.interact_pointer_pos())
        {
            let near = |a: f32, b: f32| (a - b).abs() <= 4.0;
            edit.drag = if handle.expand(3.0).contains(q) {
                Some(Drag::Resize)
            } else if near(q.x, gx1) {
                Some(Drag::Left)
            } else if near(q.x, gx2) {
                Some(Drag::Right)
            } else if near(q.y, gy1) {
                Some(Drag::Top)
            } else if near(q.y, gy2) {
                Some(Drag::Bottom)
            } else {
                None
            };
        }
    }
    if let (Some(drag), Some(q)) = (edit.drag, resp.interact_pointer_pos()) {
        let (qx, qy) = to_doc(q);
        match drag {
            Drag::Left => edit.insets.left = (qx - x).round().clamp(0.0, w),
            Drag::Right => edit.insets.right = (x + w - qx).round().clamp(0.0, w),
            Drag::Top => edit.insets.top = (qy - y).round().clamp(0.0, h),
            Drag::Bottom => edit.insets.bottom = (y + h - qy).round().clamp(0.0, h),
            Drag::Resize => {
                edit.width = (qx - x).round().max(1.0);
                edit.height = (qy - y).round().max(1.0);
            }
        }
    }
    if resp.drag_stopped() {
        if edit.drag == Some(Drag::Resize) {
            let (to, insets) = ((x, y, edit.width, edit.height), edit.insets);
            edit.drag = None;
            d.nine_slice_scale(to, insets);
            *nine = None; // new bounds: guides re-sync next frame, keeping the insets below
            if let Some((e, _)) = sync(nine, d) {
                e.insets = insets;
            }
            return;
        }
        edit.drag = None;
    }

    // Overlay: object bounds, target outline, guides, handle.
    let bounds = Rect::from_min_max(to_screen(x, y), to_screen(x + w, y + h));
    p.rect_stroke(
        bounds,
        0.0,
        Stroke::new(1.0, theme::HANDLE_BLUE),
        StrokeKind::Outside,
    );
    if target != bounds {
        p.rect_stroke(
            target,
            0.0,
            Stroke::new(1.0, Color32::from_rgb(200, 0, 0)),
            StrokeKind::Outside,
        );
    }
    let dashed = |a: Pos2, b: Pos2| p.extend(Shape::dashed_line(&[a, b], Stroke::new(1.0, GUIDE), 4.0, 3.0));
    let ext = bounds.expand(6.0);
    dashed(pos2(gx1, ext.top()), pos2(gx1, ext.bottom()));
    dashed(pos2(gx2, ext.top()), pos2(gx2, ext.bottom()));
    dashed(pos2(ext.left(), gy1), pos2(ext.right(), gy1));
    dashed(pos2(ext.left(), gy2), pos2(ext.right(), gy2));
    p.rect_filled(handle, 0.0, theme::HANDLE_BLUE);
    p.rect_stroke(handle, 0.0, Stroke::new(1.0, Color32::WHITE), StrokeKind::Inside);
}

pub fn inspector(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    ui.vertical(|ui| {
        ui.set_width(150.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), egui::Sense::hover());
            crate::icons::paint(
                ui.painter(),
                Rect::from_center_size(r.center(), vec2(20.0, 20.0)),
                firecraft_engine::Tool::NineSlice,
                true,
            );
            ui.label(egui::RichText::new("9-slice Scaling tool").size(11.0));
        });
    });
    ui.separator();
    let Some(d) = app.session.active_doc_mut() else {
        ui.weak("Open a document and select objects to scale.");
        return;
    };
    let Some((edit, b)) = sync(&mut app.nine, d) else {
        ui.weak("Select one or more objects, then drag the guides and the corner handle.");
        return;
    };
    let mut apply = false;
    ui.vertical(|ui| {
        egui::Grid::new("nine_pi").spacing(vec2(6.0, 2.0)).show(ui, |ui| {
            ui.label("W:");
            ui.add(egui::DragValue::new(&mut edit.width).range(1.0..=32000.0));
            ui.label("Guides  L:");
            ui.add(egui::DragValue::new(&mut edit.insets.left).range(0.0..=b.2));
            ui.label("R:");
            ui.add(egui::DragValue::new(&mut edit.insets.right).range(0.0..=b.2));
            ui.end_row();
            ui.label("H:");
            ui.add(egui::DragValue::new(&mut edit.height).range(1.0..=32000.0));
            ui.label("T:");
            ui.add(egui::DragValue::new(&mut edit.insets.top).range(0.0..=b.3));
            ui.label("B:");
            ui.add(egui::DragValue::new(&mut edit.insets.bottom).range(0.0..=b.3));
            ui.end_row();
        });
        ui.horizontal(|ui| {
            apply = ui.button("Apply").clicked();
            if ui.button("Reset Guides").clicked() {
                edit.insets = Insets::thirds(b.2, b.3);
            }
            ui.weak(format!(
                "from {} x {}  |  result stays Fireworks MX compatible",
                b.2.round(),
                b.3.round()
            ));
        });
    });
    if apply {
        let (to, insets) = ((b.0, b.1, edit.width, edit.height), edit.insets);
        d.nine_slice_scale(to, insets);
        app.nine = None;
        if let Some(d) = app.session.active_doc()
            && let Some((e, _)) = sync(&mut app.nine, d)
        {
            e.insets = insets;
        }
    }
}
