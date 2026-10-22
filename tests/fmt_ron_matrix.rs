//! Golden matrix: every file in `test_data/ron_corpus/` formatted with the
//! effective settings of every `fmt.ron` in `test_data/fmt_ron/`.
//!
//! The results live in `test_data/fmt_ron_matrix/`, one RON file per config,
//! named after the config file. Each holds the config's effective settings and
//! one entry per corpus file, either `Formatted("...")` or `Failed("...")`.
//! `manifest.ron` records the inputs and configs the matrix covers.
//!
//! Regenerate the golden master after an intentional formatter change:
//!
//! ```sh
//! FMTRON_UPDATE_MATRIX=1 cargo test --test fmt_ron_matrix
//! ```
//!
//! ## Reading a failure
//!
//! A formatting change fails once per affected config row, with the first
//! changed cell and its byte offset. For example, adding a second space after
//! map keys made every row fail:
//!
//! ```text
//! 164 matrix rows differ from the golden master; regenerate with
//! `FMTRON_UPDATE_MATRIX=1 cargo test --test fmt_ron_matrix`:
//! test_data/fmt_ron/blank-keep.ron
//! ron_003_67948c19c226.ron: output differs at byte 10
//! (current 19 bytes, golden 18 bytes)
//! ```
//!
//! Only the first differing cell in each row is reported, so a systematic
//! change produces a short, repeated report rather than 13,448 lines. The
//! matrix pins bytes, not just meaning: the new output can be valid RON and
//! still fail here. Decide whether the new layout is intended before
//! regenerating.
//!
//! Run the whole test suite too. `fmt_ron_corpus` formats struct-like config
//! files and can miss map-only changes; `wild_corpus` checks semantics rather
//! than exact bytes. The matrix is the cross-product check that catches both.

use fmtron::{BlankLines, Config, FileConfig, format_ron};
use ron::extensions::Extensions;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const SOURCE: &str = "test_data/ron_corpus";
const CONFIGS: &str = "test_data/fmt_ron";
const MATRIX: &str = "test_data/fmt_ron_matrix";
const MANIFEST: &str = "test_data/fmt_ron_matrix/manifest.ron";
const DESCRIPTION: &str = "Golden matrix: each test_data/ron_corpus file formatted with each fmt.ron in test_data/fmt_ron.";

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Manifest {
    description: String,
    source_corpus: String,
    config_corpus: String,
    total_inputs: usize,
    total_configs: usize,
    total_cells: usize,
    /// Sorted file names, relative to `source_corpus`.
    inputs: Vec<String>,
    /// Sorted file names, relative to `config_corpus`.
    configs: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Settings {
    max_width: usize,
    tab_size: usize,
    blank_lines: GoldenBlankLines,
    max_depth: usize,
    max_tab: usize,
    style_edition: u16,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
enum GoldenBlankLines {
    Keep,
    Remove,
}

impl From<BlankLines> for GoldenBlankLines {
    fn from(value: BlankLines) -> Self {
        match value {
            BlankLines::Keep => Self::Keep,
            BlankLines::Remove => Self::Remove,
        }
    }
}

impl Settings {
    fn of(config: &Config) -> Self {
        Self {
            max_width: config.max_width,
            tab_size: config.tab_size,
            blank_lines: config.blank_lines.into(),
            max_depth: config.max_nesting,
            max_tab: config.max_tab,
            style_edition: config.style_edition.year(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Matrix {
    /// The `fmt.ron` file name this row belongs to.
    config: String,
    /// The settings after applying the config to the built-in defaults.
    settings: Settings,
    /// One entry per corpus input, sorted by input file name.
    results: Vec<Cell>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct Cell {
    input: String,
    result: Outcome,
}

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
enum Outcome {
    Formatted(String),
    Failed(String),
}

fn ron_files(dir: &str, skip: &[&str]) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("cannot read {dir}: {e}"))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ron"))
        .filter_map(|path| {
            let name = path.file_name()?.to_string_lossy().into_owned();
            (!skip.contains(&name.as_str())).then_some(name)
        })
        .collect();
    names.sort();
    names
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn config_for(name: &str) -> (Config, Settings) {
    let path = Path::new(CONFIGS).join(name);
    let text = read(&path);
    let file: FileConfig = ron::Options::default()
        .with_default_extension(Extensions::IMPLICIT_SOME)
        .from_str(&text)
        .unwrap_or_else(|e| panic!("{}: invalid fmt.ron: {e}", path.display()));
    let mut config = Config::default();
    file.apply(&mut config);
    (config, Settings::of(&config))
}

fn generate() -> (Manifest, Vec<Matrix>) {
    let inputs = ron_files(SOURCE, &[]);
    let configs = ron_files(CONFIGS, &["manifest.ron"]);
    let input_texts: Vec<(String, String)> = inputs
        .iter()
        .map(|name| (name.clone(), read(&Path::new(SOURCE).join(name))))
        .collect();

    let mut matrix = Vec::with_capacity(configs.len());
    for config_name in &configs {
        let (config, settings) = config_for(config_name);
        let results = input_texts
            .iter()
            .map(|(input, text)| Cell {
                input: input.clone(),
                result: match format_ron(text, &config) {
                    Ok(formatted) => Outcome::Formatted(formatted),
                    Err(error) => Outcome::Failed(error.to_string()),
                },
            })
            .collect();
        matrix.push(Matrix {
            config: config_name.clone(),
            settings,
            results,
        });
    }

    let manifest = Manifest {
        description: DESCRIPTION.to_string(),
        source_corpus: SOURCE.to_string(),
        config_corpus: CONFIGS.to_string(),
        total_inputs: inputs.len(),
        total_configs: configs.len(),
        total_cells: inputs.len() * configs.len(),
        inputs,
        configs,
    };
    (manifest, matrix)
}

fn encode<T: Serialize>(value: &T) -> String {
    let mut text = ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default())
        .expect("serialize the golden matrix");
    text.push('\n');
    text
}

fn write_golden(manifest: &Manifest, matrices: &[Matrix]) {
    std::fs::create_dir_all(MATRIX).expect("create the matrix directory");
    std::fs::write(MANIFEST, encode(manifest)).expect("write the matrix manifest");

    let expected: BTreeSet<PathBuf> = matrices
        .iter()
        .map(|matrix| Path::new(MATRIX).join(&matrix.config))
        .collect();
    for matrix in matrices {
        let path = Path::new(MATRIX).join(&matrix.config);
        std::fs::write(&path, encode(matrix))
            .unwrap_or_else(|e| panic!("cannot write {}: {e}", path.display()));
    }

    // A config that was removed must not leave a stale row behind.
    for entry in std::fs::read_dir(MATRIX).expect("read the matrix directory") {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if path.extension().is_some_and(|ext| ext == "ron")
            && name != "manifest.ron"
            && !expected.contains(&path)
        {
            std::fs::remove_file(&path)
                .unwrap_or_else(|e| panic!("cannot remove {}: {e}", path.display()));
        }
    }
}

/// A short description of the first difference, for a readable failure.
fn describe_difference(current: &Matrix, golden: &Matrix) -> Option<String> {
    if current.settings != golden.settings {
        return Some(format!(
            "settings differ: current {:?}, golden {:?}",
            current.settings, golden.settings
        ));
    }
    if current.results.len() != golden.results.len() {
        return Some(format!(
            "result count differs: current {}, golden {}",
            current.results.len(),
            golden.results.len()
        ));
    }
    for (current_cell, golden_cell) in current.results.iter().zip(&golden.results) {
        if current_cell.input != golden_cell.input {
            return Some(format!(
                "input order differs: current {}, golden {}",
                current_cell.input, golden_cell.input
            ));
        }
        if current_cell.result != golden_cell.result {
            return Some(match (&current_cell.result, &golden_cell.result) {
                (Outcome::Formatted(current), Outcome::Formatted(golden)) => {
                    let at = current
                        .bytes()
                        .zip(golden.bytes())
                        .position(|(a, b)| a != b)
                        .unwrap_or_else(|| current.len().min(golden.len()));
                    format!(
                        "{}: output differs at byte {at} (current {} bytes, golden {} bytes)\n\
                         current: {:?}\n\
                         golden:  {:?}",
                        current_cell.input,
                        current.len(),
                        golden.len(),
                        String::from_utf8_lossy(&current.as_bytes()[..at.min(current.len())]),
                        String::from_utf8_lossy(&golden.as_bytes()[..at.min(golden.len())]),
                    )
                }
                (Outcome::Formatted(_), Outcome::Failed(error)) => format!(
                    "{}: current output is formatted, golden failed with {error:?}",
                    current_cell.input
                ),
                (Outcome::Failed(error), Outcome::Formatted(_)) => format!(
                    "{}: current failed with {error:?}, golden output is formatted",
                    current_cell.input
                ),
                (Outcome::Failed(current), Outcome::Failed(golden)) => format!(
                    "{}: error differs: current {current:?}, golden {golden:?}",
                    current_cell.input
                ),
            });
        }
    }
    None
}

#[test]
fn matrix_matches_golden_master() {
    let (manifest, matrices) = generate();

    if std::env::var_os("FMTRON_UPDATE_MATRIX").is_some() {
        write_golden(&manifest, &matrices);
        eprintln!(
            "updated {MATRIX}: {} configs x {} inputs = {} cells",
            manifest.total_configs, manifest.total_inputs, manifest.total_cells
        );
        return;
    }

    let golden_manifest: Manifest = ron::from_str(&read(Path::new(MANIFEST)))
        .unwrap_or_else(|e| panic!("cannot parse {MANIFEST}: {e}"));
    assert_eq!(
        manifest, golden_manifest,
        "the matrix manifest is stale; regenerate with \
         `FMTRON_UPDATE_MATRIX=1 cargo test --test fmt_ron_matrix`"
    );

    let mut failures = Vec::new();
    for current in &matrices {
        let path = Path::new(MATRIX).join(&current.config);
        let text = read(&path);
        let golden: Matrix =
            ron::from_str(&text).unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()));
        if current.config != golden.config {
            failures.push(format!(
                "{}: file contains the row for {}",
                path.display(),
                golden.config
            ));
        } else if let Some(difference) = describe_difference(current, &golden) {
            failures.push(format!(
                "{}\n{difference}",
                Path::new(CONFIGS).join(&current.config).display()
            ));
        }
    }

    let expected: BTreeSet<PathBuf> = matrices
        .iter()
        .map(|matrix| Path::new(MATRIX).join(&matrix.config))
        .collect();
    for entry in std::fs::read_dir(MATRIX).expect("read the matrix directory") {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy();
        if path.extension().is_some_and(|ext| ext == "ron")
            && name != "manifest.ron"
            && !expected.contains(&path)
        {
            failures.push(format!(
                "{}: stale matrix row; regenerate with \
                 `FMTRON_UPDATE_MATRIX=1 cargo test --test fmt_ron_matrix`",
                path.display()
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} matrix rows differ from the golden master; regenerate with \
         `FMTRON_UPDATE_MATRIX=1 cargo test --test fmt_ron_matrix`:\n{}",
        failures.len(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n\n")
    );
}
