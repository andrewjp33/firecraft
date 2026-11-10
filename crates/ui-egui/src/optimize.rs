//! The Optimize panel: the active document's export format and its options (Fireworks MX's
//! Optimize panel), with FireCraft's WebP and JPEG XL.

use egui::{ComboBox, DragValue, Slider, Ui};
use firecraft_doc::export::PRESETS;
use firecraft_doc::{ExportFormat, ExportSettings, PaletteKind, Transparency};
use firecraft_raster::Rgba;

use crate::FireCraftApp;

pub(crate) fn panel(app: &mut FireCraftApp, ui: &mut Ui) {
    let Some(d) = app.session.active_doc_mut() else {
        ui.weak("No document");
        return;
    };
    let id = d.id;
    let before = d.doc.export.clone();
    let mut s = before.clone();
    settings(ui, &mut s);
    if s != before {
        d.doc.export = s.clone();
        d.dirty = true;
    }
    ui.add_space(4.0);
    // The file size, as the Preview tab shows the export.
    let view = app.views.get(&id);
    match view.filter(|v| v.preview).and_then(|v| v.optimized.as_ref()) {
        Some(o) => match &o.result {
            Ok((bytes, colors)) => {
                let mut line = format!("{}  {}", s.format.label(), crate::canvas::file_size(*bytes));
                if s.format.indexed() {
                    line += &format!("  {colors} colours");
                }
                ui.label(line);
            }
            Err(e) => {
                ui.colored_label(egui::Color32::from_rgb(170, 30, 30), e);
            }
        },
        None => {
            ui.weak("Preview shows the export and its file size.");
        }
    }
}

fn settings(ui: &mut Ui, s: &mut ExportSettings) {
    ui.horizontal(|ui| {
        ui.label("Settings:");
        let current = s.preset_name().unwrap_or("(custom)");
        ComboBox::from_id_salt("opt_preset")
            .selected_text(current)
            .width(170.0)
            .show_ui(ui, |ui| {
                for name in PRESETS {
                    if ui.selectable_label(current == name, name).clicked()
                        && let Some(p) = ExportSettings::preset(name)
                    {
                        let raw = s.raw.take();
                        *s = ExportSettings { raw, ..p };
                    }
                }
            });
    });
    ui.horizontal(|ui| {
        ComboBox::from_id_salt("opt_format")
            .selected_text(s.format.label())
            .width(110.0)
            .show_ui(ui, |ui| {
                for f in ExportFormat::ALL {
                    let r = ui.add_enabled(
                        firecraft_io::export::available(f),
                        egui::Button::selectable(s.format == f, f.label()),
                    );
                    let r = if firecraft_io::export::available(f) {
                        r
                    } else {
                        r.on_disabled_hover_text("Not available in the browser version yet")
                    };
                    if r.clicked() {
                        s.format = f;
                    }
                }
            });
        matte(ui, s);
    });
    match s.format {
        ExportFormat::Gif | ExportFormat::AnimatedGif | ExportFormat::Png8 => indexed(ui, s),
        ExportFormat::Jpeg => jpeg(ui, s),
        ExportFormat::Png24 | ExportFormat::Png32 => {}
        ExportFormat::WebP | ExportFormat::JpegXl => modern(ui, s),
    }
}

fn matte(ui: &mut Ui, s: &mut ExportSettings) {
    let mut on = s.matte.is_some();
    if ui
        .checkbox(&mut on, "Matte")
        .on_hover_text("Colour under semi-transparent edges where the format can't keep them (off: the canvas colour, or white)")
        .changed()
    {
        s.matte = on.then_some(Rgba::new(255, 255, 255, 255));
    }
    if let Some(m) = &mut s.matte {
        let mut c = [m.r, m.g, m.b];
        if ui.color_edit_button_srgb(&mut c).changed() {
            *m = Rgba::new(c[0], c[1], c[2], 255);
        }
    }
}

fn indexed(ui: &mut Ui, s: &mut ExportSettings) {
    ui.horizontal(|ui| {
        ComboBox::from_id_salt("opt_palette")
            .selected_text(s.palette.label())
            .width(130.0)
            .show_ui(ui, |ui| {
                for p in PaletteKind::ALL {
                    ui.selectable_value(&mut s.palette, p, p.label());
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Colours:");
        let fixed = s.palette == PaletteKind::BlackWhite;
        ComboBox::from_id_salt("opt_colors")
            .selected_text(if fixed { "2".to_owned() } else { s.colors.to_string() })
            .width(56.0)
            .show_ui(ui, |ui| {
                for n in [2u16, 4, 8, 16, 32, 64, 128, 256] {
                    ui.selectable_value(&mut s.colors, n, n.to_string());
                }
            });
        ui.add_enabled(!fixed, DragValue::new(&mut s.colors).range(2..=256));
    });
    ui.horizontal(|ui| {
        ui.label("Dither:");
        ui.add(DragValue::new(&mut s.dither).range(0..=100).suffix("%"));
    });
    if s.palette == PaletteKind::WebAdaptive {
        ui.horizontal(|ui| {
            ui.label("WebSnap tolerance:")
                .on_hover_text("Palette colours this close to a Web 216 colour snap to it");
            ui.add(DragValue::new(&mut s.websnap_tolerance).range(0..=50));
        });
    }
    ComboBox::from_id_salt("opt_transparency")
        .selected_text(s.transparency.label())
        .width(150.0)
        .show_ui(ui, |ui| {
            for t in Transparency::ALL {
                ui.selectable_value(&mut s.transparency, t, t.label());
            }
        });
    if s.format != ExportFormat::Png8 {
        ui.checkbox(&mut s.interlaced, "Interlaced");
    }
    if s.format == ExportFormat::AnimatedGif {
        ui.horizontal(|ui| {
            ui.label("Loops:");
            ui.add(DragValue::new(&mut s.loops).range(0..=1000))
                .on_hover_text("Times the animation plays; 0 = forever");
            if s.loops == 0 {
                ui.weak("forever");
            }
        });
    }
}

fn jpeg(ui: &mut Ui, s: &mut ExportSettings) {
    ui.horizontal(|ui| {
        ui.label("Quality:");
        ui.add(Slider::new(&mut s.jpeg_quality, 0..=100));
    });
    ui.horizontal(|ui| {
        ui.label("Smoothing:");
        ComboBox::from_id_salt("opt_smoothing")
            .selected_text(if s.jpeg_smoothing == 0 {
                "No Smoothing".to_owned()
            } else {
                s.jpeg_smoothing.to_string()
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut s.jpeg_smoothing, 0, "No Smoothing");
                for n in 1..=8u8 {
                    ui.selectable_value(&mut s.jpeg_smoothing, n, n.to_string());
                }
            });
    });
    ui.checkbox(&mut s.progressive, "Progressive JPEG");
    ui.checkbox(&mut s.jpeg_full_color, "Full-resolution colour")
        .on_hover_text("No chroma subsampling (as Fireworks MX writes JPEGs); off: smaller 4:2:0 files");
}

fn modern(ui: &mut Ui, s: &mut ExportSettings) {
    ui.checkbox(&mut s.lossless, "Lossless");
    ui.add_enabled_ui(!s.lossless, |ui| {
        ui.horizontal(|ui| {
            ui.label("Quality:");
            ui.add(Slider::new(&mut s.quality, 0..=100));
        });
        ui.horizontal(|ui| {
            ui.label("Alpha Quality:")
                .on_hover_text("Quality of the transparency (100 keeps it exact)");
            ui.add(Slider::new(&mut s.alpha_quality, 0..=100));
        });
    });
}
