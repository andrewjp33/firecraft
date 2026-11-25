//! The Optimize panel's color table (Fireworks MX's): the colors of a GIF or PNG 8 export, and
//! their editing.
//!
//! As in Fireworks MX (its help, "Viewing colors in a palette" and after): the table shows the
//! palette of the last export (it follows the document in Preview; otherwise a Rebuild button
//! appears once the document has changed). A click selects a color, Ctrl+click adds one,
//! Shift+click a range; a double-click edits it. Swatches are marked as edited (a square at the
//! bottom left), locked (bottom right), transparent (checkered) and web safe (a diamond). The
//! buttons below: Select Transparent Color, Add Color to Transparency, Remove Color from
//! Transparency (pick a swatch or a pixel of the preview), Edit Color, Snap to Web Safe, Lock
//! Color, Add Color and Delete Color. Edits make the palette Custom (the table itself, saved in
//! the document as Fireworks MX saves it); locked colors stay with any palette. The right-click
//! menu has those and Fireworks MX's Unlock All Colors, Remove Edit, Remove All Edits, the sort
//! orders, Load Palette, Save Palette and Rebuild Color Table.

use std::collections::BTreeSet;

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use firecraft_doc::{ExportSettings, PaletteEntry, PaletteKind, Transparency};
use firecraft_raster::Rgba;

use crate::FireCraftApp;
use crate::fpui::color_picker::{self as cp, ColorChoice, Footer};
use crate::icons::{self, Glyph};

/// The table's state: selection, sort order and the tool waiting for a color (one for the app;
/// it follows the active document).
#[derive(Default)]
pub(crate) struct TableState {
    doc: u64,
    /// Selected colors (positions in the table as built) and the Shift+click anchor.
    selected: BTreeSet<usize>,
    anchor: Option<usize>,
    /// How many colors the selection was made in (a different table clears it).
    rows: usize,
    sort: Sort,
    /// A transparency eyedropper waiting for a swatch or a pixel of the preview.
    pub(crate) pick: Option<Pick>,
    /// The color a transparency eyedropper took from the preview: (document, color).
    pub(crate) picked: Option<(u64, Rgba)>,
    /// The color pop-up of Edit Color or Add Color.
    editing: Option<Editing>,
    /// Load Palette is waiting for its file (for this document).
    loading: Option<u64>,
}

/// The table's order (Fireworks MX's Sort by).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Sort {
    #[default]
    None,
    Luminance,
    Popularity,
}

impl Sort {
    const ALL: [Sort; 3] = [Sort::None, Sort::Luminance, Sort::Popularity];

    fn label(self) -> &'static str {
        match self {
            Sort::None => "None",
            Sort::Luminance => "Luminance",
            Sort::Popularity => "Popularity",
        }
    }
}

/// The transparency eyedroppers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pick {
    /// The picked color becomes the only transparent one.
    Select,
    Add,
    Remove,
}

impl Pick {
    fn label(self) -> &'static str {
        match self {
            Pick::Select => "Select Transparent Color",
            Pick::Add => "Add Color to Transparency",
            Pick::Remove => "Remove Color from Transparency",
        }
    }
}

#[derive(Clone, Copy)]
struct Editing {
    /// The color edited (its position in the table), or `None` for Add Color.
    row: Option<usize>,
    anchor: Rect,
    /// Whether the pop-up was asked to open (it opens in the frame after).
    opened: bool,
}

/// A color of the table as shown.
#[derive(Clone, Copy, Debug)]
struct Row {
    entry: PaletteEntry,
    /// Pixels using it in the last export, when known.
    usage: Option<u32>,
}

fn same_rgb(a: Rgba, b: Rgba) -> bool {
    (a.r, a.g, a.b) == (b.r, b.g, b.b)
}

/// The table: a custom palette's own entries; otherwise the last export's palette (with a table
/// per frame, the one of document frame `frame`), with the table's locked colors marked.
fn rows(s: &ExportSettings, o: Option<&crate::canvas::Optimized>, current: bool, frame: usize) -> Vec<Row> {
    let table = o.and_then(|o| o.animation.as_ref()).and_then(|a| a.table(frame));
    let (palette, transparent, usage_of) = match (table, o) {
        (Some(t), _) => (&t.palette[..], t.transparent, &t.usage[..]),
        (None, Some(o)) => (&o.palette[..], o.transparent, &o.usage[..]),
        (None, None) => (&[][..], None, &[][..]),
    };
    let usage = |i: usize| usage_of.get(i).copied().filter(|_| current);
    if s.palette == PaletteKind::Custom {
        // The export's entries by color (Remove Unused Colors leaves some out).
        return s
            .table
            .iter()
            .map(|&entry| {
                let c = entry.color;
                let at = palette.iter().position(|p| p[..3] == [c.r, c.g, c.b]);
                Row {
                    entry,
                    usage: current.then(|| at.and_then(usage).unwrap_or(0)),
                }
            })
            .collect();
    }
    palette
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let color = Rgba::new(c[0], c[1], c[2], c[3]);
            Row {
                entry: PaletteEntry {
                    color,
                    original: None,
                    locked: c[3] != 0 && s.locked().any(|e| same_rgb(e.source(), color)),
                    transparent: Some(i) == transparent || c[3] == 0,
                },
                usage: usage(i),
            }
        })
        .collect()
}

/// Make the table the palette (Custom), as an edit in Fireworks MX does. The position in the
/// table of each row (none for the entry an export adds for transparent pixels).
fn make_custom(s: &mut ExportSettings, rows: &[Row]) -> Vec<Option<usize>> {
    if s.palette == PaletteKind::Custom {
        return (0..rows.len()).map(Some).collect();
    }
    let mut map = Vec::with_capacity(rows.len());
    let mut table = Vec::with_capacity(rows.len());
    for r in rows {
        if r.entry.color.a == 0 {
            map.push(None);
            continue;
        }
        map.push(Some(table.len()));
        table.push(PaletteEntry {
            transparent: false,
            ..r.entry
        });
    }
    s.palette = PaletteKind::Custom;
    s.colors = table.len().clamp(2, 256) as u16;
    s.table = table;
    map
}

/// The table entries of these rows, made custom first.
fn custom_entries(s: &mut ExportSettings, rows: &[Row], sel: &[usize]) -> Vec<usize> {
    let map = make_custom(s, rows);
    sel.iter().filter_map(|&i| map.get(i).copied().flatten()).collect()
}

/// Give an entry a new color; it remembers the color it had (Fireworks MX's edited mark).
fn recolor(e: &mut PaletteEntry, c: Rgba) {
    let original = e.source();
    e.color = Rgba { a: 255, ..c };
    e.original = (!same_rgb(e.color, original)).then_some(original);
}

fn web_safe(c: Rgba) -> Rgba {
    let snap = |v: u8| ((u16::from(v) + 25) / 51 * 51) as u8;
    Rgba::new(snap(c.r), snap(c.g), snap(c.b), 255)
}

fn is_web_safe(c: Rgba) -> bool {
    c.r % 51 == 0 && c.g % 51 == 0 && c.b % 51 == 0
}

/// Transparent on (if any selected color isn't) or off.
fn toggle_transparent(s: &mut ExportSettings, rows: &[Row], sel: &[usize]) {
    let on = sel.iter().any(|&i| rows.get(i).is_some_and(|r| !r.entry.transparent));
    for i in custom_entries(s, rows, sel) {
        if let Some(e) = s.table.get_mut(i) {
            e.transparent = on;
        }
    }
    clear_needs_transparency(s);
}

/// A transparent color needs a transparency mode: Index Transparency when there was none.
fn clear_needs_transparency(s: &mut ExportSettings) {
    if s.transparency == Transparency::None && s.table.iter().any(|e| e.transparent) {
        s.transparency = Transparency::Index;
    }
}

/// Lock (if any selected color isn't locked) or unlock. Locking doesn't make the palette custom.
fn toggle_lock(s: &mut ExportSettings, rows: &[Row], sel: &[usize]) {
    let picked: Vec<Row> = sel.iter().filter_map(|&i| rows.get(i).copied()).filter(|r| r.entry.color.a != 0).collect();
    let lock = picked.iter().any(|r| !r.entry.locked);
    if s.palette == PaletteKind::Custom {
        for &i in sel {
            if let Some(e) = s.table.get_mut(i) {
                e.locked = lock;
            }
        }
        return;
    }
    for r in picked {
        let c = r.entry.color;
        if lock {
            if !s.table.iter().any(|e| same_rgb(e.source(), c)) {
                s.table.push(PaletteEntry {
                    locked: true,
                    ..PaletteEntry::new(Rgba { a: 255, ..c })
                });
            }
        } else {
            s.table.retain(|e| !same_rgb(e.source(), c));
        }
    }
}

fn snap_web(s: &mut ExportSettings, rows: &[Row], sel: &[usize]) {
    for i in custom_entries(s, rows, sel) {
        if let Some(e) = s.table.get_mut(i) {
            let c = web_safe(e.color);
            recolor(e, c);
        }
    }
}

fn delete(s: &mut ExportSettings, rows: &[Row], sel: &[usize]) {
    let mut gone = custom_entries(s, rows, sel);
    gone.sort_unstable();
    for i in gone.into_iter().rev() {
        if i < s.table.len() && s.table.len() > 1 {
            s.table.remove(i);
        }
    }
    s.colors = s.table.len().clamp(2, 256) as u16;
}

fn add_color(s: &mut ExportSettings, rows: &[Row], c: Rgba) {
    make_custom(s, rows);
    if s.table.len() < 256 {
        s.table.push(PaletteEntry::new(Rgba { a: 255, ..c }));
        s.colors = s.table.len().clamp(2, 256) as u16;
    }
}

fn remove_edits(s: &mut ExportSettings, sel: Option<&[usize]>) {
    if s.palette != PaletteKind::Custom {
        return;
    }
    for (i, e) in s.table.iter_mut().enumerate() {
        if sel.is_none_or(|sel| sel.contains(&i))
            && let Some(o) = e.original.take()
        {
            e.color = o;
        }
    }
}

fn unlock_all(s: &mut ExportSettings) {
    if s.palette == PaletteKind::Custom {
        s.table.iter_mut().for_each(|e| e.locked = false);
    } else {
        s.table.clear();
    }
}

/// A transparency eyedropper's pick of the row `i`.
fn apply_pick(s: &mut ExportSettings, rows: &[Row], pick: Pick, i: usize) {
    let map = make_custom(s, rows);
    let Some(t) = map.get(i).copied().flatten() else { return };
    if pick == Pick::Select {
        s.table.iter_mut().for_each(|e| e.transparent = false);
    }
    if let Some(e) = s.table.get_mut(t) {
        e.transparent = pick != Pick::Remove;
    }
    clear_needs_transparency(s);
}

/// The order the rows are shown in.
fn order(rows: &[Row], sort: Sort) -> Vec<usize> {
    let mut order: Vec<usize> = (0..rows.len()).collect();
    match sort {
        Sort::None => {}
        Sort::Luminance => {
            let luma = |r: &Row| {
                let c = r.entry.color;
                299 * u32::from(c.r) + 587 * u32::from(c.g) + 114 * u32::from(c.b)
            };
            order.sort_by_key(|&i| rows.get(i).map_or(0, luma));
        }
        Sort::Popularity => {
            order.sort_by_key(|&i| std::cmp::Reverse(rows.get(i).and_then(|r| r.usage).unwrap_or(0)));
        }
    }
    order
}

/// What a click in the table, a button or the menu asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Action {
    Edit,
    Transparent,
    SnapWeb,
    Lock,
    Add,
    Delete,
    UnlockAll,
    RemoveEdit,
    RemoveAllEdits,
    Sort(Sort),
    Load,
    Save,
    Rebuild,
    Pick(Pick),
}

/// The color table under the Optimize panel's settings.
pub(crate) fn color_table(app: &mut FireCraftApp, ui: &mut Ui, id: u64) {
    if app.optimize_table.doc != id {
        app.optimize_table = TableState {
            doc: id,
            sort: app.optimize_table.sort,
            ..TableState::default()
        };
    }
    let Some(d) = app.session.docs.iter().find(|d| d.id == id) else { return };
    let view = app.views.get(&id);
    let current = view.is_some_and(|v| crate::canvas::optimized_current(v, d));
    let optimized = view.and_then(|v| v.optimized.as_ref());
    let s = d.doc.export.clone();
    let rows = rows(&s, optimized, current, d.doc.current_frame);
    let st = &mut app.optimize_table;
    if st.rows != rows.len() {
        st.selected.clear();
        st.anchor = None;
        st.rows = rows.len();
    }
    let order = order(&rows, st.sort);
    let mut actions: Vec<Action> = Vec::new();
    // A color the transparency eyedropper took from the preview.
    if let Some((doc, c)) = st.picked.take()
        && doc == id
        && let Some(pick) = st.pick.take()
        && let Some(i) = rows.iter().position(|r| r.entry.color == c)
    {
        st.selected = BTreeSet::from([i]);
        st.anchor = Some(i);
        actions.push(Action::Pick(pick));
    }

    // ---- The swatches: a fixed number of lines that scroll (Fireworks MX) ----
    let pal = crate::theme::pal();
    let view_h = CELL_H * VISIBLE_LINES as f32;
    let framed = egui::Frame::new()
        .stroke(Stroke::new(1.0, pal.border))
        .show(ui, |ui| {
            // A bar beside the swatches (not over them), in the text color as the panels'
            // floating bars are (light in the dark themes).
            ui.spacing_mut().scroll = egui::style::ScrollStyle {
                foreground_color: true,
                ..egui::style::ScrollStyle::solid()
            };
            egui::ScrollArea::vertical()
                .id_salt("opt_table_scroll")
                .max_height(view_h)
                .min_scrolled_height(view_h)
                .auto_shrink([false, true])
                .show(ui, |ui| swatches(ui, st, &rows, &order, &s, &mut actions))
                .inner
        });
    let area = framed.response.rect;
    let mut edit_anchor = framed.inner;
    if rows.is_empty() {
        ui.painter().text(
            area.center(),
            egui::Align2::CENTER_CENTER,
            "Rebuild to show the colors",
            egui::FontId::proportional(11.0),
            pal.text_weak,
        );
    }

    // ---- Sort by, and the count or Rebuild ----
    ui.horizontal(|ui| {
        ui.label("Sort by:");
        let mut sort = st.sort;
        crate::theme::combo("opt_sort")
            .selected_text(sort.label())
            .width(90.0)
            .show_ui(ui, |ui| {
                for o in Sort::ALL {
                    ui.selectable_value(&mut sort, o, o.label());
                }
            });
        if sort != st.sort {
            actions.push(Action::Sort(sort));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if current {
                ui.label(format!("{} colors", rows.len()));
            } else if ui
                .button("Rebuild")
                .on_hover_text("Build the color table for the document as it is now")
                .clicked()
            {
                actions.push(Action::Rebuild);
            }
        });
    });

    // ---- The buttons ----
    let have_sel = !st.selected.is_empty();
    let custom = s.palette == PaletteKind::Custom;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (pick, sign) in [(Pick::Select, '='), (Pick::Add, '+'), (Pick::Remove, '-')] {
            let on = st.pick == Some(pick);
            let r = icons::glyph_button(ui, Glyph::ClearPick(sign), 20.0, !rows.is_empty());
            if on {
                ui.painter().rect_stroke(r.rect, 2.0, Stroke::new(1.5, crate::theme::pal().select), egui::StrokeKind::Inside);
            }
            if r.on_hover_text(format!("{} (click a swatch, or the Preview)", pick.label())).clicked() {
                st.pick = if on { None } else { Some(pick) };
            }
        }
        ui.add_space(6.0);
        let r = icons::glyph_button(ui, Glyph::ColorWheel, 20.0, have_sel).on_hover_text("Edit Color");
        if r.clicked() {
            edit_anchor = Some(r.rect);
            actions.push(Action::Edit);
        }
        if icons::glyph_button(ui, Glyph::WebCube, 20.0, have_sel).on_hover_text("Snap to Web Safe").clicked() {
            actions.push(Action::SnapWeb);
        }
        if icons::glyph_button(ui, Glyph::Lock, 20.0, have_sel).on_hover_text("Lock Color").clicked() {
            actions.push(Action::Lock);
        }
        ui.add_space(6.0);
        let can_add = rows.len() < 256;
        let r = icons::glyph_button(ui, Glyph::AddSwatch, 20.0, can_add).on_hover_text("Add Color");
        if r.clicked() {
            st.editing = Some(Editing {
                row: None,
                anchor: r.rect,
                opened: false,
            });
        }
        let can_delete = have_sel && (custom || !rows.is_empty()) && rows.len() > st.selected.len();
        if icons::glyph_button(ui, Glyph::Trash, 20.0, can_delete).on_hover_text("Delete Color").clicked() {
            actions.push(Action::Delete);
        }
    });
    if st.pick.is_some() {
        ui.weak("Click a swatch, or a color in the Preview (Esc: cancel).");
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            st.pick = None;
        }
    }

    // Edit Color opens the color pop-up on the first selected color.
    if actions.contains(&Action::Edit)
        && let Some(&i) = st.selected.iter().next()
    {
        st.editing = Some(Editing {
            row: Some(i),
            anchor: edit_anchor.unwrap_or(area),
            opened: false,
        });
    }
    actions.retain(|a| *a != Action::Edit);
    let edited = edit_popup(ui, st, &rows);
    let sel: Vec<usize> = st.selected.iter().copied().collect();

    // ---- Apply ----
    let mut new = s.clone();
    if let Some((row, c)) = edited {
        match row {
            Some(i) => {
                if let Some(t) = custom_entries(&mut new, &rows, &[i]).first()
                    && let Some(e) = new.table.get_mut(*t)
                {
                    recolor(e, c);
                }
            }
            None => add_color(&mut new, &rows, c),
        }
    }
    let mut load = false;
    let mut save = false;
    let mut rebuild = false;
    for a in actions {
        match a {
            Action::Transparent => toggle_transparent(&mut new, &rows, &sel),
            Action::SnapWeb => snap_web(&mut new, &rows, &sel),
            Action::Lock => toggle_lock(&mut new, &rows, &sel),
            Action::Delete => {
                delete(&mut new, &rows, &sel);
                st.selected.clear();
            }
            Action::UnlockAll => unlock_all(&mut new),
            Action::RemoveEdit => remove_edits(&mut new, Some(&sel)),
            Action::RemoveAllEdits => remove_edits(&mut new, None),
            Action::Sort(o) => st.sort = o,
            Action::Pick(pick) => {
                if let Some(&i) = sel.first() {
                    apply_pick(&mut new, &rows, pick, i);
                }
            }
            Action::Add => {
                st.editing = Some(Editing {
                    row: None,
                    anchor: area,
                    opened: false,
                });
            }
            Action::Load => load = true,
            Action::Save => save = true,
            Action::Rebuild => rebuild = true,
            Action::Edit => {}
        }
    }
    if new != s
        && let Some(d) = app.session.docs.iter_mut().find(|d| d.id == id)
    {
        d.doc.export = new;
        d.dirty = true;
    }
    if rebuild {
        rebuild_table(app, ui.ctx(), id);
    }
    if load {
        load_palette(app);
    }
    if save {
        save_palette(app, &rows);
    }
}

/// Lines of swatches shown at once (more scroll), and a swatch's height.
const VISIBLE_LINES: usize = 12;
/// A swatch's cell at its narrowest (the columns widen to fill the row), its 1-point separator line included (Fireworks MX's swatches are about this
/// small, with single lines between them).
const CELL_W: f32 = 12.0;
const CELL_H: f32 = 10.0;

/// The swatches (in the scrolling area); the rectangle of the selected one, if shown.
fn swatches(
    ui: &mut Ui,
    st: &mut TableState,
    rows: &[Row],
    order: &[usize],
    s: &ExportSettings,
    actions: &mut Vec<Action>,
) -> Option<Rect> {
    // Room for the closing line on the right.
    let room = ui.available_width() - 1.0;
    let cols = ((room / CELL_W).floor() as usize).max(6);
    let lines = rows.len().div_ceil(cols).max(VISIBLE_LINES);
    let (area, _) = ui.allocate_exact_size(vec2(room, CELL_H * lines as f32), Sense::hover());
    // The columns share the whole width (no gap on the right), their edges on whole pixels.
    let ppp = ui.ctx().pixels_per_point();
    let edge = |c: usize| area.left() + (c as f32 * room / cols as f32 * ppp).round() / ppp;
    let p = ui.painter().clone();
    let border = crate::theme::pal().border;
    let mut edit_anchor = None;
    let mut selected = Vec::new();
    for (k, &i) in order.iter().enumerate() {
        let Some(row) = rows.get(i) else { continue };
        let (c, line) = (k % cols, k / cols);
        let at = pos2(edge(c), area.top() + line as f32 * CELL_H);
        let cell = vec2(edge(c + 1) - at.x, CELL_H);
        // The swatch inside the cell's lines (shared with the neighbours).
        let r = Rect::from_min_max(at + vec2(1.0, 1.0), at + cell);
        paint_cell(&p, r, row.entry);
        if st.selected.contains(&i) {
            selected.push(r);
        }
        p.rect_stroke(
            Rect::from_min_size(at + vec2(0.5, 0.5), cell),
            0.0,
            Stroke::new(1.0, border),
            egui::StrokeKind::Middle,
        );
        let resp = ui.interact(r, egui::Id::unique(("opt_swatch", i)), Sense::click());
        if resp.clicked() {
            if let Some(pick) = st.pick.take() {
                st.selected = BTreeSet::from([i]);
                actions.push(Action::Pick(pick));
            } else {
                let mods = ui.input(|inp| inp.modifiers);
                select(st, order, i, mods.command, mods.shift);
            }
        }
        if resp.double_clicked() {
            st.selected = BTreeSet::from([i]);
            edit_anchor = Some(r);
            actions.push(Action::Edit);
        }
        if resp.secondary_clicked() && !st.selected.contains(&i) {
            st.selected = BTreeSet::from([i]);
            st.anchor = Some(i);
        }
        let resp = resp.on_hover_ui(|ui| swatch_info(ui, row));
        egui::Popup::context_menu(&resp)
            .style(crate::theme::menu_popup_style())
            .show(|ui| menu(ui, s, rows, &st.selected, st.sort, actions));
        if st.selected.contains(&i) && edit_anchor.is_none() {
            edit_anchor = Some(r);
        }
    }
    // The selection over the lines: black on the cell's lines, white inside them.
    for r in selected {
        p.rect_stroke(r.expand(0.5), 0.0, Stroke::new(1.0, Color32::BLACK), egui::StrokeKind::Middle);
        p.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::WHITE), egui::StrokeKind::Inside);
    }
    edit_anchor
}

/// Click, Ctrl+click (toggle) and Shift+click (range in the shown order).
fn select(st: &mut TableState, order: &[usize], i: usize, toggle: bool, range: bool) {
    if range && let Some(a) = st.anchor {
        let pos = |x: usize| order.iter().position(|&o| o == x).unwrap_or(0);
        let (lo, hi) = (pos(a).min(pos(i)), pos(a).max(pos(i)));
        if !toggle {
            st.selected.clear();
        }
        st.selected.extend(order.get(lo..=hi).unwrap_or(&[]).iter().copied());
        return;
    }
    if toggle {
        if !st.selected.remove(&i) {
            st.selected.insert(i);
        }
    } else {
        st.selected = BTreeSet::from([i]);
    }
    st.anchor = Some(i);
}

/// The swatch tooltip: its color, marks and use.
fn swatch_info(ui: &mut Ui, row: &Row) {
    let e = row.entry;
    let c = e.color;
    ui.label(format!("#{:02X}{:02X}{:02X}  R {} G {} B {}", c.r, c.g, c.b, c.r, c.g, c.b));
    if c.a != 255 && c.a != 0 {
        ui.label(format!("Opacity {}%", (u32::from(c.a) * 100 + 127) / 255));
    }
    if let Some(o) = e.original {
        ui.label(format!("Edited (was #{:02X}{:02X}{:02X})", o.r, o.g, o.b));
    }
    if e.locked {
        ui.label("Locked");
    }
    if e.transparent {
        ui.label("Transparent");
    } else if is_web_safe(c) {
        ui.label("Web safe");
    }
    if let Some(n) = row.usage {
        ui.label(format!("{n} pixels"));
    }
}

/// The right-click menu (Fireworks MX's palette menu).
fn menu(ui: &mut Ui, s: &ExportSettings, rows: &[Row], sel: &BTreeSet<usize>, sort: Sort, out: &mut Vec<Action>) {
    let custom = s.palette == PaletteKind::Custom;
    let any_edit = |only: bool| {
        s.table
            .iter()
            .enumerate()
            .any(|(i, e)| custom && e.original.is_some() && (!only || sel.contains(&i)))
    };
    let mut item = |ui: &mut Ui, label: &str, enabled: bool, a: Action| {
        if crate::menus::item(ui, label, "", enabled) {
            out.push(a);
        }
    };
    let have = !sel.is_empty();
    item(ui, "Edit Color...", have, Action::Edit);
    item(ui, "Add Color...", rows.len() < 256, Action::Add);
    item(ui, "Delete Color", have && rows.len() > sel.len(), Action::Delete);
    ui.separator();
    let transparent = have && sel.iter().all(|&i| rows.get(i).is_some_and(|r| r.entry.transparent));
    let locked = have && sel.iter().all(|&i| rows.get(i).is_some_and(|r| r.entry.locked));
    item(ui, "Snap to Web Safe", have, Action::SnapWeb);
    item(ui, if transparent { "Not Transparent" } else { "Transparent" }, have, Action::Transparent);
    item(ui, if locked { "Unlock Color" } else { "Lock Color" }, have, Action::Lock);
    item(ui, "Unlock All Colors", s.locked().next().is_some(), Action::UnlockAll);
    ui.separator();
    for (label, o) in [("Sort by Luminance", Sort::Luminance), ("Sort by Popularity", Sort::Popularity), ("Unsorted", Sort::None)] {
        let label = if sort == o { format!("{label}  \u{2713}") } else { label.to_owned() };
        item(ui, &label, true, Action::Sort(o));
    }
    ui.separator();
    item(ui, "Remove Edit", any_edit(true), Action::RemoveEdit);
    item(ui, "Remove All Edits", any_edit(false), Action::RemoveAllEdits);
    ui.separator();
    item(ui, "Load Palette...", true, Action::Load);
    item(ui, "Save Palette...", !rows.is_empty(), Action::Save);
    item(ui, "Rebuild Color Table", true, Action::Rebuild);
}

/// The color pop-up of Edit Color / Add Color; the color chosen, and for which row.
fn edit_popup(ui: &mut Ui, st: &mut TableState, rows: &[Row]) -> Option<(Option<usize>, Rgba)> {
    let mut ed = st.editing?;
    let swatch = egui::Id::unique("opt_table_color");
    let color = match ed.row {
        Some(i) => rows.get(i).map(|r| r.entry.color),
        None => Some(Rgba::new(255, 255, 255, 255)),
    };
    let open_at = (!ed.opened).then_some(ed.anchor.left());
    let choice = cp::show(
        ui,
        swatch,
        ed.anchor,
        open_at,
        &|_| None,
        color.map(cp::to_argb),
        Footer::Plain,
    );
    let open = egui::Popup::is_id_open(ui.ctx(), cp::popup_id(swatch));
    if let Some(ColorChoice::Color(c)) = choice {
        st.editing = None;
        return Some((ed.row, cp::to_rgba(c)));
    }
    if ed.opened && !open {
        st.editing = None;
    } else {
        ed.opened = true;
        st.editing = Some(ed);
    }
    None
}

/// One swatch: its color (over a checkerboard when transparent or partly so), and Fireworks MX's
/// marks.
fn paint_cell(p: &egui::Painter, r: Rect, e: PaletteEntry) {
    let c = e.color;
    // Transparent, or partly (a PNG 8's alpha palette): the color over a checkerboard.
    if e.transparent || c.a < 255 {
        p.rect_filled(r, 0.0, Color32::WHITE);
        let h = r.size() / 2.0;
        p.rect_filled(Rect::from_min_size(r.min, h), 0.0, Color32::from_gray(204));
        p.rect_filled(Rect::from_min_size(r.min + h, h), 0.0, Color32::from_gray(204));
    }
    if !e.transparent {
        p.rect_filled(r, 0.0, Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a));
    }
    // Web safe: a small diamond in the middle.
    if !e.transparent && c.a == 255 && is_web_safe(c) {
        let m = r.center();
        let d = 2.0;
        let pts = vec![pos2(m.x, m.y - d), pos2(m.x + d, m.y), pos2(m.x, m.y + d), pos2(m.x - d, m.y)];
        p.add(egui::Shape::convex_polygon(pts, Color32::WHITE, Stroke::new(1.0, Color32::BLACK)));
    }
    // Edited: a small square at the bottom left; locked: at the bottom right.
    let mark = |at: Pos2| {
        let b = Rect::from_min_size(at, vec2(4.0, 4.0));
        p.rect_filled(b, 0.0, Color32::WHITE);
        p.rect_stroke(b, 0.0, Stroke::new(1.0, Color32::BLACK), egui::StrokeKind::Inside);
        p.rect_filled(Rect::from_center_size(b.center(), vec2(1.0, 1.0)), 0.0, Color32::BLACK);
    };
    if e.original.is_some() {
        mark(pos2(r.left(), r.bottom() - 4.0));
    }
    if e.locked {
        mark(pos2(r.right() - 4.0, r.bottom() - 4.0));
    }
}

/// Rebuild Color Table: export again for the document as it is now.
fn rebuild_table(app: &mut FireCraftApp, ctx: &egui::Context, id: u64) {
    let Some(d) = app.session.docs.iter().find(|d| d.id == id) else { return };
    let view = app.views.entry(id).or_default();
    crate::canvas::refresh_optimized(view, d, ctx);
}

/// Load Palette: ask for the file (it may arrive later).
pub(super) fn load_palette(app: &mut FireCraftApp) {
    let Some(id) = app.session.active_doc().map(|d| d.id) else { return };
    app.optimize_table.loading = Some(id);
    if let Some(file) = app.platform.open_palette() {
        use_palette(app, id, file);
    }
}

/// A palette file chosen for Load Palette, once read.
pub(super) fn receive_palette(app: &mut FireCraftApp) {
    let Some(id) = app.optimize_table.loading else { return };
    match app.platform.take_palette() {
        Some(Ok(file)) => use_palette(app, id, file),
        Some(Err(e)) => {
            app.optimize_table.loading = None;
            app.error(e);
        }
        None => {}
    }
}

/// The file's colors become the document's custom palette.
fn use_palette(app: &mut FireCraftApp, id: u64, (name, bytes): (String, Vec<u8>)) {
    app.optimize_table.loading = None;
    let file = match firecraft_io::export::palette_file::read_palette(&bytes) {
        Ok(f) => f,
        Err(e) => {
            let shown = app.platform.display_path(&name);
            return app.error(format!("Could not load the palette {shown}: {e}"));
        }
    };
    let Some(d) = app.session.docs.iter_mut().find(|d| d.id == id) else { return };
    let s = &mut d.doc.export;
    s.palette = PaletteKind::Custom;
    s.table = file
        .colors
        .iter()
        .take(256)
        .enumerate()
        .map(|(i, &c)| PaletteEntry {
            transparent: file.transparent == Some(i),
            ..PaletteEntry::new(c)
        })
        .collect();
    s.colors = s.table.len().clamp(2, 256) as u16;
    clear_needs_transparency(s);
    d.dirty = true;
}

/// Save Palette: the table as an .act file.
fn save_palette(app: &mut FireCraftApp, rows: &[Row]) {
    if app.pending_save.is_some() {
        return;
    }
    let colors: Vec<Rgba> = rows.iter().map(|r| r.entry.color).collect();
    let transparent = rows.iter().position(|r| r.entry.transparent);
    let bytes = firecraft_io::export::palette_file::write_act(&colors, transparent);
    let name = app
        .session
        .active_doc()
        .map_or_else(|| "Palette".to_owned(), |d| d.doc.title());
    let stem = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s).to_owned();
    let status = app.platform.export_file(&format!("{stem}.act"), &bytes);
    app.save_status(crate::SaveTarget::Palette, status);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows_of(colors: &[Rgba]) -> Vec<Row> {
        colors
            .iter()
            .map(|&c| Row {
                entry: PaletteEntry::new(c),
                usage: None,
            })
            .collect()
    }

    #[test]
    fn an_edit_makes_the_palette_custom_and_remembers_the_color() {
        let (red, blue) = (Rgba::new(255, 0, 0, 255), Rgba::new(0, 0, 255, 255));
        let mut s = ExportSettings::default();
        // The export's palette has red, blue and its own transparent entry.
        let mut rows = rows_of(&[red, blue, Rgba::TRANSPARENT]);
        rows[2].entry.transparent = true;
        snap_web(&mut s, &rows, &[1]);
        assert_eq!(s.palette, PaletteKind::Custom);
        assert_eq!(s.table.len(), 2, "the export's transparent entry isn't a color");
        assert_eq!(s.table[1].color, blue, "blue is web safe already: no edit");
        assert_eq!(s.table[1].original, None);
        let rows = rows_of(&[red, blue]);
        let mut e = s.table[0];
        recolor(&mut e, Rgba::new(10, 20, 30, 255));
        assert_eq!(e.original, Some(red));
        recolor(&mut e, red);
        assert_eq!(e.original, None, "back to its own color: no longer edited");
        // Snap: 10/20/30 -> 0/0/51.
        s.table[0].color = Rgba::new(10, 20, 30, 255);
        snap_web(&mut s, &rows, &[0]);
        assert_eq!(s.table[0].color, Rgba::new(0, 0, 51, 255));
        remove_edits(&mut s, None);
        assert_eq!(s.table[0].color, Rgba::new(10, 20, 30, 255));
    }

    #[test]
    fn locks_stay_without_a_custom_palette() {
        let (red, blue) = (Rgba::new(255, 0, 0, 255), Rgba::new(0, 0, 255, 255));
        let mut s = ExportSettings::default();
        let rows = rows_of(&[red, blue]);
        toggle_lock(&mut s, &rows, &[1]);
        assert_eq!(s.palette, PaletteKind::Adaptive);
        assert_eq!(s.table.len(), 1);
        assert!(s.table[0].locked);
        let mut rows = rows;
        rows[1].entry.locked = true;
        toggle_lock(&mut s, &rows, &[1]);
        assert!(s.table.is_empty());
    }

    #[test]
    fn transparency_picks() {
        let colors = [Rgba::new(255, 0, 0, 255), Rgba::new(0, 0, 255, 255), Rgba::new(0, 255, 0, 255)];
        let mut s = ExportSettings::default();
        apply_pick(&mut s, &rows_of(&colors), Pick::Select, 0);
        assert_eq!(s.transparency, Transparency::Index);
        let rows: Vec<Row> = s.table.iter().map(|&entry| Row { entry, usage: None }).collect();
        apply_pick(&mut s, &rows, Pick::Add, 2);
        let clear: Vec<bool> = s.table.iter().map(|e| e.transparent).collect();
        assert_eq!(clear, [true, false, true]);
        let rows: Vec<Row> = s.table.iter().map(|&entry| Row { entry, usage: None }).collect();
        apply_pick(&mut s, &rows, Pick::Select, 1);
        let clear: Vec<bool> = s.table.iter().map(|e| e.transparent).collect();
        assert_eq!(clear, [false, true, false]);
    }

    #[test]
    fn sorting_and_ranges() {
        let mut rows = rows_of(&[Rgba::new(255, 255, 255, 255), Rgba::new(0, 0, 0, 255), Rgba::new(128, 128, 128, 255)]);
        assert_eq!(order(&rows, Sort::Luminance), [1, 2, 0]);
        rows[0].usage = Some(5);
        rows[1].usage = Some(1);
        rows[2].usage = Some(9);
        assert_eq!(order(&rows, Sort::Popularity), [2, 0, 1]);
        let mut st = TableState::default();
        let shown = order(&rows, Sort::Luminance);
        select(&mut st, &shown, 1, false, false);
        select(&mut st, &shown, 0, false, true);
        assert_eq!(st.selected, BTreeSet::from([0, 1, 2]));
        select(&mut st, &shown, 2, true, false);
        assert_eq!(st.selected, BTreeSet::from([0, 1]));
    }
}
