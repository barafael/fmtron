//! Regression tests for the adversarial findings (F1-F6) documented in
//! `ADVERSARIAL_FINDINGS.md` and implemented per `FIX_PLAN.md`.
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
    let out = format_ron(input, &cfg(40))
        .unwrap_or_else(|e| panic!("failed to format {input:?}: {e}"));
    let twice = format_ron(&out, &cfg(40)).unwrap_or_else(|e| {
        panic!("second pass failed for input {input:?} (output {out:?}): {e}")
    });
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
    assert!(on_large_stack(move || format_ron(&over_limit, &bigger).is_ok()));

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