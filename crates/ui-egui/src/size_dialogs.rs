//! Modify > Canvas > Canvas Size and Image Size, laid out and behaving as Fireworks MX's
//! (measured 2026-11-16, docs/behavior/canvas-size.md).

use egui::{Color32, DragValue, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use firecraft_engine::{Anchor, DocState};
use firecraft_raster::transform::Resample;

use crate::FireCraftApp;

/// A length unit in the two dialogs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unit {
    Pixels,
    Percent,
    Inches,
    Centimeters,
}

impl Unit {
    fn label(self) -> &'static str {
        match self {
            Self::Pixels => "Pixels",
            Self::Percent => "Percent",
            Self::Inches => "Inches",
            Self::Centimeters => "Centimeters",
        }
    }
}

/// Up to six significant digits, trailing zeros dropped (Canvas Size: 300 px at 72 ppi is
/// "4.16667" inches).
fn sig6(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return "0".into();
    }
    let int_digits = v.abs().log10().floor() as i32 + 1;
    let decimals = (6 - int_digits).clamp(0, 10) as usize;
    trim(format!("{v:.decimals$}"))
}

/// Up to `n` decimals, trailing zeros dropped (Image Size's print size: "2.222", "4", "2.5").
fn decimals(v: f64, n: usize) -> String {
    trim(format!("{v:.n$}"))
}

fn trim(s: String) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_owned()
    } else {
        s
    }
}

/// A number box showing `text`; the typed number when it was changed.
fn number(ui: &mut Ui, id: &str, value: f64, text: impl Fn(f64) -> String + 'static, enabled: bool) -> Option<f64> {
    let mut v = value;
    let resp = ui.add_enabled(
        enabled,
        DragValue::new(&mut v)
            .speed(0.0)
            .update_while_editing(false)
            .custom_formatter(move |v, _| text(v))
            .min_decimals(0),
    );
    let _ = id;
    (resp.changed() && v.is_finite() && v > 0.0).then_some(v)
}

fn unit_combo(ui: &mut Ui, id: &str, unit: &mut Unit, choices: &[Unit], enabled: bool) -> bool {
    let before = *unit;
    ui.add_enabled_ui(enabled, |ui| {
        egui::ComboBox::from_id_salt(id)
            .width(130.0)
            .selected_text(unit.label())
            .show_ui(ui, |ui| {
                for &c in choices {
                    ui.selectable_value(unit, c, c.label());
                }
            });
    });
    *unit != before
}

/// A titled group box, as Fireworks' dialogs have.
fn group(ui: &mut Ui, title: &str, add: impl FnOnce(&mut Ui)) {
    ui.label(title);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_width(340.0);
        add(ui);
    });
}

/// The bracket with a lock beside constrained rows (Fireworks MX).
fn lock_bracket(ui: &mut Ui, height: f32, on: bool) {
    let (r, _) = ui.allocate_exact_size(vec2(22.0, height), Sense::hover());
    if !on {
        return;
    }
    let p = ui.painter();
    let c = ui.visuals().text_color();
    let s = Stroke::new(2.0, c);
    let (x, y0, y1) = (r.left() + 7.0, r.top() + 6.0, r.bottom() - 6.0);
    p.line_segment([pos2(r.left(), y0), pos2(x, y0)], s);
    p.line_segment([pos2(x, y0), pos2(x, y1)], s);
    p.line_segment([pos2(r.left(), y1), pos2(x, y1)], s);
    // A small padlock.
    let m = pos2(r.left() + 16.0, r.center().y);
    let body = Rect::from_center_size(m + vec2(0.0, 2.0), vec2(8.0, 6.0));
    p.rect_filled(body, 1.0, c);
    p.circle_stroke(m + vec2(0.0, -2.0), 2.5, Stroke::new(1.2, c));
}

/// Enter (OK) and Escape (Cancel) for these dialogs, read before their widgets: as in a Windows
/// dialog, Enter is OK even with a unit list focused (which would take it otherwise); a number
/// being typed still takes its value first, as the dialog is drawn before OK applies. Not while
/// a list is open (it takes them itself).
fn dialog_keys(ctx: &egui::Context) -> (bool, bool) {
    if ctx.any_popup_open() {
        return (false, false);
    }
    ctx.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)))
}

// ---------------------------------------------------------------------------------------------
// Canvas Size

/// The Canvas Size dialog's state.
#[derive(Clone, Debug, PartialEq)]
pub struct CanvasSize {
    doc: u64,
    /// The current canvas, pixels.
    current: (u32, u32),
    /// Pixels per inch (for the Inches and Centimeters units).
    ppi: f64,
    /// New size in pixels, as typed (inches give fractions; the canvas takes the whole part).
    size: [f64; 2],
    units: [Unit; 2],
    pub anchor: Anchor,
}

impl CanvasSize {
    pub fn new(d: &DocState) -> Self {
        let ppi = if d.doc.resolution_cm { d.doc.resolution * 2.54 } else { d.doc.resolution };
        Self {
            doc: d.id,
            current: (d.doc.width, d.doc.height),
            ppi: if ppi > 0.0 { ppi } else { 72.0 },
            size: [f64::from(d.doc.width), f64::from(d.doc.height)],
            units: [Unit::Pixels; 2],
            anchor: Anchor::Center,
        }
    }

    /// The canvas size OK makes: typed sizes are truncated to whole pixels (Fireworks MX: 4.22
    /// inches at 72 ppi = 303.84 -> 303).
    pub fn pixels(&self) -> (u32, u32) {
        let px = |v: f64| (v.trunc() as i64).clamp(1, 32_000) as u32;
        (px(self.size[0]), px(self.size[1]))
    }

    fn shown(&self, i: usize) -> f64 {
        let v = self.size[i];
        match self.units[i] {
            Unit::Inches => v / self.ppi,
            Unit::Centimeters => v / self.ppi * 2.54,
            _ => v.trunc(),
        }
    }

    fn set(&mut self, i: usize, v: f64) {
        self.size[i] = match self.units[i] {
            Unit::Inches => v * self.ppi,
            Unit::Centimeters => v * self.ppi / 2.54,
            _ => v,
        };
    }
}

/// The 3x3 anchor buttons: the chosen one blue with a dot, the others with arrows pointing
/// away from it (where the canvas grows).
fn anchor_grid(ui: &mut Ui, anchor: &mut Anchor) {
    const CELL: f32 = 22.0;
    let (r, _) = ui.allocate_exact_size(vec2(CELL * 3.0 + 4.0, CELL * 3.0 + 4.0), Sense::hover());
    let (ax, ay) = anchor.sides();
    for a in Anchor::ALL {
        let (cx, cy) = a.sides();
        let cell = Rect::from_min_size(
            r.min + vec2((cx + 1) as f32 * (CELL + 2.0), (cy + 1) as f32 * (CELL + 2.0)),
            vec2(CELL, CELL),
        );
        let id = ui.scope_id().with(("anchor", cx, cy));
        let resp = ui.interact(cell, id, Sense::click());
        let name = format!("Anchor {a:?}");
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
        if resp.clicked() {
            *anchor = a;
        }
        let p = ui.painter();
        let on = a == *anchor;
        let fill = if on {
            Color32::from_rgb(0x33, 0x88, 0xFF)
        } else if resp.hovered() {
            ui.visuals().widgets.hovered.bg_fill
        } else {
            ui.visuals().extreme_bg_color
        };
        p.rect_filled(cell, 3.0, fill);
        p.rect_stroke(cell, 3.0, Stroke::new(1.0, Color32::from_gray(150)), egui::StrokeKind::Inside);
        let c = cell.center();
        let ink = if crate::theme::pal().dark && !on { crate::theme::pal().glyph } else { Color32::from_gray(20) };
        if on {
            p.circle_filled(c, 2.5, ink);
        } else {
            let (dx, dy) = ((cx - ax).signum() as f32, (cy - ay).signum() as f32);
            arrow(p, c, vec2(dx, dy), ink);
        }
    }
}

/// Fireworks' width (↔) or height (↕) mark, painted (the UI font lacks the arrows).
fn axis(ui: &mut Ui, horizontal: bool) {
    let (r, resp) = ui.allocate_exact_size(vec2(16.0, 14.0), Sense::hover());
    resp.on_hover_text(if horizontal { "Width" } else { "Height" });
    let c = r.center();
    let d = if horizontal { vec2(1.0, 0.0) } else { vec2(0.0, 1.0) };
    let ink = ui.visuals().text_color();
    arrow(ui.painter(), c, d, ink);
    arrow(ui.painter(), c, -d, ink);
}

fn arrow(p: &egui::Painter, c: Pos2, dir: egui::Vec2, color: Color32) {
    let d = dir.normalized();
    let (a, b) = (c - d * 5.0, c + d * 5.0);
    let s = Stroke::new(1.3, color);
    p.line_segment([a, b], s);
    let side = vec2(-d.y, d.x) * 3.0;
    p.line_segment([b, b - d * 3.0 + side], s);
    p.line_segment([b, b - d * 3.0 - side], s);
}

pub fn canvas_size(app: &mut FireCraftApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.canvas_size.take() else { return };
    let mut ok = false;
    let mut cancel = false;
    let (enter, escape) = dialog_keys(ctx);
    super::dialogs::dialog("Canvas Size").show(ctx, |ui| {
        group(ui, "New Size:", |ui| {
            egui::Grid::new("canvas_size_new").spacing(vec2(8.0, 6.0)).show(ui, |ui| {
                for (i, wh) in ["W", "H"].into_iter().enumerate() {
                    axis(ui, i == 0);
                    let unit = st.units[i];
                    let text = move |v: f64| if matches!(unit, Unit::Pixels) { format!("{v}") } else { sig6(v) };
                    if let Some(v) = number(ui, wh, st.shown(i), text, true) {
                        st.set(i, v);
                    }
                    unit_combo(
                        ui,
                        &format!("canvas_size_unit_{i}"),
                        &mut st.units[i],
                        &[Unit::Pixels, Unit::Inches, Unit::Centimeters],
                        true,
                    );
                    let (w, h) = st.pixels();
                    ui.label(format!("{wh}: {}", if i == 0 { w } else { h }));
                    ui.end_row();
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label("Anchor:");
                anchor_grid(ui, &mut st.anchor);
            });
        });
        ui.add_space(6.0);
        group(ui, "Current Size:", |ui| {
            for (i, v) in [st.current.0, st.current.1].into_iter().enumerate() {
                ui.horizontal(|ui| {
                    axis(ui, i == 0);
                    ui.label(format!("{v}  pixels"));
                });
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ok = ui.button("OK").clicked();
            cancel = ui.button("Cancel").clicked();
        });
    });
    ok |= enter;
    cancel |= escape;
    if ok {
        let (w, h) = st.pixels();
        if let Some(d) = app.session.docs.iter_mut().find(|d| d.id == st.doc) {
            d.canvas_size(w, h, st.anchor);
        }
    } else if !cancel {
        app.dialogs.canvas_size = Some(st);
    }
}

// ---------------------------------------------------------------------------------------------
// Image Size

/// The Image Size dialog's state. Sizes are kept unrounded, as Fireworks MX keeps them: with
/// Constrain Proportions, 161 px wide shows 101 px high but 1.398 inches (100.625 / 72).
#[derive(Clone, Debug, PartialEq)]
pub struct ImageSize {
    doc: u64,
    /// The document's size and pixels per inch when the dialog opened.
    orig: [f64; 2],
    orig_ppi: f64,
    /// New size in pixels.
    size: [f64; 2],
    /// Resolution in its unit.
    res: f64,
    res_cm: bool,
    px_units: [Unit; 2],
    print_units: [Unit; 2],
}

impl ImageSize {
    pub fn new(d: &DocState) -> Self {
        let mut s = Self {
            doc: d.id,
            orig: [f64::from(d.doc.width), f64::from(d.doc.height)],
            orig_ppi: 72.0,
            size: [f64::from(d.doc.width), f64::from(d.doc.height)],
            res: if d.doc.resolution > 0.0 { d.doc.resolution } else { 72.0 },
            res_cm: d.doc.resolution_cm,
            px_units: [Unit::Pixels; 2],
            // Centimeters for a pixels/cm document (Fireworks MX).
            print_units: [if d.doc.resolution_cm { Unit::Centimeters } else { Unit::Inches }; 2],
        };
        s.orig_ppi = s.ppi();
        s
    }

    fn ppi(&self) -> f64 {
        if self.res_cm { self.res * 2.54 } else { self.res }
    }

    /// The pixel size OK makes: rounded, halves up (Fireworks MX: 102.5 -> 103).
    pub fn pixels(&self) -> (u32, u32) {
        let px = |v: f64| ((v + 0.5).floor() as i64).clamp(1, 32_000) as u32;
        (px(self.size[0]), px(self.size[1]))
    }

    /// Set axis `i` to `v` pixels; with `constrain` the other follows.
    fn set_px(&mut self, i: usize, v: f64, constrain: bool) {
        self.size[i] = v;
        if constrain {
            let j = 1 - i;
            self.size[j] = v * self.orig[j] / self.orig[i];
        }
    }

    fn px_shown(&self, i: usize) -> f64 {
        match self.px_units[i] {
            Unit::Percent => self.size[i] / self.orig[i] * 100.0,
            _ => (self.size[i] + 0.5).floor(),
        }
    }

    fn print_shown(&self, i: usize) -> f64 {
        let inches = self.size[i] / self.ppi();
        match self.print_units[i] {
            Unit::Percent => inches / (self.orig[i] / self.orig_ppi) * 100.0,
            Unit::Centimeters => inches * 2.54,
            _ => inches,
        }
    }

    /// Typed print size `v` (in the row's unit) for axis `i`.
    fn set_print(&mut self, i: usize, v: f64, constrain: bool, resample: bool) {
        let inches = match self.print_units[i] {
            Unit::Percent => self.orig[i] / self.orig_ppi * v / 100.0,
            Unit::Centimeters => v / 2.54,
            _ => v,
        };
        if resample {
            // The resolution stays; the pixels change.
            self.set_px(i, inches * self.ppi(), constrain);
        } else {
            // The pixels stay; the resolution changes.
            let ppi = self.size[i] / inches;
            self.res = if self.res_cm { ppi / 2.54 } else { ppi };
        }
    }

    fn set_res(&mut self, v: f64, resample: bool) {
        if resample {
            // The print size stays; the pixels change.
            let k = v / self.res;
            self.size = [self.size[0] * k, self.size[1] * k];
        }
        self.res = v;
    }
}

pub fn image_size(app: &mut FireCraftApp, ctx: &egui::Context) {
    let Some(mut st) = app.dialogs.image_size.take() else { return };
    let mut ok = false;
    let mut cancel = false;
    let (enter, escape) = dialog_keys(ctx);
    let prefs = &mut app.prefs;
    super::dialogs::dialog("Image Size").show(ctx, |ui| {
        let resample = prefs.image_size_resample;
        // Without resampling the pixels can't change and the proportions are kept.
        let constrain = prefs.image_size_constrain || !resample;
        group(ui, "Pixel Dimensions:", |ui| {
            ui.horizontal(|ui| {
                let grid = egui::Grid::new("image_size_px").spacing(vec2(8.0, 6.0)).show(ui, |ui| {
                    for i in 0..2 {
                        axis(ui, i == 0);
                        let unit = st.px_units[i];
                        let text = move |v: f64| if unit == Unit::Percent { decimals(v, 2) } else { format!("{v}") };
                        if let Some(v) = number(ui, "px", st.px_shown(i), text, resample) {
                            let px = if unit == Unit::Percent { st.orig[i] * v / 100.0 } else { v };
                            st.set_px(i, px, constrain);
                        }
                        unit_combo(ui, &format!("image_size_px_{i}"), &mut st.px_units[i], &[Unit::Pixels, Unit::Percent], resample);
                        ui.end_row();
                    }
                });
                lock_bracket(ui, grid.response.rect.height(), constrain && resample);
            });
        });
        ui.add_space(6.0);
        group(ui, "Print Size:", |ui| {
            ui.horizontal(|ui| {
                let grid = egui::Grid::new("image_size_print").spacing(vec2(8.0, 6.0)).show(ui, |ui| {
                    for i in 0..2 {
                        axis(ui, i == 0);
                        if let Some(v) = number(ui, "print", st.print_shown(i), |v| decimals(v, 3), true) {
                            st.set_print(i, v, constrain, resample);
                        }
                        unit_combo(
                            ui,
                            &format!("image_size_print_{i}"),
                            &mut st.print_units[i],
                            &[Unit::Percent, Unit::Inches, Unit::Centimeters],
                            true,
                        );
                        ui.end_row();
                    }
                    ui.label("Resolution:");
                    // Up to three decimals (Fireworks MX's Image Size: 28.346, 42.52, 108).
                    if let Some(v) = number(ui, "res", st.res, |v| decimals(v, 3), true) {
                        st.set_res(v.clamp(1.0, 100_000.0), resample);
                    }
                    let was_cm = st.res_cm;
                    egui::ComboBox::from_id_salt("image_size_res_unit")
                        .width(130.0)
                        .selected_text(if st.res_cm { "Pixels/cm" } else { "Pixels/Inch" })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut st.res_cm, false, "Pixels/Inch");
                            ui.selectable_value(&mut st.res_cm, true, "Pixels/cm");
                        });
                    if st.res_cm != was_cm {
                        // Converted exactly (Fireworks MX's Image Size: 72 ppi -> 28.346 px/cm,
                        // without New Document's rounding); pixels and print size stay.
                        st.res = if st.res_cm { st.res / 2.54 } else { st.res * 2.54 };
                    }
                    ui.end_row();
                });
                // Without resampling the bracket takes in the resolution too (Fireworks MX).
                let h = grid.response.rect.height();
                lock_bracket(ui, if resample { h * 0.62 } else { h }, constrain);
            });
        });
        ui.add_space(6.0);
        ui.add_enabled_ui(resample, |ui| {
            let mut c = constrain;
            if ui.checkbox(&mut c, "Constrain Proportions").changed() {
                prefs.image_size_constrain = c;
            }
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut prefs.image_size_resample, "Resample Image");
            ui.add_space(40.0);
            egui::ComboBox::from_id_salt("image_size_method")
                .width(180.0)
                .selected_text(prefs.image_size_method.label())
                .show_ui(ui, |ui| {
                    for m in Resample::ALL {
                        ui.selectable_value(&mut prefs.image_size_method, m, m.label());
                    }
                });
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ok = ui.button("OK").clicked();
            cancel = ui.button("Cancel").clicked();
        });
    });
    ok |= enter;
    cancel |= escape;
    if ok {
        let (w, h) = st.pixels();
        let method = app.prefs.image_size_method;
        if let Some(d) = app.session.docs.iter_mut().find(|d| d.id == st.doc) {
            d.image_size(w, h, st.res, st.res_cm, method);
        }
    } else if !cancel {
        app.dialogs.image_size = Some(st);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use firecraft_doc::Document;

    fn doc(w: u32, h: u32) -> DocState {
        DocState::new(Document::new(w, h, 72.0, None))
    }

    #[test]
    fn formats_as_fireworks_mx() {
        assert_eq!(sig6(300.0 / 72.0), "4.16667");
        assert_eq!(sig6(200.0 / 72.0 * 2.54), "7.05556");
        assert_eq!(sig6(300.0), "300");
        assert_eq!(decimals(160.0 / 72.0, 3), "2.222");
        assert_eq!(decimals(4.0, 3), "4");
        assert_eq!(decimals(2.5, 3), "2.5");
        assert_eq!(decimals(50.0 / 72.0, 3), "0.694");
    }

    #[test]
    fn canvas_size_truncates_typed_lengths() {
        let mut s = CanvasSize::new(&doc(300, 200));
        s.units = [Unit::Inches, Unit::Centimeters];
        s.set(0, 4.22);
        s.set(1, 7.08);
        assert_eq!(s.pixels(), (303, 200));
        s.set(0, 4.21);
        assert_eq!(s.pixels(), (303, 200));
    }

    #[test]
    fn image_size_fields_follow_each_other_as_fireworks_mx() {
        // 160x100 at 72 ppi.
        let mut s = ImageSize::new(&doc(160, 100));
        s.set_px(0, 161.0, true);
        assert_eq!(s.pixels(), (161, 101));
        assert_eq!(decimals(s.print_shown(1), 3), "1.398");
        s.set_px(0, 164.0, true);
        assert_eq!(s.pixels(), (164, 103)); // 102.5 rounds up
        s.set_px(1, 107.0, true);
        assert_eq!(s.pixels(), (171, 107));
        assert_eq!(decimals(s.print_shown(0), 3), "2.378");
        // Percent: 150% of 160.
        let mut s = ImageSize::new(&doc(160, 100));
        s.set_px(0, 160.0 * 1.5, true);
        assert_eq!(s.pixels(), (240, 150));
        // Resample off: 144 ppi halves the print size; 4 inches wide makes 40 ppi.
        let mut s = ImageSize::new(&doc(160, 100));
        s.set_res(144.0, false);
        assert_eq!(decimals(s.print_shown(0), 3), "1.111");
        assert_eq!(decimals(s.print_shown(1), 3), "0.694");
        s.set_print(0, 4.0, true, false);
        assert_eq!((s.res, s.pixels()), (40.0, (160, 100)));
        assert_eq!(decimals(s.print_shown(1), 3), "2.5");
        // Resample on: 108 ppi keeps the print size, 160x100 -> 240x150.
        let mut s = ImageSize::new(&doc(160, 100));
        s.set_res(108.0, true);
        assert_eq!(s.pixels(), (240, 150));
        assert_eq!(decimals(s.print_shown(0), 3), "2.222");
    }
}
