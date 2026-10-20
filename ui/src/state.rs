//! Everything the window shows: the document, the cursor, the dialogs.

use crate::document::Document;
use crate::field::{self, Field, Kind, Value};

/// A message in the header, until the next command.
#[derive(Debug, Clone)]
pub struct Notice {
    pub text: String,
    pub error: bool,
}

/// What waits for the answer to "unsaved changes".
#[derive(Debug, Clone)]
pub enum Pending {
    Quit,
    /// Ask for a file to open.
    Open,
    /// A file already read (dropped, or picked in the browser).
    Replace(Document),
}

/// The selected field's value being typed.
#[derive(Debug, Clone)]
pub struct Editing {
    pub text: String,
    /// Not yet shown: the form focuses the text and selects it all.
    pub fresh: bool,
}

pub struct State {
    pub doc: Document,
    /// The selected row of the form.
    pub selected: usize,
    pub editing: Option<Editing>,
    pub show_help: bool,
    pub pending: Option<Pending>,
    pub notice: Option<Notice>,
    /// Close the window.
    pub quit: bool,
}

impl State {
    pub fn new(doc: Document) -> Self {
        Self {
            doc,
            selected: 0,
            editing: None,
            show_help: false,
            pending: None,
            notice: None,
            quit: false,
        }
    }

    pub fn field(&self) -> Field {
        field::ALL[self.selected]
    }

    pub fn notify(&mut self, text: impl Into<String>) {
        self.notice = Some(Notice {
            text: text.into(),
            error: false,
        });
    }

    pub fn fail(&mut self, text: impl Into<String>) {
        self.notice = Some(Notice {
            text: text.into(),
            error: true,
        });
    }

    /// Starts typing the selected field's value: a number is edited as
    /// text (an unset one from its default), a choice moves on to the next.
    pub fn begin_edit(&mut self) {
        let field = self.field();
        match field.kind() {
            Kind::Choice(_) => self.adjust(1),
            Kind::Number { .. } => {
                let set = self.doc.parsed.as_ref().ok().and_then(|f| field.get(f));
                let text = match set {
                    Some(value) => value.literal(),
                    None => {
                        let value = field.effective(&self.doc.config());
                        self.set(field, Some(&value));
                        value.literal()
                    }
                };
                self.editing = Some(Editing { text, fresh: true });
            }
        }
    }

    /// Writes the typed value, if it is a number the field takes.
    pub fn commit_edit(&mut self) {
        let Some(editing) = self.editing.take() else {
            return;
        };
        let field = self.field();
        let Kind::Number { min } = field.kind() else {
            return;
        };
        match editing.text.trim().parse::<usize>() {
            Ok(n) if n >= min => self.set(field, Some(&Value::Number(n))),
            _ => self.fail(format!(
                "{} takes a whole number of at least {min}, not `{}`",
                field.name(),
                editing.text
            )),
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        let len = field::ALL.len() as isize;
        self.selected = (self.selected as isize + delta).rem_euclid(len) as usize;
    }

    /// Writes `value` into the selected field, reporting a failure.
    pub fn set(&mut self, field: Field, value: Option<&Value>) {
        if let Err(e) = self.doc.set(field, value) {
            self.fail(e);
        }
    }

    /// Sets the selected field to the value in effect, or unsets it.
    pub fn toggle(&mut self) {
        let field = self.field();
        let set = self
            .doc
            .parsed
            .as_ref()
            .is_ok_and(|f| field.get(f).is_some());
        if set {
            self.set(field, None);
        } else {
            let value = field.effective(&self.doc.config());
            self.set(field, Some(&value));
        }
    }

    /// Steps the selected field: a number by `delta`, a choice to the next
    /// or previous one. An unset field is left alone.
    pub fn adjust(&mut self, delta: i64) {
        let field = self.field();
        let Some(current) = self.doc.parsed.as_ref().ok().and_then(|f| field.get(f)) else {
            return;
        };
        let value = match (field.kind(), current) {
            (Kind::Number { min }, Value::Number(n)) => {
                let stepped = (n as i64).saturating_add(delta).max(min as i64);
                Value::Number(stepped as usize)
            }
            (Kind::Choice(choices), Value::Choice(c)) => {
                let at = choices.iter().position(|&x| x == c).unwrap_or(0) as i64;
                let next = (at + delta.signum()).rem_euclid(choices.len() as i64);
                Value::Choice(choices[next as usize].to_string())
            }
            _ => return,
        };
        self.set(field, Some(&value));
    }
}
