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

mod align_panel;
mod canvas;
mod crop_tool;
mod size_dialogs;
pub mod cursors;
mod dialogs;
mod motion_path;
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
mod optimize;
mod panels;
mod path_tool;
mod pixels;
mod recent;
mod stroke_dialog;
mod stroke_panel;
mod tables_ui;
mod text_editor;
mod text_menu;
mod text_tool;
pub mod theme;
mod thumbs;
mod tiles;
pub mod tool_settings;
mod tools_panel;
mod transform_tool;

use std::collections::HashMap;

use egui::{Pos2, Vec2};
use firecraft_engine::Session;
/// File extensions File > Open reads (for the host's file dialog).
pub use firecraft_io::formats::OPEN_EXTENSIONS;

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
    /// The Optimize panel's Save Palette.
    Palette,
}

/// Host services the shell needs (file dialogs differ between desktop and browser).
pub trait Platform {
    /// Ask the user for a file to open: (display name, bytes). `None` when cancelled, or when the
    /// file arrives later through [`Platform::take_opened`].
    fn open_file(&mut self) -> Option<(String, Vec<u8>)>;
    /// Ask where to save `bytes` and write them. `suggested` is a file name.
    fn save_file(&mut self, suggested: &str, bytes: &[u8]) -> SaveStatus;
    /// Ask where to save an export and write it. `suggested` is a file name whose extension
    /// says the type (`.gif`, `.jpg`, `.webp`...). Default: as [`Platform::save_file`].
    fn export_file(&mut self, suggested: &str, bytes: &[u8]) -> SaveStatus {
        self.save_file(suggested, bytes)
    }
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
    /// Whether pictures pasted now should come in: a document is open and no text is being
    /// typed (where a paste is text). Told every frame; the browser uses it to decide whether a
    /// paste event's picture is FireCraft's to take.
    fn set_accepts_pictures(&mut self, _accepts: bool) {}
    /// Whether [`Platform::read_clipboard`] can look at the system clipboard (the browser's
    /// Clipboard API; the desktop reads it directly).
    fn can_read_clipboard(&self) -> bool {
        false
    }
    /// Start reading a picture from the system clipboard (Edit > Paste in the browser). The
    /// answer arrives through [`Platform::take_pasted`] as [`HostPaste::Read`].
    fn read_clipboard(&mut self) {}
    /// Pictures the host got from the system clipboard since the last call.
    fn take_pasted(&mut self) -> Vec<HostPaste> {
        Vec::new()
    }
    /// Whether the file at a document path can be opened again later without asking for it
    /// (File > Open Recent): a full path on the desktop, a file the browser gave a handle for.
    fn can_reopen(&self, _path: &str) -> bool {
        false
    }
    /// Read a recent file again: its bytes, or why it can't be read. `None`: the answer arrives
    /// later through [`Platform::take_opened`] (with this path).
    fn reopen(&mut self, _path: &str) -> Option<Result<Vec<u8>, String>> {
        Some(Err("this file can't be opened again from here".into()))
    }
    /// The recent-file list changed (the browser keeps those files' handles for later visits).
    fn recent_changed(&mut self, _paths: &[String]) {}
    /// Ask for a palette file (the Optimize panel's Load Palette: a color table `.act`, a GIF or
    /// a PNG 8): (display name, bytes). `None` when cancelled, or when the file arrives later
    /// through [`Platform::take_palette`].
    fn open_palette(&mut self) -> Option<(String, Vec<u8>)> {
        self.open_file()
    }
    /// The palette file asked for by [`Platform::open_palette`], once read, or why it could not be.
    fn take_palette(&mut self) -> Option<Result<(String, Vec<u8>), String>> {
        None
    }
}

/// A picture (image file bytes: PNG, JPEG, GIF...) the host read from the system clipboard.
pub enum HostPaste {
    /// Pasted with the keyboard (the browser's paste event): Edit > Paste it.
    Picture(Vec<u8>),
    /// The answer to [`Platform::read_clipboard`]: a picture, none on the clipboard, or why
    /// the clipboard could not be read.
    Read(Result<Option<Vec<u8>>, String>),
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
    /// File > New's last resolution unit: pixels/cm (else pixels/inch).
    pub new_doc_cm: bool,
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
    /// The Optimize panel's Show Swatch Feedback: the color table's swatch of the Preview pixel
    /// under the pointer is outlined (Fireworks MX's `XPDPaletteFeedback`, on).
    pub swatch_feedback: bool,
    /// Fireworks' install folder (or a folder of textures), for its textures; empty: look in
    /// the usual places.
    pub textures_dir: String,
    /// Fireworks' Patterns folder (for pattern fills, once they exist).
    pub patterns_dir: String,
    /// A folder of fonts to use besides the installed ones.
    pub fonts_dir: String,
    /// Edit > Preferences > UI Scale, in percent ([`UI_SCALE_RANGE`]): the whole interface drawn
    /// this much larger or smaller than the system's display scaling would draw it (100: as the
    /// system scales it, whatever the monitor's DPI). The `--ui-scale` launch option overrides it
    /// for one session.
    pub ui_scale: u32,
    /// The answer to "Automatically add new frames?" when an animation runs past the last frame,
    /// once "Don't show again" was ticked (Fireworks MX's prompt); `None`: ask.
    pub add_frames: Option<bool>,
    /// Image Size's Constrain Proportions, Resample Image and method, kept as Fireworks MX keeps
    /// them (its preferences `ResizeConstrain`, `ResizeResample`, `ImageSizeTransformMode`).
    pub image_size_constrain: bool,
    pub image_size_resample: bool,
    pub image_size_method: firecraft_raster::transform::Resample,
    /// Preferences > Interpolation: how bitmaps are resampled when objects are scaled, rotated,
    /// skewed or distorted (Fireworks MX: General > Interpolation, `EditTransformMode`,
    /// Bicubic by default; Image Size keeps its own).
    pub interpolation: firecraft_raster::transform::Resample,
    /// The Align panel's To Canvas and Anchors toggles and the part of each point Anchors mode
    /// lines up (Fireworks MX's panel keeps these between sessions).
    pub align_to_canvas: bool,
    pub align_anchors: bool,
    pub align_part: firecraft_engine::align_panel::PointPart,
    /// The Space buttons' gap in pixels (Fireworks CS6's spacing field); `None`: Evenly.
    pub align_spacing: Option<i32>,
    /// The last gap typed (Fireworks CS6 offers it in the field's menu and brings it back when
    /// the field is clicked while showing Evenly).
    pub align_spacing_px: i32,
    /// Preferences > Theme: Classic (Fireworks MX's look), Classic Dark, Modern, Modern Dark.
    pub theme: theme::ThemeKind,
    /// Preferences > Read dark mode from system: the light or dark version of the theme as
    /// the system's dark mode is, switching when it does.
    pub theme_follow_system: bool,
    /// Preferences > Accent color, in place of the default #3399FF (`None`).
    pub accent: Option<firecraft_raster::Rgba>,
}

/// The UI Scale preference's range, in percent.
pub const UI_SCALE_RANGE: std::ops::RangeInclusive<u32> = 50..=200;

/// A UI scale given as text ("150", "150%", "1.5"): percent, `None` if it isn't a number in
/// [`UI_SCALE_RANGE`] (a bare number up to 4 is read as a factor: "1.5" is 150%).
pub fn parse_ui_scale(text: &str) -> Option<u32> {
    let t = text.trim().trim_end_matches('%').trim();
    let v: f64 = t.parse().ok().filter(|v: &f64| v.is_finite())?;
    let pct = if v <= 4.0 && !text.contains('%') { v * 100.0 } else { v };
    let pct = pct.round();
    (pct >= *UI_SCALE_RANGE.start() as f64 && pct <= *UI_SCALE_RANGE.end() as f64).then_some(pct as u32)
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
            "tabs={}\nshow_tools={}\nshow_properties={}\nshow_dock={}\nundo_steps={}\nnew_doc_color_math={math}\naa_style={aa}\nnew_doc={}x{}@{}{}:{}\ncursor_style={}\nlive_drag={}\nmouse_highlight={}\ntextures_dir={}\npatterns_dir={}\nmx_perspective={}\nfonts_dir={}\nui_scale={}\nadd_frames={}\nimage_size_constrain={}\nimage_size_resample={}\nimage_size_method={}\ninterpolation={}\nalign_to_canvas={}\nalign_anchors={}\nalign_part={}\nalign_spacing={}\nalign_spacing_px={}\ntheme={}\ntheme_follow_system={}\naccent={}\nswatch_feedback={}\n",
            self.tabs,
            self.show_tools,
            self.show_properties,
            self.show_dock,
            self.undo_steps,
            self.new_doc.0,
            self.new_doc.1,
            self.new_doc.2,
            if self.new_doc_cm { "cm" } else { "" },
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
            self.ui_scale,
            match self.add_frames {
                None => "ask",
                Some(true) => "yes",
                Some(false) => "no",
            },
            self.image_size_constrain,
            self.image_size_resample,
            self.image_size_method.key(),
            self.interpolation.key(),
            self.align_to_canvas,
            self.align_anchors,
            align_panel::part_key(self.align_part),
            align_panel::spacing_text(self.align_spacing),
            self.align_spacing_px,
            self.theme.key(),
            self.theme_follow_system,
            self.accent.map(|c| c.to_hex()).unwrap_or_default(),
            self.swatch_feedback,
        )
    }

    /// Edit > Preferences / View > Theme: `kind` chosen. While reading dark mode from the
    /// system, a theme of the other darkness than the system's turns that off (else the choice
    /// would be undone at once).
    pub fn choose_theme(&mut self, kind: theme::ThemeKind, system_dark: Option<bool>) {
        if system_dark.is_some_and(|d| d != kind.is_dark()) {
            self.theme_follow_system = false;
        }
        self.theme = kind;
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
                    if let Some((d, cm)) = parse_new_doc(v.trim()) {
                        p.new_doc = d;
                        p.new_doc_cm = cm;
                    }
                }
                "cursor_style" => p.cursor_style = cursors::CursorStyle::from_key(v.trim()),
                "mx_perspective" => p.mx_perspective = v.trim() == "true",
                "live_drag" => p.live_drag = flag,
                "mouse_highlight" => p.mouse_highlight = flag,
                "swatch_feedback" => p.swatch_feedback = flag,
                // `fireworks_dir` is the older name; an install folder still finds its textures.
                "textures_dir" | "fireworks_dir" => p.textures_dir = v.trim().to_owned(),
                "patterns_dir" => p.patterns_dir = v.trim().to_owned(),
                "image_size_constrain" => p.image_size_constrain = flag,
                "image_size_resample" => p.image_size_resample = flag,
                "interpolation" => {
                    p.interpolation = firecraft_raster::transform::Resample::from_key(v.trim());
                }
                "image_size_method" => {
                    p.image_size_method = firecraft_raster::transform::Resample::from_key(v.trim());
                }
                "fonts_dir" => p.fonts_dir = v.trim().to_owned(),
                "theme" => p.theme = theme::ThemeKind::from_key(v.trim()),
                "theme_follow_system" => p.theme_follow_system = flag,
                "accent" => {
                    p.accent = firecraft_raster::Rgba::from_hex(v.trim()).map(|c| firecraft_raster::Rgba { a: 255, ..c });
                }
                "align_to_canvas" => p.align_to_canvas = flag,
                "align_anchors" => p.align_anchors = flag,
                "align_part" => p.align_part = align_panel::part_from_key(v.trim()),
                "align_spacing" => p.align_spacing = align_panel::parse_spacing(v),
                "align_spacing_px" => p.align_spacing_px = align_panel::parse_spacing(v).unwrap_or(0),
                "ui_scale" => {
                    if let Ok(n) = v.trim().parse::<u32>() {
                        p.ui_scale = n.clamp(*UI_SCALE_RANGE.start(), *UI_SCALE_RANGE.end());
                    }
                }
                "add_frames" => {
                    p.add_frames = match v.trim() {
                        "yes" => Some(true),
                        "no" => Some(false),
                        _ => None,
                    }
                }
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
/// `WxH@R:canvas`, R in pixels/inch or `Rcm` in pixels/cm.
#[allow(clippy::type_complexity)]
fn parse_new_doc(v: &str) -> Option<((u32, u32, f64, Option<firecraft_raster::Rgba>), bool)> {
    let (size, rest) = v.split_once('@')?;
    let (w, h) = size.split_once('x')?;
    let (res, canvas) = rest.split_once(':')?;
    let (res, cm) = res.strip_suffix("cm").map_or((res, false), |r| (r, true));
    let (w, h, res): (u32, u32, f64) = (w.parse().ok()?, h.parse().ok()?, res.parse().ok()?);
    if !(1..=32_000).contains(&w) || !(1..=32_000).contains(&h) || !(1.0..=100_000.0).contains(&res) {
        return None;
    }
    let canvas = if canvas == "transparent" {
        None
    } else {
        Some(firecraft_raster::Rgba::from_hex(canvas)?)
    };
    Some(((w, h, res, canvas), cm))
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
            new_doc_cm: false,
            cursor_style: cursors::CursorStyle::Fireworks,
            mx_perspective: false,
            live_drag: true,
            mouse_highlight: true,
            swatch_feedback: true,
            textures_dir: String::new(),
            patterns_dir: String::new(),
            fonts_dir: String::new(),
            ui_scale: 100,
            add_frames: None,
            image_size_constrain: true,
            image_size_resample: true,
            image_size_method: firecraft_raster::transform::Resample::Bicubic,
            interpolation: firecraft_raster::transform::Resample::Bicubic,
            align_to_canvas: false,
            align_anchors: false,
            align_part: firecraft_engine::align_panel::PointPart::Anchor,
            align_spacing: None,
            align_spacing_px: 0,
            theme: theme::ThemeKind::Modern,
            theme_follow_system: true,
            accent: None,
        }
    }
}

/// FIRECRAFT_PROFILE=1: print the parts of an update that took long.
struct Prof {
    on: bool,
    t: instant::Instant,
}

impl Prof {
    fn start() -> Self {
        Prof {
            on: std::env::var_os("FIRECRAFT_PROFILE").is_some(),
            t: instant::Instant::now(),
        }
    }

    fn lap(&mut self, what: &str) {
        if self.on {
            let ms = self.t.elapsed().as_secs_f64() * 1000.0;
            if ms >= 5.0 {
                eprintln!("update: {what} {ms:.1} ms");
            }
            self.t = instant::Instant::now();
        }
    }
}

/// Modern: the line egui shows while the dock's edge is hovered or dragged, drawn again over
/// the document area. egui paints it just outside the dock, where the document area (drawn
/// later, edge to edge in Modern) covered all but its ends.
fn dock_resize_line(ui: &egui::Ui, dock: egui::Rect) {
    if !theme::pal().modern {
        return;
    }
    // egui's id for a panel's resize handle: the panel's id, salted with "__resize".
    let id = ui.scope_id().with("dock").with("__resize");
    let Some(r) = ui.ctx().read_response(id) else {
        return;
    };
    let w = &ui.style().visuals.widgets;
    let stroke = if r.dragged() {
        w.active.fg_stroke
    } else if r.hovered() {
        w.hovered.fg_stroke
    } else {
        return;
    };
    let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Middle, id));
    painter.vline(dock.left() - 0.5 * stroke.width, dock.y_range(), stroke);
}

/// Per-document view state kept by the UI.
#[derive(Clone, Default)]
struct View {
    pan: Vec2,
    texture: Option<(u64, tiles::TiledTexture)>,
    /// Each object's rendered pixels, so moving or editing one object doesn't re-render the
    /// others.
    cache: std::rc::Rc<std::cell::RefCell<firecraft_doc::RenderCache>>,
    /// Pointer drag in progress: start in canvas coordinates.
    drag_start: Option<Pos2>,
    /// A live move in progress (Pointer drag with live dragging on): the whole-pixel offset
    /// applied to the selection so far.
    live_move: Option<(f64, f64)>,
    /// The live move duplicates (Alt held, Fireworks MX), and the objects it started with.
    live_dup: bool,
    drag_originals: Vec<firecraft_engine::ElementRef>,
    /// The drag in progress uses the secondary (right) button.
    drag_secondary: bool,
    /// The drag in progress moves the marquee (a marquee tool or the Magic Wand pressed inside
    /// the pixel selection).
    marquee_drag: bool,
    /// The Pointer drag in progress is a selection band (pressed on empty canvas).
    band: bool,
    preview: bool,
    /// The Preview tab's picture: the document exported with its Optimize settings.
    optimized: Option<canvas::Optimized>,
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
    /// The paste waiting for the host to read the system clipboard (the browser), if any.
    pending_paste: Option<menus::PasteKind>,
    /// File > Revert waiting for the host to read the file again (the browser): the document and
    /// its file.
    pending_revert: Option<(u64, String)>,
    /// The texture menu whose Other... was last used; a file arriving later goes to it.
    awaiting_texture: Option<stroke_panel::TextureSlot>,
    pi_open: bool,
    /// The theme and highlight colour applied to egui's style (`None` before the first frame).
    styled: Option<(theme::ThemeKind, Option<egui::Color32>)>,
    nine: Option<nine::NineEdit>,
    crop: Option<crop_tool::CropEdit>,
    transform: Option<transform_tool::TransformEdit>,
    path_edit: path_tool::PathEdit,
    gradient_popup: Option<gradient_ui::Popup>,
    fill_window: Option<fill_ui::Window>,
    text_tool: text_tool::TextTool,
    text_editor: Option<text_editor::TextEditor>,
    fonts: std::sync::Arc<Vec<String>>,
    fonts_for: Option<(String, u64)>,
    fonts_loaded_for: Option<(u64, u64)>,
    /// Documents whose fonts have been checked when first shown.
    fonts_checked: std::collections::HashSet<u64>,
    stroke_window: Option<fill_ui::StrokeWindow>,
    /// The tool at the last update: switching to the Pointer ends bitmap editing.
    prev_tool: firecraft_engine::Tool,
    /// The fill and stroke of the selected object the tools last took on.
    adopted: Option<(Option<firecraft_doc::Fill>, Option<firecraft_doc::Stroke>)>,
    /// Where a pattern file chosen with Other... goes, once the browser delivers it.
    awaiting_pattern: Option<gradient_ui::Target>,
    patterns: firecraft_io::textures::PatternLibrary,
    patterns_for: Option<(String, u64)>,
    fill_drag: Option<gradient_ui::FillDrag>,
    /// A point of an animation's motion path being dragged.
    path_drag: Option<motion_path::PathDrag>,
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
    /// The UI scale given at launch (`--ui-scale`), overriding the preference for this session
    /// until a new UI Scale is chosen in Preferences.
    ui_scale_override: Option<u32>,
    /// Textures and the pattern of the stored tool settings, still to be looked up.
    pending_tool_assets: tool_settings::Pending,
    /// File > Open Recent.
    recent: recent::RecentFiles,
    /// The Optimize panel's color table.
    optimize_table: optimize::TableState,
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
            pending_paste: None,
            pending_revert: None,
            awaiting_texture: None,
            pi_open: true,
            styled: None,
            nine: None,
            crop: None,
            transform: None,
            path_edit: path_tool::PathEdit::default(),
            gradient_popup: None,
            fill_window: None,
            text_tool: Default::default(),
            text_editor: None,
            fonts: Default::default(),
            fonts_for: None,
            fonts_loaded_for: None,
            fonts_checked: Default::default(),
            stroke_window: None,
            prev_tool: firecraft_engine::Tool::Pointer,
            adopted: None,
            awaiting_pattern: None,
            patterns: Default::default(),
            patterns_for: None,
            fill_drag: None,
            path_drag: None,
            effects_ui: effects_ui::EffectsUi::default(),
            clipboard: None,
            clipboard_keys: false,
            workspace: egui::Rect::NOTHING,
            quit_confirmed: false,
            quit_prompt: false,
            textures: Default::default(),
            textures_for: None,
            ui_scale_override: None,
            pending_tool_assets: Default::default(),
            recent: Default::default(),
            optimize_table: Default::default(),
        }
    }

    /// Everything kept between sessions, as (storage key, text): the preferences, the tools'
    /// last settings and the effects' last settings (Fireworks MX keeps all three).
    pub fn stored_settings(&self) -> Vec<(&'static str, String)> {
        let tools = tool_settings::serialize_tools(&self.session.options);
        vec![
            (Prefs::STORAGE_KEY, self.prefs.serialize()),
            (tool_settings::TOOLS_KEY, tools),
            (tool_settings::EFFECTS_KEY, self.effects_ui.memory.serialize()),
            (recent::STORAGE_KEY, self.recent.serialize()),
        ]
    }

    /// Take up what [`FireCraftApp::stored_settings`] gave in an earlier session; `get` reads a
    /// storage key.
    pub fn restore_settings(&mut self, get: impl Fn(&str) -> Option<String>) {
        if let Some(text) = get(Prefs::STORAGE_KEY) {
            self.set_prefs(Prefs::parse(&text));
        }
        if let Some(text) = get(tool_settings::TOOLS_KEY) {
            self.pending_tool_assets = tool_settings::parse_tools(&text, &mut self.session.options);
        }
        if let Some(text) = get(tool_settings::EFFECTS_KEY) {
            self.effects_ui.memory = tool_settings::EffectMemory::parse(&text);
        }
        if let Some(text) = get(recent::STORAGE_KEY) {
            self.recent = recent::RecentFiles::parse(&text);
            self.platform.recent_changed(self.recent.paths());
        }
    }

    /// The stored tool settings' textures and pattern, once their libraries can be read.
    fn restore_tool_assets(&mut self) {
        let pending = std::mem::take(&mut self.pending_tool_assets);
        if let Some(name) = pending.pattern {
            let label = firecraft_io::textures::pattern_label(&name).to_owned();
            if let Some((stem, image)) = self.pattern_library().get(&label) {
                self.session.options.fill_pattern =
                    Some(firecraft_engine::new_pattern(&stem, image, (0.0, 0.0, 1.0, 1.0)));
            }
        }
        if pending.stroke_texture.is_none() && pending.fill_texture.is_none() {
            return;
        }
        let lib = self.texture_library();
        let stroke = pending.stroke_texture.and_then(|n| lib.get(&n));
        let fill = pending.fill_texture.and_then(|n| lib.get(&n));
        let o = &mut self.session.options;
        if stroke.is_some() {
            o.stroke_texture = stroke;
        }
        if fill.is_some() {
            o.fill_texture = fill;
        }
    }

    /// The tools take on the selected object's fill and stroke and keep them after it is
    /// deselected, for the Tools panel and new objects (Fireworks MX).
    fn adopt_selected_attributes(&mut self) {
        let Some(d) = self.session.active_doc() else {
            return;
        };
        let Some(owned) = d.selected_attributes() else {
            self.adopted = None;
            return;
        };
        if self.adopted.as_ref() == Some(&owned) {
            return;
        }
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
        // No fonts at all (the browser): the interface's own, so text can be typed.
        if db.is_empty()
            && let Ok(font) = Font::from_data(theme::UI_FONT.to_vec(), 0)
        {
            db.add_font(font);
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
            Ok(()) => {
                self.status = None;
                self.note_recent(&name);
            }
            Err(e) => {
                self.error(format!(
                    "Could not open {}: {e}",
                    self.platform.display_path(&name)
                ));
                // Fireworks MX (MFC) drops a recent file that fails to open.
                self.forget_recent(&name);
            }
        }
    }

    /// A file was opened or saved: to the top of File > Open Recent (if it can be reopened).
    fn note_recent(&mut self, path: &str) {
        if self.platform.can_reopen(path) {
            self.recent.add(path);
            self.platform.recent_changed(self.recent.paths());
        }
    }

    fn forget_recent(&mut self, path: &str) {
        if self.recent.paths().iter().any(|p| p == path) {
            self.recent.remove(path);
            self.platform.recent_changed(self.recent.paths());
        }
    }

    /// File > Open Recent > a file. One already open is brought forward instead of opened twice.
    pub fn open_recent(&mut self, path: &str) {
        if let Some(i) = self
            .session
            .docs
            .iter()
            .position(|d| d.doc.path.as_deref() == Some(path) && d.symbol_link.is_none())
        {
            self.session.active = i;
            self.note_recent(path);
            return;
        }
        match self.platform.reopen(path) {
            Some(Ok(bytes)) => self.open_bytes(path.to_owned(), &bytes),
            Some(Err(e)) => {
                self.error(format!("Could not open {}: {e}", self.platform.display_path(path)));
                self.forget_recent(path);
            }
            None => {}
        }
    }

    /// The active document's file, if File > Revert can read it again: the document has changes
    /// and came from (or was saved to) a file this host can reopen. Fireworks MX grays Revert for
    /// unchanged and untitled documents (measured 2026-11-27); the Symbol Editor has no file.
    pub fn revert_path(&self) -> Option<String> {
        let d = self.session.active_doc()?;
        let path = d.doc.path.as_deref()?;
        (d.dirty && d.symbol_link.is_none() && self.platform.can_reopen(path)).then(|| path.to_owned())
    }

    /// File > Revert, confirmed: read document `id`'s file again and put it in its place.
    pub fn revert(&mut self, id: u64) {
        let Some(path) = self.session.docs.iter().find(|d| d.id == id).and_then(|d| d.doc.path.clone()) else {
            return;
        };
        match self.platform.reopen(&path) {
            Some(Ok(bytes)) => self.apply_revert(id, &path, &bytes),
            Some(Err(e)) => self.error(format!("Could not revert to {}: {e}", self.platform.display_path(&path))),
            None => self.pending_revert = Some((id, path)),
        }
    }

    fn apply_revert(&mut self, id: u64, path: &str, bytes: &[u8]) {
        let doc = match firecraft_io::open(bytes, Some(path.to_owned())) {
            Ok(doc) => doc,
            Err(e) => return self.error(format!("Could not revert to {}: {e}", self.platform.display_path(path))),
        };
        let Some(i) = self.session.docs.iter().position(|d| d.id == id) else { return };
        // Edits in progress belong to the old content: drop them.
        if let Some(d) = self.session.docs.get_mut(i) {
            self.text_tool.finish(d);
        }
        self.text_editor = None;
        self.crop = None;
        self.nine = None;
        self.transform = None;
        self.path_edit.end_pen();
        self.session.revert(i, doc);
        // Its fonts are checked again, as when it was opened.
        self.fonts_checked.remove(&id);
        self.status = None;
    }

    /// File > Open Recent > Clear Recent Files.
    pub fn clear_recent(&mut self) {
        self.recent.clear();
        self.platform.recent_changed(self.recent.paths());
    }

    /// Apply how a save went, or remember it if the host is still working on it.
    fn save_status(&mut self, target: SaveTarget, status: SaveStatus) {
        match (status, target) {
            (SaveStatus::Pending, _) => self.pending_save = Some(target),
            (SaveStatus::Cancelled, _) => {}
            (SaveStatus::Failed(e), SaveTarget::Doc(_)) => self.error(format!("Save failed: {e}")),
            (SaveStatus::Failed(e), SaveTarget::Export) => self.error(format!("Export failed: {e}")),
            (SaveStatus::Failed(e), SaveTarget::Palette) => self.error(format!("Could not save the palette: {e}")),
            (SaveStatus::Saved(path), SaveTarget::Palette) => {
                self.info(format!("Saved the palette as {}", self.platform.display_path(&path)));
            }
            (SaveStatus::Saved(path), SaveTarget::Doc(id)) => {
                self.note_recent(&path);
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

    /// What must happen even while the window is hidden or minimized (eframe then calls only
    /// `App::logic`, not `App::ui`). Call from `eframe::App::logic`.
    ///
    /// Closing the window (Exit, Ctrl+Q, the title bar's close button, or the taskbar's Close
    /// while minimized) with unsaved work asks first, and brings the window up to ask: checked
    /// here because a close requested while minimized never reaches `ui`, and the app used to
    /// close without asking then.
    pub fn logic(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested())
            && !self.quit_confirmed
            && self.session.docs.iter().any(|d| d.dirty)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.quit_prompt = true;
            if ctx.input(|i| i.viewport().minimized == Some(true)) {
                ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            ctx.request_repaint();
        }
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

    /// Override the UI Scale preference for this session (the `--ui-scale` launch option, a way
    /// back from a scale too large or small to use). The preference itself stays as saved.
    pub fn set_ui_scale_override(&mut self, percent: u32) {
        self.ui_scale_override =
            Some(percent.clamp(*UI_SCALE_RANGE.start(), *UI_SCALE_RANGE.end()));
    }

    /// The UI scale in use, in percent: the launch override if any, else the preference.
    pub fn ui_scale(&self) -> u32 {
        self.ui_scale_override.unwrap_or(self.prefs.ui_scale)
    }

    /// Whether the launch option overrides the preference.
    pub fn ui_scale_overridden(&self) -> bool {
        self.ui_scale_override.is_some()
    }

    /// Set the UI Scale preference (Preferences); it applies at once, ending a launch override.
    pub fn set_ui_scale(&mut self, percent: u32) {
        self.prefs.ui_scale = percent.clamp(*UI_SCALE_RANGE.start(), *UI_SCALE_RANGE.end());
        self.ui_scale_override = None;
    }

    pub fn set_prefs(&mut self, prefs: Prefs) {
        self.session.options.aa_style = prefs.aa_style;
        self.dialogs.set_new_doc(prefs.new_doc);
        self.dialogs.new_res_cm = prefs.new_doc_cm;
        self.prefs = prefs;
    }

    /// Whether Escape belongs to the document now: no dialog, pop-up, text field or text being
    /// edited, no stroke or live edit in progress, and not the Pen tool (Escape ends its path).
    fn escape_is_free(&self, ctx: &egui::Context) -> bool {
        !self.dialogs.any_open()
            && self.text_editor.is_none()
            && self.text_tool.editing.is_none()
            && self.text_tool.other_size.is_none()
            && self.gradient_popup.is_none()
            && self.fill_window.is_none()
            && self.stroke_window.is_none()
            && self.effects_ui.editing.is_none()
            && self.effects_ui.filter.is_none()
            && !crate::fpui::color_picker::any_open(ctx)
            && !ctx.any_popup_open()
            // A text field keeps Escape; a button with keyboard focus (Tab) doesn't.
            && !ctx.text_edit_focused()
            && self.session.tool != firecraft_engine::Tool::Pen
            && self
                .session
                .active_doc()
                .is_some_and(|d| !d.is_painting() && !d.in_live_edit())
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        // UI Scale: egui's zoom on top of the system's display scaling (1.0 = as the system
        // scales). Its own Ctrl+Plus/Minus zoom is off, so only the preference sets it.
        let zoom = self.ui_scale() as f32 / 100.0;
        if ui.ctx().options(|o| o.zoom_with_keyboard) {
            ui.ctx().options_mut(|o| o.zoom_with_keyboard = false);
        }
        if (ui.ctx().zoom_factor() - zoom).abs() > 1e-4 {
            ui.ctx().set_zoom_factor(zoom);
        }
        let mut prof = Prof::start();
        // Tool options the preferences remember.
        self.prefs.aa_style = self.session.options.aa_style;
        self.texture_library();
        if self.pending_tool_assets != tool_settings::Pending::default() {
            self.restore_tool_assets();
        }
        prof.lap("texture_library");
        self.adopt_selected_attributes();
        prof.lap("adopt_selected_attributes");
        self.refresh_fonts();
        self.installed_fonts_progress();
        text_tool::tidy(self);
        self.check_missing_fonts();
        prof.lap("fonts/text");
        let ctx = ui.ctx().clone();
        // Symbol Editors write their changes back to their symbols (instances update live).
        self.session.sync_symbols();
        prof.lap("sync_symbols");
        // An empty bitmap deselected before anything was put in it goes away (Fireworks MX).
        if let Some(d) = self.session.active_doc_mut() {
            d.drop_empty_bitmaps();
        }
        prof.lap("drop_empty_bitmaps");
        // Choosing the Pointer tool ends bitmap editing (Fireworks MX: "Exit Bitmap Mode").
        if self.session.tool != self.prev_tool {
            self.prev_tool = self.session.tool;
            if self.session.tool == firecraft_engine::Tool::Pointer
                && let Some(d) = self.session.active_doc_mut()
            {
                d.exit_bitmap_mode();
            }
        }
        // Whether Escape is free for ending bitmap editing this update, decided before anything
        // else that takes Escape gets a look at it.
        let escape_free = self.escape_is_free(&ctx);
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
        // Objects are transformed with the Interpolation preference.
        for d in &mut self.session.docs {
            d.interpolation = self.prefs.interpolation;
        }
        // Choosing another tool drops the crop box without cropping; the Hand and Zoom tools
        // keep it (Fireworks MX).
        if !matches!(
            self.session.tool,
            firecraft_engine::Tool::Crop | firecraft_engine::Tool::Hand | firecraft_engine::Tool::Zoom
        ) {
            self.crop = None;
        }
        if let Some((doc, id)) =
            ctx.data_mut(|m| m.remove_temp::<(usize, u32)>(egui::Id::unique("edit_symbol")))
        {
            self.session.edit_symbol(doc, id);
        }
        if let Some(editor) = ctx.data_mut(|m| m.remove_temp::<u64>(egui::Id::unique("symbol_done"))) {
            self.session.finish_symbol_edit(editor);
        }
        // "Read dark mode from system" follows the system's setting as it changes (by hand or
        // on a schedule), on every platform that reports it.
        let style = (
            self.prefs.theme.resolve(self.prefs.theme_follow_system, theme::system_dark(&ctx)),
            self.prefs.accent.map(|c| egui::Color32::from_rgb(c.r, c.g, c.b)),
        );
        if self.styled != Some(style) {
            theme::apply(&ctx, style.0, style.1);
            self.styled = Some(style);
        }
        theme::paint_backdrop(&ctx);
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
            // The file File > Revert asked for comes back like any opened file.
            let revert = match (&opened, &self.pending_revert) {
                (Ok((name, _)), Some((_, path))) if name == path => self.pending_revert.take(),
                (Err(_), Some(_)) => self.pending_revert.take(),
                _ => None,
            };
            match (opened, revert) {
                (Ok((_, bytes)), Some((id, path))) => self.apply_revert(id, &path, &bytes),
                (Ok((name, bytes)), None) => self.open_bytes(name, &bytes),
                (Err(e), _) => self.error(e),
            }
        }
        // Pictures the browser read from the clipboard; none come while typing text.
        let accepts = self.session.active_doc().is_some() && !ctx.egui_wants_keyboard_input();
        self.platform.set_accepts_pictures(accepts);
        menus::host_pastes(self);
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

        if self.quit_prompt {
            self.quit_dialog(&ctx);
        }

        for d in &mut self.session.docs {
            if d.history.limit != self.prefs.undo_steps {
                d.history.set_limit(self.prefs.undo_steps);
            }
        }
        prof.lap("misc");
        menus::shortcuts(self, &ctx);
        prof.lap("shortcuts");
        egui::Panel::top("menu_bar")
            .frame(theme::menu_frame())
            .show_separator_line(!theme::pal().modern)
            .show(ui, |ui| menus::menu_bar(self, ui));
        prof.lap("menu_bar");
        if self.prefs.show_tools {
            // Modern: a dark sidebar card in a 6 px gap.
            let w = if theme::pal().modern { 66.0 + 12.0 } else { 66.0 };
            egui::Panel::left("tools")
                .exact_size(w)
                .show_separator_line(!theme::pal().modern)
                .resizable(false)
                .frame(theme::panel_frame())
                .show(ui, |ui| {
                    let p = theme::pal();
                    theme::card(ui, p.tools_top, p.tools_bottom, 0, |ui| {
                        ui.set_min_height(ui.available_height());
                        tools_panel::show(self, ui);
                    });
                });
        }
        prof.lap("tools");
        let mut dock_rect = None;
        if self.prefs.show_dock {
            // A minimum width keeps a width saved from an earlier session (or a drag) from
            // squeezing the panels' text and controls.
            let dock = egui::Panel::right("dock")
                .default_size(250.0)
                .show_separator_line(!theme::pal().modern)
                .size_range(230.0..=520.0)
                .frame(theme::dock_frame())
                .show(ui, |ui| panels::dock(self, ui));
            dock_rect = Some(dock.response.rect);
        }
        prof.lap("dock");
        if self.prefs.show_properties {
            let modern = theme::pal().modern;
            let h = if self.pi_open { 128.0 } else { 22.0 } + if modern { 22.0 } else { 0.0 };
            egui::Panel::bottom("properties")
                .exact_size(h)
                .show_separator_line(!modern)
                .resizable(false)
                .frame(theme::properties_frame())
                .show(ui, |ui| {
                    let p = theme::pal();
                    theme::card(ui, p.panel, p.panel_bottom, 4, |ui| {
                        ui.set_min_size(ui.available_size());
                        inspector::show(self, ui);
                    });
                });
        }
        prof.lap("inspector");
        egui::CentralPanel::default()
            .frame(theme::workspace_frame())
            .show(ui, |ui| canvas::workspace(self, ui));
        if let Some(rect) = dock_rect {
            dock_resize_line(ui, rect);
        }
        prof.lap("canvas");
        dialogs::show(self, &ctx);
        prof.lap("dialogs");
        text_menu::dialogs(self, &ctx);
        gradient_ui::popup(self, &ctx);
        fill_ui::window(self, &ctx);
        fill_ui::stroke_window(self, &ctx);
        effects_ui::editor(self, &ctx);
        effects_ui::filter_dialog(self, &ctx);
        // Escape that nothing else wanted stops playback, or else ends bitmap editing (as in
        // Fireworks MX).
        if escape_free && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            let id = self.session.active_doc().map(|d| d.id);
            match id.and_then(|id| self.views.get_mut(&id)).filter(|v| v.playing) {
                Some(view) => view.playing = false,
                None => {
                    if let Some(d) = self.session.active_doc_mut() {
                        d.exit_bitmap_mode();
                    }
                }
            }
        }
        if let Some((msg, is_err)) = self.status.clone() {
            let color = if is_err {
                theme::pal().error
            } else {
                theme::pal().text
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

    struct NoPlatform;
    impl Platform for NoPlatform {
        fn open_file(&mut self) -> Option<(String, Vec<u8>)> {
            None
        }
        fn save_file(&mut self, _: &str, _: &[u8]) -> SaveStatus {
            SaveStatus::Cancelled
        }
    }

    #[test]
    fn ui_scale_pref_and_override() {
        assert_eq!(Prefs::default().ui_scale, 100);
        let p = Prefs {
            ui_scale: 150,
            ..Prefs::default()
        };
        assert_eq!(Prefs::parse(&p.serialize()).ui_scale, 150);
        // Out of range values are clamped, junk ignored.
        assert_eq!(Prefs::parse("ui_scale=900").ui_scale, 200);
        assert_eq!(Prefs::parse("ui_scale=abc").ui_scale, 100);
        assert_eq!(parse_ui_scale("125"), Some(125));
        assert_eq!(parse_ui_scale("125%"), Some(125));
        assert_eq!(parse_ui_scale("1.5"), Some(150));
        assert_eq!(parse_ui_scale("2"), Some(200));
        assert_eq!(parse_ui_scale("30"), None);
        assert_eq!(parse_ui_scale("nope"), None);
        let mut app = FireCraftApp::new(Box::new(NoPlatform));
        app.set_prefs(p);
        app.set_ui_scale_override(100);
        assert_eq!((app.ui_scale(), app.prefs.ui_scale), (100, 150));
        // Choosing a scale in Preferences ends the override and is saved.
        app.set_ui_scale(120);
        assert_eq!((app.ui_scale(), app.ui_scale_overridden(), app.prefs.ui_scale), (120, false, 120));
    }

    #[test]
    fn prefs_round_trip() {
        let p = Prefs {
            tabs: true,
            undo_steps: 250,
            color_math: firecraft_raster::ColorMath::Precise,
            live_drag: false,
            mouse_highlight: false,
            swatch_feedback: false,
            theme: theme::ThemeKind::ModernDark,
            theme_follow_system: false,
            accent: Some(firecraft_raster::Rgba::new(0x50, 0x40, 0xD0, 255)),
            ..Prefs::default()
        };
        assert_eq!(Prefs::parse(&p.serialize()), p);
        assert_eq!(Prefs::parse(&Prefs::default().serialize()), Prefs::default());
        // Without a choice: Modern, light or dark as the system is.
        let old = Prefs::parse("tabs=true");
        assert_eq!((old.theme, old.theme_follow_system), (theme::ThemeKind::Modern, true));
        let mut p = Prefs::default();
        // Picking a theme of the system's darkness keeps following it; the other darkness is
        // a choice of its own.
        p.choose_theme(theme::ThemeKind::Classic, Some(false));
        assert_eq!((p.theme, p.theme_follow_system), (theme::ThemeKind::Classic, true));
        p.choose_theme(theme::ThemeKind::Dark, Some(false));
        assert_eq!((p.theme, p.theme_follow_system), (theme::ThemeKind::Dark, false));
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
