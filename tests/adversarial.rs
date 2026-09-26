//! Regression tests for the adversarial findings documented in
//! `ADVERSARIAL_FINDINGS.md` (F1-F6, implemented per `FIX_PLAN.md`) and
//! `ADVERSARIAL_FINDINGS_2.md` (N1-N11, R1-R6).
//!
//! Every acceptance/rejection assertion is grounded in the reference `ron`
//! crate: where fmtron's new behavior is pinned, the oracle is checked first
//! so the validation itself cannot silently drift.

use fmtron::{Config, FormatError, format_ron};

fn cfg(width: usize) -> Config {
    Config {
        max_width: width,
        ..Config::default()
    }
}

/// Deeply nested inputs exercise recursive descent in pest and in the renderer.
/// Test threads run on small (2 MiB) stacks by default, so wrap probes that
/// actually *format* deep input on a thread with a real stack.
fn on_large_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(f)
        .expect("spawn big-stack thread")
        .join()
        .expect("big-stack thread panicked")
}

fn assert_valid(input: &str) {
    assert!(
        format_ron(input, &cfg(40)).is_ok(),
        "fmtron rejected valid RON {input:?}"
    );
}

fn assert_invalid(input: &str) {
    assert!(
        format_ron(input, &cfg(40)).is_err(),
        "fmtron over-accepted invalid RON {input:?}"
    );
}

/// The formatted output must be a fixed point and semantically identical to
/// the input per the reference parser.
fn assert_roundtrips(input: &str) {
    let out =
        format_ron(input, &cfg(40)).unwrap_or_else(|e| panic!("failed to format {input:?}: {e}"));
    let twice = format_ron(&out, &cfg(40))
        .unwrap_or_else(|e| panic!("second pass failed for input {input:?} (output {out:?}): {e}"));
    assert_eq!(out, twice, "not idempotent for {input:?}");
    let before: ron::Value =
        ron::from_str(input).unwrap_or_else(|e| panic!("input {input:?} not valid ron: {e}"));
    let after: ron::Value =
        ron::from_str(&out).unwrap_or_else(|e| panic!("output {out:?} not valid ron: {e}"));
    assert_eq!(before, after, "semantic drift for {input:?}");
}

// F1: deep nesting must be rejected with a clean typed error, never a stack
// overflow. The default budget is `MAX_NESTING` (512).
#[test]
fn deep_nesting_is_rejected_cleanly() {
    let at_limit = format!("{}1{}", "[".repeat(512), "]".repeat(512));
    assert!(
        on_large_stack(move || format_ron(&at_limit, &cfg(40)).is_ok()),
        "depth 512 (the default limit) must format cleanly"
    );
    let over_limit = format!("{}1{}", "[".repeat(513), "]".repeat(513));
    let err = format_ron(&over_limit, &cfg(40)).unwrap_err();
    assert!(
        matches!(
            err,
            FormatError::TooDeep {
                depth: 513,
                max: 512
            }
        ),
        "expected TooDeep, got {err:?}"
    );
    // Raising the budget admits the same input.
    let bigger = Config {
        max_nesting: 2048,
        ..cfg(40)
    };
    assert!(on_large_stack(
        move || format_ron(&over_limit, &bigger).is_ok()
    ));

    // Non-container nesting isn't counted: brackets inside literals and
    // comments must not inflate the measured depth. (Real nesting: 2.)
    let strings_and_comments = "(s: \"[[[((({[\", /* [([({)] */ t: [1, 2])";
    assert_valid(strings_and_comments);
}

// F2: `tab_size` is capped by `max_tab` (1024 by default); a pathological
// request must clamp rather than emit absurd indentation.
#[test]
fn tab_size_is_bounded_by_max_tab() {
    let input = "[1, 2, 3]";
    let clamped = Config {
        tab_size: usize::MAX,
        ..cfg(10)
    };
    assert!(
        format_ron(input, &clamped).is_ok(),
        "a huge tab_size must be clamped, not panic"
    );
}

// F3: digit separators are accepted exactly where the reference parser accepts
// them — including consecutive and trailing underscores — and byte-preserved.
#[test]
fn digit_separators_parse_and_roundtrip() {
    let oracle_accepts = [
        "(a: 1_000)",
        "(a: 1__0)",
        "(a: 1_)",
        "(a: 1__)",
        "(a: 1__0__0)",
        "(a: 0xFF_FF)",
        "(a: 0xFF__FF)",
        "(a: 0x1__0)",
        "(a: 0b1_0_1)",
        "(a: 0o7_7_)",
        "(a: 1_000.5)",
        "(a: 1_.5)",
        "(a: 1.5e1_0)",
        "(a: 1e_3)",
        "(a: 1e-_3)",
        "(a: .5e1_0)",
        "(a: 7_f32)",
        "(a: 1_999u64)",
        "(a: 3_3f32)",
        "(a: 0_1)",
    ];
    for case in oracle_accepts {
        assert!(
            ron::from_str::<ron::Value>(case).is_ok(),
            "oracle rejected {case} — validation is wrong"
        );
        assert_valid(case);
        assert_roundtrips(case);
        let out = format_ron(case, &cfg(40)).unwrap();
        assert!(
            out.contains('_'),
            "separators must be preserved, got {out:?}"
        );
    }
}

// F3/F4: forms the reference parser rejects must stay rejected. Per-radix
// alphabets (F4) reject invalid base digits; separator placement rules (F3)
// reject a leading underscore after a base prefix or decimal point.
#[test]
fn invalid_number_forms_stay_rejected() {
    let oracle_rejects = [
        "(a: 0b2)",
        "(a: 0b102)",
        "(a: 0o8)",
        "(a: 0o18)",
        "(a: 0x_1)",
        "(a: 0b__1)",
        "(a: 1._5)",
        "(a: 1e_)",
    ];
    for case in oracle_rejects {
        assert!(
            ron::from_str::<ron::Value>(case).is_err(),
            "oracle accepted {case} — validation is wrong"
        );
        assert_invalid(case);
    }
    // Valid base literals without separators still work.
    for case in ["0xFF", "0b1010", "0b1_01", "0o777", "0"] {
        assert_valid(case);
    }
}

// F5: `inf`/`NaN` are special floats only at a boundary; they must not shadow
// longer identifiers.
#[test]
fn inf_and_nan_do_not_shadow_identifiers() {
    let identifiers = [
        "(inf32)",
        "(infinity)",
        "(inf8)",
        "(inf3)",
        "(NaNfoo)",
        "(NaN3)",
        "(inff32)",
    ];
    for id in identifiers {
        assert!(
            ron::from_str::<ron::Value>(id).is_ok(),
            "oracle rejected {id} — i.e. it is not an identifier there"
        );
        assert_valid(id);
    }
    let special = ["(inf)", "(NaN)", "(-inf)", "(+NaN)", "(inf)", "(NaNf32)"];
    for s in special {
        assert_valid(s);
        assert_roundtrips(s);
    }
}

// F6: unknown escapes are rejected in both strings and chars; valid escapes
// and raw newlines (strings only) still parse and round-trip.
#[test]
fn escapes_are_strict() {
    let rejected = [
        r#"("\z")"#,
        r#"("\12")"#,
        r#"("\x4")"#,
        r#"("\xzz")"#,
        r#"("\u{}")"#,
        r#"("\u{z}")"#,
        r#"("\a")"#,
        r#"("\b")"#,
        r#"("\A")"#,
        "(c: '\\z')",
        "('a\\nb')",
    ];
    for case in rejected {
        assert!(
            ron::from_str::<ron::Value>(case).is_err(),
            "oracle accepted {case} — validation is wrong"
        );
        assert_invalid(case);
    }
    let accepted = [
        r#"("a\nb")"#,
        r#"("\t")"#,
        r#"("\0")"#,
        r#"("\'")"#,
        r#"("\x41")"#,
        r#"("\u{41}")"#,
        r#"("\u{1F600}")"#,
        r#"("\u{10FFFF}")"#,
        "('\\n')",
        "('\\\\')",
        "('\\'')",
    ];
    for case in accepted {
        assert!(
            ron::from_str::<ron::Value>(case).is_ok(),
            "oracle rejected {case} — validation is wrong"
        );
        assert_valid(case);
        assert_roundtrips(case);
    }
    // A raw newline is valid inside a plain string and must be preserved.
    let multiline = "(s: \"line1\nline2\")";
    assert!(ron::from_str::<ron::Value>(multiline).is_ok());
    let out = format_ron(multiline, &cfg(40)).unwrap();
    assert!(out.contains("line1\nline2"), "newline lost:\n{out}");
}

// F6: the byte-order mark is not an identifier character; a BOM-prefixed
// document is rejected outright.
#[test]
fn file_leading_bom_is_rejected() {
    assert!(ron::from_str::<ron::Value>("\u{FEFF}(a: 1)").is_err());
    assert_invalid("\u{FEFF}(a: 1)");
}

/// Asserts the oracle agrees before pinning fmtron's behavior.
fn assert_valid_per_oracle(input: &str) {
    assert!(
        ron::from_str::<ron::Value>(input).is_ok(),
        "oracle rejected {input:?} — validation is wrong"
    );
    assert_valid(input);
    assert_roundtrips(input);
}

fn assert_invalid_per_oracle(input: &str) {
    assert!(
        ron::from_str::<ron::Value>(input).is_err(),
        "oracle accepted {input:?} — validation is wrong"
    );
    assert_invalid(input);
}

// N1-N3: comments inside an attribute used to panic, replace the attribute's
// string, or be printed as an extension name. Such attributes are now kept
// verbatim.
#[test]
fn comments_inside_attributes_are_preserved() {
    let cases = [
        "# /*x*/ ![enable(implicit_some)]\n5",
        "#![/*x*/ enable(implicit_some)]\n5",
        "#![enable(/* c */ implicit_some)]\n5",
        "#![enable(implicit_some /* c */)]\n5",
        "#![enable(implicit_some // c\n)]\n5",
        "#![type = /*c*/ \"x\"]\n5",
        "#![schema = \"s\" // c\n]\n5",
    ];
    for case in cases {
        assert_valid_per_oracle(case);
        let out = format_ron(case, &cfg(40)).unwrap();
        let attr = case.split_once("\n5").unwrap().0;
        assert!(
            out.starts_with(attr),
            "attribute not kept for {case:?}:\n{out}"
        );
    }
    // Comment-free attributes are still normalized.
    assert_eq!(
        format_ron("#![ enable ( implicit_some , ) ]\n5", &cfg(40)).unwrap(),
        "#![enable(implicit_some)]\n5"
    );
}

// N9: comments before and between attributes keep their position.
#[test]
fn header_comments_keep_their_order() {
    let input = "// license\n#![enable(implicit_some)]\n// mid\n#![type = \"T\"]\n// lead\n5";
    assert_valid_per_oracle(input);
    assert_eq!(format_ron(input, &cfg(40)).unwrap(), input);
}

// N4: nested block comments recurse in pest just like containers, so they
// count toward the nesting budget instead of overflowing the stack.
#[test]
fn deep_block_comment_nesting_is_rejected_cleanly() {
    let deep = format!("{}{}5", "/*".repeat(60_000), "*/".repeat(60_000));
    let err = format_ron(&deep, &cfg(40)).unwrap_err();
    assert!(
        matches!(err, FormatError::TooDeep { depth: 60_000, .. }),
        "expected TooDeep, got {err:?}"
    );
    assert_valid_per_oracle("/* a /* b /* c */ */ */ [1, /* d /* e */ */ 2]");
}

// N5/N6: `true`/`false` and signed, suffixed `inf`/`NaN` must not shadow
// identifiers, and the suffixed keywords are floats.
#[test]
fn keywords_do_not_shadow_identifiers() {
    for case in [
        "trueish",
        "false_value",
        "(a: truex)",
        "{falsey: 1}",
        "Foo(true_)",
        "[true, false]",
        "-NaNf32",
        "+inff64",
        "NaNf32",
        "inff32x",
    ] {
        assert_valid_per_oracle(case);
    }
}

// N7: byte literals are ASCII or a byte escape (no `\u{...}`).
#[test]
fn byte_literals_are_supported() {
    assert_valid_per_oracle(r"[b'a', b'\\', b'\'', b'\xff', b'\n', b'(', b'\0']");
    for case in [r"b'\u{41}'", "b'\u{e9}'", "b''", "b'ab'"] {
        assert_invalid_per_oracle(case);
    }
}

// N10: only `\n` ends a line comment; a lone `\r` does not.
#[test]
fn lone_carriage_return_does_not_end_a_line_comment() {
    assert_invalid_per_oracle("[1 // c\r, 2]");
    assert_valid_per_oracle("[1 // c\r\n, 2]");
}

// N11: a char's `\x` escape must be ASCII; strings may chain `\xHH` escapes
// into multi-byte UTF-8.
#[test]
fn char_hex_escapes_are_ascii_only() {
    assert_invalid_per_oracle(r"'\x80'");
    assert_valid_per_oracle(r"'\x7f'");
    assert_valid_per_oracle(r#""\xc3\xa9""#);
}

// R1: identifiers follow the RON grammar: `XID_Start | _` then `XID_Continue`;
// raw identifiers may also start with and contain `.`, `+`, `-`.
#[test]
fn identifiers_follow_the_xid_rules() {
    for case in [
        "ǅa",
        "a·",
        "r#0",
        "r#a.b",
        "r#a+b-c",
        "(r#a.b: 1)",
        "Foo(r#1: 2)",
        "r#1(1)",
    ] {
        assert_valid_per_oracle(case);
    }
    for case in ["·a", "\u{661}a", "Foo\u{200b}", "r#"] {
        assert_invalid_per_oracle(case);
    }
}

// R2: a char holds any single unescaped char, including `'` itself and a raw
// newline, as the reference parser accepts.
#[test]
fn unescaped_quote_and_newline_chars_are_accepted() {
    for case in ["'''", "b'''", "'\n'", "'\r'", "[''', 'x']"] {
        assert_valid_per_oracle(case);
    }
    for case in ["''", "'\r\n'", "['''', 1]"] {
        assert_invalid_per_oracle(case);
    }
}

// R3: with `'''` valid, a lenient char scan in the depth guard would pair the
// first two quotes, then skip from the third to the next `'` and hide the
// nesting in between from the guard while pest recursed into it.
#[test]
fn depth_guard_is_not_fooled_by_quote_chars() {
    let deep = format!("{}1{}", "[".repeat(600), "]".repeat(600));
    for input in [
        format!("[''', {deep}, 'x']"),
        format!("[b''', {deep}, b'x']"),
        format!(r"['\'', {deep}, 'x']"),
    ] {
        let err = format_ron(&input, &cfg(40)).unwrap_err();
        assert!(
            matches!(err, FormatError::TooDeep { depth: 601, .. }),
            "expected TooDeep for {input:.20}…, got {err:?}"
        );
    }
}

// R4: blank, comment-only and attribute-only input reports `Empty`, not a
// parse error.
#[test]
fn input_without_a_value_is_empty() {
    for case in [
        "",
        "  \n\t",
        "// c\n",
        "/* c */",
        "#![enable(implicit_some)]\n// c",
    ] {
        assert_eq!(
            format_ron(case, &cfg(40)),
            Err(FormatError::Empty),
            "for {case:?}"
        );
    }
    assert!(matches!(
        format_ron("/* unterminated", &cfg(40)),
        Err(FormatError::Parse(_))
    ));
}

// R5: a parse error on a huge single line shows a bounded excerpt, not the
// whole line.
#[test]
fn parse_error_on_a_huge_line_is_bounded() {
    let input = format!("[{}@]", "1,".repeat(1_000_000));
    let msg = format_ron(&input, &cfg(40)).unwrap_err().to_string();
    assert!(msg.len() < 500, "error is {} bytes", msg.len());
    assert!(msg.contains("1:2000002"), "location missing:\n{msg}");
    assert!(msg.contains("1,1,@]"), "excerpt missing:\n{msg}");
    // Short lines keep pest's full rendering.
    let short = format_ron("[@]", &cfg(40)).unwrap_err().to_string();
    assert!(short.contains("1 | [@]"), "{short}");
}
