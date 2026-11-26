//! The Optimize panel: the active document's export format and its options (Fireworks MX's
//! Optimize panel), with FireCraft's WebP and JPEG XL, and for GIF and PNG 8 the color table.

use egui::{DragValue, Slider, Ui};
use firecraft_doc::export::PRESETS;
use firecraft_doc::{ExportFormat, ExportSettings, PaletteKind, Transparency};
use firecraft_raster::Rgba;

use crate::FireCraftApp;

mod table;

pub(crate) use table::{TableState, highlight_color, shows};

pub(crate) fn panel(app: &mut FireCraftApp, ui: &mut Ui) {
    table::receive_palette(app);
    let Some(d) = app.session.active_doc_mut() else {
        ui.weak("No document");
        return;
    };
    let id = d.id;
    let before = d.doc.export.clone();
    let mut s = before.clone();
    let load = settings(ui, &mut s);
    // Another palette keeps only the table's locked colors (Fireworks MX).
    if s.palette != PaletteKind::Custom && before.palette == PaletteKind::Custom {
        s.table.retain(|e| e.locked);
    }
    if s != before {
        d.doc.export = s.clone();
        d.dirty = true;
    }
    if load {
        table::load_palette(app);
    }
    if s.format.indexed() {
        ui.add_space(4.0);
        table::color_table(app, ui, id);
    }
    // Last, so the table doesn't move between Original and Preview: the file size, as the
    // Preview tab shows the export.
    ui.add_space(4.0);
    let view = app.views.get(&id);
    match view.filter(|v| v.preview).and_then(|v| v.optimized.as_ref()) {
        Some(o) => match &o.result {
            Ok((bytes, colors)) => {
                let mut line = format!("{}  {}", s.format.label(), crate::canvas::file_size(*bytes));
                if s.format.indexed() {
                    line += &format!("  {colors} colors");
                }
                ui.label(line);
                if s.format.png() && s.png_optimize > 0 && firecraft_io::export::png_optimizer_available() {
                    ui.weak("Export optimizes the PNG further (smaller than shown).");
                }
            }
            Err(e) => {
                ui.colored_label(crate::theme::pal().error, e);
            }
        },
        None => {
            ui.weak("Preview shows the export and its file size.");
        }
    }
}

/// The settings above the color table; true when Custom... asks for a palette file.
fn settings(ui: &mut Ui, s: &mut ExportSettings) -> bool {
    let mut load = false;
    ui.horizontal(|ui| {
        ui.label("Settings:");
        let current = s.preset_name().unwrap_or("(custom)");
        crate::theme::combo("opt_preset")
            .selected_text(current)
            // Narrower in a narrow dock, so the row fits.
            .width(ui.available_width().min(170.0))
            .show_ui(ui, |ui| {
                for name in PRESETS {
                    if ui.selectable_label(current == name, name).clicked()
                        && let Some(p) = ExportSettings::preset(name)
                    {
                        let raw = s.raw.take();
                        // Locked colors stay, whatever the palette.
                        let table = s.table.iter().filter(|e| e.locked).copied().collect();
                        *s = ExportSettings { raw, table, ..p };
                    }
                }
            });
    });
    ui.horizontal(|ui| {
        crate::theme::combo("opt_format")
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
        ExportFormat::Gif | ExportFormat::AnimatedGif | ExportFormat::Png8 => load = indexed(ui, s),
        ExportFormat::Jpeg => jpeg(ui, s),
        ExportFormat::Png24 | ExportFormat::Png32 => {}
        ExportFormat::WebP | ExportFormat::JpegXl => modern(ui, s),
    }
    if s.format.png() && firecraft_io::export::png_optimizer_available() {
        png_optimizer(ui, s);
    }
    load
}

fn matte(ui: &mut Ui, s: &mut ExportSettings) {
    use crate::fpui::color_picker::{self as cp, ColorChoice, Footer};
    ui.label("Matte:").on_hover_text(
        "The color semi-transparent edges are blended onto where the format can't keep them \
         (for an image going on that background color). None (as in Fireworks MX): edges keep \
         their own color, and fully transparent areas become the canvas color, or white.",
    );
    match cp::color_box(ui, "opt_matte", egui::vec2(22.0, 14.0), s.matte, Footer::NoneOnly) {
        Some(ColorChoice::Color(c)) => {
            let c = cp::to_rgba(c);
            s.matte = Some(Rgba { a: 255, ..c });
        }
        Some(ColorChoice::None) => s.matte = None,
        _ => {}
    }
}

/// GIF and PNG 8 options; true when Custom... asks for a palette file.
fn indexed(ui: &mut Ui, s: &mut ExportSettings) -> bool {
    let mut load = false;
    ui.horizontal(|ui| {
        crate::theme::combo("opt_palette")
            .selected_text(s.palette.label())
            .width(130.0)
            .show_ui(ui, |ui| {
                for p in PaletteKind::ALL {
                    ui.selectable_value(&mut s.palette, p, p.label());
                }
                // Fireworks MX's Custom... loads a palette (an .act file or a GIF's colors).
                if ui
                    .selectable_label(s.palette == PaletteKind::Custom, "Custom...")
                    .on_hover_text("Load a palette: a color table (.act), or the colors of a GIF or PNG 8")
                    .clicked()
                {
                    load = true;
                }
            });
    });
    ui.horizontal(|ui| {
        ui.label("Colors:");
        let fixed = matches!(s.palette, PaletteKind::BlackWhite | PaletteKind::Custom);
        ui.add_enabled_ui(!fixed, |ui| {
            if fixed {
                let mut n = s.color_limit() as u16;
                number_combo(ui, "opt_colors", &mut n, &COLOR_COUNTS, 2..=256);
            } else {
                number_combo(ui, "opt_colors", &mut s.colors, &COLOR_COUNTS, 2..=256);
            }
        });
    });
    ui.horizontal(|ui| {
        ui.label("Dither:");
        deferred_drag(ui, "opt_dither", &mut s.dither, |v| DragValue::new(v).range(0..=100).suffix("%"));
    });
    if s.palette == PaletteKind::WebAdaptive {
        ui.horizontal(|ui| {
            ui.label("WebSnap tolerance:")
                .on_hover_text("Palette colors this close to a Web 216 color snap to it");
            ui.add(DragValue::new(&mut s.websnap_tolerance).range(0..=50));
        });
    }
    crate::theme::combo("opt_transparency")
        .selected_text(s.transparency.label())
        .width(150.0)
        .show_ui(ui, |ui| {
            for t in Transparency::ALL {
                ui.selectable_value(&mut s.transparency, t, t.label());
            }
        });
    let alpha_palette = matches!(
        s.palette,
        PaletteKind::Adaptive | PaletteKind::WebAdaptive | PaletteKind::Exact
    );
    if s.format == ExportFormat::Png8 && s.transparency == Transparency::Alpha && alpha_palette {
        ui.checkbox(&mut s.smooth_alpha, "Smoother alpha").on_hover_text(
            "When the colors and their transparency don't all fit the color count, keep every \
             transparency level (smoother soft edges, colors a little coarser) instead of \
             rounding it to 16 levels as Fireworks MX does. When everything fits, transparency \
             is always kept exactly.",
        );
    }
    if s.format != ExportFormat::Png8 {
        ui.checkbox(&mut s.interlaced, "Interlaced");
    }
    ui.checkbox(&mut s.remove_unused, "Remove unused colors").on_hover_text(
        "Leave palette colors no pixel uses out of the file (locked colors stay), as Fireworks MX \
         does by default. Off: the whole palette is written, e.g. for images that must share one.",
    );
    if s.format == ExportFormat::AnimatedGif {
        ui.checkbox(&mut s.auto_crop, "Auto Crop")
            .on_hover_text("Each frame only covers the area that changed since the previous frame");
        ui.checkbox(&mut s.auto_difference, "Auto Difference")
            .on_hover_text("Pixels a frame doesn't change are left transparent, so the file is smaller");
        let adaptive = matches!(
            s.palette,
            PaletteKind::Adaptive
                | PaletteKind::WebAdaptive
                | PaletteKind::Exact
                | PaletteKind::Grayscale
                | PaletteKind::Uniform
                | PaletteKind::Web216
                | PaletteKind::Windows
                | PaletteKind::Macintosh
        );
        ui.add_enabled(adaptive, egui::Checkbox::new(&mut s.frame_palettes, "Maximize colors per frame"))
            .on_hover_text(
                "Each frame gets its own color table (up to the Colors count), made for the pixels \
                 it draws, so the animation can show more than 256 colors in all. With Auto Crop \
                 and Auto Difference each table only has to cover what changed. With Web 216, \
                 Windows or Macintosh and fewer colors than the palette has, each frame takes the \
                 palette's entries that fit it best. Takes longer to export. With dithering, areas a frame redraws can shimmer between frames \
                 (Auto Difference leaves unchanged pixels alone).",
            )
            .on_disabled_hover_text(
                "Not for Black & White (always the same two colors) or a custom palette (one color table)",
            );
        ui.horizontal(|ui| {
            ui.label("Loops:");
            ui.add(DragValue::new(&mut s.loops).range(0..=1000))
                .on_hover_text("Times the animation plays; 0 = forever");
            if s.loops == 0 {
                ui.weak("forever");
            }
        });
    }
    load
}

/// A number box whose value takes effect when the change is finished: at the end of a drag, or
/// when typing is confirmed (Enter, or clicking elsewhere). In between it shows the value being
/// set but leaves `value` alone, so an export preview that takes a while isn't redone for every
/// step of a drag or every digit typed. Arrow-key steps apply at once.
fn deferred_drag<T: egui::emath::Numeric>(
    ui: &mut Ui,
    id: &str,
    value: &mut T,
    widget: impl FnOnce(&mut T) -> DragValue<'_>,
) {
    let key = egui::Id::unique(("deferred_drag", id));
    let pending: Option<f64> = ui.data(|m| m.get_temp(key));
    let mut shown = pending.map_or(*value, T::from_f64);
    let resp = ui.add(widget(&mut shown).update_while_editing(false));
    if resp.dragged() {
        // Still dragging: remember where it is.
        ui.data_mut(|m| m.insert_temp(key, shown.to_f64()));
    } else {
        // Finished (drag released), or a typed value confirmed, or an arrow-key step.
        if pending.is_some() || resp.changed() {
            *value = shown;
        }
        ui.data_mut(|m| m.remove::<f64>(key));
    }
}

/// The preset color counts of the Colors box.
const COLOR_COUNTS: [u16; 8] = [2, 4, 8, 16, 32, 64, 128, 256];

/// A number box with a drop-down of preset values attached, like Fireworks' editable combo
/// boxes: type (or drag) any value, or pick one from the list.
fn number_combo(
    ui: &mut Ui,
    id: &str,
    value: &mut u16,
    presets: &[u16],
    range: std::ops::RangeInclusive<u16>,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        deferred_drag(ui, id, value, |v| DragValue::new(v).range(range).speed(0.25));
        crate::theme::combo(id)
            .selected_text("")
            .width(14.0)
            .show_ui(ui, |ui| {
                for &n in presets {
                    ui.selectable_value(value, n, n.to_string());
                }
            });
    });
}

/// The lossless PNG optimizer's level (applied by File > Export, after the Preview).
fn png_optimizer(ui: &mut Ui, s: &mut ExportSettings) {
    let label = |l: u8| match l {
        0 => "Off".to_owned(),
        1 => "1 (fastest)".to_owned(),
        7 => "7 (smallest, slow)".to_owned(),
        l => l.to_string(),
    };
    ui.horizontal(|ui| {
        ui.label("Optimize PNG:").on_hover_text(
            "Lossless recompression on export, like OptiPNG's -o levels: higher levels try more \
             filters, bit depths and palette reductions and compress harder. 7 adds Zopfli and \
             can take a long time on large images. The pixels never change.",
        );
        crate::theme::combo("opt_png_level")
            .selected_text(label(s.png_optimize))
            .width(120.0)
            .show_ui(ui, |ui| {
                for l in 0..=7u8 {
                    ui.selectable_value(&mut s.png_optimize, l, label(l));
                }
            });
    });
}

fn jpeg(ui: &mut Ui, s: &mut ExportSettings) {
    ui.horizontal(|ui| {
        ui.label("Quality:");
        crate::theme::slider(ui, Slider::new(&mut s.jpeg_quality, 0..=100));
    });
    ui.horizontal(|ui| {
        ui.label("Smoothing:");
        crate::theme::combo("opt_smoothing")
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
    ui.checkbox(&mut s.jpeg_full_color, "Full-resolution color")
        .on_hover_text("No chroma subsampling (as Fireworks MX writes JPEGs); off: smaller 4:2:0 files");
}

fn modern(ui: &mut Ui, s: &mut ExportSettings) {
    ui.checkbox(&mut s.lossless, "Lossless");
    ui.add_enabled_ui(!s.lossless, |ui| {
        ui.horizontal(|ui| {
            ui.label("Quality:");
            crate::theme::slider(ui, Slider::new(&mut s.quality, 0..=100));
        });
        ui.horizontal(|ui| {
            ui.label("Alpha Quality:")
                .on_hover_text("Quality of the transparency (100 keeps it exact)");
            crate::theme::slider(ui, Slider::new(&mut s.alpha_quality, 0..=100));
        });
    });
}
