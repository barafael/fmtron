//! An independent printer to cross-check the formatter against: a pest-tree
//! → `Doc` builder that knows the layout rules but none of the AST's
//! comment handling. The tests below render single-line inputs with both and
//! assert they agree, so a layout bug in either construction fails loudly,
//! and lock in the properties that motivated the Wadler/Leijen printer: the
//! old greedy heuristic decided flat-vs-broken from a value's own indentation
//! and ignored the `key: ` prefix already on the line, producing lines wider
//! than `max_width`.

use crate::pretty::{
    Doc, FillItem, comma, concat, fill, group, line, nest, render, soft_line, text,
};
use crate::{RonParser, Rule};
use pest::{Parser, iterators::Pair};

/// Walk the pest tree for a RON value and build a `Doc`. Comments are dropped
/// (this is a layout demo, not the comment-aware printer).
fn value_to_doc(p: &Pair<Rule>, tab: usize) -> Doc {
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
fn format_wadler(input: &str, width: usize, tab: usize) -> Result<String, String> {
    let mut pairs = RonParser::parse(Rule::ron_file, input).map_err(|e| format!("{e}"))?;
    let file = pairs.next().ok_or("empty input")?;
    let value = file
        .into_inner()
        .find(|p| p.as_rule() == Rule::value)
        .ok_or("no value")?;
    Ok(render(&group(value_to_doc(&value, tab)), width))
}

/// The longest line of `s`, in bytes.
fn max_line_len(s: &str) -> usize {
    s.lines().map(str::len).max().unwrap_or(0)
}

mod layout {
    use super::{format_wadler, max_line_len};
    use crate::{Config, format_ron};
    use indoc::indoc;

    fn formatted(input: &str, width: usize) -> String {
        format_ron(input, &Config::default().with_max_width(width)).expect("valid RON")
    }

    /// The whole document stays on one line when width allows it.
    #[test]
    fn whole_document_stays_flat_when_it_fits() {
        let input = "(alpha: 1, beta: 2, gamma: [1, 2, 3])";
        let out = formatted(input, 60);
        assert_eq!(out.lines().count(), 1, "broke a doc that fits:\n{out}");
        assert_eq!(out, input);
        assert_eq!(out, format_wadler(input, 60, 4).unwrap());
    }

    /// A short list after a long key would overrun the width if kept flat
    /// (35 chars at width 30). The printer breaks only the list.
    #[test]
    fn map_value_does_not_overrun_width() {
        let input = "{a_very_long_map_key: [1, 2, 3], b: 2}";
        let width = 30;

        let out = formatted(input, width);
        assert!(max_line_len(&out) <= width, "overran the width:\n{out}");
        assert!(out.contains("a_very_long_map_key: [\n"), "out:\n{out}");
        assert!(out.contains("    b: 2,\n"), "out:\n{out}");
        assert_eq!(out, format_wadler(input, width, 4).unwrap());
    }

    /// Per-group precision: an inner list that fits on its line stays flat, while
    /// an identical list behind a long key is broken — within the same container.
    #[test]
    fn inner_list_stays_flat_when_it_fits_and_breaks_when_not() {
        let input = "(short: [1, 2, 3], a_very_long_key_here: [1, 2, 3])";
        let width = 30;

        let out = formatted(input, width);
        assert!(max_line_len(&out) <= width, "overran the width:\n{out}");
        // `short: [1, 2, 3]` fits on its line -> kept flat.
        assert!(out.contains("    short: [1, 2, 3],\n"), "out:\n{out}");
        // `a_very_long_key_here: [1, 2, 3]` does not -> broken.
        assert!(out.contains("    a_very_long_key_here: [\n"), "out:\n{out}");
        assert_eq!(out, format_wadler(input, width, 4).unwrap());
    }

    /// The printer never exceeds the width on a corpus across several widths.
    /// Widths are chosen to be at least as wide as the longest unbreakable token
    /// in each case (a printer cannot wrap a single long key inside 16 columns).
    #[test]
    fn wadler_never_overruns_a_corpus_of_nested_inputs() {
        let cases = [
            "[[[1, 2, 3], [4]], 5]",
            "(a: {x: [1, 2, 3, 4], y: (b: c)}, d: [1, 2])",
            "[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]",
            "(some_long_key: [1, 2, 3, 4, 5, 6, 7])",
        ];
        for width in [24, 30, 40] {
            for input in cases {
                let out = formatted(input, width);
                assert!(max_line_len(&out) <= width, "overran width {width}:\n{out}");
                assert_eq!(
                    out,
                    format_wadler(input, width, 4).unwrap(),
                    "AST printer disagrees with pest-tree printer (width {width})"
                );
            }
        }
    }

    /// Width is measured in display columns, not bytes: `["αα", "ββ", "γγ"]` is
    /// 18 columns but 24 bytes, so it stays flat at width 20.
    #[test]
    fn width_is_measured_in_columns_not_bytes() {
        let input = r#"["αα", "ββ", "γγ"]"#;
        let out = formatted(input, 20);
        assert_eq!(out.lines().count(), 1, "broke a doc that fits:\n{out}");
        assert_eq!(out, input);
    }

    /// A container used as a map key stays flat when it fits, even when the
    /// *value* (a following sibling group) is too wide and must break. The fit
    /// check for the key's group counts the value only up to its first possible
    /// line break (`[`), so the value's width must not force the key to break.
    #[test]
    fn container_map_key_stays_flat_when_only_the_value_breaks() {
        let input = "{ {a: [1, 2, 3]}: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] }";
        let out = formatted(input, 30);
        assert!(max_line_len(&out) <= 30, "overran:\n{out}");
        // Key container fits on its line → stays flat.
        assert!(out.contains("    {a: [1, 2, 3]}: [\n"), "out:\n{out}");
    }

    /// A container map key breaks when even its own line (`key: Foo(`) would
    /// overrun the width: a following group counts up to its first possible line
    /// break, not as zero width.
    #[test]
    fn container_map_key_breaks_when_its_line_overruns() {
        let cases = [
            ("{Foo(3.5, 1e10): {1: 2}, [1]: Bar(a: 1)}", 20),
            (r#"{["s", Foo]: (a: 1, b: 2)}"#, 20),
            (r#"[{[-22, "é", None, None]: Foo(1e10, 1e10)}]"#, 30),
        ];
        for (input, width) in cases {
            let out = formatted(input, width);
            assert!(max_line_len(&out) <= width, "overran width {width}:\n{out}");
            assert_eq!(out, format_wadler(input, width, 4).unwrap());
        }
        assert_eq!(
            formatted("{Foo(3.5, 1e10): {1: 2}, [1]: Bar(a: 1)}", 20),
            indoc! {"
                {
                    Foo(
                        3.5, 1e10,
                    ): {1: 2},
                    [1]: Bar(a: 1),
                }"}
        );
    }

    /// W2: a one-element wrapper around a container (`Some((…))`, a newtype
    /// variant) is flat when everything fits, and otherwise hugs: the element
    /// breaks inside the wrapper's parentheses, as `ron`'s own pretty-printer
    /// writes it.
    #[test]
    fn single_container_wrappers_hug_when_they_break() {
        let hugged = formatted(
            "(env: Some((a: 1, b: 2, c: 3)), tint: Some(((red: 1.0, green: 0.5))))",
            20,
        );
        assert_eq!(
            hugged,
            indoc! {"
                (
                    env: Some((
                        a: 1,
                        b: 2,
                        c: 3,
                    )),
                    tint: Some(((
                        red: 1.0,
                        green: 0.5,
                    ))),
                )"}
        );
        // Even when the child would fit on a line of its own.
        assert_eq!(
            formatted("TupleNewtypeTupleStruct(TupleStruct(4, false))", 40),
            indoc! {"
                TupleNewtypeTupleStruct(TupleStruct(
                    4, false,
                ))"}
        );
        // The hugged child stays flat only if what follows the wrapper on its
        // line fits too: here the comma would land in column 31.
        assert_eq!(
            formatted(r##"[Some((br#""#, br#""#, "")), Some(None)]"##, 30),
            indoc! {r##"
                [
                    Some((
                        br#""#, br#""#, "",
                    )),
                    Some(None),
                ]"##}
        );
        // Everything fits: flat. Nested wrappers hug together.
        assert_eq!(formatted("Some(Some([1, 2]))", 40), "Some(Some([1, 2]))");
        assert_eq!(
            formatted("Some(Some([111, 222, 333]))", 12),
            indoc! {"
                Some(Some([
                    111,
                    222,
                    333,
                ]))"}
        );
        // An atom never hugs: a long string still gets its own line.
        assert_eq!(
            formatted(r#"Some("a long string that cannot fit")"#, 20),
            indoc! {r#"
                Some(
                    "a long string that cannot fit",
                )"#}
        );
        for (input, width) in [
            (
                "(env: Some((a: 1, b: 2, c: 3)), tint: Some(((red: 1.0, green: 0.5))))",
                20,
            ),
            ("Some(Some([111, 222, 333]))", 12),
        ] {
            let out = formatted(input, width);
            assert!(max_line_len(&out) <= width, "overran width {width}:\n{out}");
            assert_eq!(formatted(&out, width), out, "not idempotent");
        }
    }
}

mod printer {
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
