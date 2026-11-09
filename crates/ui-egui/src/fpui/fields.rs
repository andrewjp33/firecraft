//! Field and button helpers from FirePixel's `inspector/fields.rs`, `widgets.rs` and
//! `dialogs/mod.rs` (Apache-2.0), without FirePixel's test-audit and accessibility hooks.

use std::hash::Hash;

use egui::{Align2, Id, Rect, Response, Sense, Stroke, StrokeKind, Ui, pos2};

use super::glyphs::{self, Glyph};
use super::theme::{Theme, with_alpha};

/// Keys a focused text field keeps: arrows and Esc.
pub const TEXT_FILTER: egui::EventFilter = egui::EventFilter {
    horizontal_arrows: true,
    vertical_arrows: true,
    tab: false,
    escape: true,
};

pub const VSLIDER_PERCENT_PAD: f32 = 8.0;
pub const VSLIDER_PERCENT_TRACK: f32 = 200.0;

/// The digits of `s` as a number (other characters ignored).
pub fn parse_digits(s: &str) -> Option<f64> {
    let d: String = s.chars().filter(|c| c.is_ascii_digit()).collect();
    if d.is_empty() { None } else { d.parse::<f64>().ok() }
}

/// A label right-aligned in `r`.
pub fn label_right(ui: &Ui, r: Rect, text: &str, enabled: bool) {
    let t = Theme::current(ui.ctx());
    let col = if enabled {
        t.colors.text
    } else {
        t.colors.text_disabled
    };
    ui.painter().text(
        pos2(r.right(), r.center().y),
        Align2::RIGHT_CENTER,
        text,
        t.ui_font(),
        col,
    );
}

/// A toggle button with a glyph (pressed: blue frame, as Fireworks CS).
pub fn toggle(
    ui: &mut Ui,
    id: impl Hash + std::fmt::Debug,
    r: Rect,
    glyph: Glyph,
    on: bool,
    enabled: bool,
    tip: &str,
) -> bool {
    let t = Theme::current(ui.ctx());
    let c = &t.colors;
    let resp = ui.interact(
        r,
        Id::unique(id),
        if enabled { Sense::click() } else { Sense::hover() },
    );
    // Named for assistive technology (and tests that click widgets by name).
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));
    let hot = enabled && resp.hovered();
    if on {
        ui.painter().rect_filled(r, 2.0, c.tool_selected_bg);
        ui.painter().rect_stroke(
            r.shrink(1.0),
            1.0,
            Stroke::new(2.0, with_alpha(c.focus, 60)),
            StrokeKind::Inside,
        );
        ui.painter()
            .rect_stroke(r, 2.0, Stroke::new(1.0, c.focus), StrokeKind::Inside);
    } else {
        ui.painter()
            .rect_filled(r, 2.0, if hot { c.button_hover_bg } else { c.button_bg });
        ui.painter().rect_stroke(
            r,
            2.0,
            Stroke::new(
                1.0,
                if hot {
                    c.button_hover_border
                } else {
                    c.button_border
                },
            ),
            StrokeKind::Inside,
        );
    }
    glyphs::paint(ui.painter(), r, glyph, &t, enabled);
    let resp = if tip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tip)
    };
    enabled && resp.clicked()
}

/// A flat icon button (frame on hover).
pub fn icon_button(
    ui: &mut Ui,
    id: impl Hash + std::fmt::Debug,
    r: Rect,
    glyph: Glyph,
    enabled: bool,
    tip: &str,
) -> Response {
    let t = Theme::current(ui.ctx());
    let c = &t.colors;
    let resp = ui.interact(
        r,
        Id::unique(id),
        if enabled { Sense::click() } else { Sense::hover() },
    );
    // Named for assistive technology (and tests that click widgets by name).
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, tip));
    if enabled && resp.hovered() {
        ui.painter().rect_filled(r, 2.0, c.button_hover_bg);
        ui.painter().rect_stroke(
            r,
            2.0,
            Stroke::new(1.0, c.button_hover_border),
            StrokeKind::Inside,
        );
    }
    glyphs::paint(ui.painter(), r, glyph, &t, enabled);
    if tip.is_empty() {
        resp
    } else {
        resp.on_hover_text(tip)
    }
}

/// A transparency checkerboard in `rect` with cells of `cell` points.
pub fn checker(painter: &egui::Painter, rect: Rect, cell: f32, theme: &Theme) {
    let (a, b) = (theme.colors.checker_a, theme.colors.checker_b);
    painter.rect_filled(rect, 0.0, a);
    let p = painter.with_clip_rect(rect.intersect(painter.clip_rect()));
    let (nx, ny) = (
        (rect.width() / cell).ceil() as i32,
        (rect.height() / cell).ceil() as i32,
    );
    for y in 0..ny {
        for x in 0..nx {
            if (x + y) % 2 == 1 {
                let r = Rect::from_min_size(
                    rect.min + egui::vec2(x as f32 * cell, y as f32 * cell),
                    egui::vec2(cell, cell),
                );
                p.rect_filled(r, 0.0, b);
            }
        }
    }
}

/// A dialog push button (the default one tinted and framed in the focus colour).
pub fn push_button(ui: &mut Ui, rect: Rect, label: &str, default: bool, enabled: bool) -> bool {
    let theme = Theme::current(ui.ctx());
    let mut button = egui::Button::new(label).min_size(rect.size());
    if default && enabled {
        button = button.fill(with_alpha(theme.colors.focus, 30));
    }
    let resp = ui.add_enabled_ui(enabled, |ui| ui.put(rect, button)).inner;
    if default && enabled {
        ui.painter().rect_stroke(
            rect,
            2.0,
            Stroke::new(1.0, theme.colors.focus),
            StrokeKind::Inside,
        );
    }
    enabled && resp.clicked()
}
