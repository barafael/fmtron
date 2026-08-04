/// A compact Wadler/Leijen pretty-printer (`Doc` algebra) used by the
/// `wadler_advantage` tests to demonstrate what a `fits()`-based printer gives
/// us that the greedy heuristic does not.
///
/// Model: `Doc` is a tree; `group(d)` renders `d` flat if the *whole* rest of
/// the current line fits within the width (true `fits()` check from the actual
/// column), otherwise with line breaks. `nest(k, d)` indents broken lines by
/// `k`. Separators are `line()` (flat: " ", broken: newline) and `soft_line()`
/// (flat: "", broken: newline). `if_break(flat, broken)` picks a rendering by
/// mode (used for the trailing comma, which only appears when a container
/// breaks).
use fmtron::{RonParser, Rule};
use pest::{iterators::Pair, Parser};

#[derive(Debug, Clone)]
pub enum Doc {
    Nil,
    Text(String),
    /// `soft`: flat renders as "" (e.g. before a closing bracket);
    /// otherwise flat renders as " ". Broken always renders as newline + indent.
    Line { soft: bool },
    IfBreak { flat: Box<Doc>, broken: Box<Doc> },
    Nest(usize, Box<Doc>),
    Group(Box<Doc>),
    Concat(Vec<Doc>),
}

pub fn text(s: impl Into<String>) -> Doc {
    Doc::Text(s.into())
}

pub fn line() -> Doc {
    Doc::Line { soft: false }
}

pub fn soft_line() -> Doc {
    Doc::Line { soft: true }
}

pub fn if_break(flat: Doc, broken: Doc) -> Doc {
    Doc::IfBreak {
        flat: Box::new(flat),
        broken: Box::new(broken),
    }
}

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

/// True if the flat rendering of `doc` fits in `rem` remaining columns.
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
            Doc::IfBreak { flat, .. } => stack.push(flat),            Doc::Nest(_, d) => stack.push(d),
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

/// Walk the pest tree for a RON value and build a `Doc`. Comments are dropped
/// (this is a layout demo, not the comment-aware printer).
pub fn value_to_doc(p: &Pair<Rule>, tab: usize) -> Doc {
    for c in p.clone().into_inner() {
        match c.as_rule() {
            Rule::bool
            | Rule::char
            | Rule::string
            | Rule::byte_string
            | Rule::signed_int
            | Rule::float
            | Rule::unit_type => return text(c.as_str()),
            Rule::list => return list_doc(&c, tab),
            Rule::map => return map_doc(&c, tab),
            Rule::tuple_type => return tuple_doc(&c, tab),
            Rule::fields_type => return fields_doc(&c, tab),
            _ => {}
        }
    }
    unreachable!("value with no children")
}

/// One entry: a conditional trailing comma means the flat form has no trailing
/// comma (matching the greedy printer) while the broken form keeps one.
fn item(value: Doc, last: bool) -> Doc {
    if last {
        concat(vec![value, comma()])
    } else {
        concat(vec![value, text(",")])
    }
}

fn container_doc(open: &str, close: &str, items: Vec<Doc>, tab: usize) -> Doc {
    let n = items.len();
    let mut inner: Vec<Doc> = Vec::new();
    for (i, value) in items.into_iter().enumerate() {
        if i > 0 {
            inner.push(line());
        }
        inner.push(item(value, i == n.saturating_sub(1)));
    }
    group(concat(vec![
        text(open),
        nest(tab, concat(vec![soft_line(), concat(inner)])),
        soft_line(),
        text(close),
    ]))
}

fn list_doc(p: &Pair<Rule>, tab: usize) -> Doc {
    let items: Vec<Doc> = p
        .clone()
        .into_inner()
        .filter(|c| c.as_rule() == Rule::value)
        .map(|v| value_to_doc(&v, tab))
        .collect();
    container_doc("[", "]", items, tab)
}

fn map_doc(p: &Pair<Rule>, tab: usize) -> Doc {
    let mut entries: Vec<Doc> = Vec::new();
    for e in p.clone().into_inner().filter(|c| c.as_rule() == Rule::map_entry) {
        let mut inner = e.into_inner().filter(|c| c.as_rule() == Rule::value);
        let k = inner.next().unwrap();
        let v = inner.next().unwrap();
        entries.push(concat(vec![
            value_to_doc(&k, tab),
            text(": "),
            value_to_doc(&v, tab),
        ]));
    }
    container_doc("{", "}", entries, tab)
}

fn tuple_doc(p: &Pair<Rule>, tab: usize) -> Doc {
    let mut ident = None;
    let mut items: Vec<Doc> = Vec::new();
    for c in p.clone().into_inner() {
        match c.as_rule() {
            Rule::ident if ident.is_none() => ident = Some(c.as_str()),
            Rule::value => items.push(value_to_doc(&c, tab)),
            _ => {}
        }
    }
    container_doc(&format!("{}(", ident.unwrap_or("")), ")", items, tab)
}

fn fields_doc(p: &Pair<Rule>, tab: usize) -> Doc {
    let mut ident = None;
    let mut items: Vec<Doc> = Vec::new();
    for c in p.clone().into_inner() {
        match c.as_rule() {
            Rule::ident if ident.is_none() => ident = Some(c.as_str()),
            Rule::field => {
                let mut inner = c.into_inner();
                let name = inner.find(|x| x.as_rule() == Rule::ident).unwrap();
                let value = inner.find(|x| x.as_rule() == Rule::value).unwrap();
                items.push(concat(vec![
                    text(format!("{}: ", name.as_str())),
                    value_to_doc(&value, tab),
                ]));
            }
            _ => {}
        }
    }
    container_doc(&format!("{}(", ident.unwrap_or("")), ")", items, tab)
}

/// Format a RON string with the Wadler printer. `tab` is the indent width.
pub fn format_wadler(input: &str, width: usize, tab: usize) -> Result<String, String> {
    let mut pairs = RonParser::parse(Rule::ron_file, input).map_err(|e| format!("{e}"))?;
    let file = pairs.next().ok_or("empty input")?;
    let value = file
        .into_inner()
        .find(|p| p.as_rule() == Rule::value)
        .ok_or("no value")?;
    Ok(render(&group(value_to_doc(&value, tab)), width))
}

pub fn max_line_len(s: &str) -> usize {
    s.lines().map(str::len).max().unwrap_or(0)
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn group_flattens_when_it_fits_and_breaks_otherwise() {
        let list = concat(vec![
            text("["),
            nest(4, concat(vec![
                soft_line(),
                item(text("1"), false),
                line(),
                item(text("2"), true),
            ])),
            soft_line(),
            text("]"),
        ]);
        // Wide enough: one flat line, no trailing comma.
        assert_eq!(render(&group(list.clone()), 40), "[1, 2]");
        // Too narrow: each element on its own line, trailing comma kept.
        assert_eq!(render(&group(list.clone()), 5), "[\n    1,\n    2,\n]");
    }

    #[test]
    fn fits_measures_from_the_actual_column() {
        // `[1, 2, 3]` (9 chars) fits on a fresh line at width 10...
        let list = concat(vec![
            text("["),
            nest(4, concat(vec![
                soft_line(),
                item(text("1"), false),
                line(),
                item(text("2"), false),
                line(),
                item(text("3"), true),
            ])),
            soft_line(),
            text("]"),
        ]);
        assert_eq!(render(&group(list.clone()), 10), "[1, 2, 3]");
        // ...but does NOT fit after a 3-char "k: " prefix: remaining is 7.
        let entry = group(concat(vec![text("k: "), list]));
        assert_eq!(render(&entry, 10), "k: [\n    1,\n    2,\n    3,\n]");
    }
}
