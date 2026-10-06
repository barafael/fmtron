//! Wadler/Leijen pretty-printer support used by the `wadler_advantage` and
//! `fuzz` tests.
//!
//! The `Doc` algebra itself now lives in the library (`fmtron::pretty`) — this
//! module re-exports it and keeps the pest-tree → `Doc` builder that lets the
//! tests run the printer on real RON input. Each test binary compiles the
//! module on its own, so dead-code warnings would otherwise fire for the parts
//! a given binary does not use.
#![allow(dead_code)]
pub use fmtron::pretty::{
    Doc, FillItem, comma, concat, fill, group, line, nest, render, soft_line, text,
};

use fmtron::{RonParser, Rule};
use pest::{Parser, iterators::Pair};

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

/// True if the `value` pair is an atom.
fn is_atom(v: &Pair<Rule>) -> bool {
    v.clone().into_inner().all(|c| {
        matches!(
            c.as_rule(),
            Rule::bool
                | Rule::char
                | Rule::string
                | Rule::byte_string
                | Rule::signed_int
                | Rule::float
                | Rule::unit_type
        )
    })
}

/// True if the `value` pair may share a line in a packed list: a number,
/// bool or char, or another atom or a tuple of atoms at most 16 columns wide.
fn is_short(v: &Pair<Rule>) -> bool {
    let scalar = v.clone().into_inner().all(|c| {
        matches!(
            c.as_rule(),
            Rule::bool | Rule::char | Rule::signed_int | Rule::float
        )
    });
    if scalar {
        return true;
    }
    let shape = is_atom(v)
        || v.clone().into_inner().all(|c| {
            c.as_rule() == Rule::tuple_type
                && c.clone()
                    .into_inner()
                    .filter(|x| x.as_rule() == Rule::value)
                    .all(|x| is_atom(&x))
        });
    shape && render(&value_to_doc(v, 0), usize::MAX).chars().count() <= 16
}

/// A list or tuple of two or more short elements, packed several to a line;
/// `None` otherwise. The input is on one line, so it counts as already
/// packed whatever the elements are.
fn packed_doc(values: &[Pair<Rule>], open: &str, close: &str, tab: usize) -> Option<Doc> {
    if values.len() < 2 || !values.iter().all(is_short) {
        return None;
    }
    let items = values
        .iter()
        .map(|v| FillItem {
            blank_before: false,
            starts_line: false,
            doc: value_to_doc(v, tab),
        })
        .collect();
    Some(group(concat(vec![
        text(open),
        nest(tab, concat(vec![soft_line(), fill(items, false)])),
        soft_line(),
        text(close),
    ])))
}

fn list_doc(p: &Pair<Rule>, tab: usize) -> Doc {
    let values: Vec<Pair<Rule>> = p
        .clone()
        .into_inner()
        .filter(|c| c.as_rule() == Rule::value)
        .collect();
    packed_doc(&values, "[", "]", tab).unwrap_or_else(|| {
        let items = values.iter().map(|v| value_to_doc(v, tab)).collect();
        container_doc("[", "]", items, tab)
    })
}

fn map_doc(p: &Pair<Rule>, tab: usize) -> Doc {
    let mut entries: Vec<Doc> = Vec::new();
    for e in p
        .clone()
        .into_inner()
        .filter(|c| c.as_rule() == Rule::map_entry)
    {
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
    let mut values: Vec<Pair<Rule>> = Vec::new();
    for c in p.clone().into_inner() {
        match c.as_rule() {
            Rule::ident if ident.is_none() => ident = Some(c.as_str()),
            Rule::value => values.push(c),
            _ => {}
        }
    }
    let open = format!("{}(", ident.unwrap_or(""));
    packed_doc(&values, &open, ")", tab).unwrap_or_else(|| {
        let items = values.iter().map(|v| value_to_doc(v, tab)).collect();
        container_doc(&open, ")", items, tab)
    })
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
/// The input must be on one line: the library keeps some of the input's
/// line breaks, which this printer does not know.
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
    use indoc::indoc;

    #[test]
    fn group_flattens_when_it_fits_and_breaks_otherwise() {
        let list = concat(vec![
            text("["),
            nest(
                4,
                concat(vec![
                    soft_line(),
                    item(text("1"), false),
                    line(),
                    item(text("2"), true),
                ]),
            ),
            soft_line(),
            text("]"),
        ]);
        // Wide enough: one flat line, no trailing comma.
        assert_eq!(render(&group(list.clone()), 40), "[1, 2]");
        // Too narrow: each element on its own line, trailing comma kept.
        assert_eq!(
            render(&group(list.clone()), 5),
            indoc! {"
                [
                    1,
                    2,
                ]"}
        );
    }

    #[test]
    fn fits_measures_from_the_actual_column() {
        // `[1, 2, 3]` (9 chars) fits on a fresh line at width 10...
        let list = concat(vec![
            text("["),
            nest(
                4,
                concat(vec![
                    soft_line(),
                    item(text("1"), false),
                    line(),
                    item(text("2"), false),
                    line(),
                    item(text("3"), true),
                ]),
            ),
            soft_line(),
            text("]"),
        ]);
        assert_eq!(render(&group(list.clone()), 10), "[1, 2, 3]");
        // ...but does NOT fit after a 3-char "k: " prefix: remaining is 7.
        let entry = group(concat(vec![text("k: "), list]));
        assert_eq!(
            render(&entry, 10),
            indoc! {"
                k: [
                    1,
                    2,
                    3,
                ]"}
        );
    }
}
