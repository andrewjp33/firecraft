//! The Text tool: making text objects and editing them in place on the canvas, and the Property
//! inspector's text controls (docs/behavior/text.md).

use egui::{Color32, ComboBox, DragValue, Event, EventFilter, Key, Pos2, Rect, Stroke, pos2, vec2};
use firecraft_doc::TextRun;
use firecraft_engine::text::{self as engine, char_count};
use firecraft_engine::{DocState, ElementRef, Tool, ToolOptions};

use crate::FireCraftApp;
use crate::fpui::color_picker::{self as cp, ColorChoice, Footer};

/// A text object being edited.
pub struct Editing {
    /// The document it belongs to: several canvases share this tool.
    doc: u64,
    pub r: ElementRef,
    pub caret: usize,
    pub anchor: usize,
    is_new: bool,
    changed: bool,
    selecting: bool,
    /// Formatting chosen with nothing selected: what typing uses next.
    typing: Option<TextRun>,
}

impl Editing {
    fn range(&self) -> std::ops::Range<usize> {
        self.caret.min(self.anchor)..self.caret.max(self.anchor)
    }
}

/// The tool's state.
#[derive(Default)]
pub struct TextTool {
    pub editing: Option<Editing>,
    /// Where a press that may become a new text block started, and in which document.
    new_at: Option<(u64, f64, f64)>,
}

impl TextTool {
    /// End editing, keeping the result.
    pub fn finish(&mut self, d: &mut DocState) {
        if let Some(e) = self.editing.take() {
            d.finish_text(e.r, e.is_new, e.changed);
        }
    }

    fn begin(&mut self, d: &mut DocState, r: ElementRef, caret: usize) {
        d.begin_live_edit("Edit Text");
        d.selection = vec![r];
        self.editing = Some(Editing {
            doc: d.id,
            r,
            caret,
            anchor: caret,
            is_new: false,
            changed: false,
            selecting: false,
            typing: None,
        });
    }

    /// Edit the text object at `r`, all of it selected (a double-click with the Pointer).
    pub fn edit_all(&mut self, d: &mut DocState, r: ElementRef) {
        self.begin(d, r, 0);
        if let (Some(n), Some(e)) = (d.text_at(r).map(char_count), &mut self.editing) {
            e.caret = n;
        }
    }
}

fn caret_at(d: &DocState, r: ElementRef, x: f64, y: f64) -> usize {
    d.text_at(r)
        .map_or(0, |t| engine::layout_of(t).hit_test(x, y).char_index)
}

/// Replace the selection with `s`.
fn type_text(d: &mut DocState, e: &mut Editing, s: &str) {
    let range = e.range();
    let style = e.typing.take();
    let at = range.start;
    d.change_text(e.r, |t| {
        let inherited =
            style.unwrap_or_else(|| engine::style_at(t, if range.is_empty() { at } else { at + 1 }));
        engine::delete(t, range.clone());
        engine::insert(t, at, s, Some(&inherited));
    });
    e.caret = at + s.chars().count();
    e.anchor = e.caret;
    e.changed = true;
}

fn delete_range(d: &mut DocState, e: &mut Editing, range: std::ops::Range<usize>) {
    if range.is_empty() {
        return;
    }
    let at = range.start;
    d.change_text(e.r, |t| engine::delete(t, range));
    e.caret = at;
    e.anchor = at;
    e.changed = true;
}

fn selected_text(d: &DocState, e: &Editing) -> String {
    let range = e.range();
    d.text_at(e.r).map_or_else(String::new, |t| {
        t.text
            .chars()
            .skip(range.start)
            .take(range.len())
            .map(|c| if c == '\r' { '\n' } else { c })
            .collect()
    })
}

/// Move the caret; `extend` keeps the selection's other end.
fn place(e: &mut Editing, to: usize, extend: bool) {
    e.caret = to;
    if !extend {
        e.anchor = to;
    }
    e.typing = None;
}

/// Take keyboard input: whether editing should end.
fn keyboard(d: &mut DocState, e: &mut Editing, ctx: &egui::Context) -> bool {
    let events = ctx.input(|i| i.events.clone());
    let mut done = false;
    for ev in events {
        let len = d.text_at(e.r).map_or(0, char_count);
        match ev {
            Event::Text(s) if !s.chars().any(char::is_control) => type_text(d, e, &s),
            Event::Paste(s) => {
                let clean = s.replace("\r\n", "\r").replace('\n', "\r");
                type_text(d, e, &clean);
            }
            Event::Copy => ctx.copy_text(selected_text(d, e)),
            Event::Cut => {
                ctx.copy_text(selected_text(d, e));
                let range = e.range();
                delete_range(d, e, range);
            }
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => {
                let extend = modifiers.shift;
                let layout = || d.text_at(e.r).map(engine::layout_of);
                match key {
                    Key::Escape => done = true,
                    Key::Enter => type_text(d, e, "\r"),
                    Key::Tab => type_text(d, e, "\t"),
                    Key::A if modifiers.command => {
                        e.anchor = 0;
                        e.caret = len;
                    }
                    Key::Backspace => {
                        let range = e.range();
                        if range.is_empty() {
                            delete_range(d, e, e.caret.saturating_sub(1)..e.caret);
                        } else {
                            delete_range(d, e, range);
                        }
                    }
                    Key::Delete => {
                        let range = e.range();
                        if range.is_empty() {
                            delete_range(d, e, e.caret..(e.caret + 1).min(len));
                        } else {
                            delete_range(d, e, range);
                        }
                    }
                    Key::ArrowLeft => {
                        let range = e.range();
                        let to = if !extend && !range.is_empty() {
                            range.start
                        } else {
                            e.caret.saturating_sub(1)
                        };
                        place(e, to, extend);
                    }
                    Key::ArrowRight => {
                        let range = e.range();
                        let to = if !extend && !range.is_empty() {
                            range.end
                        } else {
                            (e.caret + 1).min(len)
                        };
                        place(e, to, extend);
                    }
                    Key::ArrowUp | Key::ArrowDown => {
                        let delta = if key == Key::ArrowUp { -1 } else { 1 };
                        if let Some(l) = layout() {
                            let to = l.move_vertical(e.caret, delta);
                            place(e, to, extend);
                        }
                    }
                    Key::Home | Key::End => {
                        if let Some(line) = layout().and_then(|l| l.line_range(e.caret)) {
                            let to = if key == Key::Home {
                                line.start
                            } else {
                                // Before the line's break, if it ends with one.
                                let text = d.text_at(e.r).map_or_else(String::new, |t| t.text.clone());
                                let last = line.end.saturating_sub(1);
                                if text.chars().nth(last) == Some('\r') {
                                    last
                                } else {
                                    line.end
                                }
                            };
                            place(e, to.min(len), extend);
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    done
}

/// End editing in a document that is no longer the active one, or has closed.
pub fn tidy(app: &mut FireCraftApp) {
    let Some(id) = app.text_tool.editing.as_ref().map(|e| e.doc) else {
        return;
    };
    let active = app.session.active_doc().map(|d| d.id);
    if active == Some(id) {
        return;
    }
    let FireCraftApp {
        session, text_tool, ..
    } = app;
    match session.docs.iter_mut().find(|d| d.id == id) {
        Some(d) => text_tool.finish(d),
        None => text_tool.editing = None,
    }
}

/// The tool on the canvas. `true`: it took the input (the rest of the canvas leaves it alone).
#[allow(clippy::too_many_arguments)]
pub fn canvas(
    state: &mut TextTool,
    d: &mut DocState,
    tool: Tool,
    options: &ToolOptions,
    resp: &egui::Response,
    painter: &egui::Painter,
    (origin, z): (Pos2, f32),
) -> bool {
    let ctx = resp.ctx.clone();
    // Every open document's canvas comes through here: only the one being edited may act on it.
    let busy_elsewhere = state
        .editing
        .as_ref()
        .map(|e| e.doc)
        .or(state.new_at.map(|n| n.0));
    if busy_elsewhere.is_some_and(|id| id != d.id) {
        return tool == Tool::Text;
    }
    if state.editing.is_some() && tool != Tool::Text {
        state.finish(d);
    }
    if tool != Tool::Text {
        return false;
    }
    // Undo can take the object away, or end the edit under way.
    if let Some(e) = &mut state.editing {
        match d.editable_text_at(e.r).map(char_count) {
            Some(n) => {
                e.caret = e.caret.min(n);
                e.anchor = e.anchor.min(n);
                if !d.in_live_edit() {
                    d.begin_live_edit("Edit Text");
                    e.is_new = false;
                    e.changed = false;
                }
            }
            None => state.editing = None,
        }
    }
    let to_doc = |p: Pos2| (((p.x - origin.x) / z) as f64, ((p.y - origin.y) / z) as f64);
    let to_screen = |x: f64, y: f64| pos2(origin.x + x as f32 * z, origin.y + y as f32 * z);
    // Presses and drags.
    let (pressed, down, press_at, pointer) = ctx.input(|i| {
        (
            i.pointer.primary_pressed(),
            i.pointer.primary_down(),
            i.pointer.press_origin(),
            i.pointer.interact_pos(),
        )
    });
    if pressed
        && resp.hovered()
        && let Some(p) = press_at.or(pointer)
    {
        let (x, y) = to_doc(p);
        let inside = state.editing.as_ref().is_some_and(|e| {
            d.text_at(e.r).is_some_and(|t| {
                x >= t.rect.0 - 3.0 && x <= t.rect.2 + 3.0 && y >= t.rect.1 - 3.0 && y <= t.rect.3 + 3.0
            })
        });
        if inside {
            if let Some(e) = &mut state.editing {
                let shift = ctx.input(|i| i.modifiers.shift);
                let at = caret_at(d, e.r, x, y);
                place(e, at, shift);
                e.selecting = true;
            }
        } else {
            state.finish(d);
            match d.hit_test(x, y).filter(|r| d.editable_text_at(*r).is_some()) {
                Some(r) => {
                    let at = caret_at(d, r, x, y);
                    state.begin(d, r, at);
                    if let Some(e) = &mut state.editing {
                        e.selecting = true;
                    }
                }
                None => state.new_at = Some((d.id, x, y)),
            }
        }
    }
    if let Some(e) = &mut state.editing
        && e.selecting
    {
        if down {
            if let Some(p) = pointer {
                let (x, y) = to_doc(p);
                e.caret = caret_at(d, e.r, x, y);
            }
        } else {
            e.selecting = false;
        }
    }
    // A press on empty canvas: a click starts a growing block, a drag sets the width.
    if let Some((_, x0, y0)) = state.new_at {
        let now = pointer.map_or((x0, y0), to_doc);
        if down {
            let (a, b) = (to_screen(x0, y0), to_screen(now.0, now.1));
            painter.rect_stroke(
                Rect::from_two_pos(a, b),
                0.0,
                Stroke::new(1.0, Color32::from_gray(90)),
                egui::StrokeKind::Inside,
            );
        } else {
            state.new_at = None;
            let size = options.text_style.size;
            let wide = (now.0 - x0).abs() >= 8.0;
            let (left, top, right) = if wide {
                (x0.min(now.0), y0.min(now.1), x0.max(now.0))
            } else {
                (x0, y0 - size / 2.0, x0 + 1.0)
            };
            let text = engine::new_text((left, top, right, top + size), !wide, options);
            let r = d.start_new_text(text);
            state.editing = Some(Editing {
                doc: d.id,
                r,
                caret: 0,
                anchor: 0,
                is_new: true,
                changed: false,
                selecting: false,
                typing: None,
            });
        }
    }
    // Typing, while no other control has the keyboard.
    let mut done = false;
    if let Some(e) = &mut state.editing {
        let focused = ctx.memory(egui::Memory::focused);
        if focused.is_none() || focused == Some(resp.id) {
            resp.request_focus();
            ctx.memory_mut(|m| {
                m.set_focus_lock_filter(
                    resp.id,
                    EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                );
            });
            done = focused == Some(resp.id) && keyboard(d, e, &ctx);
        }
    }
    if done {
        state.finish(d);
        resp.surrender_focus();
    }
    // The block, the selection and the caret.
    if let Some(e) = &state.editing
        && let Some(t) = d.text_at(e.r)
    {
        let (l, top, r, b) = t.rect;
        painter.rect_stroke(
            Rect::from_min_max(to_screen(l, top), to_screen(r, b)),
            0.0,
            Stroke::new(1.0, Color32::from_rgb(0x33, 0x99, 0xff)),
            egui::StrokeKind::Outside,
        );
        let layout = engine::layout_of(t);
        let range = e.range();
        if !range.is_empty() {
            for s in layout.selection_rects(range.start, range.end) {
                let rect = Rect::from_min_max(
                    to_screen(f64::from(s.left), f64::from(s.top)),
                    to_screen(f64::from(s.right), f64::from(s.bottom)),
                );
                painter.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(0x33, 0x99, 0xff, 90));
            }
        }
        let blink_on = (ctx.input(|i| i.time) * 1.6).fract() < 0.6;
        if blink_on && range.is_empty() {
            let (a, b) = match layout.caret(e.caret) {
                Some(c) => (to_screen(c.top[0], c.top[1]), to_screen(c.bottom[0], c.bottom[1])),
                // Empty text has no lines yet.
                None => (to_screen(l, top), to_screen(l, top + t.size)),
            };
            painter.line_segment([a, b], Stroke::new(1.0, Color32::BLACK));
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(120));
    }
    true
}

// ---- Property inspector ------------------------------------------------------------------------

/// What the controls show: the selection in the text being edited, a selected text object, or new
/// text's formatting.
fn current_style(app: &FireCraftApp) -> (TextRun, u8, bool) {
    let o = &app.session.options;
    let tool_style = (o.text_style.clone(), o.text_anti_alias, o.text_auto_kern);
    let Some(d) = app.session.active_doc() else {
        return tool_style;
    };
    if let Some(e) = &app.text_tool.editing
        && let Some(t) = d.text_at(e.r)
    {
        let at = e.range().start + usize::from(!e.range().is_empty());
        let style = e.typing.clone().unwrap_or_else(|| engine::style_at(t, at));
        let font = engine::family_of(&style.font);
        return (TextRun { font, ..style }, t.anti_alias, t.auto_kern);
    }
    d.selection
        .iter()
        .find_map(|r| d.text_at(*r))
        .map_or(tool_style, |t| {
            let style = engine::style_at(t, 0);
            let font = engine::family_of(&style.font);
            (TextRun { font, ..style }, t.anti_alias, t.auto_kern)
        })
}

/// Apply a formatting change to whatever the controls show.
fn apply(app: &mut FireCraftApp, label: &str, f: &dyn Fn(&mut TextRun)) {
    let FireCraftApp {
        session, text_tool, ..
    } = app;
    let editing = text_tool.editing.as_mut();
    let Some(d) = session.active_doc_mut() else {
        f(&mut session.options.text_style);
        return;
    };
    if let Some(e) = editing
        && d.text_at(e.r).is_some()
    {
        let range = e.range();
        if range.is_empty() {
            let empty = d.text_at(e.r).is_some_and(|t| t.text.is_empty());
            if empty {
                d.change_text(e.r, |t| t.runs.iter_mut().for_each(f));
                e.changed = true;
            } else {
                let mut style = e
                    .typing
                    .clone()
                    .or_else(|| d.text_at(e.r).map(|t| engine::style_at(t, e.caret)))
                    .unwrap_or_else(engine::default_style);
                f(&mut style);
                e.typing = Some(style);
            }
        } else {
            d.change_text(e.r, |t| engine::format(t, range, f));
            e.changed = true;
        }
        return;
    }
    if d.selection.iter().any(|r| d.editable_text_at(*r).is_some()) {
        d.format_selected_text(label, false, f);
    } else {
        f(&mut session.options.text_style);
    }
}

fn set_object(app: &mut FireCraftApp, label: &str, f: &dyn Fn(&mut firecraft_doc::Text)) {
    let FireCraftApp { session, .. } = app;
    let Some(d) = session.active_doc_mut() else {
        return;
    };
    if d.selection.iter().any(|r| d.editable_text_at(*r).is_some()) {
        d.change_selected_text_objects(label, f);
    }
}

fn number(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut f64,
    range: std::ops::RangeInclusive<f64>,
    suffix: &str,
) -> bool {
    ui.label(label);
    ui.add(DragValue::new(value).range(range).max_decimals(0).suffix(suffix))
        .changed()
}

/// The text controls of the Property inspector.
pub fn inspector(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let (style, anti_alias, auto_kern) = current_style(app);
    let fonts = app.font_names();
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            let mut font_pick = None;
            ComboBox::from_id_salt("text_font")
                .width(170.0)
                .height(400.0)
                .selected_text(style.font.as_str())
                .show_ui(ui, |ui| {
                    for name in fonts.iter() {
                        if ui.selectable_label(*name == style.font, name).clicked() {
                            font_pick = Some(name.clone());
                        }
                    }
                });
            if let Some(name) = font_pick {
                apply(app, "Text Font", &move |r| r.font.clone_from(&name));
            }
            let mut size = style.size;
            if ui
                .add(DragValue::new(&mut size).range(1.0..=1000.0).max_decimals(1))
                .on_hover_text("Size")
                .changed()
            {
                apply(app, "Text Size", &move |r| r.size = size);
            }
            let color = cp::color_box(
                ui,
                "text_color",
                vec2(22.0, 14.0),
                Some(style.color),
                Footer::Plain,
            );
            if let Some(ColorChoice::Color(c)) = color {
                let c = cp::to_rgba(c);
                apply(app, "Text Color", &move |r| r.color = c);
            }
            let (bold, italic, underline) = (style.bold, style.italic, style.underline);
            if ui.selectable_label(bold, "B").on_hover_text("Bold").clicked() {
                apply(app, "Text Style", &move |r| r.bold = !bold);
            }
            if ui.selectable_label(italic, "I").on_hover_text("Italic").clicked() {
                apply(app, "Text Style", &move |r| r.italic = !italic);
            }
            if ui
                .selectable_label(underline, "U")
                .on_hover_text("Underline")
                .clicked()
            {
                apply(app, "Text Style", &move |r| r.underline = !underline);
            }
        });
        ui.horizontal(|ui| {
            for (value, label, tip) in [
                (0u8, "Left", "Align left"),
                (1, "Center", "Center"),
                (2, "Right", "Align right"),
                (3, "Justify", "Justify"),
                (4, "Stretch", "Stretch"),
            ] {
                if ui
                    .selectable_label(style.align == value, label)
                    .on_hover_text(tip)
                    .clicked()
                {
                    apply(app, "Text Align", &move |r| r.align = value);
                }
            }
        });
        ui.horizontal(|ui| {
            let mut tracking = style.tracking * 100.0;
            if number(ui, "Kerning:", &mut tracking, -100.0..=100.0, "%") {
                apply(app, "Text Kerning", &move |r| r.tracking = tracking / 100.0);
            }
            let mut leading = if style.leading_px {
                style.leading
            } else {
                style.leading * 100.0
            };
            let suffix = if style.leading_px { " px" } else { "%" };
            if number(ui, "Leading:", &mut leading, 0.0..=1000.0, suffix) {
                let px = style.leading_px;
                apply(app, "Text Leading", &move |r| {
                    r.leading = if px { leading } else { leading / 100.0 }
                });
            }
            let mut scale = style.h_scale * 100.0;
            if number(ui, "Scale:", &mut scale, 1.0..=1000.0, "%") {
                apply(app, "Text Scale", &move |r| r.h_scale = scale / 100.0);
            }
            let mut shift = style.baseline_shift;
            if number(ui, "Baseline:", &mut shift, -500.0..=500.0, " px") {
                apply(app, "Text Baseline", &move |r| r.baseline_shift = shift);
            }
        });
        ui.horizontal(|ui| {
            const MODES: [(u8, &str); 3] = [
                (0, "Smooth Anti-Alias"),
                (1, "Crisp Anti-Alias"),
                (2, "Strong Anti-Alias"),
            ];
            let mut pick = None;
            let name = MODES
                .iter()
                .find(|(v, _)| *v == anti_alias)
                .map_or(MODES[0].1, |(_, n)| n);
            ComboBox::from_id_salt("text_aa")
                .width(130.0)
                .selected_text(name)
                .show_ui(ui, |ui| {
                    for (v, n) in MODES {
                        if ui.selectable_label(v == anti_alias, n).clicked() {
                            pick = Some(v);
                        }
                    }
                });
            if let Some(v) = pick {
                app.session.options.text_anti_alias = v;
                set_object(app, "Text Anti-Alias", &move |t| t.anti_alias = v);
            }
            let mut auto = auto_kern;
            if ui.checkbox(&mut auto, "Auto Kern").changed() {
                app.session.options.text_auto_kern = auto;
                set_object(app, "Auto Kern", &move |t| t.auto_kern = auto);
            }
        });
    });
}
