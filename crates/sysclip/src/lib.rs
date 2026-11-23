//! The system clipboard for copied objects, in the format Fireworks MX uses.
//!
//! Fireworks MX copies objects as a clipboard format named "PNG" holding a complete Fireworks
//! PNG (see `firecraft_io::clipboard_png`) and pastes that format back as editable objects.
//! Under Wine, "PNG" is the X11 target `image/png`, so on Linux FireCraft exchanges objects
//! with a Fireworks running in Wine through the X11 CLIPBOARD (also under Wayland, through
//! XWayland, where Wine runs). On Windows it's the registered format "PNG". Other
//! applications see an ordinary PNG image.
//!
//! On macOS it's the general pasteboard's `public.png` (there is no Fireworks MX to exchange
//! objects with there, but other applications see the PNG picture).
//!
//! Where no system clipboard is reachable (the browser, Wayland without XWayland) these do
//! nothing and FireCraft keeps copied objects to itself.
//!
//! Pictures other programs copy (screenshots, browser images) are read too: as a DIB on
//! Windows, `image/png`/`image/bmp`/`image/jpeg` on X11, PNG/TIFF/JPEG on macOS.

#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
mod x11;
#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
use x11 as platform;

#[cfg(windows)]
mod win;
#[cfg(windows)]
use win as platform;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
use mac as platform;

#[cfg(not(any(
    windows,
    target_os = "macos",
    all(unix, not(target_os = "macos"), not(target_arch = "wasm32"))
)))]
mod platform {
    pub fn available() -> bool {
        false
    }
    pub fn set_png(_data: Vec<u8>) -> bool {
        false
    }
    pub fn get_image() -> Option<Vec<u8>> {
        None
    }
    pub fn owns() -> bool {
        false
    }
    pub fn get_text() -> Option<String> {
        None
    }
    pub fn copied_files() -> Vec<std::path::PathBuf> {
        Vec::new()
    }
}

/// Whether a system clipboard is reachable.
pub fn available() -> bool {
    platform::available()
}

/// Put a (Fireworks) PNG on the system clipboard. False if that wasn't possible.
pub fn set_png(data: Vec<u8>) -> bool {
    platform::set_png(data)
}

/// The picture on the system clipboard, if any: a PNG (Fireworks objects from Fireworks or
/// FireCraft, or a plain picture), else a DIB (Windows `CF_DIB`/`CF_DIBV5`), a BMP file, a
/// TIFF (macOS) or a JPEG, as the clipboard holds it (`firecraft_io::read_clipboard` reads them all).
pub fn get_image() -> Option<Vec<u8>> {
    platform::get_image()
}

/// The files copied in a file manager (Explorer's `CF_HDROP`, X11 `text/uri-list` from
/// Nautilus, Dolphin, Wine..., the Finder's file URLs), if that's what the clipboard holds.
pub fn copied_files() -> Vec<std::path::PathBuf> {
    platform::copied_files()
}

/// The text on the system clipboard (for text fields: FireCraft handles Ctrl+V itself).
pub fn get_text() -> Option<String> {
    platform::get_text()
}

/// Whether the clipboard still holds what FireCraft last put there (nothing copied since).
pub fn owns() -> bool {
    platform::owns()
}
