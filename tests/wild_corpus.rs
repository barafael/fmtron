//! Runs fmtron over `test_data/wild/`: real-world RON collected from public
//! forges and crates.io, with attribution in `test_data/wild/manifest.ron`.
//!
//! For every file the reference `ron` crate accepts, the formatted output must
//! be accepted too, deserialize to the same value, be a fixed point, and keep
//! every token and every comment. Comments may move (e.g. from before a
//! colon to after it), but none may be lost or duplicated.

use fmtron::{Config, format_ron};
use std::path::{Path, PathBuf};

fn ron_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read test_data/wild") {
        let path = entry.unwrap().path();
        if path.is_dir() {
            ron_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "ron") {
            out.push(path);
        }
    }
}

/// `( )` is an empty sequence to `ron::Value` but `()` is unit; the two
/// deserialize alike into every struct and tuple type, and fmtron normalizes
/// the former to the latter.
fn norm(v: ron::Value) -> ron::Value {
    use ron::Value::*;
    match v {
        Seq(s) if s.is_empty() => Unit,
        Seq(s) => Seq(s.into_iter().map(norm).collect()),
        Option(o) => Option(o.map(|b| Box::new(norm(*b)))),
        Map(m) => Map(m.into_iter().map(|(k, v)| (norm(k), norm(v))).collect()),
        other => other,
    }
}

/// Splits RON source into (tokens, comments): `tokens` is every character
/// outside comments except whitespace and commas, in order; `comments` is the
/// sorted list of comment texts (line comments without their line ending).
fn lex(src: &str) -> (String, Vec<String>) {
    let b: Vec<char> = src.chars().collect();
    let (mut tokens, mut comments) = (String::new(), Vec::new());
    let mut i = 0;
    let quoted = |i: usize, close: char| -> usize {
        let mut k = i + 1;
        while k < b.len() && b[k] != close {
            k += if b[k] == '\\' { 2 } else { 1 };
        }
        (k + 1).min(b.len())
    };
    while i < b.len() {
        let c = b[i];
        let next = b.get(i + 1).copied();
        let end = if c == '/' && next == Some('/') {
            let mut k = i;
            while k < b.len() && b[k] != '\n' {
                k += 1;
            }
            comments.push(b[i..k].iter().collect::<String>().trim_end().to_string());
            i = k;
            continue;
        } else if c == '/' && next == Some('*') {
            let (mut k, mut depth) = (i + 2, 1);
            while k < b.len() && depth > 0 {
                if b[k] == '/' && b.get(k + 1) == Some(&'*') {
                    depth += 1;
                    k += 2;
                } else if b[k] == '*' && b.get(k + 1) == Some(&'/') {
                    depth -= 1;
                    k += 2;
                } else {
                    k += 1;
                }
            }
            comments.push(b[i..k].iter().collect());
            i = k;
            continue;
        } else if c == '"' {
            quoted(i, '"')
        } else if c == '\'' && next == Some('\'') && b.get(i + 2) == Some(&'\'') {
            i + 3
        } else if c == '\'' {
            quoted(i, '\'')
        } else if (c == 'r' || (c == 'b' && next == Some('r')))
            && (i == 0 || !(b[i - 1].is_alphanumeric() || b[i - 1] == '_'))
        {
            let mut k = i + if c == 'b' { 2 } else { 1 };
            let hashes = b[k..].iter().take_while(|&&h| h == '#').count();
            k += hashes;
            if b.get(k) == Some(&'"') {
                let close: Vec<char> = std::iter::once('"')
                    .chain(std::iter::repeat_n('#', hashes))
                    .collect();
                k += 1;
                while k < b.len() && !b[k..].starts_with(&close) {
                    k += 1;
                }
                (k + close.len()).min(b.len())
            } else {
                i + 1
            }
        } else {
            if !c.is_whitespace() && c != ',' {
                tokens.push(c);
            }
            i += 1;
            continue;
        };
        tokens.extend(&b[i..end]);
        i = end;
    }
    comments.sort();
    (tokens, comments)
}

#[test]
fn wild_corpus_formats_faithfully() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("test_data/wild");
    if !root.is_dir() {
        // Not shipped in the published crate (see `exclude` in Cargo.toml).
        eprintln!("skipping: {} not present", root.display());
        return;
    }
    let mut files = Vec::new();
    ron_files(&root, &mut files);
    // The corpus's own attribution manifest is not part of the corpus.
    files.retain(|p| p != &root.join("manifest.ron"));
    assert!(
        files.len() > 100,
        "wild corpus missing or tiny: {}",
        files.len()
    );

    let config = Config {
        max_width: 100,
        ..Config::default()
    };
    let mut failures = Vec::new();
    let mut checked = 0;
    for path in &files {
        let name = path.strip_prefix(&root).unwrap().display().to_string();
        let input = std::fs::read_to_string(path).unwrap();
        let Ok(before) = ron::from_str::<ron::Value>(&input) else {
            // Uses syntax the reference rejects (e.g. an unknown extension):
            // fmtron must still format it stably.
            if let Ok(out) = format_ron(&input, &config)
                && format_ron(&out, &config).as_ref() != Ok(&out)
            {
                failures.push(format!("{name}: not idempotent"));
            }
            continue;
        };
        checked += 1;
        let out = match format_ron(&input, &config) {
            Ok(out) => out,
            Err(e) => {
                failures.push(format!("{name}: rejected: {e}"));
                continue;
            }
        };
        match ron::from_str::<ron::Value>(&out).map(norm) {
            Ok(after) if after == norm(before) => {}
            Ok(_) => failures.push(format!("{name}: value changed")),
            Err(e) => failures.push(format!("{name}: output invalid: {e}")),
        }
        if format_ron(&out, &config).as_ref() != Ok(&out) {
            failures.push(format!("{name}: not idempotent"));
        }
        let (tin, cin) = lex(&input);
        let (tout, cout) = lex(&out);
        if tin != tout {
            failures.push(format!("{name}: tokens changed"));
        }
        if cin != cout {
            failures.push(format!("{name}: comments lost or duplicated"));
        }
    }
    assert!(
        checked > 100,
        "only {checked} files accepted by the reference"
    );
    assert!(
        failures.is_empty(),
        "{} of {} files failed:\n{}",
        failures.len(),
        files.len(),
        failures.join("\n")
    );
}
