//! The file being edited: its text, where it lives, and what it says.

use std::path::PathBuf;

use fmtron::{Config, ConfigParseError, FileConfig};

use crate::field::{Field, Value};
use crate::patch;

#[derive(Debug, Clone)]
pub struct Document {
    /// Where the file is written. None in the browser.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub path: Option<PathBuf>,
    /// Shown in the header: the path, or the browser file's name.
    pub name: String,
    /// The text, exactly as it will be saved.
    pub text: String,
    /// The text as last loaded or saved.
    saved: String,
    /// Whether `saved` is on disk, as opposed to a new file not yet written.
    exists: bool,
    pub parsed: Result<FileConfig, ConfigParseError>,
}

impl Document {
    /// A new, empty file that will be written to `path`.
    pub fn new(path: Option<PathBuf>, name: String) -> Self {
        let mut doc = Self {
            path,
            name,
            text: String::new(),
            saved: String::new(),
            exists: false,
            parsed: Ok(FileConfig::default()),
        };
        doc.reparse();
        doc
    }

    /// An existing file's text.
    pub fn from_text(path: Option<PathBuf>, name: String, text: String) -> Self {
        let mut doc = Self::new(path, name);
        doc.saved = text.clone();
        doc.text = text;
        doc.exists = true;
        doc.reparse();
        doc
    }

    pub fn is_dirty(&self) -> bool {
        self.text != self.saved
    }

    pub fn is_new(&self) -> bool {
        !self.exists
    }

    pub fn mark_saved(&mut self) {
        self.saved = self.text.clone();
        self.exists = true;
    }

    /// The settings in effect: the defaults overridden by the file.
    pub fn config(&self) -> Config {
        let mut config = Config::default();
        if let Ok(file) = &self.parsed {
            file.apply(&mut config);
        }
        config
    }

    /// Why the file would not work as a `fmt.ron`, if it would not.
    pub fn problem(&self) -> Option<String> {
        match &self.parsed {
            Err(e) => Some(e.to_string()),
            Ok(_) => {
                let config = self.config();
                (config.tab_size > config.max_tab).then(|| {
                    format!(
                        "tab_size {} exceeds max_tab {}",
                        config.tab_size, config.max_tab
                    )
                })
            }
        }
    }

    /// Re-reads the settings from the text, after it changed.
    pub fn reparse(&mut self) {
        self.parsed = if self.text.trim().is_empty() {
            Ok(FileConfig::default())
        } else {
            self.text.parse()
        };
    }

    /// Sets or unsets one field, editing only its entry in the text, then
    /// lays the file out the way fmtron would in the project it configures:
    /// with its own settings. Comments stay.
    pub fn set(&mut self, field: Field, value: Option<&Value>) -> Result<(), String> {
        if self.text.trim().is_empty() {
            self.text = String::from("()");
        }
        let patched = match value {
            Some(v) => patch::set(&self.text, field.name(), &v.literal()),
            None => patch::remove(&self.text, field.name()),
        }
        .ok_or("the file is not a RON struct; edit its text instead")?;
        let config = patched
            .parse::<FileConfig>()
            .map(|file| {
                let mut config = Config::default();
                file.apply(&mut config);
                config
            })
            .unwrap_or_default();
        self.text = match fmtron::format_ron(&patched, &config) {
            Ok(formatted) => formatted + fmtron::line_ending(&patched),
            Err(_) => patched,
        };
        self.reparse();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> Document {
        Document::from_text(None, String::from("fmt.ron"), text.to_string())
    }

    #[test]
    fn a_new_document_is_clean_and_default() {
        let d = Document::new(None, String::from("fmt.ron"));
        assert!(!d.is_dirty() && d.is_new());
        assert_eq!(d.config(), Config::default());
        assert_eq!(d.problem(), None);
    }

    #[test]
    fn setting_a_field_keeps_comments_and_formats() {
        let mut d = doc("// mine\n(max_width: 80, // -w\n    tab_size: 2)\n");
        d.set(Field::BlankLines, Some(&Value::Choice("Remove".into())))
            .unwrap();
        assert_eq!(
            d.text,
            "// mine\n(\n  max_width: 80, // -w\n  tab_size: 2,\n  blank_lines: Remove,\n)\n"
        );
        assert!(d.is_dirty());
        assert_eq!(d.config().blank_lines, fmtron::BlankLines::Remove);
    }

    #[test]
    fn the_file_is_laid_out_with_its_own_settings() {
        let mut d = doc("(\n    tab_size: 2,\n)\n");
        d.set(Field::MaxWidth, Some(&Value::Number(120))).unwrap();
        assert_eq!(d.text, "(\n  tab_size: 2,\n  max_width: 120,\n)\n");
        // A short struct written on one line is kept on one line.
        let mut d = doc("(tab_size: 2)");
        d.set(Field::MaxWidth, Some(&Value::Number(120))).unwrap();
        assert_eq!(d.text, "(tab_size: 2, max_width: 120)\n");
    }

    #[test]
    fn unsetting_the_last_field_leaves_an_empty_struct() {
        let mut d = doc("(max_width: 80)\n");
        d.set(Field::MaxWidth, None).unwrap();
        assert_eq!(d.text, "()\n");
        assert_eq!(d.config(), Config::default());
    }

    #[test]
    fn an_empty_text_gets_a_struct() {
        let mut d = Document::new(None, String::from("fmt.ron"));
        d.set(Field::TabSize, Some(&Value::Number(2))).unwrap();
        assert_eq!(d.text, "(\n  tab_size: 2,\n)\n");
    }

    #[test]
    fn problems_are_reported() {
        assert!(
            doc("(max_widht: 1)")
                .problem()
                .unwrap()
                .contains("max_widht")
        );
        assert!(
            doc("(tab_size: 8, max_tab: 4)")
                .problem()
                .unwrap()
                .contains("exceeds")
        );
        let mut d = doc("[1]");
        assert!(d.set(Field::TabSize, None).is_err());
    }
}
