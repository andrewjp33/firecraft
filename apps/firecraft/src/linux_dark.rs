//! Linux: the desktop's dark mode, which winit doesn't report (never on X11; on Wayland only its
//! window decorations' theme). Read from the same places desktop apps use, without new
//! dependencies: the XDG desktop portal (GNOME 42+, KDE Plasma 5.24+, and others), GNOME's
//! color-scheme setting, and lastly the desktop's GTK theme name (MATE, Cinnamon, Xfce...,
//! which have no dark-mode switch of their own: "Yaru-dark", "BlackMATE"). Checked again every
//! 15 seconds, so schedules and manual switches are followed. A check runs two or three short
//! commands (about 10 ms of CPU together).

use std::process::{Command, Stdio};
use std::sync::{Mutex, Once};
use std::time::Duration;

use eframe::egui;
use firecraft_ui_egui::theme;

/// The context to repaint when the setting changes (the newest, if the app was started again
/// with another renderer).
static CTX: Mutex<Option<egui::Context>> = Mutex::new(None);

/// Start watching the setting. Waits a moment for the first reading, so the first frame
/// usually has it, but never holds up startup for long (a portal that must be started first
/// can take a second or two; the theme then switches as soon as it answers).
pub fn start(ctx: &egui::Context) {
    *CTX.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(ctx.clone());
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        let (first_tx, first_rx) = std::sync::mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("dark-mode".into())
            .spawn(move || {
                let mut last = read();
                theme::set_system_dark(last);
                let _ = first_tx.send(());
                repaint();
                loop {
                    std::thread::sleep(Duration::from_secs(15));
                    let now = read();
                    if now != last {
                        last = now;
                        theme::set_system_dark(now);
                        repaint();
                    }
                }
            });
        if spawned.is_ok() {
            let _ = first_rx.recv_timeout(Duration::from_millis(300));
        }
    });
}

fn repaint() {
    if let Some(ctx) = CTX.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref() {
        ctx.request_repaint();
    }
}

/// Dark (`Some(true)`), light, or `None` when the desktop doesn't say.
fn read() -> Option<bool> {
    // The portal's org.freedesktop.appearance color-scheme: 1 prefer dark, 2 prefer light,
    // 0 no preference.
    match portal_color_scheme() {
        Some(1) => return Some(true),
        Some(2) => return Some(false),
        _ => {}
    }
    if let Some(s) = gsettings("org.gnome.desktop.interface", "color-scheme") {
        if s.contains("prefer-dark") {
            return Some(true);
        }
        if s.contains("prefer-light") {
            return Some(false);
        }
    }
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_ascii_lowercase();
    let name = if desktop.contains("mate") {
        gsettings("org.mate.interface", "gtk-theme")
    } else if desktop.contains("cinnamon") {
        gsettings("org.cinnamon.desktop.interface", "gtk-theme")
    } else if desktop.contains("xfce") {
        output("xfconf-query", &["-c", "xsettings", "-p", "/Net/ThemeName"])
    } else {
        gsettings("org.gnome.desktop.interface", "gtk-theme")
    };
    name.or_else(|| std::env::var("GTK_THEME").ok())
        .filter(|n| !n.trim().is_empty())
        .map(|n| dark_theme_name(&n))
}

/// A GTK theme named as a dark one ("Adwaita-dark", "Yaru-bark-dark", "BlackMATE").
fn dark_theme_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("dark") || n.contains("black")
}

fn portal_color_scheme() -> Option<u32> {
    let out = output(
        "gdbus",
        &[
            "call",
            "--session",
            "--timeout",
            "2",
            "--dest",
            "org.freedesktop.portal.Desktop",
            "--object-path",
            "/org/freedesktop/portal/desktop",
            "--method",
            "org.freedesktop.portal.Settings.Read",
            "org.freedesktop.appearance",
            "color-scheme",
        ],
    )?;
    // "(<<uint32 1>>,)"
    let rest = &out[out.find("uint32 ")? + 7..];
    rest.chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

fn gsettings(schema: &str, key: &str) -> Option<String> {
    output("gsettings", &["get", schema, key])
}

/// A command's output, `None` if it isn't installed or fails.
fn output(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).trim().trim_matches('\'').to_owned())
}

#[cfg(test)]
mod tests {
    use super::dark_theme_name;

    #[test]
    fn dark_theme_names() {
        assert!(dark_theme_name("Adwaita-dark"));
        assert!(dark_theme_name("Yaru-bark-dark"));
        assert!(dark_theme_name("BlackMATE"));
        assert!(!dark_theme_name("Yaru-bark"));
        assert!(!dark_theme_name("Yaru-MATE-light"));
    }
}
