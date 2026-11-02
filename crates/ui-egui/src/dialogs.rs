//! Modal-style dialogs: New Document, Canvas Size, colour pickers, Preferences, About.

use egui::{Color32, DragValue};
use firecraft_engine::DocState;
use firecraft_raster::Rgba;

use crate::FireCraftApp;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CanvasChoice {
    White,
    Transparent,
    Custom,
}

#[derive(Clone, Debug)]
pub struct Dialogs {
    pub new_doc: bool,
    pub new_w: u32,
    pub new_h: u32,
    pub new_res: f64,
    pub new_canvas: CanvasChoice,
    pub new_custom: [u8; 3],
    pub canvas_size: Option<(u32, u32)>,
    pub canvas_color: bool,
    /// Editing the stroke (`true`) or fill (`false`) tool colour.
    pub color_target: Option<bool>,
    pub prefs: bool,
    pub about: bool,
    pub feather: Option<u32>,
}

impl Default for Dialogs {
    fn default() -> Self {
        Self {
            new_doc: false,
            new_w: 660,
            new_h: 440,
            new_res: 72.0,
            new_canvas: CanvasChoice::White,
            new_custom: [255, 255, 255],
            canvas_size: None,
            canvas_color: false,
            color_target: None,
            prefs: false,
            about: false,
            feather: None,
        }
    }
}

impl Dialogs {
    /// File > New's fields from remembered settings.
    pub fn set_new_doc(&mut self, (w, h, res, canvas): (u32, u32, f64, Option<Rgba>)) {
        self.new_w = w;
        self.new_h = h;
        self.new_res = res;
        self.new_canvas = match canvas {
            None => CanvasChoice::Transparent,
            Some(c) if c == Rgba::WHITE => CanvasChoice::White,
            Some(c) => {
                self.new_custom = [c.r, c.g, c.b];
                CanvasChoice::Custom
            }
        };
    }

    pub fn open_canvas_size(&mut self, d: Option<&DocState>) {
        if let Some(d) = d {
            self.canvas_size = Some((d.doc.width, d.doc.height));
        }
    }
}

fn dialog(title: &str) -> egui::Window<'_> {
    egui::Window::new(title)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
}

fn rgba_edit(ui: &mut egui::Ui, c: &mut Rgba) -> bool {
    let mut arr = [c.r, c.g, c.b, c.a];
    let changed = ui.color_edit_button_srgba_unmultiplied(&mut arr).changed();
    *c = Rgba::new(arr[0], arr[1], arr[2], arr[3]);
    ui.label(c.to_hex());
    changed
}

pub fn show(app: &mut FireCraftApp, ctx: &egui::Context) {
    let dl = &mut app.dialogs;
    if dl.new_doc {
        let mut create = false;
        let mut cancel = false;
        dialog("New Document").show(ctx, |ui| {
            ui.strong("Canvas Size");
            egui::Grid::new("newdoc").show(ui, |ui| {
                ui.label("Width:");
                ui.add(DragValue::new(&mut dl.new_w).range(1..=32000).suffix(" Pixels"));
                ui.end_row();
                ui.label("Height:");
                ui.add(DragValue::new(&mut dl.new_h).range(1..=32000).suffix(" Pixels"));
                ui.end_row();
                ui.label("Resolution:");
                ui.add(
                    DragValue::new(&mut dl.new_res)
                        .range(1.0..=5000.0)
                        .suffix(" Pixels/Inch"),
                );
                ui.end_row();
            });
            ui.separator();
            ui.strong("Canvas Color");
            ui.radio_value(&mut dl.new_canvas, CanvasChoice::White, "White");
            ui.radio_value(&mut dl.new_canvas, CanvasChoice::Transparent, "Transparent");
            ui.horizontal(|ui| {
                ui.radio_value(&mut dl.new_canvas, CanvasChoice::Custom, "Custom");
                ui.color_edit_button_srgb(&mut dl.new_custom);
            });
            ui.separator();
            ui.horizontal(|ui| {
                create = ui.button("OK").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        if create {
            let canvas = match dl.new_canvas {
                CanvasChoice::White => Some(Rgba::WHITE),
                CanvasChoice::Transparent => None,
                CanvasChoice::Custom => Some(Rgba::new(
                    dl.new_custom[0],
                    dl.new_custom[1],
                    dl.new_custom[2],
                    255,
                )),
            };
            let (w, h, r) = (dl.new_w, dl.new_h, dl.new_res);
            dl.new_doc = false;
            // Remembered for next time (and across sessions), as in Fireworks.
            app.prefs.new_doc = (w, h, r, canvas);
            app.session.new_document(w, h, r, canvas);
            if let Some(d) = app.session.active_doc_mut() {
                d.doc.color_math = app.prefs.color_math;
            }
            return;
        }
        if cancel {
            dl.new_doc = false;
        }
    }
    let dl = &mut app.dialogs;
    if let Some((mut w, mut h)) = dl.canvas_size {
        let mut ok = false;
        let mut cancel = false;
        dialog("Canvas Size").show(ctx, |ui| {
            egui::Grid::new("canvassize").show(ui, |ui| {
                ui.label("Width:");
                ui.add(DragValue::new(&mut w).range(1..=32000).suffix(" Pixels"));
                ui.end_row();
                ui.label("Height:");
                ui.add(DragValue::new(&mut h).range(1..=32000).suffix(" Pixels"));
                ui.end_row();
            });
            ui.weak("Anchor: top left (other anchors coming soon)");
            ui.horizontal(|ui| {
                ok = ui.button("OK").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        dl.canvas_size = Some((w, h));
        if ok {
            dl.canvas_size = None;
            if let Some(d) = app.session.active_doc_mut() {
                d.set_canvas_size(w, h);
            }
        } else if cancel {
            dl.canvas_size = None;
        }
    }
    let dl = &mut app.dialogs;
    if dl.canvas_color {
        let mut close = false;
        if let Some(d) = app.session.active_doc_mut() {
            let mut canvas = d.doc.canvas;
            dialog("Canvas Color").show(ctx, |ui| {
                let mut transparent = canvas.is_none();
                ui.checkbox(&mut transparent, "Transparent");
                if transparent {
                    canvas = None;
                } else {
                    let mut c = canvas.unwrap_or(Rgba::WHITE);
                    rgba_edit(ui, &mut c);
                    canvas = Some(Rgba { a: 255, ..c });
                }
                close = ui.button("OK").clicked();
            });
            if canvas != d.doc.canvas {
                d.set_canvas(canvas);
            }
        } else {
            close = true;
        }
        if close {
            app.dialogs.canvas_color = false;
        }
    }
    let dl = &mut app.dialogs;
    if let Some(stroke) = dl.color_target {
        let mut close = false;
        let o = &mut app.session.options;
        let slot = if stroke { &mut o.stroke } else { &mut o.fill };
        dialog(if stroke { "Stroke Color" } else { "Fill Color" }).show(ctx, |ui| {
            let mut none = slot.is_none();
            ui.checkbox(&mut none, "None");
            if none {
                *slot = None;
            } else {
                let mut c = slot.unwrap_or(Rgba::BLACK);
                rgba_edit(ui, &mut c);
                *slot = Some(c);
            }
            close = ui.button("OK").clicked();
        });
        if close {
            app.dialogs.color_target = None;
        }
    }
    let dl = &mut app.dialogs;
    if let Some(mut r) = dl.feather {
        let mut ok = false;
        let mut cancel = false;
        dialog("Feather Selection").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Radius:");
                ui.add(DragValue::new(&mut r).range(0..=250).suffix(" Pixels"));
            });
            ui.horizontal(|ui| {
                ok = ui.button("OK").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
        dl.feather = Some(r);
        if ok {
            dl.feather = None;
            if let Some(d) = app.session.active_doc_mut() {
                d.feather_pixels(r);
            }
        } else if cancel {
            dl.feather = None;
        }
    }
    if app.dialogs.prefs {
        preferences(app, ctx);
    }
    if app.dialogs.about {
        let mut close = false;
        dialog("About FireCraft").show(ctx, |ui| {
            ui.heading(egui::RichText::new("FireCraft").color(Color32::from_rgb(220, 90, 20)));
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.label("An open-source web graphics editor in the spirit of Macromedia Fireworks MX.");
            ui.label("Part of the Craft family of apps. MIT OR Apache-2.0.");
            ui.weak("Fireworks is a trademark of Adobe. FireCraft is an independent, clean-room project.");
            close = ui.button("OK").clicked();
        });
        if close {
            app.dialogs.about = false;
        }
    }
}

/// Fireworks MX / High precision picker (Preferences and the document Property inspector).
pub fn color_math_combo(ui: &mut egui::Ui, id: &str, value: &mut firecraft_raster::ColorMath) -> bool {
    let mut changed = false;
    egui::ComboBox::from_id_salt(id)
        .selected_text(value.label())
        .show_ui(ui, |ui| {
            for m in firecraft_raster::ColorMath::ALL {
                changed |= ui.selectable_value(value, m, m.label()).changed();
            }
        });
    changed
}

/// Edit > Preferences.
fn preferences(app: &mut FireCraftApp, ctx: &egui::Context) {
    let mut close = false;
    let prefs = &mut app.prefs;
    dialog("Preferences").show(ctx, |ui| {
        ui.strong("Workspace");
        ui.checkbox(&mut prefs.tabs, "Open documents in tabs (instead of separate document windows)");
        ui.horizontal(|ui| {
            ui.label("Tool cursors:");
            egui::ComboBox::from_id_salt("prefs_cursor_style")
                .selected_text(prefs.cursor_style.label())
                .show_ui(ui, |ui| {
                    for t in crate::cursors::CursorStyle::ALL {
                        ui.selectable_value(&mut prefs.cursor_style, t, t.label());
                    }
                });
        })
        .response
        .on_hover_text("Fireworks: FireCraft's own cursors, the same on every system (black Pointer, white Subselection, rotate, resize...). System: the system's cursors where one exists.");
        ui.separator();
        ui.strong("Editing");
        ui.checkbox(&mut prefs.live_drag, "Show objects while moving them")
            .on_hover_text("On: objects follow the pointer when dragged, scaled, rotated, skewed or distorted. Off: only their outline or box moves and the canvas redraws when you let go (lighter on slower computers).");
        ui.horizontal(|ui| {
            ui.label("Undo Steps:");
            ui.add(DragValue::new(&mut prefs.undo_steps).range(1..=firecraft_engine::MAX_UNDO_STEPS));
        });
        ui.separator();
        ui.strong("Color math");
        ui.horizontal(|ui| {
            ui.label("New documents:");
            color_math_combo(ui, "prefs_color_math", &mut prefs.color_math);
        });
        ui.weak("FireCraft: Fireworks' formulas in double precision (no byte rounding of hue, saturation, lightness).\nFireworks MX: bit-exact Fireworks results. Files from Fireworks always open with Fireworks MX math.");
        ui.separator();
        close = ui.button("OK").clicked();
    });
    if close {
        app.dialogs.prefs = false;
    }
}
