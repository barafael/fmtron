//! Blank-line handling (`Config::blank_lines`). With the default
//! `BlankLines::Keep`, a blank line between two elements, comments or header
//! items survives (runs collapse to one), a container holding one breaks, and
//! blank lines right inside brackets are dropped. `BlankLines::Remove` drops
//! them all.

use fmtron::{BlankLines, Config, format_ron};
use indoc::indoc;

fn fmt(input: &str, blank_lines: BlankLines) -> String {
    let config = Config::default().with_blank_lines(blank_lines);
    let out = format_ron(input, &config).expect("valid RON");
    assert_eq!(
        format_ron(&out, &config).unwrap(),
        out,
        "not idempotent:\n{out}"
    );
    assert_eq!(
        ron::from_str::<ron::Value>(input).unwrap(),
        ron::from_str::<ron::Value>(&out).unwrap(),
        "value changed"
    );
    out
}

const INPUT: &str = indoc! {"
    // license


    #![enable(implicit_some)]

    // about the value
    (

        a: 1,
        b: 2,


        // section two

        c: [1,

        2],
        d: {1: 2,

        3: 4},

        // trailing notes

    )

    // end
"};

#[test]
fn blank_lines_are_kept_by_default() {
    assert_eq!(Config::default().blank_lines, BlankLines::Keep);
    assert_eq!(
        fmt(INPUT, BlankLines::Keep),
        indoc! {"
            // license

            #![enable(implicit_some)]

            // about the value
            (
                a: 1,
                b: 2,

                // section two

                c: [
                    1,

                    2,
                ],
                d: {
                    1: 2,

                    3: 4,
                },

                // trailing notes
            )

            // end"}
    );
}

#[test]
fn blank_lines_can_be_removed() {
    assert_eq!(
        fmt(INPUT, BlankLines::Remove),
        indoc! {"
            // license
            #![enable(implicit_some)]
            // about the value
            (
                a: 1,
                b: 2,
                // section two
                c: [1, 2],
                d: {1: 2, 3: 4},
                // trailing notes
            )
            // end"}
    );
}

/// Only whitespace-only lines count: a comma on its own line is not blank,
/// and a same-line trailing comment does not start a blank line.
#[test]
fn only_empty_lines_count_as_blank() {
    assert_eq!(fmt("[1\n,\n2]", BlankLines::Keep), "[1, 2]");
    assert_eq!(
        fmt(
            indoc! {"
                [
                    1, // one

                    2,
                ]"},
            BlankLines::Keep
        ),
        indoc! {"
            [
                1, // one

                2,
            ]"}
    );
    // Whitespace on the "blank" line and CRLF line endings still count.
    assert_eq!(
        fmt("[\r\n    1,\r\n    \t\r\n    2,\r\n]", BlankLines::Keep),
        "[\r\n    1,\r\n\r\n    2,\r\n]"
    );
}

/// A blank line never leaves trailing whitespace behind, at any depth.
#[test]
fn blank_lines_carry_no_indentation() {
    let out = fmt("(a: [(b: [1,\n\n2])])", BlankLines::Keep);
    assert!(out.contains("1,\n\n"), "{out}");
    assert!(
        out.lines().all(|l| l == l.trim_end()),
        "trailing whitespace:\n{out}"
    );
}
