//! Gradient fills in the interface: the ramp swatch, Fireworks' Edit Gradient pop-up and the fill
//! handles on the canvas (docs/behavior/gradients.md).

use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, pos2, vec2};
use firecraft_engine::{DocState, GRADIENT_PRESETS, opaque_ramp, preset_colors};
use firecraft_raster::Rgba;
use firecraft_raster::gradient::Gradient;

use crate::FireCraftApp;
use crate::fpui::color_picker::{self as cp, ColorChoice, Footer};

/// What the pop-up edits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    /// The selected objects' gradient.
    Selection,
    /// The gradient new objects get.
    ToolFill,
    /// The Gradient tool's.
    GradientTool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stop {
    Color(usize),
    Opacity(usize),
}

/// The Edit Gradient pop-up, while open.
pub struct Popup {
    target: Target,
    at: Pos2,
    selected: Option<Stop>,
    dragging: Option<Stop>,
    /// The Fill Options button was pressed.
    to_options: bool,
}

fn color32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// The ramp, over a checkerboard where it's transparent.
pub(crate) fn paint_ramp(painter: &egui::Painter, r: Rect, g: &Gradient) {
    let square = 5.0;
    let (cols, rows) = (
        (r.width() / square).ceil() as i32,
        (r.height() / square).ceil() as i32,
    );
    painter.rect_filled(r, 0.0, Color32::WHITE);
    for y in 0..rows {
        for x in 0..cols {
            if (x + y) % 2 == 1 {
                let c = Rect::from_min_size(
                    r.min + vec2(x as f32 * square, y as f32 * square),
                    vec2(square, square),
                )
                .intersect(r);
                painter.rect_filled(c, 0.0, Color32::from_gray(204));
            }
        }
    }
    let n = r.width().ceil().max(1.0) as i32;
    for i in 0..n {
        let c = g.color_on_ramp((i as f32 + 0.5) / n as f32);
        let x = r.left() + i as f32;
        let col = Rect::from_min_max(pos2(x, r.top()), pos2((x + 1.0).min(r.right()), r.bottom()));
        painter.rect_filled(col, 0.0, color32(c));
    }
    painter.rect_stroke(
        r,
        0.0,
        Stroke::new(1.0, Color32::from_gray(110)),
        egui::StrokeKind::Inside,
    );
}

/// A gradient's colour box.
pub fn swatch(ui: &mut egui::Ui, size: egui::Vec2, g: &Gradient) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    paint_ramp(ui.painter(), r, g);
    resp
}

/// Open the pop-up under `resp`. The selection's edit becomes one History state when it closes.
pub fn open(app: &mut FireCraftApp, target: Target, resp: &egui::Response) {
    if target == Target::Selection
        && let Some(d) = app.session.active_doc_mut()
    {
        d.begin_live_edit("Edit Gradient");
    }
    app.gradient_popup = Some(Popup {
        target,
        // Opens upward: the Property inspector sits at the bottom of the window.
        at: resp.rect.left_top() - vec2(0.0, 2.0),
        selected: None,
        dragging: None,
        to_options: false,
    });
}

fn current(app: &FireCraftApp, target: Target) -> Option<Gradient> {
    match target {
        Target::Selection => app
            .session
            .active_doc()?
            .selected_gradient()
            .map(|(_, g)| g.clone()),
        Target::ToolFill => app.session.options.fill_gradient.clone(),
        Target::GradientTool => Some(app.session.options.gradient_tool.clone()),
    }
}

/// Draw the pop-up if open; closes on Enter or a click outside it.
pub fn popup(app: &mut FireCraftApp, ctx: &egui::Context) {
    let Some(mut state) = app.gradient_popup.take() else {
        return;
    };
    let Some(mut g) = current(app, state.target) else {
        if let Some(d) = app.session.active_doc_mut() {
            d.commit_live_edit();
        }
        return;
    };
    let before = g.clone();
    let shown = egui::Area::new(egui::Id::new("edit_gradient"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(state.at)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| editor(ui, &mut g, &mut state))
        });
    if g != before {
        let (colors, opacity) = (g.colors.clone(), g.opacity.clone());
        match state.target {
            Target::Selection => {
                if let Some(d) = app.session.active_doc_mut() {
                    d.edit_gradient("Edit Gradient", true, |x| {
                        x.colors.clone_from(&colors);
                        x.opacity.clone_from(&opacity);
                    });
                }
            }
            Target::ToolFill => app.session.options.fill_gradient = Some(g),
            Target::GradientTool => app.session.options.gradient_tool = g,
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
    // The colour picker and the preset list are pop-ups of their own, outside this one's area.
    let outside = pressed
        && at.is_some_and(|p| !area.contains(p))
        && !cp::any_open(ctx)
        && !egui::Popup::is_any_open(ctx);
    if enter || outside || state.to_options {
        if state.to_options {
            crate::fill_ui::open_window(app, state.target, state.at);
        }
        if state.target == Target::Selection
            && let Some(d) = app.session.active_doc_mut()
        {
            d.commit_live_edit();
        }
    } else {
        app.gradient_popup = Some(state);
    }
}

/// A band of stops above or below the ramp. `make(t)`: a new stop's value at ramp position `t`.
fn band<T: Copy>(
    ui: &mut egui::Ui,
    width: f32,
    stops: &mut Vec<(f32, T)>,
    state: &mut Popup,
    wrap: fn(usize) -> Stop,
    make: &dyn Fn(f32) -> T,
    above: bool,
) {
    let (r, resp) = ui.allocate_exact_size(vec2(width, 13.0), Sense::click_and_drag());
    let pos_t = |x: f32| ((x - r.left()) / r.width().max(1.0)).clamp(0.0, 1.0);
    let index_of = |s: Option<Stop>| {
        s.and_then(|s| {
            let i = match s {
                Stop::Color(i) | Stop::Opacity(i) => i,
            };
            (s == wrap(i)).then_some(i)
        })
    };
    if resp.is_pointer_button_down_on()
        && ui.input(|i| i.pointer.primary_pressed())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let near = stops
            .iter()
            .enumerate()
            .map(|(i, s)| (i, (r.left() + s.0 * r.width() - p.x).abs()))
            .filter(|(_, d)| *d <= 6.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i);
        let i = near.unwrap_or_else(|| {
            let t = pos_t(p.x);
            let i = stops.iter().position(|s| s.0 > t).unwrap_or(stops.len());
            stops.insert(i, (t, make(t)));
            i
        });
        state.selected = Some(wrap(i));
        state.dragging = Some(wrap(i));
    }
    if let Some(mut i) = index_of(state.dragging) {
        if resp.dragged()
            && let Some(p) = resp.interact_pointer_pos()
            && let Some(stop) = stops.get_mut(i)
        {
            stop.0 = pos_t(p.x);
            // Keep the list in order as a stop passes its neighbours.
            while i > 0 && stops[i - 1].0 > stops[i].0 {
                stops.swap(i - 1, i);
                i -= 1;
            }
            while i + 1 < stops.len() && stops[i + 1].0 < stops[i].0 {
                stops.swap(i + 1, i);
                i += 1;
            }
            state.selected = Some(wrap(i));
            state.dragging = Some(wrap(i));
        }
        if resp.drag_stopped() || (!resp.dragged() && !resp.is_pointer_button_down_on()) {
            // Dragged away from the ramp: removed (at least two stops stay).
            let away = ui
                .input(|i| i.pointer.interact_pos())
                .is_some_and(|p| resp.drag_stopped() && !r.expand2(vec2(40.0, 40.0)).contains(p));
            if away && stops.len() > 2 {
                stops.remove(i);
                state.selected = None;
            }
            state.dragging = None;
        }
    }
    // Swatches: a little house pointing at the ramp.
    let selected = index_of(state.selected);
    for (i, s) in stops.iter().enumerate() {
        let x = r.left() + s.0 * r.width();
        let (tip, base) = if above {
            (r.bottom(), r.top())
        } else {
            (r.top(), r.bottom())
        };
        let shape = vec![
            pos2(x, tip),
            pos2(x + 5.0, (tip + base) / 2.0),
            pos2(x + 5.0, base),
            pos2(x - 5.0, base),
            pos2(x - 5.0, (tip + base) / 2.0),
        ];
        let fill = if selected == Some(i) {
            Color32::from_rgb(90, 160, 255)
        } else {
            Color32::WHITE
        };
        ui.painter().add(Shape::convex_polygon(
            shape,
            fill,
            Stroke::new(1.0, Color32::from_gray(70)),
        ));
    }
}

fn editor(ui: &mut egui::Ui, g: &mut Gradient, state: &mut Popup) {
    const WIDTH: f32 = 220.0;
    if g.opacity.is_empty() {
        g.opacity = opaque_ramp();
    }
    ui.spacing_mut().item_spacing.y = 1.0;
    let snapshot = g.clone();
    let lerp_opacity = move |t: f32| snapshot.color_on_ramp(t).a;
    band(
        ui,
        WIDTH,
        &mut g.opacity,
        state,
        Stop::Opacity,
        &lerp_opacity,
        true,
    );
    let (ramp, _) = ui.allocate_exact_size(vec2(WIDTH, 24.0), Sense::hover());
    paint_ramp(ui.painter(), ramp, g);
    let snapshot = colour_only(g);
    let lerp_color = move |t: f32| Rgba {
        a: 255,
        ..snapshot.color_on_ramp(t)
    };
    band(ui, WIDTH, &mut g.colors, state, Stop::Color, &lerp_color, false);
    ui.add_space(6.0);
    ui.horizontal(|ui| match state.selected {
        Some(Stop::Color(i)) if i < g.colors.len() => {
            ui.label("Color:");
            let c = g.colors[i].1;
            if let Some(ColorChoice::Color(new)) =
                cp::color_box(ui, "gradient_stop", vec2(22.0, 14.0), Some(c), Footer::Plain)
            {
                g.colors[i].1 = Rgba {
                    a: 255,
                    ..cp::to_rgba(new)
                };
            }
        }
        Some(Stop::Opacity(i)) if i < g.opacity.len() => {
            ui.label("Opacity:");
            let mut pct = (g.opacity[i].1 as f32 / 255.0 * 100.0).round();
            if ui
                .add(
                    egui::DragValue::new(&mut pct)
                        .range(0.0..=100.0)
                        .max_decimals(0)
                        .suffix("%"),
                )
                .changed()
            {
                g.opacity[i].1 = (pct / 100.0 * 255.0).round() as u8;
            }
        }
        _ => {
            ui.weak("Click a swatch to edit it; beside the ramp to add one.");
        }
    });
    ui.add_space(4.0);
    egui::ComboBox::from_id_salt("gradient_preset")
        .width(WIDTH - 8.0)
        .selected_text("Preset")
        .show_ui(ui, |ui| {
            for (name, stops) in GRADIENT_PRESETS {
                if ui.selectable_label(false, name).clicked() {
                    g.colors = preset_colors(stops);
                    g.opacity = opaque_ramp();
                    state.selected = None;
                }
            }
        });
    if state.target != Target::GradientTool {
        ui.add_space(4.0);
        if ui.button("Fill Options...").clicked() {
            state.to_options = true;
        }
    }
}

/// `g` without its opacity ramp.
fn colour_only(g: &Gradient) -> Gradient {
    Gradient {
        opacity: Vec::new(),
        ..g.clone()
    }
}

// ---- Handles on the canvas -------------------------------------------------------------------

type Pt = (f64, f64);

/// Screen distance within which a handle is hit.
const HIT: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Grip {
    /// The round handle: the whole fill moves.
    Move,
    End,
    Side,
    /// A connecting line: rotates about the round handle.
    Rotate,
}

/// A fill handle being dragged.
pub struct FillDrag {
    grip: Grip,
    press: Pt,
    orig: Trio,
    moved: bool,
}

/// The round, end and side handles.
type Trio = (Pt, Pt, Pt);

/// The handles after dragging `grip` from `press` to `cur` (Fireworks MX).
fn dragged((start, end, side): Trio, grip: Grip, press: Pt, cur: Pt) -> Trio {
    let (dx, dy) = (cur.0 - press.0, cur.1 - press.1);
    let moved = |p: Pt| (p.0 + dx, p.1 + dy);
    match grip {
        Grip::Move => (moved(start), moved(end), moved(side)),
        Grip::End => (start, moved(end), side),
        Grip::Side => (start, end, moved(side)),
        Grip::Rotate => {
            let (ox, oy) = start;
            let angle = (cur.1 - oy).atan2(cur.0 - ox) - (press.1 - oy).atan2(press.0 - ox);
            let (sin, cos) = angle.sin_cos();
            let turn = |p: Pt| {
                let (x, y) = (p.0 - ox, p.1 - oy);
                (ox + x * cos - y * sin, oy + x * sin + y * cos)
            };
            (start, turn(end), turn(side))
        }
    }
}

fn distance_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let d = b - a;
    let t = if d.length_sq() > 0.0 {
        ((p - a).dot(d) / d.length_sq()).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + d * t).distance(p)
}

/// The selected gradient's or pattern's handles: shows them and takes presses on them. `true` while they have
/// the mouse (the tool underneath then does nothing).
pub fn handles(
    drag: &mut Option<FillDrag>,
    d: &mut DocState,
    painter: &egui::Painter,
    resp: &egui::Response,
    (origin, z): (Pos2, f32),
) -> bool {
    let ctx = resp.ctx.clone();
    let to_screen = |q: Pt| pos2(origin.x + q.0 as f32 * z, origin.y + q.1 as f32 * z);
    let to_doc = |q: Pos2| (((q.x - origin.x) / z) as f64, ((q.y - origin.y) / z) as f64);
    let single = d.selection.len() == 1;
    let Some(h) = d.selected_fill_handles().filter(|_| single) else {
        *drag = None;
        return false;
    };
    let (start, end, side) = (to_screen(h.start), to_screen(h.end), to_screen(h.side));
    let uses_side = h.uses_side;

    let (pressed, down, origin_at, now) = ctx.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.press_origin(),
            i.pointer.interact_pos(),
        )
    });
    if pressed
        && resp.hovered()
        && drag.is_none()
        && let Some(s) = origin_at.or(now)
    {
        let grip = if s.distance(start) <= HIT {
            Some(Grip::Move)
        } else if s.distance(end) <= HIT {
            Some(Grip::End)
        } else if uses_side && s.distance(side) <= HIT {
            Some(Grip::Side)
        } else if distance_to_segment(s, start, end) <= 3.0
            || (uses_side && distance_to_segment(s, start, side) <= 3.0)
        {
            Some(Grip::Rotate)
        } else {
            None
        };
        if let Some(grip) = grip {
            d.begin_live_edit("Move Fill Handle");
            *drag = Some(FillDrag {
                grip,
                press: to_doc(s),
                orig: (h.start, h.end, h.side),
                moved: false,
            });
        }
    }
    let mut busy = false;
    if let Some(active) = drag.as_mut() {
        busy = true;
        if down && let Some(s) = now {
            let cur = to_doc(s);
            if cur != active.press || active.moved {
                active.moved = true;
                let new = dragged(active.orig, active.grip, active.press, cur);
                d.set_fill_handles(true, new.0, new.1, new.2);
                ctx.request_repaint();
            }
        }
        if !down {
            if active.moved {
                d.commit_live_edit();
            } else {
                d.discard_live_edit();
            }
            *drag = None;
        }
    }
    // Black lines with a white edge; a white diamond at the start and black squares (Fireworks).
    let line = |a: Pos2, b: Pos2| {
        painter.line_segment([a, b], Stroke::new(3.0, Color32::from_white_alpha(140)));
        painter.line_segment([a, b], Stroke::new(1.0, Color32::BLACK));
    };
    let square = |p: Pos2| {
        let r = Rect::from_center_size(p, vec2(6.0, 6.0));
        painter.rect_filled(r, 0.0, Color32::BLACK);
        painter.rect_stroke(
            r,
            0.0,
            Stroke::new(1.0, Color32::WHITE),
            egui::StrokeKind::Outside,
        );
    };
    line(start, end);
    if uses_side {
        line(start, side);
        square(side);
    }
    square(end);
    let k = 5.0;
    let diamond = vec![
        start + vec2(0.0, -k),
        start + vec2(k, 0.0),
        start + vec2(0.0, k),
        start + vec2(-k, 0.0),
    ];
    painter.add(Shape::convex_polygon(
        diamond,
        Color32::WHITE,
        Stroke::new(1.0, Color32::BLACK),
    ));
    busy
}

#[cfg(test)]
mod tests {
    use super::*;
    use firecraft_engine::new_gradient;
    use firecraft_raster::gradient::GradientKind;

    /// The Ellipse fill the drags were recorded on in Fireworks MX.
    fn trio(g: &Gradient) -> Trio {
        (g.start, g.end, g.side)
    }

    fn ellipse() -> Gradient {
        let mut g = new_gradient(
            GradientKind::Elliptical,
            vec![(0.0, Rgba::BLACK), (1.0, Rgba::WHITE)],
            opaque_ramp(),
            (120.0, 100.0, 160.0, 80.0),
        );
        g.side = (215.0, 85.0);
        g
    }

    #[test]
    fn the_round_handle_moves_the_whole_fill() {
        let g = new_gradient(
            GradientKind::Radial,
            vec![],
            opaque_ramp(),
            (60.0, 80.0, 240.0, 120.0),
        );
        let m = dragged(trio(&g), Grip::Move, (180.0, 140.0), (200.0, 150.0));
        assert_eq!((m.0, m.1, m.2), ((200.0, 150.0), (320.0, 150.0), (200.0, 90.0)));
    }

    #[test]
    fn a_square_handle_moves_only_itself() {
        let g = ellipse();
        let m = dragged(trio(&g), Grip::End, (280.0, 140.0), (300.0, 150.0));
        assert_eq!((m.0, m.1, m.2), (g.start, (300.0, 150.0), g.side));
        let m = dragged(trio(&g), Grip::Side, g.side, (230.0, 70.0));
        assert_eq!((m.0, m.1, m.2), (g.start, g.end, (230.0, 70.0)));
    }

    #[test]
    fn dragging_a_line_rotates_about_the_round_handle() {
        // Recorded in Fireworks MX: 240,140 to 240,175 gave 260.38,192.48 and 247.40,108.33.
        let g = ellipse();
        let m = dragged(trio(&g), Grip::Rotate, (240.0, 140.0), (240.0, 175.0));
        assert!(
            (m.1.0 - 260.38).abs() < 0.5 && (m.1.1 - 192.48).abs() < 0.5,
            "{:?}",
            m.1
        );
        assert!(
            (m.2.0 - 247.40).abs() < 0.5 && (m.2.1 - 108.33).abs() < 0.5,
            "{:?}",
            m.2
        );
        assert_eq!(m.0, g.start);
    }

    #[test]
    fn the_third_handle_shows_only_where_it_matters() {
        let make = |kind| new_gradient(kind, vec![], opaque_ramp(), (0.0, 0.0, 10.0, 10.0)).uses_side();
        assert!(make(GradientKind::Elliptical) && make(GradientKind::Rectangular));
        assert!(!make(GradientKind::Radial) && !make(GradientKind::Linear));
    }
}
