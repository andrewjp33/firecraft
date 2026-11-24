//! The Tools panel: Select / Bitmap / Vector / Web / Colors / View sections, two columns.

use egui::{Color32, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use firecraft_engine::Tool;
use firecraft_raster::Rgba;

use crate::{FireCraftApp, icons, theme};

const SECTIONS: &[(&str, &[Tool])] = &[
    (
        "Select",
        &[Tool::Pointer, Tool::Subselect, Tool::Scale, Tool::Crop],
    ),
    (
        "Bitmap",
        &[
            Tool::Marquee,
            Tool::Lasso,
            Tool::MagicWand,
            Tool::Brush,
            Tool::Pencil,
            Tool::Eraser,
            Tool::Blur,
            Tool::RubberStamp,
            Tool::Eyedropper,
            Tool::PaintBucket,
        ],
    ),
    (
        "Vector",
        &[
            Tool::Line,
            Tool::Pen,
            Tool::Rectangle,
            Tool::Text,
            Tool::Freeform,
            Tool::Knife,
        ],
    ),
    ("Web", &[Tool::Hotspot, Tool::Slice]),
];

/// Tools that share a slot with a flyout alternative (shown when the slot's tool is active).
fn slot_alternatives(t: Tool) -> &'static [Tool] {
    match t {
        Tool::Pointer => &[Tool::SelectBehind],
        Tool::Marquee => &[Tool::OvalMarquee],
        Tool::Scale => &[Tool::Skew, Tool::Distort, Tool::NineSlice],
        Tool::Rectangle => &[Tool::Ellipse],
        Tool::PaintBucket => &[Tool::Gradient],
        _ => &[],
    }
}

fn section_title(ui: &mut egui::Ui, title: &str) {
    let p = theme::pal();
    if p.modern {
        // A small caption on the sidebar, with room above it.
        ui.add_space(4.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 13.0), Sense::hover());
        ui.painter().text(
            r.left_center() + vec2(7.0, 0.0),
            egui::Align2::LEFT_CENTER,
            title,
            egui::FontId::proportional(10.0),
            p.section_text,
        );
        return;
    }
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 13.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, p.section);
    ui.painter()
        .rect_stroke(r, 0.0, Stroke::new(1.0, p.border), StrokeKind::Inside);
    ui.painter().text(
        r.center(),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(10.0),
        p.section_text,
    );
}

fn tool_button(app: &mut FireCraftApp, ui: &mut egui::Ui, slot: Tool) {
    // A slot shows its flyout alternative when that one is the active tool.
    let alts = slot_alternatives(slot);
    let shown = if alts.contains(&app.session.tool) {
        app.session.tool
    } else {
        slot
    };
    // Right-click cycles the slot: the tool after the shown one.
    let group: Vec<Tool> = std::iter::once(slot).chain(alts.iter().copied()).collect();
    let next = group
        .iter()
        .position(|t| *t == shown)
        .and_then(|i| group.get((i + 1) % group.len()))
        .copied();
    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 23.0), Sense::click());
    let active = app.session.tool == shown;
    let p = theme::pal();
    let ink = if active {
        p.tool_active_ink
    } else if shown.implemented() {
        p.tools_ink
    } else {
        p.tools_ink_disabled
    };
    if p.modern {
        // Rounded tiles shaded left to right with a light outline: faint on the sidebar, light
        // and raised for the active tool.
        let tile = r.shrink(0.5);
        let radius = 8.0;
        let (from, to, border) = if active {
            let shadow = egui::Shadow {
                offset: [0, 2],
                blur: 8,
                spread: 0,
                color: Color32::from_black_alpha(70),
            };
            ui.painter().add(shadow.as_shape(tile, radius));
            (p.tool_active_fill, p.tool_active_fill_end, p.tool_active_border)
        } else if resp.hovered() {
            (p.tool_hover, p.tool_fill, p.tool_border)
        } else {
            (p.tool_fill, p.tool_fill_end, p.tool_border)
        };
        ui.painter().add(theme::rounded_gradient(tile, radius, from, to, true));
        ui.painter().rect_stroke(tile, radius, Stroke::new(1.0, border), StrokeKind::Inside);
    } else if active {
        ui.painter().rect_filled(r, 1.0, p.tool_active_fill);
        ui.painter().rect_stroke(
            r,
            1.0,
            Stroke::new(1.0, p.tool_active_border),
            StrokeKind::Inside,
        );
    } else if resp.hovered() {
        ui.painter().rect_stroke(
            r,
            1.0,
            Stroke::new(1.0, p.tool_hover),
            StrokeKind::Inside,
        );
    }
    icons::paint_ink(ui.painter(), r, shown, ink);
    if p.modern && active && shown == Tool::Pointer {
        // A soft light edge around the solid arrow, as in design_edit2.png.
        let edge = theme::lerp_color(ink, p.tool_active_fill_end, 0.6);
        ui.painter()
            .add(egui::Shape::closed_line(icons::pointer_points(r), Stroke::new(1.0, edge)));
    }
    if !alts.is_empty() {
        // Flyout marker.
        let c = r.right_bottom() - if p.modern { vec2(4.0, 4.0) } else { vec2(3.0, 3.0) };
        ui.painter().add(egui::Shape::convex_polygon(
            vec![c, c + vec2(-3.0, 0.0), c + vec2(0.0, -3.0)],
            if p.dark || p.modern { ink } else { Color32::BLACK },
            Stroke::NONE,
        ));
    }
    let mut tip = shown.label().to_owned();
    if let Some(k) = shown.shortcut() {
        tip += &format!(" ({k})");
    }
    if !shown.implemented() {
        tip += " (not yet implemented)";
    }
    if let (false, Some(n)) = (alts.is_empty(), next) {
        tip += &format!("\nPress and hold for more tools (right-click: {})", n.label());
    }
    let flyout_id = egui::Id::unique("tool_flyout");
    let open: Option<Tool> = ui.ctx().data(|d| d.get_temp(flyout_id)).flatten();
    let resp = if open.is_some() {
        resp
    } else {
        resp.on_hover_text(tip)
    };
    if !alts.is_empty() {
        // Fireworks: press and hold a tool with the small arrow (or click the arrow) for a menu of
        // the tools sharing its slot.
        let arrow = Rect::from_min_max(r.right_bottom() - vec2(9.0, 9.0), r.right_bottom());
        let held = resp.is_pointer_button_down_on()
            && ui
                .ctx()
                .input(|i| i.pointer.press_start_time().is_some_and(|t| i.time - t >= 0.35));
        let on_arrow = resp.clicked()
            && resp
                .interact_pointer_pos()
                .is_some_and(|q| arrow.expand(2.0).contains(q));
        if resp.is_pointer_button_down_on() {
            ui.ctx().request_repaint();
        }
        if (held || on_arrow) && open != Some(slot) {
            ui.ctx().data_mut(|d| d.insert_temp(flyout_id, Some(slot)));
        }
        if ui.ctx().data(|d| d.get_temp::<Option<Tool>>(flyout_id)).flatten() == Some(slot) {
            flyout(app, ui.ctx(), slot, &group, r, flyout_id);
        }
        if on_arrow {
            return;
        }
    }
    if resp.clicked() && open.is_none() {
        app.session.tool = shown;
    }
    if resp.secondary_clicked()
        && !alts.is_empty()
        && let Some(n) = next
    {
        app.session.tool = n;
    }
}

/// The slot's tools as a menu beside the button; picking one selects it, clicking elsewhere
/// closes the menu.
fn flyout(
    app: &mut FireCraftApp,
    ctx: &egui::Context,
    slot: Tool,
    group: &[Tool],
    button: Rect,
    id: egui::Id,
) {
    let area = egui::Area::new(id.with(slot))
        .order(egui::Order::Foreground)
        .fixed_pos(button.right_top() + vec2(2.0, 0.0));
    let shown = area.show(ctx, |ui| {
        egui::Frame::popup(ui.style()).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(2.0, 1.0);
            for &t in group {
                let label = match t.shortcut() {
                    Some(k) => format!("{}  ({k})", t.label()),
                    None => t.label().to_owned(),
                };
                let (row, resp) = ui.allocate_exact_size(vec2(190.0, 22.0), Sense::click());
                let p = theme::pal();
                let current = app.session.tool == t;
                let icon = Rect::from_min_size(row.min, vec2(22.0, 22.0));
                let col = if p.modern {
                    // As the dropdown lists: dark grey for the current tool, a light grey
                    // under the pointer.
                    if current {
                        ui.painter().rect_filled(row, p.r_widget, p.popup_selected);
                        icons::paint_ink(ui.painter(), icon, t, p.popup_selected_text);
                        p.popup_selected_text
                    } else {
                        if resp.hovered() {
                            ui.painter().rect_filled(row, p.r_widget, p.popup_hover);
                        }
                        icons::paint(ui.painter(), icon, t, t.implemented());
                        p.text
                    }
                } else {
                    if resp.hovered() || current {
                        let fill = if resp.hovered() { theme::select() } else { p.button_active };
                        ui.painter().rect_filled(row, p.r_widget, fill);
                    }
                    icons::paint(ui.painter(), icon, t, t.implemented());
                    if resp.hovered() { p.select_text } else { p.text }
                };
                ui.painter().text(
                    row.left_center() + vec2(28.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(12.0),
                    col,
                );
                if resp.clicked() {
                    app.session.tool = t;
                    ctx.data_mut(|d| d.insert_temp::<Option<Tool>>(id, None));
                }
            }
        });
    });
    // A press outside the menu (other than the release of the opening press) closes it.
    let outside = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|p| !shown.response.rect.contains(p) && !button.contains(p))
    });
    if outside {
        ctx.data_mut(|d| d.insert_temp::<Option<Tool>>(id, None));
    }
}

pub fn show(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = vec2(2.0, 1.0);
    let p = theme::pal();
    if p.modern {
        // The sidebar's glyphs and labels in its light ink, a little more air between tiles.
        ui.visuals_mut().override_text_color = Some(p.tools_ink);
        ui.spacing_mut().item_spacing = vec2(2.0, 3.0);
        ui.add_space(6.0);
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 15.0), Sense::hover());
        // Centred over the two columns of tool tiles (5 px in, 26 + 2 + 26 wide).
        ui.painter().text(
            pos2(r.left() + 5.0 + 27.0, r.center().y),
            egui::Align2::CENTER_CENTER,
            "Tools",
            egui::FontId::proportional(12.0),
            p.tools_ink,
        );
    } else {
        // Panel title strip.
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 15.0), Sense::hover());
        ui.painter().rect_filled(r, 0.0, p.header);
        ui.painter().text(
            r.left_center() + vec2(12.0, 0.0),
            egui::Align2::LEFT_CENTER,
            "Tools",
            egui::FontId::proportional(11.0),
            p.header_text,
        );
    }
    for (title, tools) in SECTIONS {
        section_title(ui, title);
        for pair in tools.chunks(2) {
            ui.horizontal(|ui| {
                ui.add_space(if theme::pal().modern { 5.0 } else { 3.0 });
                for &t in pair {
                    tool_button(app, ui, t);
                }
            });
        }
    }
    section_title(ui, "Colors");
    colors(app, ui);
    section_title(ui, "View");
    ui.horizontal(|ui| {
        ui.add_space(if theme::pal().modern { 5.0 } else { 3.0 });
        tool_button(app, ui, Tool::Hand);
        tool_button(app, ui, Tool::Zoom);
    });
}

pub fn paint_swatch(p: &egui::Painter, r: Rect, color: Option<Rgba>) {
    match color {
        Some(c) => {
            // Checkerboard under translucent colours.
            if c.a < 255 {
                p.rect_filled(r, 0.0, Color32::WHITE);
                let h = r.height() / 2.0;
                p.rect_filled(
                    Rect::from_min_size(r.min, vec2(r.width() / 2.0, h)),
                    0.0,
                    Color32::from_gray(200),
                );
                p.rect_filled(
                    Rect::from_min_size(r.center(), vec2(r.width() / 2.0, h)),
                    0.0,
                    Color32::from_gray(200),
                );
            }
            p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a));
        }
        None => {
            p.rect_filled(r, 0.0, Color32::WHITE);
            p.line_segment([r.left_bottom(), r.right_top()], Stroke::new(2.0, Color32::RED));
        }
    }
    p.rect_stroke(
        r,
        0.0,
        Stroke::new(1.0, Color32::from_gray(80)),
        StrokeKind::Inside,
    );
}

fn colors(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    for (label, is_stroke) in [("Stroke", true), ("Fill", false)] {
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            icons::glyph(
                ui,
                if is_stroke {
                    icons::Glyph::Pencil
                } else {
                    icons::Glyph::Bucket
                },
                16.0,
            );
            if is_stroke {
                crate::fill_ui::tools_stroke_box(app, ui);
            } else {
                crate::fill_ui::tools_fill_box(app, ui);
            }
            let _ = label;
        });
    }
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        if icons::glyph_button(ui, icons::Glyph::Defaults, 16.0, true)
            .on_hover_text("Default stroke/fill colors")
            .clicked()
        {
            let fill = Some(Rgba::new(0x99, 0x99, 0x99, 255));
            crate::fill_ui::set_tool_colors(app, "Set Default Colors", Some(Rgba::BLACK), fill);
        }
        if icons::glyph_button(ui, icons::Glyph::NoColor, 16.0, true)
            .on_hover_text("No fill")
            .clicked()
        {
            crate::fill_ui::tools_no_fill(app);
        }
        if icons::glyph_button(ui, icons::Glyph::Swap, 16.0, true)
            .on_hover_text("Swap stroke/fill colors")
            .clicked()
        {
            let (stroke, fill) = (app.session.options.fill, app.session.options.stroke);
            crate::fill_ui::set_tool_colors(app, "Swap Colors", stroke, fill);
        }
    });
}
