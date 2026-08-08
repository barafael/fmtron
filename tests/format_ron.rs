use fmtron::{Config, format_ron};
use fs_walk::WalkOptions;
use std::path::Path;

fn format_default(s: &str) -> Result<String, String> {
    format_ron(s, &Config::default())
}

#[test]
fn empty_input() {
    let result = format_default("");
    assert!(result.is_err());
}

#[test]
fn invalid_input() {
    let result = format_default("This is not RON!");
    assert!(result.is_err());
}

#[test]
fn formats_test_file() {
    let content = include_str!("../test_data/test.ron");
    let ron = format_default(content).expect("unable to format RON");
    // formatting is idempotent: reformatting the output must not change it
    let ron2 = format_default(&ron).expect("unable to reformat RON");
    assert_eq!(ron, ron2, "formatter output is not idempotent");
}

/// Trim trailing whitespace per line and drop trailing empty lines so the
/// comparison is insensitive to newline-at-EOF differences but **preserves
/// indentation** — a formatter regression that drops or changes indent must
/// fail the conformance suite, not slip through silently.
fn normalize(s: &str) -> String {
    s.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
}

#[test]
fn unofficial_improvised_ron_conformance_suite() {
    let pairs = [
        ("test_data/unformatted", "test_data/formatted"),
        ("test_data/ron_corpus", "test_data/ron_corpus_formatted"),
    ];
    let mut failures: Vec<String> = Vec::new();
    let mut count = 0;
    for (unformatted_dir, formatted_dir) in pairs {
        let walker = WalkOptions::new()
            .files()
            .extension("ron")
            .walk(unformatted_dir);
        for entry in walker.flatten() {
            count += 1;
            let filename = entry.as_path().strip_prefix(unformatted_dir).unwrap();
            let formatted_path = Path::new(formatted_dir).join(filename);
            let input = std::fs::read_to_string(entry.as_path()).unwrap();
            let expected = std::fs::read_to_string(&formatted_path).unwrap();
            let case = filename.display().to_string();

            let result = match format_default(&input) {
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
    }
    assert!(count > 0, "no conformance cases found");
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
    let dirs = [
        "test_data/unformatted",
        "test_data/gaps/unformatted",
        "test_data/ron_corpus",
        "test_data/synthetic",
    ];
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
            let formatted = format_default(&input)
                .unwrap_or_else(|e| panic!("format failed for {:?}: {e}", entry.file_name()));
            let after: ron::Value = ron::from_str(&formatted)
                .unwrap_or_else(|e| panic!("output not valid ron {:?}: {e}", entry.file_name()));
            assert_eq!(before, after, "semantic drift in {:?}", entry.file_name());
            checked += 1;
        }
    }
    assert!(
        checked > 0,
        "oracle checked no files (all {skipped} skipped)"
    );
}

#[test]
fn formatting_is_idempotent() {
    // A formatter's output must be a fixed point: format(format(x)) == format(x).
    let dirs = [
        "test_data/unformatted",
        "test_data/gaps/unformatted",
        "test_data/ron_corpus",
        "test_data/synthetic",
    ];
    for dir in dirs {
        let walker = WalkOptions::new().files().extension("ron").walk(dir);
        for entry in walker.flatten() {
            let input = std::fs::read_to_string(entry.as_path()).unwrap();
            let once = format_default(&input).expect("first pass failed");
            let twice = format_default(&once).expect("second pass failed");
            assert_eq!(
                normalize(&once),
                normalize(&twice),
                "not idempotent for {:?}",
                entry.file_name()
            );
        }
    }
}

#[test]
fn synthetic_corpus_parses() {
    // Every synthetic corpus file must be accepted by fmtron. Collects all
    // parse failures (rather than aborting on the first) so a broad sweep of
    // exotic features reports every gap at once.
    let walker = WalkOptions::new()
        .files()
        .extension("ron")
        .walk("test_data/synthetic");
    let mut failures: Vec<String> = Vec::new();
    let mut count = 0;
    for entry in walker.flatten() {
        count += 1;
        let input = std::fs::read_to_string(entry.as_path()).unwrap();
        if let Err(e) = format_default(&input) {
            failures.push(format!("{}: {e}", entry.as_path().display()));
        }
    }
    assert!(count > 0, "no synthetic corpus files found");
    if !failures.is_empty() {
        panic!(
            "{} of {} synthetic corpus files failed to parse:\n\n{}",
            failures.len(),
            count,
            failures.join("\n\n")
        );
    }
}

/// Extract every comment from a RON source, as the exact text it occupies,
/// skipping over string/char literals so comment markers inside them don't
/// count. Block comments are handled with nesting.
fn extract_comments(src: &str) -> Vec<String> {
    let b = src.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        // raw / byte-raw string: r, r#, br, br# ... "
        if let Some(end) = scan_raw(b, i) {
            i = end;
            continue;
        }
        // standard or byte string: " ... " , b" ... "
        if b[i] == b'"'
            && let Some(end) = scan_quoted(b, i)
        {
            i = end;
            continue;
        }
        if b[i] == b'b'
            && b.get(i + 1) == Some(&b'"')
            && let Some(end) = scan_quoted(b, i + 1)
        {
            i = end;
            continue;
        }
        // char literal: ' ... '
        if b[i] == b'\'' {
            let mut k = i + 1;
            while k < b.len() {
                match b[k] {
                    b'\\' => k += 2,
                    b'\'' => {
                        k += 1;
                        break;
                    }
                    _ => k += 1,
                }
            }
            i = k;
            continue;
        }
        // line comment
        if b[i] == b'/' && b.get(i + 1) == Some(&b'/') {
            let start = i;
            i += 2;
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            out.push(src[start..i].trim_end().to_string());
            continue;
        }
        // block comment (nested)
        if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
            let start = i;
            i += 2;
            let mut depth = 1;
            while i < b.len() && depth > 0 {
                if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    i += 2;
                } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            out.push(src[start..i].to_string());
            continue;
        }
        i += 1;
    }
    out
}

/// `r"…"`, `r#"…"#`, `br#"…"#`: returns the index just past the close, or None.
fn scan_raw(b: &[u8], i: usize) -> Option<usize> {
    let mut k = i;
    if b[k] == b'b' {
        k += 1;
        if b.get(k) != Some(&b'r') {
            return None;
        }
    } else if b[k] != b'r' {
        return None;
    }
    // must be a raw string, not a bare identifier starting with r (e.g. `return`)
    // — only treat as raw if followed by # or "
    if b.get(k + 1) != Some(&b'#') && b.get(k + 1) != Some(&b'"') {
        return None;
    }
    k += 1;
    let hash_start = k;
    while b.get(k) == Some(&b'#') {
        k += 1;
    }
    if b.get(k) != Some(&b'"') {
        return None;
    }
    let hashes = k - hash_start;
    let mut j = k + 1;
    while j < b.len() {
        if b[j] == b'"'
            && j + 1 + hashes <= b.len()
            && b[j + 1..j + 1 + hashes].iter().all(|&x| x == b'#')
        {
            return Some(j + 1 + hashes);
        }
        j += 1;
    }
    None
}

/// `"…"` (with `\` escapes): index just past the closing quote.
fn scan_quoted(b: &[u8], i: usize) -> Option<usize> {
    let mut k = i + 1;
    while k < b.len() {
        match b[k] {
            b'\\' => k += 2,
            b'"' => return Some(k + 1),
            _ => k += 1,
        }
    }
    None
}

#[test]
fn synthetic_corpus_preserves_comments() {
    // The semantic oracle uses `ron::Value`, which discards comments, so a
    // regression that *drops* a comment would slip past both the equivalence
    // and idempotency checks (dropping is idempotent). This test pins the set
    // and order of comments across a format round-trip.
    let walker = WalkOptions::new()
        .files()
        .extension("ron")
        .walk("test_data/synthetic");
    let mut failures: Vec<String> = Vec::new();
    let mut count = 0;
    for entry in walker.flatten() {
        count += 1;
        let input = std::fs::read_to_string(entry.as_path()).unwrap();
        let before = extract_comments(&input);
        let output = match format_default(&input) {
            Ok(o) => o,
            Err(e) => {
                failures.push(format!("{}: parse failed: {e}", entry.as_path().display()));
                continue;
            }
        };
        let after = extract_comments(&output);
        if before != after {
            failures.push(format!(
                "{}:\n--- before ({}) ---\n{}\n--- after ({}) ---\n{}",
                entry.as_path().display(),
                before.len(),
                before.join("\n"),
                after.len(),
                after.join("\n"),
            ));
        }
    }
    assert!(count > 0, "no synthetic corpus files found");
    if !failures.is_empty() {
        panic!(
            "{} of {} synthetic files altered comments:\n\n{}",
            failures.len(),
            count,
            failures.join("\n\n"),
        );
    }
}
