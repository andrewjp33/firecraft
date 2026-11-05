//! The Library panel (Assets): the document's symbols. Select one to preview it; double-click
//! the name to rename; drag it onto the canvas (or use the insert button) to place an instance;
//! edit, duplicate or delete it with the buttons below.

use egui::{Color32, RichText, Sense, vec2};
use firecraft_doc::SymbolKind;

use crate::icons::{self, Glyph};
use crate::{FireCraftApp, theme};

/// What a Library row carries when dragged onto the canvas.
#[derive(Clone, Copy, Debug)]
pub struct SymbolDrag {
    pub doc: u64,
    pub symbol: u32,
}

#[derive(Clone, Default)]
struct LibraryUi {
    selected: Option<u32>,
    renaming: Option<(u32, String)>,
    /// Preview texture: (document id, symbol id, document revision) it shows.
    preview: Option<((u64, u32, u64), egui::TextureHandle)>,
}

/// The symbol type's icon, drawn (the UI font has no such characters): a diamond for
/// Graphic, a play triangle for Animation, a framed square for Button.
fn kind_icon(ui: &mut egui::Ui, kind: SymbolKind) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
    let p = ui.painter();
    let c = r.center();
    let col = ui.visuals().text_color();
    match kind {
        SymbolKind::Graphic => {
            let pts = vec![
                c + egui::vec2(0.0, -5.0),
                c + egui::vec2(5.0, 0.0),
                c + egui::vec2(0.0, 5.0),
                c + egui::vec2(-5.0, 0.0),
            ];
            p.add(egui::Shape::convex_polygon(pts, col, egui::Stroke::NONE));
        }
        SymbolKind::Animation => {
            let pts = vec![
                c + egui::vec2(-4.0, -5.0),
                c + egui::vec2(5.0, 0.0),
                c + egui::vec2(-4.0, 5.0),
            ];
            p.add(egui::Shape::convex_polygon(pts, col, egui::Stroke::NONE));
        }
        SymbolKind::Button => {
            let sq = egui::Rect::from_center_size(c, egui::vec2(10.0, 10.0));
            p.rect_stroke(sq, 1.0, egui::Stroke::new(1.2, col), egui::StrokeKind::Inside);
            p.rect_filled(sq.shrink(3.0), 0.0, col);
        }
    }
    resp
}

pub fn panel(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    // In a Symbol Editor the Library is still the document's (the editor holds a copy of its
    // symbols only so nested instances render).
    let active = app
        .session
        .active_doc()
        .and_then(|d| d.symbol_link)
        .and_then(|l| app.session.docs.iter().position(|d| d.id == l.parent))
        .unwrap_or(app.session.active);
    let Some(d) = app.session.docs.get(active) else {
        ui.weak("No document");
        return;
    };
    let state_id = egui::Id::unique(("library_ui", d.id));
    let mut st: LibraryUi = ui.ctx().data(|m| m.get_temp(state_id)).unwrap_or_default();
    let (doc_id, revision) = (d.id, d.revision);
    let symbols: Vec<(u32, String, SymbolKind)> = d
        .doc
        .symbols
        .iter()
        .map(|s| (s.id, s.name.clone(), s.kind))
        .collect();
    let nines: Vec<u32> = d
        .doc
        .symbols
        .iter()
        .filter(|s| s.nine.is_some())
        .map(|s| s.id)
        .collect();
    if st.selected.is_some_and(|id| !symbols.iter().any(|s| s.0 == id)) {
        st.selected = None;
    }
    // Preview of the selected symbol.
    let preview_size = vec2(ui.available_width().min(220.0), 90.0);
    let (rect, _) = ui.allocate_exact_size(preview_size, Sense::hover());
    ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
    ui.painter().rect_stroke(
        rect,
        0.0,
        egui::Stroke::new(1.0, theme::BORDER),
        egui::StrokeKind::Inside,
    );
    if let Some(id) = st.selected {
        let key = (doc_id, id, revision);
        if st.preview.as_ref().is_none_or(|(k, _)| *k != key)
            && let Some(img) = render_preview(d, id)
        {
            let color = egui::ColorImage::from_rgba_unmultiplied(
                [img.width() as usize, img.height() as usize],
                &img.to_rgba_bytes(),
            );
            st.preview = Some((
                key,
                ui.ctx()
                    .load_texture("library_preview", color, egui::TextureOptions::LINEAR),
            ));
        }
        if let Some((_, tex)) = &st.preview {
            let s = tex.size_vec2();
            let scale = ((rect.width() - 8.0) / s.x)
                .min((rect.height() - 8.0) / s.y)
                .min(1.0);
            let r = egui::Rect::from_center_size(rect.center(), s * scale);
            ui.painter().image(
                tex.id(),
                r,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );
        }
    }
    ui.label(format!("{} items", symbols.len()));
    let mut rename: Option<(u32, String)> = None;
    let mut edit: Option<u32> = None;
    for (id, name, kind) in &symbols {
        let selected = st.selected == Some(*id);
        if st.renaming.as_ref().is_some_and(|(r, _)| r == id) {
            ui.horizontal(|ui| {
                kind_icon(ui, *kind);
                if let Some((_, text)) = st.renaming.as_mut() {
                    let r = ui.add(egui::TextEdit::singleline(text).desired_width(140.0));
                    if !r.has_focus() && !r.lost_focus() {
                        r.request_focus();
                    }
                    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        st.renaming = None;
                    } else if r.lost_focus() {
                        rename = Some((*id, text.clone()));
                        st.renaming = None;
                    }
                }
            });
            continue;
        }
        // Only the icon and name are the drag source: egui centres what's dragged on the
        // pointer, so a full-width row would show its name far left of it.
        let row = ui
            .horizontal(|ui| {
                ui.dnd_drag_source(
                    egui::Id::unique(("library_row", doc_id, *id)),
                    SymbolDrag {
                        doc: doc_id,
                        symbol: *id,
                    },
                    |ui| {
                        ui.horizontal(|ui| {
                            kind_icon(ui, *kind).on_hover_text(kind.label());
                            let text = RichText::new(name);
                            let text = if selected {
                                text.color(Color32::WHITE).background_color(theme::SELECT_BLUE)
                            } else {
                                text
                            };
                            ui.add(egui::Label::new(text).selectable(false));
                        });
                    },
                );
                ui.add_space(ui.available_width());
            })
            .response;
        let click = ui.interact(
            row.rect,
            egui::Id::unique(("library_click", doc_id, *id)),
            Sense::click(),
        );
        let click = click.on_hover_text("Drag onto the canvas to place an instance; double-click to rename");
        if click.clicked() {
            st.selected = Some(*id);
        }
        if click.double_clicked() {
            st.renaming = Some((*id, name.clone()));
        }
    }
    if symbols.is_empty() {
        ui.weak("No symbols. Select objects and press F8 (Modify > Symbol > Convert to Symbol).");
    }
    ui.separator();
    let sel = st.selected;
    let mut insert = false;
    let mut duplicate = false;
    let mut delete = false;
    let mut nine: Option<bool> = None;
    if let Some(id) = sel {
        let mut on = nines.contains(&id);
        if ui
            .checkbox(&mut on, "9-slice scaling guides")
            .on_hover_text("Scaled instances keep their corners' size (Fireworks CS); move the guides in the Symbol Editor. Fireworks MX ignores them.")
            .changed()
        {
            nine = Some(on);
        }
    }
    ui.horizontal(|ui| {
        insert = icons::glyph_button(ui, Glyph::Plus, 16.0, sel.is_some())
            .on_hover_text("Place an instance in the middle of the canvas")
            .clicked();
        if icons::glyph_button(ui, Glyph::Pencil, 16.0, sel.is_some())
            .on_hover_text("Edit Symbol")
            .clicked()
        {
            edit = sel;
        }
        duplicate = ui
            .add_enabled(sel.is_some(), egui::Button::new("Duplicate").small())
            .clicked();
        delete = icons::glyph_button(ui, Glyph::Trash, 16.0, sel.is_some())
            .on_hover_text("Delete the symbol and its instances")
            .clicked();
    });
    ui.ctx().data_mut(|m| m.insert_temp(state_id, st.clone()));
    let Some(d) = app.session.docs.get_mut(active) else {
        return;
    };
    if let Some((id, name)) = rename {
        d.rename_symbol(id, &name);
    }
    if let Some(id) = sel {
        if insert {
            let (w, h) = (d.doc.width as f64, d.doc.height as f64);
            d.insert_instance(id, w / 2.0, h / 2.0);
        }
        if duplicate && let Some(new) = d.duplicate_symbol(id) {
            st.selected = Some(new);
            ui.ctx().data_mut(|m| m.insert_temp(state_id, st));
        }
        if delete {
            d.delete_symbol(id);
        }
        if let Some(on) = nine {
            d.set_symbol_nine(id, on);
        }
    }
    if let Some(id) = edit {
        app.session.edit_symbol(active, id);
    }
}

/// The symbol's first frame, at most 200 px across.
fn render_preview(d: &firecraft_engine::DocState, id: u32) -> Option<firecraft_raster::RgbaImage> {
    let (mut doc, _) = d.symbol_document(id)?;
    doc.canvas = Some(firecraft_raster::Rgba::WHITE);
    let img = doc.render(0);
    let scale = (200.0 / img.width().max(img.height()) as f64).min(1.0);
    if scale >= 1.0 {
        return Some(img);
    }
    let m = firecraft_raster::transform::Affine {
        a: scale,
        d: scale,
        ..firecraft_raster::transform::Affine::IDENTITY
    };
    firecraft_raster::transform::transform_image(&img, 0, 0, &m).map(|(i, _, _)| i)
}
