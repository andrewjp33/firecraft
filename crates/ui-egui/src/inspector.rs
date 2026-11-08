//! The Property inspector: one context-sensitive panel for the active tool, the selection or
//! the document (Fireworks MX's signature panel).

use egui::{ComboBox, DragValue, Rect, Sense, Slider, vec2};
use firecraft_doc::ElementKind;
use firecraft_engine::Tool;
use firecraft_raster::{AntiAliasStyle, Edge};

use crate::fpui::color_picker::{self as cp, ColorChoice, Footer};
use crate::{FireCraftApp, icons, theme};

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
                icons::paint(
                    ui.painter(),
                    Rect::from_center_size(r.center(), vec2(20.0, 20.0)),
                    t,
                    true,
                );
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
    ComboBox::from_id_salt(id)
        .width(90.0)
        .selected_text(label)
        .show_ui(ui, |ui| {
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
    if ui
        .add_enabled(feather, DragValue::new(&mut amount).range(0..=250))
        .changed()
    {
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
            Tool::Brush => brush(app, ui),
            Tool::Pencil => pencil(app, ui),
            Tool::Eraser => eraser(app, ui),
            Tool::PaintBucket => paint_bucket(app, ui),
            Tool::NineSlice => crate::nine::inspector(app, ui),
            t @ (Tool::Scale | Tool::Skew | Tool::Distort) => crate::transform_tool::inspector(app, ui, t),
            Tool::Marquee | Tool::OvalMarquee | Tool::Lasso => marquee(app, ui, tool),
            Tool::Rectangle | Tool::Ellipse | Tool::Line | Tool::Pen if !has_selection => {
                shape_tool(app, ui, tool)
            }
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
                    AntiAliasStyle::Smooth => "FireCraft (smooth)",
                    AntiAliasStyle::Classic => "Fireworks MX (classic)",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut o.aa_style, AntiAliasStyle::Smooth, "FireCraft (smooth)")
                        .on_hover_text("Edge coverage from the image itself, independent of Tolerance");
                    ui.selectable_value(&mut o.aa_style, AntiAliasStyle::Classic, "Fireworks MX (classic)")
                        .on_hover_text("Bit-compatible with Fireworks MX: edge strength equals Tolerance");
                });
        });
    });
}

/// Opacity, blend mode and Preserve Transparency, shared by the painting tools.
fn paint_mix(ui: &mut egui::Ui, o: &mut firecraft_engine::PaintOptions) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.add(DragValue::new(&mut o.opacity).range(0..=100).suffix("%"));
            ComboBox::from_id_salt("paint_blend")
                .width(90.0)
                .selected_text(o.blend.label())
                .show_ui(ui, |ui| {
                    for m in firecraft_raster::BlendMode::ALL {
                        ui.selectable_value(&mut o.blend, m, m.label());
                    }
                });
        });
        ui.checkbox(&mut o.preserve_transparency, "Preserve Transparency");
    });
}

fn brush(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    identity(ui, Tool::Brush.label(), Some(Tool::Brush), |_| {});
    crate::stroke_panel::show(app, ui, crate::stroke_panel::Target::Tool);
    ui.separator();
    paint_mix(ui, &mut app.session.options.paint);
}

fn pencil(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    identity(ui, Tool::Pencil.label(), Some(Tool::Pencil), |_| {});
    let o = &mut app.session.options;
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            icons::glyph(ui, icons::Glyph::Pencil, 16.0);
            let footer = Footer::Stroke {
                placement: None,
                enabled: false,
            };
            match cp::color_box(ui, "pencil_color", vec2(22.0, 14.0), o.stroke, footer) {
                Some(ColorChoice::Color(c)) => o.stroke = Some(cp::to_rgba(c)),
                Some(ColorChoice::None) => o.stroke = None,
                _ => {}
            }
        });
        ui.checkbox(&mut o.paint.pencil_anti_alias, "Anti-Aliased")
            .on_hover_text("Smooths the edges of the lines you draw");
        ui.checkbox(&mut o.paint.pencil_auto_erase, "Auto Erase")
            .on_hover_text("Pressing on a pixel of the stroke colour draws in the fill colour");
    });
    ui.separator();
    paint_mix(ui, &mut o.paint);
}

fn eraser(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    identity(ui, Tool::Eraser.label(), Some(Tool::Eraser), |_| {});
    let o = &mut app.session.options.paint;
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label("Size:");
            ui.add(DragValue::new(&mut o.eraser_size).range(1..=200));
            ui.selectable_value(&mut o.eraser_round, true, "Round");
            ui.selectable_value(&mut o.eraser_round, false, "Square");
        });
        ui.horizontal(|ui| {
            ui.label("Edge:");
            ui.add(DragValue::new(&mut o.eraser_edge).range(0..=100).suffix("%"));
            ui.label("Opacity:");
            ui.add(DragValue::new(&mut o.eraser_opacity).range(0..=100).suffix("%"));
        });
    });
}

fn paint_bucket(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    identity(ui, Tool::PaintBucket.label(), Some(Tool::PaintBucket), |_| {});
    let o = &mut app.session.options;
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            icons::glyph(ui, icons::Glyph::Bucket, 16.0);
            match cp::color_box(
                ui,
                "bucket_color",
                vec2(22.0, 14.0),
                o.fill,
                Footer::Fill { kinds: false },
            ) {
                Some(ColorChoice::Color(c)) => o.fill = Some(cp::to_rgba(c)),
                Some(ColorChoice::None) => o.fill = None,
                _ => {}
            }
            ui.label("Tolerance:");
            ui.add(Slider::new(&mut o.paint.bucket_tolerance, 0..=255));
        });
        ui.horizontal(|ui| {
            edge_combo(ui, "bucket_edge", &mut o.paint.bucket_edge);
            ui.checkbox(&mut o.paint.bucket_fill_selection, "Fill Selection")
                .on_hover_text("Fill the whole pixel selection instead of the similar pixels in it");
        });
    });
    ui.separator();
    paint_mix(ui, &mut o.paint);
}

fn marquee(app: &mut FireCraftApp, ui: &mut egui::Ui, tool: Tool) {
    identity(ui, tool.label(), Some(tool), |_| {});
    let o = &mut app.session.options;
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label("Style:");
            ComboBox::from_id_salt("marquee_style")
                .width(90.0)
                .selected_text("Normal")
                .show_ui(ui, |ui| {
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
        if tool == Tool::Rectangle {
            ui.horizontal(|ui| {
                ui.label("Rectangle roundness:");
                let mut pct = o.roundness * 100.0;
                if ui
                    .add(
                        DragValue::new(&mut pct)
                            .range(0.0..=100.0)
                            .max_decimals(0)
                            .suffix("%"),
                    )
                    .changed()
                {
                    o.roundness = (pct / 100.0).clamp(0.0, 1.0);
                }
            });
        }
        ui.horizontal(|ui| {
            icons::glyph(ui, icons::Glyph::Bucket, 16.0);
            match cp::color_box(
                ui,
                "tool_fill",
                vec2(22.0, 14.0),
                o.fill,
                Footer::Fill { kinds: false },
            ) {
                Some(ColorChoice::Color(c)) => o.fill = Some(cp::to_rgba(c)),
                Some(ColorChoice::None) => o.fill = None,
                _ => {}
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
        });
    });
    // Fireworks CS's stroke controls (FirePixel's layout).
    ui.separator();
    crate::stroke_panel::show(app, ui, crate::stroke_panel::Target::Tool);
}

/// A value field in the Property inspector: changes apply as it's dragged and one History state
/// is recorded on mouse up, as in Fireworks. `apply` gets whether this is part of a drag.
fn value_drag(
    d: &mut firecraft_engine::DocState,
    resp: &egui::Response,
    label: &str,
    apply: impl FnOnce(&mut firecraft_engine::DocState, bool),
) {
    if resp.drag_started() {
        d.begin_live_edit(label);
    }
    if resp.changed() {
        apply(d, resp.dragged() || resp.drag_stopped());
    }
    if resp.drag_stopped() {
        d.commit_live_edit();
    }
}

fn object(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let Some(d) = app.session.active_doc() else { return };
    let count = d.selection.len();
    let first = d.selection.first().and_then(|r| d.element(*r)).cloned();
    let bounds = first.as_ref().and_then(|e| e.bounds());
    let (title, tool) = match first.as_ref().map(|e| &e.kind) {
        _ if count > 1 => (format!("{count} objects"), Some(Tool::Pointer)),
        Some(ElementKind::Bitmap(_)) => ("Bitmap".into(), Some(Tool::Marquee)),
        Some(ElementKind::Path(p)) if p.rect.is_some() => ("Rectangle".into(), Some(Tool::Rectangle)),
        Some(ElementKind::Path(p)) if p.contours.iter().all(|c| c.closed) => ("Path".into(), Some(Tool::Pen)),
        Some(ElementKind::Path(_)) => ("Path".into(), Some(Tool::Line)),
        Some(ElementKind::Text(_)) => ("Text".into(), Some(Tool::Text)),
        Some(ElementKind::Group(_)) => ("Group".into(), Some(Tool::Pointer)),
        Some(ElementKind::Masked { .. }) => ("Masked Group".into(), Some(Tool::Pointer)),
        Some(ElementKind::Instance { name, .. }) => (format!("Symbol: {name}"), Some(Tool::Pointer)),
        Some(ElementKind::Other { kind, .. }) => (kind.clone(), Some(Tool::Slice)),
        None => ("".into(), None),
    };
    let mut moved: Vec<(f64, f64, egui::Response)> = Vec::new();
    // In the Symbol Editor positions are the symbol's own (0,0 = its origin), as in Fireworks.
    let origin = d.symbol_link.map_or((0.0, 0.0), |l| l.offset);
    identity(ui, &title, tool, |ui| {
        if let Some((x, y, w, h)) = bounds {
            let (x, y) = (x - origin.0, y - origin.1);
            let (mut nx, mut ny) = (x.round(), y.round());
            egui::Grid::new("pi_geom").spacing(vec2(4.0, 2.0)).show(ui, |ui| {
                ui.label("W:");
                ui.add_enabled(false, DragValue::new(&mut w.round()));
                ui.label("X:");
                let resp = ui.add(DragValue::new(&mut nx));
                moved.push((nx - x, 0.0, resp));
                ui.end_row();
                ui.label("H:");
                ui.add_enabled(false, DragValue::new(&mut h.round()));
                ui.label("Y:");
                let resp = ui.add(DragValue::new(&mut ny));
                moved.push((0.0, ny - y, resp));
                ui.end_row();
            });
        }
    });
    if let Some(d) = app.session.active_doc_mut() {
        for (dx, dy, resp) in &moved {
            value_drag(d, resp, "Move", |d, live| {
                if live {
                    d.move_selection_live(*dx, *dy);
                } else {
                    d.move_selection(*dx, *dy, "Move");
                }
            });
        }
    }
    // A symbol instance: edit its symbol, or turn it back into objects.
    if count == 1
        && let Some(ElementKind::Instance { symbol, .. }) = first.as_ref().map(|e| &e.kind)
    {
        let id = *symbol;
        ui.vertical(|ui| {
            if ui
                .button("Edit Symbol...")
                .on_hover_text("Open the symbol in the Symbol Editor (or double-click the instance)")
                .clicked()
            {
                let i = app.session.active;
                app.session.edit_symbol(i, id);
            }
            if ui
                .button("Break Apart")
                .on_hover_text("Replace the instance with the symbol's objects")
                .clicked()
                && let Some(d) = app.session.active_doc_mut()
            {
                d.break_apart();
            }
        });
        ui.separator();
    }
    // Fill/stroke of a single path.
    let mut fill_choice: Option<Option<firecraft_raster::Rgba>> = None;
    let is_path = matches!(first.as_ref().map(|e| &e.kind), Some(ElementKind::Path(_)));
    if let Some(ElementKind::Path(p)) = first.as_ref().map(|e| &e.kind) {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                icons::glyph(ui, icons::Glyph::Bucket, 16.0);
                let fill = cp::color_box(
                    ui,
                    "sel_fill",
                    vec2(22.0, 14.0),
                    p.fill.as_ref().map(|f| f.color),
                    Footer::Fill { kinds: false },
                );
                match fill {
                    Some(ColorChoice::Color(c)) => fill_choice = Some(Some(cp::to_rgba(c))),
                    Some(ColorChoice::None) => fill_choice = Some(None),
                    _ => {}
                }
                ui.label(if p.fill.is_some() { "Solid" } else { "None" });
            });
            // Texture, its amount and Transparent (Fireworks' fill section).
            let f = p.fill.as_ref();
            let has = f.is_some();
            let (name, amount, transparent) = f.map_or((String::new(), 0, false), |f| {
                (
                    f.texture.as_ref().map(|t| t.name.clone()).unwrap_or_default(),
                    f.texture_amount,
                    f.texture_transparent,
                )
            });
            ui.horizontal(|ui| {
                ui.add_enabled(has, egui::Label::new("Texture:"));
                if let Some(t) = crate::stroke_panel::texture_picker(
                    app,
                    ui,
                    crate::stroke_panel::TextureSlot::Fill,
                    &name,
                    has,
                ) && let Some(d) = app.session.active_doc_mut()
                {
                    d.edit_fill("Fill Texture", false, move |f| f.texture = Some(t.clone()));
                }
                let mut pct = (amount as f64 / 10.0).round();
                let resp = ui.add_enabled(
                    has,
                    DragValue::new(&mut pct)
                        .range(0.0..=100.0)
                        .max_decimals(0)
                        .suffix("%"),
                );
                if let Some(d) = app.session.active_doc_mut() {
                    let v = (pct.round() as u16).min(100) * 10;
                    value_drag(d, &resp, "Fill Texture", |d, live| {
                        d.edit_fill("Fill Texture", live, move |f| f.texture_amount = v);
                    });
                }
                let mut tr = transparent;
                if ui
                    .add_enabled(has, egui::Checkbox::new(&mut tr, "Transparent"))
                    .on_hover_text("The texture makes the fill transparent instead of lighter")
                    .changed()
                    && let Some(d) = app.session.active_doc_mut()
                {
                    d.edit_fill("Fill Texture", false, move |f| f.texture_transparent = tr);
                }
            });
            // Rectangle roundness (Fireworks: rectangle primitives only).
            if let Some(r) = p.rect {
                ui.horizontal(|ui| {
                    ui.label("Rectangle roundness:");
                    let mut pct = (r.roundness * 100.0).round();
                    let resp = ui.add(
                        DragValue::new(&mut pct)
                            .range(0.0..=100.0)
                            .max_decimals(0)
                            .suffix("%"),
                    );
                    if let Some(d) = app.session.active_doc_mut() {
                        value_drag(d, &resp, "Rectangle Roundness", |d, live| {
                            d.set_roundness(pct / 100.0, live);
                        });
                    }
                });
            }
        });
        ui.separator();
    }
    if let Some(d) = app.session.active_doc_mut() {
        if let Some(c) = fill_choice {
            d.set_fill_color(c);
        }
    }
    // Fireworks CS's stroke controls (FirePixel's layout).
    if is_path {
        crate::stroke_panel::show(app, ui, crate::stroke_panel::Target::Selection);
        ui.separator();
    }
    // Opacity / blend / effects column.
    let mut opacity = first.as_ref().map(|e| e.opacity).unwrap_or(100.0);
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            icons::glyph(ui, icons::Glyph::Opacity, 16.0);
            let resp = ui.add(
                DragValue::new(&mut opacity)
                    .range(0.0..=100.0)
                    .max_decimals(1)
                    .suffix("%"),
            );
            if let Some(d) = app.session.active_doc_mut() {
                value_drag(d, &resp, "Opacity", |d, live| d.set_opacity(opacity, live));
            }
            let current = first.as_ref().map(|e| e.blend).unwrap_or_default();
            let mut chosen = None;
            ComboBox::from_id_salt("blend")
                .width(90.0)
                .selected_text(current.label())
                .show_ui(ui, |ui| {
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
        crate::effects_ui::panel(app, ui);
    });
}

fn document(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let Some(d) = app.session.active_doc() else {
        identity(ui, app.session.tool.label(), Some(app.session.tool), |_| {});
        ui.weak("No document open. File > New (Ctrl+N) or File > Open (Ctrl+O), or drop a PNG here.");
        return;
    };
    let canvas = d.doc.canvas;
    let color_math = d.doc.color_math;
    let title = d.doc.title();
    identity(ui, "Document", None, |ui| {
        ui.label(egui::RichText::new(title).size(11.0));
    });
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.label("Canvas:");
            match cp::color_box(ui, "canvas_color", vec2(22.0, 14.0), canvas, Footer::Canvas) {
                Some(ColorChoice::Color(c)) => {
                    if let Some(d) = app.session.active_doc_mut() {
                        d.set_canvas(Some(cp::to_rgba(c)));
                    }
                }
                Some(ColorChoice::None) => {
                    if let Some(d) = app.session.active_doc_mut() {
                        d.set_canvas(None);
                    }
                }
                _ => {}
            }
            if ui.button("Canvas Size...").clicked() {
                app.dialogs.open_canvas_size(app.session.active_doc());
            }
            ui.add_enabled(false, egui::Button::new("Image Size...")).on_disabled_hover_text("Coming soon");
        });
        ui.horizontal(|ui| {
            ui.label("Color math:");
            let mut math = color_math;
            if crate::dialogs::color_math_combo(ui, "doc_color_math", &mut math)
                && let Some(d) = app.session.active_doc_mut() {
                    d.set_color_math(math);
                }
        })
        .response
        .on_hover_text("FireCraft: Fireworks' formulas in double precision (blend modes, Hue/Saturation). Fireworks MX: bit-exact Fireworks results. Fireworks MX itself always renders with its own math.");
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
