//! FireCraft's workspace, laid out like Fireworks MX:
//!
//! ```text
//! +--------------------------------------------------------------------------+
//! | File Edit View Select Modify Text Commands Filters Window Help           |
//! +------+-------------------------------------------------------+-----------+
//! |Tools | document windows (or tabs, Edit > Preferences)        | Optimize  |
//! |Select|  [Original][Preview][2-Up][4-Up]                      | Layers    |
//! |Bitmap|   canvas                                              | Frames/   |
//! |Vector|                                                       |  History  |
//! |Web   |                                                       | Assets    |
//! |Colors+-------------------------------------------------------+ Answers   |
//! |View  | Properties (context-sensitive Property inspector)     |           |
//! +------+-------------------------------------------------------+-----------+
//! ```
//!
//! All edits go through `firecraft-engine`; this crate only draws state and maps input.

mod canvas;
mod dialogs;
mod icons;
mod inspector;
mod menus;
mod nine;
mod panels;
mod theme;
mod tools_panel;

use std::collections::HashMap;

use egui::{Color32, Pos2, TextureHandle, Vec2};
use firecraft_engine::Session;

/// Host services the shell needs (file dialogs differ between desktop and browser).
pub trait Platform {
    /// Ask the user for a file to open: (display name, bytes).
    fn open_file(&mut self) -> Option<(String, Vec<u8>)>;
    /// Ask where to save `bytes`; returns the chosen path/name. `suggested` is a file name.
    fn save_file(&mut self, suggested: &str, bytes: &[u8]) -> Option<String>;
    /// Write to a known path (Save on an already-saved document). Default: ask.
    fn write_file(&mut self, path: &str, bytes: &[u8]) -> Result<(), String> {
        self.save_file(path, bytes).map(|_| ()).ok_or_else(|| "cancelled".into())
    }
    /// Begin reading a dropped file (browsers read files asynchronously); results are returned
    /// later by [`Platform::take_opened`].
    fn read_dropped(&mut self, _file: egui::DroppedFileHandle) {}
    /// Files read since the last call, ready to open.
    fn take_opened(&mut self) -> Vec<(String, Vec<u8>)> {
        Vec::new()
    }
}

/// User preferences (Edit > Preferences).
#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    /// Open documents as tabs in one window instead of separate document windows.
    pub tabs: bool,
    pub show_tools: bool,
    pub show_properties: bool,
    pub show_dock: bool,
    /// Edit > Preferences > Undo Steps.
    pub undo_steps: usize,
    /// Colour math for new documents.
    pub color_math: firecraft_raster::ColorMath,
}

impl Prefs {
    /// Key under which the apps store the preferences (eframe storage).
    pub const STORAGE_KEY: &'static str = "firecraft.prefs";

    /// `key=value` lines; unknown keys are ignored so older/newer versions can share the file.
    pub fn serialize(&self) -> String {
        let math = match self.color_math {
            firecraft_raster::ColorMath::Classic => "classic",
            firecraft_raster::ColorMath::Precise => "precise",
        };
        format!(
            "tabs={}\nshow_tools={}\nshow_properties={}\nshow_dock={}\nundo_steps={}\ncolor_math={math}\n",
            self.tabs, self.show_tools, self.show_properties, self.show_dock, self.undo_steps
        )
    }

    pub fn parse(text: &str) -> Self {
        let mut p = Self::default();
        for (k, v) in text.lines().filter_map(|l| l.split_once('=')) {
            let flag = v.trim() == "true";
            match k.trim() {
                "tabs" => p.tabs = flag,
                "show_tools" => p.show_tools = flag,
                "show_properties" => p.show_properties = flag,
                "show_dock" => p.show_dock = flag,
                "undo_steps" => {
                    if let Ok(n) = v.trim().parse::<usize>() {
                        p.undo_steps = n.clamp(1, firecraft_engine::MAX_UNDO_STEPS);
                    }
                }
                "color_math" => {
                    p.color_math = if v.trim() == "precise" {
                        firecraft_raster::ColorMath::Precise
                    } else {
                        firecraft_raster::ColorMath::Classic
                    }
                }
                _ => {}
            }
        }
        p
    }
}

impl Default for Prefs {
    fn default() -> Self {
        Self { tabs: false, show_tools: true, show_properties: true, show_dock: true, undo_steps: firecraft_engine::DEFAULT_UNDO_STEPS,
            color_math: firecraft_raster::ColorMath::Classic,
        }
    }
}

/// Per-document view state kept by the UI.
#[derive(Clone, Default)]
struct View {
    pan: Vec2,
    texture: Option<(u64, TextureHandle)>,
    /// Pointer drag in progress: start in canvas coordinates.
    drag_start: Option<Pos2>,
    /// The drag in progress uses the secondary (right) button.
    drag_secondary: bool,
    preview: bool,
    open: bool,
    /// Animation playback (status bar play button).
    playing: bool,
    last_tick: f64,
}

pub struct FireCraftApp {
    pub session: Session,
    pub prefs: Prefs,
    platform: Box<dyn Platform>,
    views: HashMap<u64, View>,
    dialogs: dialogs::Dialogs,
    status: Option<(String, bool)>,
    pi_open: bool,
    styled: bool,
    nine: Option<nine::NineEdit>,
    /// The user confirmed quitting with unsaved documents.
    quit_confirmed: bool,
    /// Showing the "save changes before quitting?" prompt.
    quit_prompt: bool,
}

impl FireCraftApp {
    pub fn new(platform: Box<dyn Platform>) -> Self {
        Self {
            session: Session::default(),
            prefs: Prefs::default(),
            platform,
            views: HashMap::new(),
            dialogs: dialogs::Dialogs::default(),
            status: None,
            pi_open: true,
            styled: false,
            nine: None,
            quit_confirmed: false,
            quit_prompt: false,
        }
    }

    /// Open a file given its bytes (command line, drag and drop).
    pub fn open_bytes(&mut self, name: String, bytes: &[u8]) {
        match self.session.open_bytes(bytes, name.clone()) {
            Ok(()) => self.status = None,
            Err(e) => self.error(format!("Could not open {name}: {e}")),
        }
    }

    fn error(&mut self, msg: String) {
        log::warn!("{msg}");
        self.status = Some((msg, true));
    }

    fn info(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), false));
    }

    fn quit_dialog(&mut self, ctx: &egui::Context) {
        let names: Vec<String> = self.session.docs.iter().filter(|d| d.dirty).map(|d| d.doc.title()).collect();
        egui::Window::new("Quit FireCraft")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("These documents have unsaved changes:");
                for n in &names {
                    ui.label(format!("    {n}"));
                }
                ui.horizontal(|ui| {
                    if ui.button("Quit Without Saving").clicked() {
                        self.quit_confirmed = true;
                        self.quit_prompt = false;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("Cancel").clicked() {
                        self.quit_prompt = false;
                    }
                });
            });
    }

    /// Draw one frame. Call from `eframe::App::ui`.
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if !self.styled {
            theme::apply(&ctx);
            self.styled = true;
        }
        // Dropped files open as documents (read synchronously on desktop, by the platform on the web).
        let dropped: Vec<egui::DroppedFileHandle> = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            #[cfg(not(target_arch = "wasm32"))]
            match f.bytes() {
                Ok(bytes) => self.open_bytes(f.path().display().to_string(), &bytes),
                Err(e) => self.error(format!("Could not read {}: {e}", f.path().display())),
            }
            #[cfg(target_arch = "wasm32")]
            self.platform.read_dropped(f);
        }
        for (name, bytes) in self.platform.take_opened() {
            self.open_bytes(name, &bytes);
        }

        // Closing the window (Exit, Ctrl+Q, the title bar's close button) with unsaved work asks first.
        if ctx.input(|i| i.viewport().close_requested()) && !self.quit_confirmed && self.session.docs.iter().any(|d| d.dirty) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.quit_prompt = true;
        }
        if self.quit_prompt {
            self.quit_dialog(&ctx);
        }

        for d in &mut self.session.docs {
            if d.history.limit != self.prefs.undo_steps {
                d.history.set_limit(self.prefs.undo_steps);
            }
        }
        menus::shortcuts(self, &ctx);
        egui::Panel::top("menu_bar").frame(theme::menu_frame()).show(ui, |ui| menus::menu_bar(self, ui));
        if self.prefs.show_tools {
            egui::Panel::left("tools").exact_size(66.0).resizable(false).frame(theme::panel_frame()).show(ui, |ui| {
                tools_panel::show(self, ui)
            });
        }
        if self.prefs.show_dock {
            egui::Panel::right("dock").default_size(230.0).frame(theme::dock_frame()).show(ui, |ui| panels::dock(self, ui));
        }
        if self.prefs.show_properties {
            let h = if self.pi_open { 128.0 } else { 22.0 };
            egui::Panel::bottom("properties").exact_size(h).resizable(false).frame(theme::panel_frame()).show(ui, |ui| {
                inspector::show(self, ui)
            });
        }
        egui::CentralPanel::default().frame(theme::workspace_frame()).show(ui, |ui| canvas::workspace(self, ui));
        dialogs::show(self, &ctx);
        if let Some((msg, is_err)) = self.status.clone() {
            let color = if is_err { Color32::from_rgb(180, 0, 0) } else { Color32::from_gray(40) };
            egui::Area::new(egui::Id::unique("status_toast"))
                .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -150.0])
                .show(&ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.colored_label(color, msg);
                            if ui.small_button("OK").clicked() {
                                self.status = None;
                            }
                        });
                    });
                });
        }
    }
}

#[cfg(test)]
mod prefs_tests {
    use super::*;

    #[test]
    fn prefs_round_trip() {
        let p = Prefs { tabs: true, undo_steps: 250, color_math: firecraft_raster::ColorMath::Precise, ..Prefs::default() };
        assert_eq!(Prefs::parse(&p.serialize()), p);
        assert_eq!(Prefs::parse("junk\nundo_steps=abc\nfuture=1"), Prefs::default());
    }
}
