//! Browser glue: the entry point, and keeping the shortcuts from the browser.

use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

/// Starts the app on the page's canvas.
pub fn start() {
    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .and_then(|w| w.document())
            .expect("no document");
        let canvas = document
            .get_element_by_id("fmtron_ui_canvas")
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            .expect("no canvas");
        claim_shortcuts();
        let started = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| {
                    let doc = crate::document::Document::new(None, String::from("fmt.ron"));
                    Ok(Box::new(crate::app::App::new(cc, doc)))
                }),
            )
            .await;
        if let Some(loading) = document.get_element_by_id("loading") {
            match started {
                Ok(()) => loading.remove(),
                Err(e) => loading.set_text_content(Some(&format!("failed to start: {e:?}"))),
            }
        }
    });
}

/// Ctrl+O and Ctrl+S would otherwise open the browser's own dialogs. The
/// app is the whole page, so they are claimed page-wide, in the capture
/// phase: eframe stops key events from bubbling up to the window.
fn claim_shortcuts() {
    let on_key = Closure::<dyn Fn(web_sys::KeyboardEvent)>::new(|event: web_sys::KeyboardEvent| {
        if (event.ctrl_key() || event.meta_key())
            && matches!(event.key().as_str(), "o" | "O" | "s" | "S")
        {
            event.prevent_default();
        }
    });
    if let Some(window) = web_sys::window() {
        let _ = window.add_event_listener_with_callback_and_bool(
            "keydown",
            on_key.as_ref().unchecked_ref(),
            true,
        );
    }
    on_key.forget();
}
