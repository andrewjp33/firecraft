//! Document windows (or tabs) and the canvas: rendering, tool input and selection overlays.

use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};
use firecraft_engine::{DocState, ElementRef, PaintTool, Tool};
use firecraft_raster::Mask;
use firecraft_raster::mask::Combine;

use instant::Instant;

use crate::{FireCraftApp, View, theme};

pub fn workspace(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    // Forget views of closed documents.
    let ids: Vec<u64> = app.session.docs.iter().map(|d| d.id).collect();
    app.views.retain(|k, _| ids.contains(k));
    if app.session.docs.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new(format!(
                    "FireCraft\n\nFile > New ({})   |   File > Open (Ctrl+O)   |   or drop a PNG here",
                    crate::menus::NEW_KEY
                ))
                .color(Color32::from_gray(230)),
            );
        });
        return;
    }
    if app.prefs.tabs {
        tabs(app, ui);
    } else {
        windows(app, ui);
    }
}

fn tabs(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let mut close: Option<usize> = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        for (i, d) in app.session.docs.iter().enumerate() {
            let active = i == app.session.active;
            let title = format!("{}{}", d.doc.title(), if d.dirty { " *" } else { "" });
            egui::Frame::new()
                .fill(if active {
                    theme::PANEL
                } else {
                    Color32::from_gray(190)
                })
                .inner_margin(egui::Margin::symmetric(6, 2))
                .show(ui, |ui| {
                    if ui.add(egui::Label::new(title).sense(Sense::click())).clicked() {
                        app.session.active = i;
                    }
                    if ui
                        .add(egui::Button::new("x").small().frame(false))
                        .on_hover_text("Close")
                        .clicked()
                    {
                        close = Some(i);
                    }
                });
        }
    });
    if let Some(i) = close {
        app.session.close(i);
        return;
    }
    let i = app.session.active;
    egui::Frame::new().fill(theme::PANEL).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        document_view(app, ui, i);
    });
}

/// A document window's state (Fireworks MX's MDI windows).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum WinState {
    #[default]
    Normal,
    Minimized,
    Maximized,
}

/// What the title bar asked for this frame.
enum TitleAction {
    None,
    Drag(Vec2),
    Set(WinState),
    Close,
}

/// A document window's title bar: title, then minimise, maximise/restore and close. Dragging
/// it moves the window; double-clicking maximises or restores.
fn title_bar(ui: &mut egui::Ui, title: &str, active: bool, state: WinState) -> TitleAction {
    let h = 20.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::click_and_drag());
    let p = ui.painter();
    let fill = if active {
        Color32::from_rgb(0xD6, 0xD6, 0xD6)
    } else {
        Color32::from_rgb(0xE6, 0xE6, 0xE6)
    };
    p.rect_filled(rect, 0.0, fill);
    let text = if active {
        Color32::from_gray(40)
    } else {
        Color32::from_gray(110)
    };
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        title,
        egui::FontId::proportional(13.0),
        text,
    );
    let mut action = TitleAction::None;
    // Buttons, right to left: close, maximise/restore, minimise.
    let size = vec2(16.0, 14.0);
    let mut x = rect.right() - 4.0 - size.x;
    let restore = state != WinState::Normal;
    let buttons: [(&str, u8); 3] = [
        ("Close", 0),
        (if restore { "Restore" } else { "Maximize" }, 1),
        ("Minimize", 2),
    ];
    for (tip, kind) in buttons {
        let r = Rect::from_min_size(pos2(x, rect.center().y - size.y / 2.0), size);
        x -= size.x + 2.0;
        let b = ui
            .interact(r, ui.scope_id().with(("win_btn", kind)), Sense::click())
            .on_hover_text(tip);
        b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
        if b.hovered() {
            ui.painter().rect_filled(r, 2.0, Color32::from_gray(190));
        }
        let c = r.center();
        let s = Stroke::new(1.3, text);
        match kind {
            0 => {
                ui.painter()
                    .line_segment([c + vec2(-4.0, -4.0), c + vec2(4.0, 4.0)], s);
                ui.painter()
                    .line_segment([c + vec2(4.0, -4.0), c + vec2(-4.0, 4.0)], s);
            }
            1 if restore => {
                ui.painter().rect_stroke(
                    Rect::from_center_size(c + vec2(-1.0, 1.0), vec2(7.0, 6.0)),
                    0.0,
                    s,
                    StrokeKind::Middle,
                );
                ui.painter()
                    .line_segment([c + vec2(-1.0, -4.0), c + vec2(5.0, -4.0)], s);
                ui.painter()
                    .line_segment([c + vec2(5.0, -4.0), c + vec2(5.0, 1.0)], s);
            }
            1 => {
                ui.painter().rect_stroke(
                    Rect::from_center_size(c, vec2(9.0, 8.0)),
                    0.0,
                    s,
                    StrokeKind::Middle,
                );
            }
            _ => {
                ui.painter()
                    .line_segment([c + vec2(-4.0, 3.0), c + vec2(4.0, 3.0)], s);
            }
        }
        if b.clicked() {
            action = match kind {
                0 => TitleAction::Close,
                1 if restore => TitleAction::Set(WinState::Normal),
                1 => TitleAction::Set(WinState::Maximized),
                _ if state == WinState::Minimized => TitleAction::Set(WinState::Normal),
                _ => TitleAction::Set(WinState::Minimized),
            };
        }
    }
    if matches!(action, TitleAction::None) {
        if resp.double_clicked() {
            action = TitleAction::Set(if state == WinState::Normal {
                WinState::Maximized
            } else {
                WinState::Normal
            });
        } else if resp.dragged() && state == WinState::Normal {
            action = TitleAction::Drag(resp.drag_delta());
        }
    }
    action
}

/// Window > Cascade / Tile Horizontal / Tile Vertical.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrangement {
    Cascade,
    /// Windows stacked top to bottom (Windows' "tile horizontally").
    TileHorizontal,
    /// Windows side by side.
    TileVertical,
}

/// Arrange the document windows over the workspace (restoring minimised/maximised ones).
pub fn arrange(app: &mut FireCraftApp, how: Arrangement) {
    let area = app.workspace;
    let ids: Vec<u64> = app.session.docs.iter().map(|d| d.id).collect();
    let n = ids.len();
    if n == 0 || !area.is_positive() {
        return;
    }
    // Grids for many windows; one row or column for up to three.
    let grid = |n: usize| {
        if n <= 3 {
            n
        } else {
            (n as f32).sqrt().ceil() as usize
        }
    };
    let (cols, rows) = match how {
        Arrangement::Cascade => (1, 1),
        Arrangement::TileVertical => (grid(n), n.div_ceil(grid(n))),
        Arrangement::TileHorizontal => (n.div_ceil(grid(n)), grid(n)),
    };
    // The active document goes first (top-left / front).
    let mut order: Vec<u64> = ids.clone();
    if let Some(a) = app.session.active_doc().map(|d| d.id) {
        order.retain(|i| *i != a);
        order.insert(0, a);
    }
    for (k, id) in order.iter().enumerate() {
        let view = app.views.entry(*id).or_default();
        let rect = match how {
            Arrangement::Cascade => {
                // The active window ends on top: it goes last in the cascade.
                let step = 24.0 * (n - 1 - k) as f32;
                let size = vec2((area.width() * 0.65).max(260.0), (area.height() * 0.7).max(200.0));
                Rect::from_min_size(area.min + vec2(4.0 + step, 4.0 + step), size)
            }
            _ => {
                let (c, r) = (k % cols, k / cols);
                let cell = vec2(area.width() / cols as f32, area.height() / rows as f32);
                Rect::from_min_size(
                    area.min + vec2(cell.x * c as f32, cell.y * r as f32),
                    cell - vec2(4.0, 4.0),
                )
            }
        };
        view.win = WinState::Normal;
        view.auto_fit = false;
        view.win_rect = Some(rect);
    }
}

/// Smallest document window.
const MIN_WINDOW: Vec2 = vec2(260.0, 120.0);

/// Resize grips along a normal window's edges and corners: each a small layer of its own
/// just above the window (egui finds the layer under the pointer by its rectangle), so they
/// win over the window's content but stay below windows in front of it. Returns the window's
/// new rectangle while one is dragged.
fn resize_grips(
    ctx: &egui::Context,
    window: egui::LayerId,
    rect: Rect,
    area: Rect,
    id: egui::Id,
) -> Option<Rect> {
    use egui::CursorIcon as C;
    let (g, c) = (5.0, 12.0);
    let (l, t, r, b) = (rect.left(), rect.top(), rect.right(), rect.bottom());
    // (zone, which edges move: left, top, right, bottom, cursor); edges stop short of corners.
    let zones = [
        (
            Rect::from_min_max(pos2(l, t), pos2(l + c, t + c)),
            [true, true, false, false],
            C::ResizeNwSe,
        ),
        (
            Rect::from_min_max(pos2(r - c, t), pos2(r, t + c)),
            [false, true, true, false],
            C::ResizeNeSw,
        ),
        (
            Rect::from_min_max(pos2(l, b - c), pos2(l + c, b)),
            [true, false, false, true],
            C::ResizeNeSw,
        ),
        (
            Rect::from_min_max(pos2(r - c, b - c), pos2(r, b)),
            [false, false, true, true],
            C::ResizeNwSe,
        ),
        (
            Rect::from_min_max(pos2(l, t + c), pos2(l + g, b - c)),
            [true, false, false, false],
            C::ResizeHorizontal,
        ),
        (
            Rect::from_min_max(pos2(r - g, t + c), pos2(r, b - c)),
            [false, false, true, false],
            C::ResizeHorizontal,
        ),
        (
            Rect::from_min_max(pos2(l + c, t), pos2(r - c, t + g)),
            [false, true, false, false],
            C::ResizeVertical,
        ),
        (
            Rect::from_min_max(pos2(l + c, b - g), pos2(r - c, b)),
            [false, false, false, true],
            C::ResizeVertical,
        ),
    ];
    let mut out = None;
    for (k, (zone, [ml, mt, mr, mb], cursor)) in zones.into_iter().enumerate() {
        let grip = egui::Area::new(id.with(("grip", k)))
            .order(egui::Order::Middle)
            .fixed_pos(zone.min)
            .constrain(false)
            .show(ctx, |ui| {
                ui.set_clip_rect(area);
                let (_, resp) = ui.allocate_exact_size(zone.size(), Sense::drag());
                if resp.hovered() || resp.dragged() {
                    ui.ctx().set_cursor_icon(cursor);
                }
                resp.dragged().then(|| resp.drag_delta())
            });
        ctx.set_sublayer(window, grip.response.layer_id);
        if let (Some(d), None) = (grip.inner, out) {
            // Between the smallest window and the workspace's size (egui draws a window no
            // larger than the area it's constrained to).
            let max = area.size();
            let mut n = rect;
            if ml {
                n.min.x = (n.min.x + d.x).clamp(n.max.x - max.x, n.max.x - MIN_WINDOW.x);
            }
            if mr {
                n.max.x = (n.max.x + d.x).clamp(n.min.x + MIN_WINDOW.x, n.min.x + max.x);
            }
            if mt {
                n.min.y = (n.min.y + d.y).clamp(n.max.y - max.y, n.max.y - MIN_WINDOW.y);
            }
            if mb {
                n.max.y = (n.max.y + d.y).clamp(n.min.y + MIN_WINDOW.y, n.min.y + max.y);
            }
            out = Some(n);
        }
    }
    out
}

/// A window rectangle moved so some of its title bar stays reachable in `area`.
fn keep_reachable(r: Rect, area: Rect) -> Rect {
    let x = r.min.x.clamp(area.left() - r.width() + 80.0, area.right() - 80.0);
    let y = r.min.y.clamp(area.top(), area.bottom() - 24.0);
    r.translate(pos2(x, y) - r.min)
}

fn windows(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let area = ui.max_rect();
    let area_changed = (app.workspace.size() - area.size()).length() > 0.5;
    app.workspace = area;
    let ctx = ui.ctx().clone();
    let mut close: Option<usize> = None;
    let count = app.session.docs.len();
    // Minimised windows line up along the bottom of the workspace.
    let mut slot = 0.0;
    let frame = egui::Frame::window(&ctx.global_style()).inner_margin(egui::Margin::same(2));
    for i in 0..count {
        let Some(d) = app.session.docs.get(i) else {
            continue;
        };
        let id = d.id;
        let title = format!(
            "{} @ {:.0}%{}",
            d.doc.title(),
            d.zoom.max(0.01) * 100.0,
            if d.dirty { " *" } else { "" }
        );
        let (w, h) = (d.doc.width as f32, d.doc.height as f32);
        let active = i == app.session.active;
        let view = app.views.entry(id).or_default();
        // A new window: the document plus its bars, within the workspace from where it
        // starts. Untouched windows keep fitting as the workspace changes size (the first
        // frames can be smaller than the final, maximised main window).
        let offset = 24.0 * (i % 8) as f32;
        let start = area.min + vec2(12.0 + offset, 12.0 + offset);
        let fit = Rect::from_min_size(
            start,
            vec2(
                (w + 40.0).min(area.right() - start.x - 12.0).max(MIN_WINDOW.x),
                (h + 110.0).min(area.bottom() - start.y - 12.0).max(MIN_WINDOW.y),
            ),
        );
        if view.win_rect.is_none() {
            view.win_rect = Some(fit);
            view.auto_fit = true;
        } else if view.auto_fit && area_changed && view.win == WinState::Normal {
            view.win_rect = Some(fit);
        }
        // No larger than the workspace (what egui will draw), e.g. after the main window shrank.
        let normal = view
            .win_rect
            .map_or(fit, |r| Rect::from_min_size(r.min, r.size().min(area.size())));
        let state = view.win;
        let rect = match state {
            WinState::Maximized => area.shrink(1.0),
            WinState::Minimized => {
                let r = Rect::from_min_size(
                    pos2(area.left() + 2.0 + slot, area.bottom() - 28.0),
                    vec2(220.0, 26.0),
                );
                slot += 224.0;
                r
            }
            WinState::Normal => normal,
        };
        // Exactly this rectangle, every frame: FireCraft owns the window's size and place.
        let shown = egui::Window::new(&title)
            .id(egui::Id::unique(("docwin", id)))
            .title_bar(false)
            .movable(false)
            .resizable(false)
            .collapsible(false)
            // Painted clipped to the workspace, but free to sit partly past its edges.
            .constrain_to(area)
            .constrain(false)
            .frame(frame)
            .fixed_rect(rect)
            .show(&ctx, |ui| {
                let action = title_bar(ui, &title, active, state);
                if state != WinState::Minimized {
                    document_view(app, ui, i);
                }
                action
            });
        let Some(shown) = shown else { continue };
        // Resize grips on their own layer just above this window (below windows in front of
        // it), so they win over the window's content along the edges.
        let resized = if state == WinState::Normal {
            resize_grips(
                &ctx,
                shown.response.layer_id,
                shown.response.rect,
                area,
                egui::Id::unique(("docgrip", id)),
            )
        } else {
            None
        };
        let r = &shown.response;
        if r.clicked() || r.dragged() || (r.contains_pointer() && ctx.input(|inp| inp.pointer.any_pressed()))
        {
            app.session.active = i;
        }
        let action = shown.inner.unwrap_or(TitleAction::None);
        let view = app.views.entry(id).or_default();
        if let Some(n) = resized {
            view.win_rect = Some(n);
            view.auto_fit = false;
            app.session.active = i;
        }
        match action {
            TitleAction::None => {}
            TitleAction::Drag(delta) => {
                app.session.active = i;
                view.auto_fit = false;
                view.win_rect = Some(keep_reachable(normal.translate(delta), area));
            }
            TitleAction::Set(s) => {
                view.win = s;
                view.auto_fit = false;
                app.session.active = i;
            }
            TitleAction::Close => close = Some(i),
        }
    }
    if let Some(i) = close {
        app.session.close(i);
    }
}

/// Tabs strip, canvas and status bar of one document. Laid out as nested panels so a resizable
/// document window keeps a stable size.
fn document_view(app: &mut FireCraftApp, ui: &mut egui::Ui, index: usize) {
    let Some(id) = app.session.docs.get(index).map(|d| d.id) else {
        return;
    };
    let mut preview = app.views.get(&id).is_some_and(|v| v.preview);
    egui::Panel::top(egui::Id::unique(("doc_tabs", id)))
        .frame(egui::Frame::new())
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut preview, false, "Original");
                ui.selectable_value(&mut preview, true, "Preview");
                ui.add_enabled(false, egui::Button::new("2-Up"))
                    .on_disabled_hover_text("Coming with the Optimize panel");
                ui.add_enabled(false, egui::Button::new("4-Up"))
                    .on_disabled_hover_text("Coming with the Optimize panel");
                // Symbol Editor: Done closes it and returns to the document (Fireworks CS).
                let editing_symbol = app
                    .session
                    .docs
                    .get(index)
                    .is_some_and(|d| d.symbol_link.is_some());
                if editing_symbol {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("Done")
                            .on_hover_text("Finish editing the symbol and return to the document")
                            .clicked()
                        {
                            ui.ctx()
                                .data_mut(|m| m.insert_temp(egui::Id::unique("symbol_done"), id));
                        }
                    });
                }
            });
        });
    if let Some(v) = app.views.get_mut(&id) {
        v.preview = preview;
    }
    // Status bar: frame controls and document size, like Fireworks' document window footer.
    egui::Panel::bottom(egui::Id::unique(("doc_status", id)))
        .frame(egui::Frame::new())
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(d) = app.session.docs.get_mut(index) {
                    let n = d.doc.frames.len().max(1);
                    let view = app.views.entry(id).or_default();
                    let multi = n > 1;
                    if crate::icons::glyph_button(ui, crate::icons::Glyph::First, 14.0, multi)
                        .on_hover_text("First frame")
                        .clicked()
                    {
                        d.doc.current_frame = 0;
                    }
                    let play = if view.playing {
                        crate::icons::Glyph::Stop
                    } else {
                        crate::icons::Glyph::Play
                    };
                    if crate::icons::glyph_button(ui, play, 14.0, multi)
                        .on_hover_text("Play/Stop")
                        .clicked()
                    {
                        view.playing = !view.playing;
                        view.last_tick = ui.ctx().input(|i| i.time);
                    }
                    if crate::icons::glyph_button(ui, crate::icons::Glyph::Last, 14.0, multi)
                        .on_hover_text("Last frame")
                        .clicked()
                    {
                        d.doc.current_frame = n - 1;
                    }
                    // Playback: advance by each frame's delay (1/100 s).
                    if view.playing && multi {
                        let now = ui.ctx().input(|i| i.time);
                        let delay = d
                            .doc
                            .frames
                            .get(d.doc.current_frame)
                            .map(|f| f.delay.max(1))
                            .unwrap_or(7) as f64
                            / 100.0;
                        if now - view.last_tick >= delay {
                            d.doc.current_frame = (d.doc.current_frame + 1) % n;
                            view.last_tick = now;
                        }
                        ui.ctx()
                            .request_repaint_after(std::time::Duration::from_secs_f64(delay.min(1.0) / 2.0));
                    }
                    ui.label(format!("{}", d.doc.current_frame + 1));
                    ui.separator();
                    ui.label(format!("{} x {}", d.doc.width, d.doc.height));
                    ui.separator();
                    egui::ComboBox::from_id_salt(("zoom", id))
                        .width(64.0)
                        .selected_text(format!("{:.0}%", d.zoom * 100.0))
                        .show_ui(ui, |ui| {
                            for z in [
                                0.0625, 0.125, 0.25, 0.5, 0.66, 1.0, 1.5, 2.0, 3.0, 4.0, 6.0, 8.0, 16.0, 32.0,
                            ] {
                                ui.selectable_value(&mut d.zoom, z, format!("{:.0}%", z * 100.0));
                            }
                        });
                }
            });
        });
    let (rect, resp) =
        ui.allocate_exact_size(ui.available_size().max(vec2(50.0, 50.0)), Sense::click_and_drag());
    if resp.is_pointer_button_down_on() || resp.clicked() {
        app.session.active = index;
    }
    canvas(app, ui, index, rect, &resp);
}

/// The stroke a tool paints by dragging, if it is one of those.
fn paint_tool(tool: Tool) -> Option<PaintTool> {
    match tool {
        Tool::Brush => Some(PaintTool::Brush),
        Tool::Pencil => Some(PaintTool::Pencil),
        Tool::Eraser => Some(PaintTool::Eraser),
        _ => None,
    }
}

/// Mouse input for a stroke. It starts at the press, not after egui's drag threshold, and takes
/// every pointer position of the frame, so a quick stroke doesn't turn into straight segments.
fn paint_input(
    d: &mut firecraft_engine::DocState,
    kind: PaintTool,
    resp: &egui::Response,
    ctx: &egui::Context,
    options: &firecraft_engine::ToolOptions,
    constrain: bool,
    to_doc: &impl Fn(Pos2) -> (f64, f64),
) {
    let (pressed, down, cancel) = ctx.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.key_pressed(egui::Key::Escape),
        )
    });
    if pressed
        && resp.hovered()
        && let Some(pos) = ctx.input(|i| i.pointer.press_origin()).or(resp.hover_pos())
    {
        let (x, y) = to_doc(pos);
        d.begin_paint(kind, x, y, options);
    }
    if !d.is_painting() {
        return;
    }
    if cancel {
        d.cancel_paint();
        return;
    }
    let moved: Vec<Pos2> = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|e| match e {
                egui::Event::PointerMoved(p) => Some(*p),
                _ => None,
            })
            .collect()
    });
    for pos in moved {
        let (x, y) = to_doc(pos);
        d.extend_paint(x, y, constrain);
    }
    d.flush_paint();
    if !down {
        d.end_paint();
    }
    // The canvas texture was drawn earlier this frame: show the stroke in the next one.
    ctx.request_repaint();
}

// One flat input-dispatch function per tool is easier to follow than splitting it; revisit when
// tools move into their own modules (roadmap: Pen/Subselection).
#[allow(clippy::cognitive_complexity)]
fn canvas(app: &mut FireCraftApp, ui: &mut egui::Ui, index: usize, rect: Rect, resp: &egui::Response) {
    let ctx = ui.ctx().clone();
    // Holding Space switches to the Hand tool temporarily, as in Fireworks.
    let space = !ui.ctx().egui_wants_keyboard_input() && ui.ctx().input(|i| i.key_down(egui::Key::Space));
    let tool = if space { Tool::Hand } else { app.session.tool };
    let options = app.session.options.clone();
    let live_drag = app.prefs.live_drag;
    let Some(d) = app.session.docs.get_mut(index) else {
        return;
    };
    let view = app.views.entry(d.id).or_default();
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, Color32::from_gray(0xE0));

    // The zoom counts screen pixels (100%: one document pixel per screen pixel, as in
    // Fireworks), so with display scaling a canvas pixel is 1/ppp points; drawing is in points.
    let ppp = ctx.pixels_per_point();
    let fit = (rect.width() / d.doc.width.max(1) as f32).min(rect.height() / d.doc.height.max(1) as f32)
        * 0.95
        * ppp;
    if d.zoom == 0.0 {
        // View > Fit All.
        d.zoom = fit.clamp(1.0 / 16.0, 64.0);
        view.auto_zoom = None;
    } else if d.zoom < 0.0 || view.auto_zoom.is_some_and(|z| d.zoom == z) {
        // A newly opened document: scaled to fit its window, at most 100%, and kept fitting
        // while the window settles, until the user zooms. A Symbol Editor fits (and centres
        // on) the symbol's content rather than its large canvas.
        match d.view_focus {
            Some((fx, fy, fw, fh)) => {
                let f =
                    (rect.width() / fw.max(1.0) as f32).min(rect.height() / fh.max(1.0) as f32) * 0.9 * ppp;
                d.zoom = f.clamp(1.0 / 16.0, 1.0);
                center_on(d, view, (fx + fw / 2.0, fy + fh / 2.0), ppp);
            }
            None => d.zoom = fit.clamp(1.0 / 16.0, 1.0),
        }
        view.auto_zoom = Some(d.zoom);
    } else if view.auto_zoom.is_some() {
        view.auto_zoom = None;
    }
    // Wheel: scroll pans, Ctrl+wheel zooms around the pointer.
    if resp.hovered() {
        let (scroll, zoom_delta) = ctx.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
        if zoom_delta != 1.0 {
            zoom_at(d, view, rect, ctx.pointer_hover_pos(), zoom_delta);
        } else {
            view.pan += scroll;
        }
    }
    let z = d.zoom / ppp;
    let size = vec2(d.doc.width as f32, d.doc.height as f32) * z;
    // On whole screen pixels, so 100% shows each document pixel on exactly one screen pixel.
    let origin = ((rect.center() - size / 2.0 + view.pan) * ppp).round() / ppp;
    let canvas_rect = Rect::from_min_size(origin, size);
    let to_doc =
        |pos: Pos2| -> (f64, f64) { (((pos.x - origin.x) / z) as f64, ((pos.y - origin.y) / z) as f64) };
    let to_screen = |x: f64, y: f64| -> Pos2 { pos2(origin.x + x as f32 * z, origin.y + y as f32 * z) };

    // Composite texture, refreshed when the document changes.
    // Re-render when the document or the shown frame changes.
    let key = (d.revision << 16) ^ d.doc.current_frame as u64;
    let stale = view.texture.as_ref().is_none_or(|(k, _)| *k != key);
    if stale {
        //let t0 = std::time::Instant::now();
        let t0 = Instant::now();
        let img = d
            .doc
            .render_cached(d.doc.current_frame, &mut view.cache.borrow_mut());
        let t1 = t0.elapsed();
        let dirty = view.cache.borrow().dirty;
        let to_color = |img: &firecraft_raster::RgbaImage| {
            egui::ColorImage::from_rgba_unmultiplied(
                [img.width() as usize, img.height() as usize],
                &img.to_rgba_bytes(),
            )
        };
        let options = egui::TextureOptions::NEAREST;
        match (&mut view.texture, dirty) {
            // Only part of the picture changed: upload just that part.
            (Some((stored, _)), firecraft_doc::Dirty::None) => *stored = key,
            (Some((stored, tex)), firecraft_doc::Dirty::Rect { x, y, w, h }) => {
                let mut part = firecraft_raster::RgbaImage::new(w, h, firecraft_raster::Rgba::TRANSPARENT);
                part.copy_from(&img, -(x as i64), -(y as i64));
                tex.set_partial([x as usize, y as usize], to_color(&part), options);
                // The cache key (not just the revision), or every frame looks stale.
                *stored = key;
            }
            (Some((stored, tex)), firecraft_doc::Dirty::Full) => {
                tex.set(to_color(&img), options);
                *stored = key;
            }
            (None, _) => {
                view.texture = Some((
                    key,
                    ctx.load_texture(format!("doc-{}", d.id), to_color(&img), options),
                ))
            }
        }
        // FIRECRAFT_PROFILE=1: how long each canvas refresh takes.
        if std::env::var_os("FIRECRAFT_PROFILE").is_some() {
            let (hits, rendered) = view.cache.borrow().last;
            eprintln!(
                "canvas refresh: render {:.1} ms ({hits} cached, {rendered} rendered), texture {:.1} ms, {dirty:?}",
                t1.as_secs_f64() * 1000.0,
                (t0.elapsed() - t1).as_secs_f64() * 1000.0
            );
        }
    }
    // Transparent canvas shows a checkerboard.
    if d.doc.canvas.is_none() {
        checkerboard(&p, canvas_rect.intersect(rect));
    }
    if let Some((_, tex)) = &view.texture {
        p.image(
            tex.id(),
            canvas_rect,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
    p.rect_stroke(
        canvas_rect,
        0.0,
        Stroke::new(1.0, Color32::from_gray(60)),
        StrokeKind::Outside,
    );
    // Symbol Editor: the symbol's origin (registration point), as Fireworks marks it.
    if let Some(link) = d.symbol_link {
        let c = to_screen(link.offset.0, link.offset.1);
        for (col, w) in [(Color32::WHITE, 3.0), (Color32::from_rgb(0, 90, 200), 1.0)] {
            let s = Stroke::new(w, col);
            p.line_segment([c - vec2(9.0, 0.0), c + vec2(9.0, 0.0)], s);
            p.line_segment([c - vec2(0.0, 9.0), c + vec2(0.0, 9.0)], s);
        }
        // 9-slice guides (dashed, draggable), as in Fireworks CS.
        if let Some((gl, gr, gt, gb)) = d.symbol_nine {
            let (ox, oy) = link.offset;
            let area = canvas_rect.intersect(rect);
            let guide = Stroke::new(1.0, Color32::from_rgb(0, 0, 255));
            let mut moved = None;
            for (k, v) in [gl, gr, gt, gb].into_iter().enumerate() {
                let vertical = k < 2;
                let s = if vertical {
                    to_screen(ox + v, 0.0).x
                } else {
                    to_screen(0.0, oy + v).y
                };
                let (line, zone, cursor) = if vertical {
                    (
                        [pos2(s, area.top()), pos2(s, area.bottom())],
                        Rect::from_min_max(pos2(s - 3.0, area.top()), pos2(s + 3.0, area.bottom())),
                        egui::CursorIcon::ResizeHorizontal,
                    )
                } else {
                    (
                        [pos2(area.left(), s), pos2(area.right(), s)],
                        Rect::from_min_max(pos2(area.left(), s - 3.0), pos2(area.right(), s + 3.0)),
                        egui::CursorIcon::ResizeVertical,
                    )
                };
                p.extend(Shape::dashed_line(&line, guide, 5.0, 4.0));
                let g = ui.interact(zone, egui::Id::unique(("nine_guide", d.id, k)), Sense::drag());
                if g.hovered() || g.dragged() {
                    ctx.set_cursor_icon(cursor);
                }
                if g.dragged()
                    && let Some(pos) = ctx.pointer_interact_pos()
                {
                    let (x, y) = to_doc(pos);
                    moved = Some((
                        k,
                        if vertical {
                            (x - ox).round()
                        } else {
                            (y - oy).round()
                        },
                    ));
                }
            }
            if let Some((k, v)) = moved {
                let mut g = [gl, gr, gt, gb];
                if let Some(slot) = g.get_mut(k) {
                    *slot = v;
                }
                // Keep each pair in order.
                let (l, r) = (g[0].min(g[1]), g[0].max(g[1]));
                let (t, b) = (g[2].min(g[3]), g[2].max(g[3]));
                d.symbol_nine = Some((l, r, t, b));
                d.mark_changed();
            }
        }
    }

    // Each tool shows its own cursor over the canvas.
    if resp.hovered() || resp.dragged() {
        let alt = ctx.input(|i| i.modifiers.alt);
        ctx.set_cursor_icon(match tool {
            Tool::Hand if resp.dragged() => egui::CursorIcon::Grabbing,
            Tool::Hand => egui::CursorIcon::Grab,
            Tool::Zoom if alt => egui::CursorIcon::ZoomOut,
            Tool::Zoom => egui::CursorIcon::ZoomIn,
            Tool::Marquee
            | Tool::OvalMarquee
            | Tool::Lasso
            | Tool::MagicWand
            | Tool::Rectangle
            | Tool::Ellipse
            | Tool::Line
            | Tool::Pen
            | Tool::Eyedropper
            | Tool::Brush
            | Tool::Pencil
            | Tool::Eraser
            | Tool::PaintBucket
            | Tool::Gradient
            | Tool::Slice
            | Tool::Hotspot
            | Tool::Crop
            | Tool::NineSlice => egui::CursorIcon::Crosshair,
            Tool::Text => egui::CursorIcon::Text,
            _ => egui::CursorIcon::Default,
        });
        // Selection tools: FireCraft's own cursors (Fireworks style: black Pointer, white
        // Subselection arrow, Select Behind).
        let own = match tool {
            Tool::Pointer => Some(crate::cursors::Cur::Pointer),
            Tool::Subselect => Some(crate::cursors::Cur::Subselect),
            Tool::SelectBehind => Some(crate::cursors::Cur::SelectBehind),
            _ => None,
        };
        if let Some(cur) = own {
            crate::cursors::set(&ctx, cur, app.prefs.cursor_style);
        }
    }

    // Tools with editing state of their own (one for the app) work in the active document
    // only; other windows just show their document (Fireworks shows handles in one window).
    let active = index == app.session.active;
    if !active
        && matches!(
            tool,
            Tool::NineSlice | Tool::Scale | Tool::Skew | Tool::Distort | Tool::Pen | Tool::Subselect
        )
    {
        return;
    }
    // The Pen and Subselection tools too.
    if matches!(tool, Tool::Pen | Tool::Subselect) {
        crate::path_tool::canvas(
            &mut app.path_edit,
            d,
            tool,
            &p,
            resp,
            (origin, z),
            &app.session.options,
            (app.prefs.cursor_style, app.prefs.mouse_highlight),
        );
        return;
    }
    // The 9-slice tool handles its own input and overlay.
    if tool == Tool::NineSlice {
        crate::nine::canvas(&mut app.nine, d, view, &p, resp, origin, z);
        return;
    }
    // So do Scale, Skew and Distort.
    if matches!(tool, Tool::Scale | Tool::Skew | Tool::Distort) {
        crate::transform_tool::canvas(
            &mut app.transform,
            d,
            tool,
            &p,
            resp,
            (origin, z),
            (app.prefs.cursor_style, app.prefs.live_drag),
        );
        return;
    }

    // The Text tool takes the canvas while it's active (and finishes when another tool is chosen).
    if crate::text_tool::canvas(&mut app.text_tool, d, tool, &options, resp, &p, (origin, z)) {
        return;
    }

    // Fill handles of a selected gradient (over the tool underneath).
    if matches!(tool, Tool::Pointer | Tool::SelectBehind | Tool::Gradient)
        && crate::gradient_ui::handles(
            &mut app.fill_drag,
            d,
            &p,
            resp,
            (origin, z),
            app.prefs.cursor_style,
        )
    {
        return;
    }

    // ---- Input -------------------------------------------------------------------------
    let mods = ctx.input(|i| i.modifiers);
    let combine = match (mods.shift, mods.alt) {
        (true, true) => Combine::Intersect,
        (true, false) => Combine::Add,
        (false, true) => Combine::Subtract,
        _ => Combine::Replace,
    };
    let hover = resp.hover_pos();
    if let Some(kind) = paint_tool(tool) {
        paint_input(d, kind, resp, &ctx, &options, mods.shift, &to_doc);
    }
    if resp.drag_started() {
        // The press position, not the current one (which is already past the drag threshold).
        view.drag_start = ctx
            .input(|i| i.pointer.press_origin())
            .or(resp.interact_pointer_pos());
        view.drag_secondary = resp.dragged_by(egui::PointerButton::Secondary);
        if matches!(tool, Tool::Pointer | Tool::SelectBehind)
            && let Some(pos) = view.drag_start
        {
            let (x, y) = to_doc(pos);
            // Pressing inside the selection drags it, even under another object (Fireworks);
            // elsewhere the object under the pointer is picked first.
            let keep = d.selection_contains(x, y) || tool == Tool::SelectBehind;
            match if keep { None } else { Some(d.hit_test(x, y)) } {
                None => {}
                Some(found) => match found {
                    Some(hit) if !d.selection.contains(&hit) => {
                        d.selection = if mods.shift {
                            [d.selection.clone(), vec![hit]].concat()
                        } else {
                            vec![hit]
                        };
                        d.pixels = None;
                    }
                    Some(_) => {}
                    None => d.deselect(),
                },
            }
            // Live dragging: the objects themselves move (one undo step for the whole drag).
            if live_drag && !d.selection.is_empty() && !view.drag_secondary {
                d.begin_live_edit("Move");
                view.live_move = Some((0.0, 0.0));
            }
        }
    }
    let drag_now = resp.interact_pointer_pos();
    let dragging = resp.dragged() && view.drag_start.is_some();
    // Live move: follow the pointer in whole pixels.
    if let (Some(applied), Some(a), Some(b)) = (view.live_move, view.drag_start, drag_now.or(hover)) {
        let ((ax, ay), (bx, by)) = (to_doc(a), to_doc(b));
        let target = ((bx - ax).round(), (by - ay).round());
        if target != applied {
            d.move_selection_live(target.0 - applied.0, target.1 - applied.1);
            view.live_move = Some(target);
            // The canvas texture was drawn earlier this frame: show the move in the next one.
            ctx.request_repaint();
        }
    }
    if resp.drag_stopped() {
        if let (Some(a), Some(b)) = (view.drag_start, drag_now.or(hover)) {
            let (ax, ay) = to_doc(a);
            let (bx, by) = to_doc(b);
            match tool {
                // A live move already moved the objects: keep it, or drop it if it went nowhere.
                Tool::Pointer | Tool::SelectBehind if view.live_move.is_some() => {
                    if view.live_move == Some((0.0, 0.0)) {
                        d.cancel_live_edit();
                    } else {
                        d.commit_live_edit();
                    }
                }
                Tool::Pointer | Tool::SelectBehind => {
                    d.move_selection((bx - ax).round(), (by - ay).round(), "Move")
                }
                Tool::Marquee | Tool::OvalMarquee => {
                    d.marquee((ax, ay, bx, by), tool == Tool::OvalMarquee, &options, combine)
                }
                Tool::Rectangle | Tool::Ellipse => {
                    d.add_shape((ax, ay, bx, by), tool == Tool::Ellipse, &options)
                }
                Tool::Line => d.add_line((ax, ay), (bx, by), &options),
                Tool::Gradient => {
                    if !d.gradient_drag((ax, ay), (bx, by), &options) {
                        d.gradient_bitmap((ax, ay), (bx, by), &options);
                    }
                }
                // Drag a box with the Zoom tool: zoom so the box fills the view (Alt/right-drag: out).
                Tool::Zoom => {
                    let (w, h) = ((bx - ax).abs() as f32, (by - ay).abs() as f32);
                    if w >= 2.0 && h >= 2.0 {
                        let out = mods.alt || view.drag_secondary;
                        let fit = (rect.width() / w).min(rect.height() / h) * ppp;
                        d.zoom = if out {
                            (d.zoom * d.zoom / fit).clamp(1.0 / 16.0, 64.0)
                        } else {
                            fit.clamp(1.0 / 16.0, 64.0)
                        };
                        center_on(d, view, ((ax + bx) / 2.0, (ay + by) / 2.0), ppp);
                    }
                }
                Tool::Hand => {}
                _ => {}
            }
        }
        view.drag_start = None;
        view.live_move = None;
    }
    if dragging && tool == Tool::Hand {
        view.pan += resp.drag_delta();
    }
    if tool == Tool::Zoom && resp.secondary_clicked() {
        // Right-click zooms out: Alt+click (Fireworks' shortcut) is used by many Linux window
        // managers to move windows, so it never reaches the app there.
        if let Some(pos) = resp.interact_pointer_pos() {
            zoom_at(d, view, rect, Some(pos), 0.5);
        }
    }
    // Double-clicking an instance opens its symbol in the Symbol Editor (handled by the app,
    // which owns the document list).
    if resp.double_clicked()
        && matches!(tool, Tool::Pointer | Tool::SelectBehind)
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let (x, y) = to_doc(pos);
        // Double-clicking text edits it, with all of it selected.
        if let Some(r) = d.hit_test(x, y).filter(|r| d.editable_text_at(*r).is_some()) {
            app.session.tool = Tool::Text;
            app.text_tool.edit_all(d, r);
            return;
        }
        if let Some(id) = d.hit_test(x, y).and_then(|r| d.instance_symbol(r)) {
            ctx.data_mut(|m| m.insert_temp(egui::Id::unique("edit_symbol"), (index, id)));
        }
    }
    // A symbol dragged from the Library: place an instance where it's dropped.
    // (Into a Symbol Editor too: its document's symbols nest, except where the edited symbol
    // would end up inside itself.)
    if let Some(drag) = resp.dnd_release_payload::<crate::library::SymbolDrag>()
        && (drag.doc == d.id || d.symbol_link.is_some_and(|l| l.parent == drag.doc))
        && let Some(pos) = ctx.pointer_interact_pos()
    {
        if d.would_nest_itself(drag.symbol) {
            app.dialogs.message = Some(("FireCraft".into(), crate::menus::SELF_NEST.into()));
        } else {
            let (x, y) = to_doc(pos);
            d.insert_instance(drag.symbol, x, y);
        }
    }
    if resp.clicked()
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let (x, y) = to_doc(pos);
        match tool {
            Tool::Pointer => match d.hit_test(x, y) {
                Some(hit) if mods.shift => {
                    if let Some(i) = d.selection.iter().position(|r| *r == hit) {
                        d.selection.remove(i);
                    } else {
                        d.selection.push(hit);
                    }
                }
                Some(hit) => {
                    d.selection = vec![hit];
                    d.pixels = None;
                }
                None => d.deselect(),
            },
            Tool::SelectBehind => d.select_behind(x, y),
            Tool::MagicWand => d.magic_wand(x, y, &options, combine),
            Tool::PaintBucket => d.paint_bucket(x, y, &options),
            // A click moves the selected gradient's round handle there.
            Tool::Gradient => {
                if let Some((sx, sy)) = d.selected_gradient().map(|(_, g)| g.start) {
                    d.edit_gradient("Move Fill Handle", false, |g| g.translate(x - sx, y - sy));
                }
            }
            Tool::Marquee | Tool::OvalMarquee => {
                if combine == Combine::Replace {
                    d.pixels = None;
                }
            }
            Tool::Zoom => {
                let f = if mods.alt { 0.5 } else { 2.0 };
                zoom_at(d, view, rect, Some(pos), f);
            }
            Tool::Eyedropper => {
                if let Some(px) = d
                    .doc
                    .render(d.doc.current_frame)
                    .get(x.floor() as i64, y.floor() as i64)
                {
                    app.session.options.fill = Some(px);
                }
            }
            _ => {}
        }
    }

    // ---- Overlays ----------------------------------------------------------------------
    // The Brush and Eraser pointer shows the size of the tip.
    let tip = match tool {
        Tool::Brush if options.stroke.is_some() => Some((options.stroke_size as f32, true)),
        Tool::Eraser => Some((options.paint.eraser_size as f32, options.paint.eraser_round)),
        _ => None,
    };
    if let (Some((size, round)), Some(pos)) = (tip, hover) {
        let screen = size * z;
        if screen >= 4.0 {
            let tip_rect = Rect::from_center_size(pos, vec2(screen, screen));
            let radius = if round { screen / 2.0 } else { 0.0 };
            let line = |c: Color32, grow: f32| {
                p.rect_stroke(
                    tip_rect.expand(grow),
                    radius + grow,
                    Stroke::new(1.0, c),
                    egui::StrokeKind::Middle,
                );
            };
            line(Color32::WHITE, 1.0);
            line(Color32::BLACK, 0.0);
        }
    }
    // Fireworks shows no selection box while a painting tool is active.
    if !view.preview && paint_tool(tool).is_none() {
        // Object selection: light-blue bounds with corner handles.
        let drag_offset = match (tool, dragging, view.drag_start, drag_now) {
            (Tool::Pointer, true, Some(a), Some(b)) if view.live_move.is_none() => b - a,
            _ => Vec2::ZERO,
        };
        for r in &d.selection {
            // A symbol instance shows the rectangle around it, dotted, and nothing else (as in
            // Fireworks; its distorted shape appears with the transform tools).
            if let Some((x, y, w, h)) = d.instance_rect(*r) {
                let q = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
                let mut ring: Vec<Pos2> = q.iter().map(|c| to_screen(c.0, c.1) + drag_offset).collect();
                ring.extend(ring.first().copied());
                p.extend(Shape::dashed_line(
                    &ring,
                    Stroke::new(1.0, theme::HANDLE_BLUE),
                    3.0,
                    3.0,
                ));
                continue;
            }
            // A path shows itself, highlighted (as in Fireworks); rectangles keep their box.
            if let Some(firecraft_doc::ElementKind::Path(path)) = d.element(*r).map(|e| &e.kind)
                && path.rect.is_none()
            {
                for c in &path.contours {
                    let pts: Vec<Pos2> = c
                        .flatten(0.0, 0.0)
                        .into_iter()
                        .map(|(x, y)| to_screen(x, y) + drag_offset)
                        .collect();
                    if pts.len() >= 2 {
                        let line = Stroke::new(1.0, theme::HANDLE_BLUE);
                        p.add(if c.closed {
                            Shape::closed_line(pts, line)
                        } else {
                            Shape::line(pts, line)
                        });
                    }
                    // An open path's ends: blue squares (Fireworks).
                    if !c.closed {
                        for b in [c.points.first(), c.points.last()].into_iter().flatten() {
                            let at = to_screen(b.x, b.y) + drag_offset;
                            p.rect_filled(
                                Rect::from_center_size(at, vec2(5.0, 5.0)),
                                0.0,
                                theme::HANDLE_BLUE,
                            );
                        }
                    }
                }
                continue;
            }
            if let Some((x, y, w, h)) = d.element(*r).and_then(|e| e.bounds()) {
                let br = Rect::from_min_max(to_screen(x, y), to_screen(x + w, y + h)).translate(drag_offset);
                p.rect_stroke(br, 0.0, Stroke::new(1.0, theme::HANDLE_BLUE), StrokeKind::Outside);
                for c in [br.left_top(), br.right_top(), br.left_bottom(), br.right_bottom()] {
                    p.rect_filled(Rect::from_center_size(c, vec2(5.0, 5.0)), 0.0, theme::HANDLE_BLUE);
                }
            }
        }
        // Pixel selection: marching ants along the mask edge.
        if let Some(sel) = &d.pixels
            && let Some(firecraft_doc::ElementKind::Bitmap(b)) = d.element(sel.target).map(|e| &e.kind)
        {
            let phase = (ctx.input(|i| i.time) * 8.0) as usize;
            marching_ants(&p, &sel.mask, b.x as f64, b.y as f64, &to_screen, phase);
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
        // Rubber band for drag tools.
        if let (true, Some(a), Some(b)) = (dragging, view.drag_start, drag_now) {
            let r = Rect::from_two_pos(a, b);
            match tool {
                Tool::Marquee => {
                    p.extend(Shape::dashed_line(
                        &[
                            r.left_top(),
                            r.right_top(),
                            r.right_bottom(),
                            r.left_bottom(),
                            r.left_top(),
                        ],
                        Stroke::new(1.0, Color32::BLACK),
                        4.0,
                        4.0,
                    ));
                }
                Tool::OvalMarquee | Tool::Ellipse => {
                    p.add(egui::epaint::EllipseShape::stroke(
                        r.center(),
                        r.size() / 2.0,
                        Stroke::new(1.0, theme::HANDLE_BLUE),
                    ));
                }
                Tool::Rectangle => {
                    p.rect_stroke(r, 0.0, Stroke::new(1.0, theme::HANDLE_BLUE), StrokeKind::Middle);
                }
                Tool::Line | Tool::Gradient => {
                    p.line_segment([a, b], Stroke::new(1.0, theme::HANDLE_BLUE));
                }
                Tool::Zoom => {
                    p.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Middle);
                }
                _ => {}
            }
        }
    }
    // Mouse highlight: the object a click would select, outlined in red (Fireworks).
    if app.prefs.mouse_highlight
        && !view.preview
        && matches!(tool, Tool::Pointer | Tool::SelectBehind)
        && !dragging
        && let Some(h) = hover
    {
        let (x, y) = to_doc(h);
        let target = if tool == Tool::SelectBehind {
            d.behind_target(x, y)
        } else {
            d.hit_test(x, y)
        };
        if let Some(r) = target
            && !d.selection.contains(&r)
        {
            mouse_highlight(&p, d, r, &to_screen);
        }
    }
    // Pointer readout in the tooltip area is handy while developing; Fireworks shows it in Info.
    let _ = hover;
}

/// Fireworks' mouse highlight: object `r` outlined in red (a path along its own outline,
/// anything else as its box; a symbol instance as its rectangle).
pub fn mouse_highlight(p: &egui::Painter, d: &DocState, r: ElementRef, to_screen: &dyn Fn(f64, f64) -> Pos2) {
    let red = Stroke::new(1.5, Color32::from_rgb(255, 0, 0));
    if let Some((x, y, w, h)) = d.instance_rect(r) {
        let q = [(x, y), (x + w, y), (x + w, y + h), (x, y + h)];
        p.add(Shape::closed_line(
            q.iter().map(|c| to_screen(c.0, c.1)).collect(),
            red,
        ));
        return;
    }
    match d.element(r).map(|e| &e.kind) {
        Some(firecraft_doc::ElementKind::Path(path)) if path.rect.is_none() => {
            for c in &path.contours {
                let pts: Vec<Pos2> = c
                    .flatten(0.0, 0.0)
                    .into_iter()
                    .map(|(x, y)| to_screen(x, y))
                    .collect();
                if pts.len() >= 2 {
                    p.add(if c.closed {
                        Shape::closed_line(pts, red)
                    } else {
                        Shape::line(pts, red)
                    });
                }
                // An open path's ends: red squares (as the selection's blue ones).
                if !c.closed {
                    for b in [c.points.first(), c.points.last()].into_iter().flatten() {
                        p.rect_filled(
                            Rect::from_center_size(to_screen(b.x, b.y), vec2(5.0, 5.0)),
                            0.0,
                            red.color,
                        );
                    }
                }
            }
        }
        Some(_) => {
            if let Some((x, y, w, h)) = d.element(r).and_then(|e| e.bounds()) {
                let br = Rect::from_min_max(to_screen(x, y), to_screen(x + w, y + h));
                p.rect_stroke(br, 0.0, red, StrokeKind::Outside);
            }
        }
        None => {}
    }
}

/// Pan so that document point `c` sits in the middle of the view.
fn center_on(d: &DocState, view: &mut View, c: (f64, f64), ppp: f32) {
    let z = d.zoom / ppp;
    let size = vec2(d.doc.width as f32, d.doc.height as f32) * z;
    view.pan = size / 2.0 - vec2(c.0 as f32, c.1 as f32) * z;
}

fn zoom_at(d: &mut DocState, view: &mut View, rect: Rect, at: Option<Pos2>, factor: f32) {
    let old = d.zoom;
    let new = (old * factor).clamp(1.0 / 16.0, 64.0);
    if let Some(at) = at {
        // Keep the document point under the pointer fixed.
        let center = rect.center() + view.pan;
        let rel = at - center;
        view.pan += rel - rel * (new / old);
    }
    d.zoom = new;
}

/// Transparency checkerboard over `r`, fixed to the screen (its squares don't move with the
/// canvas when scrolling or zooming).
fn checkerboard(p: &egui::Painter, r: Rect) {
    let s = 8.0;
    p.rect_filled(r, 0.0, Color32::WHITE);
    let (col0, row0) = ((r.left() / s).floor() as i64, (r.top() / s).floor() as i64);
    let (col1, row1) = ((r.right() / s).ceil() as i64, (r.bottom() / s).ceil() as i64);
    for row in row0..row1 {
        for col in (col0..col1).filter(|c| (c + row).rem_euclid(2) == 1) {
            let sq = Rect::from_min_size(pos2(col as f32 * s, row as f32 * s), vec2(s, s)).intersect(r);
            if sq.is_positive() {
                p.rect_filled(sq, 0.0, Color32::from_gray(204));
            }
        }
    }
}

/// Draw the 50% contour of `mask` (offset by the bitmap position) as alternating dashes.
fn marching_ants(
    p: &egui::Painter,
    mask: &Mask,
    ox: f64,
    oy: f64,
    to_screen: &impl Fn(f64, f64) -> Pos2,
    phase: usize,
) {
    let inside = |x: i64, y: i64| mask.get(x, y) >= 128;
    let (w, h) = (mask.width() as i64, mask.height() as i64);
    // Guard very large masks: draw a bounding box instead of every edge.
    if w * h > 4_000_000 {
        if let Some((x, y, bw, bh)) = mask.bounds() {
            let r = Rect::from_min_max(
                to_screen(ox + x as f64, oy + y as f64),
                to_screen(ox + (x + bw) as f64, oy + (y + bh) as f64),
            );
            p.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Middle);
        }
        return;
    }
    let mut n = 0usize;
    // Dash by position along x+y so dashes line up across segments and "march" with `phase`.
    let seg = |a: (f64, f64), b: (f64, f64), n: &mut usize| {
        *n += 1;
        let k = (a.0 + a.1) as i64 + (phase as i64);
        let color = if k.rem_euclid(8) < 4 {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        // Pixel-centre aligned so 1 px lines stay crisp.
        let (pa, pb) = (to_screen(ox + a.0, oy + a.1), to_screen(ox + b.0, oy + b.1));
        let o = vec2(0.5, 0.5);
        p.line_segment([pa.floor() + o, pb.floor() + o], Stroke::new(1.0, color));
    };
    for y in 0..=h {
        for x in 0..=w {
            let here = inside(x, y);
            if here != inside(x - 1, y) {
                seg((x as f64, y as f64), (x as f64, (y + 1) as f64), &mut n);
            }
            if here != inside(x, y - 1) {
                seg((x as f64, y as f64), ((x + 1) as f64, y as f64), &mut n);
            }
        }
    }
}
