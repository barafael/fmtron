//! Demonstrates the advantages of a Wadler/Leijen `Doc` printer over the
//! greedy heuristic currently in `format_ron`.
//!
//! The greedy printer decides whether a value is flat by comparing its flat
//! length against the width using only its own indentation
//! (`tabs * tab_size + len > max_width` in `src/ast/display.rs`). It ignores
//! text already emitted on the current line — most importantly the `key: `
//! prefix of a map/struct entry — so it keeps a value flat when the real line
//! overruns `max_width`. The Wadler printer's `group`/`fits()` decides from
//! the *actual* column, so it breaks exactly what does not fit and never
//! exceeds the width (the only exceptions are unbreakable tokens that are
//! themselves wider than the limit).

mod support;

use fmtron::{format_ron, Config};
use support::pretty::{format_wadler, max_line_len};

fn greedy(input: &str, width: usize) -> String {
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
    let w = format_wadler(input, 60, 4).unwrap();
    assert_eq!(w.lines().count(), 1, "wadler broke a doc that fits:\n{w}");
    // Flat rendering matches the greedy printer's.
    assert_eq!(w, greedy(input, 60));
}

/// Greedy keeps a short list flat after a long key and overruns the width
/// (35 chars at width 30). Wadler breaks only the list, so every line fits.
#[test]
fn map_value_does_not_overrun_width() {
    let input = "{a_very_long_map_key: [1, 2, 3], b: 2}";
    let width = 30;

    let g = greedy(input, width);
    assert!(
        max_line_len(&g) > width,
        "expected greedy to overrun; got:\n{g}"
    );

    let w = format_wadler(input, width, 4).unwrap();
    assert!(max_line_len(&w) <= width, "wadler overran:\n{w}");
    // The list is broken; the rest of the layout is unchanged.
    assert!(w.contains("a_very_long_map_key: [\n"), "wadler:\n{w}");
    assert!(w.contains("    b: 2,\n"), "wadler:\n{w}");
}

/// Per-group precision: an inner list that fits on its line stays flat, while
/// an identical list behind a long key is broken — within the same container.
#[test]
fn inner_list_stays_flat_when_it_fits_and_breaks_when_not() {
    let input = "(short: [1, 2, 3], a_very_long_key_here: [1, 2, 3])";
    let width = 30;

    let g = greedy(input, width);
    assert!(
        max_line_len(&g) > width,
        "expected greedy to overrun; got:\n{g}"
    );

    let w = format_wadler(input, width, 4).unwrap();
    assert!(max_line_len(&w) <= width, "wadler overran:\n{w}");
    // `short: [1, 2, 3]` fits on its line -> kept flat.
    assert!(w.contains("    short: [1, 2, 3],\n"), "wadler:\n{w}");
    // `a_very_long_key_here: [1, 2, 3]` does not -> broken.
    assert!(w.contains("    a_very_long_key_here: [\n"), "wadler:\n{w}");
}

/// Wadler never exceeds the width on a corpus across several widths. Widths
/// are chosen to be at least as wide as the longest unbreakable token in each
/// case (a printer cannot wrap a single 20-char key inside 16 columns).
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
            let w = format_wadler(input, width, 4).unwrap();
            assert!(
                max_line_len(&w) <= width,
                "wadler overran width {width}:\n{w}"
            );
        }
    }
}
