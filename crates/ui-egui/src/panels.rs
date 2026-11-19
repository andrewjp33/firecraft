//! The right-hand panel dock: Optimize, Layers, Frames and History, Assets, Answers.

use egui::{Color32, Rect, RichText, Sense, Stroke, StrokeKind, vec2};
use firecraft_doc::{Element, ElementKind, MaskMode};
use firecraft_engine::{DocState, ElementRef};

use crate::icons::{self, Glyph};
use crate::{FireCraftApp, theme};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    Optimize,
    Layers,
    Frames,
    Assets,
    Answers,
}

fn group_open(ui: &egui::Ui, g: Group, default: bool) -> bool {
    ui.ctx()
        .data(|d| d.get_temp(egui::Id::unique(("dock_open", g as u8))))
        .unwrap_or(default)
}

fn set_group_open(ui: &egui::Ui, g: Group, v: bool) {
    ui.ctx()
        .data_mut(|d| d.insert_temp(egui::Id::unique(("dock_open", g as u8)), v));
}

fn group(ui: &mut egui::Ui, g: Group, title: &str, default: bool, body: impl FnOnce(&mut egui::Ui)) {
    let mut open = group_open(ui, g, default);
    theme::group_header(ui, title, &mut open);
    set_group_open(ui, g, open);
    if open {
        egui::Frame::new()
            .fill(theme::PANEL)
            .inner_margin(egui::Margin::same(3))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                body(ui);
            });
    }
    ui.add_space(2.0);
}

pub fn dock(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    // The panels are laid out in a child area that never reports more than the dock's width:
    // egui keeps a resizable panel as wide as its contents, and at fractional scales (UI Scale
    // 140-150%) a full-width row rounds up by part of a pixel, so the dock grew a little on
    // every repaint. Anything wider is clipped instead.
    let rect = ui.available_rect_before_wrap();
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(*ui.layout()));
    child.shrink_clip_rect(rect);
    dock_contents(app, &mut child);
    ui.allocate_rect(rect, egui::Sense::hover());
}

fn dock_contents(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            // Fireworks MX groups the Align panel with Optimize as a second tab.
            group(ui, Group::Optimize, "Optimize and Align", false, |ui| optimize_align(app, ui));
            group(ui, Group::Layers, "Layers", true, |ui| layers(app, ui));
            group(ui, Group::Frames, "Frames and History", true, |ui| {
                frames_history(app, ui)
            });
            group(ui, Group::Assets, "Assets (Library)", false, |ui| {
                crate::library::panel(app, ui)
            });
            group(ui, Group::Answers, "Answers", false, |ui| {
                ui.weak("FireCraft documentation: docs/ in the source tree.");
            });
        });
}

fn thumbnail(ui: &mut egui::Ui, kind: &ElementKind) {
    let (r, _) = ui.allocate_exact_size(vec2(26.0, 20.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, 0.0, Color32::WHITE);
    p.rect_stroke(
        r,
        0.0,
        Stroke::new(1.0, Color32::from_gray(170)),
        StrokeKind::Inside,
    );
    let inner = r.shrink(4.0);
    match kind {
        ElementKind::Bitmap(_) => {
            p.rect_filled(
                Rect::from_min_size(inner.min, inner.size() / 2.0),
                0.0,
                Color32::from_rgb(230, 120, 60),
            );
            p.rect_filled(
                Rect::from_min_size(inner.center(), inner.size() / 2.0),
                0.0,
                Color32::from_rgb(60, 130, 220),
            );
        }
        ElementKind::Path(path) => {
            let c = path
                .fill
                .as_ref()
                .map(|f| f.color)
                .or(path.stroke.as_ref().map(|s| s.color));
            let c = c
                .map(|c| Color32::from_rgb(c.r, c.g, c.b))
                .unwrap_or(Color32::GRAY);
            p.rect_stroke(inner, 0.0, Stroke::new(1.5, c), StrokeKind::Inside);
        }
        ElementKind::Text(_) => {
            p.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "T",
                egui::FontId::proportional(12.0),
                Color32::BLACK,
            );
        }
        _ => {
            p.rect_filled(inner, 0.0, Color32::from_rgb(120, 220, 120));
        }
    }
}

/// Layers panel state between frames (kept in egui's temporary data).
#[derive(Clone, Default)]
struct LayersUi {
    /// Renaming a layer (object `None`) or an object, and the name being typed.
    renaming: Option<Rename>,
    /// Masked objects and groups shown open, listing the objects inside them.
    open: std::collections::HashSet<ElementRef>,
    /// A row being dragged.
    dragging: Option<Dragged>,
}

/// A layer (object `None`) or an object being renamed, and its name.
type Rename = (usize, Option<ElementRef>, String);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dragged {
    Layer(usize),
    /// The selected objects.
    Objects,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Row {
    Layer(usize),
    Object(usize, usize),
}

/// Where a drag would drop: objects into (layer, index), or a layer to a slot (bottom first),
/// and the y of the insertion line.
enum Drop {
    Objects(usize, usize, f32),
    Layer(usize, f32),
}

fn drop_target(
    d: &firecraft_engine::DocState,
    rows: &[(Rect, Row)],
    dragged: Dragged,
    y: f32,
) -> Option<Drop> {
    let (rect, row) = rows
        .iter()
        .find(|(r, _)| y >= r.top() && y < r.bottom())
        .copied()?;
    let upper = y < rect.center().y;
    match dragged {
        Dragged::Objects => match row {
            Row::Object(li, ei) => Some(Drop::Objects(
                li,
                if upper { ei + 1 } else { ei },
                if upper { rect.top() } else { rect.bottom() },
            )),
            Row::Layer(li) => {
                let n = d.doc.layers.get(li)?.elements(d.doc.current_frame).len();
                Some(Drop::Objects(li, n, rect.bottom()))
            }
        },
        Dragged::Layer(_) => {
            // The layer's whole block (its row and its objects' rows).
            let li = match row {
                Row::Layer(li) | Row::Object(li, _) => li,
            };
            let block = rows
                .iter()
                .filter(|(_, r)| matches!(r, Row::Layer(l) | Row::Object(l, _) if *l == li))
                .map(|(r, _)| *r)
                .reduce(|a, b| a.union(b))?;
            let above = y < block.center().y;
            Some(Drop::Layer(
                if above { li + 1 } else { li },
                if above { block.top() } else { block.bottom() },
            ))
        }
    }
}

/// A name being edited: Enter or clicking elsewhere keeps it, Escape cancels.
fn name_edit(ui: &mut egui::Ui, st: &mut LayersUi, rename: &mut Option<Rename>) {
    let Some((li, obj, text)) = st.renaming.as_mut() else {
        return;
    };
    let r = ui.add(egui::TextEdit::singleline(text).desired_width(110.0));
    if !r.has_focus() && !r.lost_focus() {
        r.request_focus();
    }
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        st.renaming = None;
    } else if r.lost_focus() {
        *rename = Some((*li, *obj, text.clone()));
        st.renaming = None;
    }
}

/// The tint of a row whose objects inside hold the selection (FireCraft; FirePixel's group
/// tree marks such rows the same way).
const HOLDS_SELECTION: Color32 = Color32::from_rgb(0xF5, 0xEB, 0xB5);

/// The frame of a selected mask's thumbnail (Fireworks CS).
const MASK_EDIT_GREEN: Color32 = Color32::from_rgb(0x66, 0xFF, 0x00);

/// Width of an object row's open/close arrow, and the indent per level inside groups and masks.
const ARROW_W: f32 = 12.0;
const LEVEL_INDENT: f32 = 12.0;

/// What the object rows asked for, applied once the list is drawn.
#[derive(Default)]
struct RowActions {
    select: Option<ElementRef>,
    toggle_vis: Option<ElementRef>,
    toggle_lock: Option<ElementRef>,
    toggle_open: Option<ElementRef>,
    /// A drag started on this top-level object's row.
    drag: Option<ElementRef>,
    rename: Option<Rename>,
}

/// Whether the object opens in the Layers panel to list what is inside it: a group's members,
/// a masked object's mask and content (FireCraft; Fireworks lists only top-level objects).
fn expandable(el: &Element) -> bool {
    match &el.kind {
        ElementKind::Group(c) => !c.is_empty(),
        ElementKind::Masked { .. } => true,
        _ => false,
    }
}

/// An object row's name, as Fireworks CS shows it: a masked object is named after its content
/// (the mask thumbnail beside it shows it is masked), and an unnamed group, or a mask over
/// several objects, by its number of objects.
fn row_label(el: &Element) -> String {
    let group = |n: usize| format!("Group: {n} objects");
    match (&el.name, &el.kind) {
        (None, ElementKind::Masked { content, .. }) => match content.as_slice() {
            [one] => one.label(),
            many => group(many.len()),
        },
        (None, ElementKind::Group(c)) => group(c.len()),
        _ => el.label(),
    }
}

/// Whether `inner` is an object somewhere inside `outer`.
fn is_inside(inner: ElementRef, outer: ElementRef) -> bool {
    let mut p = inner.parent();
    while let Some(x) = p {
        if x == outer {
            return true;
        }
        p = x.parent();
    }
    false
}

/// The kind of mask, as Fireworks CS's Property inspector names it.
fn mask_kind(mask: &Element, mode: MaskMode) -> &'static str {
    match (&mask.kind, mode) {
        (ElementKind::Bitmap(_), _) => "Bitmap mask",
        (_, MaskMode::PathOutline) => "Vector mask (path outline)",
        _ => "Vector mask",
    }
}

/// A mask's thumbnail, in the style of Fireworks CS's: black where the mask hides the content,
/// light where it shows it.
///
/// `editing`: the mask is selected; Fireworks CS frames it green then (2 px #66FF00, growing
/// outward; measured by FirePixel, its RE spec 73).
fn mask_thumbnail(ui: &mut egui::Ui, mask: &Element, mode: MaskMode, editing: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 20.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, 0.0, Color32::BLACK);
    if editing {
        p.rect_stroke(r, 0.0, Stroke::new(2.0, MASK_EDIT_GREEN), StrokeKind::Outside);
    } else {
        p.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::from_gray(170)), StrokeKind::Inside);
    }
    let inner = r.shrink(4.0);
    match (&mask.kind, mode) {
        (ElementKind::Bitmap(_), _) => {
            // A grey ramp: bitmap masks reveal by their brightness.
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(inner.left_top(), Color32::from_gray(40));
            mesh.colored_vertex(inner.right_top(), Color32::WHITE);
            mesh.colored_vertex(inner.right_bottom(), Color32::WHITE);
            mesh.colored_vertex(inner.left_bottom(), Color32::from_gray(40));
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            p.add(egui::Shape::mesh(mesh));
        }
        (ElementKind::Text(_), _) => {
            p.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "T",
                egui::FontId::proportional(12.0),
                Color32::WHITE,
            );
        }
        (_, MaskMode::PathOutline) => {
            p.add(egui::epaint::EllipseShape::stroke(
                inner.center(),
                inner.size() / 2.0,
                Stroke::new(1.5, Color32::WHITE),
            ));
        }
        _ => {
            p.add(egui::epaint::EllipseShape::filled(
                inner.center(),
                inner.size() / 2.0,
                Color32::WHITE,
            ));
        }
    }
    resp
}

/// The object's eye and lock (Fireworks CS), in the layer's eye and lock columns; returns the
/// columns' rectangle.
fn eye_and_lock(ui: &mut egui::Ui, r: ElementRef, el: &Element, edited: bool, out: &mut RowActions) -> Rect {
    ui.horizontal(|ui| {
        ui.add_space(14.0 + ui.spacing().item_spacing.x);
        let eye = icons::glyph_button(ui, Glyph::Eye, 16.0, true);
        if !el.visible {
            ui.painter().rect_filled(eye.rect.shrink(1.0), 0.0, theme::PANEL);
        }
        if eye.on_hover_text("Show/Hide Object").clicked() {
            out.toggle_vis = Some(r);
        }
        let lock = icons::glyph_button(ui, Glyph::Lock, 14.0, true);
        if !el.locked {
            ui.painter().rect_filled(lock.rect.shrink(1.0), 0.0, theme::PANEL);
            // The bitmap the pixel selection applies to shows a pencil in this column, as in
            // Fireworks MX.
            if edited {
                icons::paint_glyph(ui.painter(), lock.rect.shrink(2.0), Glyph::Pencil, Color32::from_gray(30));
            }
        }
        if lock.on_hover_text("Lock/Unlock Object").clicked() {
            out.toggle_lock = Some(r);
        }
    })
    .response
    .rect
}

/// An object's row and, when it is open, the rows of what is inside it: a masked object's
/// mask, then its content; a group's members (top first). `depth` counts the levels inside
/// top-level objects; `locked_above` is set inside a locked object, whose members can't be
/// selected either.
#[allow(clippy::too_many_arguments)]
fn object_rows(
    ui: &mut egui::Ui,
    st: &mut LayersUi,
    d: &DocState,
    r: ElementRef,
    el: &Element,
    depth: usize,
    locked_above: bool,
    out: &mut RowActions,
    rows: &mut Vec<(Rect, Row)>,
) {
    // A selected mask keeps its object's row highlighted (Fireworks CS); the mask thumbnail
    // shows it is the mask.
    let mask_ref = matches!(el.kind, ElementKind::Masked { .. }).then(|| r.mask()).flatten();
    let mask_selected = mask_ref.is_some_and(|m| d.selection.contains(&m));
    let selected = d.selection.contains(&r) || mask_selected;
    let holds = !selected && d.selection.iter().any(|s| is_inside(*s, r));
    let edited = d.pixels.as_ref().and_then(|p| p.target) == Some(r);
    let editing = st
        .renaming
        .as_ref()
        .is_some_and(|(l, o, _)| *l == r.layer && *o == Some(r));
    let open = expandable(el) && st.open.contains(&r);
    // The whole row (thumbnail, name and the space after it) selects the object.
    let background = ui.painter().add(egui::Shape::Noop);
    let mut buttons = Rect::NOTHING;
    let mut arrow = Rect::NOTHING;
    let mut mask_thumb = Rect::NOTHING;
    let row = ui.horizontal(|ui| {
        buttons = eye_and_lock(ui, r, el, edited, out);
        ui.add_space(depth as f32 * LEVEL_INDENT);
        let (a, resp) = ui.allocate_exact_size(vec2(ARROW_W, 20.0), Sense::click());
        arrow = a;
        if expandable(el) {
            let g = if open { Glyph::TriangleDown } else { Glyph::TriangleRight };
            let c = if resp.hovered() { theme::SELECT_BLUE } else { Color32::from_gray(90) };
            icons::paint_glyph(ui.painter(), a.shrink2(vec2(0.0, 4.0)), g, c);
            let name = format!("{} {}", if open { "Close" } else { "Open" }, row_label(el));
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
            let tip = if matches!(el.kind, ElementKind::Masked { .. }) {
                "Show or hide the mask and the objects it masks"
            } else {
                "Show or hide the objects in this group"
            };
            if resp.on_hover_text(tip).clicked() {
                out.toggle_open = Some(r);
            }
        }
        match &el.kind {
            ElementKind::Masked { content, mask, mode, .. } => {
                match content.as_slice() {
                    [one] => thumbnail(ui, &one.kind),
                    _ => thumbnail(ui, &ElementKind::Group(Vec::new())),
                }
                let (link, _) = ui.allocate_exact_size(vec2(8.0, 20.0), Sense::hover());
                icons::paint_glyph(ui.painter(), link.shrink2(vec2(0.0, 3.0)), Glyph::Link, Color32::from_gray(60));
                mask_thumb = mask_thumbnail(ui, mask, *mode, mask_selected)
                    .on_hover_text(format!("{}: click to select the mask", mask_kind(mask, *mode)))
                    .rect;
            }
            kind => thumbnail(ui, kind),
        }
        if editing {
            name_edit(ui, st, &mut out.rename);
            return;
        }
        let label = row_label(el);
        let text = if selected {
            RichText::new(label).color(Color32::WHITE)
        } else {
            RichText::new(label)
        };
        ui.add(egui::Label::new(text).selectable(false));
        ui.add_space(ui.available_width());
    });
    let rect = row.response.rect;
    if depth == 0 {
        rows.push((rect, Row::Object(r.layer, r.index)));
    }
    let band = Rect::from_min_max(egui::pos2(buttons.right(), rect.top()), rect.max);
    if selected || holds {
        let fill = if selected { theme::SELECT_BLUE } else { HOLDS_SELECTION };
        ui.painter().set(background, egui::Shape::rect_filled(band, 0.0, fill));
    }
    if !editing {
        let hit = Rect::from_min_max(egui::pos2(arrow.right(), rect.top()), rect.max);
        let hint = if depth == 0 {
            "Double-click to rename; drag to move (also to another layer)"
        } else {
            "Click to select this object inside its group or mask; double-click to rename"
        };
        let click = ui
            .interact(hit, ui.scope_id().with(("layer_row", r)), Sense::click_and_drag())
            .on_hover_text(hint);
        // Locked objects can't be selected (Fireworks CS).
        let selectable = !el.locked && !locked_above;
        if click.clicked() && selectable {
            // The mask thumbnail selects the mask, the rest of the row the object.
            let on_mask = click.interact_pointer_pos().is_some_and(|p| mask_thumb.contains(p));
            out.select = if on_mask { mask_ref.or(Some(r)) } else { Some(r) };
        }
        if click.double_clicked() {
            st.renaming = Some((r.layer, Some(r), el.name.clone().unwrap_or_default()));
        }
        // Objects inside groups and masks stay where they are: only top-level rows drag.
        if click.drag_started() && selectable && depth == 0 {
            out.drag = Some(r);
        }
    }
    if !open {
        return;
    }
    let locked = locked_above || el.locked;
    match &el.kind {
        ElementKind::Masked { content, mask, mode, .. } => {
            if let Some(m) = mask_ref {
                mask_row(ui, st, d, m, mask, *mode, depth + 1, locked, out, rows);
            }
            for (i, c) in content.iter().enumerate().rev() {
                if let Some(cr) = r.child(i) {
                    object_rows(ui, st, d, cr, c, depth + 1, locked, out, rows);
                }
            }
        }
        ElementKind::Group(children) => {
            for (i, c) in children.iter().enumerate().rev() {
                if let Some(cr) = r.child(i) {
                    object_rows(ui, st, d, cr, c, depth + 1, locked, out, rows);
                }
            }
        }
        _ => {}
    }
}

/// The row of a masked object's mask (`r`), above its content: a click selects the mask, as the
/// mask thumbnail does. A mask that is a group or itself masked opens like other rows, and the
/// objects in it can be selected.
#[allow(clippy::too_many_arguments)]
fn mask_row(
    ui: &mut egui::Ui,
    st: &mut LayersUi,
    d: &DocState,
    r: ElementRef,
    mask: &Element,
    mode: MaskMode,
    depth: usize,
    locked: bool,
    out: &mut RowActions,
    rows: &mut Vec<(Rect, Row)>,
) {
    let selected = d.selection.contains(&r);
    let holds = !selected && d.selection.iter().any(|s| is_inside(*s, r));
    let open = expandable(mask) && st.open.contains(&r);
    let background = ui.painter().add(egui::Shape::Noop);
    let mut buttons = Rect::NOTHING;
    let mut arrow = Rect::NOTHING;
    let row = ui.horizontal(|ui| {
        // Empty eye and lock columns: the mask shows and hides with its object.
        buttons = ui
            .horizontal(|ui| {
                ui.add_space(14.0 + ui.spacing().item_spacing.x);
                ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
            })
            .response
            .rect;
        ui.add_space(depth as f32 * LEVEL_INDENT);
        let (a, resp) = ui.allocate_exact_size(vec2(ARROW_W, 20.0), Sense::click());
        arrow = a;
        if expandable(mask) {
            let g = if open { Glyph::TriangleDown } else { Glyph::TriangleRight };
            let c = if resp.hovered() { theme::SELECT_BLUE } else { Color32::from_gray(90) };
            icons::paint_glyph(ui.painter(), a.shrink2(vec2(0.0, 4.0)), g, c);
            let name = format!("{} mask objects", if open { "Close" } else { "Open" });
            resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &name));
            if resp.on_hover_text("Show or hide the objects the mask is made of").clicked() {
                out.toggle_open = Some(r);
            }
        }
        mask_thumbnail(ui, mask, mode, selected);
        let text = RichText::new(mask_kind(mask, mode)).italics();
        let text = if selected { text.color(Color32::WHITE) } else { text };
        ui.add(egui::Label::new(text).selectable(false));
        ui.add_space(ui.available_width());
    });
    let rect = row.response.rect;
    let band = Rect::from_min_max(egui::pos2(buttons.right(), rect.top()), rect.max);
    if selected || holds {
        let fill = if selected { theme::SELECT_BLUE } else { HOLDS_SELECTION };
        ui.painter().set(background, egui::Shape::rect_filled(band, 0.0, fill));
    }
    let mode_text = match mode {
        MaskMode::Grayscale => "shows the content by its grayscale",
        MaskMode::Alpha => "shows the content by its alpha channel",
        MaskMode::PathOutline => "shows the content inside its path outline",
    };
    let hit = Rect::from_min_max(egui::pos2(arrow.right(), rect.top()), rect.max);
    let click = ui
        .interact(hit, ui.scope_id().with(("mask_row", r)), Sense::click())
        .on_hover_text(format!(
            "The mask ({}): it {mode_text}. Click to select it.",
            mask.label()
        ));
    if click.clicked() && !locked {
        out.select = Some(r);
    }
    if !open {
        return;
    }
    let list = match &mask.kind {
        ElementKind::Group(children) => children,
        ElementKind::Masked { content, .. } => content,
        _ => return,
    };
    if let (ElementKind::Masked { mask: inner, mode: inner_mode, .. }, Some(m)) = (&mask.kind, r.mask()) {
        mask_row(ui, st, d, m, inner, *inner_mode, depth + 1, locked, out, rows);
    }
    for (i, c) in list.iter().enumerate().rev() {
        if let Some(cr) = r.child(i) {
            object_rows(ui, st, d, cr, c, depth + 1, locked, out, rows);
        }
    }
}

fn layers(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let Some(d) = app.session.active_doc_mut() else {
        ui.weak("No document");
        return;
    };
    let state_id = egui::Id::unique(("layers_ui", d.id));
    let mut st: LayersUi = ui.ctx().data(|m| m.get_temp(state_id)).unwrap_or_default();
    let frame = d.doc.current_frame;
    // Rows left open by objects that are gone (or moved) close.
    st.open.retain(|r| d.element(*r).is_some_and(expandable));
    let mut set_current: Option<usize> = None;
    let mut toggle_vis: Option<usize> = None;
    let mut toggle_lock: Option<usize> = None;
    let mut toggle_disc: Option<usize> = None;
    let mut rename: Option<Rename> = None;
    let mut actions = RowActions::default();
    let mut rows: Vec<(Rect, Row)> = Vec::new();
    // Top layer first, as Fireworks lists them.
    for li in (0..d.doc.layers.len()).rev() {
        let Some(layer) = d.doc.layers.get(li) else {
            continue;
        };
        let current = d.doc.current_layer == li;
        let header = ui.horizontal(|ui| {
            let glyph = if layer.disclosed {
                Glyph::Minus
            } else {
                Glyph::Plus
            };
            if icons::glyph_button(ui, glyph, 14.0, true).clicked() {
                toggle_disc = Some(li);
            }
            let eye = icons::glyph_button(ui, Glyph::Eye, 16.0, true);
            if !layer.visible {
                ui.painter().rect_filled(eye.rect.shrink(1.0), 0.0, theme::PANEL);
            }
            if eye.on_hover_text("Show/Hide Layer").clicked() {
                toggle_vis = Some(li);
            }
            let lock = icons::glyph_button(ui, Glyph::Lock, 14.0, true);
            if !layer.locked {
                ui.painter().rect_filled(lock.rect.shrink(1.0), 0.0, theme::PANEL);
            }
            if lock.on_hover_text("Lock/Unlock Layer").clicked() {
                toggle_lock = Some(li);
            }
            icons::glyph(ui, if layer.web { Glyph::Globe } else { Glyph::Folder }, 15.0);
            if st
                .renaming
                .as_ref()
                .is_some_and(|(l, o, _)| *l == li && o.is_none())
            {
                name_edit(ui, &mut st, &mut rename);
                return;
            }
            let name = layer.name.clone();
            let text = if current {
                RichText::new(name)
                    .color(Color32::WHITE)
                    .background_color(theme::SELECT_BLUE)
            } else {
                RichText::new(name)
            };
            let r = ui.add(egui::Label::new(text).sense(Sense::click_and_drag()));
            let r = if layer.web {
                r
            } else {
                r.on_hover_text("Double-click to rename; drag to reorder")
            };
            if r.clicked() {
                set_current = Some(li);
            }
            if r.double_clicked() && !layer.web {
                st.renaming = Some((li, None, layer.name.clone()));
            }
            if r.drag_started() && !layer.web {
                st.dragging = Some(Dragged::Layer(li));
            }
        });
        rows.push((header.response.rect, Row::Layer(li)));
        if layer.disclosed {
            for (ei, el) in layer.elements(frame).iter().enumerate().rev() {
                object_rows(ui, &mut st, d, ElementRef::new(li, ei), el, 0, false, &mut actions, &mut rows);
            }
        }
    }
    if let Some(r) = actions.drag {
        // Dragging an unselected object moves just that one.
        if !d.selection.contains(&r) {
            d.selection = vec![r];
            d.pixels = None;
        }
        st.dragging = Some(Dragged::Objects);
    }
    if let Some(r) = actions.toggle_open
        && !st.open.remove(&r)
    {
        st.open.insert(r);
    }
    rename = rename.or(actions.rename.take());
    let (select, toggle_obj_vis, toggle_obj_lock) = (actions.select, actions.toggle_vis, actions.toggle_lock);
    // Drag in progress: show where it would land; drop on release.
    if let Some(dragged) = st.dragging {
        let (pos, released, down) = ui.input(|i| {
            (
                i.pointer.interact_pos(),
                i.pointer.any_released(),
                i.pointer.any_down(),
            )
        });
        let target = pos.and_then(|p| drop_target(d, &rows, dragged, p.y));
        if let Some(t) = &target {
            let y = match t {
                Drop::Objects(_, _, y) | Drop::Layer(_, y) => *y,
            };
            let x = rows.first().map_or(ui.min_rect().x_range(), |(r, _)| r.x_range());
            ui.painter().hline(x, y, Stroke::new(2.0, theme::SELECT_BLUE));
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        }
        if released || !down {
            match (dragged, target) {
                (Dragged::Objects, Some(Drop::Objects(li, index, _))) => d.move_selection_to(li, index),
                (Dragged::Layer(from), Some(Drop::Layer(slot, _))) => {
                    d.move_layer(from, if from < slot { slot - 1 } else { slot });
                }
                _ => {}
            }
            st.dragging = None;
        }
    }
    if let Some((li, obj, name)) = rename {
        match obj {
            None => d.rename_layer(li, &name),
            Some(r) => d.rename_object(r, &name),
        }
    }
    if let Some(i) = toggle_vis {
        if let Some(l) = d.doc.layers.get_mut(i) {
            l.visible = !l.visible;
        }
        d.revision += 1;
    }
    if let Some(i) = toggle_lock
        && let Some(l) = d.doc.layers.get_mut(i)
    {
        l.locked = !l.locked;
    }
    if let Some(r) = toggle_obj_vis {
        d.toggle_object_visible(r);
    }
    if let Some(r) = toggle_obj_lock {
        d.toggle_object_locked(r);
    }
    if let Some(i) = toggle_disc
        && let Some(l) = d.doc.layers.get_mut(i)
    {
        l.disclosed = !l.disclosed;
    }
    if let Some(i) = set_current {
        d.set_current_layer(i);
    }
    // With pixels selected, picking a bitmap keeps the selection and edits that bitmap with
    // it (Fireworks MX, "Edit Bitmap"); the same selection can then be cleared or filled on
    // one bitmap after another.
    let shift = ui.input(|i| i.modifiers.shift);
    if let Some(r) = select
        && (shift || !d.edit_bitmap(r))
    {
        d.doc.current_layer = r.layer;
        // Shift+click adds or removes an object, as on the canvas.
        if shift {
            if let Some(k) = d.selection.iter().position(|s| *s == r) {
                d.selection.remove(k);
            } else {
                d.selection.push(r);
            }
        } else {
            d.selection = vec![r];
        }
        d.pixels = None;
    }
    ui.ctx().data_mut(|m| m.insert_temp(state_id, st));
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(format!("Frame {}", frame + 1));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if icons::glyph_button(ui, Glyph::Trash, 16.0, true)
                .on_hover_text("Delete Layer")
                .clicked()
            {
                let cur = d.doc.current_layer;
                d.delete_layer(cur);
            }
            if icons::glyph_button(ui, Glyph::NewBitmap, 16.0, true)
                .on_hover_text("New Bitmap Image")
                .clicked()
            {
                d.insert_empty_bitmap();
            }
            if icons::glyph_button(ui, Glyph::Plus, 16.0, true)
                .on_hover_text("New Layer")
                .clicked()
            {
                d.new_layer();
            }
        });
    });
}

fn align_tab_id() -> egui::Id {
    egui::Id::unique("optimize_align_tab")
}

/// Window > Align: open the Optimize and Align group on its Align tab (as Fireworks MX does).
pub fn show_align(app: &mut FireCraftApp, ctx: &egui::Context) {
    ctx.data_mut(|d| {
        d.insert_temp(egui::Id::unique(("dock_open", Group::Optimize as u8)), true);
        d.insert_temp(align_tab_id(), true);
    });
    app.prefs.show_dock = true;
}

fn optimize_align(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let mut align = ui.ctx().data(|d| d.get_temp::<bool>(align_tab_id())).unwrap_or(false);
    ui.horizontal(|ui| {
        ui.selectable_value(&mut align, false, "Optimize");
        ui.selectable_value(&mut align, true, "Align");
    });
    ui.ctx().data_mut(|d| d.insert_temp(align_tab_id(), align));
    if align {
        crate::align_panel::panel(app, ui);
    } else {
        crate::optimize::panel(app, ui);
    }
}

fn frames_history(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let tab_id = egui::Id::unique("frames_history_tab");
    let mut history = ui.ctx().data(|d| d.get_temp::<bool>(tab_id)).unwrap_or(false);
    ui.horizontal(|ui| {
        ui.selectable_value(&mut history, false, "Frames");
        ui.selectable_value(&mut history, true, "History");
    });
    ui.ctx().data_mut(|d| d.insert_temp(tab_id, history));
    let Some(d) = app.session.active_doc_mut() else {
        return;
    };
    if history {
        // Every step, oldest first; steps after the current one are greyed (undone). Clicking a
        // step moves the document to the state just after it, backwards or forwards.
        let done: Vec<String> = d.history.labels().map(str::to_owned).collect();
        let undone: Vec<String> = d.history.redo_labels().map(str::to_owned).collect();
        let current = done.len();
        let mut target: Option<usize> = None;
        let row = |ui: &mut egui::Ui, text: String, active: bool, here: bool| {
            let rt = if here {
                RichText::new(text)
                    .color(Color32::WHITE)
                    .background_color(theme::SELECT_BLUE)
            } else if active {
                RichText::new(text)
            } else {
                RichText::new(text).color(Color32::GRAY)
            };
            ui.add(egui::Label::new(rt).sense(Sense::click())).clicked()
        };
        if row(ui, "(Original)".into(), true, current == 0) {
            target = Some(0);
        }
        for (i, l) in done.iter().chain(undone.iter()).enumerate() {
            if row(ui, format!("{}. {l}", i + 1), i < current, i + 1 == current) {
                target = Some(i + 1);
            }
        }
        if let Some(t) = target {
            for _ in t..current {
                d.undo();
            }
            for _ in current..t {
                d.redo();
            }
        }
        ui.horizontal(|ui| {
            ui.add_enabled(false, egui::Button::new("Replay"))
                .on_disabled_hover_text("Replaying steps is on the roadmap");
            ui.weak(format!("{} of {} steps", current, d.history.limit));
        });
    } else {
        let mut pick = None;
        egui::ScrollArea::vertical()
            .max_height(220.0)
            .id_salt("frames_list")
            .show(ui, |ui| {
                for (i, f) in d.doc.frames.iter().enumerate() {
                    let cur = i == d.doc.current_frame;
                    ui.horizontal(|ui| {
                        let text = format!("{}  {}", i + 1, f.name);
                        let rt = if cur {
                            RichText::new(text)
                                .color(Color32::WHITE)
                                .background_color(theme::SELECT_BLUE)
                        } else {
                            RichText::new(text)
                        };
                        if ui.add(egui::Label::new(rt).sense(Sense::click())).clicked() {
                            pick = Some(i);
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.weak(f.delay.to_string())
                        })
                        .response
                        .on_hover_text("Frame delay (1/100 s)");
                    });
                }
            });
        if let Some(i) = pick {
            d.doc.current_frame = i;
        }
        ui.weak("Adding, removing and timing frames is on the roadmap.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> Element {
        Element::new(ElementKind::Group(Vec::new()))
    }

    fn masked(n: usize) -> Element {
        Element::new(ElementKind::Masked {
            content: (0..n).map(|_| plain()).collect(),
            mask: Box::new(plain()),
            mode: MaskMode::Grayscale,
            cache: None,
            cache_key: 0,
        })
    }

    #[test]
    fn rows_are_named_as_in_fireworks_cs() {
        // One masked object takes its content's name; several count as a group.
        let mut one = masked(1);
        if let ElementKind::Masked { content, .. } = &mut one.kind {
            content[0].name = Some("Photo".into());
        }
        assert_eq!(row_label(&one), "Photo");
        assert_eq!(row_label(&masked(3)), "Group: 3 objects");
        let group = Element::new(ElementKind::Group(vec![plain(), plain()]));
        assert_eq!(row_label(&group), "Group: 2 objects");
        let mut named = masked(3);
        named.name = Some("Banner".into());
        assert_eq!(row_label(&named), "Banner");
    }

    #[test]
    fn masked_objects_and_groups_open() {
        assert!(expandable(&masked(1)));
        assert!(expandable(&Element::new(ElementKind::Group(vec![plain()]))));
        assert!(!expandable(&plain()));
    }

    #[test]
    fn inside_follows_the_child_path() {
        let top = ElementRef::new(1, 2);
        let child = top.child(0).unwrap();
        let grandchild = child.child(3).unwrap();
        assert!(is_inside(child, top));
        assert!(is_inside(grandchild, top));
        assert!(is_inside(grandchild, child));
        assert!(!is_inside(top, top));
        assert!(!is_inside(child, ElementRef::new(1, 3)));
    }
}
