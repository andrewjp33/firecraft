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
pub mod cursors;
mod dialogs;
pub mod effects_ui;
mod icons;
mod inspector;
mod library;
mod menus;
mod nine;
mod panels;
mod path_tool;
mod theme;
mod tools_panel;
mod transform_tool;

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
        self.save_file(path, bytes)
            .map(|_| ())
            .ok_or_else(|| "cancelled".into())
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
    /// Magic Wand / selection anti-alias style last chosen in the Property inspector.
    pub aa_style: firecraft_raster::AntiAliasStyle,
    /// File > New's last settings: width, height, resolution, canvas (None = transparent).
    pub new_doc: (u32, u32, f64, Option<firecraft_raster::Rgba>),
    /// Tool cursors: FireCraft's own (Fireworks style) or the system's.
    pub cursor_style: cursors::CursorStyle,
    /// Show objects while moving them: dragged with the Pointer, or scaled, rotated, skewed or
    /// distorted (off: only their outline or box moves, and the canvas redraws once on release,
    /// for slower computers).
    pub live_drag: bool,
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
        let aa = match self.aa_style {
            firecraft_raster::AntiAliasStyle::Smooth => "smooth",
            firecraft_raster::AntiAliasStyle::Classic => "classic",
        };
        format!(
            "tabs={}\nshow_tools={}\nshow_properties={}\nshow_dock={}\nundo_steps={}\nnew_doc_color_math={math}\naa_style={aa}\nnew_doc={}x{}@{}:{}\ncursor_style={}\nlive_drag={}\n",
            self.tabs,
            self.show_tools,
            self.show_properties,
            self.show_dock,
            self.undo_steps,
            self.new_doc.0,
            self.new_doc.1,
            self.new_doc.2,
            self.new_doc
                .3
                .map_or_else(|| "transparent".to_owned(), |c| c.to_hex()),
            self.cursor_style.key(),
            self.live_drag,
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
                // (The pre-2026-11-01 key "color_math" recorded the old Fireworks MX default and is
                // ignored, so the new FireCraft default applies.)
                "new_doc_color_math" => {
                    p.color_math = if v.trim() == "precise" {
                        firecraft_raster::ColorMath::Precise
                    } else {
                        firecraft_raster::ColorMath::Classic
                    }
                }
                "new_doc" => {
                    if let Some(d) = parse_new_doc(v.trim()) {
                        p.new_doc = d;
                    }
                }
                "cursor_style" => p.cursor_style = cursors::CursorStyle::from_key(v.trim()),
                "live_drag" => p.live_drag = flag,
                "aa_style" => {
                    p.aa_style = if v.trim() == "classic" {
                        firecraft_raster::AntiAliasStyle::Classic
                    } else {
                        firecraft_raster::AntiAliasStyle::Smooth
                    }
                }
                _ => {}
            }
        }
        p
    }
}

/// `WxH@RES:canvas` with canvas `transparent` or a hex colour.
fn parse_new_doc(v: &str) -> Option<(u32, u32, f64, Option<firecraft_raster::Rgba>)> {
    let (size, rest) = v.split_once('@')?;
    let (w, h) = size.split_once('x')?;
    let (res, canvas) = rest.split_once(':')?;
    let (w, h, res): (u32, u32, f64) = (w.parse().ok()?, h.parse().ok()?, res.parse().ok()?);
    if !(1..=32_000).contains(&w) || !(1..=32_000).contains(&h) || !(1.0..=5000.0).contains(&res) {
        return None;
    }
    let canvas = if canvas == "transparent" {
        None
    } else {
        Some(firecraft_raster::Rgba::from_hex(canvas)?)
    };
    Some((w, h, res, canvas))
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            tabs: false,
            show_tools: true,
            show_properties: true,
            show_dock: true,
            undo_steps: firecraft_engine::DEFAULT_UNDO_STEPS,
            // FireCraft's improved formulas by default (Fireworks-exact is an option).
            color_math: firecraft_raster::ColorMath::Precise,
            aa_style: firecraft_raster::AntiAliasStyle::Smooth,
            new_doc: (660, 440, 72.0, Some(firecraft_raster::Rgba::WHITE)),
            cursor_style: cursors::CursorStyle::Fireworks,
            live_drag: true,
        }
    }
}

/// Per-document view state kept by the UI.
#[derive(Clone, Default)]
struct View {
    pan: Vec2,
    texture: Option<(u64, TextureHandle)>,
    /// Each object's rendered pixels, so moving or editing one object doesn't re-render the
    /// others.
    cache: std::rc::Rc<std::cell::RefCell<firecraft_doc::RenderCache>>,
    /// Pointer drag in progress: start in canvas coordinates.
    drag_start: Option<Pos2>,
    /// A live move in progress (Pointer drag with live dragging on): the whole-pixel offset
    /// applied to the selection so far.
    live_move: Option<(f64, f64)>,
    /// The drag in progress uses the secondary (right) button.
    drag_secondary: bool,
    preview: bool,
    /// Windows mode: the document window's rectangle when shown normally (FireCraft keeps it;
    /// egui draws the window exactly there), and its state.
    win_rect: Option<egui::Rect>,
    win: canvas::WinState,
    /// Not moved, resized, maximised or arranged by the user yet: the window keeps its default
    /// fit as the workspace changes size (e.g. when the main window maximises at startup).
    auto_fit: bool,
    /// The zoom FireCraft set to fit a newly opened document (while it equals the document's
    /// zoom, the fit follows the window; the user zooming ends it).
    auto_zoom: Option<f32>,
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
    transform: Option<transform_tool::TransformEdit>,
    path_edit: path_tool::PathEdit,
    effects_ui: effects_ui::EffectsUi,
    /// Edit > Copy/Cut: the objects, and the note put on the system clipboard (pasting the
    /// note back pastes them).
    clipboard: Option<(firecraft_engine::Clipboard, String)>,
    /// The app handles Ctrl+X/C/V itself (native: egui's clipboard shortcuts are off, so
    /// copied objects can go to the system clipboard in Fireworks' format).
    clipboard_keys: bool,
    /// The document area (windows mode), for Window > Cascade/Tile.
    workspace: egui::Rect,
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
            transform: None,
            path_edit: path_tool::PathEdit::default(),
            effects_ui: effects_ui::EffectsUi::default(),
            clipboard: None,
            clipboard_keys: false,
            workspace: egui::Rect::NOTHING,
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
        let names: Vec<String> = self
            .session
            .docs
            .iter()
            .filter(|d| d.dirty)
            .map(|d| d.doc.title())
            .collect();
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
    /// Apply loaded preferences (also the tool options they remember).
    /// The native app turned egui's clipboard shortcuts off and passes Ctrl+X/C/V through as
    /// keys: they copy and paste objects (with Fireworks MX, through the system clipboard).
    pub fn set_clipboard_keys(&mut self, on: bool) {
        self.clipboard_keys = on;
    }

    /// Native apps: make Ctrl+X/C/V cut, copy and paste text again while a text field has
    /// focus (egui's own clipboard shortcuts are off). Call from `eframe::App::raw_input_hook`.
    pub fn text_clipboard_keys(ctx: &egui::Context, input: &mut egui::RawInput) {
        if !ctx.egui_wants_keyboard_input() {
            return;
        }
        for e in &mut input.events {
            if let egui::Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = e
                && modifiers.command
                && !modifiers.alt
            {
                let replacement = match key {
                    egui::Key::C => Some(egui::Event::Copy),
                    egui::Key::X => Some(egui::Event::Cut),
                    egui::Key::V => firecraft_sysclip::get_text().map(egui::Event::Paste),
                    _ => None,
                };
                if let Some(r) = replacement {
                    *e = r;
                }
            }
        }
    }

    pub fn set_prefs(&mut self, prefs: Prefs) {
        self.session.options.aa_style = prefs.aa_style;
        self.dialogs.set_new_doc(prefs.new_doc);
        self.prefs = prefs;
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        // Tool options the preferences remember.
        self.prefs.aa_style = self.session.options.aa_style;
        let ctx = ui.ctx().clone();
        // Symbol Editors write their changes back to their symbols (instances update live).
        self.session.sync_symbols();
        // Choosing another tool finishes the path the Pen is drawing (as in Fireworks).
        if self.session.tool != firecraft_engine::Tool::Pen {
            self.path_edit.end_pen();
        }
        if let Some((doc, id)) =
            ctx.data_mut(|m| m.remove_temp::<(usize, u32)>(egui::Id::unique("edit_symbol")))
        {
            self.session.edit_symbol(doc, id);
        }
        if let Some(editor) = ctx.data_mut(|m| m.remove_temp::<u64>(egui::Id::unique("symbol_done"))) {
            self.session.finish_symbol_edit(editor);
        }
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
        if ctx.input(|i| i.viewport().close_requested())
            && !self.quit_confirmed
            && self.session.docs.iter().any(|d| d.dirty)
        {
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
        egui::Panel::top("menu_bar")
            .frame(theme::menu_frame())
            .show(ui, |ui| menus::menu_bar(self, ui));
        if self.prefs.show_tools {
            egui::Panel::left("tools")
                .exact_size(66.0)
                .resizable(false)
                .frame(theme::panel_frame())
                .show(ui, |ui| tools_panel::show(self, ui));
        }
        if self.prefs.show_dock {
            egui::Panel::right("dock")
                .default_size(230.0)
                .frame(theme::dock_frame())
                .show(ui, |ui| panels::dock(self, ui));
        }
        if self.prefs.show_properties {
            let h = if self.pi_open { 128.0 } else { 22.0 };
            egui::Panel::bottom("properties")
                .exact_size(h)
                .resizable(false)
                .frame(theme::panel_frame())
                .show(ui, |ui| inspector::show(self, ui));
        }
        egui::CentralPanel::default()
            .frame(theme::workspace_frame())
            .show(ui, |ui| canvas::workspace(self, ui));
        dialogs::show(self, &ctx);
        effects_ui::editor(self, &ctx);
        if let Some((msg, is_err)) = self.status.clone() {
            let color = if is_err {
                Color32::from_rgb(180, 0, 0)
            } else {
                Color32::from_gray(40)
            };
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
        let p = Prefs {
            tabs: true,
            undo_steps: 250,
            color_math: firecraft_raster::ColorMath::Precise,
            live_drag: false,
            ..Prefs::default()
        };
        assert_eq!(Prefs::parse(&p.serialize()), p);
        // Files from before the setting existed keep live dragging on.
        assert!(Prefs::parse("tabs=true").live_drag);
        assert_eq!(Prefs::parse("junk\nundo_steps=abc\nfuture=1"), Prefs::default());
    }
}
