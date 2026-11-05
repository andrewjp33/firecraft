//! The colours and font FirePixel's widgets are drawn with: FirePixel's default ("normal") theme
//! values, reduced to what FireCraft's copies of its widgets use.

use std::sync::Arc;

use egui::{Color32, FontId};

const fn g(v: u8) -> Color32 {
    Color32::from_rgb(v, v, v)
}

const fn rgb(r: u8, gg: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, gg, b)
}

/// FirePixel's palette (theme.rs `Palette`), the entries its picker and controls use.
#[derive(Clone, Debug)]
pub struct Palette {
    pub menu_bg: Color32,
    pub menu_border: Color32,
    pub frame_bg: Color32,
    pub line_dark: Color32,
    pub line_light: Color32,
    pub checker_a: Color32,
    pub checker_b: Color32,
    pub text: Color32,
    pub text_disabled: Color32,
    pub field_bg: Color32,
    pub field_border: Color32,
    pub field_text: Color32,
    pub button_bg: Color32,
    pub button_border: Color32,
    pub button_hover_bg: Color32,
    pub button_hover_border: Color32,
    pub tool_selected_bg: Color32,
    pub focus: Color32,
    pub icon_fg: Color32,
    pub icon_light: Color32,
    pub icon_paper: Color32,
    pub icon_disabled: Color32,
    pub icon_red: Color32,
    pub icon_green: Color32,
    pub icon_blue: Color32,
    pub icon_yellow: Color32,
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub colors: Palette,
    /// FirePixel's "glow" look (off in its normal theme).
    pub glow: bool,
}

impl Theme {
    pub fn normal() -> Theme {
        Theme {
            colors: Palette {
                menu_bg: g(249),
                menu_border: g(150),
                frame_bg: g(214),
                line_dark: g(100),
                line_light: g(232),
                checker_a: g(255),
                checker_b: g(204),
                text: g(0),
                text_disabled: g(150),
                field_bg: g(255),
                field_border: rgb(130, 135, 144),
                field_text: g(0),
                button_bg: g(225),
                button_border: g(160),
                button_hover_bg: g(240),
                button_hover_border: g(120),
                tool_selected_bg: g(255),
                focus: rgb(0, 120, 215),
                icon_fg: g(40),
                icon_light: g(150),
                icon_paper: g(255),
                icon_disabled: g(170),
                icon_red: rgb(220, 30, 30),
                icon_green: rgb(40, 150, 40),
                icon_blue: rgb(30, 110, 220),
                icon_yellow: rgb(240, 190, 30),
            },
            glow: false,
        }
    }

    pub fn current(_ctx: &egui::Context) -> Arc<Theme> {
        static T: std::sync::OnceLock<Arc<Theme>> = std::sync::OnceLock::new();
        T.get_or_init(|| Arc::new(Theme::normal())).clone()
    }

    /// FirePixel's UI font (11.5 pt).
    pub fn ui_font(&self) -> FontId {
        FontId::proportional(11.5)
    }

    pub fn font(&self, size: f32) -> FontId {
        FontId::proportional(size)
    }

    /// FireCraft has no bold face loaded: the proportional font.
    pub fn bold(&self, size: f32) -> FontId {
        FontId::proportional(size)
    }
}

pub fn with_alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

/// Vertical gradient (FirePixel widgets.rs).
pub fn vgradient(painter: &egui::Painter, rect: egui::Rect, top: Color32, bottom: Color32) {
    if top == bottom {
        painter.rect_filled(rect, 0.0, top);
        return;
    }
    let mut mesh = egui::epaint::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    painter.add(egui::Shape::mesh(mesh));
}
