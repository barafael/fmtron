//! Reading and writing the file, and the dialogs that choose it. Results
//! arrive as [`FileEvent`]s on a channel, on both targets: the browser's
//! dialogs are asynchronous.

use std::sync::mpsc::Sender;

use crate::document::Document;

#[derive(Debug)]
pub enum FileEvent {
    Opened(Document),
    /// The text went out (to disk, or as a download).
    Saved,
    Failed(String),
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
#[cfg(target_arch = "wasm32")]
pub use web::*;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use std::path::{Path, PathBuf};

    /// The name shown for `path`: relative to the current directory when it
    /// is inside it.
    pub fn display_name(path: &Path) -> String {
        std::env::current_dir()
            .ok()
            .and_then(|cwd| path.strip_prefix(cwd).ok())
            .unwrap_or(path)
            .display()
            .to_string()
    }

    /// The document to start with: the given file (which need not exist
    /// yet), else the `fmt.ron` or `.fmt.ron` in the current directory,
    /// else a new `fmt.ron` there.
    pub fn initial(arg: Option<PathBuf>) -> Result<Document, String> {
        if let Some(path) = arg {
            return if path.is_file() {
                read(&path)
            } else {
                Ok(Document::new(Some(path.clone()), display_name(&path)))
            };
        }
        let cwd = std::env::current_dir().map_err(|e| format!("current directory: {e}"))?;
        for name in fmtron::CONFIG_FILE_NAMES {
            let path = cwd.join(name);
            if path.is_file() {
                return read(&path);
            }
        }
        let path = cwd.join(fmtron::CONFIG_FILE_NAMES[0]);
        Ok(Document::new(Some(path.clone()), display_name(&path)))
    }

    pub fn read(path: &Path) -> Result<Document, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("unable to read {}: {e}", path.display()))?;
        Ok(Document::from_text(
            Some(path.to_path_buf()),
            display_name(path),
            text,
        ))
    }

    /// Writes the whole text, then moves it into place, so that a failure
    /// leaves the old file intact.
    pub fn write(path: &Path, text: &str) -> Result<(), String> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
        let result = std::fs::write(&tmp, text).and_then(|()| std::fs::rename(&tmp, path));
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result.map_err(|e| format!("unable to write {}: {e}", path.display()))
    }

    fn dialog(start: Option<&Path>) -> rfd::FileDialog {
        let dir = start
            .and_then(|p| p.parent())
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok());
        let mut dialog = rfd::FileDialog::new().add_filter("RON", &["ron"]);
        if let Some(dir) = dir {
            dialog = dialog.set_directory(dir);
        }
        dialog
    }

    /// Asks for a file and reads it. Blocks while the dialog is open.
    pub fn open(current: Option<&Path>, tx: &Sender<FileEvent>) {
        if let Some(path) = dialog(current).pick_file() {
            let _ = tx.send(match read(&path) {
                Ok(doc) => FileEvent::Opened(doc),
                Err(e) => FileEvent::Failed(e),
            });
        }
    }

    /// Asks where to save. Blocks while the dialog is open.
    pub fn choose_save_path(current: Option<&Path>) -> Option<PathBuf> {
        let name = current
            .and_then(|p| p.file_name())
            .map_or("fmt.ron", |n| n.to_str().unwrap_or("fmt.ron"));
        dialog(current).set_file_name(name).save_file()
    }
}

#[cfg(target_arch = "wasm32")]
mod web {
    use super::*;

    /// Opens the browser's file picker; the file arrives on `tx`.
    pub fn open(tx: Sender<FileEvent>, ctx: egui::Context) {
        wasm_bindgen_futures::spawn_local(async move {
            let picked = rfd::AsyncFileDialog::new()
                .add_filter("RON", &["ron"])
                .pick_file()
                .await;
            if let Some(handle) = picked {
                let text = String::from_utf8_lossy(&handle.read().await).into_owned();
                let _ = tx.send(FileEvent::Opened(Document::from_text(
                    None,
                    handle.file_name(),
                    text,
                )));
                ctx.request_repaint();
            }
        });
    }

    /// Hands the text to the browser as a download named `name`.
    pub fn download(name: String, text: String, tx: Sender<FileEvent>, ctx: egui::Context) {
        wasm_bindgen_futures::spawn_local(async move {
            let handle = rfd::AsyncFileDialog::new()
                .set_file_name(&name)
                .save_file()
                .await;
            let event = match handle {
                Some(handle) => match handle.write(text.as_bytes()).await {
                    Ok(()) => FileEvent::Saved,
                    Err(e) => FileEvent::Failed(format!("unable to download: {e}")),
                },
                None => return,
            };
            let _ = tx.send(event);
            ctx.request_repaint();
        });
    }
}
