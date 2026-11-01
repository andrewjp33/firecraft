//! The Property inspector: one context-sensitive panel for the active tool, the selection or
//! the document (Fireworks MX's signature panel).

use egui::{vec2, Color32, ComboBox, DragValue, Rect, Sense, Slider};
use firecraft_doc::ElementKind;
use firecraft_engine::Tool;
use firecraft_raster::{AntiAliasStyle, Edge};

use crate::{icons, theme, tools_panel, FireCraftApp};

fn header(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let mut open = app.pi_open;
    theme::group_header(ui, "Properties", &mut open);
    app.pi_open = open;
}

/// Left column: icon + name, like Fireworks' PI.
fn identity(ui: &mut egui::Ui, title: &str, tool: Option<Tool>, extra: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.set_width(150.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::hover());
            if let Some(t) = tool {
                icons::paint(ui.painter(), Rect::from_center_size(r.center(), vec2(20.0, 20.0)), t, true);
            }
            ui.label(egui::RichText::new(title).size(11.0));
        });
        extra(ui);
    });
    ui.separator();
}

fn edge_combo(ui: &mut egui::Ui, id: &str, edge: &mut Edge) {
    let label = match edge {
        Edge::Hard => "Hard Edge",
        Edge::AntiAlias => "Anti-alias",
        Edge::Feather(_) => "Feather",
    };
    ui.label("Edge:");
    ComboBox::from_id_salt(id).width(90.0).selected_text(label).show_ui(ui, |ui| {
        let amount = match edge {
            Edge::Feather(a) => *a,
            _ => 10,
        };
        ui.selectable_value(edge, Edge::Hard, "Hard Edge");
        ui.selectable_value(edge, Edge::AntiAlias, "Anti-alias");
        ui.selectable_value(edge, Edge::Feather(amount), "Feather");
    });
    // The amount box is only live for Feather (grayed otherwise), exactly as in Fireworks MX.
    let mut amount = match edge {
        Edge::Feather(a) => *a,
        _ => 0,
    };
    let feather = matches!(edge, Edge::Feather(_));
    if ui.add_enabled(feather, DragValue::new(&mut amount).range(0..=250)).changed() {
        *edge = Edge::Feather(amount);
    }
}

pub fn show(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    header(app, ui);
    if !app.pi_open {
        return;
    }
    ui.add_space(2.0);
    ui.horizontal_top(|ui| {
        let tool = app.session.tool;
        let has_selection = app.session.active_doc().is_some_and(|d| !d.selection.is_empty());
        match tool {
            Tool::MagicWand => wand(app, ui),
            Tool::NineSlice => crate::nine::inspector(app, ui),
            Tool::Marquee | Tool::OvalMarquee | Tool::Lasso => marquee(app, ui, tool),
            Tool::Rectangle | Tool::Ellipse | Tool::Line | Tool::Pen if !has_selection => shape_tool(app, ui, tool),
            Tool::Zoom | Tool::Hand => view_tool(ui, tool),
            _ if has_selection => object(app, ui),
            _ => document(app, ui),
        }
    });
}

fn wand(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    identity(ui, "Magic Wand tool", Some(Tool::MagicWand), |_| {});
    let o = &mut app.session.options;
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label("Tolerance:");
            ui.add(Slider::new(&mut o.tolerance, 0..=255));
            edge_combo(ui, "wand_edge", &mut o.wand_edge);
        });
        ui.horizontal(|ui| {
            // FireCraft addition (see docs/behavior/magic-wand.md): Fireworks MX ties the
            // anti-alias strength to Tolerance; "Smooth" computes real edge coverage instead.
            ui.label("Anti-alias:");
            ComboBox::from_id_salt("aa_style")
                .width(140.0)
                .selected_text(match o.aa_style {
                    AntiAliasStyle::Smooth => "Smooth (FireCraft)",
                    AntiAliasStyle::Classic => "Classic (Fireworks MX)",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut o.aa_style, AntiAliasStyle::Smooth, "Smooth (FireCraft)")
                        .on_hover_text("Edge coverage from the image itself, independent of Tolerance");
                    ui.selectable_value(&mut o.aa_style, AntiAliasStyle::Classic, "Classic (Fireworks MX)")
                        .on_hover_text("Bit-compatible with Fireworks MX: edge strength equals Tolerance");
                });
        });
    });
}

fn marquee(app: &mut FireCraftApp, ui: &mut egui::Ui, tool: Tool) {
    identity(ui, tool.label(), Some(tool), |_| {});
    let o = &mut app.session.options;
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label("Style:");
            ComboBox::from_id_salt("marquee_style").width(90.0).selected_text("Normal").show_ui(ui, |ui| {
                let _ = ui.selectable_label(true, "Normal");
                ui.add_enabled(false, egui::Button::new("Fixed Ratio"));
                ui.add_enabled(false, egui::Button::new("Fixed Size"));
            });
            edge_combo(ui, "marquee_edge", &mut o.marquee_edge);
        });
        ui.weak("Shift: add to selection   Alt: subtract   Shift+Alt: intersect");
    });
}

fn shape_tool(app: &mut FireCraftApp, ui: &mut egui::Ui, tool: Tool) {
    identity(ui, tool.label(), Some(tool), |_| {});
    let o = &mut app.session.options;
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            icons::glyph(ui, icons::Glyph::Bucket, 16.0);
            if tools_panel::swatch(ui, o.fill, vec2(22.0, 14.0)).clicked() {
                app.dialogs.color_target = Some(false);
            }
            ComboBox::from_id_salt("fill_kind")
                .width(80.0)
                .selected_text(if o.fill.is_some() { "Solid" } else { "None" })
                .show_ui(ui, |ui| {
                    if ui.selectable_label(o.fill.is_none(), "None").clicked() {
                        o.fill = None;
                    }
                    if ui.selectable_label(o.fill.is_some(), "Solid").clicked() && o.fill.is_none() {
                        o.fill = Some(firecraft_raster::Rgba::new(0x99, 0x99, 0x99, 255));
                    }
                    ui.add_enabled(false, egui::Button::new("Linear (soon)"));
                });
            ui.separator();
            icons::glyph(ui, icons::Glyph::Pencil, 16.0);
            if tools_panel::swatch(ui, o.stroke, vec2(22.0, 14.0)).clicked() {
                app.dialogs.color_target = Some(true);
            }
            ui.add(DragValue::new(&mut o.stroke_size).range(1..=100).suffix(" px"));
            ComboBox::from_id_salt("stroke_kind")
                .width(110.0)
                .selected_text(if o.stroke.is_some() { "1-Pixel Soft" } else { "None" })
                .show_ui(ui, |ui| {
                    if ui.selectable_label(o.stroke.is_none(), "None").clicked() {
                        o.stroke = None;
                    }
                    if ui.selectable_label(o.stroke.is_some(), "Pencil").clicked() && o.stroke.is_none() {
                        o.stroke = Some(firecraft_raster::Rgba::BLACK);
                    }
                });
        });
    });
}

fn object(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let Some(d) = app.session.active_doc() else { return };
    let count = d.selection.len();
    let first = d.selection.first().and_then(|r| d.element(*r)).cloned();
    let bounds = first.as_ref().and_then(|e| e.bounds());
    let (title, tool) = match first.as_ref().map(|e| &e.kind) {
        _ if count > 1 => (format!("{count} objects"), Some(Tool::Pointer)),
        Some(ElementKind::Bitmap(_)) => ("Bitmap".into(), Some(Tool::Marquee)),
        Some(ElementKind::Path(p)) if p.contours.iter().all(|c| c.closed) => ("Path".into(), Some(Tool::Pen)),
        Some(ElementKind::Path(_)) => ("Path".into(), Some(Tool::Line)),
        Some(ElementKind::Text(_)) => ("Text".into(), Some(Tool::Text)),
        Some(ElementKind::Group(_)) => ("Group".into(), Some(Tool::Pointer)),
        Some(ElementKind::Other { kind, .. }) => (kind.clone(), Some(Tool::Slice)),
        None => ("".into(), None),
    };
    let mut move_to: Option<(f64, f64)> = None;
    identity(ui, &title, tool, |ui| {
        if let Some((x, y, w, h)) = bounds {
            let (mut nx, mut ny) = (x.round(), y.round());
            egui::Grid::new("pi_geom").spacing(vec2(4.0, 2.0)).show(ui, |ui| {
                ui.label("W:");
                ui.add_enabled(false, DragValue::new(&mut w.round()));
                ui.label("X:");
                if ui.add(DragValue::new(&mut nx)).changed() {
                    move_to = Some((nx - x, 0.0));
                }
                ui.end_row();
                ui.label("H:");
                ui.add_enabled(false, DragValue::new(&mut h.round()));
                ui.label("Y:");
                if ui.add(DragValue::new(&mut ny)).changed() {
                    move_to = Some((0.0, ny - y));
                }
                ui.end_row();
            });
        }
    });
    if let Some((dx, dy)) = move_to {
        if let Some(d) = app.session.active_doc_mut() {
            d.move_selection(dx, dy, "Move");
        }
    }
    // Fill/stroke of a single path.
    if let Some(ElementKind::Path(p)) = first.as_ref().map(|e| &e.kind) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                icons::glyph(ui, icons::Glyph::Bucket, 16.0);
                tools_panel::swatch(ui, p.fill.as_ref().map(|f| f.color), vec2(22.0, 14.0));
                ui.label(if p.fill.is_some() { "Solid" } else { "None" });
                ui.separator();
                icons::glyph(ui, icons::Glyph::Pencil, 16.0);
                tools_panel::swatch(ui, p.stroke.as_ref().map(|s| s.color), vec2(22.0, 14.0));
                if let Some(s) = &p.stroke {
                    ui.label(format!("{} px", s.diameter));
                }
            });
        });
        ui.separator();
    }
    // Opacity / blend / effects column.
    let mut opacity = first.as_ref().map(|e| e.opacity).unwrap_or(100);
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            icons::glyph(ui, icons::Glyph::Opacity, 16.0);
            if ui.add(DragValue::new(&mut opacity).range(0..=100).suffix("%")).changed() {
                if let Some(d) = app.session.active_doc_mut() {
                    d.set_opacity(opacity);
                }
            }
            let current = first.as_ref().map(|e| e.blend).unwrap_or_default();
            let mut chosen = None;
            ComboBox::from_id_salt("blend").width(90.0).selected_text(current.label()).show_ui(ui, |ui| {
                for m in firecraft_raster::BlendMode::ALL {
                    if ui.selectable_label(m == current, m.label()).clicked() && m != current {
                        chosen = Some(m);
                    }
                }
            });
            if let (Some(m), Some(d)) = (chosen, app.session.active_doc_mut()) {
                d.set_blend(m);
            }
        });
        ui.horizontal(|ui| {
            ui.label("Effects:");
            ui.add_enabled(false, egui::Button::new("+").small()).on_disabled_hover_text("Live Effects are on the roadmap");
        });
        let has_effects = first.as_ref().and_then(|e| e.raw.as_ref()).and_then(|r| r.child("EFL")).is_some_and(|e| e.child("EPS").is_some_and(|p| !p.children.is_empty()));
        let (r, _) = ui.allocate_exact_size(vec2(160.0, 40.0), Sense::hover());
        ui.painter().rect_filled(r, 0.0, Color32::WHITE);
        ui.painter().rect_stroke(r, 0.0, egui::Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
        if has_effects {
            ui.painter().text(r.left_top() + vec2(4.0, 4.0), egui::Align2::LEFT_TOP, "(effects preserved)", egui::FontId::proportional(10.0), Color32::GRAY);
        }
    });
}

fn document(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let Some(d) = app.session.active_doc() else {
        identity(ui, app.session.tool.label(), Some(app.session.tool), |_| {});
        ui.weak("No document open. File > New (Ctrl+N) or File > Open (Ctrl+O), or drop a PNG here.");
        return;
    };
    let canvas = d.doc.canvas;
    let title = d.doc.title();
    identity(ui, "Document", None, |ui| {
        ui.label(egui::RichText::new(title).size(11.0));
    });
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label("Canvas:");
            if tools_panel::swatch(ui, canvas, vec2(22.0, 14.0)).on_hover_text("Canvas color").clicked() {
                app.dialogs.canvas_color = true;
            }
            if ui.button("Canvas Size...").clicked() {
                app.dialogs.open_canvas_size(app.session.active_doc());
            }
            ui.add_enabled(false, egui::Button::new("Image Size...")).on_disabled_hover_text("Coming soon");
        });
    });
}

fn view_tool(ui: &mut egui::Ui, tool: Tool) {
    identity(ui, tool.label(), Some(tool), |_| {});
    ui.vertical(|ui| {
        if tool == Tool::Zoom {
            ui.label("Click: zoom in    Right-click or Alt+click: zoom out");
            ui.label("Drag a box: zoom to fit it    Ctrl+wheel: zoom at the pointer");
        } else {
            ui.label("Drag to scroll the view    Mouse wheel: scroll");
        }
    });
}
