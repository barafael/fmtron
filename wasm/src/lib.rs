//! WebAssembly bindings to the [fmtron](https://github.com/barafael/fmtron)
//! RON formatter, for the [online demo](https://barafael.github.io/fmtron/).

use fmtron::FormatError;
use wasm_bindgen::prelude::*;

/// Why formatting failed, with the position for a parse error so that the
/// page can mark it. Thrown by [`format_ron`]; the page reads its getters
/// and calls `free()`.
#[wasm_bindgen]
pub struct FormatFailure {
    kind: &'static str,
    line: u32,
    column: u32,
    message: String,
    rendered: String,
}

#[wasm_bindgen]
impl FormatFailure {
    /// `parse`, `empty` (no value in the input), `too_deep`, `indent_too_wide`
    /// or `other`.
    #[wasm_bindgen(getter)]
    pub fn kind(&self) -> String {
        self.kind.to_string()
    }

    /// The line of a parse error, counting from 1; 0 for any other error.
    #[wasm_bindgen(getter)]
    pub fn line(&self) -> u32 {
        self.line
    }

    /// The column of a parse error, counting from 1 in characters; 0 for
    /// any other error.
    #[wasm_bindgen(getter)]
    pub fn column(&self) -> u32 {
        self.column
    }

    /// One line saying what went wrong, without the position.
    #[wasm_bindgen(getter)]
    pub fn message(&self) -> String {
        self.message.clone()
    }

    /// The full error as fmtron prints it: the offending line, a caret and
    /// the message.
    #[wasm_bindgen(getter)]
    pub fn rendered(&self) -> String {
        self.rendered.clone()
    }
}

impl From<FormatError> for FormatFailure {
    fn from(e: FormatError) -> Self {
        let (kind, line, column, message) = match &e {
            FormatError::Parse(p) => ("parse", p.line(), p.column(), p.message()),
            FormatError::Empty => ("empty", 0, 0, e.to_string()),
            FormatError::TooDeep { .. } => ("too_deep", 0, 0, e.to_string()),
            FormatError::IndentTooWide { .. } => ("indent_too_wide", 0, 0, e.to_string()),
            _ => ("other", 0, 0, e.to_string()),
        };
        Self {
            kind,
            line: line.try_into().unwrap_or(u32::MAX),
            column: column.try_into().unwrap_or(u32::MAX),
            message,
            rendered: e.to_string(),
        }
    }
}

/// Formats RON text with fmtron's layout rules.
#[wasm_bindgen]
pub fn format_ron(
    input: &str,
    tab_size: u32,
    max_width: u32,
    keep_blank_lines: bool,
) -> Result<String, FormatFailure> {
    format(input, tab_size, max_width, keep_blank_lines).map_err(Into::into)
}

/// The formatting itself, callable on any target: exported functions refuse
/// to run in a native test harness.
fn format(
    input: &str,
    tab_size: u32,
    max_width: u32,
    keep_blank_lines: bool,
) -> Result<String, FormatError> {
    let mut config = fmtron::Config::default()
        .with_tab_size(tab_size.max(1) as usize)
        .with_max_width(max_width.max(1) as usize);
    if !keep_blank_lines {
        config = config.with_blank_lines(fmtron::BlankLines::Remove);
    }
    fmtron::format_ron(input, &config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_and_reports_parse_errors() {
        let out = format("(  bar :  \"baz\"  ,foo:1 )  ", 4, 100, true).unwrap();
        assert_eq!(out, "(bar: \"baz\", foo: 1)");

        // A blank line forces the container to stay broken; blank_lines =
        // remove drops it and the one-line form fits.
        let out = format("(a: 1,\n\nb: 2)", 4, 100, true).unwrap();
        assert_eq!(out, "(\n    a: 1,\n\n    b: 2,\n)");
        let out = format("(a: 1,\n\nb: 2)", 4, 100, false).unwrap();
        assert_eq!(out, "(a: 1, b: 2)");

        let failure: FormatFailure = format("(a: 1 b: 2)", 4, 100, true).unwrap_err().into();
        assert_eq!(failure.kind, "parse");
        assert_eq!((failure.line, failure.column), (1, 7));
        assert_eq!(failure.message, "expected `,` or `)`, found `b`");
        assert!(failure.rendered.contains("^---"), "{}", failure.rendered);

        // Other errors carry no position.
        let deep: FormatFailure = format(&"[".repeat(600), 4, 100, true).unwrap_err().into();
        assert_eq!((deep.kind, deep.line, deep.column), ("too_deep", 0, 0));
        assert!(deep.message.contains("nested"), "{}", deep.message);
        let empty: FormatFailure = format("// nothing yet", 4, 100, true).unwrap_err().into();
        assert_eq!(empty.kind, "empty");
    }
}
