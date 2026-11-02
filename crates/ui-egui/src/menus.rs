//! Menu bar (Fireworks MX's menus, in order) and keyboard shortcuts.
//!
//! Items FireCraft does not implement yet are shown disabled so the menus read like Fireworks'
//! and the gaps are visible. "Send to"/"Launch and Edit" integrations with other applications are
//! intentionally out of scope (see docs/roadmap.md).

use egui::{Button, Key, KeyboardShortcut, Modifiers};
use firecraft_engine::{Align, Arrange, Tool, Turn};

use crate::FireCraftApp;

fn item(ui: &mut egui::Ui, label: &str, shortcut: &str, enabled: bool) -> bool {
    let clicked = ui
        .add_enabled(enabled, Button::new(label).shortcut_text(shortcut))
        .clicked();
    if clicked {
        ui.close();
    }
    clicked
}

fn todo(ui: &mut egui::Ui, label: &str, shortcut: &str) {
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
            if item(ui, "New...", "Ctrl+N", true) {
                app.dialogs.new_doc = true;
            }
            if item(ui, "Open...", "Ctrl+O", true) {
                open(app);
            }
            todo(ui, "Open Recent", "");
            todo(ui, "Reconstitute Table...", "");
            todo(ui, "Scan", "");
            if item(ui, "Close", "Ctrl+W", has_doc) {
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
                if item(ui, "Bitmap via Copy", "", has_pixels) {
                    with_doc(app, |d| d.bitmap_via(false));
                }
                if item(ui, "Bitmap via Cut", "", has_pixels) {
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
                with_doc(app, |d| d.delete());
            }
            ui.separator();
            if item(ui, "Duplicate", "Ctrl+Alt+D", has_objects) {
                with_doc(app, |d| d.duplicate(true));
            }
            if item(ui, "Clone", "Ctrl+Shift+D", has_objects) {
                with_doc(app, |d| d.duplicate(false));
            }
            ui.separator();
            todo(ui, "Crop Selected Bitmap", "");
            todo(ui, "Crop Document", "");
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
            ui.menu_button("Canvas", |ui| {
                if item(ui, "Canvas Size...", "", has_doc) {
                    app.dialogs.open_canvas_size(app.session.active_doc());
                }
                if item(ui, "Canvas Color...", "", has_doc) {
                    app.dialogs.canvas_color = true;
                }
                todo(ui, "Image Size...", "");
                todo(ui, "Fit Canvas", "");
                todo(ui, "Rotate 180°", "");
            });
            todo(ui, "Animation", "");
            ui.menu_button("Symbol", |ui| {
                if item(ui, "Convert to Symbol...", "F8", has_objects) {
                    app.dialogs.convert_symbol =
                        Some(("Symbol".into(), firecraft_doc::SymbolKind::Graphic, false));
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
            });
            todo(ui, "Pop-up Menu", "");
            ui.separator();
            todo(ui, "Mask", "");
            todo(ui, "Selective JPEG", "");
            ui.separator();
            todo(ui, "Flatten Selection", "Ctrl+Alt+Shift+Z");
            todo(ui, "Merge Down", "Ctrl+E");
            todo(ui, "Flatten Layers", "");
            ui.separator();
            ui.menu_button("Transform", |ui| {
                for (label, key, tool) in [
                    ("Free Transform", "Ctrl+T", Tool::Scale),
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
            });
            ui.menu_button("Arrange", |ui| {
                for (label, key, how) in ARRANGE {
                    if item(ui, label, key, has_objects) {
                        with_doc(app, |d| d.arrange(how));
                    }
                }
            });
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
        ui.menu_button("Text", |ui| {
            for l in [
                "Font",
                "Size",
                "Style",
                "Align",
                "Editor...",
                "Attach to Path",
                "Convert to Paths",
                "Check Spelling...",
            ] {
                todo(ui, l, "");
            }
        });
        ui.menu_button("Commands", |ui| {
            todo(ui, "Manage Saved Commands...", "");
            todo(ui, "Run Script...", "");
            ui.weak("JavaScript commands (Fireworks API) are on the roadmap.");
        });
        ui.menu_button("Filters", |ui| {
            todo(ui, "Repeat Filter", "Ctrl+Alt+Shift+X");
            ui.separator();
            for l in ["Adjust Color", "Blur", "Other", "Sharpen"] {
                todo(ui, l, "");
            }
        });
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
    let existing = d
        .doc
        .path
        .clone()
        .filter(|p| !save_as && (p.contains('/') || p.contains('\\')));
    let result = match existing {
        Some(path) => app.platform.write_file(&path, &bytes).map(|_| Some(path)),
        None => Ok(app.platform.save_file(&d.doc.title(), &bytes)),
    };
    match result {
        Ok(Some(path)) => {
            if let Some(d) = app.session.active_doc_mut() {
                d.doc.path = Some(path.clone());
                d.dirty = false;
            }
        }
        Ok(None) => {}
        Err(e) => app.error(format!("Save failed: {e}")),
    }
}

pub fn export(app: &mut FireCraftApp) {
    let Some(d) = app.session.active_doc() else { return };
    let name = d.doc.title().trim_end_matches(".png").to_owned() + "-export.png";
    match firecraft_io::export_png(&d.doc) {
        Ok(bytes) => {
            if let Some(p) = app.platform.save_file(&name, &bytes) {
                app.info(format!("Exported {p}"));
            }
        }
        Err(e) => app.error(format!("Export failed: {e}")),
    }
}

#[allow(clippy::cognitive_complexity)] // a flat shortcut table; to be replaced by craft-keymap
pub fn shortcuts(app: &mut FireCraftApp, ctx: &egui::Context) {
    // Don't steal keys from text fields.
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    clipboard_events(app, ctx);
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
    } else if pressed(cmd, Key::T) && app.session.active_doc().is_some_and(|d| !d.selection.is_empty()) {
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
    if pressed(cmd, Key::N) {
        app.dialogs.new_doc = true;
    }
    if pressed(cmd, Key::O) {
        open(app);
    }
    if pressed(cmd, Key::W) {
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
    if pressed(cmd, Key::Num1) {
        with_doc(app, |d| d.zoom = 1.0);
    }
    if pressed(cmd, Key::Num0) {
        fit(app);
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
            with_doc(app, |d| d.delete());
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
    // Single-letter tool shortcuts (pressing again cycles a shared slot, as in Fireworks).
    let letters: [(Key, &[Tool]); 14] = [
        (Key::V, &[Tool::Pointer, Tool::SelectBehind]),
        (Key::A, &[Tool::Subselect]),
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
        .and_then(|b| firecraft_io::clipboard_png(&clip.elements, &clip.symbols, &clip.tiles, b).ok())
        .is_some_and(firecraft_sysclip::set_png);
    let note = clipboard_note(clip.elements.len());
    // Without a system clipboard for objects (the browser), a note comes back as a paste.
    if !shared && !app.clipboard_keys {
        ctx.copy_text(note.clone());
    }
    app.clipboard = Some((clip, note));
}

/// Edit > Paste: objects or a picture on the system clipboard (from Fireworks MX, another
/// FireCraft, any application), else FireCraft's own last copy.
fn paste(app: &mut FireCraftApp) {
    if firecraft_sysclip::available()
        && !firecraft_sysclip::owns()
        && let Some(png) = firecraft_sysclip::get_png()
    {
        match firecraft_io::read_clipboard_png(&png) {
            Ok((elements, symbols, tiles)) => {
                with_doc(app, |d| {
                    d.paste(&firecraft_engine::Clipboard {
                        elements,
                        symbols,
                        tiles,
                    })
                });
            }
            Err(e) => app.status = Some((format!("Can't paste the clipboard's image: {e}"), true)),
        }
        return;
    }
    if let (Some((clip, _)), Some(d)) = (&app.clipboard, app.session.docs.get_mut(app.session.active)) {
        d.paste(clip);
    }
}

/// Ctrl+X / Ctrl+C / Ctrl+V (egui turns them into clipboard events). An image from another
/// application pastes as a new bitmap object in the middle of the canvas.
fn clipboard_events(app: &mut FireCraftApp, ctx: &egui::Context) {
    if app.session.active_doc().is_none() {
        return;
    }
    let events = ctx.input(|i| i.events.clone());
    for e in events {
        match e {
            egui::Event::Copy => copy(app, ctx, false),
            egui::Event::Cut => copy(app, ctx, true),
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
                let Some(image) = firecraft_raster::RgbaImage::from_rgba_bytes(w, h, &rgba) else {
                    continue;
                };
                with_doc(app, |d| {
                    let (x, y) = (
                        (d.doc.width as i64 - w as i64) / 2,
                        (d.doc.height as i64 - h as i64) / 2,
                    );
                    let el = firecraft_doc::Element::new(firecraft_doc::ElementKind::Bitmap(
                        firecraft_doc::Bitmap { x, y, image },
                    ));
                    d.add_element(el, "Paste");
                });
            }
            _ => {}
        }
    }
}
