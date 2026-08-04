//! Gap-validation test cases.
//!
//! These are RON constructs that are valid according to the reference `ron`
//! crate (https://crates.io/crates/ron) but that fmtron's pest grammar
//! currently cannot parse. The cases live under `test_data/gaps/` (separate
//! from `test_data/unformatted/`, which the main conformance suite walks with
//! an `unwrap`, so adding unsupported cases there would abort that suite).
//!
//! The first test proves the inputs are genuinely valid RON (and that the
//! formatted counterparts are semantically equivalent round-trips). The second
//! test documents fmtron's current inability to handle them; it is expected to
//! start failing (and should then be updated) as fmtron gains support.

use std::path::Path;

/// Each of these inputs must be accepted by the official `ron` parser, and the
/// `formatted` counterpart must deserialize to the same `ron::Value`.
#[test]
fn gap_cases_are_valid_ron() {
    let unformatted_dir = Path::new("test_data/gaps/unformatted");
    let formatted_dir = Path::new("test_data/gaps/formatted");

    for entry in std::fs::read_dir(unformatted_dir).expect("gaps/unformatted dir missing") {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let unformatted =
            std::fs::read_to_string(entry.path()).expect("unable to read unformatted case");
        let formatted = std::fs::read_to_string(formatted_dir.join(&name))
            .expect("unable to read formatted counterpart");

        let unf: ron::Value = ron::from_str(&unformatted).unwrap_or_else(|e| panic!(
            "{}: unformatted input is not valid RON per the reference parser: {e}",
            name.to_string_lossy()
        ));
        let fmt: ron::Value = ron::from_str(&formatted).unwrap_or_else(|e| panic!(
            "{}: formatted output is not valid RON per the reference parser: {e}",
            name.to_string_lossy()
        ));

        assert_eq!(
            unf, fmt,
            "{}: formatted output is not semantically equivalent to the unformatted input",
            name.to_string_lossy()
        );
    }
}

/// Documents that fmtron currently cannot parse these valid RON constructs.
/// As support is added, flip the corresponding entry to `true`.
#[test]
fn gap_cases_currently_unsupported_by_fmtron() {
    // (filename, currently_supported)
    let cases: &[(&str, bool)] = &[
        ("signed_exponent.ron", true),
        ("special_floats.ron", true),
        ("number_suffixes.ron", true),
        ("byte_strings.ron", true),
        ("raw_identifier.ron", true),
        ("unicode_identifiers.ron", true),
    ];

    for (filename, currently_supported) in cases {
        let path = Path::new("test_data/gaps/unformatted").join(filename);
        let input = std::fs::read_to_string(&path).expect("gap case file missing");
        let result = fmtron::format_ron(&input);

        if *currently_supported {
            assert!(
                result.is_ok(),
                "{filename}: marked as supported, but format_ron failed: {:?}",
                result.err()
            );
        } else {
            assert!(
                result.is_err(),
                "{filename}: marked as unsupported, but format_ron succeeded \
                 — please flip `currently_supported` to `true`",
            );
        }
    }
}
