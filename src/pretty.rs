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
    /// `[open, child, close]`, e.g. `Some(` `(…)` `)`; see [`hug`].
    Hug {
        tab: usize,
        parts: Box<[Doc; 3]>,
    },
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

/// `open child close` for a wrapper around a single container, such as
/// `Some((…))`. Renders flat if it all fits. Otherwise, if `child` fits flat
/// on a line of its own, it breaks like a container (`Some(` / child / `)`);
/// if even that does not fit, it "hugs": `open` and `close` stay on the
/// child's first and last lines and the child breaks inside (`Some((` … `))`),
/// the layout `ron`'s own pretty-printer uses.
pub fn hug(open: Doc, child: Doc, close: Doc, tab: usize) -> Doc {
    Doc::Hug {
        tab,
        parts: Box::new([open, child, close]),
    }
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
    render_with_newline(doc, width, "\n")
}

/// Like [`render`], but line breaks emit `newline` (e.g. `"\r\n"`).
pub fn render_with_newline(doc: &Doc, width: usize, newline: &str) -> String {
    let mut out = Out {
        buf: String::new(),
        newline,
    };
    best(&mut out, doc, width, 0, 0, Mode::Break);
    out.buf
}

/// The output buffer plus the line ending to emit at breaks.
struct Out<'a> {
    buf: String,
    newline: &'a str,
}

impl Out<'_> {
    /// A line break followed by `indent` spaces.
    fn break_line(&mut self, indent: usize) {
        self.buf.push_str(self.newline);
        self.buf.extend(std::iter::repeat_n(' ', indent));
    }
}

/// `indent` = indentation to print after a line break; `col` = the *actual*
/// current column (advances with emitted text). Only `col` feeds the fit
/// checks, which is what catches "key: " prefixes pushing a value past the
/// width.
fn best(out: &mut Out, doc: &Doc, width: usize, indent: usize, col: usize, mode: Mode) -> usize {
    match doc {
        Doc::Nil => col,
        Doc::Text(s) => {
            out.buf.push_str(s);
            col + s.width()
        }
        Doc::Line { soft } => match mode {
            Mode::Flat => {
                if *soft {
                    col
                } else {
                    out.buf.push(' ');
                    col + 1
                }
            }
            Mode::Break => {
                out.break_line(indent);
                indent
            }
        },
        Doc::HardLine => {
            out.break_line(indent);
            indent
        }
        Doc::IfBreak { flat, broken } => match mode {
            Mode::Flat => best(out, flat, width, indent, col, mode),
            Mode::Break => best(out, broken, width, indent, col, mode),
        },
        Doc::Nest(n, d) => best(out, d, width, indent.saturating_add(*n), col, mode),
        // A group or hug reached directly by `best` (not intercepted by
        // `best_seq`) has no siblings to consider: it flattens iff its own
        // flat form fits the remaining width.
        Doc::Group(_) | Doc::Hug { .. } => {
            best_seq(out, std::slice::from_ref(doc), width, indent, col, mode)
        }
        Doc::Concat(docs) => best_seq(out, docs, width, indent, col, mode),
    }
}

/// Render `docs` in sequence. Each `Group`/`Hug` is decided from its *whole*
/// share of the current line: its own flat form plus whatever follows it up
/// to the next line break (e.g. the trailing comma of a broken container,
/// which would otherwise push the line one character past the width).
fn best_seq(
    out: &mut Out,
    docs: &[Doc],
    width: usize,
    indent: usize,
    col: usize,
    mode: Mode,
) -> usize {
    let mut c = col;
    for (i, d) in docs.iter().enumerate() {
        c = match d {
            Doc::Group(inner) => {
                let fits = fits_rest(width.saturating_sub(c), mode, &docs[i..]);
                let m = if fits { Mode::Flat } else { Mode::Break };
                best(out, inner, width, indent, c, m)
            }
            Doc::Hug { tab, parts } => {
                let [open, child, close] = &**parts;
                if fits_rest(width.saturating_sub(c), mode, &docs[i..]) {
                    let c = best(out, open, width, indent, c, Mode::Flat);
                    let c = best(out, child, width, indent, c, Mode::Flat);
                    best(out, close, width, indent, c, Mode::Flat)
                } else if child_fits_alone(child, width, indent + tab) {
                    // Break around the child like a container: it then fits
                    // flat on its own line, followed by a comma.
                    let inner = indent + tab;
                    best(out, open, width, indent, c, Mode::Break);
                    out.break_line(inner);
                    let c = best_seq(
                        out,
                        std::slice::from_ref(child),
                        width,
                        inner,
                        inner,
                        Mode::Break,
                    );
                    out.buf.push(',');
                    out.break_line(indent);
                    best(out, close, width, indent, c + 1, Mode::Break)
                } else {
                    // Hug: the child breaks inside, sharing its first line
                    // with `open` and its last with `close`.
                    let c = best(out, open, width, indent, c, Mode::Break);
                    best_seq(out, &parts[1..], width, indent, c, Mode::Break)
                }
            }
            _ => best(out, d, width, indent, c, mode),
        };
    }
    c
}

/// True if `child` renders flat, plus a trailing comma, on a fresh line
/// indented by `indent`.
fn child_fits_alone(child: &Doc, width: usize, indent: usize) -> bool {
    let mut rem = width.saturating_sub(indent);
    fits_probe(&mut rem, Mode::Flat, child) != Fit::Overflow && rem >= 1
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
        Doc::Hug { parts, .. } => fits_seq(rem, Mode::Flat, &parts[..]),
        Doc::Concat(docs) => fits_seq(rem, mode, docs),
    }
}

/// `fits_probe` over a sequence: stops at the first overflow or line break.
fn fits_seq(rem: &mut usize, mode: Mode, docs: &[Doc]) -> Fit {
    for x in docs {
        match fits_probe(rem, mode, x) {
            Fit::Continue => {}
            other => return other,
        }
    }
    Fit::Continue
}

/// True if `docs` fit in `rem` remaining columns on the current line, where
/// every doc renders in `mode` and scanning stops at the first line break.
///
/// `docs[0]` is the group whose flat/break decision is being made; it is
/// always measured in flat mode (via `fits_probe`). Any *following* `Group`
/// (index > 0) decides independently whether to flatten or break, so it is
/// measured in break mode: only its text up to its first possible line break
/// (e.g. the `Foo(` of a map value) must share the current line. A large
/// sibling therefore cannot force a small container key to break, but the
/// key still breaks when even `key: Foo(` would overrun the width.
fn fits_rest(rem: usize, mode: Mode, docs: &[Doc]) -> bool {
    let mut rem = rem;
    for (i, d) in docs.iter().enumerate() {
        let fit = match d {
            Doc::Group(inner) if i > 0 => fits_probe(&mut rem, Mode::Break, inner),
            // A following hug breaks right after its opener at the latest.
            Doc::Hug { parts, .. } if i > 0 => match fits_probe(&mut rem, Mode::Break, &parts[0]) {
                Fit::Continue => Fit::LineBreak,
                other => other,
            },
            _ => fits_probe(&mut rem, mode, d),
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
