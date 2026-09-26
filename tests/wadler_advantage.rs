//! Regression tests for the Wadler/Leijen `Doc` printer that now powers
//! `format_ron` (see `src/pretty.rs`).
//!
//! These lock in the properties that motivated replacing the old greedy
//! heuristic, which decided flat-vs-broken from a value's own indentation and
//! ignored the `key: ` prefix already on the line — producing lines wider than
//! `max_width`. `group`/`fits()` measures from the actual column, breaks
//! exactly what does not fit, and keeps a nested group flat when it fits.
//!
//! Every case is also rendered by an independent pest-tree → `Doc` builder
//! (`support::pretty::format_wadler`) and asserted to agree with the AST-based
//! printer, so a layout bug in either construction fails loudly.

mod support;

use fmtron::{Config, format_ron};
use support::pretty::{format_wadler, max_line_len};

fn formatted(input: &str, width: usize) -> String {
    format_ron(
        input,
        &Config {
            tab_size: 4,
            max_width: width,
            ..Config::default()
        },
    )
    .expect("valid RON")
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
        "{\n    Foo(\n        3.5,\n        1e10,\n    ): {1: 2},\n    [1]: Bar(a: 1),\n}"
    );
}

/// W2: a one-element wrapper around a container (`Some((…))`, a newtype
/// variant) hugs when the element must break anyway, as `ron`'s own
/// pretty-printer writes it; when the element fits flat on its own line, the
/// wrapper breaks around it like any container; when everything fits, flat.
#[test]
fn single_container_wrappers_hug_only_when_the_child_must_break() {
    let hugged = formatted(
        "(env: Some((a: 1, b: 2, c: 3)), tint: Some(((red: 1.0, green: 0.5))))",
        20,
    );
    assert_eq!(
        hugged,
        "(\n    env: Some((\n        a: 1,\n        b: 2,\n        c: 3,\n    )),\n    tint: Some(((\n        red: 1.0,\n        green: 0.5,\n    ))),\n)"
    );
    // The child fits on its own line: break around it instead of hugging.
    assert_eq!(
        formatted("TupleNewtypeTupleStruct(TupleStruct(4, false))", 40),
        "TupleNewtypeTupleStruct(\n    TupleStruct(4, false),\n)"
    );
    // Everything fits: flat. Nested wrappers hug together.
    assert_eq!(formatted("Some(Some([1, 2]))", 40), "Some(Some([1, 2]))");
    assert_eq!(
        formatted("Some(Some([111, 222, 333]))", 12),
        "Some(Some([\n    111,\n    222,\n    333,\n]))"
    );
    // An atom never hugs: a long string still gets its own line.
    assert_eq!(
        formatted(r#"Some("a long string that cannot fit")"#, 20),
        "Some(\n    \"a long string that cannot fit\",\n)"
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
