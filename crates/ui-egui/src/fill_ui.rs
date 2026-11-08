//! Fill controls shared by the Tools panel and the Property inspector: the box showing a fill of
//! any kind, the Fill Options menu and Fireworks MX's Fill Options window for Pattern and Web
//! Dither fills (docs/behavior/gradients.md).

use egui::{Color32, ComboBox, Pos2, Rect, Sense, Stroke, vec2};
use firecraft_doc::Fill;
use firecraft_engine::{PATTERN, fill_label, new_pattern};
use firecraft_io::textures::pattern_label;
use firecraft_raster::Rgba;
use firecraft_raster::pattern::web_dither_at;

use crate::FireCraftApp;
use crate::fpui::color_picker::{self as cp, ColorChoice, Footer};
use crate::gradient_ui::{self, Target};

/// What a Fill Options menu chose.
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
    app.fill_window = Some(Window { target, at });
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
        .and_then(|d| d.selected_fill().map(|f| f.cloned()));
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
    let at = ui.next_widget_position() + vec2(30.0, 40.0);
    let color = fill.as_ref().map(|f| f.color);
    let choice = cp::color_box(ui, "tools_fill_box", size, color, Footer::FillOptions);
    match choice {
        Some(ColorChoice::FillOptions) => open_window(app, target, at),
        Some(ColorChoice::Color(c)) => set_color(app, target, Some(cp::to_rgba(c))),
        Some(ColorChoice::None) => set_color(app, target, None),
        _ => {}
    }
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
}

#[derive(Default)]
struct Actions {
    pick: Option<FillPick>,
    pattern: Option<String>,
    other: bool,
    source: Option<Rgba>,
    dither: Option<[Rgba; 4]>,
    transparent: Option<bool>,
    edit_gradient: Option<egui::Response>,
}

fn current(app: &FireCraftApp, target: Target) -> Option<Option<Fill>> {
    if target == Target::Selection {
        return app.session.active_doc()?.selected_fill().map(|f| f.cloned());
    }
    Some(app.session.options.new_fill((0.0, 0.0, 64.0, 64.0)))
}

fn body(ui: &mut egui::Ui, fill: Option<&Fill>, names: &[String], target: Target, act: &mut Actions) {
    ui.set_min_width(190.0);
    let label = fill.map_or("None", |f| fill_label(&f.category));
    act.pick = fill_options(ui, "fill_window_kind", label, false);
    let Some(f) = fill else {
        return;
    };
    if let Some(p) = &f.pattern {
        ComboBox::from_id_salt("fill_window_pattern")
            .width(170.0)
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
    if f.gradient.is_some() {
        let resp = swatch(ui, vec2(170.0, 14.0), f).on_hover_text("Edit Gradient");
        if resp.clicked() {
            act.edit_gradient = Some(resp);
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
    let names = app.pattern_library().names();
    let mut act = Actions::default();
    let shown = egui::Area::new(egui::Id::new("fill_options_window"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(state.at)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style())
                .show(ui, |ui| body(ui, fill.as_ref(), &names, state.target, &mut act));
        });
    let selection = state.target == Target::Selection;
    if let Some(pick) = act.pick {
        choose(app, state.target, pick);
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
    }
    if let Some(resp) = act.edit_gradient {
        gradient_ui::open(app, state.target, &resp);
        return;
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
