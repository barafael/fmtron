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

#[derive(Debug, Clone)]
pub enum Doc {
    Nil,
    Text(String),
    /// `soft`: flat renders as "" (e.g. before a closing bracket);
    /// otherwise flat renders as " ". Broken always renders as newline + indent.
    Line { soft: bool },
    /// Always a newline + indent, regardless of mode.
    HardLine,
    IfBreak { flat: Box<Doc>, broken: Box<Doc> },
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
/// current column (advances with emitted text). Only `col` feeds `fits()`,
/// which is what catches "key: " prefixes pushing a value past the width.
fn best(out: &mut String, doc: &Doc, width: usize, indent: usize, col: usize, mode: Mode) -> usize {
    match doc {
        Doc::Nil => col,
        Doc::Text(s) => {
            out.push_str(s);
            col + s.len()
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
            if fits(width.saturating_sub(col), d) {
                best(out, d, width, indent, col, Mode::Flat)
            } else {
                best(out, d, width, indent, col, Mode::Break)
            }
        }
        Doc::Concat(docs) => {
            let mut c = col;
            for d in docs {
                c = best(out, d, width, indent, c, mode);
            }
            c
        }
    }
}

/// True if the content of `doc` up to the first line break fits in `rem`
/// remaining columns. A hard break means the line ends early, so it always
/// fits.
fn fits(rem: usize, doc: &Doc) -> bool {
    let mut stack: Vec<&Doc> = vec![doc];
    let mut rem = rem;
    while let Some(d) = stack.pop() {
        match d {
            Doc::Nil => {}
            Doc::Text(s) => {
                if s.len() > rem {
                    return false;
                }
                rem -= s.len();
            }
            Doc::Line { soft } => {
                if !*soft {
                    if rem == 0 {
                        return false;
                    }
                    rem -= 1;
                }
            }
            Doc::HardLine => return true,
            Doc::IfBreak { flat, .. } => stack.push(flat),
            Doc::Nest(_, d) => stack.push(d),
            Doc::Group(d) => stack.push(d),
            Doc::Concat(docs) => {
                for x in docs.iter().rev() {
                    stack.push(x);
                }
            }
        }
    }
    true
}
