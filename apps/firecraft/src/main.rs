// A GUI app on Windows (no console window). Log messages go to %TEMP%\firecraft.log there.
#![cfg_attr(windows, windows_subsystem = "windows")]

//! FireCraft desktop app (Windows, Linux, macOS).

mod splash;

use std::sync::{Arc, Mutex, PoisonError};

use firecraft_ui_egui::{FireCraftApp, Platform};

/// The splash process, closed once the main window has drawn.
type SplashHandle = Arc<Mutex<Option<std::process::Child>>>;

fn close_splash(s: &SplashHandle) {
    if let Some(child) = s.lock().unwrap_or_else(PoisonError::into_inner).take() {
        splash::close(child);
    }
}

struct Desktop;

impl Platform for Desktop {
    fn open_file(&mut self) -> Option<(String, Vec<u8>)> {
        let path = rfd::FileDialog::new()
            .add_filter("Images", &["png"])
            .add_filter("All files", &["*"])
            .pick_file()?;
        let bytes = std::fs::read(&path).ok()?;
        Some((path.display().to_string(), bytes))
    }

    fn save_file(&mut self, suggested: &str, bytes: &[u8]) -> Option<String> {
        let name = std::path::Path::new(suggested)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let path = rfd::FileDialog::new()
            .set_file_name(name)
            .add_filter("Fireworks PNG", &["png"])
            .save_file()?;
        std::fs::write(&path, bytes).ok()?;
        Some(path.display().to_string())
    }

    fn write_file(&mut self, path: &str, bytes: &[u8]) -> Result<(), String> {
        // Write to a sibling temp file and rename, so a failed save never truncates the document.
        let tmp = format!("{path}.firecraft-tmp");
        std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }
}

struct App {
    inner: FireCraftApp,
    splash: SplashHandle,
    frames: u32,
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(
            firecraft_ui_egui::Prefs::STORAGE_KEY,
            self.inner.prefs.serialize(),
        );
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.inner.ui(ui);
        // Close the splash once a frame has actually reached the screen (the second frame).
        if self.frames < 2 {
            self.frames += 1;
            if self.frames == 2 {
                close_splash(&self.splash);
            } else {
                ui.ctx().request_repaint();
            }
        }
    }
}

/// Window and renderer settings.
///
/// Renderer choice: wgpu on Vulkan/DX12/Metal first. wgpu's OpenGL path is left out by default
/// because some virtual GPUs (e.g. VirtualBox's SVGA3D driver) advertise OpenGL but fail to create
/// a context, leaving a black window. If no such adapter exists, `main` retries with the plain
/// OpenGL (glow) renderer. Override with `WGPU_BACKEND=vulkan|gl|...` or `FIRECRAFT_RENDERER=glow`.
fn native_options(renderer: eframe::Renderer) -> eframe::NativeOptions {
    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("FireCraft")
            .with_inner_size([1280.0, 860.0]),
        renderer,
        ..Default::default()
    };
    if renderer == eframe::Renderer::Wgpu && std::env::var_os("WGPU_BACKEND").is_none() {
        if let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup {
            create.instance_descriptor.backends = eframe::wgpu::Backends::PRIMARY;
        }
    }
    options
}

fn run(renderer: eframe::Renderer, files: Vec<String>, splash: SplashHandle) -> eframe::Result {
    eframe::run_native(
        "FireCraft",
        native_options(renderer),
        Box::new(move |cc| {
            let mut app = FireCraftApp::new(Box::new(Desktop));
            if let Some(text) = cc
                .storage
                .and_then(|s| s.get_string(firecraft_ui_egui::Prefs::STORAGE_KEY))
            {
                app.prefs = firecraft_ui_egui::Prefs::parse(&text);
            }
            for f in files {
                match std::fs::read(&f) {
                    Ok(bytes) => app.open_bytes(f, &bytes),
                    Err(e) => log::warn!("{f}: {e}"),
                }
            }
            Ok(Box::new(App {
                inner: app,
                splash,
                frames: 0,
            }))
        }),
    )
}

/// True when the only Vulkan/DX12/Metal adapters are software rasterizers (e.g. VirtualBox,
/// whose 3D driver offers hardware OpenGL through GLX but no hardware Vulkan).
fn primary_adapters_are_software() -> bool {
    use eframe::wgpu;
    let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
    desc.backends = wgpu::Backends::PRIMARY;
    let instance = wgpu::Instance::new(desc);
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::PRIMARY));
    adapters
        .iter()
        .all(|a| a.get_info().device_type == wgpu::DeviceType::Cpu)
}

fn main() -> eframe::Result {
    if std::env::args().nth(1).as_deref() == Some("--splash") {
        splash::run();
        return Ok(());
    }
    env_logger_init();
    let splash: SplashHandle = Arc::new(Mutex::new(splash::spawn()));
    let files: Vec<String> = std::env::args().skip(1).collect();
    let result = run_with_fallback(files, &splash);
    close_splash(&splash);
    result
}

fn run_with_fallback(files: Vec<String>, splash: &SplashHandle) -> eframe::Result {
    let forced = std::env::var("FIRECRAFT_RENDERER")
        .ok()
        .map(|v| v.to_ascii_lowercase());
    let (first, second) = match forced.as_deref() {
        Some("glow" | "gl" | "opengl") => return run(eframe::Renderer::Glow, files, splash.clone()),
        Some("wgpu") => return run(eframe::Renderer::Wgpu, files, splash.clone()),
        _ if std::env::var_os("WGPU_BACKEND").is_some() => (eframe::Renderer::Wgpu, eframe::Renderer::Glow),
        // No hardware Vulkan/DX12/Metal: OpenGL (glow) can still use the GPU; software wgpu is the fallback.
        _ if primary_adapters_are_software() => (eframe::Renderer::Glow, eframe::Renderer::Wgpu),
        _ => (eframe::Renderer::Wgpu, eframe::Renderer::Glow),
    };
    match run(first, files.clone(), splash.clone()) {
        Ok(()) => Ok(()),
        Err(e) => {
            log::warn!("{first:?} renderer unavailable ({e}); retrying with {second:?}");
            run(second, files, splash.clone())
        }
    }
}

fn env_logger_init() {
    // Minimal logger without extra dependencies: FireCraft's own warnings, and errors from any
    // crate (graphics drivers' informational warnings, e.g. the Vulkan loader's registry probes on
    // Windows, are dropped). Set FIRECRAFT_LOG=all to see every warning.
    struct L {
        all: bool,
        file: Option<std::sync::Mutex<std::fs::File>>,
    }
    impl log::Log for L {
        fn enabled(&self, m: &log::Metadata) -> bool {
            m.level() <= log::Level::Error
                || (m.level() <= log::Level::Warn && (self.all || m.target().starts_with("firecraft")))
        }
        fn log(&self, r: &log::Record) {
            if !self.enabled(r.metadata()) {
                return;
            }
            let line = format!("[{}] {}\n", r.level(), r.args());
            match &self.file {
                Some(f) => {
                    use std::io::Write;
                    let _ = f
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .write_all(line.as_bytes());
                }
                None => eprint!("{line}"),
            }
        }
        fn flush(&self) {}
    }
    // A Windows GUI app has no console: log to a file instead.
    let file = if cfg!(windows) {
        std::fs::File::create(std::env::temp_dir().join("firecraft.log"))
            .ok()
            .map(std::sync::Mutex::new)
    } else {
        None
    };
    let all = std::env::var("FIRECRAFT_LOG").is_ok_and(|v| v == "all");
    let logger = Box::leak(Box::new(L { all, file }));
    let _ = log::set_logger(logger).map(|()| log::set_max_level(log::LevelFilter::Warn));
}
