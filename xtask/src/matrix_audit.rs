//! Audit the golden matrix: format `test_data/ron_corpus/` under every
//! settings row of `test_data/fmt_ron_matrix/` (and, with `--extended`, a set
//! of settings the matrix does not pin), check every cell, and count the
//! layouts worth a look.
//!
//! usage: cargo xtask matrix-audit [--extended] [--dump <dir>]
//!
//! Checks, each a failure:
//!
//! - the reference `ron` crate accepts the output with the input's value, the
//!   output keeps the input's tokens in order, and formatting it again
//!   changes nothing (see [`crate::oracle`]);
//! - widening `max_width` never adds lines;
//! - the input's line ending; no trailing whitespace, no blank first or last
//!   line, no blank line after an opening bracket, none at all with
//!   `blank_lines: Remove`;
//! - no line wider than `max_width` (or half of it past the line's
//!   indentation, if that is wider) in display columns, the formatter's
//!   metric, unless it has a reason (as in `tests/matrix_invariants.rs`).
//!
//! Counts, for judging layout:
//!
//! - `indent-past-width`: cells with a line indented to `max_width` or more;
//! - `newtype-split`: `Some(` / scalar / `)` on three lines, and how many of
//!   those would fit joined within `max_width + tab_size`;
//! - `stranded-single`: a lone element after a row of several, at one indent;
//! - `comment-after-opener`: an opening bracket's line followed by a comment
//!   line;
//! - `comment-run-after-colon`: two block comments in a row after a colon.
//!
//! `--dump <dir>` writes every output to `<dir>/<row>/<input>`.

use crate::oracle::{self, Verdict};
use crate::util::{ensure_dir, repo_root, ron_files};
use fmtron::{BlankLines, Config};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use unicode_width::UnicodeWidthStr;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
struct Settings {
    max_width: usize,
    tab_size: usize,
    blank_lines: Blank,
    max_depth: usize,
    max_tab: usize,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
enum Blank {
    Keep,
    Remove,
}

/// The part of a golden row the audit needs.
#[derive(Deserialize)]
struct Row {
    config: String,
    settings: Settings,
}

impl Settings {
    fn config(&self) -> Config {
        Config::default()
            .with_max_width(self.max_width)
            .with_tab_size(self.tab_size)
            .with_blank_lines(match self.blank_lines {
                Blank::Keep => BlankLines::Keep,
                Blank::Remove => BlankLines::Remove,
            })
            .with_max_nesting(self.max_depth)
            .with_max_tab(self.max_tab)
    }

    fn label(&self) -> String {
        let width = match self.max_width {
            usize::MAX => "max".to_owned(),
            w => w.to_string(),
        };
        let blank = match self.blank_lines {
            Blank::Keep => "keep",
            Blank::Remove => "remove",
        };
        format!("w{width}-t{}-{blank}-d{}", self.tab_size, self.max_depth)
    }
}

/// Settings the golden matrix does not pin: odd and extreme tabs, probes
/// around common widths, tiny widths and small depths.
fn extended() -> Vec<Settings> {
    let keep = |max_width, tab_size| Settings {
        max_width,
        tab_size,
        blank_lines: Blank::Keep,
        max_depth: fmtron::MAX_NESTING,
        max_tab: fmtron::MAX_TAB,
    };
    let mut out = Vec::new();
    for tab in [3, 5, 6, 7, 9, 10, 12] {
        out.push(keep(100, tab));
    }
    for width in [63, 65, 99, 100, 101] {
        for tab in [2, 4, 8] {
            out.push(keep(width, tab));
        }
    }
    for tab in [20, 24, 32, 40, 48, 64] {
        for width in [100, 120, 512, 1_000_000] {
            out.push(keep(width, tab));
        }
    }
    for (width, tab) in [
        (1, 64),
        (2, 1),
        (2, 4),
        (3, 4),
        (5, 4),
        (13, 4),
        (13, 13),
        (40, 2),
        (40, 8),
        (64, 2),
        (64, 4),
        (64, 8),
        (96, 4),
        (120, 2),
        (120, 8),
        (128, 4),
        (200, 4),
        (512, 4),
        (512, 24),
    ] {
        out.push(keep(width, tab));
    }
    let removed: Vec<Settings> = out
        .iter()
        .step_by(3)
        .map(|s| Settings {
            blank_lines: Blank::Remove,
            ..s.clone()
        })
        .collect();
    out.extend(removed);
    for depth in [1, 2, 3, 5, 7] {
        out.push(Settings {
            max_depth: depth,
            ..keep(100, 4)
        });
    }
    out
}

/// A cell's input and settings other than the width: input, tab size, blank
/// lines, depth limit, tab ceiling.
type Strand = (String, usize, Blank, usize, usize);

#[derive(Default)]
struct Tally {
    cells: usize,
    formatted: usize,
    rejected: usize,
    /// fmtron formats input that `ron` rejects (RON extensions in the corpus).
    accepts_invalid: usize,
    failures: Vec<String>,
    indent_past_width: usize,
    newtype_split: usize,
    newtype_split_near: usize,
    stranded_single: usize,
    comment_after_opener: usize,
    comment_run_after_colon: usize,
    /// Lines within `max_width` as chars but not as display columns.
    width_metric_gap: usize,
}

pub fn run(args: &[String]) -> Result<(), String> {
    let mut with_extended = false;
    let mut dump: Option<PathBuf> = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--extended" => with_extended = true,
            "--dump" => {
                dump = Some(PathBuf::from(
                    rest.next().ok_or("--dump needs a directory")?,
                ));
            }
            other => {
                return Err(format!(
                    "`{other}`: usage: matrix-audit [--extended] [--dump <dir>]"
                ));
            }
        }
    }
    let root = repo_root();
    let inputs = read_inputs(&root.join("test_data").join("ron_corpus"))?;
    let rows = read_rows(&root.join("test_data").join("fmt_ron_matrix"))?;
    let pinned: BTreeSet<Settings> = rows.iter().map(|(_, s)| s.clone()).collect();

    let mut sets = vec![("golden", rows)];
    if with_extended {
        let mut seen = pinned.clone();
        let extra = extended()
            .into_iter()
            .filter(|s| seen.insert(s.clone()))
            .map(|s| (s.label(), s))
            .collect();
        sets.push(("extended", extra));
    }

    std::panic::set_hook(Box::new(|_| {}));
    // (width, lines) of each cell, by everything else, for the monotonicity
    // check.
    let mut lines_by_width: BTreeMap<Strand, Vec<(usize, usize)>> = BTreeMap::new();
    let mut tallies = Vec::new();
    for (name, rows) in &sets {
        let mut tally = Tally::default();
        for (label, settings) in rows {
            let config = settings.config();
            for (input, text) in &inputs {
                tally.cells += 1;
                let out = match oracle::check(text, &config) {
                    Verdict::Ok(out) => out,
                    Verdict::Rejected { .. } => {
                        tally.rejected += 1;
                        continue;
                    }
                    Verdict::AcceptsInvalid(_) => {
                        // No value to compare; still idempotent.
                        tally.accepts_invalid += 1;
                        let Ok(out) = fmtron::format_ron(text, &config) else {
                            continue;
                        };
                        if fmtron::format_ron(&out, &config).as_ref() != Ok(&out) {
                            tally
                                .failures
                                .push(format!("{label} {input}: not-idempotent"));
                            continue;
                        }
                        out
                    }
                    other => {
                        let (status, detail) = other.status();
                        tally
                            .failures
                            .push(format!("{label} {input}: {status} {detail}"));
                        continue;
                    }
                };
                tally.formatted += 1;
                for problem in hygiene(text, &out, settings) {
                    tally.failures.push(format!("{label} {input}: {problem}"));
                }
                scan(&out, settings, &mut tally);
                lines_by_width
                    .entry((
                        input.clone(),
                        settings.tab_size,
                        settings.blank_lines,
                        settings.max_depth,
                        settings.max_tab,
                    ))
                    .or_default()
                    .push((settings.max_width, out.lines().count()));
                if let Some(dir) = &dump {
                    let dir = dir.join(label);
                    ensure_dir(&dir)?;
                    let path = dir.join(input);
                    fs::write(&path, &out).map_err(|e| format!("{}: {e}", path.display()))?;
                }
            }
        }
        tallies.push((*name, tally));
    }
    let _ = std::panic::take_hook();

    let mut comparisons = 0;
    let mut monotonicity = Vec::new();
    for ((input, tab, blank, depth, _), mut counts) in lines_by_width {
        counts.sort();
        counts.dedup_by_key(|(width, _)| *width);
        for pair in counts.windows(2) {
            comparisons += 1;
            let ((narrow, before), (wide, after)) = (pair[0], pair[1]);
            if after > before {
                monotonicity.push(format!(
                    "{input} t{tab} {blank:?} d{depth}: {before} lines at w{narrow}, {after} at w{wide}"
                ));
            }
        }
    }

    let mut failed = !monotonicity.is_empty();
    for (name, t) in &tallies {
        println!(
            "{name}: {} cells, {} formatted, {} rejected",
            t.cells, t.formatted, t.rejected
        );
        println!("  accepts input ron rejects  {:6}", t.accepts_invalid);
        println!(
            "  indent-past-width          {:6} cells",
            t.indent_past_width
        );
        println!(
            "  newtype-split              {:6} ({} would fit joined within max_width + tab_size)",
            t.newtype_split, t.newtype_split_near
        );
        println!("  stranded-single            {:6}", t.stranded_single);
        println!("  comment-after-opener       {:6}", t.comment_after_opener);
        println!(
            "  comment-run-after-colon    {:6}",
            t.comment_run_after_colon
        );
        println!(
            "  width-metric gap           {:6} lines",
            t.width_metric_gap
        );
        println!("  failures                   {:6}", t.failures.len());
        for failure in t.failures.iter().take(20) {
            println!("    {failure}");
        }
        failed |= !t.failures.is_empty();
    }
    println!(
        "monotonicity: {comparisons} width comparisons, {} violations",
        monotonicity.len()
    );
    for violation in monotonicity.iter().take(20) {
        println!("    {violation}");
    }
    if failed {
        Err("the matrix audit found failures".into())
    } else {
        Ok(())
    }
}

fn read_inputs(dir: &Path) -> Result<Vec<(String, String)>, String> {
    ron_files(dir)?
        .into_iter()
        .map(|path| {
            let name = path
                .file_name()
                .expect("file name")
                .to_string_lossy()
                .into_owned();
            let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok((name, text))
        })
        .collect()
}

fn read_rows(dir: &Path) -> Result<Vec<(String, Settings)>, String> {
    let mut rows = Vec::new();
    for path in ron_files(dir)? {
        if path.file_name().is_some_and(|name| name == "manifest.ron") {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let row: Row = ron::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let label = row.config.trim_end_matches(".ron").to_owned();
        rows.push((label, row.settings));
    }
    if rows.is_empty() {
        return Err(format!("{}: no matrix rows", dir.display()));
    }
    Ok(rows)
}

fn indent(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

fn opens(line: &str) -> bool {
    line.ends_with(['(', '[', '{'])
}

/// What is wrong with `out`'s whitespace and width, for `input`.
fn hygiene(input: &str, out: &str, settings: &Settings) -> Vec<String> {
    let lines: Vec<&str> = out.split('\n').map(|l| l.trim_end_matches('\r')).collect();
    let mut problems = Vec::new();
    if out.contains('\n') && fmtron::line_ending(out) != fmtron::line_ending(input) {
        problems.push("line ending changed".to_owned());
    }
    if lines.first().is_some_and(|l| l.is_empty()) || lines.last().is_some_and(|l| l.is_empty()) {
        problems.push("blank first or last line".to_owned());
    }
    for (i, line) in lines.iter().enumerate() {
        let n = i + 1;
        if line.ends_with([' ', '\t']) {
            problems.push(format!("line {n}: trailing whitespace"));
        }
        if line.is_empty() && i > 0 && opens(lines[i - 1]) {
            problems.push(format!("line {n}: blank line after an opening bracket"));
        }
        if line.is_empty() && settings.blank_lines == Blank::Remove {
            problems.push(format!("line {n}: blank line with blank_lines: Remove"));
        }
        let limit = line_limit(settings.max_width, indent(line));
        if line.width() > limit && !has_unbreakable_content(line, limit) {
            problems.push(format!(
                "line {n}: breakable line is {} columns wide: {line:?}",
                line.width()
            ));
        }
    }
    problems
}

/// The width a line indented by `indent` is laid out within: `max_width`, or
/// half of it past the indentation, whichever is wider (as in
/// `src/pretty.rs`).
fn line_limit(max_width: usize, indent: usize) -> usize {
    max_width.max(indent.saturating_add(max_width / 2))
}

/// Whether an over-wide line has a reason to be over-wide; as in
/// `tests/matrix_invariants.rs`.
fn has_unbreakable_content(line: &str, width: usize) -> bool {
    let trimmed = line.trim_start();
    if line.width() - trimmed.width() >= width {
        return true;
    }
    if ["//", "/*", "*", "#!["]
        .iter()
        .any(|p| trimmed.starts_with(p))
        || ["//", "/*", "#![", "#["]
            .iter()
            .any(|p| trimmed.contains(p))
        || trimmed.contains(['"', '\''])
    {
        return true;
    }
    if trimmed
        .split_whitespace()
        .any(|token| token.width() >= width)
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
    line.match_indices(',').any(|(index, _)| {
        let tail = line[index + 1..].trim_start();
        let tail = tail.trim_start_matches([')', ']', '}']).trim_start();
        !tail.is_empty() && !tail.starts_with(',')
    })
}

/// A line holding one list element and its comma, nothing nested.
fn single_item(trimmed: &str) -> bool {
    trimmed.ends_with(',')
        && !trimmed.contains(", ")
        && !trimmed.contains([':', '(', ')', '[', ']', '{', '}', '/'])
}

/// A line holding several list elements, nothing nested.
fn packed_row(trimmed: &str) -> bool {
    trimmed.ends_with(',')
        && trimmed.contains(", ")
        && !trimmed.contains([':', '[', ']', '{', '}', '/'])
}

fn scan(out: &str, settings: &Settings, tally: &mut Tally) {
    let lines: Vec<&str> = out.lines().collect();
    let width = settings.max_width;
    if lines.iter().any(|l| !l.is_empty() && indent(l) >= width) {
        tally.indent_past_width += 1;
    }
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if line.chars().count() <= width && line.width() > width {
            tally.width_metric_gap += 1;
        }
        if trimmed.contains(": /*") && trimmed.contains("*/ /*") {
            tally.comment_run_after_colon += 1;
        }
        if opens(line)
            && lines
                .get(i + 1)
                .is_some_and(|next| next.trim_start().starts_with("//"))
        {
            tally.comment_after_opener += 1;
        }
        if let (Some(mid), Some(close)) = (lines.get(i + 1), lines.get(i + 2)) {
            let head = trimmed.trim_end_matches('(');
            let close_t = close.trim_start();
            if line.ends_with('(')
                && !head.is_empty()
                && head.ends_with(|c: char| c.is_alphanumeric() || c == '_')
                && single_item(mid.trim_start())
                && close_t.starts_with(')')
                && indent(close) == indent(line)
            {
                tally.newtype_split += 1;
                let scalar = mid.trim_start().trim_end_matches(',');
                if line.width() + scalar.width() + close_t.width() <= width + settings.tab_size {
                    tally.newtype_split_near += 1;
                }
            }
        }
        if i > 0
            && single_item(trimmed)
            && packed_row(lines[i - 1].trim_start())
            && indent(lines[i - 1]) == indent(line)
        {
            tally.stranded_single += 1;
        }
    }
}
