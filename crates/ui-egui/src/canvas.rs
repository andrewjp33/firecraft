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
                .color({
                    let p = theme::pal();
                    match (p.modern, p.dark) {
                        (false, false) => Color32::from_gray(230),
                        (true, false) => Color32::from_gray(0x50),
                        _ => Color32::from_gray(0x90),
                    }
                }),
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
    let mut restore = false;
    // Modern: the tabs sit right on the document below. egui adds the gap when it places the
    // tab row, so it goes before; the document view gets the usual spacing back.
    let spacing = ui.spacing().item_spacing.y;
    if theme::pal().modern {
        ui.spacing_mut().item_spacing.y = 0.0;
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 1.0;
        for (i, d) in app.session.docs.iter().enumerate() {
            let active = i == app.session.active;
            let title = format!("{}{}", d.doc.title(), if d.dirty { " *" } else { "" });
            let p = theme::pal();
            let r = p.r_widget.saturating_mul(if p.modern { 2 } else { 0 });
            egui::Frame::new()
                .fill(if active { p.tab_active } else { p.tab_inactive })
                .corner_radius(egui::CornerRadius { nw: r, ne: r, sw: 0, se: 0 })
                .inner_margin(egui::Margin::symmetric(if p.modern { 10 } else { 6 }, 2))
                .show(ui, |ui| {
                    // A button, not selectable text.
                    let name = egui::Label::new(title).selectable(false).sense(Sense::click());
                    if ui.add(name).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
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
        // Tabs are how maximised documents show (as in Fireworks): Restore goes back to
        // document windows.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(4.0);
            restore = restore_button(ui).clicked();
        });
    });
    if restore {
        app.prefs.tabs = false;
    }
    if let Some(i) = close {
        app.session.close(i);
        return;
    }
    let i = app.session.active;
    let p = theme::pal();
    let body = if p.modern {
        // A rounded card like a maximised window's, square only where the first tab joins it.
        let r = p.r_card;
        egui::Frame::new()
            .fill(p.title_active)
            .corner_radius(egui::CornerRadius { nw: 0, ne: r, sw: r, se: r })
            // Room inside the rounded corners for the status bar's first and last controls.
            .inner_margin(egui::Margin { left: 3, right: 3, top: 1, bottom: 3 })
            .shadow(ui.visuals().window_shadow)
    } else {
        egui::Frame::new().fill(p.panel)
    };
    body.show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = spacing;
        ui.set_min_size(ui.available_size());
        document_view(app, ui, i);
    });
}

/// The tab strip's Restore button (back to document windows), drawn like a title bar's.
fn restore_button(ui: &mut egui::Ui) -> egui::Response {
    let tip = "Restore (show documents in windows)";
    let (r, b) = ui.allocate_exact_size(vec2(16.0, 14.0), Sense::click());
    let b = b.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand);
    b.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, tip));
    let pal = theme::pal();
    if b.hovered() {
        let fill = if pal.dark || pal.modern { pal.button_hover } else { Color32::from_gray(190) };
        ui.painter().rect_filled(r, 2.0, fill);
    }
    let c = r.center();
    let s = Stroke::new(1.3, pal.title_text);
    ui.painter().rect_stroke(
        Rect::from_center_size(c + vec2(-1.0, 1.0), vec2(7.0, 6.0)),
        0.0,
        s,
        StrokeKind::Middle,
    );
    ui.painter().line_segment([c + vec2(-1.0, -4.0), c + vec2(5.0, -4.0)], s);
    ui.painter().line_segment([c + vec2(5.0, -4.0), c + vec2(5.0, 1.0)], s);
    b
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
    let pal = theme::pal();
    let fill = if active { pal.title_active } else { pal.title_inactive };
    // Modern windows are rounded: the bar follows the window's top corners.
    let r = pal.r_card.saturating_sub(2);
    p.rect_filled(rect, egui::CornerRadius { nw: r, ne: r, sw: 0, se: 0 }, fill);
    let text = if active { pal.title_text } else { pal.title_text_inactive };
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
            ui.painter().rect_filled(r, 2.0, if pal.dark { pal.button_hover } else { Color32::from_gray(190) });
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
            // Maximising shows the documents in tabs, which look like a maximised window with
            // a tab strip for its title (as in Fireworks); the window itself stays as it was,
            // for when Restore goes back to windows.
            TitleAction::Set(WinState::Maximized) => {
                view.win = WinState::Normal;
                app.session.active = i;
                app.prefs.tabs = true;
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
                theme::mode_value(ui, &mut preview, false, "Original");
                theme::mode_value(ui, &mut preview, true, "Preview");
                ui.add_enabled(false, egui::Button::new("2-Up"))
                    .on_disabled_hover_text("Coming soon (Preview works)");
                ui.add_enabled(false, egui::Button::new("4-Up"))
                    .on_disabled_hover_text("Coming soon (Preview works)");
                // Preview: the export's format and file size, as Fireworks shows them.
                if preview
                    && let Some(o) = app.views.get(&id).and_then(|v| v.optimized.as_ref())
                    && let Some(d) = app.session.docs.get(index)
                {
                    match &o.result {
                        Ok((bytes, _)) => {
                            ui.label(format!("{}  {}", d.doc.export.format.label(), file_size(*bytes)));
                        }
                        Err(e) => {
                            ui.colored_label(theme::pal().error, e);
                        }
                    }
                }
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
    // Modern: room left of the first frame button (inside the rounded corner), and above the
    // row as much as the document's padding leaves below it, so the controls sit centred.
    let status_frame = if theme::pal().modern {
        egui::Frame::new().inner_margin(egui::Margin { left: 2, right: 0, top: 3, bottom: 0 })
    } else {
        egui::Frame::new()
    };
    egui::Panel::bottom(egui::Id::unique(("doc_status", id)))
        .frame(status_frame)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(d) = app.session.docs.get_mut(index) {
                    let n = d.doc.frames.len().max(1);
                    let view = app.views.entry(id).or_default();
                    let multi = n > 1;
                    // Playback: advance by each frame's delay (1/100 s), before the controls are drawn so
                    // they show the frame being shown.
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
                    let cur = d.doc.current_frame;
                    if crate::icons::glyph_button(ui, crate::icons::Glyph::First, 14.0, multi && cur > 0)
                        .on_hover_text("First frame")
                        .clicked()
                    {
                        // Going to a frame stops playback.
                        view.playing = false;
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
                    if crate::icons::glyph_button(ui, crate::icons::Glyph::Last, 14.0, multi && cur + 1 < n)
                        .on_hover_text("Last frame")
                        .clicked()
                    {
                        view.playing = false;
                        d.doc.current_frame = n - 1;
                    }
                    frame_number(ui, id, d, n);
                    // One frame back or on, wrapping round (Fireworks MX's footer; Ctrl+Page Up/Down).
                    if crate::icons::glyph_button(ui, crate::icons::Glyph::PrevFrame, 14.0, multi)
                        .on_hover_text("Previous frame (Ctrl+Page Up)")
                        .clicked()
                    {
                        view.playing = false;
                        d.doc.current_frame = (d.doc.current_frame + n - 1) % n;
                    }
                    if crate::icons::glyph_button(ui, crate::icons::Glyph::NextFrame, 14.0, multi)
                        .on_hover_text("Next frame (Ctrl+Page Down)")
                        .clicked()
                    {
                        view.playing = false;
                        d.doc.current_frame = (d.doc.current_frame + 1) % n;
                    }
                    // Exit Bitmap Mode: red while a bitmap is being edited (Fireworks MX's footer).
                    let editing = d.in_bitmap_mode();
                    if exit_bitmap_button(ui, editing)
                        .on_hover_text("Exit Bitmap Mode")
                        .on_disabled_hover_text("Exit Bitmap Mode")
                        .clicked()
                    {
                        d.exit_bitmap_mode();
                    }
                    ui.separator();
                    ui.label(format!("{} x {}", d.doc.width, d.doc.height));
                    ui.separator();
                    crate::theme::combo(("zoom", id))
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
    if resp.is_pointer_button_down_on() || resp.clicked() || resp.secondary_clicked() {
        app.session.active = index;
    }
    canvas(app, ui, index, rect, &resp);
    // Right-click: Fireworks MX's context menu (not with the Zoom tool: its right-click zooms
    // out).
    if app.session.tool != Tool::Zoom {
        egui::Popup::context_menu(&resp)
            .style(theme::menu_popup_style())
            .show(|ui| crate::menus::context_menu(app, ui));
    }
}

/// The footer's frame number: type a frame and press Enter to go there (Escape: back). It keeps
/// its size while edited, so the footer doesn't move.
fn frame_number(ui: &mut egui::Ui, id: u64, d: &mut firecraft_engine::DocState, n: usize) {
    let key = egui::Id::unique(("frame_number", id));
    let shown = (d.doc.current_frame + 1).to_string();
    let mut text: String = ui.ctx().data(|m| m.get_temp(key)).unwrap_or_else(|| shown.clone());
    let field = egui::TextEdit::singleline(&mut text)
        .frame(egui::Frame::NONE)
        .align(egui::Align2::CENTER_CENTER)
        .char_limit(5)
        .desired_width(26.0);
    let resp = ui.add_sized([30.0, ui.spacing().interact_size.y], field);
    // Clicked into: the whole number is selected, so typing replaces it.
    if resp.gained_focus()
        && let Some(mut state) = egui::text_edit::TextEditState::load(ui.ctx(), resp.id)
    {
        state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::select_all_str(&text)));
        state.store(ui.ctx(), resp.id);
    }
    if resp.has_focus() {
        // Show that it's being edited without changing the layout.
        ui.painter().rect_stroke(
            resp.rect,
            1.0,
            egui::Stroke::new(1.0, theme::select()),
            egui::StrokeKind::Inside,
        );
    }
    let (enter, escape) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
    if resp.lost_focus() {
        if enter
            && !escape
            && let Ok(f) = text.trim().parse::<usize>()
            && (1..=n).contains(&f)
        {
            d.doc.current_frame = f - 1;
        }
        ui.ctx().data_mut(|m| m.remove::<String>(key));
    } else if resp.has_focus() {
        ui.ctx().data_mut(|m| m.insert_temp(key, text));
    } else {
        ui.ctx().data_mut(|m| m.remove::<String>(key));
    }
}

/// Fireworks MX's Exit Bitmap Mode button: a round stop sign, red when there is bitmap editing
/// to end, grey otherwise.
fn exit_bitmap_button(ui: &mut egui::Ui, enabled: bool) -> egui::Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (r, resp) = ui.allocate_exact_size(vec2(14.0, 14.0), sense);
    let p = ui.painter();
    if resp.hovered() && enabled {
        p.rect_stroke(r, 1.0, Stroke::new(1.0, Color32::from_gray(150)), StrokeKind::Inside);
    }
    let pal = theme::pal();
    let (fill, cross) = if enabled {
        (Color32::from_rgb(210, 30, 30), Color32::WHITE)
    } else if pal.dark {
        // As dim as the disabled frame buttons beside it.
        (pal.glyph_disabled, theme::lerp_color(pal.glyph_disabled, Color32::BLACK, 0.45))
    } else {
        (Color32::from_gray(175), Color32::from_gray(235))
    };
    let c = r.center();
    p.circle_filled(c, 5.0, fill);
    let d = 2.2;
    p.line_segment([c + vec2(-d, -d), c + vec2(d, d)], Stroke::new(1.4, cross));
    p.line_segment([c + vec2(-d, d), c + vec2(d, -d)], Stroke::new(1.4, cross));
    resp
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

/// Tools that drag the marquee when pressed inside it (Fireworks MX).
fn marquee_moves(tool: Tool) -> bool {
    matches!(tool, Tool::Marquee | Tool::OvalMarquee | Tool::Lasso | Tool::MagicWand)
}

/// How far in from the view's edge a guide can always be grabbed, in screen points.
const GUIDE_EDGE: f32 = 10.0;

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
    // Whether Enter and Escape are free for the crop box (no dialog or text field wants them).
    let keys_free = app.escape_is_free(&ctx);
    // A press on a window makes it active in time for the tool to act on that press.
    if resp.hovered() && ctx.input(|i| i.pointer.primary_pressed()) {
        app.session.active = index;
    }
    let Some(d) = app.session.docs.get_mut(index) else {
        return;
    };
    let view = app.views.entry(d.id).or_default();
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, theme::pal().pasteboard);

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
        let options = egui::TextureOptions::NEAREST;
        let name = format!("doc-{}", d.id);
        match (&mut view.texture, dirty) {
            // Only part of the picture changed: upload just that part.
            (Some((stored, _)), firecraft_doc::Dirty::None) => *stored = key,
            (Some((stored, tex)), firecraft_doc::Dirty::Rect { x, y, w, h }) => {
                tex.set_rect(&img, x as usize, y as usize, w as usize, h as usize, options);
                // The cache key (not just the revision), or every frame looks stale.
                *stored = key;
            }
            (Some((stored, tex)), firecraft_doc::Dirty::Full) => {
                tex.set(&ctx, &name, &img, options);
                *stored = key;
            }
            (None, _) => view.texture = Some((key, crate::tiles::TiledTexture::new(&ctx, &name, &img, options))),
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
    // Preview: the document as it exports (the Optimize panel's settings).
    if view.preview {
        refresh_optimized(view, d, &ctx);
    }
    // A color table swatch held down: the Preview with that color's pixels in the highlight
    // color, each keeping its opacity (Fireworks MX), in place of the export's picture.
    let held = app.optimize_table.held.filter(|&(doc, _)| doc == d.id && view.preview);
    if let (Some((_, c)), Some(o)) = (held, view.optimized.as_mut())
        && o.highlight.as_ref().is_none_or(|h| (h.0, h.1) != (c, o.frame))
        && let Some(pic) = o.picture.as_ref()
    {
        let mark = crate::optimize::highlight_color(c);
        let px: Vec<firecraft_raster::Rgba> = pic
            .pixels()
            .iter()
            .map(|&q| if crate::optimize::shows(q, c) { firecraft_raster::Rgba { a: q.a, ..mark } } else { q })
            .collect();
        if let Some(img) = firecraft_raster::RgbaImage::from_pixels(pic.width(), pic.height(), px) {
            let tex = crate::tiles::TiledTexture::new(
                &ctx,
                &format!("optimized-highlight-{}", d.id),
                &img,
                egui::TextureOptions::NEAREST,
            );
            o.highlight = Some((c, o.frame, tex));
        }
    }
    let highlighted = held.and_then(|_| view.optimized.as_ref()).and_then(|o| o.highlight.as_ref()).map(|h| &h.2);
    let shown = if view.preview { view.optimized.as_ref().and_then(|o| o.texture.as_ref()) } else { None };
    if let Some(tex) = highlighted.or(shown).or(view.texture.as_ref().map(|t| &t.1)) {
        tex.paint(&p, canvas_rect);
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
        // 9-slice guides (dashed, draggable), as in Fireworks CS. They come last for the pointer:
        // tools, handles and objects get the press, leaving guides the empty canvas and, whatever
        // is there or whichever tool is chosen, the edges of the view.
        if let Some((gl, gr, gt, gb)) = d.symbol_nine {
            let (ox, oy) = link.offset;
            let area = canvas_rect.intersect(rect);
            let guide = Stroke::new(1.0, Color32::from_rgb(0, 0, 255));
            let pointer_at = ctx.pointer_hover_pos();
            let guide_free = pointer_at.is_some_and(|q| {
                let (x, y) = to_doc(q);
                !area.shrink(GUIDE_EDGE).contains(q)
                    || (matches!(tool, Tool::Pointer | Tool::SelectBehind)
                        && !crate::gradient_ui::over_handle(d, q, (origin, z))
                        && !d.selection_contains(x, y)
                        && d.hit_test(x, y).is_none())
            });
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
                let id = egui::Id::unique(("nine_guide", d.id, k));
                if !ctx.is_being_dragged(id) && !guide_free {
                    continue;
                }
                let g = ui.interact(zone, id, Sense::drag());
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
        // Over the pixel selection a marquee tool or the Magic Wand drags it: a move cursor.
        let plain = !ctx.input(|i| i.modifiers.shift || i.modifiers.alt);
        let over_marquee = marquee_moves(tool)
            && (view.marquee_drag
                || plain
                    && resp.hover_pos().is_some_and(|pos| {
                        let (x, y) = to_doc(pos);
                        d.pixels_contain(x, y)
                    }));
        if over_marquee {
            ctx.set_cursor_icon(egui::CursorIcon::Move);
        }
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

    // The Preview pixel under the pointer, for the color table's swatch feedback (Fireworks MX).
    let under = resp
        .hover_pos()
        .filter(|_| view.preview)
        .and_then(|pos| {
            let (x, y) = to_doc(pos);
            let pic = view.optimized.as_ref().and_then(|o| o.picture.as_ref())?;
            pic.get(x.floor() as i64, y.floor() as i64)
        });
    match under {
        Some(c) => app.optimize_table.hovered = Some((d.id, c)),
        None if app.optimize_table.hovered.is_some_and(|(doc, _)| doc == d.id) => app.optimize_table.hovered = None,
        None => {}
    }
    // The Optimize panel's transparency eyedroppers take their color from the Preview (Fireworks
    // MX); no tool acts meanwhile.
    if view.preview && app.optimize_table.pick.is_some() {
        if resp.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if resp.clicked()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            let (x, y) = to_doc(pos);
            let picture = view.optimized.as_ref().and_then(|o| o.picture.as_ref());
            if let Some(c) = picture.and_then(|img| img.get(x.floor() as i64, y.floor() as i64)) {
                app.optimize_table.picked = Some((d.id, c));
            }
        }
        return;
    }
    // Tools with editing state of their own (one for the app) work in the active document
    // only; other windows just show their document (Fireworks shows handles in one window).
    let active = index == app.session.active;
    if !active
        && matches!(
            tool,
            Tool::NineSlice | Tool::Scale | Tool::Skew | Tool::Distort | Tool::Pen | Tool::Subselect | Tool::Crop
        )
    {
        return;
    }
    // The Subselection tool's double-click opens a symbol inside groups and masks too
    // (Fireworks MX): the instance it reaches is the one under the pointer, however deep.
    if resp.double_clicked()
        && tool == Tool::Subselect
        && let Some(pos) = resp.interact_pointer_pos()
    {
        let (x, y) = to_doc(pos);
        // A selected symbol under the pointer first (chosen in the Layers panel, inside masks
        // too; a masked one counts as its symbol), then the deepest one there.
        let selected = d
            .hit_test(x, y)
            .and_then(|hit| d.selected_under(x, y, hit))
            .map(|r| d.paint_ref(r))
            .and_then(|r| d.instance_symbol(r));
        if let Some(id) = selected.or_else(|| d.hit_deep(x, y).and_then(|r| d.instance_symbol(r))) {
            ctx.data_mut(|m| m.insert_temp(egui::Id::unique("edit_symbol"), (index, id)));
        }
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
    // The Crop tool too (Escape and finished bitmap crops go back to the Pointer tool).
    if tool == Tool::Crop {
        if crate::crop_tool::canvas(&mut app.crop, d, &p, resp, (origin, z), keys_free) {
            app.session.tool = Tool::Pointer;
        }
        return;
    }
    // The Hand and Zoom tools (Space too) keep the crop box and show it.
    if matches!(tool, Tool::Hand | Tool::Zoom) && active {
        crate::crop_tool::overlay(&app.crop, d, &p, (origin, z));
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

    // The motion path of a selected animation symbol: its points drag with the Pointer
    // (Fireworks MX), over the object underneath.
    if active
        && matches!(tool, Tool::Pointer | Tool::SelectBehind)
        && crate::motion_path::handles(&mut app.path_drag, d, resp, (origin, z))
    {
        // The overlay below isn't reached while the path is busy: show the path as it is now,
        // so the drag can be judged (Fireworks MX draws it moving; its selection box hides).
        crate::motion_path::draw(&p, d, &to_screen);
        return;
    }

    // Fill handles of a selected gradient (over the tool underneath). Their drag is shared between
    // windows, so they belong to the active one: another would cancel the drag.
    if active
        && matches!(tool, Tool::Pointer | Tool::SelectBehind | Tool::Gradient)
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
        // A marquee tool or the Magic Wand pressed inside the pixel selection drags the marquee
        // (Fireworks MX); with Shift or Alt it draws one to add, subtract or intersect.
        view.marquee_drag = marquee_moves(tool)
            && combine == Combine::Replace
            && view.drag_start.is_some_and(|pos| {
                let (x, y) = to_doc(pos);
                d.pixels_contain(x, y)
            });
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
                    // Empty canvas: a selection band (Fireworks MX). With Shift it adds to and
                    // removes from the selection, so that stays for now.
                    None => {
                        if !mods.shift {
                            d.deselect();
                        }
                        view.band = tool == Tool::Pointer;
                    }
                },
            }
            // Live dragging: the objects themselves move (one undo step for the whole drag).
            // With Alt, copies move and the originals stay (Fireworks MX's Alt+drag).
            if live_drag && !d.selection.is_empty() && !view.drag_secondary && !view.band {
                view.drag_originals = d.selection.clone();
                view.live_dup = mods.alt;
                d.begin_drag(mods.alt);
                view.live_move = Some((0.0, 0.0));
            }
        }
    }
    let drag_now = resp.interact_pointer_pos();
    let dragging = resp.dragged() && view.drag_start.is_some();
    // Alt pressed or let go during the drag: what Alt says at the release decides between
    // moving and duplicating (Fireworks MX), so switch the live drag now.
    if let Some((dx, dy)) = view.live_move
        && mods.alt != view.live_dup
    {
        view.live_dup = mods.alt;
        d.switch_drag(mods.alt, &view.drag_originals, dx, dy);
        ctx.request_repaint();
    }
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
                _ if view.band => d.select_in_rect((ax, ay, bx, by), mods.shift),
                _ if view.marquee_drag => {
                    d.move_pixel_selection((bx - ax).round() as i64, (by - ay).round() as i64)
                }
                // A live move already moved the objects: keep it, or drop it if it went nowhere.
                Tool::Pointer | Tool::SelectBehind if view.live_move.is_some() => {
                    if view.live_move == Some((0.0, 0.0)) {
                        // An Alt+click makes no copy; the objects stay selected.
                        d.cancel_live_edit();
                        d.selection = std::mem::take(&mut view.drag_originals);
                    } else {
                        d.commit_live_edit();
                    }
                }
                Tool::Pointer | Tool::SelectBehind if mods.alt => {
                    let (dx, dy) = ((bx - ax).round(), (by - ay).round());
                    if dx != 0.0 || dy != 0.0 {
                        d.duplicate_moved(dx, dy);
                    }
                }
                Tool::Pointer | Tool::SelectBehind => {
                    d.move_selection((bx - ax).round(), (by - ay).round(), "Move")
                }
                Tool::Marquee | Tool::OvalMarquee => {
                    d.marquee((ax, ay, bx, by), tool == Tool::OvalMarquee, &options, combine)
                }
                // Corners on the nearest pixel grid lines, so the shape's edges are crisp; Ctrl
                // draws it where the pointer is, sub-pixel (as Ctrl with the Scale tool).
                Tool::Rectangle | Tool::Ellipse => {
                    let rect = if mods.ctrl || mods.command {
                        (ax, ay, bx, by)
                    } else {
                        (ax.round(), ay.round(), bx.round(), by.round())
                    };
                    d.add_shape(rect, tool == Tool::Ellipse, &options)
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
        view.live_dup = false;
        view.drag_originals.clear();
        view.marquee_drag = false;
        view.band = false;
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
        // A selected symbol under the pointer opens, even inside a mask or group (chosen in
        // the Layers panel); otherwise the top-level one.
        let selected = d.hit_test(x, y).and_then(|hit| d.selected_under(x, y, hit));
        if let Some(id) = selected
            .or_else(|| d.hit_test(x, y))
            .map(|r| d.paint_ref(r))
            .and_then(|r| d.instance_symbol(r))
        {
            ctx.data_mut(|m| m.insert_temp(egui::Id::unique("edit_symbol"), (index, id)));
        }
        // Double-clicking a bitmap edits it: bitmap mode, with the Marquee tool (Fireworks MX).
        if let Some(r) = d.hit_test(x, y)
            && d.enter_bitmap_mode(r)
        {
            app.session.tool = Tool::Marquee;
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
                // A click selects the top-level object, a masked one whole (Fireworks MX).
                Some(hit) => {
                    d.selection = vec![hit];
                    d.pixels = None;
                }
                None => d.deselect(),
            },
            Tool::SelectBehind => d.select_behind(x, y),
            // A click inside the pixel selection ends it (Fireworks MX, recorded as "Select
            // Pixels"); elsewhere the wand selects.
            Tool::MagicWand if combine == Combine::Replace && d.pixels_contain(x, y) => d.deselect_pixels(),
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
                    d.deselect_pixels();
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
    // Pixel selection: marching ants along the mask edge, with every tool (Brush and Pencil
    // paint inside it). Anchored to the canvas; while the marquee is dragged it shows where it
    // will land.
    if !view.preview
        && let Some(sel) = &d.pixels
    {
        let (dx, dy) = match (view.marquee_drag, view.drag_start, drag_now) {
            (true, Some(a), Some(b)) => {
                let ((ax, ay), (bx, by)) = (to_doc(a), to_doc(b));
                ((bx - ax).round(), (by - ay).round())
            }
            _ => (0.0, 0.0),
        };
        let phase = (ctx.input(|i| i.time) * 8.0) as usize;
        marching_ants(&p, &sel.mask, sel.x as f64 + dx, sel.y as f64 + dy, &to_screen, phase);
        ctx.request_repaint_after(std::time::Duration::from_millis(120));
    }
    // Fireworks shows no selection box while a painting tool is active.
    if !view.preview && paint_tool(tool).is_none() {
        // Object selection: light-blue bounds with corner handles.
        let drag_offset = match (tool, dragging, view.drag_start, drag_now) {
            (Tool::Pointer, true, Some(a), Some(b)) if view.live_move.is_none() && !view.band => b - a,
            _ => Vec2::ZERO,
        };
        // A selected animation symbol: its motion path, as in Fireworks MX.
        crate::motion_path::draw(&p, d, &to_screen);
        // While pixels are selected (bitmap editing) only the marquee shows, as in Fireworks MX.
        let boxed: &[firecraft_engine::ElementRef] = if d.in_bitmap_mode() { &[] } else { &d.selection };
        for r in boxed {
            // An empty bitmap (Insert > Empty Bitmap) shows no box, as in Fireworks MX.
            if d.is_empty_bitmap(*r) {
                continue;
            }
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
            // The whole pixels the object covers, soft edges included (what the Property
            // inspector's W, H, X and Y show).
            if let Some((x, y, w, h)) = d.element(*r).and_then(|e| e.bounds()).map(firecraft_engine::covered_pixels) {
                let br = Rect::from_min_max(to_screen(x, y), to_screen(x + w, y + h)).translate(drag_offset);
                p.rect_stroke(br, 0.0, Stroke::new(1.0, theme::HANDLE_BLUE), StrokeKind::Outside);
                for c in [br.left_top(), br.right_top(), br.left_bottom(), br.right_bottom()] {
                    p.rect_filled(Rect::from_center_size(c, vec2(5.0, 5.0)), 0.0, theme::HANDLE_BLUE);
                }
            }
        }
        // Rubber band for drag tools.
        if let (true, false, Some(a), Some(b)) = (dragging, view.marquee_drag, view.drag_start, drag_now) {
            let r = Rect::from_two_pos(a, b);
            // The marquee's box sits on the canvas's pixel grid, as the Crop tool's: what it shows
            // is what will be selected (and the Rectangle and Ellipse tools' shape).
            let grid = || {
                let ((ax, ay), (bx, by)) = (to_doc(a), to_doc(b));
                let (l, t, r, b) = firecraft_engine::DocState::marquee_box((ax, ay, bx, by));
                Rect::from_min_max(to_screen(l, t), to_screen(r, b))
            };
            match tool {
                Tool::Marquee => {
                    // Where the marching ants will run once it's let go (and the Crop tool's box).
                    // On its outermost pixels: right and bottom on the last pixel inside, as
                    // Fireworks MX draws marching ants.
                    let r = grid();
                    let lt = r.min.floor() + vec2(0.5, 0.5);
                    let rb = (r.max.ceil() - vec2(0.5, 0.5)).max(lt);
                    p.extend(Shape::dashed_line(
                        &[lt, pos2(rb.x, lt.y), rb, pos2(lt.x, rb.y), lt],
                        Stroke::new(1.0, Color32::BLACK),
                        4.0,
                        4.0,
                    ));
                }
                Tool::OvalMarquee => {
                    let r = grid();
                    p.add(egui::epaint::EllipseShape::stroke(
                        r.center(),
                        r.size() / 2.0,
                        Stroke::new(1.0, theme::HANDLE_BLUE),
                    ));
                }
                // The shape as it will be drawn: on the pixel grid unless Ctrl is held.
                Tool::Ellipse => {
                    let r = if mods.ctrl || mods.command { r } else { grid() };
                    p.add(egui::epaint::EllipseShape::stroke(
                        r.center(),
                        r.size() / 2.0,
                        Stroke::new(1.0, theme::HANDLE_BLUE),
                    ));
                }
                Tool::Rectangle => {
                    let r = if mods.ctrl || mods.command { r } else { grid() };
                    p.rect_stroke(r, 0.0, Stroke::new(1.0, theme::HANDLE_BLUE), StrokeKind::Middle);
                }
                Tool::Line | Tool::Gradient => {
                    p.line_segment([a, b], Stroke::new(1.0, theme::HANDLE_BLUE));
                }
                // The Pointer's selection band: a thin black rectangle, as in Fireworks MX.
                Tool::Zoom | Tool::Pointer if tool == Tool::Zoom || view.band => {
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
    // Each edge from canvas point `a` to `b` (one pixel long, across or down), drawn as whole
    // screen pixels on the selected side of it, as Fireworks MX draws them: the selection's
    // left and top edges on its first pixel, its right and bottom edges on its last pixel
    // (measured 2026-11-26), so a selection of the whole canvas runs along the canvas's own
    // edge pixels. `after`: the selection lies right of (below) the edge. Dashed by position
    // along x+y, so dashes line up across edges and "march" with `phase`. The pixels go into
    // one mesh of plain squares: 1 px line segments get their ends rounded by the tessellator,
    // and one a pixel long from edge to edge vanishes.
    let mut mesh = egui::Mesh::default();
    let mut seg = |a: (f64, f64), b: (f64, f64), after: bool| {
        let k = (a.0 + a.1) as i64 + (phase as i64);
        let color = if k.rem_euclid(8) < 4 {
            Color32::BLACK
        } else {
            Color32::WHITE
        };
        let (pa, pb) = (to_screen(ox + a.0, oy + a.1), to_screen(ox + b.0, oy + b.1));
        let side = |v: f32| if after { v.floor() } else { v.ceil() - 1.0 };
        let r = if a.0 == b.0 {
            let x = side(pa.x);
            Rect::from_min_max(pos2(x, pa.y.floor()), pos2(x + 1.0, pb.y.floor()))
        } else {
            let y = side(pa.y);
            Rect::from_min_max(pos2(pa.x.floor(), y), pos2(pb.x.floor(), y + 1.0))
        };
        if r.is_positive() {
            mesh.add_colored_rect(r, color);
        }
    };
    for y in 0..=h {
        for x in 0..=w {
            let here = inside(x, y);
            if here != inside(x - 1, y) {
                seg((x as f64, y as f64), (x as f64, (y + 1) as f64), here);
            }
            if here != inside(x, y - 1) {
                seg((x as f64, y as f64), ((x + 1) as f64, y as f64), here);
            }
        }
    }
    p.add(egui::Shape::mesh(mesh));
}

/// The Preview tab's export of a document, kept until the document, its frame or its export
/// settings change.
#[derive(Clone)]
pub(crate) struct Optimized {
    key: (u64, usize, String),
    texture: Option<crate::tiles::TiledTexture>,
    /// File size in bytes and palette colours, or why it couldn't be exported.
    pub result: Result<(usize, usize), String>,
    /// An indexed export's palette (the Optimize panel's color table), its transparent entry
    /// and how many pixels use each entry.
    pub palette: Vec<[u8; 4]>,
    pub transparent: Option<usize>,
    pub usage: Vec<u32>,
    /// The shown frame as exported (the Optimize panel's transparency eyedroppers read it).
    pub picture: Option<firecraft_raster::RgbaImage>,
    /// An animated GIF's every frame, and the frame the texture shows: another frame (playing,
    /// stepping) is shown from it without exporting again.
    pub animation: Option<std::sync::Arc<firecraft_io::export::AnimatedPreview>>,
    frame: usize,
    /// The pixels of a held color table swatch, in the highlight color: (color, frame, texture).
    highlight: Option<(firecraft_raster::Rgba, usize, crate::tiles::TiledTexture)>,
}

/// What the preview depends on. An animated GIF's export holds every frame, so not the current
/// frame.
fn optimize_key(d: &firecraft_engine::DocState) -> (u64, usize, String) {
    let mut s = d.doc.export.clone();
    s.raw = None;
    let frame = if s.format == firecraft_doc::ExportFormat::AnimatedGif {
        usize::MAX
    } else {
        d.doc.current_frame
    };
    (d.revision, frame, format!("{s:?}"))
}

/// Export the document again if anything it depends on changed since the last preview.
pub(crate) fn refresh_optimized(view: &mut crate::View, d: &firecraft_engine::DocState, ctx: &egui::Context) {
    let key = optimize_key(d);
    let frame = d.doc.current_frame;
    if let Some(o) = view.optimized.as_mut().filter(|o| o.key == key) {
        // The same export: another frame of an animation is shown from the kept frames.
        if o.frame != frame
            && let (Some(a), Some(tex)) = (o.animation.as_ref(), o.texture.as_mut())
        {
            let picture = a.frame(frame);
            tex.set(ctx, &format!("optimized-{}", d.id), &picture, egui::TextureOptions::NEAREST);
            o.picture = Some(picture);
            // Each frame's own color table ("Maximize colors per frame"): the Optimize panel
            // shows the one of the frame shown.
            if let Some(t) = a.table(frame) {
                o.palette.clone_from(&t.palette);
                o.transparent = t.transparent;
                o.usage.clone_from(&t.usage);
            }
            o.frame = frame;
        }
        return;
    }
    let mut o = Optimized {
        key,
        texture: None,
        result: Err(String::new()),
        palette: Vec::new(),
        transparent: None,
        usage: Vec::new(),
        picture: None,
        animation: None,
        frame,
        highlight: None,
    };
    match firecraft_io::export::export(&d.doc, &d.doc.export) {
        Ok(e) => {
            o.texture = Some(crate::tiles::TiledTexture::new(
                ctx,
                &format!("optimized-{}", d.id),
                &e.preview,
                egui::TextureOptions::NEAREST,
            ));
            o.result = Ok((e.bytes.len(), e.palette.len()));
            o.palette = e.palette;
            o.transparent = e.transparent;
            o.usage = e.usage;
            o.picture = Some(e.preview);
            o.animation = e.animation;
        }
        Err(e) => o.result = Err(e.to_string()),
    }
    view.optimized = Some(o);
}

/// Whether the kept export still shows the document as it is (else the Optimize panel's color
/// table offers Rebuild, as Fireworks MX's does).
pub(crate) fn optimized_current(view: &crate::View, d: &firecraft_engine::DocState) -> bool {
    view.optimized.as_ref().is_some_and(|o| o.key == optimize_key(d))
}

/// "4.6K" / "820 bytes": a file size as Fireworks shows it.
pub(crate) fn file_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} bytes")
    } else if bytes < 1024 * 1024 {
        format!("{:.2}K", bytes as f64 / 1024.0)
    } else {
        format!("{:.2}M", bytes as f64 / (1024.0 * 1024.0))
    }
}
