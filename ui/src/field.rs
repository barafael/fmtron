//! The six `fmt.ron` fields, as rows of the form.

use fmtron::{BlankLines, Config, FileConfig};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    MaxWidth,
    TabSize,
    BlankLines,
    MaxDepth,
    MaxTab,
    StyleEdition,
}

/// Every field, in the order of the form.
pub const ALL: [Field; 6] = [
    Field::MaxWidth,
    Field::TabSize,
    Field::BlankLines,
    Field::MaxDepth,
    Field::MaxTab,
    Field::StyleEdition,
];

/// What a field holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Number { min: usize },
    Choice(&'static [&'static str]),
}

/// A field's value, as the form shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Number(usize),
    /// A variant or edition, as written in RON.
    Choice(String),
}

impl Value {
    /// The value as RON.
    pub fn literal(&self) -> String {
        match self {
            Self::Number(n) => n.to_string(),
            Self::Choice(c) => c.clone(),
        }
    }
}

impl Field {
    pub const fn name(self) -> &'static str {
        match self {
            Self::MaxWidth => "max_width",
            Self::TabSize => "tab_size",
            Self::BlankLines => "blank_lines",
            Self::MaxDepth => "max_depth",
            Self::MaxTab => "max_tab",
            Self::StyleEdition => "style_edition",
        }
    }

    /// The command-line flag with the same meaning.
    pub const fn flag(self) -> &'static str {
        match self {
            Self::MaxWidth => "-w",
            Self::TabSize => "-t",
            Self::BlankLines => "--blank-lines",
            Self::MaxDepth => "--max-depth",
            Self::MaxTab => "--max-tab",
            Self::StyleEdition => "--style-edition",
        }
    }

    pub const fn help(self) -> &'static str {
        match self {
            Self::MaxWidth => {
                "Soft maximum line width, at least 1. Long or deeply nested values may overrun it."
            }
            Self::TabSize => "Indentation per nesting level, in spaces; at least 1 and at most max_tab.",
            Self::BlankLines => {
                "Keep: one blank line wherever the input has one or more. Remove: none."
            }
            Self::MaxDepth => "Deeper input is rejected instead of overflowing the stack.",
            Self::MaxTab => "Upper bound on tab_size; a larger tab_size is an error.",
            Self::StyleEdition => "The set of layout rules. 2026 is the only edition so far.",
        }
    }

    pub const fn kind(self) -> Kind {
        match self {
            Self::MaxWidth | Self::TabSize | Self::MaxTab | Self::MaxDepth => {
                Kind::Number { min: 1 }
            }
            Self::BlankLines => Kind::Choice(&["Keep", "Remove"]),
            Self::StyleEdition => Kind::Choice(&["2026"]),
        }
    }

    /// The value the file sets, if it sets one.
    pub fn get(self, file: &FileConfig) -> Option<Value> {
        match self {
            Self::MaxWidth => file.max_width.map(Value::Number),
            Self::TabSize => file.tab_size.map(Value::Number),
            Self::BlankLines => file.blank_lines.map(|b| Value::Choice(blank_lines(b))),
            Self::MaxDepth => file.max_depth.map(|d| Value::Number(d.get())),
            Self::MaxTab => file.max_tab.map(Value::Number),
            Self::StyleEdition => file.style_edition.map(|e| Value::Choice(e.to_string())),
        }
    }

    /// The value in effect: the file's, or the default.
    pub fn effective(self, config: &Config) -> Value {
        match self {
            Self::MaxWidth => Value::Number(config.max_width),
            Self::TabSize => Value::Number(config.tab_size),
            Self::BlankLines => Value::Choice(blank_lines(config.blank_lines)),
            Self::MaxDepth => Value::Number(config.max_nesting),
            Self::MaxTab => Value::Number(config.max_tab),
            Self::StyleEdition => Value::Choice(config.style_edition.to_string()),
        }
    }
}

fn blank_lines(b: BlankLines) -> String {
    format!("{b:?}")
}
