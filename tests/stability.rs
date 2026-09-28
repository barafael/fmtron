//! Formatting stability. Every change to fmtron's output is churn in its
//! users' diffs, so the output must only ever change on purpose.
//!
//! `test_data/stability/showcase.ron` exercises nearly every construct,
//! comment position and layout decision fmtron makes, written messily. What
//! the CLI writes for it under each configuration in [`CONFIGS`] is pinned,
//! byte for byte, in `showcase.<name>.ron` next to it.
//!
//! If a formatting change is intended, regenerate the pinned files with
//!
//! ```sh
//! FMTRON_BLESS=1 cargo test --test stability
//! ```
//!
//! and review their diff: it is exactly what users will see change.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const INPUT: &str = "test_data/stability/showcase.ron";

/// Each pinned output's name and the CLI flags producing it.
const CONFIGS: [(&str, &[&str]); 3] = [
    ("default", &[]),
    // The default width before 0.9: breaks and hugs nearly everything.
    ("narrow", &["-w", "40"]),
    ("compact", &["-t", "2", "--blank-lines", "remove"]),
];

fn pinned(name: &str) -> PathBuf {
    Path::new(INPUT).with_file_name(format!("showcase.{name}.ron"))
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// What the CLI prints for `input` on stdin. Any `fmt.ron` around the test
/// run is ignored, so only `flags` apply.
fn fmtron(input: &str, flags: &[&str]) -> String {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fmtron"))
        .arg("--no-config")
        .args(flags)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run fmtron");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "fmtron {flags:?} failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).expect("fmtron prints UTF-8")
}

/// A unified diff from `expected` to `actual`, or `None` if they are equal.
fn diff(expected: &str, actual: &str, label: &str) -> Option<String> {
    (expected != actual).then(|| {
        similar::TextDiff::from_lines(expected, actual)
            .unified_diff()
            .header(label, &format!("{label} (now)"))
            .to_string()
    })
}

#[test]
fn output_matches_the_pinned_files() {
    let input = read(Path::new(INPUT));
    let bless = std::env::var_os("FMTRON_BLESS").is_some();
    let mut failures = Vec::new();
    for (name, flags) in CONFIGS {
        let path = pinned(name);
        let actual = fmtron(&input, flags);
        if bless {
            std::fs::write(&path, &actual).unwrap();
            continue;
        }
        let Ok(expected) = std::fs::read_to_string(&path) else {
            failures.push(format!("{} is missing", path.display()));
            continue;
        };
        failures.extend(diff(&expected, &actual, &path.display().to_string()));
    }
    assert!(
        failures.is_empty(),
        "fmtron's output changed. If that is intended, rerun with FMTRON_BLESS=1 \
         to update the pinned files.\n\n{}",
        failures.join("\n")
    );
}

/// Reformatting formatted output, as users do on every save, changes nothing.
#[test]
fn output_is_a_fixed_point() {
    let input = read(Path::new(INPUT));
    for (name, flags) in CONFIGS {
        let once = fmtron(&input, flags);
        let twice = fmtron(&once, flags);
        if let Some(d) = diff(&once, &twice, name) {
            panic!("reformatting the {name} output changes it:\n{d}");
        }
    }
}

/// CRLF input gives the same output, with CRLF line endings.
#[test]
fn crlf_input_gives_the_same_output() {
    let input = read(Path::new(INPUT));
    let lf = fmtron(&input, &[]);
    let crlf = fmtron(&input.replace('\n', "\r\n"), &[]);
    if let Some(d) = diff(&lf.replace('\n', "\r\n"), &crlf, "default") {
        panic!("CRLF input formats differently:\n{d}");
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

/// The showcase is valid RON to the reference `ron` crate, and formatting
/// keeps its meaning, so a pinned output can never bless a broken one.
#[test]
fn output_keeps_the_meaning() {
    let value = |src: &str, what: &str| {
        norm(ron::from_str(src).unwrap_or_else(|e| panic!("{what} is not valid RON: {e}")))
    };
    let input = read(Path::new(INPUT));
    let before = value(&input, INPUT);
    for (name, flags) in CONFIGS {
        let after = value(&fmtron(&input, flags), name);
        assert!(after == before, "the {name} output means something else");
    }
}
