//! Windows: the registered clipboard format "PNG" (written and read); pictures are also read
//! from `CF_DIB`, `CF_DIBV5` and "JFIF".

use std::sync::atomic::{AtomicU32, Ordering};

use clipboard_win::{Clipboard, raw};

const CF_DIB: u32 = 8;
const CF_DIBV5: u32 = 17;

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

/// Picture formats, most preferred first: "PNG" carries Fireworks objects (and real alpha from
/// browsers and image editors); `CF_DIB` is what Print Screen and most programs put there.
/// `CF_DIB` comes before `CF_DIBV5` because a 32-bit `CF_DIB` has no alpha to misread, while
/// Windows may give a `CF_DIBV5` made from it an alpha mask over leftover bytes.
pub fn get_image() -> Option<Vec<u8>> {
    let png = format();
    let jfif = clipboard_win::register_format("JFIF").map(|f| f.get());
    let _open = Clipboard::new_attempts(10).ok()?;
    [png, Some(CF_DIB), Some(CF_DIBV5), jfif]
        .into_iter()
        .flatten()
        .filter(|&f| raw::is_format_avail(f))
        .find_map(|f| {
            let mut out = Vec::new();
            raw::get_vec(f, &mut out).ok()?;
            (!out.is_empty()).then_some(out)
        })
}

pub fn get_text() -> Option<String> {
    clipboard_win::get_clipboard_string().ok()
}

pub fn owns() -> bool {
    let ours = OURS.load(Ordering::Relaxed);
    ours != 0 && raw::seq_num().is_some_and(|n| n.get() == ours)
}
