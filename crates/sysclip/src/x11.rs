//! X11 (also under Wayland through XWayland): the CLIPBOARD selection. FireCraft offers
//! `image/png`; it reads `image/png`, `image/bmp` or `image/jpeg`.
//!
//! A background thread owns the selection while FireCraft holds the clipboard and answers
//! requests (TARGETS, TIMESTAMP, image/png; large data with the INCR protocol). Reading uses a
//! short-lived connection.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use x11rb::connection::{Connection, RequestConnection as _};
use x11rb::protocol::Event;
use x11rb::protocol::xproto::{
    AtomEnum, ChangeWindowAttributesAux, ConnectionExt as _, CreateWindowAux, EventMask, PropMode, Property,
    SELECTION_NOTIFY_EVENT, SelectionNotifyEvent, SelectionRequestEvent, Window, WindowClass,
};
use x11rb::rust_connection::RustConnection;
use x11rb::wrapper::ConnectionExt as _;
use x11rb::{CURRENT_TIME, NONE};

#[derive(Clone, Copy)]
struct Atoms {
    clipboard: u32,
    targets: u32,
    timestamp: u32,
    incr: u32,
    png: u32,
    property: u32,
}

fn atoms(conn: &RustConnection) -> Option<Atoms> {
    let get = |name: &str| -> Option<u32> {
        Some(conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok()?.atom)
    };
    Some(Atoms {
        clipboard: get("CLIPBOARD")?,
        targets: get("TARGETS")?,
        timestamp: get("TIMESTAMP")?,
        incr: get("INCR")?,
        png: get("image/png")?,
        property: get("FIRECRAFT_CLIPBOARD")?,
    })
}

/// The selection owner: its connection, window and the data it serves.
struct Owner {
    conn: Arc<RustConnection>,
    window: Window,
    atoms: Atoms,
    data: Arc<Mutex<Option<Arc<Vec<u8>>>>>,
}

static OWNER: OnceLock<Option<Owner>> = OnceLock::new();

fn owner() -> Option<&'static Owner> {
    OWNER
        .get_or_init(|| {
            std::env::var_os("DISPLAY")?;
            let (conn, screen) = RustConnection::connect(None).ok()?;
            let conn = Arc::new(conn);
            let root = conn.setup().roots.get(screen)?.root;
            let window = conn.generate_id().ok()?;
            conn.create_window(
                0,
                window,
                root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new(),
            )
            .ok()?;
            let atoms = atoms(&conn)?;
            conn.flush().ok()?;
            let data: Arc<Mutex<Option<Arc<Vec<u8>>>>> = Arc::default();
            let (c, d) = (conn.clone(), data.clone());
            std::thread::Builder::new()
                .name("firecraft-clipboard".into())
                .spawn(move || serve(&c, window, atoms, &d))
                .ok()?;
            Some(Owner {
                conn,
                window,
                atoms,
                data,
            })
        })
        .as_ref()
}

/// An INCR transfer in progress: the data and how much was sent.
struct Transfer {
    property: u32,
    data: Arc<Vec<u8>>,
    sent: usize,
}

fn serve(conn: &RustConnection, window: Window, a: Atoms, data: &Mutex<Option<Arc<Vec<u8>>>>) {
    // Larger replies go in pieces (INCR).
    let chunk = (conn.maximum_request_bytes() / 4).clamp(4096, 256 * 1024);
    let mut transfers: HashMap<Window, Transfer> = HashMap::new();
    while let Ok(event) = conn.wait_for_event() {
        match event {
            Event::SelectionClear(e) if e.selection == a.clipboard => {
                if let Ok(mut d) = data.lock() {
                    *d = None;
                }
            }
            Event::SelectionRequest(req) => {
                let current = data.lock().ok().and_then(|d| d.clone());
                let property = answer(conn, &req, a, current, chunk, &mut transfers);
                let notify = SelectionNotifyEvent {
                    response_type: SELECTION_NOTIFY_EVENT,
                    sequence: 0,
                    time: req.time,
                    requestor: req.requestor,
                    selection: req.selection,
                    target: req.target,
                    property,
                };
                let _ = conn.send_event(false, req.requestor, EventMask::NO_EVENT, notify);
                let _ = conn.flush();
            }
            // The requestor took a piece of an INCR transfer: send the next (empty = done).
            Event::PropertyNotify(e) if e.state == Property::DELETE => {
                let Some(t) = transfers.get_mut(&e.window) else {
                    continue;
                };
                if t.property != e.atom {
                    continue;
                }
                let end = (t.sent + chunk).min(t.data.len());
                let piece = t.data.get(t.sent..end).unwrap_or(&[]);
                let _ = conn.change_property8(PropMode::REPLACE, e.window, t.property, a.png, piece);
                if piece.is_empty() {
                    transfers.remove(&e.window);
                } else {
                    t.sent = end;
                }
                let _ = conn.flush();
            }
            _ => {}
        }
    }
    let _ = window;
}

/// Store the requested target on the requestor's property; the property used, or NONE.
fn answer(
    conn: &RustConnection,
    req: &SelectionRequestEvent,
    a: Atoms,
    data: Option<Arc<Vec<u8>>>,
    chunk: usize,
    transfers: &mut HashMap<Window, Transfer>,
) -> u32 {
    // Obsolete clients leave the property empty: use the target's name.
    let property = if req.property == NONE {
        req.target
    } else {
        req.property
    };
    let Some(data) = data.filter(|_| req.selection == a.clipboard) else {
        return NONE;
    };
    let ok = if req.target == a.targets {
        conn.change_property32(
            PropMode::REPLACE,
            req.requestor,
            property,
            AtomEnum::ATOM,
            &[a.targets, a.timestamp, a.png],
        )
        .is_ok()
    } else if req.target == a.timestamp {
        conn.change_property32(
            PropMode::REPLACE,
            req.requestor,
            property,
            AtomEnum::INTEGER,
            &[CURRENT_TIME],
        )
        .is_ok()
    } else if req.target == a.png {
        if data.len() <= chunk {
            conn.change_property8(PropMode::REPLACE, req.requestor, property, a.png, &data)
                .is_ok()
        } else {
            // INCR: announce the size, then send pieces as the requestor deletes the property.
            let watch = ChangeWindowAttributesAux::new().event_mask(EventMask::PROPERTY_CHANGE);
            let _ = conn.change_window_attributes(req.requestor, &watch);
            let size = u32::try_from(data.len()).unwrap_or(u32::MAX);
            transfers.insert(
                req.requestor,
                Transfer {
                    property,
                    data,
                    sent: 0,
                },
            );
            conn.change_property32(PropMode::REPLACE, req.requestor, property, a.incr, &[size])
                .is_ok()
        }
    } else {
        false
    };
    if ok { property } else { NONE }
}

pub fn available() -> bool {
    owner().is_some()
}

pub fn set_png(data: Vec<u8>) -> bool {
    let Some(o) = owner() else { return false };
    if let Ok(mut d) = o.data.lock() {
        *d = Some(Arc::new(data));
    }
    let set = o
        .conn
        .set_selection_owner(o.window, o.atoms.clipboard, CURRENT_TIME)
        .is_ok()
        && o.conn.flush().is_ok();
    set && owns()
}

pub fn owns() -> bool {
    let Some(o) = owner() else { return false };
    let current = o
        .conn
        .get_selection_owner(o.atoms.clipboard)
        .ok()
        .and_then(|c| c.reply().ok());
    current.is_some_and(|r| r.owner == o.window)
}

/// Wait (briefly) for an event matching `f` on `conn`.
fn wait<T>(conn: &RustConnection, until: Instant, mut f: impl FnMut(&Event) -> Option<T>) -> Option<T> {
    while Instant::now() < until {
        match conn.poll_for_event() {
            Ok(Some(e)) => {
                if let Some(v) = f(&e) {
                    return Some(v);
                }
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(5)),
            Err(_) => return None,
        }
    }
    None
}

/// Picture targets, most preferred first: `image/png` carries Fireworks objects (and is what
/// most programs offer); Wine offers a Windows program's `CF_DIB` as `image/bmp`.
const IMAGE_TARGETS: [&str; 5] = ["image/png", "image/bmp", "image/x-bmp", "image/x-MS-bmp", "image/jpeg"];

pub fn get_image() -> Option<Vec<u8>> {
    let Some(offered) = targets() else {
        // An owner that can't list its targets: try the usual one.
        return get("image/png");
    };
    IMAGE_TARGETS
        .iter()
        .filter(|t| offered.iter().any(|o| o == *t))
        .find_map(|t| get(t))
}

/// The names of the targets the clipboard's owner offers.
fn targets() -> Option<Vec<String>> {
    let raw = get("TARGETS")?;
    let (conn, _) = RustConnection::connect(None).ok()?;
    let names = raw
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_ne_bytes(*c))
        .filter_map(|atom| {
            let reply = conn.get_atom_name(atom).ok()?.reply().ok()?;
            String::from_utf8(reply.name).ok()
        })
        .collect();
    Some(names)
}

pub fn copied_files() -> Vec<std::path::PathBuf> {
    get("text/uri-list").map_or_else(Vec::new, |b| file_uris(&b))
}

/// The local files in a `text/uri-list` (RFC 2483: one URI per line, `#` comments).
fn file_uris(list: &[u8]) -> Vec<std::path::PathBuf> {
    use std::os::unix::ffi::OsStringExt as _;
    String::from_utf8_lossy(list)
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|uri| {
            let rest = uri.strip_prefix("file://")?;
            // An empty host or localhost; other hosts aren't local files.
            let path = rest.strip_prefix("localhost").unwrap_or(rest);
            path.starts_with('/')
                .then(|| std::path::PathBuf::from(std::ffi::OsString::from_vec(percent_decode(path))))
        })
        .collect()
}

fn percent_decode(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while let Some(&c) = b.get(i) {
        let hex = b.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
        match (c, hex) {
            (b'%', Some(v)) => {
                out.push(v);
                i += 3;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

pub fn get_text() -> Option<String> {
    get("UTF8_STRING").and_then(|b| String::from_utf8(b).ok())
}

/// The clipboard's data for `target`.
fn get(target: &str) -> Option<Vec<u8>> {
    std::env::var_os("DISPLAY")?;
    let (conn, screen) = RustConnection::connect(None).ok()?;
    let root = conn.setup().roots.get(screen)?.root;
    let a = atoms(&conn)?;
    let window = conn.generate_id().ok()?;
    let aux = CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE);
    conn.create_window(0, window, root, 0, 0, 1, 1, 0, WindowClass::INPUT_OUTPUT, 0, &aux)
        .ok()?;
    let target = conn
        .intern_atom(false, target.as_bytes())
        .ok()?
        .reply()
        .ok()?
        .atom;
    conn.convert_selection(window, a.clipboard, target, a.property, CURRENT_TIME)
        .ok()?;
    conn.flush().ok()?;
    let until = Instant::now() + Duration::from_millis(1500);
    let property = wait(&conn, until, |e| match e {
        Event::SelectionNotify(n) if n.requestor == window => Some(n.property),
        _ => None,
    })?;
    if property == NONE {
        return None;
    }
    let take = |conn: &RustConnection| {
        conn.get_property(true, window, a.property, AtomEnum::ANY, 0, u32::MAX / 4)
            .ok()?
            .reply()
            .ok()
    };
    let first = take(&conn)?;
    conn.flush().ok()?;
    if first.type_ != a.incr {
        return (!first.value.is_empty()).then_some(first.value);
    }
    // INCR: pieces arrive as new values of the property until an empty one.
    let mut out = Vec::new();
    let until = Instant::now() + Duration::from_secs(10);
    loop {
        wait(&conn, until, |e| match e {
            Event::PropertyNotify(p)
                if p.window == window && p.atom == a.property && p.state == Property::NEW_VALUE =>
            {
                Some(())
            }
            _ => None,
        })?;
        let piece = take(&conn)?;
        conn.flush().ok()?;
        if piece.value.is_empty() {
            break;
        }
        out.extend_from_slice(&piece.value);
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uri_lists_give_local_paths() {
        let list = b"# copied\r\nfile:///home/me/My%20Shots/a%C3%A9.png\r\nfile://localhost/tmp/b.jpg\r\nhttp://x/c.png\r\nfile://other/d.png\r\n";
        assert_eq!(
            file_uris(list),
            vec![
                std::path::PathBuf::from("/home/me/My Shots/a\u{e9}.png"),
                std::path::PathBuf::from("/tmp/b.jpg"),
            ]
        );
    }
}

/// X11: `image/png` only (browsers and Fireworks under Wine read it; Wine makes a DIB from it).
pub fn set_png_and_dib(png: Vec<u8>, _dib: Vec<u8>) -> bool {
    set_png(png)
}
