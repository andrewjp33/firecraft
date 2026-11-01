//! FireCraft in the browser: the same app compiled to WebAssembly (`trunk build --release`).
//!
//! Files open by dragging them onto the page; Save and Export download the file.

#[cfg(target_arch = "wasm32")]
mod web {
    use firecraft_ui_egui::{FireCraftApp, Platform};
    use wasm_bindgen::JsCast;

    type Inbox = std::rc::Rc<std::cell::RefCell<Vec<(String, Vec<u8>)>>>;

    #[derive(Default)]
    struct Browser {
        inbox: Inbox,
    }

    impl Platform for Browser {
        fn read_dropped(&mut self, file: egui::DroppedFileHandle) {
            let inbox = self.inbox.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let name = file.path().display().to_string();
                match file.bytes_async().await {
                    Ok(bytes) => inbox.borrow_mut().push((name, bytes)),
                    Err(e) => web_sys::console::error_1(&format!("{name}: {e}").into()),
                }
            });
        }

        fn take_opened(&mut self) -> Vec<(String, Vec<u8>)> {
            std::mem::take(&mut *self.inbox.borrow_mut())
        }

        fn open_file(&mut self) -> Option<(String, Vec<u8>)> {
            // Browsers only offer async pickers; drag and drop a file onto the page instead.
            None
        }

        fn save_file(&mut self, suggested: &str, bytes: &[u8]) -> Option<String> {
            let name = suggested
                .rsplit(['/', '\\'])
                .next()
                .unwrap_or(suggested)
                .to_owned();
            download(&name, bytes).ok()?;
            Some(name)
        }
    }

    /// Offer `bytes` as a download named `name`.
    fn download(name: &str, bytes: &[u8]) -> Result<(), wasm_bindgen::JsValue> {
        let window = web_sys::window().ok_or("no window")?;
        let document = window.document().ok_or("no document")?;
        let array = js_sys::Uint8Array::from(bytes);
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
        wasm_bindgen_futures::spawn_local(async {
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
                        let mut app = FireCraftApp::new(Box::new(Browser::default()));
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
