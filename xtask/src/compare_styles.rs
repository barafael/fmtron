//! Compare the output of fmtron builds over a corpus, to judge a style change.
//!
//! usage: cargo xtask compare-styles <out dir> <name>=<fmtron binary>... -- <corpus dir>...
//!
//! For each build, formats every `.ron` file and prints, over the files every
//! build formats:
//!
//! - churn:   lines removed plus lines added from input to output, summed
//! - lines:   output lines, summed
//! - halved:  files (over 5 lines) whose output has under half the input's lines
//! - doubled: files (over 2 lines) whose output has over twice the input's lines
//! - differs: files whose output differs from the first build's
//!
//! Writes each build's output to `<out dir>/<name>/<input path>`.

use crate::util::{ensure_dir, ron_files};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct Metrics {
    input_lines: usize,
    output_lines: usize,
    churn: usize,
    text: String,
}

pub fn run(args: &[String]) -> Result<(), String> {
    let sep = args
        .iter()
        .position(|arg| arg == "--")
        .ok_or("usage: compare-styles <out dir> <name>=<bin>... -- <dir>...")?;
    if sep < 2 || sep + 1 >= args.len() {
        return Err("usage: compare-styles <out dir> <name>=<bin>... -- <dir>...".into());
    }
    let out_dir = PathBuf::from(&args[0]);
    let mut builds = Vec::new();
    for arg in &args[1..sep] {
        let (name, binary) = arg
            .split_once('=')
            .ok_or_else(|| format!("`{arg}`: expected <name>=<binary>"))?;
        builds.push((name.to_owned(), PathBuf::from(binary)));
    }
    if builds.is_empty() {
        return Err("compare-styles needs at least one build".into());
    }

    let mut files = Vec::new();
    for dir in &args[sep + 1..] {
        files.extend(ron_files(Path::new(dir))?);
    }

    let mut results: Vec<(String, HashMap<PathBuf, Metrics>)> = Vec::new();
    for (name, binary) in &builds {
        results.push((name.clone(), run_build(binary, &files)?));
    }

    let mut common: HashSet<PathBuf> = results[0].1.keys().cloned().collect();
    for (_, result) in &results[1..] {
        common.retain(|path| result.contains_key(path));
    }
    let mut seen = HashSet::new();
    let common: Vec<PathBuf> = files
        .iter()
        .filter(|path| common.contains(*path) && seen.insert((*path).clone()))
        .cloned()
        .collect();

    let first = &results[0].1;
    println!("{} files formatted by every build\n", common.len());
    println!(
        "{:<20} {:>8} {:>8} {:>7} {:>8} {:>8}",
        "build", "churn", "lines", "halved", "doubled", "differs"
    );
    for (name, result) in &results {
        let rows: Vec<&Metrics> = common.iter().filter_map(|path| result.get(path)).collect();
        let churn: usize = rows.iter().map(|metrics| metrics.churn).sum();
        let lines: usize = rows.iter().map(|metrics| metrics.output_lines).sum();
        let halved = rows
            .iter()
            .filter(|metrics| {
                metrics.input_lines > 5 && 2 * metrics.output_lines < metrics.input_lines
            })
            .count();
        let doubled = rows
            .iter()
            .filter(|metrics| {
                metrics.input_lines > 2 && metrics.output_lines > 2 * metrics.input_lines
            })
            .count();
        let differs = common
            .iter()
            .filter(|path| match (result.get(*path), first.get(*path)) {
                (Some(a), Some(b)) => a.text != b.text,
                _ => false,
            })
            .count();
        println!("{name:<20} {churn:>8} {lines:>8} {halved:>7} {doubled:>8} {differs:>8}");
    }
    let input_lines: usize = common
        .iter()
        .filter_map(|path| first.get(path))
        .map(|metrics| metrics.input_lines)
        .sum();
    println!("\ninput lines: {input_lines}");

    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    for (name, result) in &results {
        for path in &common {
            let Some(metrics) = result.get(path) else {
                continue;
            };
            let relative = path.strip_prefix(&cwd).unwrap_or(path);
            let destination = out_dir.join(name).join(relative);
            ensure_dir(destination.parent().unwrap_or(&out_dir))?;
            std::fs::write(&destination, &metrics.text)
                .map_err(|e| format!("{}: {e}", destination.display()))?;
        }
    }
    Ok(())
}

fn run_build(binary: &Path, files: &[PathBuf]) -> Result<HashMap<PathBuf, Metrics>, String> {
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(files.len().max(1));
    let chunk = files.len().div_ceil(workers).max(1);

    let mut result = HashMap::new();
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for slice in files.chunks(chunk) {
            handles.push(scope.spawn(move || {
                slice
                    .iter()
                    .filter_map(|path| {
                        format_one(binary, path)
                            .ok()
                            .flatten()
                            .map(|metrics| (path.clone(), metrics))
                    })
                    .collect::<Vec<_>>()
            }));
        }
        for handle in handles {
            let rows = handle.join().expect("format worker panicked");
            result.extend(rows);
        }
    });
    Ok(result)
}

fn format_one(binary: &Path, path: &Path) -> Result<Option<Metrics>, String> {
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut child = Command::new(binary)
        .arg("--no-config")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{}: {e}", binary.display()))?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let input = data.clone();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(&input);
    });
    let output = child
        .wait_with_output()
        .map_err(|e| format!("{}: {e}", binary.display()))?;
    let _ = writer.join();
    if !output.status.success() {
        return Ok(None);
    }

    let out = String::from_utf8_lossy(&output.stdout).into_owned();
    let a = lines(&String::from_utf8_lossy(&data));
    let b = lines(&out);
    let same = matching(&a, &b);
    Ok(Some(Metrics {
        input_lines: a.len(),
        output_lines: b.len(),
        churn: (a.len() - same) + (b.len() - same),
        text: out,
    }))
}

fn lines(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .trim_end_matches('\n')
        .split('\n')
        .map(str::to_owned)
        .collect()
}

/// Number of lines matched by Python difflib's Ratcliff/Obershelp algorithm
/// (`SequenceMatcher(..., autojunk=False).get_matching_blocks()`).
fn matching(a: &[String], b: &[String]) -> usize {
    let mut total = 0;
    let mut stack = vec![(0, a.len(), 0, b.len())];
    while let Some((alo, ahi, blo, bhi)) = stack.pop() {
        let (i, j, size) = longest_match(a, b, alo, ahi, blo, bhi);
        if size == 0 {
            continue;
        }
        total += size;
        if alo < i && blo < j {
            stack.push((alo, i, blo, j));
        }
        if i + size < ahi && j + size < bhi {
            stack.push((i + size, ahi, j + size, bhi));
        }
    }
    total
}

fn longest_match(
    a: &[String],
    b: &[String],
    alo: usize,
    ahi: usize,
    blo: usize,
    bhi: usize,
) -> (usize, usize, usize) {
    let (mut best_i, mut best_j, mut best_size) = (alo, blo, 0);
    let mut previous: HashMap<usize, usize> = HashMap::new();
    for (i, item) in a.iter().enumerate().take(ahi).skip(alo) {
        let mut current = HashMap::new();
        for (j, other) in b.iter().enumerate().take(bhi).skip(blo) {
            if item != other {
                continue;
            }
            let size = previous.get(&j.wrapping_sub(1)).copied().unwrap_or(0) + 1;
            current.insert(j, size);
            if size > best_size {
                best_i = i + 1 - size;
                best_j = j + 1 - size;
                best_size = size;
            }
        }
        previous = current;
    }
    (best_i, best_j, best_size)
}
