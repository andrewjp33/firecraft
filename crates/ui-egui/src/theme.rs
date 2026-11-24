//! The interface's look. Four themes:
//!
//! - **Classic**: Fireworks MX's look: light panels, grey panel-group headers, mid-grey workspace.
//! - **Dark**: the same layout and shapes in dark greys.
//! - **Modern**: soft greys, gradients, rounded cards and pill buttons, a dark tool sidebar
//!   and an orange accent.
//! - **Modern Dark**: Modern's shapes in dark greys.
//!
//! Colours that belong to the document (the canvas, checkerboards, selection handles, previews
//! of colours and strokes) stay the same in every theme; only the interface around them changes.

use std::sync::atomic::{AtomicU8, Ordering};

use egui::{Color32, CornerRadius, Frame, Margin, Rect, Shadow, Stroke, pos2};

/// Object selection outline / handles (Fireworks' light blue), in every theme.
pub const HANDLE_BLUE: Color32 = Color32::from_rgb(0x00, 0xA8, 0xFF);
/// Path outlines and points on the canvas (Fireworks' selection blue), in every theme.
pub const PATH_BLUE: Color32 = Color32::from_rgb(0x33, 0x99, 0xFF);

/// Edit > Preferences > Theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ThemeKind {
    #[default]
    Classic,
    Dark,
    Modern,
    ModernDark,
}

impl ThemeKind {
    pub const ALL: [ThemeKind; 4] = [Self::Classic, Self::Dark, Self::Modern, Self::ModernDark];

    pub fn label(self) -> &'static str {
        match self {
            Self::Classic => "Classic",
            Self::Dark => "Classic Dark",
            Self::Modern => "Modern",
            Self::ModernDark => "Modern Dark",
        }
    }

    /// The preferences file's value.
    pub fn key(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Dark => "dark",
            Self::Modern => "modern",
            Self::ModernDark => "modern_dark",
        }
    }

    pub fn from_key(k: &str) -> Self {
        Self::ALL.into_iter().find(|t| t.key() == k).unwrap_or_default()
    }

    pub fn palette(self) -> &'static Palette {
        match self {
            Self::Classic => &CLASSIC,
            Self::Dark => &DARK,
            Self::Modern => &MODERN,
            Self::ModernDark => &MODERN_DARK,
        }
    }
}

/// Every colour (and corner) the interface is drawn with.
#[derive(Debug)]
pub struct Palette {
    pub dark: bool,
    /// Rounded cards, gradients and a tool sidebar (the Modern themes).
    pub modern: bool,
    /// Panel background (Classic: the panels; Modern: the cards).
    pub panel: Color32,
    /// Modern cards are drawn with a soft vertical gradient from `panel` to this.
    pub panel_bottom: Color32,
    /// Behind the document windows.
    pub workspace: Color32,
    /// Modern: the window background behind the cards, top to bottom.
    pub backdrop_top: Color32,
    pub backdrop_bottom: Color32,
    /// Panel-group title bars.
    pub header: Color32,
    pub header_text: Color32,
    pub header_ink: Color32,
    /// The Tools panel's section titles (Classic: white boxes).
    pub section: Color32,
    pub section_text: Color32,
    pub border: Color32,
    /// Selected toggles and modes (Classic: Fireworks' blue; Modern: orange).
    pub select: Color32,
    pub select_text: Color32,
    /// The current row of a list (layer, frame, history step, symbol). Classic: the same blue
    /// as `select`; Modern: a quieter grey, keeping orange for modes.
    pub highlight: Color32,
    pub highlight_text: Color32,
    pub text: Color32,
    pub text_weak: Color32,
    /// Tool icons on panels.
    pub ink: Color32,
    pub ink_disabled: Color32,
    /// Small glyphs (eye, lock, folder, frame buttons).
    pub glyph: Color32,
    pub glyph_disabled: Color32,
    /// Text fields and lists.
    pub field: Color32,
    pub button: Color32,
    pub button_hover: Color32,
    pub button_active: Color32,
    pub button_border: Color32,
    pub menu: Color32,
    /// Between the docked panel groups.
    pub dock: Color32,
    /// Around the canvas in a document window (Fireworks' pasteboard).
    pub pasteboard: Color32,
    pub title_active: Color32,
    pub title_inactive: Color32,
    pub title_text: Color32,
    pub title_text_inactive: Color32,
    pub tab_inactive: Color32,
    /// The Tools panel, top to bottom.
    pub tools_top: Color32,
    pub tools_bottom: Color32,
    pub tools_ink: Color32,
    pub tools_ink_disabled: Color32,
    /// Modern: a tool button's rounded tile, a left-to-right gradient with an outline.
    pub tool_fill: Color32,
    pub tool_fill_end: Color32,
    pub tool_border: Color32,
    pub tool_hover: Color32,
    pub tool_active_fill: Color32,
    pub tool_active_fill_end: Color32,
    pub tool_active_ink: Color32,
    pub tool_active_border: Color32,
    pub error: Color32,
    /// Corner radii: cards and document windows, widgets (buttons, fields), menus.
    pub r_card: u8,
    pub r_widget: u8,
    pub r_menu: u8,
}

const fn g(v: u8) -> Color32 {
    Color32::from_rgb(v, v, v)
}

const fn rgb(r: u8, gg: u8, b: u8) -> Color32 {
    Color32::from_rgb(r, gg, b)
}

const fn white_a(a: u8) -> Color32 {
    Color32::from_rgba_premultiplied(a, a, a, a)
}

const CLASSIC: Palette = Palette {
    dark: false,
    modern: false,
    panel: g(0xF2),
    panel_bottom: g(0xF2),
    workspace: g(0x80),
    backdrop_top: g(0xA6),
    backdrop_bottom: g(0xA6),
    header: g(0xA6),
    header_text: Color32::WHITE,
    header_ink: g(30),
    section: Color32::WHITE,
    section_text: g(30),
    border: g(0x9A),
    select: rgb(0x33, 0x99, 0xFF),
    select_text: Color32::WHITE,
    highlight: rgb(0x33, 0x99, 0xFF),
    highlight_text: Color32::WHITE,
    text: g(30),
    text_weak: g(110),
    ink: g(20),
    ink_disabled: g(150),
    glyph: g(30),
    glyph_disabled: g(160),
    field: Color32::WHITE,
    button: g(0xFA),
    button_hover: g(0xFA),
    button_active: g(0xE6),
    button_border: g(0x9A),
    menu: Color32::WHITE,
    dock: g(0xA6),
    pasteboard: g(0xE0),
    title_active: g(0xD6),
    title_inactive: g(0xE6),
    title_text: g(40),
    title_text_inactive: g(110),
    tab_inactive: g(190),
    tools_top: g(0xF2),
    tools_bottom: g(0xF2),
    tools_ink: g(20),
    tools_ink_disabled: g(150),
    tool_fill: Color32::TRANSPARENT,
    tool_fill_end: Color32::TRANSPARENT,
    tool_border: Color32::TRANSPARENT,
    tool_hover: g(170),
    tool_active_fill: Color32::WHITE,
    tool_active_fill_end: Color32::WHITE,
    tool_active_ink: g(20),
    tool_active_border: g(110),
    error: rgb(180, 0, 0),
    r_card: 0,
    r_widget: 1,
    r_menu: 0,
};

const DARK: Palette = Palette {
    dark: true,
    modern: false,
    panel: g(0x38),
    panel_bottom: g(0x38),
    workspace: g(0x1E),
    backdrop_top: g(0x26),
    backdrop_bottom: g(0x26),
    header: g(0x4E),
    header_text: g(0xEC),
    header_ink: g(0xD0),
    section: g(0x2C),
    section_text: g(0xC8),
    border: g(0x1C),
    select: rgb(0x2D, 0x7F, 0xE0),
    select_text: Color32::WHITE,
    highlight: rgb(0x2D, 0x7F, 0xE0),
    highlight_text: Color32::WHITE,
    text: g(0xDC),
    text_weak: g(0x9A),
    ink: g(0xE2),
    ink_disabled: g(0x70),
    glyph: g(0xD8),
    glyph_disabled: g(0x6A),
    field: g(0x24),
    button: g(0x4A),
    button_hover: g(0x56),
    button_active: g(0x60),
    button_border: g(0x22),
    menu: g(0x2E),
    dock: g(0x26),
    pasteboard: g(0x2A),
    title_active: g(0x4A),
    title_inactive: g(0x38),
    title_text: g(0xEA),
    title_text_inactive: g(0x9A),
    tab_inactive: g(0x2A),
    tools_top: g(0x38),
    tools_bottom: g(0x38),
    tools_ink: g(0xE2),
    tools_ink_disabled: g(0x70),
    tool_fill: Color32::TRANSPARENT,
    tool_fill_end: Color32::TRANSPARENT,
    tool_border: Color32::TRANSPARENT,
    tool_hover: g(0x6A),
    tool_active_fill: g(0x5C),
    tool_active_fill_end: g(0x5C),
    tool_active_ink: Color32::WHITE,
    tool_active_border: g(0x80),
    error: rgb(0xFF, 0x6B, 0x6B),
    r_card: 0,
    r_widget: 1,
    r_menu: 0,
};

/// The design inspiration's orange.
const ORANGE: Color32 = rgb(0xDB, 0x67, 0x45);

const MODERN: Palette = Palette {
    dark: false,
    modern: true,
    panel: g(0xEB),
    panel_bottom: g(0xF8),
    workspace: g(0xC4),
    backdrop_top: g(0xD5),
    backdrop_bottom: g(0x8E),
    header: g(0xEB),
    header_text: g(0x16),
    header_ink: g(0x40),
    section: Color32::TRANSPARENT,
    section_text: g(0xC0),
    border: g(0xD6),
    select: ORANGE,
    select_text: Color32::WHITE,
    highlight: g(0xDD),
    highlight_text: g(0x14),
    text: g(0x1E),
    text_weak: g(0x80),
    ink: g(0x22),
    ink_disabled: g(0xA8),
    glyph: g(0x30),
    glyph_disabled: g(0xB0),
    field: Color32::WHITE,
    button: Color32::WHITE,
    button_hover: g(0xFA),
    button_active: g(0xE4),
    button_border: g(0xDC),
    menu: Color32::TRANSPARENT,
    dock: Color32::TRANSPARENT,
    pasteboard: g(0xE8),
    title_active: g(0xF6),
    title_inactive: g(0xEB),
    title_text: g(0x1E),
    title_text_inactive: g(0x8A),
    tab_inactive: g(0xD8),
    tools_top: g(0x6B),
    tools_bottom: g(0x3F),
    tools_ink: g(0xF2),
    tools_ink_disabled: g(0x8C),
    // design_edit2.png: over the sidebar's #686868, #7E fading to #6E, outlined #7F.
    tool_fill: white_a(0x25),
    tool_fill_end: white_a(0x0A),
    tool_border: white_a(0x27),
    tool_hover: white_a(0x40),
    tool_active_fill: g(0xD8),
    tool_active_fill_end: g(0xFC),
    tool_active_ink: g(0x67),
    tool_active_border: g(0xF9),
    error: rgb(0xC0, 0x30, 0x20),
    r_card: 14,
    r_widget: 8,
    r_menu: 10,
};

const ORANGE_DARK: Color32 = rgb(0xF0, 0x6E, 0x48);

const MODERN_DARK: Palette = Palette {
    dark: true,
    modern: true,
    panel: g(0x2C),
    panel_bottom: g(0x24),
    workspace: g(0x18),
    backdrop_top: g(0x26),
    backdrop_bottom: g(0x0E),
    header: g(0x2C),
    header_text: g(0xF0),
    header_ink: g(0xC8),
    section: Color32::TRANSPARENT,
    section_text: g(0x9C),
    border: g(0x3C),
    select: ORANGE_DARK,
    select_text: Color32::WHITE,
    highlight: g(0x4A),
    highlight_text: g(0xF2),
    text: g(0xE8),
    text_weak: g(0x90),
    ink: g(0xE8),
    ink_disabled: g(0x6A),
    glyph: g(0xDC),
    glyph_disabled: g(0x66),
    field: g(0x1A),
    button: g(0x3A),
    button_hover: g(0x44),
    button_active: g(0x50),
    button_border: g(0x46),
    menu: Color32::TRANSPARENT,
    dock: Color32::TRANSPARENT,
    pasteboard: g(0x1E),
    title_active: g(0x36),
    title_inactive: g(0x2C),
    title_text: g(0xF0),
    title_text_inactive: g(0x8A),
    tab_inactive: g(0x22),
    tools_top: g(0x4A),
    tools_bottom: g(0x22),
    tools_ink: g(0xEE),
    tools_ink_disabled: g(0x78),
    tool_fill: white_a(0x1C),
    tool_fill_end: white_a(0x08),
    tool_border: white_a(0x20),
    tool_hover: white_a(0x34),
    tool_active_fill: g(0xB0),
    tool_active_fill_end: g(0xD4),
    tool_active_ink: g(0x3A),
    tool_active_border: g(0xDC),
    error: rgb(0xFF, 0x7A, 0x66),
    r_card: 14,
    r_widget: 8,
    r_menu: 10,
};

static CURRENT: AtomicU8 = AtomicU8::new(0);

/// The theme in use.
pub fn kind() -> ThemeKind {
    ThemeKind::ALL[CURRENT.load(Ordering::Relaxed) as usize % ThemeKind::ALL.len()]
}

/// The colours of the theme in use.
pub fn pal() -> &'static Palette {
    kind().palette()
}

/// Panel background.
pub fn panel() -> Color32 {
    pal().panel
}

pub fn border() -> Color32 {
    pal().border
}

pub fn select() -> Color32 {
    pal().select
}

/// Switch themes: colours, corners and egui's widget style.
pub fn apply(ctx: &egui::Context, kind: ThemeKind) {
    CURRENT.store(ThemeKind::ALL.iter().position(|k| *k == kind).unwrap_or(0) as u8, Ordering::Relaxed);
    let p = kind.palette();
    ctx.set_visuals(if p.dark {
        egui::Visuals::dark()
    } else {
        egui::Visuals::light()
    });
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(4.0, 3.0);
        s.spacing.button_padding = egui::vec2(if p.modern { 8.0 } else { 4.0 }, 1.0);
        s.spacing.interact_size.y = 18.0;
        let v = &mut s.visuals;
        v.panel_fill = p.panel;
        v.window_fill = p.panel;
        v.extreme_bg_color = p.field;
        v.selection.bg_fill = p.select;
        v.selection.stroke = Stroke::new(1.0, p.select_text);
        v.window_corner_radius = CornerRadius::same(p.r_card);
        v.menu_corner_radius = CornerRadius::same(p.r_menu);
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.corner_radius = CornerRadius::same(p.r_widget);
        }
        v.widgets.inactive.bg_stroke = Stroke::new(1.0, p.button_border);
        v.widgets.inactive.weak_bg_fill = p.button;
        if p.dark || p.modern {
            v.window_stroke = Stroke::new(1.0, p.border);
            v.widgets.noninteractive.bg_fill = p.panel;
            v.widgets.noninteractive.weak_bg_fill = p.panel;
            v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
            v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
            v.widgets.inactive.bg_fill = if p.dark { p.field } else { p.button_active };
            v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
            v.widgets.hovered.weak_bg_fill = p.button_hover;
            v.widgets.hovered.bg_fill = p.button_hover;
            v.widgets.hovered.bg_stroke = Stroke::new(1.0, if p.modern { p.select } else { p.text_weak });
            v.widgets.hovered.fg_stroke = Stroke::new(1.5, p.text);
            v.widgets.active.weak_bg_fill = p.button_active;
            v.widgets.active.bg_fill = p.button_active;
            v.widgets.active.bg_stroke = Stroke::new(1.0, p.select);
            v.widgets.active.fg_stroke = Stroke::new(2.0, p.text);
            v.widgets.open.weak_bg_fill = p.button_active;
            v.widgets.open.bg_fill = p.button_active;
            v.widgets.open.bg_stroke = Stroke::new(1.0, p.border);
            v.widgets.open.fg_stroke = Stroke::new(1.0, p.text);
            v.faint_bg_color = if p.dark { g(0x30) } else { g(0xF4) };
            v.error_fg_color = p.error;
        }
        if p.modern {
            // Soft, wide shadows under windows and menus, as the inspiration's cards.
            let a = if p.dark { 110 } else { 38 };
            v.window_shadow = Shadow {
                offset: [0, 6],
                blur: 24,
                spread: 0,
                color: Color32::from_black_alpha(a),
            };
            v.popup_shadow = Shadow {
                offset: [0, 4],
                blur: 16,
                spread: 0,
                color: Color32::from_black_alpha(a),
            };
            v.window_stroke = Stroke::new(1.0, if p.dark { g(0x3A) } else { g(0xDE) });
            v.window_fill = if p.dark { g(0x2E) } else { g(0xF6) };
            v.slider_trailing_fill = true;
            v.handle_shape = egui::style::HandleShape::Circle;
            s.spacing.menu_margin = Margin::same(6);
        } else {
            v.window_shadow = if p.dark {
                Shadow {
                    offset: [3, 4],
                    blur: 8,
                    spread: 0,
                    color: Color32::from_black_alpha(90),
                }
            } else {
                egui::Visuals::light().window_shadow
            };
        }
        for (_, font) in s.text_styles.iter_mut() {
            font.size = (font.size - 2.0).max(10.0);
        }
    });
}

/// Modern: paints the window's gradient backdrop (behind the panels and cards).
pub fn paint_backdrop(ctx: &egui::Context) {
    let p = pal();
    if !p.modern {
        return;
    }
    let rect = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::background());
    painter.add(vgradient(rect, 0, p.backdrop_top, p.backdrop_bottom));
}

pub fn menu_frame() -> Frame {
    let p = pal();
    if p.modern {
        Frame::new().fill(p.menu).inner_margin(Margin {
            left: 10,
            right: 10,
            top: 4,
            bottom: 2,
        })
    } else {
        Frame::new().fill(p.menu).inner_margin(Margin::symmetric(4, 2))
    }
}

/// The frame of a docked panel (Tools, Property inspector).
pub fn panel_frame() -> Frame {
    let p = pal();
    if p.modern {
        // The card itself is painted by `card`; the panel only leaves a gap around it.
        Frame::new().inner_margin(Margin::same(6))
    } else {
        Frame::new()
            .fill(p.panel)
            .inner_margin(Margin::same(2))
            .stroke(Stroke::new(1.0, p.border))
    }
}

pub fn dock_frame() -> Frame {
    let p = pal();
    if p.modern {
        Frame::new().inner_margin(Margin {
            left: 2,
            right: 8,
            top: 6,
            bottom: 6,
        })
    } else {
        Frame::new().fill(p.dock).inner_margin(Margin::same(2))
    }
}

pub fn workspace_frame() -> Frame {
    let p = pal();
    if p.modern {
        Frame::new().inner_margin(Margin::symmetric(0, 6))
    } else {
        Frame::new().fill(p.workspace)
    }
}

/// Modern: a rounded card with a soft gradient and shadow around `body`; Classic: `body` as is
/// (with the given background, if any).
pub fn card<R>(
    ui: &mut egui::Ui,
    top: Color32,
    bottom: Color32,
    margin: i8,
    body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let p = pal();
    if !p.modern {
        return body(ui);
    }
    let shadow_at = ui.painter().add(egui::Shape::Noop);
    let fill_at = ui.painter().add(egui::Shape::Noop);
    let r = Frame::new()
        .inner_margin(Margin::same(margin))
        .show(ui, body);
    let rect = r.response.rect;
    let a = if p.dark { 90 } else { 30 };
    let shadow = Shadow {
        offset: [0, 3],
        blur: 14,
        spread: 0,
        color: Color32::from_black_alpha(a),
    };
    ui.painter()
        .set(shadow_at, shadow.as_shape(rect, CornerRadius::same(p.r_card)));
    ui.painter().set(fill_at, rounded_vgradient(rect, p.r_card as f32, top, bottom));
    r.inner
}

/// A rectangle filled with a vertical gradient (`radius` > 0: rounded corners).
pub fn vgradient(rect: Rect, radius: u8, top: Color32, bottom: Color32) -> egui::Shape {
    if radius > 0 {
        return rounded_vgradient(rect, radius as f32, top, bottom);
    }
    let mut mesh = egui::epaint::Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    egui::Shape::mesh(mesh)
}

/// A rounded rectangle with a vertical gradient: a triangle fan over the outline, with a thin
/// feathered rim so the corners are anti-aliased like egui's own shapes.
pub fn rounded_vgradient(rect: Rect, radius: f32, top: Color32, bottom: Color32) -> egui::Shape {
    rounded_gradient(rect, radius, top, bottom, false)
}

/// A rounded rectangle shaded from `from` to `to`: top to bottom, or left to right when
/// `horizontal`.
pub fn rounded_gradient(rect: Rect, radius: f32, from: Color32, to: Color32, horizontal: bool) -> egui::Shape {
    let r = radius.min(rect.width() / 2.0).min(rect.height() / 2.0).max(0.0);
    let color_at = |q: egui::Pos2| {
        let t = if horizontal {
            (q.x - rect.left()) / rect.width().max(1.0)
        } else {
            (q.y - rect.top()) / rect.height().max(1.0)
        };
        lerp_color(from, to, t.clamp(0.0, 1.0))
    };
    // The outline, clockwise from the top-left corner's start.
    let mut pts: Vec<egui::Pos2> = Vec::new();
    let steps = 8;
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -90.0_f32),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 90.0),
        (pos2(rect.left() + r, rect.top() + r), 180.0),
    ];
    for (c, start) in corners {
        for i in 0..=steps {
            let a = (start + 90.0 * i as f32 / steps as f32).to_radians();
            pts.push(pos2(c.x + r * a.cos(), c.y + r * a.sin()));
        }
    }
    let mut mesh = egui::epaint::Mesh::default();
    let center = rect.center();
    mesh.colored_vertex(center, color_at(center));
    let n = pts.len() as u32;
    // Inner ring (solid) and outer ring (transparent), half a pixel either side of the outline.
    let feather = 0.5;
    for q in &pts {
        let d = (*q - center).normalized();
        mesh.colored_vertex(*q - d * feather, color_at(*q));
    }
    for q in &pts {
        let d = (*q - center).normalized();
        mesh.colored_vertex(*q + d * feather, Color32::TRANSPARENT);
    }
    for i in 0..n {
        let j = (i + 1) % n;
        mesh.add_triangle(0, 1 + i, 1 + j);
        mesh.add_triangle(1 + i, 1 + j, 1 + n + i);
        mesh.add_triangle(1 + j, 1 + n + j, 1 + n + i);
    }
    egui::Shape::mesh(mesh)
}

pub fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

/// Corner radius of a highlighted list row: square in Classic; in Modern a quarter of its
/// height, half as round as the pill-shaped buttons.
pub fn row_radius(row: Rect) -> f32 {
    if pal().modern { row.height() / 4.0 } else { 0.0 }
}

/// A list's current row label (layer, frame, history step, symbol). Classic: white text on
/// Fireworks' blue; Modern: a grey pill around the text, which stays where an unmarked row's
/// text is.
pub fn current_label(ui: &mut egui::Ui, text: impl Into<String>, sense: egui::Sense) -> egui::Response {
    let p = pal();
    let text = text.into();
    if !p.modern {
        let rt = egui::RichText::new(text).color(p.highlight_text).background_color(p.highlight);
        return ui.add(egui::Label::new(rt).sense(sense));
    }
    let galley = egui::WidgetText::from(text.as_str()).into_galley(
        ui,
        Some(egui::TextWrapMode::Extend),
        f32::INFINITY,
        egui::TextStyle::Body,
    );
    let (rect, resp) = ui.allocate_exact_size(galley.size(), sense);
    let pill = rect.expand2(egui::vec2(5.0, 2.0));
    ui.painter().rect_filled(pill, row_radius(pill), p.highlight);
    ui.painter().galley(rect.min, galley, p.highlight_text);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, ui.is_enabled(), &text));
    resp
}

/// A mode switch (Original/Preview, Frames/History): egui's selectable button, which in Modern
/// glows softly around the selected one.
pub fn mode_value<V: PartialEq>(ui: &mut egui::Ui, current: &mut V, value: V, text: &str) -> egui::Response {
    let p = pal();
    let glow = ui.painter().add(egui::Shape::Noop);
    let was = *current == value;
    let resp = ui.selectable_value(current, value, text);
    if p.modern && (was || resp.clicked()) {
        let radius = (resp.rect.height() / 2.0).min(p.r_widget as f32 * 2.0) as u8;
        let shadow = egui::Shadow {
            offset: [0, 0],
            blur: 5,
            spread: 1,
            color: p.select.gamma_multiply(if p.dark { 0.35 } else { 0.28 }),
        };
        ui.painter().set(glow, shadow.as_shape(resp.rect, radius));
    }
    resp
}

/// A panel-group title bar like Fireworks' ("> Layers") that toggles `open`.
pub fn group_header(ui: &mut egui::Ui, title: &str, open: &mut bool) {
    let p = pal();
    let h = if p.modern { 24.0 } else { 18.0 };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), h), egui::Sense::click());
    // Named for accessibility (screen readers, and UI tests that click by name).
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), title));
    let painter = ui.painter();
    let glyph = if *open {
        crate::icons::Glyph::TriangleDown
    } else {
        crate::icons::Glyph::TriangleRight
    };
    if p.modern {
        // A bold title on the card, the chevron on the right.
        if resp.hovered() {
            painter.rect_filled(rect, CornerRadius::same(p.r_widget), p.button_active.gamma_multiply(0.6));
        }
        painter.text(
            egui::pos2(rect.left() + 6.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            title,
            egui::FontId::proportional(13.0),
            p.header_text,
        );
        // Same widget: a second, offset pass reads as a heavier weight without a bold face.
        painter.text(
            egui::pos2(rect.left() + 6.4, rect.center().y),
            egui::Align2::LEFT_CENTER,
            title,
            egui::FontId::proportional(13.0),
            p.header_text,
        );
        crate::icons::paint_glyph(
            painter,
            egui::Rect::from_center_size(
                egui::pos2(rect.right() - 12.0, rect.center().y),
                egui::vec2(9.0, 9.0),
            ),
            glyph,
            p.header_ink,
        );
    } else {
        painter.rect_filled(rect, 0.0, p.header);
        // Gripper dots on the left, as in Fireworks.
        let grip = if p.dark { g(0x80) } else { g(90) };
        for i in 0..3 {
            painter.circle_filled(
                egui::pos2(rect.left() + 4.0, rect.top() + 5.0 + i as f32 * 4.0),
                0.8,
                grip,
            );
        }
        crate::icons::paint_glyph(
            painter,
            egui::Rect::from_center_size(
                egui::pos2(rect.left() + 15.0, rect.center().y),
                egui::vec2(9.0, 9.0),
            ),
            glyph,
            p.header_ink,
        );
        painter.text(
            egui::pos2(rect.left() + 24.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            title,
            egui::FontId::proportional(12.0),
            p.header_text,
        );
    }
    if resp.clicked() {
        *open = !*open;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip() {
        for k in ThemeKind::ALL {
            assert_eq!(ThemeKind::from_key(k.key()), k);
        }
        assert_eq!(ThemeKind::from_key("nonsense"), ThemeKind::Classic);
    }
}
