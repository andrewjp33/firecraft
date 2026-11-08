//! FireCraft in the browser: the same app compiled to WebAssembly (`trunk build --release`).
//!
//! File > Open and Save use the browser's file pickers; a file that came from one is saved back
//! in place, as on the desktop. Browsers without them (Firefox, Safari, plain http) get an upload
//! dialog for Open and a name prompt plus download for Save. Files can also be dragged onto the page.

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::rc::Rc;

    use firecraft_ui_egui::{FireCraftApp, Platform, SaveStatus};
    use js_sys::{Promise, Reflect, Uint8Array};
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_futures::{JsFuture, spawn_local};

    // Each function calls its picker before its first `await`, so the browser still sees the
    // click or key press that asked for it.
    #[wasm_bindgen(inline_js = r#"
const TYPES = [{ description: "Fireworks PNG", accept: { "image/png": [".png"] } }];

// null when the user cancelled.
async function orCancelled(picker) {
  try {
    return await picker;
  } catch (e) {
    if (e.name === "AbortError") return null;
    throw e;
  }
}

function chooseUpload() {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".png,image/png";
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
export async function pickAndRead() {
  let handle, file;
  if (window.showOpenFilePicker) {
    const picked = await orCancelled(window.showOpenFilePicker({ types: TYPES }));
    if (!picked) return null;
    handle = picked[0];
    file = await handle.getFile();
  } else {
    file = await chooseUpload();
    if (!file) return null;
  }
  return { name: file.name, bytes: new Uint8Array(await file.arrayBuffer()), handle };
}

export function canPickSave() {
  return !!window.showSaveFilePicker;
}

// The chosen handle, or null when cancelled.
export async function pickAndWrite(suggestedName, bytes) {
  const handle = await orCancelled(window.showSaveFilePicker({ suggestedName, types: TYPES }));
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
        fn pick_and_read() -> Promise;
        #[wasm_bindgen(js_name = canPickSave)]
        fn can_pick_save() -> bool;
        #[wasm_bindgen(js_name = pickAndWrite)]
        fn pick_and_write(suggested: &str, bytes: &Uint8Array) -> Promise;
        #[wasm_bindgen(js_name = writeTo)]
        fn write_to(handle: &JsValue, bytes: &Uint8Array) -> Promise;
    }

    /// (document path or name, bytes), or why the file could not be read.
    type OpenedFile = Result<(String, Vec<u8>), String>;

    /// Prefix of the document path of a file we hold a handle for. Browsers don't reveal real
    /// paths; this one is unique (two files can share a name) and still ends in the file name.
    const HANDLE_PATH: &str = "browser-file:/";

    /// State the asynchronous browser callbacks share with the app.
    struct Shared {
        ctx: egui::Context,
        opened: RefCell<Vec<OpenedFile>>,
        saved: RefCell<Option<SaveStatus>>,
        handles: RefCell<HashMap<String, JsValue>>,
        next_handle: Cell<u64>,
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
            Self {
                shared: Rc::new(Shared {
                    ctx,
                    opened: RefCell::default(),
                    saved: RefCell::default(),
                    handles: RefCell::default(),
                    next_handle: Cell::new(0),
                }),
            }
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
            let picking = pick_and_read();
            let shared = self.shared.clone();
            spawn_local(async move {
                match JsFuture::from(picking).await {
                    Ok(picked) if picked.is_null() => {}
                    Ok(picked) => {
                        let name = text_field(&picked, "name");
                        let bytes = field(&picked, "bytes");
                        let handle = field(&picked, "handle");
                        let path = if handle.is_undefined() {
                            name
                        } else {
                            shared.remember(handle, &name)
                        };
                        shared.finish_open(Ok((path, Uint8Array::new(&bytes).to_vec())));
                    }
                    Err(e) => shared.finish_open(Err(format!("Could not open the file: {}", error_text(&e)))),
                }
            });
            None
        }

        fn open_image(&mut self) -> Option<(String, Vec<u8>)> {
            // Texture files are not supported in the browser yet; `open_file` would open a document.
            None
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
        let name = if name.to_ascii_lowercase().ends_with(".png") {
            name.to_owned()
        } else {
            format!("{name}.png")
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
