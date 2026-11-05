//! Fireworks CS's stroke controls in the Property inspector, after FirePixel's `stroke_panel`
//! (`fp-ui/src/inspector/obj_quads.rs`, Apache-2.0): the colour box with the stroke footer
//! (placement for a selection), the tip size, the stroke category menu (None, the categories
//! with their presets, Stroke Options...), the placement buttons, the edge preview and the edge
//! softness. For the selected paths, or for the tools' new strokes.

use egui::{Color32, Rect, Sense, Stroke as Line, StrokeKind, vec2};
use firecraft_doc::brush_presets::{self, BRUSH_PRESETS};
use firecraft_doc::{ElementKind, Stroke};

use crate::FireCraftApp;
use crate::fpui::color_picker::{self as cp, ColorChoice, Footer};
use crate::fpui::{fields, glyphs::Glyph};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The selected paths.
    Selection,
    /// New strokes (Tools panel / shape tool options).
    Tool,
}

/// What the controls show.
struct Shown {
    has: bool,
    color: Option<firecraft_raster::Rgba>,
    size: f64,
    softness: u8,
    placement: u8,
    label: String,
    preset: Option<usize>,
}

fn shown(app: &FireCraftApp, target: Target) -> Option<Shown> {
    match target {
        Target::Tool => {
            let o = &app.session.options;
            let p = BRUSH_PRESETS.get(o.stroke_preset);
            Some(Shown {
                has: o.stroke.is_some(),
                color: o.stroke,
                size: o.stroke_size as f64,
                softness: o.stroke_softness,
                placement: 1,
                label: if o.stroke.is_some() {
                    p.map(brush_presets::label).unwrap_or_default()
                } else {
                    "None".into()
                },
                preset: Some(o.stroke_preset),
            })
        }
        Target::Selection => {
            let d = app.session.active_doc()?;
            let path = d
                .selection
                .iter()
                .find_map(|r| match d.element(*r).map(|e| &e.kind) {
                    Some(ElementKind::Path(p)) => Some(p),
                    _ => None,
                })?;
            let s: Option<&Stroke> = path.stroke.as_ref();
            Some(Shown {
                has: s.is_some(),
                color: s.map(|s| s.color),
                size: s.map_or(1.0, |s| s.diameter),
                softness: s.map_or(0, |s| s.softness),
                placement: s.map_or(1, |s| s.placement),
                label: match s {
                    Some(s) => s
                        .brush
                        .as_ref()
                        .map(brush_presets::node_label)
                        .unwrap_or_else(|| s.name.trim_start_matches("bn_").to_owned()),
                    None => "None".into(),
                },
                preset: s.and_then(|s| brush_presets::find(&s.category, &s.name)),
            })
        }
    }
}

/// The tip preview beside "Edge" (FirePixel's `paint_edge_preview`).
fn paint_edge_preview(p: &egui::Painter, pv: Rect, col: Color32, size: f64, soft: u8) {
    let full = (pv.height() / 2.0 - 2.0).max(1.0);
    let r0 = (size.max(1.0) as f32 / 2.0).clamp(0.5, full);
    let steps = if r0 < 2.0 { 1 } else { 4 };
    for k in 0..steps {
        let f = 1.0 - k as f32 / steps as f32;
        let a = if soft == 0 {
            255
        } else {
            (255.0 * (1.0 - f * soft as f32 / 120.0)).clamp(30.0, 255.0) as u8
        };
        p.circle_filled(
            pv.center(),
            r0 * f,
            Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), a),
        );
    }
}

pub fn show(app: &mut FireCraftApp, ui: &mut egui::Ui, target: Target) {
    let Some(s) = shown(app, target) else { return };
    let sel = target == Target::Selection;
    let id = egui::Id::unique(("pi-stroke", sel));
    ui.vertical(|ui| {
        // Row 1: colour, size, category.
        ui.horizontal(|ui| {
            crate::icons::glyph(ui, crate::icons::Glyph::Pencil, 16.0);
            let footer = Footer::Stroke {
                placement: (sel && s.has).then_some(s.placement),
                enabled: sel,
            };
            match cp::color_box(ui, ("pi-stroke-color", sel), vec2(22.0, 14.0), s.color, footer) {
                Some(ColorChoice::Color(c)) => set_color(app, target, Some(cp::to_rgba(c))),
                Some(ColorChoice::None) => set_color(app, target, None),
                Some(ColorChoice::Placement(k)) => set_placement(app, k),
                _ => {}
            }
            let mut size = s.size.round();
            let resp = ui.add_enabled(
                s.has,
                egui::DragValue::new(&mut size)
                    .range(1.0..=100.0)
                    .max_decimals(0)
                    .suffix(" px"),
            );
            if resp.changed() {
                match target {
                    Target::Tool => app.session.options.stroke_size = size as u32,
                    Target::Selection => edit(app, "Stroke Size", &resp, move |st| st.diameter = size),
                }
            } else if resp.drag_started() {
                begin(app, target, "Stroke Size");
            }
            category_menu(app, ui, target, &s);
        });
        // Row 2: placement, edge preview, softness.
        ui.horizontal(|ui| {
            if sel {
                for (g, k, tip) in [
                    (Glyph::StrokeInside, 0u8, "Stroke inside"),
                    (Glyph::StrokeCenter, 1, "Stroke centered"),
                    (Glyph::StrokeOutside, 2, "Stroke outside"),
                ] {
                    let (r, _) = ui.allocate_exact_size(vec2(22.0, 18.0), Sense::hover());
                    if fields::toggle(ui, id.with(("place", k)), r, g, s.placement == k, s.has, tip) {
                        set_placement(app, k);
                    }
                }
            }
            ui.add_enabled(s.has, egui::Label::new("Edge:"));
            let (pv, _) = ui.allocate_exact_size(vec2(22.0, 18.0), Sense::hover());
            ui.painter().rect_filled(pv, 0.0, Color32::WHITE);
            ui.painter().rect_stroke(
                pv,
                0.0,
                Line::new(1.0, Color32::from_gray(140)),
                StrokeKind::Inside,
            );
            if s.has {
                let c = s.color.unwrap_or(firecraft_raster::Rgba::BLACK);
                paint_edge_preview(
                    ui.painter(),
                    pv,
                    Color32::from_rgb(c.r, c.g, c.b),
                    s.size,
                    s.softness,
                );
            }
            let mut soft = s.softness as f64;
            let resp = ui.add_enabled(
                s.has,
                egui::DragValue::new(&mut soft).range(0.0..=100.0).max_decimals(0),
            );
            if resp.changed() {
                let v = soft.round().clamp(0.0, 100.0) as u8;
                match target {
                    Target::Tool => app.session.options.stroke_softness = v,
                    Target::Selection => edit(app, "Stroke Edge", &resp, move |st| st.softness = v),
                }
            } else if resp.drag_started() {
                begin(app, target, "Stroke Edge");
            }
        });
    });
}

/// Start a live edit for a dragged value (one undo step for the drag).
fn begin(app: &mut FireCraftApp, target: Target, label: &str) {
    if target == Target::Selection
        && let Some(d) = app.session.active_doc_mut()
    {
        d.begin_live_edit(label);
    }
}

/// Change the selected strokes: live while the value is dragged, else one undo step.
fn edit(app: &mut FireCraftApp, label: &str, resp: &egui::Response, f: impl Fn(&mut Stroke)) {
    let dragging = resp.dragged();
    if let Some(d) = app.session.active_doc_mut() {
        if resp.drag_started() {
            d.begin_live_edit(label);
        }
        d.edit_stroke(label, dragging, f);
    }
}

fn set_color(app: &mut FireCraftApp, target: Target, c: Option<firecraft_raster::Rgba>) {
    match target {
        Target::Tool => app.session.options.stroke = c,
        Target::Selection => {
            if let Some(d) = app.session.active_doc_mut() {
                d.set_stroke_color(c);
            }
        }
    }
}

fn set_placement(app: &mut FireCraftApp, k: u8) {
    if let Some(d) = app.session.active_doc_mut() {
        d.edit_stroke("Stroke Position", false, move |st| st.placement = k.min(2));
    }
}

/// The stroke category menu: None, each category with its presets, Stroke Options...
fn category_menu(app: &mut FireCraftApp, ui: &mut egui::Ui, target: Target, s: &Shown) {
    let mut pick: Option<Option<usize>> = None;
    let mut pick_options = false;
    let button = egui::Button::new(format!("{}     ", s.label)).min_size(vec2(130.0, 18.0));
    let (resp, _) = egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
        if ui.selectable_label(!s.has, "None").clicked() {
            pick = Some(None);
            ui.close();
        }
        ui.separator();
        for &(cat, cat_label) in brush_presets::CATEGORY_LABELS {
            ui.menu_button(cat_label, |ui| {
                for (i, p) in BRUSH_PRESETS
                    .iter()
                    .enumerate()
                    .filter(|(_, p)| p.category == cat)
                {
                    let on = s.has && s.preset == Some(i);
                    if ui.selectable_label(on, brush_presets::label(p)).clicked() {
                        pick = Some(Some(i));
                        ui.close();
                    }
                }
            });
        }
        ui.separator();
        if ui.button("Stroke Options...").clicked() {
            pick_options = true;
            ui.close();
        }
    });
    // The drop-down arrow (FirePixel's chevron glyph).
    let r = resp.rect;
    let arrow = Rect::from_center_size(egui::pos2(r.right() - 10.0, r.center().y), vec2(9.0, 9.0));
    crate::fpui::glyphs::chevron_down(ui.painter(), arrow, Color32::from_gray(40));
    if pick_options {
        crate::stroke_dialog::open(app, target == Target::Tool);
    }
    let Some(choice) = pick else { return };
    match (target, choice) {
        (Target::Tool, None) => app.session.options.stroke = None,
        (Target::Tool, Some(i)) => {
            let o = &mut app.session.options;
            if o.stroke.is_none() {
                o.stroke = Some(firecraft_raster::Rgba::BLACK);
                o.stroke_size = BRUSH_PRESETS.get(i).map_or(1, |p| p.diameter.max(1) as u32);
            }
            o.stroke_preset = i;
            o.stroke_brush = None;
            o.stroke_softness = BRUSH_PRESETS
                .get(i)
                .map_or(0, |p| (p.softness / 10).clamp(0, 100) as u8);
        }
        (Target::Selection, None) => set_color(app, target, None),
        (Target::Selection, Some(i)) => {
            let opts = app.session.options.clone();
            if let Some(d) = app.session.active_doc_mut() {
                d.set_stroke_preset(i, &opts);
            }
        }
    }
}
