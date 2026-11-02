//! Windows: the registered clipboard format "PNG".

use std::sync::atomic::{AtomicU32, Ordering};

use clipboard_win::{Clipboard, raw};

/// The clipboard's sequence number after FireCraft's last copy.
static OURS: AtomicU32 = AtomicU32::new(0);

fn format() -> Option<u32> {
    clipboard_win::register_format("PNG").map(|f| f.get())
}

pub fn available() -> bool {
    true
}

pub fn set_png(data: Vec<u8>) -> bool {
    let Some(fmt) = format() else { return false };
    let Ok(_open) = Clipboard::new_attempts(10) else {
        return false;
    };
    let ok = raw::set(fmt, &data).is_ok();
    if ok {
        OURS.store(raw::seq_num().map_or(0, |n| n.get()), Ordering::Relaxed);
    }
    ok
}

pub fn get_png() -> Option<Vec<u8>> {
    let fmt = format()?;
    let _open = Clipboard::new_attempts(10).ok()?;
    if !raw::is_format_avail(fmt) {
        return None;
    }
    let mut out = Vec::new();
    raw::get_vec(fmt, &mut out).ok()?;
    (!out.is_empty()).then_some(out)
}

pub fn get_text() -> Option<String> {
    clipboard_win::get_clipboard_string().ok()
}

pub fn owns() -> bool {
    let ours = OURS.load(Ordering::Relaxed);
    ours != 0 && raw::seq_num().is_some_and(|n| n.get() == ours)
}
