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

use fmtron::{format_ron, Config};
use support::pretty::{format_wadler, max_line_len};

fn formatted(input: &str, width: usize) -> String {
    format_ron(input, &Config {
        tab_size: 4,
        max_width: width,
    })
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
