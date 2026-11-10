//! Text > Editor...: Fireworks MX's Text Editor. The selected text object is edited as a working
//! copy in a document of its own, with the Text tool's typing, selection and formatting, shown in
//! its own fonts, sizes and colours (or plainly, with Show Font / Show Size & Color unticked).
//! OK and Apply put it back as one History step; Cancel and Esc leave the object as it was.

use egui::{Color32, ComboBox, DragValue, Sense, Stroke, vec2};
use firecraft_doc::{Document, Text, TextRun};
use firecraft_engine::text as engine;
use firecraft_engine::{DocState, ElementRef, Tool};
use firecraft_raster::transform::Affine;

use crate::FireCraftApp;
use crate::fpui::color_picker::{self as cp, ColorChoice, Footer};
use crate::text_tool::{TextTool, apply_editing, editing_style};

/// The text area's inner margin, in points.
const MARGIN: f64 = 6.0;

pub struct TextEditor {
    /// The document and text object being edited.
    doc: u64,
    target: ElementRef,
    work: DocState,
    tool: TextTool,
    /// The drawn working copy, for (revision, view, width).
    preview: Option<((u64, bool, bool, u32), egui::TextureHandle, egui::Pos2)>,
}

/// Whether Text > Editor... has something to edit.
pub fn can_open(app: &FireCraftApp) -> bool {
    app.text_tool.editing.is_some() || app.session.active_doc().is_some_and(|d| d.can_convert_text())
}

/// Open the Text Editor on the selected text object (finishing any text being edited first).
pub fn open(app: &mut FireCraftApp) {
    let Some(d) = app.session.active_doc_mut() else {
        return;
    };
    app.text_tool.finish(d);
    let Some(r) = d
        .selection
        .iter()
        .copied()
        .find(|r| d.editable_text_at(*r).is_some())
    else {
        return;
    };
    let Some(t) = d.text_at(r).cloned() else { return };
    let mut text = Text {
        matrix: Affine::IDENTITY,
        fill: None,
        anchor: None,
        on_path: false,
        auto_width: false,
        rect: (MARGIN, MARGIN, 400.0, MARGIN + t.size),
        cached: None,
        ..t
    };
    engine::refresh(&mut text);
    let mut work = DocState::new(Document::new(4000, 4000, 72.0, None));
    let r0 = work.start_new_text(text);
    work.commit_live_edit();
    let mut tool = TextTool::default();
    tool.contained = true;
    tool.edit_all(&mut work, r0);
    app.text_editor = Some(TextEditor {
        doc: d.id,
        target: r,
        work,
        tool,
        preview: None,
    });
}

/// Put the working copy back into the document (one History step).
fn apply(app: &mut FireCraftApp, ed: &TextEditor) {
    let Some(r0) = ed.tool.editing.as_ref().map(|e| e.r) else {
        return;
    };
    let Some(w) = ed.work.text_at(r0) else { return };
    let (runs, aa, kern) = (w.runs.clone(), w.anti_alias, w.auto_kern);
    let Some(d) = app.session.docs.iter_mut().find(|d| d.id == ed.doc) else {
        return;
    };
    let target = ed.target;
    d.edit("Text Editor", |s| {
        s.change_text(target, |t| {
            t.runs.clone_from(&runs);
            t.anti_alias = aa;
            t.auto_kern = kern;
        });
    });
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

pub fn show(app: &mut FireCraftApp, ctx: &egui::Context) {
    let Some(mut ed) = app.text_editor.take() else {
        return;
    };
    let fonts = app.font_names();
    let options = app.session.options.clone();
    let (mut ok, mut cancel, mut apply_now) = (false, false, false);
    let mut open = true;
    egui::Window::new("Text Editor")
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size(vec2(560.0, 320.0))
        .pivot(egui::Align2::CENTER_CENTER)
        .default_pos(ctx.content_rect().center())
        .show(ctx, |ui| {
            let TextEditor {
                work, tool, preview, ..
            } = &mut ed;
            let Some(e) = tool.editing.as_mut() else { return };
            let Some(t) = work.text_at(e.r) else { return };
            let style = editing_style(e, t);
            let (aa, kern) = (t.anti_alias, t.auto_kern);
            let mut change: Option<Box<dyn Fn(&mut TextRun)>> = None;
            ui.horizontal(|ui| {
                if let Some(name) =
                    crate::font_picker::font_combo(ui, "editor_font", 170.0, 400.0, &style.font, &fonts)
                {
                    app.platform.load_installed_font(&name);
                    change = Some(Box::new(move |r| r.font.clone_from(&name)));
                }
                let mut size = style.size;
                if ui
                    .add(DragValue::new(&mut size).range(1.0..=1000.0).max_decimals(1))
                    .on_hover_text("Size")
                    .changed()
                {
                    change = Some(Box::new(move |r| r.size = size));
                }
                if let Some(ColorChoice::Color(c)) = cp::color_box(
                    ui,
                    "editor_color",
                    vec2(22.0, 14.0),
                    Some(style.color),
                    Footer::Plain,
                ) {
                    let c = cp::to_rgba(c);
                    change = Some(Box::new(move |r| r.color = c));
                }
                let (b, i, u) = (style.bold, style.italic, style.underline);
                if ui.selectable_label(b, "B").on_hover_text("Bold").clicked() {
                    change = Some(Box::new(move |r| r.bold = !b));
                }
                if ui.selectable_label(i, "I").on_hover_text("Italic").clicked() {
                    change = Some(Box::new(move |r| r.italic = !i));
                }
                if ui.selectable_label(u, "U").on_hover_text("Underline").clicked() {
                    change = Some(Box::new(move |r| r.underline = !u));
                }
            });
            ui.horizontal(|ui| {
                let mut tracking = style.tracking * 100.0;
                if number(ui, "Kerning:", &mut tracking, -100.0..=100.0, "%") {
                    change = Some(Box::new(move |r| r.tracking = tracking / 100.0));
                }
                let px = style.leading_px;
                let mut leading = if px { style.leading } else { style.leading * 100.0 };
                if number(
                    ui,
                    "Leading:",
                    &mut leading,
                    0.0..=1000.0,
                    if px { " px" } else { "%" },
                ) {
                    change = Some(Box::new(move |r| {
                        r.leading = if px { leading } else { leading / 100.0 }
                    }));
                }
                for (value, label) in [
                    (0u8, "Left"),
                    (1, "Center"),
                    (2, "Right"),
                    (3, "Justify"),
                    (4, "Stretch"),
                ] {
                    if ui.selectable_label(style.align == value, label).clicked() {
                        change = Some(Box::new(move |r| r.align = value));
                    }
                }
            });
            let mut object: Option<(u8, bool)> = None;
            ui.horizontal(|ui| {
                let mut scale = style.h_scale * 100.0;
                if number(ui, "Scale:", &mut scale, 1.0..=1000.0, "%") {
                    change = Some(Box::new(move |r| r.h_scale = scale / 100.0));
                }
                let mut shift = style.baseline_shift;
                if number(ui, "Baseline:", &mut shift, -500.0..=500.0, " px") {
                    change = Some(Box::new(move |r| r.baseline_shift = shift));
                }
                const MODES: [(u8, &str); 3] = [
                    (0, "Smooth Anti-Alias"),
                    (1, "Crisp Anti-Alias"),
                    (2, "Strong Anti-Alias"),
                ];
                let name = MODES
                    .iter()
                    .find(|(v, _)| *v == aa)
                    .map_or(MODES[0].1, |(_, n)| n);
                ComboBox::from_id_salt("editor_aa")
                    .width(130.0)
                    .selected_text(name)
                    .show_ui(ui, |ui| {
                        for (v, n) in MODES {
                            if ui.selectable_label(v == aa, n).clicked() {
                                object = Some((v, kern));
                            }
                        }
                    });
                let mut auto = kern;
                if ui.checkbox(&mut auto, "Auto Kern").changed() {
                    object = Some((aa, auto));
                }
            });
            if let Some(f) = change {
                apply_editing(e, work, &*f);
            }
            if let Some((aa, kern)) = object {
                let r = e.r;
                work.change_text(r, |t| {
                    t.anti_alias = aa;
                    t.auto_kern = kern;
                });
            }
            // The text area: the working copy drawn in it, edited like text on the canvas. White, or
            // dark for light text (white text would vanish); it scrolls when the text is taller.
            let height = (ui.available_height() - 34.0).max(120.0);
            let light = work.text_at(e.r).is_some_and(|t| {
                let lum = |c: firecraft_raster::Rgba| {
                    0.3 * f64::from(c.r) + 0.59 * f64::from(c.g) + 0.11 * f64::from(c.b)
                };
                let n = t.runs.len().max(1) as f64;
                t.runs.iter().map(|r| lum(r.color)).sum::<f64>() / n > 200.0
            });
            let background = if light && tool.view.size_color {
                Color32::from_gray(64)
            } else {
                Color32::WHITE
            };
            let text_height = work.text_at(e.r).map_or(0.0, |t| t.rect.3 + MARGIN) as f32;
            let area_width = ui.available_width();
            let scrolled = egui::ScrollArea::vertical()
                .max_height(height)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.allocate_exact_size(
                        vec2(area_width - 14.0, height.max(text_height)),
                        Sense::click_and_drag(),
                    )
                });
            let (rect, resp) = scrolled.inner;
            let painter = ui.painter_at(scrolled.inner_rect);
            painter.rect_filled(rect, 0.0, background);
            painter.rect_stroke(
                scrolled.inner_rect,
                0.0,
                Stroke::new(1.0, Color32::from_gray(150)),
                egui::StrokeKind::Inside,
            );
            // Wrap at the area's width.
            let width = f64::from(rect.width()) - MARGIN;
            let r = e.r;
            if work.text_at(r).is_some_and(|t| (t.rect.2 - width).abs() > 0.5) {
                work.change_text(r, |t| t.rect.2 = width.max(t.rect.0 + 10.0));
            }
            let view = tool.view;
            let key = (work.revision, view.font, view.size_color, width as u32);
            if preview.as_ref().is_none_or(|p| p.0 != key)
                && let Some(t) = work.text_at(r)
            {
                let shown = engine::display_copy(t, view.font, view.size_color);
                *preview = engine::rasterize(&shown).map(|b| {
                    let size = [b.image.width() as usize, b.image.height() as usize];
                    let pixels: Vec<u8> = b
                        .image
                        .pixels()
                        .iter()
                        .flat_map(|p| [p.r, p.g, p.b, p.a])
                        .collect();
                    let image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);
                    let tex =
                        ui.ctx()
                            .load_texture("text_editor_preview", image, egui::TextureOptions::NEAREST);
                    (key, tex, egui::pos2(b.x as f32, b.y as f32))
                });
            }
            if let Some((_, tex, at)) = preview.as_ref() {
                let min = rect.min + at.to_vec2();
                painter.image(
                    tex.id(),
                    egui::Rect::from_min_size(min, tex.size_vec2()),
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            crate::text_tool::canvas(tool, work, Tool::Text, &options, &resp, &painter, (rect.min, 1.0));
            if tool.editing.is_none() {
                // Esc: as Cancel.
                cancel = true;
            }
            ui.horizontal(|ui| {
                let mut v = tool.view;
                ui.checkbox(&mut v.font, "Show Font");
                ui.checkbox(&mut v.size_color, "Show Size & Color");
                tool.view = v;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    apply_now = ui.button("Apply").clicked();
                    cancel |= ui.button("Cancel").clicked();
                    ok = ui.button("OK").clicked();
                });
            });
        });
    if ok || apply_now {
        apply(app, &ed);
    }
    if !(ok || cancel || !open) {
        app.text_editor = Some(ed);
    }
}
