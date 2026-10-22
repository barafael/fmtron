//! Invariants over the golden matrix, independent of the golden bytes.
//!
//! The matrix test pins layout; this one checks properties that must hold for
//! every formatted cell: idempotence, no trailing whitespace, preserved line
//! endings, and overruns only where the line contains something the formatter
//! cannot break (a long atom, literal, comment, attribute, or indentation).

use fmtron::{BlankLines, Config, format_ron};
use serde::Deserialize;
use std::collections::BTreeMap;

const SOURCE: &str = "test_data/ron_corpus";
const MATRIX: &str = "test_data/fmt_ron_matrix";

#[derive(Debug, Deserialize)]
struct Settings {
    max_width: usize,
    tab_size: usize,
    blank_lines: GoldenBlankLines,
    max_depth: usize,
    max_tab: usize,
    style_edition: u16,
}

#[derive(Debug, Clone, Copy, Deserialize)]
enum GoldenBlankLines {
    Keep,
    Remove,
}

#[derive(Debug, Deserialize)]
struct Matrix {
    config: String,
    settings: Settings,
    results: Vec<Cell>,
}

#[derive(Debug, Deserialize)]
struct Cell {
    input: String,
    result: Outcome,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
enum Outcome {
    Formatted(String),
    Failed(String),
}

fn config(settings: &Settings) -> Config {
    assert_eq!(settings.style_edition, 2026, "unknown golden style edition");
    Config::default()
        .with_max_width(settings.max_width)
        .with_tab_size(settings.tab_size)
        .with_max_tab(settings.max_tab)
        .with_max_nesting(settings.max_depth)
        .with_blank_lines(match settings.blank_lines {
            GoldenBlankLines::Keep => BlankLines::Keep,
            GoldenBlankLines::Remove => BlankLines::Remove,
        })
}

#[test]
fn every_formatted_cell_obeys_the_layout_invariants() {
    let mut inputs: BTreeMap<String, String> = BTreeMap::new();
    for entry in std::fs::read_dir(SOURCE).expect("read the input corpus") {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|ext| ext == "ron") {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            inputs.insert(name, std::fs::read_to_string(&path).unwrap());
        }
    }

    let mut files: Vec<_> = std::fs::read_dir(MATRIX)
        .expect("read the matrix")
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "ron")
                && path.file_name().is_some_and(|name| name != "manifest.ron")
        })
        .collect();
    files.sort();

    let mut checked = 0usize;
    let mut failures = Vec::new();
    for path in files {
        let matrix: Matrix = ron::from_str(
            &std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display())),
        )
        .unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()));
        let config = config(&matrix.settings);

        for cell in &matrix.results {
            let Outcome::Formatted(output) = &cell.result else {
                continue;
            };
            checked += 1;
            let label = format!("{} / {}", matrix.config, cell.input);

            if let Some(line) = output
                .lines()
                .find(|line| line.ends_with(' ') || line.ends_with('\t'))
            {
                failures.push(format!("{label}: trailing whitespace in {line:?}"));
            }

            let idempotent = format_ron(output, &config);
            match idempotent {
                Ok(again) if again == *output => {}
                Ok(_) => failures.push(format!("{label}: not idempotent")),
                Err(error) => failures.push(format!("{label}: reformat failed: {error}")),
            }

            let input = &inputs[&cell.input];
            if output.contains('\n') && fmtron::line_ending(output) != fmtron::line_ending(input) {
                failures.push(format!("{label}: line ending changed"));
            }

            for line in output.lines() {
                let width = line.chars().count();
                if width > matrix.settings.max_width
                    && !has_unbreakable_content(line, matrix.settings.max_width)
                {
                    failures.push(format!(
                        "{label}: breakable line is {width} columns wide (limit {}): {line:?}",
                        matrix.settings.max_width
                    ));
                }
            }
        }
    }

    assert!(checked > 0, "no formatted matrix cells found");
    assert!(
        failures.is_empty(),
        "{} invariant failures (first 20):\n{}",
        failures.len(),
        failures
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Whether an over-wide line has a reason to be over-wide: indentation alone,
/// a comment or attribute the formatter keeps together, a quoted literal, or
/// one token longer than the width.
fn has_unbreakable_content(line: &str, width: usize) -> bool {
    let trimmed = line.trim_start();
    let indent = line.chars().count() - trimmed.chars().count();
    if indent >= width {
        return true;
    }
    if trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.contains("//")
        || trimmed.contains("/*")
        || trimmed.contains("#![")
        || trimmed.contains("#[")
    {
        return true;
    }
    if trimmed.contains('"') || trimmed.contains('\'') {
        return true;
    }
    if trimmed
        .split_whitespace()
        .any(|token| token.chars().count() >= width)
    {
        return true;
    }
    // A line that no longer contains a comma with content after it has no
    // break opportunity left: it is one key, one value, or one wrapper.
    !has_breakable_comma(trimmed)
}

/// Whether a comma is followed by another element on the same line, as
/// opposed to a trailing comma before the closing delimiter.
fn has_breakable_comma(line: &str) -> bool {
    for (index, _) in line.match_indices(',') {
        let tail = line[index + 1..].trim_start();
        let tail = tail.trim_start_matches([')', ']', '}']).trim_start();
        if !tail.is_empty() && !tail.starts_with(',') {
            return true;
        }
    }
    false
}
