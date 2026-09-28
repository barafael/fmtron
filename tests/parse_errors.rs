//! Parse errors say what was expected in RON terms (`,`, `]`, a value, end
//! of input) and what was found, instead of pest's rule names
//! (`expected COMMENT`), and name common mistakes seen in real files.

use fmtron::{Config, FormatError, format_ron};

/// The message of the parse error for `input`, and its 1-based line/column.
fn error(input: &str) -> (String, (usize, usize)) {
    match format_ron(input, &Config::default()) {
        Err(FormatError::Parse(e)) => {
            let pos = match e.line_col {
                pest::error::LineColLocation::Pos(p) | pest::error::LineColLocation::Span(p, _) => {
                    p
                }
            };
            (e.variant.message().into_owned(), pos)
        }
        other => panic!("expected a parse error for {input:?}, got {other:?}"),
    }
}

#[test]
fn messages_name_expected_tokens_and_what_was_found() {
    let cases = [
        ("[1, 2", "expected `,` or `]`, found end of input", (1, 6)),
        ("(a: 1 b: 2)", "expected `,` or `)`, found `b`", (1, 7)),
        ("{1 2}", "expected `:`, found `2`", (1, 4)),
        ("Foo(a: )", "expected a value, found `)`", (1, 8)),
        ("[1] [2]", "expected end of input, found `[`", (1, 5)),
        (
            "[@]",
            "unexpected character `@`, expected `]` or a value",
            (1, 2),
        ),
        (
            "[Foo @]",
            "unexpected character `@`, expected `,` or `]`",
            (1, 6),
        ),
        ("[\"a\" \"b\"]", "expected `,` or `]`, found `\"`", (1, 6)),
        // A complete number is not continued by "a value".
        ("(a: 1.", "expected `,` or `)`, found end of input", (1, 7)),
        // A number that breaks off after its sign or `.`.
        ("(a: -)", "expected a value, found `)`", (1, 6)),
        ("-", "expected a value, found end of input", (1, 2)),
        ("[.x]", "expected a value, found `x`", (1, 3)),
        // `(` is named when nothing else fits.
        (
            "#![enable implicit_some)]\n()",
            "expected `(`, found `i`",
            (1, 11),
        ),
        // Characters that would not show are escaped.
        (
            "(a: 1,\u{feff} b: 2)",
            "unexpected character `\\u{feff}`, expected `)` or a value",
            (1, 7),
        ),
        ("(a:\u{a0}1)", "expected a value, found `\\u{a0}`", (1, 4)),
        ("[1\t2]", "expected `,` or `]`, found `2`", (1, 4)),
    ];
    for (input, message, pos) in cases {
        assert_eq!(error(input), (message.to_string(), pos), "for {input:?}");
    }
}

#[test]
fn broken_literals_are_explained_where_they_start() {
    let cases = [
        ("\"abc", "unterminated string literal", (1, 1)),
        ("[1, \"ok\", \"abc", "unterminated string literal", (1, 11)),
        ("r#\"abc", "unterminated raw string literal", (1, 1)),
        ("('a", "unterminated char literal", (1, 2)),
        (
            r#"["a\zb"]"#,
            r"invalid escape `\z` in string literal",
            (1, 4),
        ),
        (
            r#"(a: "x\u{zz}")"#,
            r"invalid escape `\u` in string literal",
            (1, 7),
        ),
        (r"'\q'", r"invalid escape `\q` in char literal", (1, 2)),
        (
            "('ab')",
            "a char literal holds exactly one character; use \"…\" for a string",
            (1, 2),
        ),
        (
            "('')",
            "a char literal holds exactly one character; use \"…\" for a string",
            (1, 2),
        ),
    ];
    for (input, message, pos) in cases {
        assert_eq!(error(input), (message.to_string(), pos), "for {input:?}");
    }
    // A quote inside a comment is not a literal, and non-ASCII text in a
    // block comment is stepped over.
    assert_eq!(
        error("// \"quote\n[1 2]").0,
        "expected `,` or `]`, found `2`"
    );
    assert_eq!(
        error("/* café */ [1 2]"),
        ("expected `,` or `]`, found `2`".to_string(), (1, 15))
    );
}

#[test]
fn common_mistakes_get_a_hint() {
    let cases = [
        (
            "# copied from elsewhere\n(a: 1)",
            "`#` does not start a comment in RON; use `// …` or `/* … */`",
            (1, 1),
        ),
        (
            "(a: 1 # note\n)",
            "`#` does not start a comment in RON; use `// …` or `/* … */`",
            (1, 7),
        ),
        (
            "(selected_tab: Color::Reset)",
            "`::` paths are not RON; write the variant alone (`Reset`, not `Color::Reset`)",
            (1, 21),
        ),
        ("(x: 1.5.5)", "invalid number literal `1.5.5`", (1, 5)),
        ("[0x_FF]", "invalid number literal `0x_FF`", (1, 2)),
        ("(a: -0b12)", "invalid number literal `-0b12`", (1, 5)),
        ("(a: 1e)", "invalid number literal `1e`", (1, 5)),
        ("[1, /* note\n2]", "unterminated block comment", (1, 5)),
    ];
    for (input, message, pos) in cases {
        assert_eq!(error(input), (message.to_string(), pos), "for {input:?}");
    }
    // Attributes are not mistaken for `#` comments; valid numbers are fine.
    assert!(
        format_ron(
            "#![enable(implicit_some)]\n[1_000, 0xFF, 1e5, -2.5]",
            &Config::default()
        )
        .is_ok()
    );
}

/// The rendering keeps pest's excerpt and caret, `with_path` names the file,
/// and long lines are still shown as an excerpt.
#[test]
fn rendering_keeps_excerpt_path_and_truncation() {
    let Err(FormatError::Parse(e)) = format_ron("(a: 1\n b: 2)", &Config::default()) else {
        panic!("expected a parse error");
    };
    let shown = e.with_path("levels/one.ron").to_string();
    assert!(shown.contains("--> levels/one.ron:2:2"), "{shown}");
    assert!(shown.contains("2 |  b: 2)"), "{shown}");
    assert!(shown.contains("expected `,` or `)`, found `b`"), "{shown}");

    let long = format!("[{}@]", "1,".repeat(1_000));
    let msg = format_ron(&long, &Config::default())
        .unwrap_err()
        .to_string();
    assert!(msg.len() < 500, "{} bytes", msg.len());
    assert!(msg.contains("unexpected character `@`"), "{msg}");
}

/// Explaining an error uses no process-global state, so every thread
/// formatting at the same time gets the full message.
#[test]
fn errors_are_explained_alike_on_many_threads() {
    let expected = (
        "expected `,` or `]`, found end of input".to_string(),
        (1, 6),
    );
    std::thread::scope(|s| {
        for _ in 0..8 {
            s.spawn(|| {
                for _ in 0..200 {
                    assert_eq!(error("[1, 2"), expected);
                }
            });
        }
    });
}
