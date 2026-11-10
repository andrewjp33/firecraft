//! FireCraft in the browser: the same app compiled to WebAssembly (`trunk build --release`).
//!
//! File > Open and Save use the browser's file pickers; a file that came from one is saved back
//! in place, as on the desktop. Browsers without them (Firefox, Safari, plain http) get an upload
//! dialog for Open and a name prompt plus download for Save. Files can also be dragged onto the page.

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::{Cell, RefCell};
    use std::collections::{HashMap, HashSet};
    use std::rc::Rc;

    use firecraft_ui_egui::{FireCraftApp, FolderKind, Platform, SaveStatus};
    use js_sys::{Promise, Reflect, Uint8Array};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::{JsFuture, spawn_local};

    // Each function calls its picker before its first `await`, so the browser still sees the
    // click or key press that asked for it.
    #[wasm_bindgen(inline_js = r#"
const TYPES = [{ description: "Fireworks PNG", accept: { "image/png": [".png"] } }];
const IMAGE_TYPES = [{ description: "Images", accept: { "image/png": [".png"], "image/gif": [".gif"] } }];
// Everything File > Open reads (firecraft_io::formats::OPEN_EXTENSIONS).
const OPEN_TYPES = [{
  description: "Images",
  accept: {
    "image/png": [".png"],
    "image/jpeg": [".jpg", ".jpeg", ".jpe", ".jfif"],
    "image/gif": [".gif"],
    "image/bmp": [".bmp"],
    "image/tiff": [".tif", ".tiff"],
    "image/x-tga": [".tga"],
    "image/webp": [".webp"],
    "image/jxl": [".jxl"],
  },
}];

// null when the user cancelled.
async function orCancelled(picker) {
  try {
    return await picker;
  } catch (e) {
    if (e.name === "AbortError") return null;
    throw e;
  }
}

function chooseUpload(types) {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = Object.entries(types[0].accept).flatMap(([mime, exts]) => [mime, ...exts]).join(",");
    input.onchange = () => resolve(input.files[0] ?? null);
    input.oncancel = () => resolve(null);
    input.click();
  });
}

async function write(handle, bytes) {
  const out = await handle.createWritable();
  try {
    await out.write(bytes);
    await out.close();
  } catch (e) {
    await out.abort();
    throw e;
  }
}

// { name, bytes, handle? }, or null when cancelled.
export async function pickAndRead(images) {
  const types = images ? IMAGE_TYPES : OPEN_TYPES;
  let handle, file;
  if (window.showOpenFilePicker) {
    const picked = await orCancelled(window.showOpenFilePicker({ types }));
    if (!picked) return null;
    handle = picked[0];
    file = await handle.getFile();
  } else {
    file = await chooseUpload(types);
    if (!file) return null;
  }
  return { name: file.name, bytes: new Uint8Array(await file.arrayBuffer()), handle };
}

function chooseFolder() {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.webkitdirectory = true;
    input.onchange = () => resolve(Array.from(input.files));
    input.oncancel = () => resolve(null);
    input.click();
  });
}

const storedFolder = async (kind, create) =>
  (await navigator.storage.getDirectory()).getDirectoryHandle(kind, { create });

// Keep a private copy in the browser's storage so the folder survives a reload. Best effort:
// without it (e.g. plain http) the folder lasts until the page closes.
async function store(kind, files) {
  try {
    const root = await navigator.storage.getDirectory();
    await root.removeEntry(kind, { recursive: true }).catch(() => {});
    const dir = await storedFolder(kind, true);
    for (const { name, bytes } of files) {
      const out = await (await dir.getFileHandle(name, { create: true })).createWritable();
      await out.write(bytes);
      await out.close();
    }
  } catch (e) {
    console.warn("Could not keep the folder in browser storage", e);
  }
}

// What each kind of folder is read for.
const IMAGE_FILE = /\.(png|gif|jpe?g|bmp|tiff?)$/i;
const FONT_FILE = /\.(ttf|otf|ttc)$/i;
const readAll = (files) =>
  Promise.all(files.map(async (f) => ({ name: f.name, bytes: new Uint8Array(await f.arrayBuffer()) })));

// The images (or fonts) directly inside a chosen folder: { folder, files: [{ name, bytes }] }; null when
// cancelled.
export async function pickFolder(kind) {
  const chosen = await chooseFolder();
  if (!chosen) return null;
  // webkitRelativePath is "Folder/file": two parts for files directly inside it.
  const fonts = kind === "firecraft-fonts";
  const wanted = fonts ? FONT_FILE : IMAGE_FILE;
  const direct = chosen.filter((f) => f.webkitRelativePath.split("/").length === 2 && wanted.test(f.name));
  if (direct.length === 0) throw new Error(`That folder has no ${fonts ? "fonts" : "images"} in it.`);
  const files = await readAll(direct);
  await store(kind, files);
  return { folder: direct[0].webkitRelativePath.split("/")[0], files };
}

// The computer's installed fonts (Chrome and Edge, with the visitor's permission).
let localFonts = [];
export const canUseLocalFonts = () => !!window.queryLocalFonts;
export async function localFontsGranted() {
  try {
    return window.queryLocalFonts && (await navigator.permissions.query({ name: "local-fonts" })).state === "granted";
  } catch {
    return false;
  }
}
// What's on offer, without reading any font's data: { families, names }.
export async function queryLocalFontNames() {
  localFonts = await window.queryLocalFonts();
  if (localFonts.length === 0) throw new Error("The browser didn't share any fonts. Check the site's font permission.");
  return {
    families: [...new Set(localFonts.map((f) => f.family))],
    names: [...new Set(localFonts.flatMap((f) => [f.family, f.postscriptName, f.fullName]))],
  };
}
// Every style of the family with this name: [{ name, bytes }].
export async function readLocalFont(name) {
  const match = localFonts.find((f) => f.family === name || f.postscriptName === name || f.fullName === name);
  if (!match) return [];
  const faces = localFonts.filter((f) => f.family === match.family);
  return Promise.all(faces.map(async (f) => ({
    name: `${f.postscriptName}.ttf`,
    bytes: new Uint8Array(await (await f.blob()).arrayBuffer()),
  })));
}

// The stored copy of a folder: [{ name, bytes }], empty if there is none.
export async function storedFiles(kind) {
  try {
    const files = [];
    for await (const handle of (await storedFolder(kind, false)).values()) files.push(await handle.getFile());
    return await readAll(files);
  } catch {
    return [];
  }
}

export function canPickSave() {
  return !!window.showSaveFilePicker;
}

// Save picker types for an export, from its file name's extension.
const EXPORT_TYPES = {
  gif: { description: "GIF image", accept: { "image/gif": [".gif"] } },
  jpg: { description: "JPEG image", accept: { "image/jpeg": [".jpg", ".jpeg"] } },
  webp: { description: "WebP image", accept: { "image/webp": [".webp"] } },
  jxl: { description: "JPEG XL image", accept: { "image/jxl": [".jxl"] } },
};
function typesFor(name) {
  const ext = (name.split(".").pop() || "").toLowerCase();
  return EXPORT_TYPES[ext] ? [EXPORT_TYPES[ext]] : TYPES;
}

// The chosen handle, or null when cancelled.
export async function pickAndWrite(suggestedName, bytes) {
  const handle = await orCancelled(window.showSaveFilePicker({ suggestedName, types: typesFor(suggestedName) }));
  if (handle) await write(handle, bytes);
  return handle;
}

// The handle, or null when the user refused write access. (A file from Open is read-only until
// its first save; the browser asks then.)
export async function writeTo(handle, bytes) {
  if ((await handle.requestPermission({ mode: "readwrite" })) !== "granted") return null;
  await write(handle, bytes);
  return handle;
}
"#)]
    extern "C" {
        #[wasm_bindgen(js_name = pickAndRead)]
        fn pick_and_read(images: bool) -> Promise;
        #[wasm_bindgen(js_name = canPickSave)]
        fn can_pick_save() -> bool;
        #[wasm_bindgen(js_name = pickAndWrite)]
        fn pick_and_write(suggested: &str, bytes: &Uint8Array) -> Promise;
        #[wasm_bindgen(js_name = pickFolder)]
        fn pick_folder(kind: &str) -> Promise;
        #[wasm_bindgen(js_name = storedFiles)]
        fn stored_files(kind: &str) -> Promise;
        #[wasm_bindgen(js_name = canUseLocalFonts)]
        fn can_use_local_fonts() -> bool;
        #[wasm_bindgen(js_name = localFontsGranted)]
        fn local_fonts_granted() -> Promise;
        #[wasm_bindgen(js_name = queryLocalFontNames)]
        fn query_local_font_names() -> Promise;
        #[wasm_bindgen(js_name = readLocalFont)]
        fn read_local_font(name: &str) -> Promise;
        #[wasm_bindgen(js_name = writeTo)]
        fn write_to(handle: &JsValue, bytes: &Uint8Array) -> Promise;
    }

    /// (document path or name, bytes), or why the file could not be read.
    type OpenedFile = Result<(String, Vec<u8>), String>;

    /// (file name, contents) of the images in a folder.
    type FolderFiles = Vec<(String, Vec<u8>)>;

    /// Which folder setting was browsed for, and the folder's name or why it could not be read.
    type PickedFolder = (FolderKind, Result<String, String>);

    /// Prefix of the document path of a file we hold a handle for. Browsers don't reveal real
    /// paths; this one is unique (two files can share a name) and still ends in the file name.
    const HANDLE_PATH: &str = "browser-file:/";

    /// State the asynchronous browser callbacks share with the app.
    struct Shared {
        ctx: egui::Context,
        opened: RefCell<Vec<OpenedFile>>,
        image: RefCell<Option<OpenedFile>>,
        saved: RefCell<Option<SaveStatus>>,
        handles: RefCell<HashMap<String, JsValue>>,
        next_handle: Cell<u64>,
        folders: RefCell<HashMap<FolderKind, FolderFiles>>,
        folder_revision: Cell<u64>,
        picked_folder: RefCell<Option<PickedFolder>>,
        local_families: RefCell<Vec<String>>,
        local_names: RefCell<HashSet<String>>,
        local_requested: RefCell<HashSet<String>>,
        fonts_status: RefCell<Option<Result<usize, String>>>,
    }

    impl Shared {
        /// Keep `handle` and return the document path that refers to it.
        fn remember(&self, handle: JsValue, name: &str) -> String {
            let id = self.next_handle.get();
            self.next_handle.set(id + 1);
            let path = format!("{HANDLE_PATH}{id}/{name}");
            self.handles.borrow_mut().insert(path.clone(), handle);
            path
        }

        fn finish_open(&self, file: OpenedFile) {
            self.opened.borrow_mut().push(file);
            self.ctx.request_repaint();
        }

        fn finish_image(&self, file: OpenedFile) {
            *self.image.borrow_mut() = Some(file);
            self.ctx.request_repaint();
        }

        fn set_folder(&self, kind: FolderKind, files: FolderFiles) {
            self.folders.borrow_mut().insert(kind, files);
            self.folder_revision.set(self.folder_revision.get() + 1);
            self.ctx.request_repaint();
        }

        /// Find out which fonts are on offer, keeping the outcome for the app.
        async fn query_local_fonts(&self) {
            let strings = |v: &JsValue| -> Vec<String> {
                js_sys::Array::from(v)
                    .iter()
                    .filter_map(|x| x.as_string())
                    .collect()
            };
            let status = match JsFuture::from(query_local_font_names()).await {
                Ok(found) => {
                    let families = strings(&field(&found, "families"));
                    *self.local_names.borrow_mut() = strings(&field(&found, "names")).into_iter().collect();
                    let count = families.len();
                    *self.local_families.borrow_mut() = families;
                    self.folder_revision.set(self.folder_revision.get() + 1);
                    Ok(count)
                }
                Err(e) => Err(error_text(&e)),
            };
            *self.fonts_status.borrow_mut() = Some(status);
            self.ctx.request_repaint();
        }

        fn finish_folder(&self, kind: FolderKind, folder: Result<String, String>) {
            *self.picked_folder.borrow_mut() = Some((kind, folder));
            self.ctx.request_repaint();
        }

        fn finish_save(&self, status: SaveStatus) {
            *self.saved.borrow_mut() = Some(status);
            self.ctx.request_repaint();
        }
    }

    struct Browser {
        shared: Rc<Shared>,
    }

    impl Browser {
        fn new(ctx: egui::Context) -> Self {
            let browser = Self {
                shared: Rc::new(Shared {
                    ctx,
                    opened: RefCell::default(),
                    image: RefCell::default(),
                    saved: RefCell::default(),
                    handles: RefCell::default(),
                    next_handle: Cell::new(0),
                    folders: RefCell::default(),
                    folder_revision: Cell::new(0),
                    picked_folder: RefCell::default(),
                    local_families: RefCell::default(),
                    local_names: RefCell::default(),
                    local_requested: RefCell::default(),
                    fonts_status: RefCell::default(),
                }),
            };
            // Permission given on an earlier visit: no need to ask again.
            {
                let shared = browser.shared.clone();
                spawn_local(async move {
                    let granted = JsFuture::from(local_fonts_granted()).await;
                    if granted.is_ok_and(|g| g.is_truthy()) {
                        shared.query_local_fonts().await;
                    }
                });
            }
            // Folders chosen in earlier visits.
            for kind in [FolderKind::Textures, FolderKind::Patterns, FolderKind::Fonts] {
                let shared = browser.shared.clone();
                spawn_local(async move {
                    if let Ok(files) = JsFuture::from(stored_files(storage_name(kind))).await {
                        let files = folder_entries(&files);
                        if !files.is_empty() {
                            shared.set_folder(kind, files);
                        }
                    }
                });
            }
            browser
        }

        /// Report `writing` when it finishes. It resolves to the file handle, or null when
        /// cancelled. `path` is the document path of a handle we already hold.
        fn track_save(&self, writing: Promise, path: Option<String>) -> SaveStatus {
            let shared = self.shared.clone();
            spawn_local(async move {
                let status = match JsFuture::from(writing).await {
                    Ok(handle) if handle.is_null() => SaveStatus::Cancelled,
                    Ok(handle) => SaveStatus::Saved(path.unwrap_or_else(|| {
                        let name = text_field(&handle, "name");
                        shared.remember(handle, &name)
                    })),
                    Err(e) => SaveStatus::Failed(error_text(&e)),
                };
                shared.finish_save(status);
            });
            SaveStatus::Pending
        }
    }

    impl Platform for Browser {
        fn read_dropped(&mut self, file: egui::DroppedFileHandle) {
            let shared = self.shared.clone();
            spawn_local(async move {
                let name = file.path().display().to_string();
                let read = file.bytes_async().await;
                shared.finish_open(
                    read.map(|bytes| (name.clone(), bytes))
                        .map_err(|e| format!("Could not read {name}: {e}")),
                );
            });
        }

        fn take_opened(&mut self) -> Vec<OpenedFile> {
            std::mem::take(&mut *self.shared.opened.borrow_mut())
        }

        fn take_saved(&mut self) -> Option<SaveStatus> {
            self.shared.saved.borrow_mut().take()
        }

        fn open_file(&mut self) -> Option<(String, Vec<u8>)> {
            let picking = pick_and_read(false);
            let shared = self.shared.clone();
            spawn_local(async move {
                match picked_file(picking).await {
                    Ok(None) => {}
                    Ok(Some(file)) => {
                        let path = if file.handle.is_undefined() {
                            file.name
                        } else {
                            shared.remember(file.handle, &file.name)
                        };
                        shared.finish_open(Ok((path, file.bytes)));
                    }
                    Err(e) => shared.finish_open(Err(e)),
                }
            });
            None
        }

        fn open_image(&mut self) -> Option<(String, Vec<u8>)> {
            let picking = pick_and_read(true);
            let shared = self.shared.clone();
            spawn_local(async move {
                match picked_file(picking).await {
                    Ok(None) => {}
                    Ok(Some(file)) => shared.finish_image(Ok((file.name, file.bytes))),
                    Err(e) => shared.finish_image(Err(e)),
                }
            });
            None
        }

        fn pick_folder(&mut self, kind: FolderKind) -> Option<String> {
            let picking = pick_folder(storage_name(kind));
            let shared = self.shared.clone();
            spawn_local(async move {
                match JsFuture::from(picking).await {
                    Ok(picked) if picked.is_null() => {}
                    Ok(picked) => {
                        shared.set_folder(kind, folder_entries(&field(&picked, "files")));
                        shared.finish_folder(kind, Ok(text_field(&picked, "folder")));
                    }
                    Err(e) => shared.finish_folder(kind, Err(error_text(&e))),
                }
            });
            None
        }

        fn take_folder(&mut self) -> Option<PickedFolder> {
            self.shared.picked_folder.borrow_mut().take()
        }

        fn folder_files(&self, kind: FolderKind) -> FolderFiles {
            self.shared
                .folders
                .borrow()
                .get(&kind)
                .cloned()
                .unwrap_or_default()
        }

        fn folder_revision(&self) -> u64 {
            self.shared.folder_revision.get()
        }

        fn can_request_fonts(&self) -> bool {
            can_use_local_fonts()
        }

        fn request_fonts(&mut self) {
            let shared = self.shared.clone();
            spawn_local(async move { shared.query_local_fonts().await });
        }

        fn take_fonts_status(&mut self) -> Option<Result<usize, String>> {
            self.shared.fonts_status.borrow_mut().take()
        }

        fn installed_fonts(&self) -> Vec<String> {
            self.shared.local_families.borrow().clone()
        }

        fn load_installed_font(&mut self, name: &str) {
            if !self.shared.local_names.borrow().contains(name)
                || !self.shared.local_requested.borrow_mut().insert(name.to_owned())
            {
                return;
            }
            let reading = read_local_font(name);
            let shared = self.shared.clone();
            spawn_local(async move {
                if let Ok(files) = JsFuture::from(reading).await {
                    let files = folder_entries(&files);
                    let mut all = shared
                        .folders
                        .borrow()
                        .get(&FolderKind::InstalledFonts)
                        .cloned()
                        .unwrap_or_default();
                    for file in files {
                        if !all.iter().any(|(n, _)| *n == file.0) {
                            all.push(file);
                        }
                    }
                    shared.set_folder(FolderKind::InstalledFonts, all);
                }
            });
        }

        fn take_image(&mut self) -> Option<OpenedFile> {
            self.shared.image.borrow_mut().take()
        }

        fn save_file(&mut self, suggested: &str, bytes: &[u8]) -> SaveStatus {
            let name = suggested.rsplit(['/', '\\']).next().unwrap_or(suggested);
            if can_pick_save() {
                // `Uint8Array::from` copies: the write outlives this call and `bytes`.
                self.track_save(pick_and_write(name, &Uint8Array::from(bytes)), None)
            } else {
                save_by_download(name, bytes)
            }
        }

        fn write_file(&mut self, path: &str, bytes: &[u8]) -> SaveStatus {
            let handle = self.shared.handles.borrow().get(path).cloned();
            match handle {
                Some(handle) => {
                    self.track_save(write_to(&handle, &Uint8Array::from(bytes)), Some(path.to_owned()))
                }
                None => self.save_file(path, bytes),
            }
        }

        fn display_path(&self, path: &str) -> String {
            match path.strip_prefix(HANDLE_PATH) {
                Some(rest) => rest.rsplit('/').next().unwrap_or(rest).to_owned(),
                None => path.to_owned(),
            }
        }
    }

    /// A file the user chose.
    struct PickedFile {
        name: String,
        bytes: Vec<u8>,
        /// The file's handle, or undefined if the browser has no file pickers.
        handle: JsValue,
    }

    /// Wait for `picking` (from `pick_and_read`); `None` when the user cancelled.
    async fn picked_file(picking: Promise) -> Result<Option<PickedFile>, String> {
        let picked = JsFuture::from(picking)
            .await
            .map_err(|e| format!("Could not open the file: {}", error_text(&e)))?;
        if picked.is_null() {
            return Ok(None);
        }
        Ok(Some(PickedFile {
            name: text_field(&picked, "name"),
            bytes: Uint8Array::new(&field(&picked, "bytes")).to_vec(),
            handle: field(&picked, "handle"),
        }))
    }

    /// The browser-storage folder that holds the copy of a Fireworks folder.
    fn storage_name(kind: FolderKind) -> &'static str {
        match kind {
            FolderKind::Textures => "fireworks-textures",
            FolderKind::Patterns => "fireworks-patterns",
            FolderKind::Fonts => "firecraft-fonts",
            FolderKind::InstalledFonts => "firecraft-installed-fonts",
        }
    }

    /// (file name, contents) from a JS array of `{ name, bytes }`.
    fn folder_entries(files: &JsValue) -> FolderFiles {
        js_sys::Array::from(files)
            .iter()
            .map(|f| {
                (
                    text_field(&f, "name"),
                    Uint8Array::new(&field(&f, "bytes")).to_vec(),
                )
            })
            .collect()
    }

    fn field(object: &JsValue, name: &str) -> JsValue {
        Reflect::get(object, &name.into()).unwrap_or(JsValue::UNDEFINED)
    }

    fn text_field(object: &JsValue, name: &str) -> String {
        field(object, name).as_string().unwrap_or_default()
    }

    fn error_text(e: &JsValue) -> String {
        let message = text_field(e, "message");
        if message.is_empty() {
            format!("{e:?}")
        } else {
            message
        }
    }

    /// For browsers with no save picker: ask for a name, then download under that name.
    fn save_by_download(suggested: &str, bytes: &[u8]) -> SaveStatus {
        let Some(window) = web_sys::window() else {
            return SaveStatus::Failed("no window".into());
        };
        let Ok(Some(name)) = window.prompt_with_message_and_default("Save as:", suggested) else {
            return SaveStatus::Cancelled;
        };
        let name = name.trim();
        if name.is_empty() {
            return SaveStatus::Cancelled;
        }
        // Keep the suggested file's extension (.png, or an export's .gif, .jpg...).
        let ext = suggested.rsplit_once('.').map_or("png", |(_, e)| e).to_ascii_lowercase();
        let name = if name.to_ascii_lowercase().ends_with(&format!(".{ext}")) {
            name.to_owned()
        } else {
            format!("{name}.{ext}")
        };
        match download(&name, bytes) {
            Ok(()) => SaveStatus::Saved(name),
            Err(e) => SaveStatus::Failed(error_text(&e)),
        }
    }

    /// Offer `bytes` as a download named `name`.
    fn download(name: &str, bytes: &[u8]) -> Result<(), JsValue> {
        let window = web_sys::window().ok_or("no window")?;
        let document = window.document().ok_or("no document")?;
        let array = Uint8Array::from(bytes);
        let parts = js_sys::Array::of1(&array);
        let opts = web_sys::BlobPropertyBag::new();
        opts.set_type("image/png");
        let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts)?;
        let url = web_sys::Url::create_object_url_with_blob(&blob)?;
        let a: web_sys::HtmlAnchorElement = document.create_element("a")?.dyn_into()?;
        a.set_href(&url);
        a.set_download(name);
        a.click();
        web_sys::Url::revoke_object_url(&url)?;
        Ok(())
    }

    struct App(FireCraftApp);

    impl eframe::App for App {
        fn save(&mut self, storage: &mut dyn eframe::Storage) {
            storage.set_string(firecraft_ui_egui::Prefs::STORAGE_KEY, self.0.prefs.serialize());
        }

        fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
            self.0.ui(ui);
        }
    }

    pub fn start() {
        spawn_local(async {
            let Some(document) = web_sys::window().and_then(|w| w.document()) else {
                return;
            };
            let Some(canvas) = document
                .get_element_by_id("firecraft_canvas")
                .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            else {
                return;
            };
            let result = eframe::WebRunner::new()
                .start(
                    canvas,
                    eframe::WebOptions::default(),
                    Box::new(|cc| {
                        let mut app = FireCraftApp::new(Box::new(Browser::new(cc.egui_ctx.clone())));
                        if let Some(text) = cc
                            .storage
                            .and_then(|s| s.get_string(firecraft_ui_egui::Prefs::STORAGE_KEY))
                        {
                            app.set_prefs(firecraft_ui_egui::Prefs::parse(&text));
                        }
                        Ok(Box::new(App(app)))
                    }),
                )
                .await;
            if let Some(loading) = document.get_element_by_id("firecraft_loading") {
                loading.remove();
            }
            if let Err(e) = result {
                web_sys::console::error_1(&e);
            }
        });
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("firecraft-web is the browser build: cd apps/firecraft-web && trunk serve");
}
