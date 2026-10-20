//! The eframe application: keys, file events and the close guard.

use std::sync::mpsc;

use crate::command::{self, Answer, Command};
use crate::document::Document;
use crate::file::{self, FileEvent};
use crate::state::{Pending, State};
use crate::ui;

pub struct App {
    state: State,
    events_tx: mpsc::Sender<FileEvent>,
    events_rx: mpsc::Receiver<FileEvent>,
    /// The window was told to close.
    closing: bool,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, doc: Document) -> Self {
        ui::theme::install(&cc.egui_ctx);
        let (events_tx, events_rx) = mpsc::channel();
        Self {
            state: State::new(doc),
            events_tx,
            events_rx,
            closing: false,
        }
    }

    fn poll_events(&mut self) {
        while let Ok(event) = self.events_rx.try_recv() {
            match event {
                FileEvent::Opened(doc) => self.replace(doc),
                FileEvent::Saved => {
                    self.state.doc.mark_saved();
                    let name = self.state.doc.name.clone();
                    self.state.notify(format!("saved {name}"));
                }
                FileEvent::Failed(e) => self.state.fail(e),
            }
        }
    }

    /// Shows `doc`, unless the current one has unsaved changes: then asks.
    fn replace(&mut self, doc: Document) {
        if self.state.doc.is_dirty() {
            self.state.pending = Some(Pending::Replace(doc));
        } else {
            let name = doc.name.clone();
            self.state.doc = doc;
            self.state.notify(format!("opened {name}"));
        }
    }

    /// Opens dropped files: by path on the desktop, by contents in the browser.
    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for dropped in dropped {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let _ = self.events_tx.send(match file::read(dropped.path()) {
                    Ok(doc) => FileEvent::Opened(doc),
                    Err(e) => FileEvent::Failed(e),
                });
            }
            #[cfg(target_arch = "wasm32")]
            {
                let tx = self.events_tx.clone();
                let ctx = ctx.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let name = dropped
                        .path()
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| String::from("dropped file"));
                    if let Ok(bytes) = dropped.bytes_async().await {
                        let text = String::from_utf8_lossy(&bytes).into_owned();
                        let _ = tx.send(FileEvent::Opened(Document::from_text(None, name, text)));
                        ctx.request_repaint();
                    }
                });
            }
        }
    }

    /// Closing the window with unsaved changes asks first.
    fn handle_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested())
            && self.state.doc.is_dirty()
            && !self.state.quit
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.state.pending = Some(Pending::Quit);
        }
    }

    /// The keys of an open dialog: it takes them all.
    fn dialog_keys(&mut self, ctx: &egui::Context) -> bool {
        use egui::{Key, Modifiers};
        const NONE: Modifiers = Modifiers::NONE;
        if self.state.show_help {
            if ctx.input_mut(|i| {
                [Key::Escape, Key::Enter, Key::H, Key::F1]
                    .into_iter()
                    .any(|k| i.consume_key(NONE, k))
            }) {
                self.state.show_help = false;
            }
            return true;
        }
        if self.state.pending.is_some() {
            let answer = ctx.input_mut(|i| {
                if i.consume_key(NONE, Key::S) || i.consume_key(NONE, Key::Enter) {
                    Some(Answer::Save)
                } else if i.consume_key(NONE, Key::D) {
                    Some(Answer::Discard)
                } else if i.consume_key(NONE, Key::Escape) {
                    Some(Answer::Cancel)
                } else {
                    None
                }
            });
            if let Some(answer) = answer {
                self.run(Command::Confirm(answer), ctx);
            }
            return true;
        }
        false
    }

    fn run(&mut self, command: Command, ctx: &egui::Context) {
        self.state.notice = None;
        match command {
            Command::Open => {
                if self.state.doc.is_dirty() {
                    self.state.pending = Some(Pending::Open);
                } else {
                    self.open(ctx);
                }
            }
            Command::Save => self.save(ctx),
            Command::SaveAs => self.save_as(ctx),
            Command::Quit => {
                if self.state.doc.is_dirty() {
                    self.state.pending = Some(Pending::Quit);
                } else {
                    self.state.quit = true;
                }
            }
            Command::Up => self.state.move_selection(-1),
            Command::Down => self.state.move_selection(1),
            Command::Select(row) => self.state.selected = row,
            Command::Toggle => self.state.toggle(),
            Command::Adjust(delta) => self.state.adjust(delta),
            Command::EditValue => self.state.begin_edit(),
            Command::EditRow(row) => {
                self.state.selected = row;
                self.state.begin_edit();
            }
            Command::Set(field, value) => self.state.set(field, value.as_ref()),
            Command::Help => self.state.show_help = !self.state.show_help,
            Command::Confirm(answer) => self.confirm(answer, ctx),
        }
    }

    fn confirm(&mut self, answer: Answer, ctx: &egui::Context) {
        let Some(pending) = self.state.pending.take() else {
            return;
        };
        match answer {
            Answer::Cancel => return,
            Answer::Save => {
                self.save(ctx);
                if self.state.doc.is_dirty() {
                    // Not saved (no path chosen, or an error): stay.
                    return;
                }
            }
            Answer::Discard => {}
        }
        match pending {
            Pending::Quit => self.state.quit = true,
            Pending::Open => self.open(ctx),
            Pending::Replace(doc) => {
                let name = doc.name.clone();
                self.state.doc = doc;
                self.state.notify(format!("opened {name}"));
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn open(&mut self, _ctx: &egui::Context) {
        file::open(self.state.doc.path.as_deref(), &self.events_tx);
    }

    #[cfg(target_arch = "wasm32")]
    fn open(&mut self, ctx: &egui::Context) {
        file::open(self.events_tx.clone(), ctx.clone());
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save(&mut self, ctx: &egui::Context) {
        let Some(path) = self.state.doc.path.clone() else {
            return self.save_as(ctx);
        };
        let _ = self
            .events_tx
            .send(match file::write(&path, &self.state.doc.text) {
                Ok(()) => FileEvent::Saved,
                Err(e) => FileEvent::Failed(e),
            });
        self.poll_events();
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn save_as(&mut self, ctx: &egui::Context) {
        if let Some(path) = file::choose_save_path(self.state.doc.path.as_deref()) {
            self.state.doc.name = file::display_name(&path);
            self.state.doc.path = Some(path);
            self.save(ctx);
        }
    }

    /// In the browser, saving is a download either way.
    #[cfg(target_arch = "wasm32")]
    fn save(&mut self, ctx: &egui::Context) {
        let doc = &self.state.doc;
        file::download(
            doc.name.clone(),
            doc.text.clone(),
            self.events_tx.clone(),
            ctx.clone(),
        );
    }

    #[cfg(target_arch = "wasm32")]
    fn save_as(&mut self, ctx: &egui::Context) {
        self.save(ctx);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_events();
        self.handle_drops(&ctx);
        self.handle_close(&ctx);
        if !self.dialog_keys(&ctx) {
            let typing = command::text_focused(&ctx);
            for command in command::take(&ctx, typing) {
                self.run(command, &ctx);
            }
        }
        if let Some(command) = ui::render(&mut self.state, ui) {
            self.run(command, &ctx);
            ctx.request_repaint();
        }
        if self.state.quit && !self.closing {
            self.closing = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }
}

/// The app driven offscreen with simulated keys.
#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Key, Modifiers};
    use egui_kittest::Harness;

    const FILE: &str = "// mine\n(\n    max_width: 80, // -w\n    tab_size: 2,\n)\n";

    fn harness() -> Harness<'static, App> {
        let doc = Document::from_text(None, String::from("fmt.ron"), FILE.to_string());
        let mut harness = Harness::new_eframe(|cc| App::new(cc, doc));
        harness.run();
        harness
    }

    fn text<'h>(harness: &'h Harness<'_, App>) -> &'h str {
        &harness.state().state.doc.text
    }

    #[test]
    fn moving_and_toggling_edit_the_file() {
        let mut h = harness();
        for key in [Key::J, Key::J, Key::Space] {
            h.key_press(key);
            h.run();
        }
        assert_eq!(
            text(&h),
            "// mine\n(\n  max_width: 80, // -w\n  tab_size: 2,\n  blank_lines: Keep,\n)\n"
        );
        h.key_press(Key::ArrowRight);
        h.run();
        assert!(text(&h).contains("blank_lines: Remove,"), "{}", text(&h));
        h.key_press(Key::Space);
        h.run();
        assert!(!text(&h).contains("blank_lines"), "{}", text(&h));
        assert!(h.state().state.doc.is_dirty());
    }

    #[test]
    fn stepping_a_number() {
        let mut h = harness();
        h.key_press_modifiers(Modifiers::SHIFT, Key::ArrowRight);
        h.run();
        assert!(text(&h).contains("max_width: 90, // -w"), "{}", text(&h));
        h.key_press(Key::ArrowLeft);
        h.run();
        assert!(text(&h).contains("max_width: 89, // -w"), "{}", text(&h));
    }

    #[test]
    fn typing_a_number_replaces_it() {
        let mut h = harness();
        h.key_press(Key::Enter);
        h.run();
        assert!(h.state().state.editing.is_some());
        h.event(Event::Text(String::from("72")));
        h.run();
        h.key_press(Key::Enter);
        h.run();
        assert!(h.state().state.editing.is_none());
        assert!(text(&h).contains("max_width: 72, // -w"), "{}", text(&h));

        h.key_press(Key::Enter);
        h.run();
        h.event(Event::Text(String::from("ab")));
        h.run();
        h.key_press(Key::Enter);
        h.run();
        assert!(text(&h).contains("max_width: 72,"), "{}", text(&h));
        let notice = h.state().state.notice.as_ref().expect("a notice");
        assert!(
            notice.error && notice.text.contains("`ab`"),
            "{}",
            notice.text
        );

        // Escape abandons the edit.
        h.key_press(Key::Enter);
        h.run();
        h.event(Event::Text(String::from("5")));
        h.run();
        h.key_press(Key::Escape);
        h.run();
        assert!(h.state().state.editing.is_none());
        assert!(text(&h).contains("max_width: 72,"), "{}", text(&h));
    }

    #[test]
    fn help_opens_and_closes() {
        let mut h = harness();
        h.key_press(Key::H);
        h.run();
        assert!(h.state().state.show_help);
        // Keys are the help's while it is open.
        h.key_press(Key::J);
        h.run();
        assert_eq!(h.state().state.selected, 0);
        h.key_press(Key::Escape);
        h.run();
        assert!(!h.state().state.show_help);
    }

    #[test]
    fn quitting_with_changes_asks_first() {
        let mut h = harness();
        h.key_press_modifiers(Modifiers::COMMAND, Key::Q);
        h.run();
        assert!(h.state().state.quit, "a clean document quits at once");

        let mut h = harness();
        h.key_press(Key::Space);
        h.run();
        h.key_press_modifiers(Modifiers::COMMAND, Key::Q);
        h.run();
        assert!(matches!(h.state().state.pending, Some(Pending::Quit)));
        assert!(!h.state().state.quit);
        h.key_press(Key::Escape);
        h.run();
        assert!(h.state().state.pending.is_none() && !h.state().state.quit);
        h.key_press_modifiers(Modifiers::COMMAND, Key::Q);
        h.run();
        h.key_press(Key::D);
        h.run();
        assert!(h.state().state.quit);
    }
}
