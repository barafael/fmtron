//! Whitespace and line breaks: which characters separate tokens, which line
//! ending the output uses, and how block comments spanning several lines are
//! laid out.

use fmtron::{Config, format_ron, line_ending};
use indoc::indoc;

fn fmt(input: &str) -> String {
    let out = format_ron(input, &Config::default()).expect("valid RON");
    assert_eq!(
        format_ron(&out, &Config::default()).unwrap(),
        out,
        "not idempotent:\n{out}"
    );
    out
}

/// fmtron separates tokens by exactly the characters the reference parser
/// does: Unicode's `Pattern_White_Space`.
#[test]
fn whitespace_is_what_the_reference_accepts() {
    for ws in [
        '\u{0B}', '\u{0C}', '\u{85}', '\u{200E}', '\u{200F}', '\u{2028}', '\u{2029}',
    ] {
        let input = format!("(a:{ws}1,{ws}b: [2,{ws}3])");
        let reference = ron::from_str::<ron::Value>(&input);
        assert!(reference.is_ok(), "{ws:?}: {reference:?}");
        assert_eq!(fmt(&input), "(a: 1, b: [2, 3])", "{ws:?}");
    }
    // Other Unicode spaces are not whitespace to either.
    for c in ['\u{A0}', '\u{3000}', '\u{FEFF}'] {
        let input = format!("(a:{c}1)");
        assert!(ron::from_str::<ron::Value>(&input).is_err(), "{c:?}");
        assert!(format_ron(&input, &Config::default()).is_err(), "{c:?}");
    }
}

/// The first line break outside string and char literals decides the line
/// ending; one inside a literal belongs to its value.
#[test]
fn the_line_ending_is_decided_outside_literals() {
    assert_eq!(line_ending("(a: \"x\ny\",\r\n b: 1)"), "\r\n");
    assert_eq!(line_ending("(a: r#\"x\ny\"#,\r\n b: 1)"), "\r\n");
    assert_eq!(line_ending("['\n',\r\n 1]"), "\r\n");
    assert_eq!(line_ending("(a: r\"x\r\ny\")\n"), "\n");
    // Comments are layout: their line breaks count, their quotes do not.
    assert_eq!(line_ending("/* a\r\n */ 1"), "\r\n");
    assert_eq!(line_ending("// say \"hi\r\n(a: \"x\ny\")"), "\r\n");
    assert_eq!(line_ending("/* \" */ 1\r\n"), "\r\n");
    assert_eq!(fmt("(a: \"x\ny\",\r\n b: 1)\r\n"), "(a: \"x\ny\", b: 1)");
}

/// The later lines of a block comment move with the line it starts on,
/// keeping their indentation relative to it, and use the file's line ending.
#[test]
fn block_comments_are_reindented_with_their_line() {
    // Deeper: the comment's lines keep their shape.
    assert_eq!(
        fmt(indoc! {"
            (
            b: [
            /* one
               two
            */
            1],
            )"}),
        indoc! {"
            (
                b: [
                    /* one
                       two
                    */
                    1,
                ],
            )"}
    );
    // Shallower, and trailing a value.
    assert_eq!(
        fmt(indoc! {"
            [
                        1, /* one
                          two */
            ]"}),
        indoc! {"
            [
                1, /* one
                  two */
            ]"}
    );
    // Lines indented less than the comment's line go to its indentation;
    // blank lines stay blank.
    assert_eq!(
        fmt(indoc! {"
            [
                    /* one

              two */
                1,
            ]"}),
        indoc! {"
            [
                /* one

                two */
                1,
            ]"}
    );
    // Tabs count like spaces when removing the old indentation; tabs left
    // over in front of a line become `tab_size` spaces, like any indentation.
    assert_eq!(
        fmt("[\n\t\t/* one\n\t\t * two */\n\t\t1,\n]"),
        "[\n    /* one\n     * two */\n    1,\n]"
    );
    assert_eq!(
        fmt("[\n\t/* one\n\t\ttwo */\n\t1,\n]"),
        "[\n    /* one\n        two */\n    1,\n]"
    );
    let two = Config {
        tab_size: 2,
        ..Config::default()
    };
    assert_eq!(
        format_ron("[\n\t/* one\n\t\ttwo */\n\t1,\n]", &two).unwrap(),
        "[\n  /* one\n    two */\n  1,\n]"
    );
}

#[test]
fn line_breaks_in_block_comments_follow_the_file() {
    // A CRLF comment in an LF file, and an LF comment in a CRLF file.
    assert_eq!(
        fmt("[\n    /* one\r\n    two */\n    1,\n]"),
        "[\n    /* one\n    two */\n    1,\n]"
    );
    assert_eq!(
        fmt("[\r\n    /* one\n    two */\r\n    1,\r\n]"),
        "[\r\n    /* one\r\n    two */\r\n    1,\r\n]"
    );
    // At the top level too.
    assert_eq!(
        fmt("// top\n/* a\r\n b */\n1\n/* c\r\n d */"),
        "// top\n/* a\n b */\n1\n/* c\n d */\n"
    );
    // A lone `\r` is not a line break, as in line comments.
    assert_eq!(
        fmt("[\n    /* one\rtwo */\n    1,\n]"),
        "[\n    /* one\rtwo */\n    1,\n]"
    );
}
