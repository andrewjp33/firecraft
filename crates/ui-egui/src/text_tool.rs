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
/// What the text is laid out with while it's edited: its own fonts, sizes and colours, or (the Text
/// Editor's Show Font / Show Size & Color unticked) plain ones. Edits change the real formatting.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct View {
    pub font: bool,
    pub size_color: bool,
}

impl Default for View {
    fn default() -> Self {
        View {
            font: true,
            size_color: true,
        }
    }
}

/// The layout the caret and selection follow.
fn layout(t: &firecraft_doc::Text, view: View) -> firecraft_text::TextLayout {
    if view == View::default() {
        engine::layout_of(t)
    } else {
        engine::layout_of(&engine::display_copy(t, view.font, view.size_color))
    }
}

#[derive(Default)]
pub struct TextTool {
    pub editing: Option<Editing>,
    /// How the text being edited is laid out (the Text Editor's display options).
    pub(crate) view: View,
    /// The Text Editor's text area: every press is in the text being edited.
    pub(crate) contained: bool,
    /// Where a press that may become a new text block started, and in which document.
    new_at: Option<(u64, f64, f64)>,
    /// The canvas taking the keyboard while text is edited.
    pub(crate) focus: Option<egui::Id>,
    /// Text > Size > Other...: the size being typed.
    pub(crate) other_size: Option<f64>,
}

impl TextTool {
    /// End editing, keeping the result.
    pub fn finish(&mut self, d: &mut DocState) {
        if let Some(e) = self.editing.take() {
            d.finish_text(e.r, e.is_new, e.changed);
        }
    }

    pub(crate) fn begin(&mut self, d: &mut DocState, r: ElementRef, caret: usize) {
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

/// The character position under canvas point (`x`, `y`).
fn caret_at(d: &DocState, r: ElementRef, (x, y): (f64, f64), view: View) -> usize {
    d.text_at(r).map_or(0, |t| {
        let (x, y) = engine::to_block(t, x, y);
        layout(t, view).hit_test(x, y).char_index
    })
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
fn keyboard(d: &mut DocState, e: &mut Editing, ctx: &egui::Context, view: View) -> bool {
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
                let layout = || d.text_at(e.r).map(|t| layout(t, view));
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
        let view = state.view;
        let inside = state.contained
            || state.editing.as_ref().is_some_and(|e| {
                d.text_at(e.r).is_some_and(|t| {
                    // Text on a path: anywhere on its characters.
                    if t.path.is_some() {
                        return d.hit_test(x, y) == Some(e.r);
                    }
                    let (x, y) = engine::to_block(t, x, y);
                    x >= t.rect.0 - 3.0 && x <= t.rect.2 + 3.0 && y >= t.rect.1 - 3.0 && y <= t.rect.3 + 3.0
                })
            });
        if inside {
            if let Some(e) = &mut state.editing {
                let shift = ctx.input(|i| i.modifiers.shift);
                let at = caret_at(d, e.r, (x, y), view);
                place(e, at, shift);
                e.selecting = true;
            }
        } else {
            state.finish(d);
            match d.hit_test(x, y).filter(|r| d.editable_text_at(*r).is_some()) {
                Some(r) => {
                    let at = caret_at(d, r, (x, y), view);
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
                e.caret = caret_at(d, e.r, (x, y), state.view);
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
            state.focus = Some(resp.id);
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
            done = focused == Some(resp.id) && keyboard(d, e, &ctx, state.view);
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
        // Drawn through the text's matrix: a scaled or rotated block shows as it is on the canvas.
        let at = |x: f64, y: f64| {
            let (x, y) = engine::from_block(t, x, y);
            to_screen(x, y)
        };
        let quad = |l: f64, top: f64, r: f64, b: f64| vec![at(l, top), at(r, top), at(r, b), at(l, b)];
        let (l, top, r, b) = t.rect;
        let outline = Stroke::new(1.0, Color32::from_rgb(0x33, 0x99, 0xff));
        match t.path.as_ref().map(|tp| &tp.path.kind) {
            // Text on a path: the path it runs along.
            Some(firecraft_doc::ElementKind::Path(p)) => {
                for c in &p.contours {
                    let pts: Vec<Pos2> = c.flatten(0.0, 0.0).iter().map(|q| at(q.0, q.1)).collect();
                    let shape = if c.closed {
                        egui::Shape::closed_line(pts, outline)
                    } else {
                        egui::Shape::line(pts, outline)
                    };
                    painter.add(shape);
                }
            }
            _ if !state.contained => {
                painter.add(egui::Shape::closed_line(
                    quad(l - 1.0, top - 1.0, r + 1.0, b + 1.0),
                    outline,
                ));
            }
            _ => {}
        }
        let layout = layout(t, state.view);
        let range = e.range();
        if !range.is_empty() {
            for s in layout.selection_rects(range.start, range.end) {
                let corners = quad(
                    f64::from(s.left),
                    f64::from(s.top),
                    f64::from(s.right),
                    f64::from(s.bottom),
                );
                painter.add(egui::Shape::convex_polygon(
                    corners,
                    Color32::from_rgba_unmultiplied(0x33, 0x99, 0xff, 90),
                    Stroke::NONE,
                ));
            }
        }
        let blink_on = (ctx.input(|i| i.time) * 1.6).fract() < 0.6;
        if blink_on && range.is_empty() {
            let (a, b) = match layout.caret(e.caret) {
                Some(c) => (at(c.top[0], c.top[1]), at(c.bottom[0], c.bottom[1])),
                // Empty text has no lines yet.
                None => (at(l, top), at(l, top + t.size)),
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
pub(crate) fn current_style(app: &FireCraftApp) -> (TextRun, u8, bool) {
    let o = &app.session.options;
    let tool_style = (o.text_style.clone(), o.text_anti_alias, o.text_auto_kern);
    let Some(d) = app.session.active_doc() else {
        return tool_style;
    };
    if let Some(e) = &app.text_tool.editing
        && let Some(t) = d.text_at(e.r)
    {
        return (editing_style(e, t), t.anti_alias, t.auto_kern);
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

/// Apply a formatting change to the text being edited: its selection, or what's typed next.
pub(crate) fn apply_editing(e: &mut Editing, d: &mut DocState, f: &dyn Fn(&mut TextRun)) {
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
}

/// The formatting the controls show for text being edited.
pub(crate) fn editing_style(e: &Editing, t: &firecraft_doc::Text) -> TextRun {
    let at = e.range().start + usize::from(!e.range().is_empty());
    let style = e.typing.clone().unwrap_or_else(|| engine::style_at(t, at));
    let font = engine::family_of(&style.font);
    TextRun { font, ..style }
}

/// Apply a formatting change to whatever the controls show.
pub(crate) fn apply(app: &mut FireCraftApp, label: &str, f: &dyn Fn(&mut TextRun)) {
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
        apply_editing(e, d, f);
        return;
    }
    if d.selection.iter().any(|r| d.editable_text_at(*r).is_some()) {
        d.format_selected_text(label, false, f);
    } else {
        f(&mut session.options.text_style);
    }
}

/// Apply a number field's change. Selected objects change as it's dragged, with one History state
/// on mouse up; text being edited records its changes when editing ends.
fn apply_value(app: &mut FireCraftApp, field: &egui::Response, label: &str, f: &dyn Fn(&mut TextRun)) {
    let editing = app.text_tool.editing.as_ref().map(|e| e.r);
    if let Some(d) = app.session.active_doc_mut()
        && !editing.is_some_and(|r| d.text_at(r).is_some())
        && d.selection.iter().any(|r| d.editable_text_at(*r).is_some())
    {
        crate::inspector::value_drag(d, field, label, |d, live| {
            d.format_selected_text(label, live, f);
        });
    } else if field.changed() {
        apply(app, label, f);
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
    mixed: bool,
) -> egui::Response {
    ui.label(label);
    let field = DragValue::new(value)
        .range(range)
        .max_decimals(0)
        .update_while_editing(false);
    // Blank when the selection has several values (Fireworks MX).
    let field = if mixed {
        field.custom_formatter(|_, _| String::new())
    } else {
        field.suffix(suffix)
    };
    ui.add(field)
}

/// Which of the controls' values differ across what a change would apply to: the characters
/// selected in the text being edited, or every run of the selected text objects. Fireworks MX
/// shows those controls blank.
#[derive(Clone, Copy, Default)]
pub(crate) struct Mixed {
    pub font: bool,
    pub size: bool,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub tracking: bool,
    pub leading: bool,
    pub h_scale: bool,
    pub shift: bool,
    pub align: bool,
}

pub(crate) fn mixed(app: &FireCraftApp) -> Mixed {
    let Some(d) = app.session.active_doc() else {
        return Mixed::default();
    };
    let mut runs: Vec<&TextRun> = Vec::new();
    if let Some(e) = &app.text_tool.editing
        && let Some(t) = d.text_at(e.r)
    {
        let range = e.range();
        let mut at = 0;
        for r in &t.runs {
            let n = r.text.chars().count();
            if at < range.end && at + n > range.start {
                runs.push(r);
            }
            at += n;
        }
    } else {
        for r in &d.selection {
            if let Some(t) = d.text_at(*r) {
                runs.extend(t.runs.iter().filter(|r| !r.text.is_empty()));
            }
        }
    }
    let Some(first) = runs.first() else {
        return Mixed::default();
    };
    let differs = |f: &dyn Fn(&TextRun) -> bool| runs.iter().any(|r| !f(r));
    let family = engine::family_of(&first.font);
    Mixed {
        font: differs(&|r| engine::family_of(&r.font) == family),
        size: differs(&|r| r.size == first.size),
        bold: differs(&|r| r.bold == first.bold),
        italic: differs(&|r| r.italic == first.italic),
        underline: differs(&|r| r.underline == first.underline),
        tracking: differs(&|r| r.tracking == first.tracking),
        leading: differs(&|r| r.leading == first.leading && r.leading_px == first.leading_px),
        h_scale: differs(&|r| r.h_scale == first.h_scale),
        shift: differs(&|r| r.baseline_shift == first.baseline_shift),
        align: differs(&|r| r.align == first.align),
    }
}

/// The text controls of the Property inspector.
pub fn inspector(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let (style, anti_alias, auto_kern) = current_style(app);
    let mix = mixed(app);
    app.platform.load_installed_font(&style.font);
    let fonts = app.font_names();
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            if let Some(name) = crate::font_picker::font_combo(
                ui,
                "text_font",
                170.0,
                400.0,
                if mix.font { "" } else { &style.font },
                &fonts,
            ) {
                app.platform.load_installed_font(&name);
                apply(app, "Text Font", &move |r| r.font.clone_from(&name));
            }
            let mut size = style.size;
            let mut size_field = DragValue::new(&mut size)
                .range(1.0..=1000.0)
                .max_decimals(1)
                .update_while_editing(false);
            if mix.size {
                size_field = size_field.custom_formatter(|_, _| String::new());
            }
            let field = ui.add(size_field).on_hover_text("Size");
            apply_value(app, &field, "Text Size", &move |r| r.size = size);
            // A selected text object's fill: a gradient, pattern or dither shows as a swatch that
            // opens its editor; plain colour is the characters' own.
            let object_fill = app
                .session
                .active_doc()
                .filter(|d| d.selection.iter().any(|r| d.text_at(*r).is_some()))
                .and_then(|d| d.selected_fill().map(|f| f.cloned()));
            let fancy = object_fill
                .clone()
                .flatten()
                .filter(|f| !crate::fill_ui::is_plain(f));
            match &fancy {
                Some(f) => {
                    let resp = crate::fill_ui::swatch(ui, vec2(22.0, 14.0), f);
                    if resp.clicked() {
                        crate::fill_ui::open(app, crate::gradient_ui::Target::Selection, f, &resp);
                    }
                }
                None => {
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
                }
            }
            if let Some(fill) = &object_fill {
                let label = fill
                    .as_ref()
                    .map_or("Solid", |f| firecraft_engine::fill_label(&f.category));
                if let Some(pick) = crate::fill_ui::fill_options(ui, "text_fill_kind", label, false) {
                    crate::fill_ui::choose(app, crate::gradient_ui::Target::Selection, pick);
                }
            }
            // Mixed: shown off, and a click turns it on for all.
            let bold = style.bold && !mix.bold;
            let italic = style.italic && !mix.italic;
            let underline = style.underline && !mix.underline;
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
                    .selectable_label(style.align == value && !mix.align, label)
                    .on_hover_text(tip)
                    .clicked()
                {
                    apply(app, "Text Align", &move |r| r.align = value);
                }
            }
        });
        ui.horizontal(|ui| {
            let mut tracking = style.tracking * 100.0;
            let field = number(ui, "Kerning:", &mut tracking, -100.0..=100.0, "%", mix.tracking);
            apply_value(app, &field, "Text Kerning", &move |r| {
                r.tracking = tracking / 100.0
            });
            let mut leading = if style.leading_px {
                style.leading
            } else {
                style.leading * 100.0
            };
            let suffix = if style.leading_px { " px" } else { "%" };
            let field = number(ui, "Leading:", &mut leading, 0.0..=1000.0, suffix, mix.leading);
            let px = style.leading_px;
            apply_value(app, &field, "Text Leading", &move |r| {
                r.leading = if px { leading } else { leading / 100.0 }
            });
            let mut scale = style.h_scale * 100.0;
            let field = number(ui, "Scale:", &mut scale, 1.0..=1000.0, "%", mix.h_scale);
            apply_value(app, &field, "Text Scale", &move |r| r.h_scale = scale / 100.0);
            let mut shift = style.baseline_shift;
            let field = number(ui, "Baseline:", &mut shift, -500.0..=500.0, " px", mix.shift);
            apply_value(app, &field, "Text Baseline", &move |r| r.baseline_shift = shift);
        });
        // Text on a path: how far along the path it starts (Fireworks MX's Text Offset).
        if let Some(d) = app.session.active_doc_mut()
            && let Some((_, offset)) = d.text_path_settings()
        {
            ui.horizontal(|ui| {
                ui.label("Text offset:");
                let mut v = offset;
                let field = ui.add(DragValue::new(&mut v).max_decimals(0).suffix(" px"));
                crate::inspector::value_drag(d, &field, "Text Offset", |d, live| {
                    d.edit_text_path("Text Offset", live, |tp| tp.offset = v);
                });
            });
        }
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
