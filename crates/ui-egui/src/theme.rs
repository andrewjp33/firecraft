//! The interface's look. Four themes:
//!
//! - **Classic**: Fireworks MX's look: light panels, grey panel-group headers, mid-grey workspace.
//! - **Dark**: the same layout and shapes in dark greys.
//! - **Modern**: soft greys, gradients, rounded cards and pill buttons and a dark tool sidebar.
//!
//! All use Fireworks' blue (#3399FF) as the accent unless the user picks another.
//! - **Modern Dark**: Modern's shapes in dark greys.
//!
//! Colours that belong to the document (the canvas, checkerboards, selection handles, previews
//! of colours and strokes) stay the same in every theme; only the interface around them changes.

use std::sync::RwLock;
use std::sync::atomic::{AtomicU8, Ordering};

use egui::{Color32, CornerRadius, Frame, Margin, Rect, Shadow, Stroke, pos2};

/// Object selection outline / handles (Fireworks' light blue), in every theme.
pub const HANDLE_BLUE: Color32 = Color32::from_rgb(0x00, 0xA8, 0xFF);
/// Path outlines and points on the canvas (Fireworks' selection blue), in every theme.
pub const PATH_BLUE: Color32 = Color32::from_rgb(0x33, 0x99, 0xFF);

/// Edit > Preferences > Theme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ThemeKind {
    Classic,
    Dark,
    #[default]
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

    pub fn is_dark(self) -> bool {
        matches!(self, Self::Dark | Self::ModernDark)
    }

    /// The same theme family (Classic or Modern), light or dark.
    pub fn with_dark(self, dark: bool) -> Self {
        match (self, dark) {
            (Self::Classic | Self::Dark, false) => Self::Classic,
            (Self::Classic | Self::Dark, true) => Self::Dark,
            (Self::Modern | Self::ModernDark, false) => Self::Modern,
            (Self::Modern | Self::ModernDark, true) => Self::ModernDark,
        }
    }

    /// The theme shown: with "Read dark mode from system", the light or dark version as the
    /// system is set (`None`: the system doesn't say; the theme as chosen).
    pub fn resolve(self, follow_system: bool, system_dark: Option<bool>) -> Self {
        match system_dark {
            Some(dark) if follow_system => self.with_dark(dark),
            _ => self,
        }
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
#[derive(Clone, Copy, Debug)]
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
    /// Modern popups (dropdown lists, menus, the tools' press-and-hold menu): the row under
    /// the pointer, and the selected row.
    pub popup_hover: Color32,
    pub popup_selected: Color32,
    pub popup_selected_text: Color32,
    /// Modern: a menu bar title under the pointer or open (on the backdrop, so darker).
    pub menu_bar_hover: Color32,
    /// The filled part of a slider's track (Modern: grey; orange is kept for modes).
    pub slider_fill: Color32,
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
    popup_hover: g(0xFA),
    popup_selected: rgb(0x33, 0x99, 0xFF),
    popup_selected_text: Color32::WHITE,
    menu_bar_hover: g(0xFA),
    slider_fill: rgb(0x33, 0x99, 0xFF),
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
    select: ACCENT,
    select_text: Color32::WHITE,
    highlight: ACCENT,
    highlight_text: Color32::WHITE,
    popup_hover: g(0x56),
    popup_selected: ACCENT,
    popup_selected_text: Color32::WHITE,
    menu_bar_hover: g(0x56),
    slider_fill: ACCENT,
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

/// Every theme's default accent: Fireworks MX's selection blue (the user's choice; Modern was
/// first the design inspiration's orange #DB6745, then the splash screen's #5040D0).
pub const ACCENT: Color32 = rgb(0x33, 0x99, 0xFF);

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
    select: ACCENT,
    select_text: Color32::WHITE,
    highlight: g(0xDD),
    highlight_text: g(0x14),
    popup_hover: g(0xDD),
    popup_selected: g(0x5A),
    popup_selected_text: Color32::WHITE,
    menu_bar_hover: g(0xC4),
    slider_fill: g(0x4A),
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



// Lighter greys, closer to Classic Dark's (user, 2026-11-23: the first version was too dark).
const MODERN_DARK: Palette = Palette {
    dark: true,
    modern: true,
    panel: g(0x3C),
    panel_bottom: g(0x35),
    workspace: g(0x26),
    backdrop_top: g(0x34),
    backdrop_bottom: g(0x1E),
    header: g(0x3C),
    header_text: g(0xF0),
    header_ink: g(0xC8),
    section: Color32::TRANSPARENT,
    section_text: g(0xA4),
    border: g(0x4C),
    select: ACCENT,
    select_text: Color32::WHITE,
    highlight: g(0x58),
    highlight_text: g(0xF2),
    popup_hover: g(0x52),
    popup_selected: g(0x6E),
    popup_selected_text: Color32::WHITE,
    menu_bar_hover: g(0x4C),
    slider_fill: g(0x9C),
    text: g(0xE8),
    text_weak: g(0x90),
    ink: g(0xE8),
    ink_disabled: g(0x6A),
    glyph: g(0xDC),
    glyph_disabled: g(0x66),
    field: g(0x28),
    button: g(0x4A),
    button_hover: g(0x54),
    button_active: g(0x5E),
    button_border: g(0x56),
    menu: Color32::TRANSPARENT,
    dock: Color32::TRANSPARENT,
    pasteboard: g(0x2A),
    title_active: g(0x48),
    title_inactive: g(0x3C),
    title_text: g(0xF0),
    title_text_inactive: g(0x8A),
    tab_inactive: g(0x30),
    tools_top: g(0x58),
    tools_bottom: g(0x34),
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

/// The palette in use: the theme's, or a copy with the user's highlight colour.
static ACTIVE: RwLock<&'static Palette> = RwLock::new(&CLASSIC);

/// The theme in use.
pub fn kind() -> ThemeKind {
    ThemeKind::ALL[CURRENT.load(Ordering::Relaxed) as usize % ThemeKind::ALL.len()]
}

/// The colours of the theme in use.
pub fn pal() -> &'static Palette {
    *ACTIVE.read().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// White or black, whichever reads on `bg`: black on colours brighter than this by perceived
/// brightness (luma, Rec. 601 weights), so the default #3399FF (luma 134) keeps white text and
/// yellows, light greens, cyans and pastels get black.
pub fn text_on(bg: Color32) -> Color32 {
    let luma = 0.299 * bg.r() as f32 + 0.587 * bg.g() as f32 + 0.114 * bg.b() as f32;
    if luma >= 150.0 { Color32::BLACK } else { Color32::WHITE }
}

/// The system's dark mode as FireCraft read it itself (Linux, where winit doesn't report it):
/// 0 unknown, 1 light, 2 dark.
static SYSTEM_DARK: AtomicU8 = AtomicU8::new(0);

/// Set by the desktop app's own reading of the system's dark mode (`None`: unknown).
pub fn set_system_dark(dark: Option<bool>) {
    SYSTEM_DARK.store(dark.map_or(0, |d| if d { 2 } else { 1 }), Ordering::Relaxed);
}

/// Is the system in dark mode? FireCraft's own reading first (Linux), then what the window
/// system reports (Windows, macOS, browsers); `None` if neither knows.
pub fn system_dark(ctx: &egui::Context) -> Option<bool> {
    match SYSTEM_DARK.load(Ordering::Relaxed) {
        1 => Some(false),
        2 => Some(true),
        _ => ctx.system_theme().map(|t| t == egui::Theme::Dark),
    }
}

/// `kind`'s palette with `accent` (Edit > Preferences > Highlight color) in place of the theme's
/// own highlight colour. Classic themes use theirs for current rows and selected menu items
/// too; Modern keeps grey for those. Made once per combination and kept.
fn palette_with(kind: ThemeKind, accent: Option<Color32>) -> &'static Palette {
    type Made = Vec<(ThemeKind, Color32, &'static Palette)>;
    static MADE: std::sync::Mutex<Made> = std::sync::Mutex::new(Vec::new());
    let base = kind.palette();
    let Some(c) = accent.filter(|c| *c != base.select) else {
        return base;
    };
    let mut made = MADE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((.., p)) = made.iter().find(|(k, a, _)| *k == kind && *a == c) {
        return p;
    }
    let mut p = *base;
    let text = text_on(c);
    p.select = c;
    p.select_text = text;
    if !base.modern {
        p.highlight = c;
        p.highlight_text = text;
        p.popup_selected = c;
        p.popup_selected_text = text;
        p.slider_fill = c;
    }
    let p: &'static Palette = Box::leak(Box::new(p));
    made.push((kind, c, p));
    p
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

/// Switch themes: colours, corners and egui's widget style. `accent`: the user's highlight
/// colour, or the theme's.
pub fn apply(ctx: &egui::Context, kind: ThemeKind, accent: Option<Color32>) {
    CURRENT.store(ThemeKind::ALL.iter().position(|k| *k == kind).unwrap_or(0) as u8, Ordering::Relaxed);
    let p = palette_with(kind, accent);
    *ACTIVE.write().unwrap_or_else(std::sync::PoisonError::into_inner) = p;
    // egui keeps a light and a dark style and picks one by the system's setting; FireCraft
    // picks the theme itself, so egui stays on the one that matches it.
    ctx.set_theme(if p.dark { egui::Theme::Dark } else { egui::Theme::Light });
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
            v.window_stroke = Stroke::new(1.0, if p.dark { g(0x4C) } else { g(0xDE) });
            v.window_fill = if p.dark { g(0x3A) } else { g(0xF6) };
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

/// The interface's font, Ubuntu Regular (Ubuntu Font Licence, `fonts/UFL.txt`). It replaces
/// egui's bundled fonts (its `default_fonts` feature is off): Ubuntu Light, whose hairline
/// strokes were hard to read at FireCraft's small sizes, Hack for monospaced text, which
/// FireCraft doesn't show, and egui's logo icons, which it doesn't use. Regular covers the same
/// characters as Light, so nothing falls back.
pub const UI_FONT: &[u8] = include_bytes!("../fonts/Ubuntu-Regular.ttf");

/// Give egui its fonts. The host calls this before the first frame: egui has none of its own.
pub fn install_fonts(ctx: &egui::Context) {
    let mut defs = egui::FontDefinitions::empty();
    defs.font_data.insert("Ubuntu-Regular".to_owned(), std::sync::Arc::new(egui::FontData::from_static(UI_FONT)));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        defs.families.insert(family, vec!["Ubuntu-Regular".to_owned()]);
    }
    ctx.set_fonts(defs);
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

/// The Property inspector's panel. Modern: no side gaps, so its card lines up with the
/// document area above it (and a tabbed document's edges).
pub fn properties_frame() -> Frame {
    if pal().modern {
        Frame::new().inner_margin(Margin { left: 0, right: 0, top: 6, bottom: 6 })
    } else {
        panel_frame()
    }
}

pub fn dock_frame() -> Frame {
    let p = pal();
    if p.modern {
        // The same 6 px gap on every side as the Tools panel's.
        Frame::new().inner_margin(Margin::same(6))
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
    // The card never grows past the space it was given: contents too wide for it are cut off
    // inside, and its right side stays rounded.
    let right = ui.max_rect().right();
    let r = Frame::new().inner_margin(Margin::same(margin)).show(ui, |ui| {
        let mut clip = ui.clip_rect();
        clip.max.x = clip.max.x.min(right - margin as f32);
        ui.set_clip_rect(clip);
        body(ui)
    });
    let mut rect = r.response.rect;
    rect.max.x = rect.max.x.min(right);
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

/// Modern popup colours: a visible grey under the pointer, dark grey for the selected row
/// (orange is kept for modes).
fn popup_colors(s: &mut egui::Style) {
    let p = pal();
    let v = &mut s.visuals;
    for w in [&mut v.widgets.hovered, &mut v.widgets.active] {
        w.weak_bg_fill = p.popup_hover;
        w.bg_fill = p.popup_hover;
        w.bg_stroke = Stroke::NONE;
        w.fg_stroke = Stroke::new(1.0, p.text);
    }
    v.selection.bg_fill = p.popup_selected;
    v.selection.stroke = Stroke::new(1.0, p.popup_selected_text);
    // Room around the text, so a highlighted row's rounded band doesn't touch it.
    s.spacing.button_padding = egui::vec2(8.0, 3.0);
}

/// The menu bar's titles (File, Edit...): egui's menu style with room around the text.
pub fn menu_bar_style() -> egui::style::StyleModifier {
    if pal().modern {
        egui::style::StyleModifier::new(|s| {
            egui::containers::menu::menu_style(s);
            popup_colors(s);
            s.spacing.button_padding = egui::vec2(7.0, 2.0);
            let p = pal();
            s.visuals.widgets.hovered.weak_bg_fill = p.menu_bar_hover;
            s.visuals.widgets.hovered.bg_fill = p.menu_bar_hover;
        })
    } else {
        egui::containers::menu::menu_style.into()
    }
}

/// The style of a dropdown list's popup.
pub fn popup_style() -> egui::style::StyleModifier {
    if pal().modern {
        egui::style::StyleModifier::new(popup_colors)
    } else {
        egui::style::StyleModifier::default()
    }
}

/// The style of menus (egui's menu style, plus the popup colours in Modern).
pub fn menu_popup_style() -> egui::style::StyleModifier {
    if pal().modern {
        egui::style::StyleModifier::new(|s| {
            egui::containers::menu::menu_style(s);
            popup_colors(s);
        })
    } else {
        egui::containers::menu::menu_style.into()
    }
}

/// A menu bar title and its menu; in Modern the title stays highlighted while its menu is open
/// (egui only highlights it under the pointer).
pub fn bar_menu<R>(
    ui: &mut egui::Ui,
    title: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let behind = ui.painter().add(egui::Shape::Noop);
    let r = ui.menu_button(title, add_contents);
    let p = pal();
    if p.modern && r.inner.is_some() {
        let band = egui::Shape::rect_filled(r.response.rect, p.r_widget, p.menu_bar_hover);
        ui.painter().set(behind, band);
    }
    r
}

/// Menu settings carrying [`menu_popup_style`] to a menu and its submenus.
pub fn menu_config() -> egui::containers::menu::MenuConfig {
    egui::containers::menu::MenuConfig::new().style(menu_popup_style())
}

/// A slider; in Modern its track fills in grey rather than the selection's orange.
pub fn slider(ui: &mut egui::Ui, slider: impl egui::Widget) -> egui::Response {
    let p = pal();
    if !p.modern {
        return ui.add(slider);
    }
    ui.scope(|ui| {
        ui.visuals_mut().selection.bg_fill = p.slider_fill;
        ui.add(slider)
    })
    .inner
}

/// A dropdown list styled for the theme.
pub fn combo<'a>(id_salt: impl egui::AsIdSalt) -> egui::ComboBox<'a> {
    egui::ComboBox::from_id_salt(id_salt).popup_style(popup_style())
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
    fn reading_dark_mode_from_the_system() {
        use ThemeKind::*;
        assert_eq!(ThemeKind::default(), Modern);
        assert_eq!(Modern.resolve(true, Some(true)), ModernDark);
        assert_eq!(ModernDark.resolve(true, Some(false)), Modern);
        assert_eq!(Classic.resolve(true, Some(true)), Dark);
        assert_eq!(Dark.resolve(true, None), Dark);
        assert_eq!(Classic.resolve(false, Some(true)), Classic);
    }

    #[test]
    fn text_reads_on_the_accent() {
        assert_eq!(text_on(ACCENT), Color32::WHITE);
        assert_eq!(text_on(Color32::from_rgb(0x50, 0x40, 0xD0)), Color32::WHITE);
        assert_eq!(text_on(Color32::YELLOW), Color32::BLACK);
        assert_eq!(text_on(Color32::from_rgb(0x90, 0xEE, 0x90)), Color32::BLACK);
        assert_eq!(text_on(Color32::BLACK), Color32::WHITE);
    }

    #[test]
    fn a_highlight_colour_replaces_the_accent() {
        let c = Color32::from_rgb(0x90, 0xEE, 0x90);
        let m = palette_with(ThemeKind::Modern, Some(c));
        assert_eq!(m.select, c);
        // Light green: black text on it.
        assert_eq!(m.select_text, Color32::BLACK);
        assert_eq!(m.highlight, MODERN.highlight);
        let k = palette_with(ThemeKind::Classic, Some(c));
        assert_eq!((k.select, k.highlight, k.highlight_text), (c, c, Color32::BLACK));
        for t in ThemeKind::ALL {
            assert_eq!(t.palette().select, ACCENT);
        }
        assert!(std::ptr::eq(palette_with(ThemeKind::Modern, None), ThemeKind::Modern.palette()));
        assert!(std::ptr::eq(palette_with(ThemeKind::Modern, Some(c)), m));
    }

    #[test]
    fn keys_round_trip() {
        for k in ThemeKind::ALL {
            assert_eq!(ThemeKind::from_key(k.key()), k);
        }
        assert_eq!(ThemeKind::from_key("nonsense"), ThemeKind::Modern);
    }
}
