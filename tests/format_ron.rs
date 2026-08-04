use fmtron::format_ron;
use fs_walk::WalkOptions;
use std::path::Path;

#[test]
fn empty_input() {
    let result = format_ron("");
    assert!(result.is_err());
}

#[test]
fn invalid_input() {
    let result = format_ron("This is not RON!");
    assert!(result.is_err());
}

#[test]
fn formats_test_file() {
    let content = include_str!("../test_data/test.ron");
    let ron = format_ron(content).expect("unable to format RON");
    // formatting is idempotent: reformatting the output must not change it
    let ron2 = format_ron(&ron).expect("unable to reformat RON");
    assert_eq!(ron, ron2, "formatter output is not idempotent");
}

fn normalize(s: &str) -> String {
    s.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn unofficial_improvised_ron_conformance_suite() {
    let unformatted_dir = "test_data/unformatted";
    let formatted_dir = "test_data/formatted";
    let walker = WalkOptions::new()
        .files()
        .extension("ron")
        .walk(unformatted_dir);
    let mut failures: Vec<String> = Vec::new();
    let mut count = 0;
    for entry in walker.flatten() {
        count += 1;
        let filename = entry.as_path().strip_prefix(unformatted_dir).unwrap();
        let formatted_path = Path::new(formatted_dir).join(filename);
        let input = std::fs::read_to_string(entry.as_path()).unwrap();
        let expected = std::fs::read_to_string(&formatted_path).unwrap();
        let case = filename.display().to_string();

        let result = match fmtron::format_ron(&input) {
            Ok(ron) => {
                if normalize(&ron) == normalize(&expected) {
                    None
                } else {
                    Some(format!(
                        "{case}: output mismatch\n--- expected ---\n{}\n--- actual ---\n{}",
                        normalize(&expected),
                        normalize(&ron)
                    ))
                }
            }
            Err(e) => Some(format!("{case}: format_ron failed: {e}")),
        };
        if let Some(msg) = result {
            failures.push(msg);
        }
    }
    assert!(count > 0, "no conformance cases found in {unformatted_dir}");
    if !failures.is_empty() {
        panic!(
            "{} of {} conformance cases failed:\n\n{}",
            failures.len(),
            count,
            failures.join("\n\n")
        );
    }
}

#[test]
fn formatted_output_is_semantically_equivalent() {
    // The official `ron` crate is the spec oracle: formatting must not change
    // the meaning of a file. Compares across the unformatted corpus AND the
    // gaps corpus.
    //
    // Some corpus files use placeholder extension names (e.g. `foo`, `bar`)
    // that the official parser rejects as unknown extensions. Such files
    // cannot be checked by this oracle and are skipped (reported as skipped,
    // not failed).
    let dirs = ["test_data/unformatted", "test_data/gaps/unformatted"];
    let mut checked = 0;
    let mut skipped = 0;
    for dir in dirs {
        let walker = WalkOptions::new().files().extension("ron").walk(dir);
        for entry in walker.flatten() {
            let input = std::fs::read_to_string(entry.as_path()).unwrap();
            let before = match ron::from_str::<ron::Value>(&input) {
                Ok(v) => v,
                Err(_) => {
                    skipped += 1;
                    continue;
                }
            };
            let formatted = fmtron::format_ron(&input)
                .unwrap_or_else(|e| panic!("format failed for {:?}: {e}", entry.file_name()));
            let after: ron::Value = ron::from_str(&formatted)
                .unwrap_or_else(|e| panic!("output not valid ron {:?}: {e}", entry.file_name()));
            assert_eq!(before, after, "semantic drift in {:?}", entry.file_name());
            checked += 1;
        }
    }
    assert!(checked > 0, "oracle checked no files (all {skipped} skipped)");
}

#[test]
fn formatting_is_idempotent() {
    // A formatter's output must be a fixed point: format(format(x)) == format(x).
    let dirs = ["test_data/unformatted", "test_data/gaps/unformatted"];
    for dir in dirs {
        let walker = WalkOptions::new().files().extension("ron").walk(dir);
        for entry in walker.flatten() {
            let input = std::fs::read_to_string(entry.as_path()).unwrap();
            let once = fmtron::format_ron(&input).expect("first pass failed");
            let twice = fmtron::format_ron(&once).expect("second pass failed");
            assert_eq!(
                normalize(&once),
                normalize(&twice),
                "not idempotent for {:?}",
                entry.file_name()
            );
        }
    }
}

