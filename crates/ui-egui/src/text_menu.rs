//! The Text menu, in Fireworks MX's order, and its formatting shortcuts. Font, Size, Style and Align
//! change the text being edited, the selected text objects, or the Text tool's settings, like the
//! Property inspector's controls.

use egui::{Button, Key, KeyboardShortcut, Modifiers, ScrollArea};

use crate::FireCraftApp;
use crate::menus::{item, todo};
use crate::text_tool::{apply, current_style};

/// Size > presets (Fireworks MX).
const SIZES: [f64; 12] = [
    8.0, 9.0, 10.0, 12.0, 14.0, 18.0, 24.0, 36.0, 48.0, 72.0, 96.0, 120.0,
];
/// Align: value, label, shortcut key (with Ctrl+Alt+Shift).
const ALIGN: [(u8, &str, Key); 5] = [
    (0, "Left", Key::L),
    (1, "Centered Horizontally", Key::C),
    (2, "Right", Key::R),
    (3, "Justified", Key::J),
    (4, "Stretched", Key::S),
];
const BIGGEST: f64 = 1000.0;

/// An item with a check mark when `checked`.
fn check(ui: &mut egui::Ui, checked: bool, label: &str, shortcut: &str) -> bool {
    let clicked = ui
        .add(Button::selectable(checked, label).shortcut_text(shortcut))
        .clicked();
    if clicked {
        ui.close();
    }
    clicked
}

fn set_size(app: &mut FireCraftApp, size: f64) {
    apply(app, "Text Size", &move |r| r.size = size);
}

fn plain(app: &mut FireCraftApp) {
    apply(app, "Text Style", &|r| {
        r.bold = false;
        r.italic = false;
        r.underline = false;
    });
}

fn align(app: &mut FireCraftApp, value: u8) {
    apply(app, "Text Align", &move |r| r.align = value);
}

pub fn menu(app: &mut FireCraftApp, ui: &mut egui::Ui) {
    let (style, _, _) = current_style(app);
    let mut font = None;
    ui.menu_button("Font", |ui| {
        let fonts = app.font_names();
        ScrollArea::vertical().max_height(480.0).show(ui, |ui| {
            for name in fonts.iter() {
                if check(ui, *name == style.font, name, "") {
                    font = Some(name.clone());
                }
            }
        });
    });
    if let Some(name) = font {
        app.platform.load_installed_font(&name);
        apply(app, "Text Font", &move |r| r.font.clone_from(&name));
    }
    ui.menu_button("Size", |ui| {
        if item(ui, "Other...", "", true) {
            app.text_tool.other_size = Some(style.size);
        }
        if item(ui, "Smaller", "", true) {
            apply(app, "Text Size", &|r| r.size = (r.size - 1.0).max(1.0));
        }
        if item(ui, "Larger", "", true) {
            apply(app, "Text Size", &|r| r.size = (r.size + 1.0).min(BIGGEST));
        }
        ui.separator();
        for size in SIZES {
            if check(ui, style.size == size, &format!("{size}"), "") {
                set_size(app, size);
            }
        }
    });
    ui.menu_button("Style", |ui| {
        let none = !style.bold && !style.italic && !style.underline;
        if check(ui, none, "Plain", "Ctrl+Alt+Shift+P") {
            plain(app);
        }
        ui.separator();
        let (bold, italic, underline) = (style.bold, style.italic, style.underline);
        if check(ui, bold, "Bold", "Ctrl+Alt+Shift+B") {
            apply(app, "Text Style", &move |r| r.bold = !bold);
        }
        if check(ui, italic, "Italic", "Ctrl+Alt+Shift+I") {
            apply(app, "Text Style", &move |r| r.italic = !italic);
        }
        if check(ui, underline, "Underline", "Ctrl+Alt+Shift+U") {
            apply(app, "Text Style", &move |r| r.underline = !underline);
        }
    });
    ui.menu_button("Align", |ui| {
        for (value, label, key) in ALIGN {
            if check(
                ui,
                style.align == value,
                label,
                &format!("Ctrl+Alt+Shift+{key:?}"),
            ) {
                align(app, value);
            }
        }
        ui.separator();
        // For vertical text, which FireCraft doesn't lay out yet.
        for label in [
            "Top",
            "Centered Vertically",
            "Bottom",
            "Justified Vertically",
            "Stretched Vertically",
        ] {
            todo(ui, label, "");
        }
    });
    ui.separator();
    todo(ui, "Editor...", "");
    ui.separator();
    todo(ui, "Attach to Path", "Ctrl+Shift+Y");
    todo(ui, "Detach from Path", "");
    ui.menu_button("Orientation", |ui| {
        for label in [
            "Rotate Around Path",
            "Vertical",
            "Skew Vertical",
            "Skew Horizontal",
        ] {
            todo(ui, label, "");
        }
    });
    todo(ui, "Reverse Direction", "");
    ui.separator();
    let can_convert = app.session.active_doc().is_some_and(|d| d.can_convert_text());
    if item(ui, "Convert to Paths", "Ctrl+Shift+P", can_convert) {
        convert_to_paths(app);
    }
    ui.separator();
    todo(ui, "Check Spelling...", "Shift+F7");
    todo(ui, "Spelling Setup...", "");
}

/// Size > Other...: a size to type.
pub fn dialogs(app: &mut FireCraftApp, ctx: &egui::Context) {
    let Some(mut size) = app.text_tool.other_size else {
        return;
    };
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new("Text Size")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Size:");
                let field = ui.add(
                    egui::DragValue::new(&mut size)
                        .range(1.0..=BIGGEST)
                        .max_decimals(1),
                );
                if !field.has_focus() && !field.lost_focus() && ui.memory(|m| m.focused().is_none()) {
                    field.request_focus();
                }
                ok = field.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            });
            ui.horizontal(|ui| {
                ok |= ui.button("OK").clicked();
                cancel = ui.button("Cancel").clicked();
            });
        });
    cancel |= ctx.input(|i| i.key_pressed(Key::Escape));
    app.text_tool.other_size = Some(size);
    if ok {
        app.text_tool.other_size = None;
        set_size(app, size);
    } else if cancel {
        app.text_tool.other_size = None;
    }
}

/// Text > Convert to Paths (finishing any text being edited first).
fn convert_to_paths(app: &mut FireCraftApp) {
    if let Some(d) = app.session.active_doc_mut() {
        app.text_tool.finish(d);
        d.convert_text_to_paths();
    }
}

/// Text commands' keys that work on the selection, not while typing.
pub fn command_shortcuts(app: &mut FireCraftApp, ctx: &egui::Context) {
    let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
    if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(cmd_shift, Key::P))) {
        convert_to_paths(app);
    }
}

/// Fireworks MX's formatting keys. They also work while text is being edited.
pub fn shortcuts(app: &mut FireCraftApp, ctx: &egui::Context) {
    let all = Modifiers::COMMAND | Modifiers::ALT | Modifiers::SHIFT;
    let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(m, k)));
    let (style, _, _) = current_style(app);
    if pressed(all, Key::P) || pressed(Modifiers::NONE, Key::F5) {
        plain(app);
    }
    let (bold, italic, underline) = (style.bold, style.italic, style.underline);
    if pressed(all, Key::B) || pressed(Modifiers::NONE, Key::F6) {
        apply(app, "Text Style", &move |r| r.bold = !bold);
    }
    if pressed(all, Key::I) || pressed(Modifiers::NONE, Key::F7) {
        apply(app, "Text Style", &move |r| r.italic = !italic);
    }
    if pressed(all, Key::U) {
        apply(app, "Text Style", &move |r| r.underline = !underline);
    }
    for (value, _, key) in ALIGN {
        if pressed(all, key) {
            align(app, value);
        }
    }
}
