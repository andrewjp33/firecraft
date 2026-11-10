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
mod fill_ui;
mod font_picker;
mod fpui;
mod gradient_ui;
mod icons;
mod inspector;
mod library;
mod menus;
mod nine;
mod panels;
mod path_tool;
mod stroke_dialog;
mod stroke_panel;
mod text_menu;
mod text_tool;
mod theme;
mod tools_panel;
mod transform_tool;

use std::collections::HashMap;

use egui::{Color32, Pos2, TextureHandle, Vec2};
use firecraft_engine::Session;

/// How a save went, as far as the host knows when it returns.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SaveStatus {
    /// Written; the path or name to remember for the document.
    Saved(String),
    Cancelled,
    Failed(String),
    /// Still running (browser file pickers are asynchronous); [`Platform::take_saved`] gives the
    /// outcome later.
    Pending,
}

/// The Fireworks folders set in Edit > Preferences.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FolderKind {
    Textures,
    Patterns,
    Fonts,
    /// Fonts the browser handed over, loaded as they're needed.
    InstalledFonts,
}

/// What a save in progress is for.
#[derive(Clone, Copy, Debug)]
enum SaveTarget {
    /// The document with this id.
    Doc(u64),
    Export,
}

/// Host services the shell needs (file dialogs differ between desktop and browser).
pub trait Platform {
    /// Ask the user for a file to open: (display name, bytes). `None` when cancelled, or when the
    /// file arrives later through [`Platform::take_opened`].
    fn open_file(&mut self) -> Option<(String, Vec<u8>)>;
    /// Ask where to save `bytes` and write them. `suggested` is a file name.
    fn save_file(&mut self, suggested: &str, bytes: &[u8]) -> SaveStatus;
    /// Write to a known path (Save on an already-saved document). Default: ask.
    fn write_file(&mut self, path: &str, bytes: &[u8]) -> SaveStatus {
        self.save_file(path, bytes)
    }
    /// The outcome of the save that returned [`SaveStatus::Pending`], once it has finished.
    fn take_saved(&mut self) -> Option<SaveStatus> {
        None
    }
    /// A saved path as the user should see it.
    fn display_path(&self, path: &str) -> String {
        path.to_owned()
    }
    /// Begin reading a dropped file (browsers read files asynchronously); results are returned
    /// later by [`Platform::take_opened`].
    fn read_dropped(&mut self, _file: egui::DroppedFileHandle) {}
    /// Files read since the last call, ready to open, or why one could not be read.
    fn take_opened(&mut self) -> Vec<Result<(String, Vec<u8>), String>> {
        Vec::new()
    }
    /// Ask for an image to use as a texture (PNG or GIF): (display name, bytes). `None` when
    /// cancelled, or when the file arrives later through [`Platform::take_image`].
    fn open_image(&mut self) -> Option<(String, Vec<u8>)> {
        self.open_file()
    }
    /// The image asked for by an earlier [`Platform::open_image`], once read, or why it could
    /// not be read.
    fn take_image(&mut self) -> Option<Result<(String, Vec<u8>), String>> {
        None
    }
    /// Ask for a folder. `None` when cancelled, or when the answer arrives later through
    /// [`Platform::take_folder`].
    fn pick_folder(&mut self, _kind: FolderKind) -> Option<String> {
        None
    }
    /// The answer to an earlier [`Platform::pick_folder`], once its files are read.
    fn take_folder(&mut self) -> Option<(FolderKind, Result<String, String>)> {
        None
    }
    /// The (file name, contents) of a folder only the host can read (a browser can't open a
    /// path); empty when the folder setting is an ordinary path.
    fn folder_files(&self, _kind: FolderKind) -> Vec<(String, Vec<u8>)> {
        Vec::new()
    }
    /// Changes whenever [`Platform::folder_files`] may give something new.
    fn folder_revision(&self) -> u64 {
        0
    }
    /// Whether the host can ask for the computer's installed fonts (a browser permission).
    fn can_request_fonts(&self) -> bool {
        false
    }
    /// Ask for them; the outcome arrives through [`Platform::take_fonts_status`].
    fn request_fonts(&mut self) {}
    /// The answer to [`Platform::request_fonts`]: how many families, or why not.
    fn take_fonts_status(&mut self) -> Option<Result<usize, String>> {
        None
    }
    /// The installed font families on offer (their data is loaded as needed).
    fn installed_fonts(&self) -> Vec<String> {
        Vec::new()
    }
    /// Fetch an installed font's data, by family or PostScript name, if it's on offer. It arrives
    /// in [`Platform::folder_files`] as [`FolderKind::InstalledFonts`].
    fn load_installed_font(&mut self, _name: &str) {}
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
    /// New documents draw perspective symbols as Fireworks MX does (else a pixel-correct warp).
    pub mx_perspective: bool,
    /// Show objects while moving them: dragged with the Pointer, or scaled, rotated, skewed or
    /// distorted (off: only their outline or box moves, and the canvas redraws once on release,
    /// for slower computers).
    pub live_drag: bool,
    /// Mouse highlight (Fireworks: Preferences > Editing): the object the Pointer or
    /// Subselection tool would select is outlined in red under the mouse.
    pub mouse_highlight: bool,
    /// Fireworks' install folder (or a folder of textures), for its textures; empty: look in
    /// the usual places.
    pub textures_dir: String,
    /// Fireworks' Patterns folder (for pattern fills, once they exist).
    pub patterns_dir: String,
    /// A folder of fonts to use besides the installed ones.
    pub fonts_dir: String,
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
            "tabs={}\nshow_tools={}\nshow_properties={}\nshow_dock={}\nundo_steps={}\nnew_doc_color_math={math}\naa_style={aa}\nnew_doc={}x{}@{}:{}\ncursor_style={}\nlive_drag={}\nmouse_highlight={}\ntextures_dir={}\npatterns_dir={}\nmx_perspective={}\nfonts_dir={}\n",
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
            self.mouse_highlight,
            self.textures_dir.replace(['\n', '\r'], ""),
            self.patterns_dir.replace(['\n', '\r'], ""),
            self.mx_perspective,
            self.fonts_dir.replace(['\n', '\r'], ""),
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
                "mx_perspective" => p.mx_perspective = v.trim() == "true",
                "live_drag" => p.live_drag = flag,
                "mouse_highlight" => p.mouse_highlight = flag,
                // `fireworks_dir` is the older name; an install folder still finds its textures.
                "textures_dir" | "fireworks_dir" => p.textures_dir = v.trim().to_owned(),
                "patterns_dir" => p.patterns_dir = v.trim().to_owned(),
                "fonts_dir" => p.fonts_dir = v.trim().to_owned(),
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
            mx_perspective: false,
            live_drag: true,
            mouse_highlight: true,
            textures_dir: String::new(),
            patterns_dir: String::new(),
            fonts_dir: String::new(),
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
    /// The save still in progress in the host, if any.
    pending_save: Option<SaveTarget>,
    /// The texture menu whose Other... was last used; a file arriving later goes to it.
    awaiting_texture: Option<stroke_panel::TextureSlot>,
    pi_open: bool,
    styled: bool,
    nine: Option<nine::NineEdit>,
    transform: Option<transform_tool::TransformEdit>,
    path_edit: path_tool::PathEdit,
    gradient_popup: Option<gradient_ui::Popup>,
    fill_window: Option<fill_ui::Window>,
    text_tool: text_tool::TextTool,
    fonts: std::sync::Arc<Vec<String>>,
    fonts_for: Option<(String, u64)>,
    fonts_loaded_for: Option<(u64, u64)>,
    /// Documents whose fonts have been checked when first shown.
    fonts_checked: std::collections::HashSet<u64>,
    stroke_window: Option<fill_ui::StrokeWindow>,
    /// The fill and stroke of the selected object the tools last took on.
    adopted: Option<(Option<firecraft_doc::Fill>, Option<firecraft_doc::Stroke>)>,
    /// Where a pattern file chosen with Other... goes, once the browser delivers it.
    awaiting_pattern: Option<gradient_ui::Target>,
    patterns: firecraft_io::textures::PatternLibrary,
    patterns_for: Option<(String, u64)>,
    fill_drag: Option<gradient_ui::FillDrag>,
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
    /// Textures offered for strokes, and the Fireworks folder setting they were read for.
    textures: firecraft_io::textures::TextureLibrary,
    textures_for: Option<(String, u64)>,
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
            pending_save: None,
            awaiting_texture: None,
            pi_open: true,
            styled: false,
            nine: None,
            transform: None,
            path_edit: path_tool::PathEdit::default(),
            gradient_popup: None,
            fill_window: None,
            text_tool: Default::default(),
            fonts: Default::default(),
            fonts_for: None,
            fonts_loaded_for: None,
            fonts_checked: Default::default(),
            stroke_window: None,
            adopted: None,
            awaiting_pattern: None,
            patterns: Default::default(),
            patterns_for: None,
            fill_drag: None,
            effects_ui: effects_ui::EffectsUi::default(),
            clipboard: None,
            clipboard_keys: false,
            workspace: egui::Rect::NOTHING,
            quit_confirmed: false,
            quit_prompt: false,
            textures: Default::default(),
            textures_for: None,
        }
    }

    /// The tools take on the selected object's fill and stroke and keep them after it is
    /// deselected, for the Tools panel and new objects (Fireworks MX).
    fn adopt_selected_attributes(&mut self) {
        let Some(d) = self.session.active_doc() else {
            return;
        };
        let Some((fill, stroke)) = d.selected_path_attributes() else {
            self.adopted = None;
            return;
        };
        if self
            .adopted
            .as_ref()
            .is_some_and(|(f, s)| f.as_ref() == fill && s.as_ref() == stroke)
        {
            return;
        }
        let owned = (fill.cloned(), stroke.cloned());
        self.session.options.adopt_fill(owned.0.as_ref());
        self.session.options.adopt_stroke(owned.1.as_ref());
        self.adopted = Some(owned);
    }

    /// Pick up the Fonts folder when it changes. The browser has no system fonts, so there
    /// egui's own stand in until a folder is chosen.
    fn refresh_fonts(&mut self) {
        use firecraft_text::{Font, FontDb};
        let source = (self.prefs.fonts_dir.clone(), self.platform.folder_revision());
        if self.fonts_for.as_ref() == Some(&source) {
            return;
        }
        self.fonts_for = Some(source);
        let dir = self.prefs.fonts_dir.trim().to_owned();
        let mut files = self.platform.folder_files(FolderKind::Fonts);
        files.extend(self.platform.folder_files(FolderKind::InstalledFonts));
        let web = cfg!(target_arch = "wasm32");
        if dir.is_empty() && files.is_empty() && !web {
            return;
        }
        let mut db = FontDb::system();
        if !dir.is_empty() {
            db.add_dir(std::path::Path::new(&dir));
        }
        for (_, bytes) in files {
            if let Ok(font) = Font::from_data(bytes, 0) {
                db.add_font(font);
            }
        }
        if db.is_empty() {
            for data in egui::FontDefinitions::default().font_data.values() {
                if let Ok(font) = Font::from_data((*data.font).as_ref().to_vec(), 0) {
                    db.add_font(font);
                }
            }
        }
        FontDb::set_shared(db);
        self.fonts = Default::default();
        // Text that was edited before its font arrived.
        for d in &mut self.session.docs {
            d.redraw_edited_text();
        }
    }

    /// Installed fonts (in the browser): report the permission's outcome, and fetch the fonts
    /// the active document needs once they're on offer.
    fn installed_fonts_progress(&mut self) {
        match self.platform.take_fonts_status() {
            Some(Err(message)) => self.dialogs.message = Some(("Installed Fonts".into(), message)),
            Some(Ok(_)) => self.fonts = Default::default(),
            None => {}
        }
        let Some(d) = self.session.active_doc() else {
            return;
        };
        let key = (d.id, self.platform.folder_revision());
        if self.fonts_loaded_for == Some(key) {
            return;
        }
        self.fonts_loaded_for = Some(key);
        for font in firecraft_engine::text::missing_fonts(d) {
            self.platform.load_installed_font(&font);
        }
    }

    /// When a document first shows, offer a choice about any fonts it needs that aren't here.
    fn check_missing_fonts(&mut self) {
        let Some(d) = self.session.active_doc() else {
            return;
        };
        if !self.fonts_checked.insert(d.id) {
            return;
        }
        let missing = firecraft_engine::text::missing_fonts(d);
        if !missing.is_empty() {
            let names = self.font_names();
            let fallback = ["Arial", "Liberation Sans", "Helvetica", "DejaVu Sans"]
                .iter()
                .find_map(|n| names.iter().find(|f| f.as_str() == *n))
                .or_else(|| names.first())
                .cloned()
                .unwrap_or_default();
            self.dialogs.missing_fonts = Some(missing.into_iter().map(|f| (f, fallback.clone())).collect());
        }
    }

    /// The installed font families, read once.
    pub(crate) fn font_names(&mut self) -> std::sync::Arc<Vec<String>> {
        if self.fonts.is_empty() {
            let mut names = firecraft_text::FontDb::shared().families();
            names.extend(self.platform.installed_fonts());
            names.sort_by_key(|n| n.to_lowercase());
            names.dedup();
            self.fonts = std::sync::Arc::new(names);
        }
        self.fonts.clone()
    }

    /// The pattern list, read again when the Patterns folder setting changes.
    pub(crate) fn pattern_library(&mut self) -> &firecraft_io::textures::PatternLibrary {
        self.patterns_mut()
    }

    pub(crate) fn patterns_mut(&mut self) -> &mut firecraft_io::textures::PatternLibrary {
        let source = (self.prefs.patterns_dir.clone(), self.platform.folder_revision());
        if self.patterns_for.as_ref() != Some(&source) {
            let dir = self.prefs.patterns_dir.trim();
            let configured = (!dir.is_empty()).then(|| std::path::Path::new(dir));
            let files = self.platform.folder_files(FolderKind::Patterns);
            let mut lib = firecraft_io::textures::PatternLibrary::scan(configured, files);
            lib.keep_added(std::mem::take(&mut self.patterns));
            self.patterns = lib;
            self.patterns_for = Some(source);
        }
        &mut self.patterns
    }

    /// The texture list, read again when the Fireworks folder setting changes; new strokes get
    /// its default texture (Fireworks' "Grain" when found).
    pub(crate) fn texture_library(&mut self) -> &mut firecraft_io::textures::TextureLibrary {
        let source = (self.prefs.textures_dir.clone(), self.platform.folder_revision());
        if self.textures_for.as_ref() != Some(&source) {
            let dir = self.prefs.textures_dir.trim();
            let dirs =
                firecraft_io::textures::fireworks_dirs((!dir.is_empty()).then(|| std::path::Path::new(dir)));
            let files = self.platform.folder_files(FolderKind::Textures);
            let mut lib = firecraft_io::textures::TextureLibrary::scan(&dirs, files);
            for t in std::mem::take(&mut self.textures).loaded() {
                lib.add(t);
            }
            self.textures = lib;
            self.textures_for = Some(source);
            let default = self.textures.default_texture();
            let o = &mut self.session.options;
            let builtin = |t: &Option<firecraft_doc::Texture>| {
                t.as_ref()
                    .is_none_or(|t| firecraft_io::textures::BUILTIN.contains(&t.name.as_str()))
            };
            if builtin(&o.stroke_texture) {
                o.stroke_texture.clone_from(&default);
            }
            if builtin(&o.fill_texture) {
                o.fill_texture = default;
            }
        }
        &mut self.textures
    }

    /// Open a file given its bytes (command line, drag and drop).
    pub fn open_bytes(&mut self, name: String, bytes: &[u8]) {
        match self.session.open_bytes(bytes, name.clone()) {
            Ok(()) => self.status = None,
            Err(e) => self.error(format!(
                "Could not open {}: {e}",
                self.platform.display_path(&name)
            )),
        }
    }

    /// Apply how a save went, or remember it if the host is still working on it.
    fn save_status(&mut self, target: SaveTarget, status: SaveStatus) {
        match (status, target) {
            (SaveStatus::Pending, _) => self.pending_save = Some(target),
            (SaveStatus::Cancelled, _) => {}
            (SaveStatus::Failed(e), SaveTarget::Doc(_)) => self.error(format!("Save failed: {e}")),
            (SaveStatus::Failed(e), SaveTarget::Export) => self.error(format!("Export failed: {e}")),
            (SaveStatus::Saved(path), SaveTarget::Doc(id)) => {
                if let Some(d) = self.session.docs.iter_mut().find(|d| d.id == id) {
                    d.doc.path = Some(path);
                    d.dirty = false;
                }
            }
            (SaveStatus::Saved(path), SaveTarget::Export) => {
                self.info(format!("Exported {}", self.platform.display_path(&path)));
            }
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
        self.texture_library();
        self.adopt_selected_attributes();
        self.refresh_fonts();
        self.installed_fonts_progress();
        text_tool::tidy(self);
        self.check_missing_fonts();
        let ctx = ui.ctx().clone();
        // Symbol Editors write their changes back to their symbols (instances update live).
        self.session.sync_symbols();
        // Selection and vector tools leave the Pen's path open, to resume on returning; any
        // other tool finishes it (Fireworks MX).
        if !matches!(
            self.session.tool,
            firecraft_engine::Tool::Pen
                | firecraft_engine::Tool::Pointer
                | firecraft_engine::Tool::SelectBehind
                | firecraft_engine::Tool::Subselect
                | firecraft_engine::Tool::Scale
                | firecraft_engine::Tool::Skew
                | firecraft_engine::Tool::Distort
                | firecraft_engine::Tool::Line
                | firecraft_engine::Tool::Rectangle
                | firecraft_engine::Tool::Ellipse
                | firecraft_engine::Tool::Freeform
                | firecraft_engine::Tool::Knife
        ) {
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
        for opened in self.platform.take_opened() {
            match opened {
                Ok((name, bytes)) => self.open_bytes(name, &bytes),
                Err(e) => self.error(e),
            }
        }
        if let Some((kind, folder)) = self.platform.take_folder() {
            match (folder, kind) {
                (Ok(dir), FolderKind::Textures) => self.prefs.textures_dir = dir,
                (Ok(dir), FolderKind::Patterns) => self.prefs.patterns_dir = dir,
                (Ok(dir), FolderKind::Fonts) => self.prefs.fonts_dir = dir,
                (Ok(_), FolderKind::InstalledFonts) => {}
                (Err(e), _) => self.error(e),
            }
        }
        if let Some(status) = self.platform.take_saved()
            && let Some(target) = self.pending_save.take()
        {
            self.save_status(target, status);
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
        text_menu::dialogs(self, &ctx);
        gradient_ui::popup(self, &ctx);
        fill_ui::window(self, &ctx);
        fill_ui::stroke_window(self, &ctx);
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
            mouse_highlight: false,
            ..Prefs::default()
        };
        assert_eq!(Prefs::parse(&p.serialize()), p);
        // Files from before the setting existed keep live dragging on.
        assert!(Prefs::parse("tabs=true").live_drag);
        assert_eq!(Prefs::parse("junk\nundo_steps=abc\nfuture=1"), Prefs::default());
    }

    #[test]
    fn folder_prefs() {
        let p = Prefs {
            textures_dir: "/fw/Textures".into(),
            patterns_dir: "/fw/Patterns".into(),
            fonts_dir: String::new(),
            ..Prefs::default()
        };
        assert_eq!(Prefs::parse(&p.serialize()), p);
        // The older single setting carries over as the textures folder.
        assert_eq!(Prefs::parse("fireworks_dir=/fw").textures_dir, "/fw");
    }
}
