//! Generated `fmt.ron` variety: settings combinations and syntax forms.
//!
//! The static corpus documents user-facing examples. This test fills in the
//! settings space behind it, then checks every generated file parses to the
//! expected settings and produces idempotent output.

#[path = "support/config_cases.rs"]
mod config_cases;

use std::collections::HashSet;

use fmtron::{Config, FileConfig, format_ron};

const SAMPLE_INPUTS: [&str; 3] = [
    "(alpha: 1, beta: 2, gamma: 3)",
    "// before\n(\n    alpha: 1,\n\n    // between\n    beta: 2,\n)\n",
    "(\n    /* block */ alpha: \"a long value that may overrun a narrow width\",\n    beta: [1, 2, 3],\n)",
];

#[test]
fn generated_settings_parse_to_the_expected_configuration() {
    let cases = config_cases::all();
    assert!(
        cases.len() >= 1_000,
        "expected at least 1000 generated config texts, got {}",
        cases.len()
    );
    let unique: HashSet<Config> = cases.iter().map(|case| case.config).collect();
    assert!(
        unique.len() >= 250,
        "expected at least 250 distinct settings, got {}",
        unique.len()
    );

    for case in &cases {
        let file: FileConfig = case
            .text
            .parse()
            .unwrap_or_else(|e| panic!("{}: generated text is invalid: {e}", case.name));
        file.validate()
            .unwrap_or_else(|e| panic!("{}: generated settings are invalid: {e}", case.name));

        let mut config = Config::default();
        file.apply(&mut config);
        assert_eq!(
            config, case.config,
            "{}: generated text parsed to the wrong settings",
            case.name
        );
    }
}

#[test]
fn generated_settings_format_every_sample_idempotently() {
    for case in config_cases::all() {
        let inputs = if case.config.max_nesting < 2 {
            &SAMPLE_INPUTS[..1]
        } else {
            &SAMPLE_INPUTS[..]
        };
        for input in inputs {
            let output = format_ron(input, &case.config)
                .unwrap_or_else(|e| panic!("{}: formatting sample failed: {e}", case.name));
            assert_eq!(
                format_ron(&output, &case.config).unwrap(),
                output,
                "{}: formatting is not idempotent",
                case.name
            );
            assert!(
                !output
                    .lines()
                    .any(|line| line.ends_with(' ') || line.ends_with('\t')),
                "{}: output has trailing whitespace",
                case.name
            );
        }
    }
}

#[test]
fn zero_width_and_tab_are_sanitized_for_library_callers() {
    let config = Config::default()
        .with_max_width(0)
        .with_tab_size(0)
        .with_max_tab(0);
    let output = format_ron("(alpha: 1, beta: [1, 2])", &config).unwrap();
    let indented = output
        .lines()
        .skip(1)
        .find(|line| !line.trim().is_empty())
        .unwrap_or_default();
    assert!(
        indented.starts_with(' ') || indented.starts_with('\t'),
        "zero tab size produced unindented output: {output:?}"
    );
    assert_eq!(format_ron(&output, &config).unwrap(), output);
}
