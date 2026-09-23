//! Mechanical validation of the black-box findings against the reference
//! `ron` crate.
//!
//! For every finding, this file pins what the reference implementation does
//! — what it *emits* (canonical forms) and what it *accepts* — so that the
//! finding is grounded in the behavior of `ron` itself, not in hand-waving.
//! Assertions about fmtron's *current* (wrong) behavior carry a `flip`
//! comment describing what to assert once the fix lands.

use fmtron::{Config, format_ron};
use ron::value::{Number, Value};

fn cfg(width: usize) -> Config {
    Config {
        tab_size: 4,
        max_width: width,
        ..Config::default()
    }
}

fn deep(n: usize) -> Value {
    let mut v = Value::Number(Number::from(1u8));
    for _ in 0..n {
        v = Value::Seq(vec![v]);
    }
    v
}

/// The reference emitter only produces canonical literal forms: plain digits,
/// chars as one escaped-or-bare scalar, strings with all control characters
/// and the BOM escaped, and never a file-leading BOM.
#[test]
fn reference_emits_canonical_forms() {
    assert_eq!(
        ron::to_string(&Value::Number(Number::from(1000u16))).unwrap(),
        "1000",
        "the emitter never produces digit separators"
    );
    assert_eq!(ron::to_string(&Value::Char('x')).unwrap(), "'x'");
    // Known upstream quirk (ron 0.12): the *emitter* writes '\n' as a raw
    // newline inside the quotes — output its own parser would reject. The
    // parse-side checks below are the validity authority.
    assert_eq!(ron::to_string(&Value::Char('\n')).unwrap(), "'\n'");
    assert_eq!(ron::to_string(&Value::Char('\\')).unwrap(), "'\\\\'");
    assert_eq!(
        ron::to_string(&Value::String("a\nb".into())).unwrap(),
        "\"a\\nb\"",
        "raw newlines are never emitted inside strings"
    );
    assert_eq!(
        ron::to_string(&Value::String("\u{FEFF}x".into())).unwrap(),
        "\"\\u{feff}x\"",
        "the BOM is escaped inside strings"
    );
    assert_eq!(
        ron::to_string(&Value::Seq(vec![Value::Seq(vec![Value::Number(
            1u8.into()
        )])]))
        .unwrap(),
        "[[1]]"
    );
}

/// Digit separators are valid RON syntax (the reference parser accepts a
/// fairly loose set, including `1__0` and a trailing `1_`), but the emitter
/// canonicalizes them away. fmtron now parses and byte-preserves them.
#[test]
fn digit_separators_are_valid_ron() {
    let cases = [
        "(a: 1_000)",
        "(a: 1__0)",
        "(a: 1_)",
        "(a: 0xFF_FF)",
        "(a: 0b1_0_1)",
        "(a: 1_000.5)",
        "(a: 1.5e1_0)",
        "(a: .5e1_0)",
        "(a: 7_f32)",
    ];
    for case in cases {
        assert!(
            ron::from_str::<Value>(case).is_ok(),
            "reference rejected {case} — this validation is wrong"
        );
        let out = fmtron::format_ron(case, &cfg(40)).expect("separators must parse");
        assert!(
            out.contains("_"),
            "separators must be byte-preserved, not canonicalized:\n{out}"
        );
        assert_eq!(
            ron::from_str::<Value>(&out).unwrap(),
            ron::from_str::<Value>(case).unwrap(),
            "formatting changed the value of {case}"
        );
    }
}

/// Separators are pure syntax sugar: the emitter emits plain digits, and
/// inserting separators into emitted output must preserve the value exactly.
#[test]
fn separators_preserve_the_reference_value() {
    let value = Value::Seq(vec![Value::Number(Number::from(1000u16))]);
    let canonical = ron::to_string(&value).unwrap();
    assert_eq!(canonical, "[1000]");
    let sugared = canonical.replace("1000", "1_000");
    assert_eq!(
        ron::from_str::<Value>(&sugared).unwrap(),
        ron::from_str::<Value>(&canonical).unwrap(),
        "separators changed the value"
    );
}

/// The reference implementation bounds recursion at 128 in *both* directions
/// (emit and parse) and always reports a clean error. fmtron allows more (512
/// by default) but must guard its own budget with a clean typed error instead
/// of aborting with a stack overflow (previously somewhere between depth 1000
/// debug and 5000 release).
#[test]
fn deep_input_is_bounded_by_a_clean_error() {
    // Reference: refuses depth 500 emission with a clean error.
    assert!(
        ron::to_string(&deep(500)).is_err(),
        "reference must refuse deep emission cleanly"
    );
    let d200 = format!("{}1{}", "[".repeat(200), "]".repeat(200));
    assert!(ron::from_str::<Value>(&d200).is_err());
    // fmtron: depth 200 is inside its default budget, so it formats fine.
    assert!(fmtron::format_ron(&d200, &cfg(40)).is_ok());
    // Depth beyond the default limit returns a clean error, never an abort.
    let deep = format!("{}1{}", "[".repeat(600), "]".repeat(600));
    let err = fmtron::format_ron(&deep, &cfg(40)).unwrap_err();
    assert!(
        matches!(
            err,
            fmtron::FormatError::TooDeep {
                depth: 600,
                max: 512
            }
        ),
        "expected a clean TooDeep error, got {err:?}"
    );
    // The budget is configurable: a higher cap admits the same input. Test
    // threads have small stacks, so format deep input on a big-stack thread.
    let wide = Config {
        max_nesting: 1024,
        ..cfg(40)
    };
    let admitted = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || fmtron::format_ron(&deep, &wide).is_ok())
        .unwrap()
        .join()
        .unwrap();
    assert!(admitted);
}

/// The reference parser rejects malformed char literals; the emitter proves
/// the canonical shape is exactly one scalar between quotes. fmtron now
/// rejects all three instead of blessing them into output the reference
/// still rejects.
#[test]
fn malformed_char_literals_are_invalid_ron() {
    let cases = ["('')", "('ab')", "('x\n')"];
    for case in cases {
        assert!(
            ron::from_str::<Value>(case).is_err(),
            "reference accepted {case} — this validation is wrong"
        );
        assert!(
            fmtron::format_ron(case, &cfg(40)).is_err(),
            "fmtron accepted malformed char literal {case}"
        );
    }
}

/// Raw newlines inside plain *strings* are valid RON (only chars forbid
/// them), so fmtron must keep accepting and byte-preserving them.
#[test]
fn multiline_plain_strings_are_valid_ron() {
    let input = "(s: \"line1\nline2\")";
    assert!(ron::from_str::<Value>(input).is_ok());
    let out = format_ron(input, &cfg(40)).expect("valid RON must format");
    assert!(
        out.contains("line1\nline2"),
        "newline inside string lost:\n{out}"
    );
}

/// A file-leading BOM is invalid RON: the reference rejects it, and the
/// emitter escapes the BOM inside strings rather than emitting it raw. fmtron
/// must reject it too, not silently turn it into an identifier.
#[test]
fn file_leading_bom_is_invalid_ron() {
    let bom_input = "\u{FEFF}(a: 1)";
    assert!(ron::from_str::<Value>(bom_input).is_err());
    // The emitter never starts a document with a raw BOM.
    let emitted = ron::to_string(&Value::String("\u{FEFF}x".into())).unwrap();
    assert!(!emitted.starts_with('\u{FEFF}'));
    // fmtron rejects a BOM-prefixed document outright.
    assert!(
        fmtron::format_ron(bom_input, &cfg(40)).is_err(),
        "fmtron accepted a BOM-prefixed document"
    );
}

/// Comment-relocation inputs are all valid RON (comments are whitespace to
/// the reference). fmtron must format them, keep every comment verbatim, and
/// stay idempotent — where the comment *lands* is a quality question, tracked
/// in the fix plan.
#[test]
fn comment_relocation_inputs_are_valid_ron() {
    let cases = [
        ("{ 1 /* c1 */: 2 }", "/* c1 */"),
        ("(a /* c1 */: 1)", "/* c1 */"),
        ("[1 /* c1 */, 2]", "/* c1 */"),
        ("// lead\n#![enable(unwrap_newtypes)]\nSome(1)", "// lead"),
    ];
    for (input, marker) in cases {
        assert!(
            ron::from_str::<Value>(input).is_ok(),
            "reference rejected {input:?}"
        );
        let out = format_ron(input, &cfg(40)).expect("valid RON must format");
        assert_eq!(
            out.matches(marker).count(),
            1,
            "comment {marker} lost or duplicated:\n{out}"
        );
        let again = format_ron(&out, &cfg(40)).expect("second pass must succeed");
        assert_eq!(again.trim_end(), out.trim_end(), "not idempotent:\n{out}");
    }
}
