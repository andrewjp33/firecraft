//! Window > Align: Fireworks MX's Align panel (FireCraft's own artwork; the layout and
//! tooltips follow the panel MX ships). See docs/behavior/align.md.

use egui::{Color32, Painter, Pos2, Rect, Stroke, pos2, vec2};
use firecraft_engine::align_panel::{Edge, MatchSize, PointPart, Space};

use crate::{FireCraftApp, path_tool, theme};

/// What a button of the panel shows (and does).
#[derive(Clone, Copy, PartialEq)]
enum Icon {
    Align(Edge),
    Distribute(Edge),
    Match(MatchSize),
    Space(Space),
    ToCanvas,
    Anchors,
    Part(PointPart),
}

const ORANGE: Color32 = Color32::from_rgb(0xFF, 0x66, 0x00);
/// The panel's content width.
const WIDTH: f32 = 214.0;

/// The panel's artwork colours (Fireworks MX's in the Classic theme; lighter on dark themes).
fn accent_blue() -> Color32 {
    if theme::pal().dark { Color32::from_rgb(0x5A, 0xA8, 0xE0) } else { Color32::from_rgb(0x1F, 0x6A, 0x9A) }
}

fn grey_fill() -> Color32 {
    if theme::pal().dark { Color32::from_gray(0x66) } else { Color32::from_rgb(0xD4, 0xD4, 0xD4) }
}

fn grey_edge() -> Color32 {
    if theme::pal().dark { Color32::from_gray(0xA0) } else { Color32::from_rgb(0x80, 0x80, 0x80) }
}

fn line_color() -> Color32 {
    if theme::pal().dark { Color32::from_gray(0xE8) } else { Color32::from_rgb(0x10, 0x10, 0x10) }
}

fn label_color() -> Color32 {
    let p = theme::pal();
    if p.dark || p.modern { p.text } else { Color32::from_rgb(0x1A, 0x33, 0x66) }
}

/// The preference value of a point part.
pub fn part_key(p: PointPart) -> &'static str {
    match p {
        PointPart::Anchor => "anchor",
        PointPart::Preceding => "preceding",
        PointPart::Succeeding => "succeeding",
    }
}

pub fn part_from_key(k: &str) -> PointPart {
    match k {
        "preceding" => PointPart::Preceding,
        "succeeding" => PointPart::Succeeding,
        _ => PointPart::Anchor,
    }
}

fn tooltip(i: Icon) -> &'static str {
    match i {
        Icon::Align(e) => match e {
            Edge::Left => "Align left edge",
            Edge::HorizCenter => "Align horizontal center",
            Edge::Right => "Align right edge",
            Edge::Top => "Align top edge",
            Edge::VertCenter => "Align vertical center",
            Edge::Bottom => "Align bottom edge",
        },
        Icon::Distribute(e) => match e {
            Edge::Left => "Distribute left edge",
            Edge::HorizCenter => "Distribute horizontal center",
            Edge::Right => "Distribute right edge",
            Edge::Top => "Distribute top edge",
            Edge::VertCenter => "Distribute vertical center",
            Edge::Bottom => "Distribute bottom edge",
        },
        Icon::Match(MatchSize::Width) => "Match width",
        Icon::Match(MatchSize::Height) => "Match height",
        Icon::Match(MatchSize::Both) => "Match width and height",
        Icon::Space(Space::Vertical) => "Space evenly vertically",
        Icon::Space(Space::Horizontal) => "Space evenly horizontally",
        Icon::ToCanvas => "To Canvas",
        Icon::Anchors => "Anchors",
        Icon::Part(p) => part_description(p),
    }
}

fn part_description(p: PointPart) -> &'static str {
    match p {
        PointPart::Anchor => "Apply to selected anchors and control points.",
        PointPart::Preceding => "Apply to the preceding control points of the selected anchors.",
        PointPart::Succeeding => "Apply to the succeeding control points of the selected anchors.",
    }
}

/// A square button with a raised frame (sunken while `on`).
fn icon_button(ui: &mut egui::Ui, icon: Icon, size: egui::Vec2, on: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let name = tooltip(icon);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, true, on, name));
    let p = ui.painter();
    let pal = theme::pal();
    let down = on || resp.is_pointer_button_down_on();
    if pal.modern {
        // A soft rounded tile; pressed or on: sunk a shade and outlined in the accent.
        let fill = if down { pal.button_active } else if resp.hovered() { pal.button_hover } else { pal.button };
        p.rect_filled(r, 6.0, fill);
        let edge = if on { pal.select } else { pal.button_border };
        p.rect_stroke(r, 6.0, Stroke::new(1.0, edge), egui::StrokeKind::Inside);
    } else {
        let (fill, light, dark) = match (down, pal.dark) {
            (true, false) => (Color32::from_gray(0xDD), Color32::from_gray(0x70), Color32::WHITE),
            (false, false) => (Color32::from_gray(0xFA), Color32::WHITE, Color32::from_gray(0x90)),
            (true, true) => (Color32::from_gray(0x2A), Color32::from_gray(0x18), Color32::from_gray(0x60)),
            (false, true) => (pal.button, Color32::from_gray(0x60), Color32::from_gray(0x22)),
        };
        p.rect_filled(r, 0.0, fill);
        p.line_segment([r.left_top(), r.right_top()], Stroke::new(1.0, light));
        p.line_segment([r.left_top(), r.left_bottom()], Stroke::new(1.0, light));
        p.line_segment([r.right_top(), r.right_bottom()], Stroke::new(1.0, dark));
        p.line_segment([r.left_bottom(), r.right_bottom()], Stroke::new(1.0, dark));
    }
    let accent = if on || resp.hovered() { ORANGE } else { accent_blue() };
    paint(p, r.shrink2(vec2((r.width() - 16.0) / 2.0, (r.height() - 16.0) / 2.0)), icon, accent);
    resp.on_hover_text(name)
}

/// Paint `icon` in a 16x16 `r`; `accent` is the colour of its main (moving) shape.
fn paint(p: &Painter, r: Rect, icon: Icon, accent: Color32) {
    let at = |fx: f32, fy: f32| pos2(r.left() + fx * r.width(), r.top() + fy * r.height());
    let bx = |x0: f32, y0: f32, x1: f32, y1: f32| Rect::from_min_max(at(x0, y0), at(x1, y1));
    let blue = |b: Rect| {
        p.rect_filled(b, 0.0, accent);
        p.rect_stroke(b, 0.0, Stroke::new(1.0, accent.gamma_multiply(0.7)), egui::StrokeKind::Inside);
    };
    let grey = |b: Rect| {
        p.rect_filled(b, 0.0, grey_fill());
        p.rect_stroke(b, 0.0, Stroke::new(1.0, grey_edge()), egui::StrokeKind::Inside);
    };
    let line = |a: Pos2, b: Pos2| p.line_segment([a, b], Stroke::new(1.0, line_color()));
    let vline = |x: f32, y0: f32, y1: f32| line(at(x, y0), at(x, y1));
    let hline = |y: f32, x0: f32, x1: f32| line(at(x0, y), at(x1, y));
    match icon {
        Icon::Align(e) => match e {
            Edge::Left => {
                vline(0.03, 0.0, 1.0);
                grey(bx(0.13, 0.1, 0.5, 0.38));
                blue(bx(0.13, 0.55, 0.95, 0.85));
            }
            Edge::HorizCenter => {
                vline(0.5, 0.0, 1.0);
                grey(bx(0.3, 0.1, 0.7, 0.38));
                blue(bx(0.08, 0.55, 0.92, 0.85));
            }
            Edge::Right => {
                vline(0.97, 0.0, 1.0);
                grey(bx(0.5, 0.1, 0.87, 0.38));
                blue(bx(0.05, 0.55, 0.87, 0.85));
            }
            Edge::Top => {
                hline(0.03, 0.0, 1.0);
                blue(bx(0.1, 0.13, 0.4, 0.95));
                grey(bx(0.58, 0.13, 0.85, 0.5));
            }
            Edge::VertCenter => {
                hline(0.5, 0.0, 1.0);
                blue(bx(0.1, 0.08, 0.4, 0.92));
                grey(bx(0.58, 0.3, 0.85, 0.7));
            }
            Edge::Bottom => {
                hline(0.97, 0.0, 1.0);
                blue(bx(0.1, 0.05, 0.4, 0.87));
                grey(bx(0.58, 0.5, 0.85, 0.87));
            }
        },
        Icon::Distribute(e) => {
            // Two objects with the distributed edges marked by lines across them.
            if matches!(e, Edge::Top | Edge::VertCenter | Edge::Bottom) {
                let (g, b) = (bx(0.3, 0.08, 0.7, 0.36), bx(0.18, 0.6, 0.82, 0.88));
                grey(g);
                blue(b);
                let y = |r: Rect| match e {
                    Edge::Top => r.top(),
                    Edge::Bottom => r.bottom(),
                    _ => r.center().y,
                };
                for r in [g, b] {
                    line(pos2(at(0.0, 0.0).x, y(r)), pos2(at(1.0, 0.0).x, y(r)));
                }
            } else {
                let (b, g) = (bx(0.1, 0.12, 0.38, 0.88), bx(0.6, 0.3, 0.85, 0.7));
                blue(b);
                grey(g);
                let x = |r: Rect| match e {
                    Edge::Left => r.left(),
                    Edge::Right => r.right(),
                    _ => r.center().x,
                };
                for r in [b, g] {
                    line(pos2(x(r), at(0.0, 0.0).y), pos2(x(r), at(0.0, 1.0).y));
                }
            }
        }
        Icon::Match(m) => match m {
            MatchSize::Width => {
                vline(0.05, 0.0, 1.0);
                vline(0.95, 0.0, 1.0);
                grey(bx(0.28, 0.1, 0.72, 0.38));
                blue(bx(0.05, 0.55, 0.95, 0.85));
            }
            MatchSize::Height => {
                hline(0.05, 0.55, 1.0);
                hline(0.95, 0.55, 1.0);
                blue(bx(0.1, 0.05, 0.4, 0.95));
                grey(bx(0.6, 0.3, 0.88, 0.7));
            }
            MatchSize::Both => {
                grey(bx(0.08, 0.05, 0.3, 0.3));
                blue(bx(0.08, 0.45, 0.48, 0.95));
                grey(bx(0.65, 0.62, 0.9, 0.82));
                vline(0.5, 0.05, 0.4);
                hline(0.95, 0.55, 1.0);
            }
        },
        Icon::Space(Space::Vertical) => {
            grey(bx(0.45, 0.05, 0.85, 0.3));
            hline(0.4, 0.0, 0.6);
            hline(0.55, 0.0, 0.6);
            blue(bx(0.25, 0.65, 0.75, 0.92));
        }
        Icon::Space(Space::Horizontal) => {
            blue(bx(0.08, 0.25, 0.35, 0.95));
            vline(0.45, 0.0, 0.7);
            vline(0.6, 0.0, 0.7);
            grey(bx(0.68, 0.5, 0.92, 0.95));
        }
        Icon::ToCanvas => {
            // A sheet with crop marks at its corners.
            let fill = if accent == ORANGE { ORANGE } else { grey_fill() };
            let b = bx(0.2, 0.2, 0.8, 0.8);
            p.rect_filled(b, 0.0, fill);
            p.rect_stroke(b, 0.0, Stroke::new(1.0, grey_edge()), egui::StrokeKind::Inside);
            for (x, y, dx, dy) in [(0.2, 0.2, -1.0, -1.0), (0.8, 0.2, 1.0, -1.0), (0.2, 0.8, -1.0, 1.0), (0.8, 0.8, 1.0, 1.0)] {
                let c = at(x, y);
                line(c + vec2(dx * 1.0, 0.0), c + vec2(dx * 4.0, 0.0));
                line(c + vec2(0.0, dy * 1.0), c + vec2(0.0, dy * 4.0));
            }
        }
        Icon::Anchors => {
            // A path through three points, with a curve's handles at the middle one.
            let pts = [at(0.08, 0.92), at(0.5, 0.5), at(0.92, 0.1)];
            p.line_segment([pts[0], pts[2]], Stroke::new(1.0, accent));
            let handle = Stroke::new(1.0, grey_edge());
            p.add(egui::Shape::QuadraticBezier(egui::epaint::QuadraticBezierShape::from_points_stroke(
                [at(0.5, 0.5), at(0.75, 0.55), at(0.85, 0.75)],
                false,
                Color32::TRANSPARENT,
                handle,
            )));
            p.line_segment([at(0.5, 0.5), at(0.3, 0.85)], handle);
            for c in pts {
                p.rect_filled(Rect::from_center_size(c, vec2(3.0, 3.0)), 0.0, accent);
            }
        }
        Icon::Part(part) => {
            // A point with its two handles; the part that lines up is drawn large.
            let y = r.center().y;
            let xs = [r.left() + 1.5, r.center().x, r.right() - 1.5];
            line(pos2(xs[0], y), pos2(xs[2], y));
            let big = |i: usize| match part {
                PointPart::Anchor => true,
                PointPart::Preceding => i == 0,
                PointPart::Succeeding => i == 2,
            };
            for (i, x) in xs.into_iter().enumerate() {
                let (s, c) = if big(i) {
                    (if i == 1 { 5.0 } else { 4.0 }, ORANGE)
                } else if i == 1 {
                    (3.0, line_color())
                } else {
                    (3.0, accent_blue())
                };
                p.rect_filled(Rect::from_center_size(pos2(x, y), vec2(s, s)), 0.0, c);
            }
        }
    }
}

/// The spacing field's text: "Evenly" or the gap in pixels.
pub fn spacing_text(v: Option<i32>) -> String {
    v.map_or_else(|| "Evenly".to_owned(), |n| n.to_string())
}

/// The gap typed in the spacing field, read as Fireworks CS6 reads it (JavaScript `parseInt`:
/// leading spaces, a sign and the digits that follow; "12px" is 12, "1.9" is 1); anything
/// else, "Evenly" included, means Evenly.
pub fn parse_spacing(text: &str) -> Option<i32> {
    let t = text.trim_start();
    let (neg, rest) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let n: i64 = digits.get(..digits.len().min(9))?.parse().ok()?;
    let n = n.min(100_000) as i32;
    Some(if neg { -n } else { n })
}

/// Keep what Fireworks CS6's field accepts: digits, and a minus sign in front.
fn digits_only(text: &str) -> String {
    text.chars()
        .enumerate()
        .filter(|&(i, c)| c.is_ascii_digit() || (i == 0 && c == '-'))
        .map(|(_, c)| c)
        .collect()
}

/// The spacing field's accessible name and tooltip.
const SPACING_NAME: &str = "Space between objects: Evenly, or a gap in pixels";

/// The spacing field's menu: the last gap ("12 px") and Evenly. Whether the gap was chosen.
fn spacing_menu(ui: &mut egui::Ui, value: &mut Option<i32>, last: i32) -> bool {
    let mut gap = false;
    if ui.button(format!("{last} px")).clicked() {
        *value = Some(last);
        gap = true;
        ui.close();
    }
    if ui.button("Evenly").clicked() {
        *value = None;
        ui.close();
    }
    gap
}

/// The Space section's field (after Fireworks CS6's): "Evenly", or a gap in pixels. Showing
/// Evenly it is a drop-down: a click on the text or the arrow opens the menu (the last gap,
/// "12 px", or Evenly), and clicking elsewhere leaves it as it was. Choosing the gap turns it
/// into a number field with the gap selected, so typing replaces it; only the menu goes back to
/// Evenly.
fn spacing_field(ui: &mut egui::Ui, value: &mut Option<i32>, last: &mut i32) {
    let text_id = egui::Id::unique("align_spacing_text");
    let edit_id = egui::Id::unique("align_spacing_edit");
    let focus_id = egui::Id::unique("align_spacing_focus");
    let field = vec2(58.0, 18.0);
    if value.is_none() {
        let (rect, resp) = ui.allocate_exact_size(vec2(field.x + 15.0, field.y), egui::Sense::click());
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::ComboBox, true, SPACING_NAME));
        let p = ui.painter();
        let text_rect = Rect::from_min_size(rect.min, field);
        p.rect_filled(text_rect, 0.0, theme::pal().field);
        p.rect_stroke(text_rect, 0.0, Stroke::new(1.0, theme::border()), egui::StrokeKind::Inside);
        p.text(
            text_rect.left_center() + vec2(4.0, 0.0),
            egui::Align2::LEFT_CENTER,
            "Evenly",
            egui::FontId::proportional(12.0),
            ui.visuals().text_color(),
        );
        let arrow = Rect::from_min_max(pos2(text_rect.right() + 1.0, rect.top()), rect.max);
        if resp.hovered() {
            p.rect_stroke(arrow, 1.0, Stroke::new(1.0, theme::pal().text_weak), egui::StrokeKind::Inside);
        }
        crate::icons::paint_glyph(
            p,
            Rect::from_center_size(arrow.center(), vec2(10.0, 10.0)),
            crate::icons::Glyph::TriangleDown,
            theme::pal().ink,
        );
        let resp = resp.on_hover_text(SPACING_NAME);
        egui::Popup::menu(&resp).show(|ui| {
            if spacing_menu(ui, value, *last) {
                ui.data_mut(|d| d.insert_temp(focus_id, true));
            }
        });
        ui.data_mut(|d| d.insert_temp(text_id, spacing_text(*value)));
        return;
    }
    let mut text = ui
        .data(|d| d.get_temp::<String>(text_id))
        .unwrap_or_else(|| spacing_text(*value));
    // Just switched from Evenly by the menu: the gap, selected, ready to type over.
    if ui.data_mut(|d| d.remove_temp::<bool>(focus_id)).unwrap_or(false) {
        text = spacing_text(*value);
        let mut state = egui::text_edit::TextEditState::load(ui.ctx(), edit_id).unwrap_or_default();
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::select_all_str(&text)));
        state.store(ui.ctx(), edit_id);
        ui.memory_mut(|m| m.request_focus(edit_id));
    }
    // A visible box on the panel.
    let resp = egui::Frame::new()
        .stroke(Stroke::new(1.0, theme::border()))
        .fill(theme::pal().field)
        .inner_margin(egui::Margin::symmetric(2, 0))
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut text)
                    .id(edit_id)
                    .frame(egui::Frame::NONE)
                    .desired_width(52.0),
            )
        })
        .inner
        .on_hover_text(SPACING_NAME);
    // Typing never switches to Evenly (only the menu does): while the text is empty or just a
    // minus sign the gap stays what it was, and leaving the field puts that gap back.
    if resp.changed() {
        text = digits_only(&text);
        if let Some(n) = parse_spacing(&text) {
            *value = Some(n);
            *last = n;
        }
    }
    if !resp.has_focus() {
        text = spacing_text(*value);
    }
    let menu = crate::icons::glyph_button(ui, crate::icons::Glyph::TriangleDown, 14.0, true)
        .on_hover_text("Spacing");
    egui::Popup::menu(&menu).show(|ui| {
        if spacing_menu(ui, value, *last) {
            ui.data_mut(|d| d.insert_temp(focus_id, true));
        }
    });
    ui.data_mut(|d| d.insert_temp(text_id, text));
}

fn label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).color(label_color()).size(11.0));
}

/// A row of buttons; returns the one clicked.
fn row(ui: &mut egui::Ui, icons: &[Icon]) -> Option<Icon> {
    let mut hit = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        for &i in icons {
            if icon_button(ui, i, vec2(24.0, 24.0), false).clicked() {
                hit = Some(i);
            }
        }
    });
    hit
}

/// The Align tab of the dock's "Optimize and Align" group (Window > Align), and what its
/// buttons ask.
pub fn panel(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let mut hit: Option<Icon> = None;
    let anchors = app.prefs.align_anchors;
    let to_canvas = app.prefs.align_to_canvas;
    let part = app.prefs.align_part;
    let (aligns, other_aligns) = (
        [Icon::Align(Edge::Left), Icon::Align(Edge::HorizCenter), Icon::Align(Edge::Right)],
        [Icon::Align(Edge::Top), Icon::Align(Edge::VertCenter), Icon::Align(Edge::Bottom)],
    );
    // White, as Fireworks MX's panel.
    egui::Frame::new()
        .fill(if theme::pal().dark || theme::pal().modern { Color32::TRANSPARENT } else { Color32::WHITE })
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            let width = ui.available_width().min(WIDTH);
            // To Canvas on the left, Anchors on the right (To Canvas hides in Anchors mode).
            let (top, _) = ui.allocate_exact_size(vec2(width, 46.0), egui::Sense::hover());
            let mut toggle = |ui: &mut egui::Ui, rect: Rect, text: &str, icon: Icon, on: bool, align: egui::Align| {
                let mut c = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(rect)
                        .layout(egui::Layout::top_down(align)),
                );
                label(&mut c, text);
                if icon_button(&mut c, icon, vec2(26.0, 26.0), on).clicked() {
                    hit = Some(icon);
                }
            };
            if !anchors {
                let left = Rect::from_min_size(top.min + vec2(8.0, 0.0), vec2(80.0, top.height()));
                toggle(ui, left, "To Canvas:", Icon::ToCanvas, to_canvas, egui::Align::Min);
            }
            let right = Rect::from_min_max(pos2(top.right() - 88.0, top.top()), pos2(top.right() - 8.0, top.bottom()));
            toggle(ui, right, "Anchors:", Icon::Anchors, anchors, egui::Align::Max);
            ui.add_space(4.0);
            egui::Frame::new()
                .stroke(Stroke::new(1.0, if theme::pal().dark { theme::border() } else { Color32::from_gray(0xCC) }))
                .corner_radius(egui::CornerRadius {
                    nw: 10,
                    ne: 10,
                    sw: 0,
                    se: 0,
                })
                .inner_margin(egui::Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    label(ui, "Align:");
                    ui.horizontal(|ui| {
                        hit = hit.or(row(ui, &aligns));
                        ui.add_space(10.0);
                        hit = hit.or(row(ui, &other_aligns));
                    });
                    if anchors {
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            for p in [PointPart::Preceding, PointPart::Anchor, PointPart::Succeeding] {
                                if icon_button(ui, Icon::Part(p), vec2(36.0, 16.0), p == part).clicked() {
                                    hit = Some(Icon::Part(p));
                                }
                                ui.add_space(14.0);
                            }
                        });
                        ui.add(egui::Label::new(egui::RichText::new(part_description(part)).color(theme::pal().text_weak)).wrap());
                        return;
                    }
                    label(ui, "Distribute:");
                    ui.horizontal(|ui| {
                        hit = hit.or(row(
                            ui,
                            &[
                                Icon::Distribute(Edge::Top),
                                Icon::Distribute(Edge::VertCenter),
                                Icon::Distribute(Edge::Bottom),
                            ],
                        ));
                        ui.add_space(10.0);
                        hit = hit.or(row(
                            ui,
                            &[
                                Icon::Distribute(Edge::Left),
                                Icon::Distribute(Edge::HorizCenter),
                                Icon::Distribute(Edge::Right),
                            ],
                        ));
                    });
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            label(ui, "Match Size:");
                            hit = hit.or(row(
                                ui,
                                &[
                                    Icon::Match(MatchSize::Width),
                                    Icon::Match(MatchSize::Height),
                                    Icon::Match(MatchSize::Both),
                                ],
                            ));
                        });
                        ui.add_space(10.0);
                        // As in Fireworks CS6: the Space buttons and the spacing field in a
                        // grey box of their own.
                        egui::Frame::new()
                            .fill(if theme::pal().dark { Color32::from_white_alpha(10) } else if theme::pal().modern { Color32::from_black_alpha(10) } else { Color32::from_gray(0xEC) })
                            .corner_radius(4)
                            .inner_margin(egui::Margin::symmetric(6, 3))
                            .show(ui, |ui| {
                                ui.vertical(|ui| {
                                    label(ui, "Space:");
                                    hit = hit.or(row(ui, &[Icon::Space(Space::Vertical), Icon::Space(Space::Horizontal)]));
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 1.0;
                                        let prefs = &mut app.prefs;
                                        spacing_field(ui, &mut prefs.align_spacing, &mut prefs.align_spacing_px);
                                    });
                                });
                            });
                    });
                });
        });
    if let Some(i) = hit {
        act(app, i);
    }
}

fn act(app: &mut FireCraftApp, icon: Icon) {
    let to_canvas = app.prefs.align_to_canvas;
    match icon {
        Icon::ToCanvas => app.prefs.align_to_canvas = !to_canvas,
        Icon::Anchors => app.prefs.align_anchors = !app.prefs.align_anchors,
        Icon::Part(p) => app.prefs.align_part = p,
        Icon::Align(e) if app.prefs.align_anchors => align_points(app, e),
        _ => {
            let Some(d) = app.session.active_doc_mut() else {
                return;
            };
            match icon {
                Icon::Align(e) => d.panel_align(e, to_canvas),
                Icon::Distribute(e) => d.panel_distribute(e, to_canvas),
                Icon::Match(m) => d.match_size(m, to_canvas),
                Icon::Space(s) => match app.prefs.align_spacing {
                    None => d.space(s, to_canvas),
                    Some(gap) => d.space_by(s, gap, to_canvas),
                },
                _ => {}
            }
        }
    }
}

/// Anchors mode: line up the selected points (Subselection or Pen) of the selected path.
fn align_points(app: &mut FireCraftApp, edge: Edge) {
    let part = app.prefs.align_part;
    let Some(d) = app.session.active_doc_mut() else {
        return;
    };
    // As Fireworks MX's panel: only paths can have their points aligned.
    let paths_only = !d.selection.is_empty() && d.selection.iter().all(|r| d.path(*r).is_some());
    if !paths_only {
        app.error("Invalid selection. Please try again.".into());
        return;
    }
    if let Some((r, pts)) = path_tool::selected_points(&mut app.path_edit, d) {
        d.align_points(r, &pts, edge, part);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_reads_like_parse_int() {
        assert_eq!(parse_spacing("10"), Some(10));
        assert_eq!(parse_spacing("  -5"), Some(-5));
        assert_eq!(parse_spacing("12px"), Some(12));
        assert_eq!(parse_spacing("1.9"), Some(1));
        assert_eq!(parse_spacing("Evenly"), None);
        assert_eq!(parse_spacing(""), None);
        assert_eq!(parse_spacing("-"), None);
        assert_eq!(parse_spacing(&spacing_text(Some(-3))), Some(-3));
        assert_eq!(digits_only("-1a2-3"), "-123");
        assert_eq!(digits_only("Evenly"), "");
    }

    #[test]
    fn part_keys_round_trip() {
        for p in [PointPart::Anchor, PointPart::Preceding, PointPart::Succeeding] {
            assert_eq!(part_from_key(part_key(p)), p);
        }
        assert_eq!(part_from_key("junk"), PointPart::Anchor);
    }
}
