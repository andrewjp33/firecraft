//! macOS: the general pasteboard (NSPasteboard). FireCraft writes `public.png` (a Fireworks
//! PNG, which other applications read as an ordinary picture); it reads `public.png`, else
//! `public.tiff` (what many Mac applications copy pictures as), else `public.jpeg`; text as
//! `public.utf8-plain-text`; files copied in the Finder as `public.file-url` items.
//!
//! The pasteboard types are written as their UTI strings, which are what AppKit's
//! `NSPasteboardType…` constants hold.

use std::sync::atomic::{AtomicIsize, Ordering};

use objc2_app_kit::NSPasteboard;
use objc2_foundation::{NSData, NSString, NSURL};

const PNG: &str = "public.png";
const TIFF: &str = "public.tiff";
const JPEG: &str = "public.jpeg";
const TEXT: &str = "public.utf8-plain-text";
const HTML: &str = "public.html";
const FILE_URL: &str = "public.file-url";

/// The pasteboard's change count after FireCraft's last copy (-1: never copied).
static OURS: AtomicIsize = AtomicIsize::new(-1);

pub fn available() -> bool {
    true
}

pub fn set_png(data: Vec<u8>) -> bool {
    let pb = NSPasteboard::generalPasteboard();
    pb.clearContents();
    let ok = pb.setData_forType(Some(&NSData::with_bytes(&data)), &NSString::from_str(PNG));
    if ok {
        OURS.store(pb.changeCount(), Ordering::Relaxed);
    }
    ok
}

/// Picture types, most preferred first: PNG carries Fireworks objects (and real alpha).
pub fn get_image() -> Option<Vec<u8>> {
    let pb = NSPasteboard::generalPasteboard();
    [PNG, TIFF, JPEG].into_iter().find_map(|t| {
        let data = pb.dataForType(&NSString::from_str(t))?.to_vec();
        (!data.is_empty()).then_some(data)
    })
}

pub fn owns() -> bool {
    let ours = OURS.load(Ordering::Relaxed);
    ours >= 0 && NSPasteboard::generalPasteboard().changeCount() == ours
}

pub fn get_text() -> Option<String> {
    NSPasteboard::generalPasteboard()
        .stringForType(&NSString::from_str(TEXT))
        .map(|s| s.to_string())
}

pub fn get_html() -> Option<Vec<u8>> {
    let data = NSPasteboard::generalPasteboard().dataForType(&NSString::from_str(HTML))?.to_vec();
    (!data.is_empty()).then_some(data)
}

pub fn copied_files() -> Vec<std::path::PathBuf> {
    let Some(items) = NSPasteboard::generalPasteboard().pasteboardItems() else {
        return Vec::new();
    };
    let kind = NSString::from_str(FILE_URL);
    items
        .iter()
        .filter_map(|item| {
            let text = item.stringForType(&kind)?;
            let url = NSURL::URLWithString(&text)?;
            // Finder may give a file reference URL (file:///.file/id=...): resolve it to a path.
            let path = url.filePathURL()?.path()?;
            Some(std::path::PathBuf::from(path.to_string()))
        })
        .collect()
}

/// macOS: `public.png` only.
pub fn set_png_and_dib(png: Vec<u8>, _dib: Vec<u8>) -> bool {
    set_png(png)
}
