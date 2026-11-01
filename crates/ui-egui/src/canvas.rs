//! Document windows (or tabs) and the canvas: rendering, tool input and selection overlays.

use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Vec2, pos2, vec2};
use firecraft_engine::{DocState, Tool};
use firecraft_raster::Mask;
use firecraft_raster::mask::Combine;

use crate::{FireCraftApp, View, theme};

pub fn workspace(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    // Forget views of closed documents.
    let ids: Vec<u64> = app.session.docs.iter().map(|d| d.id).collect();
    app.views.retain(|k, _| ids.contains(k));
    if app.session.docs.is_empty() {
        ui.centered_and_justified(|ui| {
            ui.label(
                egui::RichText::new(
                    "FireCraft\n\nFile > New (Ctrl+N)   |   File > Open (Ctrl+O)   |   or drop a PNG here",
                )
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

fn windows(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let area = ui.max_rect();
    let ctx = ui.ctx().clone();
    let mut close: Option<usize> = None;
    let count = app.session.docs.len();
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
        let view = app.views.entry(id).or_insert_with(|| View {
            open: true,
            ..View::default()
        });
        let mut open = view.open;
        let offset = 24.0 * (i % 8) as f32;
        let resp = egui::Window::new(title)
            .id(egui::Id::unique(("docwin", id)))
            .open(&mut open)
            .constrain_to(area)
            .default_pos(area.min + vec2(12.0 + offset, 12.0 + offset))
            .default_size(vec2(
                (w + 40.0).clamp(260.0, area.width() - 40.0),
                (h + 110.0).clamp(200.0, area.height() - 40.0),
            ))
            .collapsible(true)
            .resizable(true)
            .show(&ctx, |ui| document_view(app, ui, i));
        if let Some(r) = resp {
            if r.response.clicked()
                || r.response.dragged()
                || r.response.contains_pointer() && ctx.input(|inp| inp.pointer.any_pressed())
            {
                app.session.active = i;
            }
        }
        if !open {
            close = Some(i);
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

// One flat input-dispatch function per tool is easier to follow than splitting it; revisit when
// tools move into their own modules (roadmap: Pen/Subselection).
#[allow(clippy::cognitive_complexity)]
fn canvas(app: &mut FireCraftApp, ui: &mut egui::Ui, index: usize, rect: Rect, resp: &egui::Response) {
    let ctx = ui.ctx().clone();
    // Holding Space switches to the Hand tool temporarily, as in Fireworks.
    let space = !ui.ctx().egui_wants_keyboard_input() && ui.ctx().input(|i| i.key_down(egui::Key::Space));
    let tool = if space { Tool::Hand } else { app.session.tool };
    let options = app.session.options.clone();
    let Some(d) = app.session.docs.get_mut(index) else {
        return;
    };
    let view = app.views.entry(d.id).or_insert_with(|| View {
        open: true,
        ..View::default()
    });
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, Color32::from_gray(0xE0));

    // Fit All requested (zoom 0) or first show.
    if d.zoom <= 0.0 {
        let fit =
            (rect.width() / d.doc.width.max(1) as f32).min(rect.height() / d.doc.height.max(1) as f32) * 0.95;
        d.zoom = fit.clamp(1.0 / 16.0, 64.0);
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
    let z = d.zoom;
    let size = vec2(d.doc.width as f32, d.doc.height as f32) * z;
    let origin = (rect.center() - size / 2.0 + view.pan).round();
    let canvas_rect = Rect::from_min_size(origin, size);
    let to_doc =
        |pos: Pos2| -> (f64, f64) { (((pos.x - origin.x) / z) as f64, ((pos.y - origin.y) / z) as f64) };
    let to_screen = |x: f64, y: f64| -> Pos2 { pos2(origin.x + x as f32 * z, origin.y + y as f32 * z) };

    // Composite texture, refreshed when the document changes.
    // Re-render when the document or the shown frame changes.
    let key = (d.revision << 16) ^ d.doc.current_frame as u64;
    let stale = view.texture.as_ref().is_none_or(|(k, _)| *k != key);
    if stale {
        let img = d.doc.render(d.doc.current_frame);
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [img.width() as usize, img.height() as usize],
            &img.to_rgba_bytes(),
        );
        match &mut view.texture {
            Some((rev, tex)) => {
                tex.set(color, egui::TextureOptions::NEAREST);
                *rev = d.revision;
            }
            None => {
                view.texture = Some((
                    d.revision,
                    ctx.load_texture(format!("doc-{}", d.id), color, egui::TextureOptions::NEAREST),
                ))
            }
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
            | Tool::Slice
            | Tool::Hotspot
            | Tool::Crop
            | Tool::NineSlice => egui::CursorIcon::Crosshair,
            Tool::Text => egui::CursorIcon::Text,
            _ => egui::CursorIcon::Default,
        });
    }

    // The 9-slice tool handles its own input and overlay.
    if tool == Tool::NineSlice {
        crate::nine::canvas(&mut app.nine, d, view, &p, resp, origin, z);
        return;
    }
    // So do Scale, Skew and Distort.
    if matches!(tool, Tool::Scale | Tool::Skew | Tool::Distort) {
        crate::transform_tool::canvas(&mut app.transform, d, tool, &p, resp, origin, z);
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
    if resp.drag_started() {
        // The press position, not the current one (which is already past the drag threshold).
        view.drag_start = ctx
            .input(|i| i.pointer.press_origin())
            .or(resp.interact_pointer_pos());
        view.drag_secondary = resp.dragged_by(egui::PointerButton::Secondary);
        if matches!(tool, Tool::Pointer | Tool::SelectBehind) {
            if let Some(pos) = view.drag_start {
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
            }
        }
    }
    let drag_now = resp.interact_pointer_pos();
    let dragging = resp.dragged() && view.drag_start.is_some();
    if resp.drag_stopped() {
        if let (Some(a), Some(b)) = (view.drag_start, drag_now.or(hover)) {
            let (ax, ay) = to_doc(a);
            let (bx, by) = to_doc(b);
            match tool {
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
                // Drag a box with the Zoom tool: zoom so the box fills the view (Alt/right-drag: out).
                Tool::Zoom => {
                    let (w, h) = ((bx - ax).abs() as f32, (by - ay).abs() as f32);
                    if w >= 2.0 && h >= 2.0 {
                        let out = mods.alt || view.drag_secondary;
                        let fit = (rect.width() / w).min(rect.height() / h);
                        d.zoom = if out {
                            (d.zoom * d.zoom / fit).clamp(1.0 / 16.0, 64.0)
                        } else {
                            fit.clamp(1.0 / 16.0, 64.0)
                        };
                        center_on(d, view, ((ax + bx) / 2.0, (ay + by) / 2.0));
                    }
                }
                Tool::Hand => {}
                _ => {}
            }
        }
        view.drag_start = None;
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
    if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
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
    }

    // ---- Overlays ----------------------------------------------------------------------
    if !view.preview {
        // Object selection: light-blue bounds with corner handles.
        let drag_offset = match (tool, dragging, view.drag_start, drag_now) {
            (Tool::Pointer, true, Some(a), Some(b)) => b - a,
            _ => Vec2::ZERO,
        };
        for r in &d.selection {
            if let Some((x, y, w, h)) = d.element(*r).and_then(|e| e.bounds()) {
                let br = Rect::from_min_max(to_screen(x, y), to_screen(x + w, y + h)).translate(drag_offset);
                p.rect_stroke(br, 0.0, Stroke::new(1.0, theme::HANDLE_BLUE), StrokeKind::Outside);
                for c in [br.left_top(), br.right_top(), br.left_bottom(), br.right_bottom()] {
                    p.rect_filled(Rect::from_center_size(c, vec2(5.0, 5.0)), 0.0, theme::HANDLE_BLUE);
                }
            }
        }
        // Pixel selection: marching ants along the mask edge.
        if let Some(sel) = &d.pixels {
            if let Some(firecraft_doc::ElementKind::Bitmap(b)) = d.element(sel.target).map(|e| &e.kind) {
                let phase = (ctx.input(|i| i.time) * 8.0) as usize;
                marching_ants(&p, &sel.mask, b.x as f64, b.y as f64, &to_screen, phase);
                ctx.request_repaint_after(std::time::Duration::from_millis(120));
            }
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
                Tool::Line => {
                    p.line_segment([a, b], Stroke::new(1.0, theme::HANDLE_BLUE));
                }
                Tool::Zoom => {
                    p.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Middle);
                }
                _ => {}
            }
        }
    }
    // Pointer readout in the tooltip area is handy while developing; Fireworks shows it in Info.
    let _ = hover;
}

/// Pan so that document point `c` sits in the middle of the view.
fn center_on(d: &DocState, view: &mut View, c: (f64, f64)) {
    let size = vec2(d.doc.width as f32, d.doc.height as f32) * d.zoom;
    view.pan = size / 2.0 - vec2(c.0 as f32, c.1 as f32) * d.zoom;
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

fn checkerboard(p: &egui::Painter, r: Rect) {
    let s = 8.0;
    p.rect_filled(r, 0.0, Color32::WHITE);
    let mut y = r.top();
    let mut row = 0;
    while y < r.bottom() {
        let mut x = r.left() + if row % 2 == 0 { 0.0 } else { s };
        while x < r.right() {
            p.rect_filled(
                Rect::from_min_max(pos2(x, y), pos2((x + s).min(r.right()), (y + s).min(r.bottom()))),
                0.0,
                Color32::from_gray(204),
            );
            x += 2.0 * s;
        }
        y += s;
        row += 1;
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
