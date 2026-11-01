//! Fireworks MX's look: light panels, grey panel-group headers, mid-grey workspace.

use egui::{Color32, CornerRadius, Frame, Margin, Stroke};

pub const PANEL: Color32 = Color32::from_rgb(0xF2, 0xF2, 0xF2);
pub const WORKSPACE: Color32 = Color32::from_rgb(0x80, 0x80, 0x80);
pub const HEADER: Color32 = Color32::from_rgb(0xA6, 0xA6, 0xA6);
pub const HEADER_TEXT: Color32 = Color32::WHITE;
pub const SECTION: Color32 = Color32::from_rgb(0xFF, 0xFF, 0xFF);
pub const BORDER: Color32 = Color32::from_rgb(0x9A, 0x9A, 0x9A);
pub const SELECT_BLUE: Color32 = Color32::from_rgb(0x33, 0x99, 0xFF);
/// Object selection outline / handles (Fireworks' light blue).
pub const HANDLE_BLUE: Color32 = Color32::from_rgb(0x00, 0xA8, 0xFF);

pub fn apply(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::light());
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(4.0, 3.0);
        s.spacing.button_padding = egui::vec2(4.0, 1.0);
        s.spacing.interact_size.y = 18.0;
        s.visuals.panel_fill = PANEL;
        s.visuals.window_fill = PANEL;
        s.visuals.extreme_bg_color = Color32::WHITE;
        s.visuals.selection.bg_fill = SELECT_BLUE;
        s.visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
        s.visuals.window_corner_radius = CornerRadius::same(0);
        s.visuals.menu_corner_radius = CornerRadius::same(0);
        for w in [
            &mut s.visuals.widgets.noninteractive,
            &mut s.visuals.widgets.inactive,
            &mut s.visuals.widgets.hovered,
            &mut s.visuals.widgets.active,
            &mut s.visuals.widgets.open,
        ] {
            w.corner_radius = CornerRadius::same(1);
        }
        s.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, BORDER);
        s.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(0xFA, 0xFA, 0xFA);
        for (_, font) in s.text_styles.iter_mut() {
            font.size = (font.size - 2.0).max(10.0);
        }
    });
}

pub fn menu_frame() -> Frame {
    Frame::new().fill(Color32::WHITE).inner_margin(Margin::symmetric(4, 2))
}

pub fn panel_frame() -> Frame {
    Frame::new().fill(PANEL).inner_margin(Margin::same(2)).stroke(Stroke::new(1.0, BORDER))
}

pub fn dock_frame() -> Frame {
    Frame::new().fill(Color32::from_rgb(0xA6, 0xA6, 0xA6)).inner_margin(Margin::same(2))
}

pub fn workspace_frame() -> Frame {
    Frame::new().fill(WORKSPACE)
}

/// A panel-group title bar like Fireworks' ("> Layers") that toggles `open`.
pub fn group_header(ui: &mut egui::Ui, title: &str, open: &mut bool) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 18.0), egui::Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 0.0, HEADER);
    // Gripper dots on the left, as in Fireworks.
    for i in 0..3 {
        p.circle_filled(egui::pos2(rect.left() + 4.0, rect.top() + 5.0 + i as f32 * 4.0), 0.8, Color32::from_gray(90));
    }
    let g = if *open { crate::icons::Glyph::TriangleDown } else { crate::icons::Glyph::TriangleRight };
    crate::icons::paint_glyph(p, egui::Rect::from_center_size(egui::pos2(rect.left() + 15.0, rect.center().y), egui::vec2(9.0, 9.0)), g, Color32::from_gray(30));
    p.text(egui::pos2(rect.left() + 24.0, rect.center().y), egui::Align2::LEFT_CENTER, title, egui::FontId::proportional(12.0), HEADER_TEXT);
    if resp.clicked() {
        *open = !*open;
    }
}
