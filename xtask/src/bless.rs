//! Regenerate every golden output from the current formatter, after an
//! intended change to the output. Review the diff afterwards: it is what
//! users will see change.
//!
//! - `test_data/formatted/` and `test_data/ron_corpus_formatted/`: the
//!   conformance suite in `tests/format_ron.rs` (width 40);
//! - `test_data/fmt_ron_formatted/`: `tests/fmt_ron_corpus.rs` (defaults);
//! - `test_data/fmt_ron_matrix/`: `tests/fmt_ron_matrix.rs`, which writes it
//!   itself under `FMTRON_UPDATE_MATRIX`;
//! - `test_data/stability/showcase.*.ron`: `tests/stability.rs`, under
//!   `FMTRON_BLESS`;
//! - `www/matrix/data/`: copied from the matrix by `build-matrix-site`.

use crate::util::{repo_root, ron_files};
use fmtron::{Config, format_ron, line_ending};
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn run(args: &[String]) -> Result<(), String> {
    if !args.is_empty() {
        return Err("bless takes no arguments".into());
    }
    let root = repo_root();
    let data = root.join("test_data");
    let narrow = Config::default().with_max_width(40);
    for (inputs, golden) in [
        ("unformatted", "formatted"),
        ("ron_corpus", "ron_corpus_formatted"),
    ] {
        bless_dir(&data.join(inputs), &data.join(golden), &narrow)?;
    }
    bless_dir(
        &data.join("fmt_ron"),
        &data.join("fmt_ron_formatted"),
        &Config::default(),
    )?;
    cargo_test(&root, "fmt_ron_matrix", "FMTRON_UPDATE_MATRIX")?;
    cargo_test(&root, "stability", "FMTRON_BLESS")?;
    crate::build_matrix_site::run(&[])
}

/// Rewrite each golden file in `golden` with the formatted file of the same
/// name in `inputs`, followed by the input's line ending.
fn bless_dir(inputs: &Path, golden: &Path, config: &Config) -> Result<(), String> {
    let mut changed = 0;
    let files = ron_files(golden)?;
    for path in &files {
        let name = path
            .strip_prefix(golden)
            .expect("under the golden directory");
        let input = inputs.join(name);
        let text = fs::read_to_string(&input).map_err(|e| format!("{}: {e}", input.display()))?;
        let formatted =
            format_ron(&text, config).map_err(|e| format!("{}: {e}", input.display()))?;
        let output = format!("{formatted}{}", line_ending(&text));
        if fs::read_to_string(path).ok().as_deref() != Some(output.as_str()) {
            fs::write(path, output).map_err(|e| format!("{}: {e}", path.display()))?;
            changed += 1;
        }
    }
    println!(
        "{}: {changed} of {} files changed",
        golden.display(),
        files.len()
    );
    Ok(())
}

fn cargo_test(root: &Path, test: &str, variable: &str) -> Result<(), String> {
    let status = Command::new(env!("CARGO"))
        .args(["test", "--quiet", "--test", test])
        .env(variable, "1")
        .current_dir(root)
        .status()
        .map_err(|e| format!("cargo: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("`{variable}=1 cargo test --test {test}` failed"))
    }
}
