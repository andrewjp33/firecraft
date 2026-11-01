//! The right-hand panel dock: Optimize, Layers, Frames and History, Assets, Answers.

use egui::{vec2, Color32, Rect, RichText, Sense, Stroke, StrokeKind};
use firecraft_doc::ElementKind;
use firecraft_engine::ElementRef;

use crate::icons::{self, Glyph};
use crate::{theme, FireCraftApp};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Optimize,
    Layers,
    Frames,
    Assets,
    Answers,
}

fn group_open(ui: &egui::Ui, g: Group, default: bool) -> bool {
    ui.ctx().data(|d| d.get_temp(egui::Id::unique(("dock_open", g as u8)))).unwrap_or(default)
}

fn set_group_open(ui: &egui::Ui, g: Group, v: bool) {
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::unique(("dock_open", g as u8)), v));
}

fn group(ui: &mut egui::Ui, g: Group, title: &str, default: bool, body: impl FnOnce(&mut egui::Ui)) {
    let mut open = group_open(ui, g, default);
    theme::group_header(ui, title, &mut open);
    set_group_open(ui, g, open);
    if open {
        egui::Frame::new().fill(theme::PANEL).inner_margin(egui::Margin::same(3)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            body(ui);
        });
    }
    ui.add_space(2.0);
}

pub fn dock(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        group(ui, Group::Optimize, "Optimize", false, |ui| {
            ui.label("Export format: PNG 32");
            ui.weak("GIF/JPEG optimization and palettes are on the roadmap.");
        });
        group(ui, Group::Layers, "Layers", true, |ui| layers(app, ui));
        group(ui, Group::Frames, "Frames and History", true, |ui| frames_history(app, ui));
        group(ui, Group::Assets, "Assets", false, |ui| {
            ui.weak("Styles, URL and Library panels are on the roadmap.");
        });
        group(ui, Group::Answers, "Answers", false, |ui| {
            ui.weak("FireCraft documentation: docs/ in the source tree.");
        });
    });
}

fn thumbnail(ui: &mut egui::Ui, kind: &ElementKind) {
    let (r, _) = ui.allocate_exact_size(vec2(26.0, 20.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, 0.0, Color32::WHITE);
    p.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_gray(170)), StrokeKind::Inside);
    let inner = r.shrink(4.0);
    match kind {
        ElementKind::Bitmap(_) => {
            p.rect_filled(Rect::from_min_size(inner.min, inner.size() / 2.0), 0.0, Color32::from_rgb(230, 120, 60));
            p.rect_filled(Rect::from_min_size(inner.center(), inner.size() / 2.0), 0.0, Color32::from_rgb(60, 130, 220));
        }
        ElementKind::Path(path) => {
            let c = path.fill.as_ref().map(|f| f.color).or(path.stroke.as_ref().map(|s| s.color));
            let c = c.map(|c| Color32::from_rgb(c.r, c.g, c.b)).unwrap_or(Color32::GRAY);
            p.rect_stroke(inner, 0.0, Stroke::new(1.5, c), StrokeKind::Inside);
        }
        ElementKind::Text(_) => {
            p.text(r.center(), egui::Align2::CENTER_CENTER, "T", egui::FontId::proportional(12.0), Color32::BLACK);
        }
        _ => {
            p.rect_filled(inner, 0.0, Color32::from_rgb(120, 220, 120));
        }
    }
}

fn layers(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let Some(d) = app.session.active_doc_mut() else {
        ui.weak("No document");
        return;
    };
    let frame = d.doc.current_frame;
    let mut select: Option<ElementRef> = None;
    let mut set_current: Option<usize> = None;
    let mut toggle_vis: Option<usize> = None;
    let mut toggle_lock: Option<usize> = None;
    let mut toggle_disc: Option<usize> = None;
    // Top layer first, as Fireworks lists them.
    for li in (0..d.doc.layers.len()).rev() {
        let Some(layer) = d.doc.layers.get(li) else { continue };
        let current = d.doc.current_layer == li;
        ui.horizontal(|ui| {
            if icons::glyph_button(ui, if layer.disclosed { Glyph::Minus } else { Glyph::Plus }, 14.0, true).clicked() {
                toggle_disc = Some(li);
            }
            let eye = icons::glyph_button(ui, Glyph::Eye, 16.0, true);
            if !layer.visible {
                ui.painter().rect_filled(eye.rect.shrink(1.0), 0.0, theme::PANEL);
            }
            if eye.on_hover_text("Show/Hide Layer").clicked() {
                toggle_vis = Some(li);
            }
            let lock = icons::glyph_button(ui, Glyph::Lock, 14.0, true);
            if !layer.locked {
                ui.painter().rect_filled(lock.rect.shrink(1.0), 0.0, theme::PANEL);
            }
            if lock.on_hover_text("Lock/Unlock Layer").clicked() {
                toggle_lock = Some(li);
            }
            icons::glyph(ui, if layer.web { Glyph::Globe } else { Glyph::Folder }, 15.0);
            let name = layer.name.clone();
            let text = if current { RichText::new(name).color(Color32::WHITE).background_color(theme::SELECT_BLUE) } else { RichText::new(name) };
            if ui.add(egui::Label::new(text).sense(Sense::click())).clicked() {
                set_current = Some(li);
            }
        });
        if layer.disclosed {
            for (ei, el) in layer.elements(frame).iter().enumerate().rev() {
                let r = ElementRef { layer: li, index: ei };
                let selected = d.selection.contains(&r);
                ui.horizontal(|ui| {
                    ui.add_space(46.0);
                    thumbnail(ui, &el.kind);
                    let label = el.label();
                    let text = if selected { RichText::new(label).color(Color32::WHITE).background_color(theme::SELECT_BLUE) } else { RichText::new(label) };
                    if ui.add(egui::Label::new(text).sense(Sense::click())).clicked() {
                        select = Some(r);
                    }
                });
            }
        }
    }
    if let Some(i) = toggle_vis {
        if let Some(l) = d.doc.layers.get_mut(i) {
            l.visible = !l.visible;
        }
        d.revision += 1;
    }
    if let Some(i) = toggle_lock {
        if let Some(l) = d.doc.layers.get_mut(i) {
            l.locked = !l.locked;
        }
    }
    if let Some(i) = toggle_disc {
        if let Some(l) = d.doc.layers.get_mut(i) {
            l.disclosed = !l.disclosed;
        }
    }
    if let Some(i) = set_current {
        d.doc.current_layer = i;
    }
    if let Some(r) = select {
        d.doc.current_layer = r.layer;
        d.selection = vec![r];
        d.pixels = None;
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(format!("Frame {}", frame + 1));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::glyph_button(ui, Glyph::Trash, 16.0, true).on_hover_text("Delete Layer").clicked() {
                let cur = d.doc.current_layer;
                d.delete_layer(cur);
            }
            if icons::glyph_button(ui, Glyph::Plus, 16.0, true).on_hover_text("New Layer").clicked() {
                d.new_layer();
            }
        });
    });
}

fn frames_history(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let tab_id = egui::Id::unique("frames_history_tab");
    let mut history = ui.ctx().data(|d| d.get_temp::<bool>(tab_id)).unwrap_or(false);
    ui.horizontal(|ui| {
        ui.selectable_value(&mut history, false, "Frames");
        ui.selectable_value(&mut history, true, "History");
    });
    ui.ctx().data_mut(|d| d.insert_temp(tab_id, history));
    let Some(d) = app.session.active_doc_mut() else { return };
    if history {
        let mut undo_to: Option<usize> = None;
        let labels: Vec<String> = d.history.labels().map(str::to_owned).collect();
        let redo: Vec<String> = d.history.redo_labels().map(str::to_owned).collect();
        let n = labels.len();
        for (i, l) in labels.iter().enumerate() {
            if ui.selectable_label(false, format!("{}. {l}", i + 1)).on_hover_text("Click to undo back to here").clicked() {
                undo_to = Some(n - i);
            }
        }
        for l in redo.iter().rev() {
            ui.label(RichText::new(format!("   {l}")).color(Color32::GRAY));
        }
        if labels.is_empty() && redo.is_empty() {
            ui.weak("No steps yet");
        }
        if let Some(k) = undo_to {
            for _ in 0..k.saturating_sub(1) {
                d.undo();
            }
        }
        ui.horizontal(|ui| {
            ui.add_enabled(false, egui::Button::new("Replay")).on_disabled_hover_text("Replaying steps is on the roadmap");
        });
    } else {
        for (i, f) in d.doc.frames.iter().enumerate() {
            let cur = i == d.doc.current_frame;
            ui.horizontal(|ui| {
                let text = format!("{}  {}", i + 1, f.name);
                let rt = if cur { RichText::new(text).color(Color32::WHITE).background_color(theme::SELECT_BLUE) } else { RichText::new(text) };
                ui.label(rt);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.weak(f.delay.to_string()));
            });
        }
        ui.weak("Animation editing is on the roadmap.");
    }
}
