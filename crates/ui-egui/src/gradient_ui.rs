//! Gradient fills in the interface: the ramp swatch, Fireworks' Edit Gradient pop-up and the fill
//! handles on the canvas (docs/behavior/gradients.md).

use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, pos2, vec2};
use firecraft_engine::{
    DocState, GRADIENT_FILLS, GRADIENT_PRESETS, PATTERN, WEB_DITHER, opaque_ramp, preset_colors, preset_name,
};
use firecraft_raster::Rgba;
use firecraft_raster::gradient::{Gradient, GradientKind};

use crate::FireCraftApp;
use crate::fill_ui::{FillPick, fill_options};
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
struct Stop {
    /// In the opacity row above the ramp (else the colour row below it).
    opacity: bool,
    index: usize,
}

/// The gradient pop-up, while open.
pub struct Popup {
    target: Target,
    at: Pos2,
    selected: Option<Stop>,
    dragging: Option<Stop>,
    /// The colour stop whose colour picker is open.
    picker: Option<usize>,
    /// The selection's live edit holds a change.
    changed: bool,
}

/// Size of the pop-up's contents (FirePixel's layout, with Fireworks MX's Preset row).
const POPUP_SIZE: egui::Vec2 = vec2(200.0, 156.0);

/// How far past the stops' area a stop must be dragged to be removed.
const STOP_TEAR_OFF: f32 = 24.0;

/// The ramp, over a checkerboard where it's transparent. A single mesh, so it stays smooth at
/// fractional display scales (rectangles a point wide leave seams there).
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
    let n = r.width().ceil().max(1.0) as u32;
    let mut mesh = egui::Mesh::default();
    for i in 0..=n {
        let t = i as f32 / n as f32;
        let c = g.color_on_ramp(t);
        let col = Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a);
        let x = r.left() + t * r.width();
        mesh.colored_vertex(pos2(x, r.top()), col);
        mesh.colored_vertex(pos2(x, r.bottom()), col);
        if i > 0 {
            let at = 2 * i;
            mesh.add_triangle(at - 2, at - 1, at);
            mesh.add_triangle(at - 1, at, at + 1);
        }
    }
    painter.add(Shape::mesh(mesh));
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

/// Open the pop-up above `resp`. The selection's edit becomes one History state when it closes.
pub fn open(app: &mut FireCraftApp, target: Target, resp: &egui::Response) {
    open_at(app, target, resp.rect.left_top() - vec2(0.0, 2.0));
}

/// Open the pop-up with its bottom left corner at `at`.
pub fn open_at(app: &mut FireCraftApp, target: Target, at: Pos2) {
    if target == Target::Selection
        && let Some(d) = app.session.active_doc_mut()
    {
        d.begin_live_edit("Edit Gradient");
    }
    app.gradient_popup = Some(Popup {
        target,
        at,
        selected: None,
        dragging: None,
        picker: None,
        changed: false,
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

/// End the selection's live edit: a History state only if something changed.
fn finish(app: &mut FireCraftApp, state: &Popup) {
    if state.target == Target::Selection
        && let Some(d) = app.session.active_doc_mut()
    {
        if state.changed {
            d.commit_live_edit();
        } else {
            d.discard_live_edit();
        }
    }
}

/// What the pop-up's controls asked for beyond editing the ramp.
#[derive(Default)]
struct Picked {
    fill: Option<FillPick>,
    angle: Option<f64>,
}

/// Draw the pop-up if open; closes on Enter or a click outside it.
pub fn popup(app: &mut FireCraftApp, ctx: &egui::Context) {
    let Some(mut state) = app.gradient_popup.take() else {
        return;
    };
    let Some(mut g) = current(app, state.target) else {
        finish(app, &state);
        return;
    };
    let before = g.clone();
    let angle = (state.target == Target::Selection)
        .then(|| app.session.active_doc().and_then(DocState::fill_angle))
        .flatten();
    let mut picked = Picked::default();
    let shown = egui::Area::new(egui::Id::unique("edit_gradient"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::LEFT_BOTTOM)
        .fixed_pos(state.at)
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| editor(ui, &mut g, &mut state, angle, &mut picked))
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
                state.changed = true;
            }
            Target::ToolFill => app.session.options.fill_gradient = Some(g),
            Target::GradientTool => app.session.options.gradient_tool = g,
        }
    }
    if let Some(a) = picked.angle
        && let Some(d) = app.session.active_doc_mut()
    {
        d.set_fill_angle(a, true);
        state.changed = true;
    }
    if let Some(pick) = picked.fill {
        match pick {
            FillPick::Category(c) if GradientKind::from_category(c).is_some() => {
                if state.target == Target::GradientTool {
                    let o = &mut app.session.options;
                    o.gradient_category = c.to_owned();
                    if let Some(kind) = GradientKind::from_category(c) {
                        o.gradient_tool.kind = kind;
                    }
                } else {
                    finish(app, &state);
                    crate::fill_ui::choose(app, state.target, pick);
                    if state.target == Target::Selection
                        && let Some(d) = app.session.active_doc_mut()
                    {
                        d.begin_live_edit("Edit Gradient");
                    }
                    state.changed = false;
                }
            }
            // Not a gradient any more: the pop-up of the new kind of fill, if it has one.
            _ => {
                finish(app, &state);
                crate::fill_ui::choose(app, state.target, pick);
                if matches!(pick, FillPick::Category(PATTERN | WEB_DITHER)) {
                    crate::fill_ui::open_window(app, state.target, state.at);
                }
                return;
            }
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
    // The colour picker and the lists are pop-ups of their own, outside this one's area.
    let outside = pressed
        && at.is_some_and(|p| !area.contains(p))
        && !cp::any_open(ctx)
        && !egui::Popup::is_any_open(ctx);
    if enter || outside {
        finish(app, &state);
    } else {
        app.gradient_popup = Some(state);
    }
}

/// Put a stop at `pos`, keeping the list sorted: its new index.
fn reposition<T: Copy>(list: &mut Vec<(f32, T)>, i: usize, pos: f32) -> usize {
    if i >= list.len() {
        return i;
    }
    let value = list.remove(i).1;
    let at = list.partition_point(|s| s.0 <= pos);
    list.insert(at, (pos, value));
    at
}

/// Controls placed at a fixed spot of the pop-up.
fn place(ui: &mut egui::Ui, r: Rect, add: impl FnOnce(&mut egui::Ui)) {
    ui.scope_builder(egui::UiBuilder::new().max_rect(r), add);
}

/// The pop-up's contents (FirePixel's gradient pop-up: kind, ramp with opacity stops above and
/// colour stops below, Reverse and angle; Fireworks MX's Preset list).
fn editor(ui: &mut egui::Ui, g: &mut Gradient, state: &mut Popup, angle: Option<f64>, picked: &mut Picked) {
    if g.opacity.is_empty() {
        g.opacity = opaque_ramp();
    }
    let (all, _) = ui.allocate_exact_size(POPUP_SIZE, Sense::hover());
    let o = all.min;
    let id = ui.scope_id().with("gradient_editor");
    let kind = GRADIENT_FILLS
        .iter()
        .find(|(c, _)| GradientKind::from_category(c) == Some(g.kind))
        .map_or("Linear", |(_, l)| l);
    place(ui, Rect::from_min_size(o, vec2(POPUP_SIZE.x, 20.0)), |ui| {
        ui.horizontal(|ui| {
            ui.label("Fill:");
            let only_gradients = state.target == Target::GradientTool;
            picked.fill = fill_options(ui, "gradient_kind", kind, only_gradients);
        });
    });

    let bar = Rect::from_min_max(o + vec2(16.0, 40.0), o + vec2(192.0, 74.0));
    let area = Rect::from_min_max(
        pos2(bar.left() - 8.0, o.y + 26.0),
        pos2(bar.right() + 8.0, o.y + 90.0),
    );
    let resp = ui.interact(area, id.with("bar"), Sense::click_and_drag());
    paint_ramp(ui.painter(), bar, g);
    let stop_rect = |opacity: bool, p: f32| {
        let x = bar.left() + p.clamp(0.0, 1.0) * bar.width();
        let y = if opacity {
            bar.top() - 7.0
        } else {
            bar.bottom() + 7.0
        };
        Rect::from_center_size(pos2(x, y), vec2(9.0, 11.0))
    };
    let pointer = resp.interact_pointer_pos();
    let torn = |opacity: bool, at: Option<Pos2>| {
        at.is_some_and(|p| {
            if opacity {
                p.y < area.top() - STOP_TEAR_OFF
            } else {
                p.y > area.bottom() + STOP_TEAR_OFF
            }
        })
    };

    // The stops.
    let focus = Color32::from_rgb(0x33, 0x99, 0xff);
    let paint_stop = |stop: Stop, p: f32, fill: Color32, count: usize| {
        if state.dragging == Some(stop) && torn(stop.opacity, pointer) && count > 2 {
            return;
        }
        let r = stop_rect(stop.opacity, p);
        ui.painter().rect_filled(r, 1.0, fill);
        let selected = state.selected == Some(stop);
        ui.painter().rect_stroke(
            r,
            1.0,
            if selected {
                Stroke::new(2.0, focus)
            } else {
                Stroke::new(1.0, Color32::from_gray(60))
            },
            egui::StrokeKind::Inside,
        );
    };
    let count = g.opacity.len();
    for (index, (p, a)) in g.opacity.iter().enumerate() {
        paint_stop(
            Stop { opacity: true, index },
            *p,
            Color32::from_gray(255 - a),
            count,
        );
    }
    let count = g.colors.len();
    for (index, (p, c)) in g.colors.iter().enumerate() {
        let fill = Color32::from_rgb(c.r, c.g, c.b);
        paint_stop(
            Stop {
                opacity: false,
                index,
            },
            *p,
            fill,
            count,
        );
    }

    // Presses: judged where the press happened, as the pointer has moved on by the drag's start.
    let mut open_picker_at = None;
    let origin = if resp.drag_started() {
        ui.ctx().input(|i| i.pointer.press_origin()).or(pointer)
    } else {
        pointer
    };
    if (resp.drag_started() || resp.clicked())
        && let Some(pp) = origin
    {
        let hit_opacity = g
            .opacity
            .iter()
            .position(|(p, _)| stop_rect(true, *p).expand(2.0).contains(pp))
            .map(|index| Stop { opacity: true, index });
        let hit_color = g
            .colors
            .iter()
            .position(|(p, _)| stop_rect(false, *p).expand(2.0).contains(pp))
            .map(|index| Stop {
                opacity: false,
                index,
            });
        match hit_opacity.or(hit_color) {
            Some(stop) => {
                state.selected = Some(stop);
                if resp.drag_started() {
                    state.dragging = Some(stop);
                } else if !stop.opacity {
                    // A click on a colour stop opens its colour picker.
                    state.picker = Some(stop.index);
                    open_picker_at = Some(pp.x);
                }
            }
            None if resp.clicked() => {
                let pos = ((pp.x - bar.left()) / bar.width()).clamp(0.0, 1.0);
                if pp.y < bar.top() {
                    let a = g.color_on_ramp(pos).a;
                    let at = g.opacity.partition_point(|s| s.0 <= pos);
                    g.opacity.insert(at, (pos, a));
                } else {
                    let c = Rgba {
                        a: 255,
                        ..colour_only(g).color_on_ramp(pos)
                    };
                    let at = g.colors.partition_point(|s| s.0 <= pos);
                    g.colors.insert(at, (pos, c));
                }
                state.selected = None;
            }
            None => {}
        }
    }
    if let (Some(stop), Some(pp)) = (state.dragging, pointer)
        && resp.dragged()
    {
        let pos = ((pp.x - bar.left()) / bar.width()).clamp(0.0, 1.0);
        let index = if stop.opacity {
            reposition(&mut g.opacity, stop.index, pos)
        } else {
            reposition(&mut g.colors, stop.index, pos)
        };
        let moved = Stop { index, ..stop };
        state.dragging = Some(moved);
        state.selected = Some(moved);
    }
    if resp.drag_stopped()
        && let Some(stop) = state.dragging.take()
    {
        // Dragged out of the pop-up: removed (two stops always stay).
        if torn(stop.opacity, ui.ctx().input(|i| i.pointer.latest_pos())) {
            if stop.opacity && g.opacity.len() > 2 && stop.index < g.opacity.len() {
                g.opacity.remove(stop.index);
                state.selected = None;
            } else if !stop.opacity && g.colors.len() > 2 && stop.index < g.colors.len() {
                g.colors.remove(stop.index);
                state.selected = None;
            }
        }
    }

    // The colour wheel beside the colour stops: the system picker for the selected stop.
    let wheel = Rect::from_min_size(o + vec2(0.0, 75.0), vec2(11.0, 11.0));
    let wheel_resp = ui.interact(wheel, id.with("wheel"), Sense::click());
    cp::paint_wheel(ui.painter(), wheel.center(), 4.5);
    if wheel_resp.clicked() && !g.colors.is_empty() {
        let index = match state.selected {
            Some(Stop {
                opacity: false,
                index,
            }) if index < g.colors.len() => index,
            _ => 0,
        };
        state.selected = Some(Stop {
            opacity: false,
            index,
        });
        state.picker = Some(index);
        open_picker_at = Some(stop_rect(false, g.colors[index].0).center().x);
        cp::request_wheel(ui.ctx(), id.with(("stop-color", index)));
    }
    if let Some(index) = state.picker {
        let anchor = g.colors.get(index).map_or(bar, |s| stop_rect(false, s.0));
        let swatch = id.with(("stop-color", index));
        let color = g.colors.get(index).map(|s| cp::to_argb(s.1));
        match cp::show(
            ui,
            swatch,
            anchor,
            open_picker_at,
            &|_| None,
            color,
            Footer::Plain,
        ) {
            Some(ColorChoice::Color(c)) => {
                if let Some(s) = g.colors.get_mut(index) {
                    s.1 = Rgba {
                        a: 255,
                        ..cp::to_rgba(c)
                    };
                }
                state.picker = None;
            }
            Some(_) => state.picker = None,
            None if open_picker_at.is_none() && !egui::Popup::is_id_open(ui.ctx(), cp::popup_id(swatch)) => {
                state.picker = None;
            }
            None => {}
        }
    }

    // A selected opacity stop's opacity; the row's space is kept when none is selected.
    if let Some(Stop { opacity: true, index }) = state.selected
        && state.dragging.is_none()
        && index < g.opacity.len()
    {
        place(
            ui,
            Rect::from_min_size(o + vec2(16.0, 90.0), vec2(176.0, 18.0)),
            |ui| {
                ui.horizontal(|ui| {
                    ui.label("Opacity:");
                    let mut pct = (f32::from(g.opacity[index].1) / 255.0 * 100.0).round();
                    let field = egui::DragValue::new(&mut pct)
                        .range(0.0..=100.0)
                        .max_decimals(0)
                        .suffix("%");
                    if ui.add(field).changed() {
                        g.opacity[index].1 = (pct / 100.0 * 255.0).round() as u8;
                    }
                });
            },
        );
    }

    lower_controls(ui, g, state, o, angle, picked);
}

/// The Preset list, Reverse and the angle.
fn lower_controls(
    ui: &mut egui::Ui,
    g: &mut Gradient,
    state: &mut Popup,
    o: Pos2,
    angle: Option<f64>,
    picked: &mut Picked,
) {
    // Fireworks MX's presets: the name shows while the stops still are that preset.
    place(
        ui,
        Rect::from_min_size(o + vec2(0.0, 110.0), vec2(POPUP_SIZE.x, 20.0)),
        |ui| {
            ui.horizontal(|ui| {
                ui.label("Preset:");
                let shown = preset_name(g);
                egui::ComboBox::from_id_salt("gradient_preset")
                    .width(150.0)
                    .height(400.0)
                    .selected_text(shown.unwrap_or(""))
                    .show_ui(ui, |ui| {
                        for (name, stops) in GRADIENT_PRESETS {
                            if ui.selectable_label(shown == Some(name), name).clicked() {
                                g.colors = preset_colors(stops);
                                g.opacity = opaque_ramp();
                                state.selected = None;
                            }
                        }
                    });
            });
        },
    );

    // Reverse and the angle.
    let row = o.y + POPUP_SIZE.y - 20.0;
    let reverse = Rect::from_min_size(pos2(o.x, row), vec2(64.0, 20.0));
    if ui.put(reverse, egui::Button::new("Reverse")).clicked() {
        reverse_stops(&mut g.colors);
        reverse_stops(&mut g.opacity);
        state.selected = None;
    }
    place(
        ui,
        Rect::from_min_size(pos2(o.x + 120.0, row), vec2(80.0, 20.0)),
        |ui| {
            ui.add_enabled_ui(angle.is_some(), |ui| {
                ui.horizontal(|ui| {
                    ui.label("Angle:");
                    let mut value = angle.unwrap_or(0.0).round();
                    let field = egui::DragValue::new(&mut value)
                        .range(0.0..=360.0)
                        .max_decimals(0)
                        .suffix("\u{b0}");
                    if ui.add(field).changed() {
                        picked.angle = Some(value.rem_euclid(360.0));
                    }
                });
            });
        },
    );
}

/// Mirror a ramp's stops.
fn reverse_stops<T: Copy>(stops: &mut [(f32, T)]) {
    stops.reverse();
    for s in stops {
        s.0 = 1.0 - s.0;
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
