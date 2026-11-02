//! The right-hand panel dock: Optimize, Layers, Frames and History, Assets, Answers.

use egui::{Color32, Rect, RichText, Sense, Stroke, StrokeKind, vec2};
use firecraft_doc::ElementKind;
use firecraft_engine::ElementRef;

use crate::icons::{self, Glyph};
use crate::{FireCraftApp, theme};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Optimize,
    Layers,
    Frames,
    Assets,
    Answers,
}

fn group_open(ui: &egui::Ui, g: Group, default: bool) -> bool {
    ui.ctx()
        .data(|d| d.get_temp(egui::Id::unique(("dock_open", g as u8))))
        .unwrap_or(default)
}

fn set_group_open(ui: &egui::Ui, g: Group, v: bool) {
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(("dock_open", g as u8)), v));
}

fn group(ui: &mut egui::Ui, g: Group, title: &str, default: bool, body: impl FnOnce(&mut egui::Ui)) {
    let mut open = group_open(ui, g, default);
    theme::group_header(ui, title, &mut open);
    set_group_open(ui, g, open);
    if open {
        egui::Frame::new()
            .fill(theme::PANEL)
            .inner_margin(egui::Margin::same(3))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                body(ui);
            });
    }
    ui.add_space(2.0);
}

pub fn dock(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            group(ui, Group::Optimize, "Optimize", false, |ui| {
                ui.label("Export format: PNG 32");
                ui.weak("GIF/JPEG optimization and palettes are on the roadmap.");
            });
            group(ui, Group::Layers, "Layers", true, |ui| layers(app, ui));
            group(ui, Group::Frames, "Frames and History", true, |ui| {
                frames_history(app, ui)
            });
            group(ui, Group::Assets, "Assets (Library)", false, |ui| {
                crate::library::panel(app, ui)
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
    p.rect_stroke(
        r,
        0.0,
        Stroke::new(1.0, Color32::from_gray(170)),
        StrokeKind::Inside,
    );
    let inner = r.shrink(4.0);
    match kind {
        ElementKind::Bitmap(_) => {
            p.rect_filled(
                Rect::from_min_size(inner.min, inner.size() / 2.0),
                0.0,
                Color32::from_rgb(230, 120, 60),
            );
            p.rect_filled(
                Rect::from_min_size(inner.center(), inner.size() / 2.0),
                0.0,
                Color32::from_rgb(60, 130, 220),
            );
        }
        ElementKind::Path(path) => {
            let c = path
                .fill
                .as_ref()
                .map(|f| f.color)
                .or(path.stroke.as_ref().map(|s| s.color));
            let c = c
                .map(|c| Color32::from_rgb(c.r, c.g, c.b))
                .unwrap_or(Color32::GRAY);
            p.rect_stroke(inner, 0.0, Stroke::new(1.5, c), StrokeKind::Inside);
        }
        ElementKind::Text(_) => {
            p.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "T",
                egui::FontId::proportional(12.0),
                Color32::BLACK,
            );
        }
        _ => {
            p.rect_filled(inner, 0.0, Color32::from_rgb(120, 220, 120));
        }
    }
}

/// Layers panel state between frames (kept in egui's temporary data).
#[derive(Clone, Default)]
struct LayersUi {
    /// Renaming a layer (object `None`) or an object, and the name being typed.
    renaming: Option<(usize, Option<usize>, String)>,
    /// A row being dragged.
    dragging: Option<Dragged>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dragged {
    Layer(usize),
    /// The selected objects.
    Objects,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Row {
    Layer(usize),
    Object(usize, usize),
}

/// Where a drag would drop: objects into (layer, index), or a layer to a slot (bottom first),
/// and the y of the insertion line.
enum Drop {
    Objects(usize, usize, f32),
    Layer(usize, f32),
}

fn drop_target(
    d: &firecraft_engine::DocState,
    rows: &[(Rect, Row)],
    dragged: Dragged,
    y: f32,
) -> Option<Drop> {
    let (rect, row) = rows
        .iter()
        .find(|(r, _)| y >= r.top() && y < r.bottom())
        .copied()?;
    let upper = y < rect.center().y;
    match dragged {
        Dragged::Objects => match row {
            Row::Object(li, ei) => Some(Drop::Objects(
                li,
                if upper { ei + 1 } else { ei },
                if upper { rect.top() } else { rect.bottom() },
            )),
            Row::Layer(li) => {
                let n = d.doc.layers.get(li)?.elements(d.doc.current_frame).len();
                Some(Drop::Objects(li, n, rect.bottom()))
            }
        },
        Dragged::Layer(_) => {
            // The layer's whole block (its row and its objects' rows).
            let li = match row {
                Row::Layer(li) | Row::Object(li, _) => li,
            };
            let block = rows
                .iter()
                .filter(|(_, r)| matches!(r, Row::Layer(l) | Row::Object(l, _) if *l == li))
                .map(|(r, _)| *r)
                .reduce(|a, b| a.union(b))?;
            let above = y < block.center().y;
            Some(Drop::Layer(
                if above { li + 1 } else { li },
                if above { block.top() } else { block.bottom() },
            ))
        }
    }
}

fn layers(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let Some(d) = app.session.active_doc_mut() else {
        ui.weak("No document");
        return;
    };
    let state_id = egui::Id::unique(("layers_ui", d.id));
    let mut st: LayersUi = ui.ctx().data(|m| m.get_temp(state_id)).unwrap_or_default();
    let frame = d.doc.current_frame;
    let mut select: Option<ElementRef> = None;
    let mut set_current: Option<usize> = None;
    let mut toggle_vis: Option<usize> = None;
    let mut toggle_lock: Option<usize> = None;
    let mut toggle_disc: Option<usize> = None;
    let mut rename: Option<(usize, Option<usize>, String)> = None;
    let mut rows: Vec<(Rect, Row)> = Vec::new();
    // A name being edited: Enter or clicking elsewhere keeps it, Escape cancels.
    let name_edit =
        |ui: &mut egui::Ui, st: &mut LayersUi, rename: &mut Option<(usize, Option<usize>, String)>| {
            let Some((li, obj, text)) = st.renaming.as_mut() else {
                return;
            };
            let r = ui.add(egui::TextEdit::singleline(text).desired_width(110.0));
            if !r.has_focus() && !r.lost_focus() {
                r.request_focus();
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                st.renaming = None;
            } else if r.lost_focus() {
                *rename = Some((*li, *obj, text.clone()));
                st.renaming = None;
            }
        };
    // Top layer first, as Fireworks lists them.
    for li in (0..d.doc.layers.len()).rev() {
        let Some(layer) = d.doc.layers.get(li) else {
            continue;
        };
        let current = d.doc.current_layer == li;
        let header = ui.horizontal(|ui| {
            let glyph = if layer.disclosed {
                Glyph::Minus
            } else {
                Glyph::Plus
            };
            if icons::glyph_button(ui, glyph, 14.0, true).clicked() {
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
            if st
                .renaming
                .as_ref()
                .is_some_and(|(l, o, _)| *l == li && o.is_none())
            {
                name_edit(ui, &mut st, &mut rename);
                return;
            }
            let name = layer.name.clone();
            let text = if current {
                RichText::new(name)
                    .color(Color32::WHITE)
                    .background_color(theme::SELECT_BLUE)
            } else {
                RichText::new(name)
            };
            let r = ui.add(egui::Label::new(text).sense(Sense::click_and_drag()));
            let r = if layer.web {
                r
            } else {
                r.on_hover_text("Double-click to rename; drag to reorder")
            };
            if r.clicked() {
                set_current = Some(li);
            }
            if r.double_clicked() && !layer.web {
                st.renaming = Some((li, None, layer.name.clone()));
            }
            if r.drag_started() && !layer.web {
                st.dragging = Some(Dragged::Layer(li));
            }
        });
        rows.push((header.response.rect, Row::Layer(li)));
        if layer.disclosed {
            for (ei, el) in layer.elements(frame).iter().enumerate().rev() {
                let r = ElementRef { layer: li, index: ei };
                let selected = d.selection.contains(&r);
                let editing = st
                    .renaming
                    .as_ref()
                    .is_some_and(|(l, o, _)| *l == li && *o == Some(ei));
                // The whole row (thumbnail, name and the space after it) selects the object.
                let background = ui.painter().add(egui::Shape::Noop);
                let row = ui.horizontal(|ui| {
                    ui.add_space(46.0);
                    thumbnail(ui, &el.kind);
                    if editing {
                        name_edit(ui, &mut st, &mut rename);
                        return;
                    }
                    let label = el.label();
                    let text = if selected {
                        RichText::new(label).color(Color32::WHITE)
                    } else {
                        RichText::new(label)
                    };
                    ui.add(egui::Label::new(text).selectable(false));
                    ui.add_space(ui.available_width());
                });
                let rect = row.response.rect;
                rows.push((rect, Row::Object(li, ei)));
                if selected {
                    let band = egui::Rect::from_min_max(egui::pos2(rect.left() + 44.0, rect.top()), rect.max);
                    ui.painter().set(
                        background,
                        egui::Shape::rect_filled(band, 0.0, theme::SELECT_BLUE),
                    );
                }
                if editing {
                    continue;
                }
                let click = ui
                    .interact(
                        rect,
                        ui.scope_id().with(("layer_row", li, ei)),
                        Sense::click_and_drag(),
                    )
                    .on_hover_text("Double-click to rename; drag to move (also to another layer)");
                if click.clicked() {
                    select = Some(r);
                }
                if click.double_clicked() {
                    st.renaming = Some((li, Some(ei), el.name.clone().unwrap_or_default()));
                }
                if click.drag_started() {
                    // Dragging an unselected object moves just that one.
                    if !selected {
                        d.selection = vec![r];
                        d.pixels = None;
                    }
                    st.dragging = Some(Dragged::Objects);
                }
            }
        }
    }
    // Drag in progress: show where it would land; drop on release.
    if let Some(dragged) = st.dragging {
        let (pos, released, down) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.any_released(),
                i.pointer.any_down(),
            )
        });
        let target = pos.and_then(|p| drop_target(d, &rows, dragged, p.y));
        if let Some(t) = &target {
            let y = match t {
                Drop::Objects(_, _, y) | Drop::Layer(_, y) => *y,
            };
            let x = rows.first().map_or(ui.min_rect().x_range(), |(r, _)| r.x_range());
            ui.painter().hline(x, y, Stroke::new(2.0, theme::SELECT_BLUE));
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if released || !down {
            match (dragged, target) {
                (Dragged::Objects, Some(Drop::Objects(li, index, _))) => d.move_selection_to(li, index),
                (Dragged::Layer(from), Some(Drop::Layer(slot, _))) => {
                    d.move_layer(from, if from < slot { slot - 1 } else { slot });
                }
                _ => {}
            }
            st.dragging = None;
        }
    }
    if let Some((li, obj, name)) = rename {
        match obj {
            None => d.rename_layer(li, &name),
            Some(ei) => d.rename_object(ElementRef { layer: li, index: ei }, &name),
        }
    }
    if let Some(i) = toggle_vis {
        if let Some(l) = d.doc.layers.get_mut(i) {
            l.visible = !l.visible;
        }
        d.revision += 1;
    }
    if let Some(i) = toggle_lock
        && let Some(l) = d.doc.layers.get_mut(i)
    {
        l.locked = !l.locked;
    }
    if let Some(i) = toggle_disc
        && let Some(l) = d.doc.layers.get_mut(i)
    {
        l.disclosed = !l.disclosed;
    }
    if let Some(i) = set_current {
        d.doc.current_layer = i;
    }
    if let Some(r) = select {
        d.doc.current_layer = r.layer;
        // Shift+click adds or removes an object, as on the canvas.
        if ui.input(|i| i.modifiers.shift) {
            if let Some(k) = d.selection.iter().position(|s| *s == r) {
                d.selection.remove(k);
            } else {
                d.selection.push(r);
            }
        } else {
            d.selection = vec![r];
        }
        d.pixels = None;
    }
    ui.ctx().data_mut(|m| m.insert_temp(state_id, st));
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(format!("Frame {}", frame + 1));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::glyph_button(ui, Glyph::Trash, 16.0, true)
                .on_hover_text("Delete Layer")
                .clicked()
            {
                let cur = d.doc.current_layer;
                d.delete_layer(cur);
            }
            if icons::glyph_button(ui, Glyph::Plus, 16.0, true)
                .on_hover_text("New Layer")
                .clicked()
            {
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
    let Some(d) = app.session.active_doc_mut() else {
        return;
    };
    if history {
        // Every step, oldest first; steps after the current one are greyed (undone). Clicking a
        // step moves the document to the state just after it, backwards or forwards.
        let done: Vec<String> = d.history.labels().map(str::to_owned).collect();
        let undone: Vec<String> = d.history.redo_labels().map(str::to_owned).collect();
        let current = done.len();
        let mut target: Option<usize> = None;
        let row = |ui: &mut egui::Ui, text: String, active: bool, here: bool| {
            let rt = if here {
                RichText::new(text)
                    .color(Color32::WHITE)
                    .background_color(theme::SELECT_BLUE)
            } else if active {
                RichText::new(text)
            } else {
                RichText::new(text).color(Color32::GRAY)
            };
            ui.add(egui::Label::new(rt).sense(Sense::click())).clicked()
        };
        if row(ui, "(Original)".into(), true, current == 0) {
            target = Some(0);
        }
        for (i, l) in done.iter().chain(undone.iter()).enumerate() {
            if row(ui, format!("{}. {l}", i + 1), i < current, i + 1 == current) {
                target = Some(i + 1);
            }
        }
        if let Some(t) = target {
            for _ in t..current {
                d.undo();
            }
            for _ in current..t {
                d.redo();
            }
        }
        ui.horizontal(|ui| {
            ui.add_enabled(false, egui::Button::new("Replay"))
                .on_disabled_hover_text("Replaying steps is on the roadmap");
            ui.weak(format!("{} of {} steps", current, d.history.limit));
        });
    } else {
        let mut pick = None;
        egui::ScrollArea::vertical()
            .max_height(220.0)
            .id_salt("frames_list")
            .show(ui, |ui| {
                for (i, f) in d.doc.frames.iter().enumerate() {
                    let cur = i == d.doc.current_frame;
                    ui.horizontal(|ui| {
                        let text = format!("{}  {}", i + 1, f.name);
                        let rt = if cur {
                            RichText::new(text)
                                .color(Color32::WHITE)
                                .background_color(theme::SELECT_BLUE)
                        } else {
                            RichText::new(text)
                        };
                        if ui.add(egui::Label::new(rt).sense(Sense::click())).clicked() {
                            pick = Some(i);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.weak(f.delay.to_string())
                        })
                        .response
                        .on_hover_text("Frame delay (1/100 s)");
                    });
                }
            });
        if let Some(i) = pick {
            d.doc.current_frame = i;
        }
        ui.weak("Adding, removing and timing frames is on the roadmap.");
    }
}
