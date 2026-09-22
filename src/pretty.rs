//! A Wadler/Leijen pretty-printer (`Doc` algebra) with a real `fits()` check.
//!
//! `group(d)` renders `d` flat if the *whole* rest of the current line fits
//! within the width (measured from the actual column, so a `key: ` prefix
//! counts), otherwise with line breaks. `nest(k, d)` indents broken lines by
//! `k`. Separators are `line()` (flat: " ", broken: newline) and `soft_line()`
//! (flat: "", broken: newline). `hard_line()` always breaks, even in flat
//! mode, and short-circuits `fits()` (content up to a hard break never
//! overruns). `if_break(flat, broken)` picks a rendering by mode — used for
//! the trailing comma, which only appears when a container breaks.
//!
//! Widths are measured in display columns (`unicode-width`), not bytes, so
//! multibyte text does not trigger premature line breaks.

use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone)]
pub enum Doc {
    Nil,
    Text(String),
    /// `soft`: flat renders as "" (e.g. before a closing bracket);
    /// otherwise flat renders as " ". Broken always renders as newline + indent.
    Line {
        soft: bool,
    },
    /// Always a newline + indent, regardless of mode.
    HardLine,
    IfBreak {
        flat: Box<Doc>,
        broken: Box<Doc>,
    },
    Nest(usize, Box<Doc>),
    Group(Box<Doc>),
    Concat(Vec<Doc>),
}

pub fn text(s: impl Into<String>) -> Doc {
    Doc::Text(s.into())
}

/// Flat: " "; broken: newline + indent.
pub fn line() -> Doc {
    Doc::Line { soft: false }
}

/// Flat: ""; broken: newline + indent.
pub fn soft_line() -> Doc {
    Doc::Line { soft: true }
}

/// Always a newline + indent.
pub fn hard_line() -> Doc {
    Doc::HardLine
}

pub fn if_break(flat: Doc, broken: Doc) -> Doc {
    Doc::IfBreak {
        flat: Box::new(flat),
        broken: Box::new(broken),
    }
}

/// A comma that appears only when the enclosing group breaks.
pub fn comma() -> Doc {
    if_break(Doc::Nil, text(","))
}

pub fn nest(n: usize, d: Doc) -> Doc {
    Doc::Nest(n, Box::new(d))
}

pub fn group(d: Doc) -> Doc {
    Doc::Group(Box::new(d))
}

pub fn concat(docs: Vec<Doc>) -> Doc {
    match docs.len() {
        0 => Doc::Nil,
        1 => docs.into_iter().next().unwrap(),
        _ => Doc::Concat(docs),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Flat,
    Break,
}

/// Render `doc` at width `width`, starting at column 0.
pub fn render(doc: &Doc, width: usize) -> String {
    let mut out = String::new();
    best(&mut out, doc, width, 0, 0, Mode::Break);
    out
}

/// `indent` = indentation to print after a line break; `col` = the *actual*
/// current column (advances with emitted text). Only `col` feeds the fit
/// checks, which is what catches "key: " prefixes pushing a value past the
/// width.
fn best(out: &mut String, doc: &Doc, width: usize, indent: usize, col: usize, mode: Mode) -> usize {
    match doc {
        Doc::Nil => col,
        Doc::Text(s) => {
            out.push_str(s);
            col + s.width()
        }
        Doc::Line { soft } => match mode {
            Mode::Flat => {
                if *soft {
                    col
                } else {
                    out.push(' ');
                    col + 1
                }
            }
            Mode::Break => {
                out.push('\n');
                out.push_str(&" ".repeat(indent));
                indent
            }
        },
        Doc::HardLine => {
            out.push('\n');
            out.push_str(&" ".repeat(indent));
            indent
        }
        Doc::IfBreak { flat, broken } => match mode {
            Mode::Flat => best(out, flat, width, indent, col, mode),
            Mode::Break => best(out, broken, width, indent, col, mode),
        },
        Doc::Nest(n, d) => best(out, d, width, indent + n, col, mode),
        Doc::Group(d) => {
            // A group reached directly by `best` (not intercepted by the
            // `Concat` arm) has no siblings to consider — it flattens iff its
            // own flat form fits the remaining width.
            if fits_rest(
                width.saturating_sub(col),
                Mode::Flat,
                std::slice::from_ref(d),
            ) {
                best(out, d, width, indent, col, Mode::Flat)
            } else {
                best(out, d, width, indent, col, Mode::Break)
            }
        }
        Doc::Concat(docs) => {
            let mut c = col;
            let mut i = 0;
            while i < docs.len() {
                let d = &docs[i];
                if let Doc::Group(inner) = d {
                    // Decide the group from its *whole* share of the current
                    // line: its own flat form plus whatever follows it up to
                    // the next line break (e.g. the trailing comma of a
                    // broken container, which would otherwise push the line
                    // one character past the width).
                    let fits = fits_rest(width.saturating_sub(c), mode, &docs[i..]);
                    let m = if fits { Mode::Flat } else { Mode::Break };
                    c = best(out, inner, width, indent, c, m);
                    i += 1;
                } else {
                    c = best(out, d, width, indent, c, mode);
                    i += 1;
                }
            }
            c
        }
    }
}

/// Result of probing how one `Doc` renders on the current line.
#[derive(PartialEq)]
enum Fit {
    /// No line break yet — keep scanning.
    Continue,
    /// Content would run past the remaining width.
    Overflow,
    /// Renders as a line break in `mode` — the current line ends here.
    LineBreak,
}

/// How `d` renders on the current line in `mode`: how many columns it costs
/// and whether it ends the line.
fn fits_probe(rem: &mut usize, mode: Mode, d: &Doc) -> Fit {
    match d {
        Doc::Nil => Fit::Continue,
        Doc::Text(s) => {
            let w = s.width();
            if w > *rem {
                return Fit::Overflow;
            }
            *rem -= w;
            Fit::Continue
        }
        Doc::Line { soft } => match mode {
            Mode::Flat => {
                if !*soft {
                    if *rem == 0 {
                        return Fit::Overflow;
                    }
                    *rem -= 1;
                }
                Fit::Continue
            }
            Mode::Break => Fit::LineBreak,
        },
        Doc::HardLine => Fit::LineBreak,
        Doc::IfBreak { flat, broken } => {
            let inner = if mode == Mode::Flat { flat } else { broken };
            fits_probe(rem, mode, inner)
        }
        Doc::Nest(_, d) => fits_probe(rem, mode, d),
        // A nested group is assumed to flatten while measuring the current
        // line; its own break decision is made separately when rendered.
        Doc::Group(d) => fits_probe(rem, Mode::Flat, d),
        Doc::Concat(docs) => {
            for x in docs {
                match fits_probe(rem, mode, x) {
                    Fit::Continue => {}
                    other => return other,
                }
            }
            Fit::Continue
        }
    }
}

/// True if `docs` fit in `rem` remaining columns on the current line, where
/// every doc renders in `mode` and scanning stops at the first line break.
///
/// `docs[0]` is the group whose flat/break decision is being made; it is
/// always measured in flat mode (via `fits_probe`). Any *following* `Group`
/// (index > 0) is treated as a boundary: it will decide independently whether
/// to flatten or break, so its content must not be counted against the current
/// group's share of the line. This prevents a large sibling (e.g. a container
/// value in a map entry) from forcing a small container key to break
/// unnecessarily.
fn fits_rest(rem: usize, mode: Mode, docs: &[Doc]) -> bool {
    let mut rem = rem;
    for (i, d) in docs.iter().enumerate() {
        let fit = if i > 0 && matches!(d, Doc::Group(_)) {
            Fit::LineBreak
        } else {
            fits_probe(&mut rem, mode, d)
        };
        match fit {
            Fit::Continue => {}
            Fit::Overflow => return false,
            Fit::LineBreak => return true,
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fit checks measure display columns (unicode width), not bytes: this
    /// group renders 12 columns but 20 bytes, so it stays flat at width 12.
    #[test]
    fn fits_measures_display_width_not_bytes() {
        let doc = group(concat(vec![text("\"αααααααα\""), line(), text("2")]));
        assert_eq!(render(&doc, 12), "\"αααααααα\" 2");
        // One column less and it must break.
        assert_eq!(render(&doc, 11), "\"αααααααα\"\n2");
    }
}
