//! Edit Stroke (Stroke Options...): Fireworks' brush editor with its Options, Shape and
//! Sensitivity tabs and a sample stroke drawn with the edited brush. After FirePixel's
//! `dialogs/brush_edit.rs` (Apache-2.0): the same fields, ranges and rules (spacing and flow
//! only for natural brushes; tip spacing and variation only with several tips; "Ink amount" is
//! the concentration; the dialog never changes the brush type), laid out with egui.
//!
//! It edits the stroke of the selected paths (one undo step on OK) or, with nothing selected or
//! from the tool options, the brush of new strokes.

use egui::{Color32, ComboBox, DragValue, Grid, Rect, vec2};
use firecraft_doc::brush_presets::BrushFields;
use firecraft_doc::{BezierPoint, Contour, ElementKind, Path, Stroke};
use firecraft_fwpng::Node;
use firecraft_raster::{Rgba, RgbaImage};

use crate::FireCraftApp;

const SOURCES: [(u8, &str); 5] = [
    (b'P', "Pressure:"),
    (b'S', "Speed:"),
    (b'H', "Horizontal:"),
    (b'V', "Vertical:"),
    (b'R', "Random:"),
];
/// "Stroke property" list and its target (Fireworks: "Ink amount" is the opacity target `O`).
const PROPERTIES: [(&str, u8); 7] = [
    ("Size", b'Z'),
    ("Angle", b'A'),
    ("Ink amount", b'O'),
    ("Scatter", b'R'),
    ("Hue", b'H'),
    ("Lightness", b'L'),
    ("Saturation", b'S'),
];
/// Edge effects in Fireworks' list order and their `BEFi` (alpha remap) values.
const EFFECTS: [(&str, i32); 6] = [
    ("None", 0),
    ("Harsh Wet", 2),
    ("White Neon", 1),
    ("Wavy Gravy", 4),
    ("Smooth Neon", 3),
    ("White Neon Off-Center", 5),
];
const VARIATIONS: [&str; 5] = ["Random", "Uniform", "Complementary", "Hue", "Shadow"];
const DASHES: [&str; 4] = ["None", "Basic Dash", "Double Dash", "Triple Dash"];

#[derive(Clone, Debug, PartialEq)]
pub struct EditStroke {
    /// New strokes (`true`) or the selected paths' stroke.
    tool: bool,
    /// The brush node the fields came from (fields FireCraft doesn't edit stay).
    base: Node,
    brush: BrushFields,
    original: BrushFields,
    tab: usize,
    property: usize,
    /// The stroke's texture (for the sample stroke).
    texture: Option<firecraft_doc::Texture>,
}

/// Open Edit Stroke for the selection (`tool` false; with nothing selected, for new strokes)
/// or for new strokes.
pub fn open(app: &mut FireCraftApp, tool: bool) {
    let selected = app.session.active_doc().and_then(|d| {
        d.selection
            .iter()
            .find_map(|r| match d.element(*r).map(|e| &e.kind) {
                Some(ElementKind::Path(Path { stroke: Some(s), .. })) => Some(s.clone()),
                _ => None,
            })
    });
    let nothing_selected = app.session.active_doc().is_none_or(|d| d.selection.is_empty());
    let (tool, stroke) = match (tool, selected) {
        (false, Some(s)) => (false, s),
        (false, None) if !nothing_selected => {
            app.dialogs.message = Some((
                "Edit Stroke".into(),
                "Please select an object with a stroke first.".into(),
            ));
            return;
        }
        _ => {
            let o = &app.session.options;
            (true, o.new_stroke(o.stroke.unwrap_or(Rgba::BLACK)))
        }
    };
    let mut base = stroke
        .brush
        .clone()
        .unwrap_or_else(|| firecraft_doc::stroke_fw::pencil_preset(&stroke));
    firecraft_doc::stroke_fw::sync_preset(&mut base, &stroke);
    let brush = BrushFields::from_node(&base);
    app.dialogs.edit_stroke = Some(EditStroke {
        texture: stroke.texture.clone(),
        tool,
        base,
        original: brush.clone(),
        brush,
        tab: 0,
        property: 0,
    });
}

fn pct(v: i32) -> f64 {
    (v as f64 / 10.0).round()
}

fn permille(v: f64) -> i32 {
    (v * 10.0).round() as i32
}

/// A percent field for a per-mille value; whether it changed.
fn percent(ui: &mut egui::Ui, v: &mut i32, max: f64, enabled: bool) -> bool {
    let mut p = pct(*v);
    let changed = ui
        .add_enabled(
            enabled,
            DragValue::new(&mut p)
                .range(0.0..=max)
                .max_decimals(0)
                .suffix("%"),
        )
        .changed();
    if changed {
        *v = permille(p);
    }
    changed
}

fn number(ui: &mut egui::Ui, v: &mut i32, range: std::ops::RangeInclusive<i32>, enabled: bool) {
    ui.add_enabled(enabled, DragValue::new(v).range(range));
}

fn combo(ui: &mut egui::Ui, id: &str, items: &[&str], cur: usize, enabled: bool) -> Option<usize> {
    let mut out = None;
    ui.add_enabled_ui(enabled, |ui| {
        ComboBox::from_id_salt(("edit_stroke", id))
            .width(150.0)
            .selected_text(items.get(cur).copied().unwrap_or_default())
            .show_ui(ui, |ui| {
                for (i, l) in items.iter().enumerate() {
                    if ui.selectable_label(i == cur, *l).clicked() {
                        out = Some(i);
                    }
                }
            });
    });
    out
}

fn options_page(ui: &mut egui::Ui, b: &mut BrushFields) {
    // As in Fireworks: spacing and flow only for natural brushes.
    let natural = b.natural;
    Grid::new("edit_stroke_options")
        .num_columns(2)
        .spacing(vec2(8.0, 6.0))
        .show(ui, |ui| {
            ui.label("Ink amount:");
            percent(ui, &mut b.concentration, 100.0, true);
            ui.end_row();
            ui.label("Spacing:");
            percent(ui, &mut b.spacing, 1000.0, natural);
            ui.end_row();
            ui.label("Flow rate:");
            percent(ui, &mut b.flow_rate, 100.0, natural);
            ui.end_row();
            ui.label("");
            let mut build = b.feedback == 1;
            if ui.checkbox(&mut build, "Build-up").changed() {
                b.feedback = i32::from(build);
            }
            ui.end_row();
            ui.label("Texture:");
            percent(ui, &mut b.texture_blend, 100.0, true);
            ui.end_row();
            ui.label("Edge texture:");
            percent(ui, &mut b.texture_edge, 100.0, true);
            ui.end_row();
            ui.label("Edge effect:");
            let names: Vec<&str> = EFFECTS.iter().map(|e| e.0).collect();
            let cur = EFFECTS.iter().position(|e| e.1 == b.alpha_remap).unwrap_or(0);
            if let Some(i) = combo(ui, "effect", &names, cur, true) {
                b.alpha_remap = EFFECTS[i].1;
            }
            ui.end_row();
            ui.label("Tips:");
            b.tip_count = b.tip_count.max(1);
            number(ui, &mut b.tip_count, 1..=32, true);
            ui.end_row();
            let multi = b.tip_count > 1;
            ui.label("Tip spacing:");
            percent(ui, &mut b.tip_spacing, 1000.0, multi);
            ui.end_row();
            ui.label("Variation:");
            if let Some(i) = combo(
                ui,
                "variation",
                &VARIATIONS,
                b.tip_coloring_mode.clamp(0, 4) as usize,
                multi,
            ) {
                b.tip_coloring_mode = i as i32;
            }
            ui.end_row();
            ui.label("Dashed:");
            if let Some(i) = combo(ui, "dashes", &DASHES, b.dash_count.clamp(0, 3) as usize, true) {
                b.dash_count = i as i32;
                for k in 0..i {
                    if b.dash_on[k] == 0 {
                        b.dash_on[k] = 10;
                    }
                    if b.dash_off[k] == 0 {
                        b.dash_off[k] = 5;
                    }
                }
            }
            ui.end_row();
            for k in 0..3usize {
                let en = (k as i32) < b.dash_count;
                ui.label("");
                ui.horizontal(|ui| {
                    ui.add_enabled(en, egui::Label::new("On:"));
                    number(ui, &mut b.dash_on[k], 0..=1000, en);
                    ui.add_enabled(en, egui::Label::new("Off:"));
                    number(ui, &mut b.dash_off[k], 0..=1000, en);
                });
                ui.end_row();
            }
        });
}

fn shape_page(
    ui: &mut egui::Ui,
    base: &Node,
    b: &mut BrushFields,
    color: Rgba,
    tex: Option<&firecraft_doc::Texture>,
) {
    preview(ui, base, b, color, tex, vec2(80.0, 80.0), true);
    let mut square = !b.round;
    if ui.checkbox(&mut square, "Square").changed() {
        b.round = !square;
    }
    Grid::new("edit_stroke_shape")
        .num_columns(2)
        .spacing(vec2(8.0, 6.0))
        .show(ui, |ui| {
            ui.label("Size:");
            let mut d = b.diameter as f64;
            if ui
                .add(
                    egui::Slider::new(&mut d, 1.0..=100.0)
                        .max_decimals(0)
                        .clamping(egui::SliderClamping::Never),
                )
                .changed()
            {
                b.diameter = (d.round() as i32).clamp(1, 1000);
            }
            ui.end_row();
            ui.label("Edge:");
            let mut e = pct(b.softness);
            if ui
                .add(egui::Slider::new(&mut e, 0.0..=100.0).max_decimals(0))
                .changed()
            {
                b.softness = permille(e);
            }
            ui.end_row();
            ui.label("Aspect:");
            let mut a = pct(b.aspect).max(1.0);
            if ui
                .add(egui::Slider::new(&mut a, 1.0..=100.0).max_decimals(0))
                .changed()
            {
                b.aspect = permille(a);
            }
            ui.end_row();
            ui.label("Angle:");
            let mut ang = if b.angle == 0 { 360 } else { b.angle };
            if ui
                .add(DragValue::new(&mut ang).range(0..=360).suffix("°"))
                .changed()
            {
                b.angle = ang.rem_euclid(360);
            }
            ui.end_row();
        });
}

fn sensitivity_page(ui: &mut egui::Ui, b: &mut BrushFields, property: &mut usize) {
    ui.label("Stroke property:");
    let names: Vec<&str> = PROPERTIES.iter().map(|p| p.0).collect();
    if let Some(i) = combo(
        ui,
        "property",
        &names,
        (*property).min(PROPERTIES.len() - 1),
        true,
    ) {
        *property = i;
    }
    let target = PROPERTIES[(*property).min(PROPERTIES.len() - 1)].1;
    ui.add_space(6.0);
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label("Affected by:");
        Grid::new("edit_stroke_sens")
            .num_columns(2)
            .spacing(vec2(8.0, 6.0))
            .show(ui, |ui| {
                for (src, label) in SOURCES {
                    ui.label(label);
                    let mut v = b.sensitivity(src, target);
                    if percent(ui, &mut v, 100.0, true) {
                        b.set_sensitivity(src, target, v);
                    }
                    ui.end_row();
                }
            });
    });
}

/// A sample stroke (an S curve), or one dab (`dot`), drawn with the brush by the stroke engine.
fn preview(
    ui: &mut egui::Ui,
    base: &Node,
    b: &BrushFields,
    color: Rgba,
    tex: Option<&firecraft_doc::Texture>,
    size: egui::Vec2,
    dot: bool,
) {
    let (r, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    ui.painter().rect_filled(r, 0.0, Color32::WHITE);
    ui.painter().rect_stroke(
        r,
        0.0,
        egui::Stroke::new(1.0, Color32::from_gray(150)),
        egui::StrokeKind::Inside,
    );
    let inner = r.shrink(1.0);
    let (w, h) = (inner.width().max(1.0) as u32, inner.height().max(1.0) as u32);
    let mut bb = b.clone();
    // Very large tips are kept to the frame.
    bb.diameter = bb.diameter.clamp(
        1,
        if dot {
            (h as i32 - 2).max(1)
        } else {
            (h as i32 / 2).max(1)
        },
    );
    let key = egui::Id::unique((
        "edit_stroke_preview",
        dot,
        w,
        h,
        color.r,
        color.g,
        color.b,
        format!("{bb:?}"),
        tex.map(|t| t.name.clone()),
    ));
    let tex = ui
        .ctx()
        .data(|d| d.get_temp::<egui::TextureHandle>(key))
        .or_else(|| {
            let pts: Vec<BezierPoint> = if dot {
                vec![BezierPoint::corner(w as f64 / 2.0, h as f64 / 2.0)]
            } else {
                (0..=40)
                    .map(|i| {
                        let t = i as f64 / 40.0;
                        let x = 10.0 + (w as f64 - 20.0) * t;
                        let y = h as f64 / 2.0 + (h as f64 / 4.0) * (t * std::f64::consts::TAU).sin();
                        BezierPoint::corner(x, y)
                    })
                    .collect()
            };
            let path = Path {
                contours: vec![Contour {
                    points: pts,
                    closed: false,
                }],
                ..Path::default()
            };
            let node = bb.to_node(base);
            let stroke = Stroke {
                color: Rgba { a: 255, ..color },
                category: bb.category.clone(),
                name: bb.name.clone(),
                diameter: bb.diameter as f64,
                softness: (bb.softness.clamp(0, 1000) / 10) as u8,
                anti_alias: bb.antialiased,
                brush: Some(node),
                placement: 1,
                texture: tex.cloned(),
            };
            let mut img = RgbaImage::new(w, h, Rgba::TRANSPARENT);
            firecraft_doc::stroke_fw::draw(&mut img, &path, &stroke, None, (0.0, 0.0));
            let px: Vec<u8> = img.pixels().iter().flat_map(|p| [p.r, p.g, p.b, p.a]).collect();
            let ci = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px);
            let tex = ui
                .ctx()
                .load_texture("edit_stroke_preview", ci, egui::TextureOptions::NEAREST);
            ui.ctx().data_mut(|d| d.insert_temp(key, tex.clone()));
            Some(tex)
        });
    if let Some(tex) = tex {
        ui.painter().image(
            tex.id(),
            inner,
            Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}

pub fn show(app: &mut FireCraftApp, ctx: &egui::Context) {
    let Some(mut d) = app.dialogs.edit_stroke.clone() else {
        return;
    };
    let color = app.session.options.stroke.unwrap_or(Rgba::BLACK);
    let mut result = None;
    let mut open = true;
    egui::Window::new("Edit Stroke")
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_width(330.0);
            ui.horizontal(|ui| {
                for (i, t) in ["Options", "Shape", "Sensitivity"].iter().enumerate() {
                    if ui.selectable_label(d.tab == i, *t).clicked() {
                        d.tab = i;
                    }
                }
            });
            ui.separator();
            match d.tab {
                0 => options_page(ui, &mut d.brush),
                1 => shape_page(ui, &d.base, &mut d.brush, color, d.texture.as_ref()),
                _ => sensitivity_page(ui, &mut d.brush, &mut d.property),
            }
            ui.add_space(6.0);
            // The sample stroke with the edited brush (every tab).
            preview(
                ui,
                &d.base,
                &d.brush,
                color,
                d.texture.as_ref(),
                vec2(330.0, 60.0),
                false,
            );
            ui.add_space(6.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Cancel").clicked() {
                    result = Some(false);
                }
                if ui.button("OK").clicked() {
                    result = Some(true);
                }
            });
        });
    if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        result = Some(false);
    }
    match result {
        Some(true) => {
            app.dialogs.edit_stroke = None;
            if d.brush != d.original {
                apply(app, &d);
            }
        }
        Some(false) => app.dialogs.edit_stroke = None,
        None => app.dialogs.edit_stroke = Some(d),
    }
}

fn apply(app: &mut FireCraftApp, d: &EditStroke) {
    let node = d.brush.to_node(&d.base);
    let b = d.brush.clone();
    if d.tool {
        let o = &mut app.session.options;
        o.stroke_brush = Some(node);
        o.stroke_size = b.diameter.clamp(1, 1000) as u32;
        o.stroke_softness = (b.softness.clamp(0, 1000) / 10) as u8;
        return;
    }
    if let Some(doc) = app.session.active_doc_mut() {
        doc.edit_stroke("Edit Stroke", false, move |st| {
            st.brush = Some(node.clone());
            st.diameter = b.diameter.clamp(1, 1000) as f64;
            st.softness = (b.softness.clamp(0, 1000) / 10) as u8;
            st.anti_alias = b.antialiased;
        });
    }
}
