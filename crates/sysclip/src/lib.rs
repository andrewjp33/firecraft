//! The system clipboard for copied objects, in the format Fireworks MX uses.
//!
//! Fireworks MX copies objects as a clipboard format named "PNG" holding a complete Fireworks
//! PNG (see `firecraft_io::clipboard_png`) and pastes that format back as editable objects.
//! Under Wine, "PNG" is the X11 target `image/png`, so on Linux FireCraft exchanges objects
//! with a Fireworks running in Wine through the X11 CLIPBOARD (also under Wayland, through
//! XWayland, where Wine runs). On Windows it's the registered format "PNG". Other
//! applications see an ordinary PNG image.
//!
//! Where no system clipboard is reachable (the browser, Wayland without XWayland) these do
//! nothing and FireCraft keeps copied objects to itself.

#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
mod x11;
#[cfg(all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))]
use x11 as platform;

#[cfg(windows)]
mod win;
#[cfg(windows)]
use win as platform;

#[cfg(not(any(windows, all(unix, not(target_os = "macos"), not(target_arch = "wasm32")))))]
mod platform {
    pub fn available() -> bool {
        false
    }
    pub fn set_png(_data: Vec<u8>) -> bool {
        false
    }
    pub fn get_png() -> Option<Vec<u8>> {
        None
    }
    pub fn owns() -> bool {
        false
    }
    pub fn get_text() -> Option<String> {
        None
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

/// The PNG on the system clipboard (from Fireworks, FireCraft or any application), if any.
pub fn get_png() -> Option<Vec<u8>> {
    platform::get_png()
}

/// The text on the system clipboard (for text fields: FireCraft handles Ctrl+V itself).
pub fn get_text() -> Option<String> {
    platform::get_text()
}

/// Whether the clipboard still holds what FireCraft last put there (nothing copied since).
pub fn owns() -> bool {
    platform::owns()
}
