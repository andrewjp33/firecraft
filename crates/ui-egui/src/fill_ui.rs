//! Fill controls shared by the Tools panel and the Property inspector: the box showing a fill of
//! any kind, the Fill Options menu and Fireworks MX's Fill Options window for Pattern and Web
//! Dither fills (docs/behavior/gradients.md).

use egui::{Color32, ComboBox, Pos2, Rect, Sense, Stroke, vec2};
use firecraft_doc::Fill;
use firecraft_engine::{PATTERN, WEB_DITHER, fill_label, new_pattern};
use firecraft_io::textures::pattern_label;
use firecraft_raster::Rgba;
use firecraft_raster::pattern::web_dither_at;

use crate::FireCraftApp;
use crate::fpui::color_picker::{self as cp, ColorChoice, Footer};
use crate::gradient_ui::{self, Target};

/// What a Fill Options menu chose.
#[derive(Clone, Copy)]
pub(crate) enum FillPick {
    None,
    Category(&'static str),
}

/// Fireworks MX's Fill Options menu.
pub(crate) fn fill_options(
    ui: &mut egui::Ui,
    id: &str,
    label: &str,
    gradients_only: bool,
) -> Option<FillPick> {
    let mut pick = None;
    ComboBox::from_id_salt(id)
        .width(90.0)
        .height(400.0)
        .selected_text(label)
        .show_ui(ui, |ui| {
            if !gradients_only {
                if ui.selectable_label(label == "None", "None").clicked() {
                    pick = Some(FillPick::None);
                }
                for (category, name) in [
                    ("fc_Solid", "Solid"),
                    (firecraft_engine::WEB_DITHER, "Web Dither"),
                    (PATTERN, "Pattern"),
                ] {
                    if ui.selectable_label(label == name, name).clicked() {
                        pick = Some(FillPick::Category(category));
                    }
                }
                ui.separator();
            }
            for (category, name) in firecraft_engine::GRADIENT_FILLS {
                if ui.selectable_label(label == name, name).clicked() {
                    pick = Some(FillPick::Category(category));
                }
            }
        });
    pick
}

/// Whether a fill is a plain colour.
pub(crate) fn is_plain(f: &Fill) -> bool {
    f.gradient.is_none() && f.pattern.is_none() && f.dither.is_none()
}

fn paint(painter: &egui::Painter, r: Rect, fill: &Fill) {
    if let Some(g) = &fill.gradient {
        gradient_ui::paint_ramp(painter, r, g);
        return;
    }
    painter.rect_filled(r, 0.0, Color32::WHITE);
    // A mesh: the painter's rectangles are feathered away at one pixel.
    let mut mesh = egui::Mesh::default();
    for y in 0..r.height().ceil() as i64 {
        for x in 0..r.width().ceil() as i64 {
            let c = match (&fill.pattern, &fill.dither) {
                (Some(p), _) => p
                    .image
                    .get(
                        x.rem_euclid(i64::from(p.image.width().max(1))),
                        y.rem_euclid(i64::from(p.image.height().max(1))),
                    )
                    .unwrap_or(Rgba::TRANSPARENT),
                (_, Some(tile)) if fill.dither_transparent && (x + y) % 2 == 1 => Rgba { a: 0, ..tile[0] },
                (_, Some(tile)) => web_dither_at(tile, x as f64, y as f64),
                _ => fill.color,
            };
            let at = r.min + vec2(x as f32, y as f32);
            let pixel = Rect::from_min_size(at, vec2(1.0, 1.0)).intersect(r);
            mesh.add_colored_rect(pixel, Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a));
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    painter.rect_stroke(
        r,
        0.0,
        Stroke::new(1.0, Color32::from_gray(110)),
        egui::StrokeKind::Inside,
    );
}

/// One cell of the Web Dither tile (`None`: transparent).
fn paint_tile_box(painter: &egui::Painter, r: Rect, c: Option<Rgba>) {
    let fill = c.map_or(Color32::from_gray(232), |c| Color32::from_rgb(c.r, c.g, c.b));
    painter.rect_filled(r, 0.0, fill);
    painter.rect_stroke(
        r,
        0.0,
        Stroke::new(1.0, Color32::from_gray(110)),
        egui::StrokeKind::Inside,
    );
}

/// The box of a gradient, Pattern or Web Dither fill.
pub(crate) fn swatch(ui: &mut egui::Ui, size: egui::Vec2, fill: &Fill) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    paint(ui.painter(), r, fill);
    resp
}

/// Open the fill's editor: Edit Gradient for gradients, else the Fill Options window.
pub(crate) fn open(app: &mut FireCraftApp, target: Target, fill: &Fill, resp: &egui::Response) {
    if fill.gradient.is_some() {
        gradient_ui::open(app, target, resp);
    } else {
        open_window(app, target, resp.rect.left_top() - vec2(0.0, 2.0));
    }
}

/// Open the Fill Options window, its bottom left corner at `at`.
pub(crate) fn open_window(app: &mut FireCraftApp, target: Target, at: Pos2) {
    app.fill_window = Some(Window {
        target,
        at,
        preview: None,
    });
}

/// Apply a Fill Options menu choice.
pub(crate) fn choose(app: &mut FireCraftApp, target: Target, pick: FillPick) {
    if matches!(pick, FillPick::Category(PATTERN)) && app.session.options.fill_pattern.is_none() {
        let first = app.pattern_library().first();
        app.session.options.fill_pattern =
            first.map(|(name, image)| new_pattern(&name, image, (0.0, 0.0, 1.0, 1.0)));
    }
    if target == Target::Selection {
        let options = app.session.options.clone();
        if let Some(d) = app.session.active_doc_mut() {
            match pick {
                FillPick::None => d.set_fill_color(None),
                FillPick::Category(c) => d.set_fill_category(c, &options),
            }
        }
        return;
    }
    let o = &mut app.session.options;
    match pick {
        FillPick::None => {
            o.fill = None;
            o.set_fill_category("fc_Solid");
        }
        FillPick::Category(c) => {
            o.fill.get_or_insert(Rgba::new(0x99, 0x99, 0x99, 255));
            o.set_fill_category(c);
        }
    }
}

/// The Tools panel's Fill box: the selected object's fill, or new objects' with none selected.
pub(crate) fn tools_fill_box(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let selected = app
        .session
        .active_doc()
        .and_then(|d| d.selected_attributes().map(|(f, _)| f));
    let target = if selected.is_some() {
        Target::Selection
    } else {
        Target::ToolFill
    };
    let size = vec2(24.0, 14.0);
    let fill = selected.unwrap_or_else(|| app.session.options.new_fill((0.0, 0.0, 64.0, 64.0)));
    if let Some(f) = fill.as_ref().filter(|f| !is_plain(f)) {
        let resp = swatch(ui, size, f).on_hover_text("Fill");
        if resp.clicked() {
            open(app, target, f, &resp);
        }
        return;
    }
    // A solid fill (or none): the colour picker, whose footer offers the other kinds of fill.
    let (r, _) = ui.allocate_exact_size(size, Sense::hover());
    let resp = ui.interact(r, egui::Id::unique("tools_fill_box"), Sense::click());
    let color = fill.as_ref().map(|f| f.color);
    crate::tools_panel::paint_swatch(ui.painter(), r, color);
    let footer = Footer::Fill { kinds: true };
    match cp::popup(ui, &resp, &|_| None, color.map(cp::to_argb), footer) {
        Some(ColorChoice::Color(c)) => set_color(app, target, Some(cp::to_rgba(c))),
        Some(ColorChoice::None) => set_color(app, target, None),
        Some(ColorChoice::Solid) => choose(app, target, FillPick::Category("fc_Solid")),
        Some(ColorChoice::Gradient) => {
            choose(app, target, FillPick::Category("fc_Linear"));
            gradient_ui::open(app, target, &resp);
        }
        Some(ColorChoice::Pattern) => {
            choose(app, target, FillPick::Category(PATTERN));
            open_window(app, target, r.left_top() - vec2(0.0, 2.0));
        }
        Some(ColorChoice::WebDither) => {
            choose(app, target, FillPick::Category(WEB_DITHER));
            open_window(app, target, r.left_top() - vec2(0.0, 2.0));
        }
        _ => {}
    }
}

/// The Tools panel's Stroke box: the selected object's stroke, or new objects' with none selected.
pub(crate) fn tools_stroke_box(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let selected = app
        .session
        .active_doc()
        .and_then(|d| d.selected_attributes().map(|(_, s)| s));
    let o = &app.session.options;
    let (color, placement) = match &selected {
        Some(s) => (s.as_ref().map(|s| s.color), s.as_ref().map(|s| s.placement)),
        None => (o.stroke, o.stroke.map(|_| o.stroke_placement)),
    };
    let footer = Footer::Stroke {
        placement,
        enabled: placement.is_some(),
        options: true,
    };
    let on_selection = selected.is_some();
    let at = ui.next_widget_position() - vec2(0.0, 2.0);
    let choice = cp::color_box(ui, "tools_stroke_box", vec2(24.0, 14.0), color, footer);
    let color = match choice {
        Some(ColorChoice::Color(c)) => Some(cp::to_rgba(c)),
        Some(ColorChoice::None) => None,
        Some(ColorChoice::Placement(k)) => {
            app.session.options.stroke_placement = k;
            if on_selection && let Some(d) = app.session.active_doc_mut() {
                d.edit_stroke("Stroke Placement", false, move |s| s.placement = k);
            }
            return;
        }
        Some(ColorChoice::StrokeOptions) => {
            let target = if on_selection {
                crate::stroke_panel::Target::Selection
            } else {
                crate::stroke_panel::Target::Tool
            };
            app.stroke_window = Some(StrokeWindow { target, at });
            return;
        }
        _ => return,
    };
    if on_selection && let Some(d) = app.session.active_doc_mut() {
        d.set_stroke_color(color);
    }
    app.session.options.stroke = color;
}

/// The Stroke Options window, while open.
pub struct StrokeWindow {
    target: crate::stroke_panel::Target,
    at: Pos2,
}

/// Draw the Stroke Options window if open (the Property inspector's stroke controls); closes on
/// Enter, a click outside it, or when the selection it edits goes.
pub(crate) fn stroke_window(app: &mut FireCraftApp, ctx: &egui::Context) {
    let Some(state) = app.stroke_window.take() else {
        return;
    };
    let lost = state.target == crate::stroke_panel::Target::Selection
        && app
            .session
            .active_doc()
            .is_none_or(|d| d.selected_attributes().is_none());
    if lost {
        return;
    }
    let shown = egui::Area::new(egui::Id::unique("stroke_options_window"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(state.at)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                crate::stroke_panel::show_in(app, ui, state.target, "window")
            });
        });
    let area = shown.response.rect;
    let (pressed, at, enter) = ctx.input(|i| {
        (
            i.pointer.any_pressed(),
            i.pointer.interact_pos(),
            i.key_pressed(egui::Key::Enter),
        )
    });
    let outside = pressed
        && at.is_some_and(|p| !area.contains(p))
        && !cp::any_open(ctx)
        && !egui::Popup::is_any_open(ctx);
    if !(enter || outside) {
        app.stroke_window = Some(state);
    }
}

/// The Tools panel's colour buttons: new objects' colours and the selection's, in one step.
pub(crate) fn set_tool_colors(app: &mut FireCraftApp, label: &str, stroke: Option<Rgba>, fill: Option<Rgba>) {
    if let Some(d) = app.session.active_doc_mut()
        && d.selected_attributes().is_some()
    {
        d.set_colors(label, stroke, fill);
    }
    let o = &mut app.session.options;
    o.stroke = stroke;
    o.fill = fill;
}

/// The Tools panel's No Fill button.
pub(crate) fn tools_no_fill(app: &mut FireCraftApp) {
    let target = if app
        .session
        .active_doc()
        .is_some_and(|d| d.selected_attributes().is_some())
    {
        Target::Selection
    } else {
        Target::ToolFill
    };
    set_color(app, target, None);
}

fn set_color(app: &mut FireCraftApp, target: Target, color: Option<Rgba>) {
    if target == Target::Selection
        && let Some(d) = app.session.active_doc_mut()
    {
        d.set_fill_color(color);
    }
    app.session.options.fill = color;
}

/// The Fill Options window, while open.
pub struct Window {
    target: Target,
    at: Pos2,
    /// The Pattern preview's texture and the pattern it shows.
    preview: Option<(String, egui::TextureHandle)>,
}

#[derive(Default)]
struct Actions {
    pick: Option<FillPick>,
    pattern: Option<String>,
    other: bool,
    source: Option<Rgba>,
    dither: Option<[Rgba; 4]>,
    transparent: Option<bool>,
    angle: Option<(f64, egui::Response)>,
}

fn current(app: &FireCraftApp, target: Target) -> Option<Option<Fill>> {
    if target == Target::Selection {
        return app.session.active_doc()?.selected_fill().map(|f| f.cloned());
    }
    Some(app.session.options.new_fill((0.0, 0.0, 64.0, 64.0)))
}

/// The Pattern preview's size (FirePixel's pop-up).
const PREVIEW: egui::Vec2 = vec2(250.0, 123.0);

fn preview_texture(ui: &egui::Ui, p: &firecraft_raster::pattern::PatternFill) -> egui::TextureHandle {
    let (w, h) = (p.image.width() as usize, p.image.height() as usize);
    let pixels = (0..h as i64)
        .flat_map(|y| (0..w as i64).map(move |x| (x, y)))
        .map(|(x, y)| {
            let c = p.image.get(x, y).unwrap_or(Rgba::TRANSPARENT);
            Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
        })
        .collect();
    ui.ctx().load_texture(
        "pattern_preview",
        egui::ColorImage::new([w, h], pixels),
        egui::TextureOptions::NEAREST,
    )
}

fn body(
    ui: &mut egui::Ui,
    fill: Option<&Fill>,
    names: &[String],
    state: &mut Window,
    angle: Option<f64>,
    act: &mut Actions,
) {
    let target = state.target;
    let label = fill.map_or("None", |f| fill_label(&f.category));
    ui.horizontal(|ui| {
        ui.label("Fill:");
        act.pick = fill_options(ui, "fill_window_kind", label, false);
    });
    let Some(f) = fill else {
        return;
    };
    if let Some(p) = &f.pattern {
        ui.horizontal(|ui| {
            ui.label("Pattern:");
            ComboBox::from_id_salt("fill_window_pattern")
                .width(190.0)
                .height(400.0)
                .selected_text(pattern_label(&p.name))
                .show_ui(ui, |ui| {
                    for name in names {
                        if ui
                            .selectable_label(name == pattern_label(&p.name), name)
                            .clicked()
                        {
                            act.pattern = Some(name.clone());
                        }
                    }
                    ui.separator();
                    if ui.selectable_label(false, "Other...").clicked() {
                        act.other = true;
                    }
                });
        });
        let (r, _) = ui.allocate_exact_size(PREVIEW, Sense::hover());
        let texture = match &state.preview {
            Some((name, texture)) if *name == p.name => texture.clone(),
            _ => {
                let texture = preview_texture(ui, p);
                state.preview = Some((p.name.clone(), texture.clone()));
                texture
            }
        };
        let painter = ui.painter().with_clip_rect(r);
        let tile = vec2(p.image.width().max(1) as f32, p.image.height().max(1) as f32);
        let uv = Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
        for row in 0..(PREVIEW.y / tile.y).ceil() as i32 {
            for col in 0..(PREVIEW.x / tile.x).ceil() as i32 {
                let at = r.min + vec2(col as f32 * tile.x, row as f32 * tile.y);
                painter.image(texture.id(), Rect::from_min_size(at, tile), uv, Color32::WHITE);
            }
        }
        painter.rect_stroke(
            r,
            0.0,
            Stroke::new(1.0, Color32::from_gray(110)),
            egui::StrokeKind::Inside,
        );
        ui.add_enabled_ui(angle.is_some(), |ui| {
            ui.horizontal(|ui| {
                ui.label("Angle:");
                let mut value = angle.unwrap_or(0.0).round();
                let field = egui::DragValue::new(&mut value)
                    .range(0.0..=360.0)
                    .max_decimals(0)
                    .suffix("\u{b0}");
                let resp = ui.add(field);
                act.angle = Some((value.rem_euclid(360.0), resp));
            });
        });
    }
    if let Some(tile) = f.dither {
        let size = vec2(18.0, 14.0);
        ui.horizontal(|ui| {
            ui.label("Source:");
            if let Some(ColorChoice::Color(c)) =
                cp::color_box(ui, "web_dither_source", size, Some(f.color), Footer::Plain)
            {
                act.source = Some(cp::to_rgba(c));
            }
            ui.add_space(6.0);
            // Fireworks' 2 x 2 tile: the right-hand boxes can be edited.
            ui.vertical(|ui| {
                for row in 0..2 {
                    ui.horizontal(|ui| {
                        let shade = if f.dither_transparent && row == 1 {
                            None
                        } else {
                            Some(tile[row * 2])
                        };
                        let (r, _) = ui.allocate_exact_size(size, Sense::hover());
                        paint_tile_box(ui.painter(), r, shade);
                        if f.dither_transparent && row == 0 {
                            let (r, _) = ui.allocate_exact_size(size, Sense::hover());
                            paint_tile_box(ui.painter(), r, None);
                            return;
                        }
                        let id = ("web_dither_box", row);
                        if let Some(ColorChoice::Color(c)) =
                            cp::color_box(ui, id, size, Some(tile[row * 2 + 1]), Footer::Plain)
                        {
                            let c = cp::to_rgba(c);
                            let mut new = if f.dither_transparent { [c; 4] } else { tile };
                            new[row * 2 + 1] = c;
                            new[(1 - row) * 2] = c;
                            act.dither = Some(new);
                        }
                    });
                }
            });
        });
        if target == Target::Selection {
            let mut on = f.dither_transparent;
            if ui.checkbox(&mut on, "Transparent").changed() {
                act.transparent = Some(on);
            }
        }
    }
}

fn set_pattern(app: &mut FireCraftApp, target: Target, name: &str, image: firecraft_raster::RgbaImage) {
    if target == Target::Selection
        && let Some(d) = app.session.active_doc_mut()
    {
        d.set_fill_pattern(name, &image);
    }
    app.session.options.fill_pattern = Some(new_pattern(name, image, (0.0, 0.0, 1.0, 1.0)));
}

/// A pattern file chosen with Other..., now or on a later frame (the browser).
fn receive_pattern(app: &mut FireCraftApp, target: Target, file: Result<(String, Vec<u8>), String>) {
    let added = file.and_then(|(name, bytes)| app.patterns_mut().add(&name, bytes));
    match added {
        Ok((name, image)) => set_pattern(app, target, &name, image),
        Err(message) => app.dialogs.message = Some(("Pattern".into(), message)),
    }
}

/// Draw the window if open; closes on Enter or a click outside it, as Fireworks' does.
pub(crate) fn window(app: &mut FireCraftApp, ctx: &egui::Context) {
    if let Some(target) = app.awaiting_pattern
        && let Some(file) = app.platform.take_image()
    {
        app.awaiting_pattern = None;
        receive_pattern(app, target, file);
    }
    let Some(state) = app.fill_window.take() else {
        return;
    };
    let Some(fill) = current(app, state.target) else {
        return;
    };
    let mut state = state;
    let names = app.pattern_library().names();
    let angle = (state.target == Target::Selection)
        .then(|| {
            app.session
                .active_doc()
                .and_then(firecraft_engine::DocState::fill_angle)
        })
        .flatten();
    let mut act = Actions::default();
    let shown = egui::Area::new(egui::Id::unique("fill_options_window"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(state.at)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                body(ui, fill.as_ref(), &names, &mut state, angle, &mut act)
            });
        });
    let selection = state.target == Target::Selection;
    if let Some(pick) = act.pick {
        choose(app, state.target, pick);
        match pick {
            FillPick::Category(PATTERN | WEB_DITHER) => {}
            FillPick::Category(c) if firecraft_raster::gradient::GradientKind::from_category(c).is_some() => {
                gradient_ui::open_at(app, state.target, state.at);
                return;
            }
            // Solid or None: back to the box, whose colour picker now applies.
            _ => return,
        }
    }
    if let Some(name) = act.pattern
        && let Some((stem, image)) = app.pattern_library().get(&name)
    {
        set_pattern(app, state.target, &stem, image);
    }
    if act.other {
        match app.platform.open_image() {
            Some(file) => receive_pattern(app, state.target, Ok(file)),
            None => app.awaiting_pattern = Some(state.target),
        }
    }
    if let Some(c) = act.source {
        set_color(app, state.target, Some(c));
    }
    if selection && let Some(d) = app.session.active_doc_mut() {
        if let Some(tile) = act.dither {
            d.set_web_dither(tile);
        }
        if let Some(on) = act.transparent {
            d.set_web_dither_transparent(on);
        }
        if let Some((a, resp)) = &act.angle {
            let a = *a;
            crate::inspector::value_drag(d, resp, "Fill Options", move |d, live| d.set_fill_angle(a, live));
        }
    }
    let area = shown.response.rect;
    let (pressed, at, enter) = ctx.input(|i| {
        (
            i.pointer.any_pressed(),
            i.pointer.interact_pos(),
            i.key_pressed(egui::Key::Enter),
        )
    });
    let outside = pressed
        && at.is_some_and(|p| !area.contains(p))
        && !cp::any_open(ctx)
        && !egui::Popup::is_any_open(ctx);
    if !(enter || outside) {
        app.fill_window = Some(state);
    }
}
