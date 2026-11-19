//! Menu bar (Fireworks MX's menus, in order) and keyboard shortcuts.
//!
//! Items FireCraft does not implement yet are shown disabled so the menus read like Fireworks'
//! and the gaps are visible. "Send to"/"Launch and Edit" integrations with other applications are
//! intentionally out of scope (see docs/roadmap.md).

use egui::{Button, Key, KeyboardShortcut, Modifiers};
use firecraft_engine::{Align, Arrange, Tool, Turn};

use crate::FireCraftApp;
use crate::SaveTarget;

pub(crate) fn item(ui: &mut egui::Ui, label: &str, shortcut: &str, enabled: bool) -> bool {
    let clicked = ui
        .add_enabled(enabled, Button::new(label).shortcut_text(shortcut))
        .clicked();
    if clicked {
        ui.close();
    }
    clicked
}

pub(crate) fn todo(ui: &mut egui::Ui, label: &str, shortcut: &str) {
    ui.add_enabled(false, Button::new(label).shortcut_text(shortcut))
        .on_disabled_hover_text("Not implemented yet");
}

pub fn menu_bar(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let has_doc = app.session.active_doc().is_some();
    let can_undo = app.session.active_doc().is_some_and(|d| d.history.can_undo());
    let can_redo = app.session.active_doc().is_some_and(|d| d.history.can_redo());
    let undo_label = app
        .session
        .active_doc()
        .and_then(|d| d.history.next_undo().map(|l| format!("Undo {l}")))
        .unwrap_or_else(|| "Can't Undo".into());
    let redo_label = app
        .session
        .active_doc()
        .and_then(|d| d.history.next_redo().map(|l| format!("Redo {l}")))
        .unwrap_or_else(|| "Can't Redo".into());
    let has_pixels = app.session.active_doc().is_some_and(|d| d.pixels.is_some());
    let has_objects = app.session.active_doc().is_some_and(|d| !d.selection.is_empty());
    let several = app.session.active_doc().is_some_and(|d| d.selection.len() >= 2);
    let can_ungroup = app.session.active_doc().is_some_and(|d| d.can_ungroup());
    let can_paste = has_doc && (app.clipboard.is_some() || firecraft_sysclip::available());
    let has_sel = app
        .session
        .active_doc()
        .is_some_and(|d| !d.selection.is_empty() || d.pixels.is_some());

    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            if item(ui, "New...", NEW_KEY, true) {
                open_new_doc(app);
            }
            if item(ui, "Open...", "Ctrl+O", true) {
                open(app);
            }
            todo(ui, "Open Recent", "");
            todo(ui, "Reconstitute Table...", "");
            todo(ui, "Scan", "");
            if item(ui, "Close", CLOSE_KEY, has_doc) {
                let i = app.session.active;
                app.session.close(i);
            }
            ui.separator();
            if item(ui, "Save", "Ctrl+S", has_doc) {
                save(app, false);
            }
            if item(ui, "Save As...", "Ctrl+Shift+S", has_doc) {
                save(app, true);
            }
            todo(ui, "Save a Copy...", "");
            todo(ui, "Revert", "");
            ui.separator();
            todo(ui, "Import...", "Ctrl+R");
            if item(ui, "Export...", "Ctrl+Shift+R", has_doc) {
                export(app);
            }
            todo(ui, "Export Preview...", "Ctrl+Shift+X");
            todo(ui, "Update HTML...", "");
            ui.separator();
            todo(ui, "Export Wizard...", "");
            todo(ui, "Batch Process...", "");
            todo(ui, "Preview in Browser", "");
            ui.separator();
            todo(ui, "Print...", "Ctrl+P");
            todo(ui, "HTML Setup...", "");
            ui.separator();
            if item(ui, "Exit", "Ctrl+Q", true) {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        ui.menu_button("Edit", |ui| {
            if item(ui, &undo_label, "Ctrl+Z", can_undo) {
                with_doc(app, |d| d.undo());
            }
            if item(ui, &redo_label, "Ctrl+Y", can_redo) {
                with_doc(app, |d| d.redo());
            }
            ui.separator();
            ui.menu_button("Insert", |ui| {
                if item(ui, "New Layer", "", has_doc) {
                    with_doc(app, |d| d.new_layer());
                }
                ui.separator();
                if item(ui, "Empty Bitmap", "", has_doc) {
                    with_doc(app, |d| d.insert_empty_bitmap());
                }
                if item(ui, "Bitmap Via Copy", "", has_bitmap_pixels(app)) {
                    with_doc(app, |d| d.bitmap_via(false));
                }
                if item(ui, "Bitmap Via Cut", "", has_bitmap_pixels(app)) {
                    with_doc(app, |d| d.bitmap_via(true));
                }
            });
            todo(ui, "Libraries", "");
            todo(ui, "Find and Replace...", "Ctrl+F");
            ui.separator();
            if item(ui, "Cut", "Ctrl+X", has_sel) {
                copy(app, ui.ctx(), true);
            }
            if item(ui, "Copy", "Ctrl+C", has_sel) {
                copy(app, ui.ctx(), false);
            }
            if item(ui, "Paste", "Ctrl+V", can_paste) {
                paste(app);
            }
            if item(ui, "Clear", "Backspace", has_sel) {
                delete_selection(app);
            }
            ui.separator();
            paste_mask_items(app, ui, can_paste);
            ui.separator();
            if item(ui, "Duplicate", "Ctrl+Alt+D", has_objects) {
                with_doc(app, |d| d.duplicate(true));
            }
            if item(ui, "Clone", "Ctrl+Shift+D", has_objects) {
                with_doc(app, |d| d.duplicate(false));
            }
            ui.separator();
            crop_items(app, ui);
            ui.separator();
            if item(ui, "Preferences...", "Ctrl+U", true) {
                app.dialogs.prefs = true;
            }
            todo(ui, "Keyboard Shortcuts...", "");
        });
        ui.menu_button("View", |ui| {
            if item(ui, "Zoom In", "Ctrl+=", has_doc) {
                zoom(app, 2.0);
            }
            if item(ui, "Zoom Out", "Ctrl+-", has_doc) {
                zoom(app, 0.5);
            }
            if item(ui, "100%", "Ctrl+1", has_doc) {
                with_doc(app, |d| d.zoom = 1.0);
            }
            if item(ui, "Fit All", "Ctrl+0", has_doc) {
                fit(app);
            }
            ui.separator();
            todo(ui, "Rulers", "Ctrl+Alt+R");
            todo(ui, "Grid", "");
            todo(ui, "Guides", "");
            ui.separator();
            let tools = app.prefs.show_tools && app.prefs.show_properties && app.prefs.show_dock;
            if item(ui, if tools { "Hide Panels" } else { "Show Panels" }, "F4", true) {
                toggle_panels(app);
            }
        });
        ui.menu_button("Select", |ui| {
            if item(ui, "Select All", "Ctrl+A", has_doc) {
                with_doc(app, |d| d.select_all());
            }
            if item(ui, "Deselect", "Ctrl+D", has_sel) {
                with_doc(app, |d| d.deselect());
            }
            if item(ui, "Superselect", "Ctrl+Right", has_sel) {
                with_doc(app, |d| d.superselect());
            }
            if item(ui, "Subselect", "Ctrl+Left", has_sel) {
                with_doc(app, |d| d.subselect());
            }
            ui.separator();
            todo(ui, "Select Similar", "");
            if item(ui, "Select Inverse", "Ctrl+Shift+I", has_pixels) {
                with_doc(app, |d| d.select_inverse());
            }
            if item(ui, "Feather...", "", has_pixels) {
                app.dialogs.feather = Some(10);
            }
            todo(ui, "Expand Marquee...", "");
            todo(ui, "Contract Marquee...", "");
            todo(ui, "Border Marquee...", "");
            todo(ui, "Smooth Marquee...", "");
        });
        ui.menu_button("Modify", |ui| {
            ui.menu_button("Canvas", |ui| canvas_items(app, ui));
            ui.menu_button("Animation", |ui| animation_items(app, ui));
            ui.menu_button("Symbol", |ui| symbol_items(app, ui, has_objects));
            todo(ui, "Pop-up Menu", "");
            ui.separator();
            ui.menu_button("Mask", |ui| mask_items(app, ui));
            todo(ui, "Selective JPEG", "");
            ui.separator();
            todo(ui, "Flatten Selection", "Ctrl+Alt+Shift+Z");
            todo(ui, "Merge Down", "Ctrl+E");
            todo(ui, "Flatten Layers", "");
            ui.separator();
            ui.menu_button("Transform", |ui| transform_items(app, ui, has_objects));
            ui.menu_button("Arrange", |ui| arrange_items(app, ui, has_objects));
            ui.menu_button("Align", |ui| {
                for (label, key, how) in ALIGN {
                    let enabled = match how {
                        Align::DistributeWidths | Align::DistributeHeights => {
                            app.session.active_doc().is_some_and(|d| d.selection.len() >= 3)
                        }
                        _ => has_objects,
                    };
                    if item(ui, label, key, enabled) {
                        with_doc(app, |d| d.align(how));
                    }
                }
            });
            ui.separator();
            todo(ui, "Combine Paths", "");
            todo(ui, "Alter Path", "");
            ui.separator();
            if item(ui, "Group", "Ctrl+G", has_objects) {
                with_doc(app, |d| d.group());
            }
            if item(ui, "Ungroup", "Ctrl+Shift+G", can_ungroup) {
                with_doc(app, |d| d.ungroup());
            }
            let _ = several;
        });
        ui.menu_button("Text", |ui| crate::text_menu::menu(app, ui));
        ui.menu_button("Commands", |ui| {
            todo(ui, "Manage Saved Commands...", "");
            todo(ui, "Run Script...", "");
            ui.weak("JavaScript commands (Fireworks API) are on the roadmap.");
        });
        ui.menu_button("Filters", |ui| crate::effects_ui::filters_menu(app, ui));
        ui.menu_button("Window", |ui| {
            let mut v = app.prefs.show_tools;
            if ui.checkbox(&mut v, "Tools").changed() {
                app.prefs.show_tools = v;
            }
            let mut v = app.prefs.show_properties;
            if ui.checkbox(&mut v, "Properties").changed() {
                app.prefs.show_properties = v;
            }
            let mut v = app.prefs.show_dock;
            if ui
                .checkbox(&mut v, "Panels (Optimize, Layers, Frames, History...)")
                .changed()
            {
                app.prefs.show_dock = v;
            }
            ui.separator();
            if item(ui, "Align", "", true) {
                crate::panels::show_align(app, ui.ctx());
            }
            ui.separator();
            let mut tabs = app.prefs.tabs;
            if ui.checkbox(&mut tabs, "Documents in Tabs").changed() {
                app.prefs.tabs = tabs;
            }
            let windows = !app.prefs.tabs && has_doc;
            for (label, how) in [
                ("Cascade", crate::canvas::Arrangement::Cascade),
                ("Tile Horizontal", crate::canvas::Arrangement::TileHorizontal),
                ("Tile Vertical", crate::canvas::Arrangement::TileVertical),
            ] {
                if item(ui, label, "", windows) {
                    crate::canvas::arrange(app, how);
                }
            }
            ui.separator();
            let titles: Vec<String> = app
                .session
                .docs
                .iter()
                .map(|d| format!("{} @ {:.0}%", d.doc.title(), d.zoom * 100.0))
                .collect();
            for (i, t) in titles.iter().enumerate() {
                if ui
                    .radio(i == app.session.active, format!("{} {t}", i + 1))
                    .clicked()
                {
                    app.session.active = i;
                    ui.close();
                }
            }
        });
        ui.menu_button("Help", |ui| {
            if item(ui, "About FireCraft", "", true) {
                app.dialogs.about = true;
            }
        });
    });
}

/// Modify > Canvas (also the canvas context menu's Modify Canvas).
fn canvas_items(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let has_doc = app.session.active_doc().is_some();
    // In Fireworks MX's order.
    if item(ui, "Image Size...", "", has_doc) {
        app.dialogs.open_image_size(app.session.active_doc());
    }
    if item(ui, "Canvas Size...", "", has_doc) {
        app.dialogs.open_canvas_size(app.session.active_doc());
    }
    if item(ui, "Canvas Color...", "", has_doc) {
        app.dialogs.canvas_color = true;
    }
    todo(ui, "Fit Canvas", "");
    todo(ui, "Rotate 180°", "");
}

/// Modify > Symbol (also in the canvas context menu).
fn symbol_items(app: &mut FireCraftApp, ui: &mut egui::Ui, has_objects: bool) {
    if item(ui, "Convert to Symbol...", "F8", has_objects) {
        app.dialogs.convert_symbol = Some(("Symbol".into(), firecraft_doc::SymbolKind::Graphic, false));
    }
    let instance = app
        .session
        .active_doc()
        .and_then(|d| d.selection.first().and_then(|r| d.instance_symbol(*r)));
    if item(ui, "Edit Symbol", "", instance.is_some()) {
        let i = app.session.active;
        if let Some(id) = instance {
            app.session.edit_symbol(i, id);
        }
    }
    todo(ui, "Tween Instances...", "");
    if item(
        ui,
        "Break Apart",
        "",
        app.session.active_doc().is_some_and(|d| d.can_break_apart()),
    ) {
        with_doc(app, |d| d.break_apart());
    }
}

/// Modify > Transform (also in the canvas context menu).
fn transform_items(app: &mut FireCraftApp, ui: &mut egui::Ui, has_objects: bool) {
    for (label, key, tool) in [
        ("Free Transform", TRANSFORM_KEY, Tool::Scale),
        ("Scale", "", Tool::Scale),
        ("Skew", "", Tool::Skew),
        ("Distort", "", Tool::Distort),
    ] {
        if item(ui, label, key, has_objects) {
            app.session.tool = tool;
        }
    }
    ui.separator();
    if item(ui, "Numeric Transform...", "Ctrl+Shift+T", has_objects) {
        app.dialogs.open_numeric(app.session.active_doc());
    }
    ui.separator();
    for (label, key, how) in TURNS {
        if item(ui, label, key, has_objects) {
            with_doc(app, |d| d.turn(how));
        }
    }
}

/// Modify > Animation (also in the canvas context menu), as in Fireworks MX: Animate Selection
/// for objects, Settings and Remove Animation for an animation symbol.
fn animation_items(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let doc = app.session.active_doc();
    let can_animate = doc.is_some_and(|d| d.can_animate_selection());
    let animation = doc.is_some_and(|d| d.selected_animation().is_some());
    if item(ui, "Animate Selection...", "Alt+Shift+F8", can_animate) || item(ui, "Settings...", "", animation) {
        app.dialogs.animate = app.session.active_doc().and_then(crate::dialogs::AnimateDialog::for_doc);
    }
    ui.separator();
    if item(ui, "Remove Animation", "", animation) {
        with_doc(app, |d| d.remove_animation());
    }
}

/// Modify > Arrange (also in the canvas context menu).
fn arrange_items(app: &mut FireCraftApp, ui: &mut egui::Ui, has_objects: bool) {
    for (label, key, how) in ARRANGE {
        if item(ui, label, key, has_objects) {
            with_doc(app, |d| d.arrange(how));
        }
    }
}

/// Pixels selected in a bitmap (Bitmap Via Copy/Cut apply).
fn has_bitmap_pixels(app: &FireCraftApp) -> bool {
    app.session
        .active_doc()
        .is_some_and(|d| d.pixels.as_ref().is_some_and(|p| p.target.is_some()))
}

/// Edit > Crop Selected Bitmap (a crop box around the bitmap) and Crop Document (at once, to
/// the selection's bounds). Both need something selected, as in Fireworks MX.
fn crop_items(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let d = app.session.active_doc();
    let bitmap = d.is_some_and(|d| d.symbol_link.is_none() && d.crop_bitmap_target().is_some());
    let rect = d.filter(|d| d.symbol_link.is_none()).and_then(|d| d.crop_document_rect());
    if item(ui, "Crop Selected Bitmap", "", bitmap) {
        crate::crop_tool::begin_bitmap_crop(app);
    }
    if item(ui, "Crop Document", "", rect.is_some())
        && let Some(r) = rect
    {
        with_doc(app, |d| {
            d.crop_document(r);
        });
    }
}

/// The document window's context menu (right-click), as Fireworks MX shows it: one for bitmap
/// editing (a pixel selection), one for selected objects, one for the empty canvas (measured
/// 2026-11-15). Items FireCraft doesn't have yet are shown disabled.
pub(crate) fn context_menu(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let Some(d) = app.session.active_doc() else {
        ui.close();
        return;
    };
    let has_pixels = d.pixels.is_some();
    let has_objects = !d.selection.is_empty();
    let can_ungroup = d.can_ungroup();
    let can_paste = app.clipboard.is_some() || firecraft_sysclip::available();
    if has_pixels {
        ui.menu_button("Edit", |ui| edit_items(app, ui, can_paste, false));
        ui.menu_button("Magnification", |ui| magnification_items(app, ui));
        crop_items(app, ui);
        ui.separator();
        if item(ui, "New Bitmap Via Copy", "", has_bitmap_pixels(app)) {
            with_doc(app, |d| d.bitmap_via(false));
        }
        if item(ui, "New Bitmap Via Cut", "", has_bitmap_pixels(app)) {
            with_doc(app, |d| d.bitmap_via(true));
        }
        todo(ui, "Merge Down", "Ctrl+E");
        ui.separator();
        if item(ui, "Select All", "Ctrl+A", true) {
            with_doc(app, |d| d.select_all());
        }
        if item(ui, "Deselect", "Ctrl+D", true) {
            with_doc(app, |d| d.deselect());
        }
        todo(ui, "Select Similar", "");
        ui.menu_button("Modify Marquee", |ui| {
            todo(ui, "Expand Marquee...", "");
            todo(ui, "Contract Marquee...", "");
            todo(ui, "Border Marquee...", "");
            todo(ui, "Smooth Marquee...", "");
        });
        todo(ui, "Hide Edges", "F9");
        ui.separator();
        todo(ui, "Save Selection", "");
        todo(ui, "Restore Selection", "");
        todo(ui, "Selective JPEG", "");
        ui.separator();
        // Transforming selected pixels isn't built (the transform tools move objects).
        todo(ui, "Free Transform", TRANSFORM_KEY);
        todo(ui, "Numeric Transform...", "Ctrl+Shift+T");
    } else if has_objects {
        ui.menu_button("Edit", |ui| edit_items(app, ui, can_paste, true));
        ui.separator();
        ui.menu_button("Magnification", |ui| magnification_items(app, ui));
        ui.menu_button("Select", |ui| {
            if item(ui, "Select All", "Ctrl+A", true) {
                with_doc(app, |d| d.select_all());
            }
            if item(ui, "Deselect", "Ctrl+D", true) {
                with_doc(app, |d| d.deselect());
            }
            if item(ui, "Superselect", "Ctrl+Right", true) {
                with_doc(app, |d| d.superselect());
            }
            if item(ui, "Subselect", "Ctrl+Left", true) {
                with_doc(app, |d| d.subselect());
            }
            todo(ui, "Select Similar", "");
        });
        ui.separator();
        todo(ui, "Hide Selection", "Ctrl+L");
        todo(ui, "Hide Edges", "F9");
        ui.separator();
        ui.menu_button("Arrange", |ui| arrange_items(app, ui, true));
        ui.menu_button("Transform", |ui| transform_items(app, ui, true));
        ui.separator();
        if item(ui, "Group", "Ctrl+G", true) {
            with_doc(app, |d| d.group());
        }
        if item(ui, "Ungroup", "Ctrl+Shift+G", can_ungroup) {
            with_doc(app, |d| d.ungroup());
        }
        ui.separator();
        if item(ui, "Convert to Symbol", "F8", true) {
            app.dialogs.convert_symbol = Some(("Symbol".into(), firecraft_doc::SymbolKind::Graphic, false));
        }
        ui.menu_button("Animation", |ui| animation_items(app, ui));
        ui.menu_button("Symbol", |ui| symbol_items(app, ui, true));
    } else {
        if item(ui, "Paste", "Ctrl+V", can_paste) {
            paste(app);
        }
        todo(ui, "Show All", "Ctrl+Shift+L");
        ui.separator();
        ui.menu_button("Magnification", |ui| magnification_items(app, ui));
        if item(ui, "Select All", "Ctrl+A", true) {
            with_doc(app, |d| d.select_all());
        }
        todo(ui, "Flatten Layers", "");
        ui.separator();
        ui.menu_button("Modify Canvas", |ui| canvas_items(app, ui));
        ui.separator();
        todo(ui, "Insert New Button...", "Ctrl+Shift+F8");
        todo(ui, "Insert New Symbol...", "Ctrl+F8");
        if item(ui, "Insert Empty Bitmap", "", true) {
            with_doc(app, |d| d.insert_empty_bitmap());
        }
        ui.separator();
        todo(ui, "Import...", "Ctrl+R");
        if item(ui, "Export...", "Ctrl+Shift+R", true) {
            export(app);
        }
        todo(ui, "Export Preview...", "Ctrl+Shift+X");
        ui.separator();
        if item(ui, "Preferences...", "Ctrl+U", true) {
            app.dialogs.prefs = true;
        }
    }
    if has_pixels || has_objects {
        // Their own group in the bitmap menu; right after Symbol in the object menu.
        if has_pixels {
            ui.separator();
        }
        todo(ui, "Insert Hotspot", "Ctrl+Shift+U");
        todo(ui, "Insert Slice", "Alt+Shift+U");
    }
    if has_objects && !has_pixels {
        // Fireworks MX's own wording.
        todo(ui, "Export Select Slice...", "");
        ui.separator();
        todo(ui, "Flatten Selection", "Ctrl+Alt+Shift+Z");
        todo(ui, "Merge Down", "Ctrl+E");
        todo(ui, "Flatten Layers", "");
    }
    ui.separator();
    todo(ui, "Exit Full Screen Mode", "");
}

/// The context menu's Edit submenu. Duplicate and Clone are for objects (Fireworks MX shows them
/// disabled while pixels are selected).
fn edit_items(app: &mut FireCraftApp, ui: &mut egui::Ui, can_paste: bool, objects: bool) {
    if item(ui, "Cut", "Ctrl+X", true) {
        copy(app, ui.ctx(), true);
    }
    if item(ui, "Copy", "Ctrl+C", true) {
        copy(app, ui.ctx(), false);
    }
    if item(ui, "Paste", "Ctrl+V", can_paste) {
        paste(app);
    }
    ui.separator();
    paste_mask_items(app, ui, can_paste);
    ui.separator();
    if item(ui, "Duplicate", "Ctrl+Alt+D", objects) {
        with_doc(app, |d| d.duplicate(true));
    }
    if item(ui, "Clone", "Ctrl+Shift+D", objects) {
        with_doc(app, |d| d.duplicate(false));
    }
}

/// The context menu's Magnification submenu: Fireworks MX's zoom levels.
fn magnification_items(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    todo(ui, "Fit Selection", "");
    if item(ui, "Fit All", "Ctrl+0", true) {
        fit(app);
    }
    ui.separator();
    let now = app.session.active_doc().map_or(1.0, |d| d.zoom);
    for (label, z) in [
        ("6%", 0.0625),
        ("12%", 0.125),
        ("25%", 0.25),
        ("50%", 0.5),
        ("66%", 0.66),
        ("100%", 1.0),
        ("150%", 1.5),
        ("200%", 2.0),
        ("300%", 3.0),
        ("400%", 4.0),
        ("800%", 8.0),
        ("1600%", 16.0),
        ("3200%", 32.0),
        ("6400%", 64.0),
    ] {
        let key = if z == 1.0 { "Ctrl+1" } else { "" };
        let checked = (now - z).abs() < 1e-3;
        if ui
            .add(Button::new(label).shortcut_text(key).selected(checked))
            .clicked()
        {
            with_doc(app, |d| d.zoom = z);
            ui.close();
        }
    }
}

fn with_doc(app: &mut FireCraftApp, f: impl FnOnce(&mut firecraft_engine::DocState)) {
    if let Some(d) = app.session.active_doc_mut() {
        f(d);
    }
}

fn zoom(app: &mut FireCraftApp, factor: f32) {
    with_doc(app, |d| d.zoom = (d.zoom * factor).clamp(1.0 / 16.0, 64.0));
}

fn fit(app: &mut FireCraftApp) {
    if let Some(id) = app.session.active_doc().map(|d| d.id) {
        if let Some(v) = app.views.get_mut(&id) {
            v.pan = egui::Vec2::ZERO;
        }
        // The canvas view computes the fitting zoom on its next frame.
        with_doc(app, |d| d.zoom = 0.0);
    }
}

fn toggle_panels(app: &mut FireCraftApp) {
    let shown = app.prefs.show_tools && app.prefs.show_properties && app.prefs.show_dock;
    app.prefs.show_tools = !shown;
    app.prefs.show_properties = !shown;
    app.prefs.show_dock = !shown;
}

pub fn open(app: &mut FireCraftApp) {
    if let Some((name, bytes)) = app.platform.open_file() {
        app.open_bytes(name, &bytes);
    }
}

pub fn save(app: &mut FireCraftApp, save_as: bool) {
    if app.pending_save.is_some() {
        return;
    }
    // In the Symbol Editor, Save saves the document the symbol belongs to (Fireworks doesn't
    // save from its Symbol Editor; FireCraft updates the document live, so saving it is the
    // useful meaning here).
    if let Some(link) = app.session.active_doc().and_then(|d| d.symbol_link)
        && let Some(p) = app.session.docs.iter().position(|d| d.id == link.parent)
    {
        app.session.sync_symbols();
        let editor = app.session.active;
        app.session.active = p;
        save(app, save_as);
        app.session.active = editor.min(app.session.docs.len().saturating_sub(1));
        return;
    }
    let Some(d) = app.session.active_doc() else { return };
    let bytes = match firecraft_io::save_fireworks_png(&d.doc) {
        Ok(b) => b,
        Err(e) => return app.error(format!("Save failed: {e}")),
    };
    // A document opened from a JPEG, GIF... is saved as a new Fireworks PNG, never over the
    // original (Fireworks asks for a name too).
    let existing = d.doc.path.clone().filter(|p| {
        !save_as && (p.contains('/') || p.contains('\\')) && firecraft_io::saves_in_place(p)
    });
    let id = d.id;
    let status = match existing {
        Some(path) => app.platform.write_file(&path, &bytes),
        None => app.platform.save_file(&firecraft_io::png_name(&d.doc.title()), &bytes),
    };
    app.save_status(SaveTarget::Doc(id), status);
}

pub fn export(app: &mut FireCraftApp) {
    if app.pending_save.is_some() {
        return;
    }
    let Some(d) = app.session.active_doc() else { return };
    // With the Optimize panel's settings, named after the document with the format's extension.
    let name = firecraft_io::export::file_name(&d.doc.title(), d.doc.export.format);
    match firecraft_io::export::export_file(&d.doc, &d.doc.export) {
        Ok(bytes) => {
            let status = app.platform.export_file(&name, &bytes);
            app.save_status(SaveTarget::Export, status);
        }
        Err(e) => app.error(format!("Export failed: {e}")),
    }
}

#[allow(clippy::cognitive_complexity)] // a flat shortcut table; to be replaced by craft-keymap
/// The shortcuts shown for commands whose usual keys a browser keeps for itself.
pub(crate) const NEW_KEY: &str = if cfg!(target_arch = "wasm32") {
    "Ctrl+Alt+N"
} else {
    "Ctrl+N"
};
const CLOSE_KEY: &str = if cfg!(target_arch = "wasm32") {
    "Ctrl+Alt+W"
} else {
    "Ctrl+W"
};
const TRANSFORM_KEY: &str = if cfg!(target_arch = "wasm32") {
    "Ctrl+Alt+T"
} else {
    "Ctrl+T"
};

pub fn shortcuts(app: &mut FireCraftApp, ctx: &egui::Context) {
    // A colour pop-up takes the keyboard (typed HEX/RGB values, Enter, Esc).
    if crate::fpui::color_picker::any_open(ctx) {
        return;
    }
    // Text formatting keys work while text is being edited too.
    let editing_text = app.text_tool.editing.is_some()
        && app.text_tool.focus.is_some()
        && ctx.memory(egui::Memory::focused) == app.text_tool.focus;
    if editing_text || !ctx.egui_wants_keyboard_input() {
        crate::text_menu::shortcuts(app, ctx);
    }
    // Don't steal keys from text fields.
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    clipboard_events(app, ctx);
    crate::text_menu::command_shortcuts(app, ctx);
    crate::effects_ui::filter_shortcuts(app, ctx);
    let cmd = Modifiers::COMMAND;
    let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
    // Native: Ctrl+X/C/V arrive as keys (see FireCraftApp::set_clipboard_keys).
    if app.clipboard_keys && app.session.active_doc().is_some() {
        if pressed(cmd, Key::X) {
            copy(app, ctx, true);
        }
        if pressed(cmd, Key::C) {
            copy(app, ctx, false);
        }
        if pressed(cmd, Key::V) {
            paste(app);
        }
    }
    // Object commands first: a shortcut without Shift/Alt also matches with them (Ctrl+D
    // would take Ctrl+Shift+D).
    let alt = cmd | Modifiers::ALT;
    let shift = cmd | Modifiers::SHIFT;
    if pressed(alt, Key::D) {
        with_doc(app, |d| d.duplicate(true));
    }
    if pressed(shift, Key::D) {
        with_doc(app, |d| d.duplicate(false));
    }
    if pressed(shift, Key::G) {
        with_doc(app, |d| d.ungroup());
    } else if pressed(cmd, Key::G) {
        with_doc(app, |d| d.group());
    }
    if pressed(shift, Key::T) {
        app.dialogs.open_numeric(app.session.active_doc());
    } else if (pressed(cmd | Modifiers::ALT, Key::T) || pressed(cmd, Key::T))
        && app.session.active_doc().is_some_and(|d| !d.selection.is_empty())
    {
        app.session.tool = Tool::Scale;
    }
    for (_, _, how, key) in ARRANGE_KEYS {
        let m = if matches!(how, Arrange::BringToFront | Arrange::SendToBack) {
            shift
        } else {
            cmd
        };
        if pressed(m, key) {
            with_doc(app, |d| d.arrange(how));
        }
    }
    for (how, key) in [
        (Align::Left, Key::Num1),
        (Align::CenterVertical, Key::Num2),
        (Align::Right, Key::Num3),
        (Align::Top, Key::Num4),
        (Align::CenterHorizontal, Key::Num5),
        (Align::Bottom, Key::Num6),
        (Align::DistributeWidths, Key::Num7),
        (Align::DistributeHeights, Key::Num9),
    ] {
        if pressed(alt, key) {
            with_doc(app, |d| d.align(how));
        }
    }
    if pressed(shift, Key::Num9) {
        with_doc(app, |d| d.turn(Turn::Rotate90Cw));
    }
    if pressed(shift, Key::Num7) {
        with_doc(app, |d| d.turn(Turn::Rotate90Ccw));
    }
    if pressed(cmd | Modifiers::SHIFT, Key::S) {
        save(app, true);
    } else if pressed(cmd, Key::S) {
        save(app, false);
    }
    if pressed(cmd, Key::Q) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    // Browsers keep Ctrl+N, Ctrl+W and Ctrl+T for themselves: these work there.
    let cmd_alt = cmd | Modifiers::ALT;
    if pressed(cmd_alt, Key::N) || pressed(cmd, Key::N) {
        open_new_doc(app);
    }
    if pressed(cmd, Key::O) {
        open(app);
    }
    if pressed(cmd_alt, Key::W) || pressed(cmd, Key::W) {
        let i = app.session.active;
        app.session.close(i);
    }
    if pressed(cmd | Modifiers::SHIFT, Key::R) {
        export(app);
    }
    if pressed(cmd | Modifiers::SHIFT, Key::Z) || pressed(cmd, Key::Y) {
        with_doc(app, |d| d.redo());
    } else if pressed(cmd, Key::Z) {
        with_doc(app, |d| d.undo());
    }
    if pressed(cmd, Key::A) {
        with_doc(app, |d| d.select_all());
    }
    if pressed(cmd, Key::D) {
        with_doc(app, |d| d.deselect());
    }
    if pressed(cmd | Modifiers::SHIFT, Key::I) {
        with_doc(app, |d| d.select_inverse());
    }
    if pressed(cmd, Key::U) {
        app.dialogs.prefs = true;
    }
    if pressed(cmd, Key::Equals) || pressed(cmd, Key::Plus) {
        zoom(app, 2.0);
    }
    if pressed(cmd, Key::Minus) {
        zoom(app, 0.5);
    }
    // Select > Superselect / Subselect (Fireworks MX shortcuts).
    if pressed(cmd, Key::ArrowRight) {
        with_doc(app, |d| d.superselect());
    }
    if pressed(cmd, Key::ArrowLeft) {
        with_doc(app, |d| d.subselect());
    }
    // Previous/next frame (Fireworks MX's shortcut set: Ctrl+Page Up / Ctrl+Page Down).
    for (key, back) in [(Key::PageUp, true), (Key::PageDown, false)] {
        if pressed(cmd, key) {
            // Like the footer's buttons, stepping stops playback.
            if let Some(id) = app.session.active_doc().map(|d| d.id)
                && let Some(view) = app.views.get_mut(&id)
            {
                view.playing = false;
            }
            with_doc(app, |d| {
                let n = d.doc.frames.len().max(1);
                d.doc.current_frame = if back {
                    (d.doc.current_frame + n - 1) % n
                } else {
                    (d.doc.current_frame + 1) % n
                };
            });
        }
    }
    if pressed(cmd, Key::Num1) {
        with_doc(app, |d| d.zoom = 1.0);
    }
    if pressed(cmd, Key::Num0) {
        fit(app);
    }
    // Modify > Animation > Animate Selection (Fireworks MX: Alt+Shift+F8).
    if pressed(Modifiers::ALT | Modifiers::SHIFT, Key::F8) {
        app.dialogs.animate = app
            .session
            .active_doc()
            .filter(|d| d.can_animate_selection())
            .and_then(crate::dialogs::AnimateDialog::for_doc);
    }
    if pressed(Modifiers::NONE, Key::F8) && app.session.active_doc().is_some_and(|d| !d.selection.is_empty())
    {
        app.dialogs.convert_symbol = Some(("Symbol".into(), firecraft_doc::SymbolKind::Graphic, false));
    }
    if pressed(Modifiers::NONE, Key::F4) {
        toggle_panels(app);
    }
    let point_tool = matches!(app.session.tool, Tool::Subselect | Tool::Pen);
    if pressed(Modifiers::NONE, Key::Delete) || pressed(Modifiers::NONE, Key::Backspace) {
        // Selected path points (Subselection/Pen), else the selected objects.
        let i = app.session.active;
        let points = point_tool
            && app
                .session
                .docs
                .get_mut(i)
                .is_some_and(|d| crate::path_tool::delete(&mut app.path_edit, d));
        if !points {
            delete_selection(app);
        }
    }
    // Arrow keys nudge the selection (Shift: 10 px).
    for (k, dx, dy) in [
        (Key::ArrowLeft, -1.0, 0.0),
        (Key::ArrowRight, 1.0, 0.0),
        (Key::ArrowUp, 0.0, -1.0),
        (Key::ArrowDown, 0.0, 1.0),
    ] {
        let step = if pressed(Modifiers::SHIFT, k) {
            10.0
        } else if pressed(Modifiers::NONE, k) {
            1.0
        } else {
            continue;
        };
        // The crop box with the Crop tool; the marquee with the marquee tools and the Magic
        // Wand (Fireworks MX).
        let (sx, sy) = ((dx * step) as i64, (dy * step) as i64);
        if app.session.tool == Tool::Crop {
            crate::crop_tool::nudge(app, sx, sy);
            continue;
        }
        let marquee_tool = matches!(
            app.session.tool,
            Tool::Marquee | Tool::OvalMarquee | Tool::Lasso | Tool::MagicWand
        );
        if marquee_tool && app.session.active_doc().is_some_and(|d| d.pixels.is_some()) {
            with_doc(app, |d| d.move_pixel_selection(sx, sy));
            continue;
        }
        // Selected path points (Subselection/Pen), else the selected objects.
        let i = app.session.active;
        let points = point_tool
            && app
                .session
                .docs
                .get_mut(i)
                .is_some_and(|d| crate::path_tool::nudge(&mut app.path_edit, d, dx * step, dy * step));
        if !points {
            with_doc(app, |d| d.move_selection(dx * step, dy * step, "Move"));
        }
    }
    // Single-letter tool shortcuts (pressing again cycles a shared slot, as in Fireworks; keys from
    // Fireworks MX's shortcut file).
    let letters: [(Key, &[Tool]); 18] = [
        (Key::V, &[Tool::Pointer, Tool::SelectBehind]),
        (Key::A, &[Tool::Subselect]),
        (Key::C, &[Tool::Crop]),
        (Key::Q, &[Tool::Scale, Tool::Skew, Tool::Distort, Tool::NineSlice]),
        (Key::M, &[Tool::Marquee, Tool::OvalMarquee]),
        (Key::W, &[Tool::MagicWand]),
        (Key::I, &[Tool::Eyedropper]),
        (Key::N, &[Tool::Line]),
        (Key::U, &[Tool::Rectangle, Tool::Ellipse]),
        (Key::H, &[Tool::Hand]),
        (Key::Z, &[Tool::Zoom]),
        (Key::L, &[Tool::Lasso]),
        (Key::P, &[Tool::Pen]),
        (Key::T, &[Tool::Text]),
        (Key::B, &[Tool::Brush]),
        (Key::Y, &[Tool::Pencil]),
        (Key::E, &[Tool::Eraser]),
        (Key::G, &[Tool::PaintBucket, Tool::Gradient]),
    ];
    for (k, tools) in letters {
        if pressed(Modifiers::NONE, k) {
            let cur = tools.iter().position(|t| *t == app.session.tool);
            let next = cur.map(|i| (i + 1) % tools.len()).unwrap_or(0);
            if let Some(t) = tools.get(next) {
                app.session.tool = *t;
            }
        }
    }
}

const TURNS: [(&str, &str, Turn); 5] = [
    ("Rotate 180°", "", Turn::Rotate180),
    ("Rotate 90° CW", "Ctrl+Shift+9", Turn::Rotate90Cw),
    ("Rotate 90° CCW", "Ctrl+Shift+7", Turn::Rotate90Ccw),
    ("Flip Horizontal", "", Turn::FlipHorizontal),
    ("Flip Vertical", "", Turn::FlipVertical),
];

const ARRANGE_KEYS: [(&str, &str, Arrange, Key); 4] = [
    (
        "Bring to Front",
        "Ctrl+Shift+Up",
        Arrange::BringToFront,
        Key::ArrowUp,
    ),
    ("Bring Forward", "Ctrl+Up", Arrange::BringForward, Key::ArrowUp),
    (
        "Send Backward",
        "Ctrl+Down",
        Arrange::SendBackward,
        Key::ArrowDown,
    ),
    (
        "Send to Back",
        "Ctrl+Shift+Down",
        Arrange::SendToBack,
        Key::ArrowDown,
    ),
];

const ARRANGE: [(&str, &str, Arrange); 4] = [
    (ARRANGE_KEYS[0].0, ARRANGE_KEYS[0].1, ARRANGE_KEYS[0].2),
    (ARRANGE_KEYS[1].0, ARRANGE_KEYS[1].1, ARRANGE_KEYS[1].2),
    (ARRANGE_KEYS[2].0, ARRANGE_KEYS[2].1, ARRANGE_KEYS[2].2),
    (ARRANGE_KEYS[3].0, ARRANGE_KEYS[3].1, ARRANGE_KEYS[3].2),
];

const ALIGN: [(&str, &str, Align); 8] = [
    ("Left", "Ctrl+Alt+1", Align::Left),
    ("Center Vertical", "Ctrl+Alt+2", Align::CenterVertical),
    ("Right", "Ctrl+Alt+3", Align::Right),
    ("Top", "Ctrl+Alt+4", Align::Top),
    ("Center Horizontal", "Ctrl+Alt+5", Align::CenterHorizontal),
    ("Bottom", "Ctrl+Alt+6", Align::Bottom),
    ("Distribute Widths", "Ctrl+Alt+7", Align::DistributeWidths),
    ("Distribute Heights", "Ctrl+Alt+9", Align::DistributeHeights),
];

/// What Copy puts on the system clipboard: a note that FireCraft holds the copied objects.
/// Pasting that text back (Ctrl+V) pastes the objects; text always round-trips through the
/// system clipboard (X11, Wayland, Windows, browsers), unlike images.
fn clipboard_note(n: usize) -> String {
    let what = if n == 1 {
        "1 object".to_owned()
    } else {
        format!("{n} objects")
    };
    format!("FireCraft: {what} (paste in FireCraft)")
}

/// Edit > Cut / Copy. The objects also go to the system clipboard as a Fireworks PNG (the
/// "PNG" format Fireworks MX copies and pastes; other applications see a picture).
fn copy(app: &mut FireCraftApp, ctx: &egui::Context, cut: bool) {
    let Some(d) = app.session.active_doc_mut() else {
        return;
    };
    let res = (d.doc.resolution, d.doc.resolution_cm);
    let clip = if cut { d.cut() } else { d.copy() };
    let Some(clip) = clip else { return };
    let bounds = clip
        .elements
        .iter()
        .filter_map(firecraft_doc::Element::bounds)
        .reduce(|a, b| {
            let (x0, y0) = (a.0.min(b.0), a.1.min(b.1));
            (
                x0,
                y0,
                (a.0 + a.2).max(b.0 + b.2) - x0,
                (a.1 + a.3).max(b.1 + b.3) - y0,
            )
        });
    let shared = bounds
        .and_then(|b| firecraft_io::clipboard_png(&clip.elements, &clip.symbols, &clip.tiles, b, res).ok())
        .is_some_and(firecraft_sysclip::set_png);
    let note = clipboard_note(clip.elements.len());
    // Without a system clipboard for objects (the browser), a note comes back as a paste.
    if !shared && !app.clipboard_keys {
        ctx.copy_text(note.clone());
    }
    app.clipboard = Some((clip, note));
}

/// Edit > Paste: objects or a picture on the system clipboard (from Fireworks MX, another
/// FireCraft, a screenshot, any application), else FireCraft's own last copy.
fn paste(app: &mut FireCraftApp) {
    if firecraft_sysclip::available()
        && !firecraft_sysclip::owns()
        && let Some(data) = firecraft_sysclip::get_image()
    {
        match firecraft_io::read_clipboard(&data) {
            Ok(firecraft_io::Pasted::Objects(elements, symbols, tiles)) => {
                let clip = firecraft_engine::Clipboard {
                    elements,
                    symbols,
                    tiles,
                };
                with_doc(app, |d| d.paste(&clip));
            }
            Ok(firecraft_io::Pasted::Image(image)) => paste_image(app, image),
            Err(e) => app.status = Some((format!("Can't paste the clipboard's image: {e}"), true)),
        }
        return;
    }
    if firecraft_sysclip::available() {
        let files = copied_pictures();
        if !files.is_empty() {
            paste_files(app, &files);
            return;
        }
    }
    if let (Some((clip, _)), Some(d)) = (&app.clipboard, app.session.docs.get_mut(app.session.active)) {
        d.paste(clip);
    }
}

/// Copied files (from Explorer or a file manager) that File > Open would take, by extension.
/// Copied files exist only where the system clipboard does (the desktop app).
fn copied_pictures() -> Vec<std::path::PathBuf> {
    firecraft_sysclip::copied_files()
        .into_iter()
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| firecraft_io::formats::OPEN_EXTENSIONS.iter().any(|x| x.eq_ignore_ascii_case(e)))
        })
        .collect()
}

/// Edit > Paste with image files copied in a file manager (FireCraft's own addition: Fireworks
/// MX can't paste files). Each file pastes as File > Open would read it, centred on the canvas:
/// a picture as a bitmap, a Fireworks PNG as its objects. One undo step per file.
fn paste_files(app: &mut FireCraftApp, files: &[std::path::PathBuf]) {
    let Some(d) = app.session.active_doc_mut() else {
        return;
    };
    let canvas = (d.doc.width, d.doc.height);
    let mut failed = Vec::new();
    for path in files {
        let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let pasted = std::fs::read(path)
            .map_err(|e| e.to_string())
            .and_then(|bytes| firecraft_io::file_as_paste(&bytes, &name, canvas).map_err(|e| e.to_string()));
        match pasted {
            Ok((elements, symbols, tiles)) => d.paste(&firecraft_engine::Clipboard {
                elements,
                symbols,
                tiles,
            }),
            Err(e) => failed.push(format!("{name}: {e}")),
        }
    }
    if !failed.is_empty() {
        app.status = Some((format!("Can't paste {}", failed.join("; ")), true));
    }
}

/// File > New: the last document's settings, or, with a picture on the clipboard, its size and
/// resolution (in its own unit) as in Fireworks MX.
pub(crate) fn open_new_doc(app: &mut FireCraftApp) {
    app.dialogs.new_doc = true;
    app.dialogs.set_new_doc(app.prefs.new_doc);
    app.dialogs.new_res_cm = app.prefs.new_doc_cm;
    if let Some((w, h, res)) = clipboard_canvas(app) {
        let dl = &mut app.dialogs;
        (dl.new_w, dl.new_h) = (w.clamp(1, 32000), h.clamp(1, 32000));
        if let Some((r, cm)) = res {
            (dl.new_res, dl.new_res_cm) = (r.clamp(1.0, 100_000.0), cm);
        }
    }
}

/// The size (and resolution, if known) of what Edit > Paste would bring in.
fn clipboard_canvas(app: &FireCraftApp) -> Option<(u32, u32, Option<firecraft_io::Resolution>)> {
    if firecraft_sysclip::available() {
        if let Some(data) = firecraft_sysclip::get_image() {
            return firecraft_io::clipboard_image_info(&data).map(|(w, h, r)| (w, h, Some(r)));
        }
        // A copied file (FireCraft's addition): the first one that opens, at the resolution
        // File > Open gives it.
        if let Some(doc) = copied_pictures().iter().find_map(|p| {
            let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
            firecraft_io::open(&std::fs::read(p).ok()?, name).ok()
        }) {
            return Some((doc.width, doc.height, Some((doc.resolution, doc.resolution_cm))));
        }
    }
    // FireCraft's own copy (the browser, where no system clipboard holds it).
    let (clip, _) = app.clipboard.as_ref()?;
    let (x0, y0, x1, y1) = clip
        .elements
        .iter()
        .filter_map(firecraft_doc::Element::bounds)
        .map(|(x, y, w, h)| (x, y, x + w, y + h))
        .reduce(|a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)))?;
    let res = app.session.active_doc().map(|d| (d.doc.resolution, d.doc.resolution_cm));
    Some(((x1.ceil() - x0.floor()) as u32, (y1.ceil() - y0.floor()) as u32, res))
}

/// A picture from another application, pasted as Fireworks MX does: a new bitmap object
/// centred on the canvas (the canvas keeps its size), selected.
fn paste_image(app: &mut FireCraftApp, image: firecraft_raster::RgbaImage) {
    with_doc(app, |d| {
        let (x, y) = firecraft_io::pasted_image_origin(
            (d.doc.width, d.doc.height),
            (image.width(), image.height()),
        );
        let el = firecraft_doc::Element::new(firecraft_doc::ElementKind::Bitmap(firecraft_doc::Bitmap {
            x,
            y,
            image,
        }));
        d.add_element(el, "Paste");
    });
}

/// Fireworks MX's message when a symbol dropped from the Library into a Symbol Editor would
/// end up containing itself (the drop is refused).
pub const SELF_NEST: &str = "Could not drag and drop.\nAlthough symbols may contain other symbols, a symbol can not contain a copy of itself.";

/// Ctrl+X / Ctrl+C / Ctrl+V (egui turns them into clipboard events). An image from another
/// application pastes as a new bitmap object in the middle of the canvas (see `paste_image`).
fn clipboard_events(app: &mut FireCraftApp, ctx: &egui::Context) {
    if app.session.active_doc().is_none() {
        return;
    }
    let events = ctx.input(|i| i.events.clone());
    for e in events {
        match e {
            egui::Event::Copy => copy(app, ctx, false),
            egui::Event::Cut => copy(app, ctx, true),
            // Ctrl+Shift+V comes as a paste too: Paste Inside.
            egui::Event::Paste(_) if ctx.input(|i| i.modifiers.shift) => paste_inside(app),
            egui::Event::Paste(text) => {
                if app.clipboard.as_ref().is_some_and(|(_, note)| *note == text) {
                    paste(app);
                } else {
                    app.status = Some((
                        "Pasting text isn't supported yet (FireCraft has no text tool yet).".into(),
                        false,
                    ));
                }
            }
            egui::Event::PasteImage(img) => {
                let rgba: Vec<u8> = img
                    .pixels
                    .iter()
                    .flat_map(|c| c.to_srgba_unmultiplied())
                    .collect();
                let (w, h) = (img.size[0] as u32, img.size[1] as u32);
                if let Some(image) = firecraft_raster::RgbaImage::from_rgba_bytes(w, h, &rgba) {
                    paste_image(app, image);
                }
            }
            _ => {}
        }
    }
}

/// Delete or Clear: a selected mask asks how to remove it (Delete Mask), anything else goes.
fn delete_selection(app: &mut FireCraftApp) {
    let mask = app.session.active_doc().and_then(|d| match d.selection.as_slice() {
        [r] if r.is_mask() && d.pixels.is_none() => d.mask_owner(*r),
        _ => None,
    });
    match mask {
        Some(owner) => app.dialogs.delete_mask = Some(owner),
        None => with_doc(app, |d| d.delete()),
    }
}

/// Modify > Mask, for the selected masked object (or its mask).
fn mask_items(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let owner = app.session.active_doc().and_then(|d| d.selected_mask_owner());
    let settings = owner.and_then(|o| app.session.active_doc().and_then(|d| d.mask_settings(o)));
    todo(ui, "Reveal All", "");
    todo(ui, "Hide All", "");
    todo(ui, "Reveal Selection", "");
    todo(ui, "Hide Selection", "");
    ui.separator();
    todo(ui, "Group as Mask", "");
    ui.separator();
    let enabled = settings.is_none_or(|s| s.1.enabled);
    if item(ui, if enabled { "Disable Mask" } else { "Enable Mask" }, "", owner.is_some())
        && let Some(o) = owner
    {
        with_doc(app, |d| d.set_mask_enabled(o, !enabled));
    }
    let linked = settings.is_none_or(|s| s.1.linked);
    if item(ui, if linked { "Unlink Mask" } else { "Link Mask" }, "", owner.is_some())
        && let Some(o) = owner
    {
        with_doc(app, |d| d.set_mask_linked(o, !linked));
    }
    if item(ui, "Delete Mask...", "", owner.is_some()) {
        app.dialogs.delete_mask = owner;
    }
}

/// Edit > Paste as Mask (and Paste Inside, not built yet). Fireworks MX enables Paste as Mask
/// for exactly one selected object.
fn paste_mask_items(app: &mut FireCraftApp, ui: &mut egui::Ui, can_paste: bool) {
    let target = app.session.active_doc().and_then(|d| d.paste_as_mask_target());
    if item(ui, "Paste as Mask", "", can_paste && target.is_some()) {
        paste_as_mask(app);
    }
    if item(ui, "Paste Inside", "Ctrl+Shift+V", can_paste && target.is_some()) {
        paste_inside(app);
    }
}

/// Edit > Paste Inside: the clipboard's objects inside the selected object, which becomes
/// their mask (or joins an existing mask's content).
fn paste_inside(app: &mut FireCraftApp) {
    if app.session.active_doc().and_then(|d| d.paste_as_mask_target()).is_none() {
        return;
    }
    if let Some(clip) = clipboard_contents(app) {
        with_doc(app, |d| d.paste_inside(&clip));
    }
}

/// What a paste would bring: the system clipboard's Fireworks objects or picture, copied
/// picture files, or FireCraft's own clipboard (in that order, as Edit > Paste).
fn clipboard_contents(app: &mut FireCraftApp) -> Option<firecraft_engine::Clipboard> {
    use firecraft_engine::Clipboard;
    if firecraft_sysclip::available()
        && !firecraft_sysclip::owns()
        && let Some(data) = firecraft_sysclip::get_image()
    {
        return match firecraft_io::read_clipboard(&data) {
            Ok(firecraft_io::Pasted::Objects(elements, symbols, tiles)) => Some(Clipboard {
                elements,
                symbols,
                tiles,
            }),
            Ok(firecraft_io::Pasted::Image(image)) => {
                let d = app.session.active_doc()?;
                let (x, y) = firecraft_io::pasted_image_origin((d.doc.width, d.doc.height), (image.width(), image.height()));
                let el = firecraft_doc::Element::new(firecraft_doc::ElementKind::Bitmap(firecraft_doc::Bitmap { x, y, image }));
                Some(Clipboard {
                    elements: vec![el],
                    ..Clipboard::default()
                })
            }
            Err(e) => {
                app.status = Some((format!("Can't paste the clipboard's image: {e}"), true));
                None
            }
        };
    }
    if firecraft_sysclip::available() {
        let files = copied_pictures();
        if !files.is_empty() {
            let canvas = app.session.active_doc().map(|d| (d.doc.width, d.doc.height))?;
            let mut clip = Clipboard::default();
            for path in &files {
                let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                if let Ok((elements, symbols, tiles)) = std::fs::read(path)
                    .map_err(|e| e.to_string())
                    .and_then(|b| firecraft_io::file_as_paste(&b, &name, canvas).map_err(|e| e.to_string()))
                {
                    clip.elements.extend(elements);
                    clip.symbols.extend(symbols);
                    clip.tiles.extend(tiles);
                }
            }
            return (!clip.elements.is_empty()).then_some(clip);
        }
    }
    app.clipboard.as_ref().map(|(c, _)| c.clone())
}

/// Edit > Paste as Mask: an object that already has a mask asks whether to replace it or add
/// to it (Fireworks MX's question).
fn paste_as_mask(app: &mut FireCraftApp) {
    let Some(clip) = clipboard_contents(app) else { return };
    let masked = app.session.active_doc().is_some_and(|d| {
        d.paste_as_mask_target()
            .and_then(|r| d.mask_owner(r))
            .is_some()
    });
    if masked {
        app.dialogs.paste_mask = Some(clip);
    } else {
        with_doc(app, |d| d.paste_as_mask(&clip, firecraft_engine::MaskPaste::Replace));
    }
}
