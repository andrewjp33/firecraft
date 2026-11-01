//! The Tools panel: Select / Bitmap / Vector / Web / Colors / View sections, two columns.

use egui::{vec2, Color32, Rect, Sense, Stroke, StrokeKind};
use firecraft_engine::Tool;
use firecraft_raster::Rgba;

use crate::{icons, theme, FireCraftApp};

const SECTIONS: &[(&str, &[Tool])] = &[
    ("Select", &[Tool::Pointer, Tool::Subselect, Tool::Scale, Tool::Crop]),
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
    ("Vector", &[Tool::Line, Tool::Pen, Tool::Rectangle, Tool::Text, Tool::Freeform, Tool::Knife]),
    ("Web", &[Tool::Hotspot, Tool::Slice]),
];

/// Tools that share a slot with a flyout alternative (shown when the slot's tool is active).
fn slot_alternative(t: Tool) -> Option<Tool> {
    match t {
        Tool::Marquee => Some(Tool::OvalMarquee),
        Tool::Scale => Some(Tool::NineSlice),
        Tool::Rectangle => Some(Tool::Ellipse),
        _ => None,
    }
}

fn section_title(ui: &mut egui::Ui, title: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 13.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, theme::SECTION);
    ui.painter().rect_stroke(r, 0.0, Stroke::new(1.0, theme::BORDER), StrokeKind::Inside);
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, title, egui::FontId::proportional(10.0), Color32::from_gray(30));
}

fn tool_button(app: &mut FireCraftApp, ui: &mut egui::Ui, slot: Tool) {
    // A slot shows its flyout alternative when that one is the active tool.
    let shown = match slot_alternative(slot) {
        Some(alt) if app.session.tool == alt => alt,
        _ => slot,
    };
    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 23.0), Sense::click());
    let active = app.session.tool == shown;
    if active {
        ui.painter().rect_filled(r, 1.0, Color32::WHITE);
        ui.painter().rect_stroke(r, 1.0, Stroke::new(1.0, Color32::from_gray(110)), StrokeKind::Inside);
    } else if resp.hovered() {
        ui.painter().rect_stroke(r, 1.0, Stroke::new(1.0, Color32::from_gray(170)), StrokeKind::Inside);
    }
    icons::paint(ui.painter(), r, shown, shown.implemented());
    if slot_alternative(slot).is_some() {
        // Flyout marker.
        let c = r.right_bottom() - vec2(3.0, 3.0);
        ui.painter().add(egui::Shape::convex_polygon(vec![c, c + vec2(-3.0, 0.0), c + vec2(0.0, -3.0)], Color32::BLACK, Stroke::NONE));
    }
    let mut tip = shown.label().to_owned();
    if let Some(k) = shown.shortcut() {
        tip += &format!(" ({k})");
    }
    if !shown.implemented() {
        tip += " (not yet implemented)";
    }
    if let Some(alt) = slot_alternative(slot) {
        tip += &format!("\nRight-click: {}", if shown == slot { alt.label() } else { slot.label() });
    }
    let resp = resp.on_hover_text(tip);
    if resp.clicked() {
        app.session.tool = shown;
    }
    if resp.secondary_clicked() {
        if let Some(alt) = slot_alternative(slot) {
            app.session.tool = if shown == slot { alt } else { slot };
        }
    }
}

pub fn show(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = vec2(2.0, 1.0);
    // Panel title strip.
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 15.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, theme::HEADER);
    ui.painter().text(r.left_center() + vec2(12.0, 0.0), egui::Align2::LEFT_CENTER, "Tools", egui::FontId::proportional(11.0), Color32::WHITE);
    for (title, tools) in SECTIONS {
        section_title(ui, title);
        for pair in tools.chunks(2) {
            ui.horizontal(|ui| {
                ui.add_space(3.0);
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
        ui.add_space(3.0);
        tool_button(app, ui, Tool::Hand);
        tool_button(app, ui, Tool::Zoom);
    });
}

pub fn swatch(ui: &mut egui::Ui, color: Option<Rgba>, size: egui::Vec2) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    paint_swatch(ui.painter(), r, color);
    resp
}

pub fn paint_swatch(p: &egui::Painter, r: Rect, color: Option<Rgba>) {
    match color {
        Some(c) => {
            // Checkerboard under translucent colours.
            if c.a < 255 {
                p.rect_filled(r, 0.0, Color32::WHITE);
                let h = r.height() / 2.0;
                p.rect_filled(Rect::from_min_size(r.min, vec2(r.width() / 2.0, h)), 0.0, Color32::from_gray(200));
                p.rect_filled(Rect::from_min_size(r.center(), vec2(r.width() / 2.0, h)), 0.0, Color32::from_gray(200));
            }
            p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a));
        }
        None => {
            p.rect_filled(r, 0.0, Color32::WHITE);
            p.line_segment([r.left_bottom(), r.right_top()], Stroke::new(2.0, Color32::RED));
        }
    }
    p.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_gray(80)), StrokeKind::Inside);
}

fn colors(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    for (label, is_stroke) in [("Stroke", true), ("Fill", false)] {
        ui.horizontal(|ui| {
            ui.add_space(4.0);
            icons::glyph(ui, if is_stroke { icons::Glyph::Pencil } else { icons::Glyph::Bucket }, 16.0);
            let cur = if is_stroke { app.session.options.stroke } else { app.session.options.fill };
            let resp = swatch(ui, cur, vec2(24.0, 14.0)).on_hover_text(format!("{label} color (click to edit)"));
            if resp.clicked() {
                app.dialogs.color_target = Some(is_stroke);
            }
        });
    }
    ui.horizontal(|ui| {
        ui.add_space(4.0);
        if icons::glyph_button(ui, icons::Glyph::Defaults, 16.0, true).on_hover_text("Default stroke/fill colors").clicked() {
            app.session.options.stroke = Some(Rgba::BLACK);
            app.session.options.fill = Some(Rgba::new(0x99, 0x99, 0x99, 255));
        }
        if icons::glyph_button(ui, icons::Glyph::NoColor, 16.0, true).on_hover_text("No fill").clicked() {
            app.session.options.fill = None;
        }
        if icons::glyph_button(ui, icons::Glyph::Swap, 16.0, true).on_hover_text("Swap stroke/fill colors").clicked() {
            let o = &mut app.session.options;
            std::mem::swap(&mut o.stroke, &mut o.fill);
        }
    });
}
