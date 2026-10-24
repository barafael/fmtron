//! Merge the collected indexes, dedupe by content, run fmtron's `wild`
//! harness over everything, and summarize. Writes `results.jsonl` (one record
//! per unique file, with provenance + status) and prints a summary.

use crate::util::{ensure_dir, read_jsonl, repo_root, sha256_hex, work_dir};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};

pub fn run() -> Result<(), String> {
    let work = work_dir();
    ensure_dir(&work)?;
    let root = repo_root();
    let harness_dir = root.join("tools").join("wild-corpus").join("harness");
    let harness = harness_dir
        .join("target")
        .join("release")
        .join("wild-harness");

    let mut records = Vec::new();
    for index in [
        "gh_files.jsonl",
        "crates_index.jsonl",
        "forge_index.jsonl",
        "codeberg_index.jsonl",
    ] {
        records.extend(read_jsonl(&work.join(index))?);
    }

    let mut order: Vec<Value> = Vec::new();
    let mut by_hash: HashMap<String, usize> = HashMap::new();
    let mut duplicates = 0;
    let collected = records.len();
    for mut record in records {
        let Some(local) = record
            .get("local")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            continue;
        };
        let path = work.join(&local);
        if !path.exists() {
            continue;
        }
        let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let hash = sha256_hex(&bytes);
        if let Some(&existing) = by_hash.get(&hash) {
            let origin = record
                .get("html_url")
                .or_else(|| record.get("source_url"))
                .cloned()
                .unwrap_or(Value::Null);
            if let Some(object) = order[existing].as_object_mut() {
                object
                    .entry("also_at")
                    .or_insert_with(|| Value::Array(Vec::new()))
                    .as_array_mut()
                    .expect("also_at is an array")
                    .push(origin);
            }
            duplicates += 1;
            continue;
        }
        record
            .as_object_mut()
            .expect("records are objects")
            .insert("sha256".into(), Value::String(hash.clone()));
        by_hash.insert(hash, order.len());
        order.push(record);
    }

    if order.is_empty() {
        return Err(format!(
            "no collected files under {}; run a `cargo xtask wild ...-collect` task first",
            work.display()
        ));
    }
    build_harness(&harness_dir)?;
    let mut input = String::new();
    for record in &order {
        let local = record.get("local").and_then(Value::as_str).unwrap_or("");
        input.push_str(&work.join(local).to_string_lossy());
        input.push('\n');
    }
    let output = run_harness(&harness, &input)?;
    let mut status: HashMap<String, (String, String)> = HashMap::new();
    for line in output.lines() {
        let mut parts = line.splitn(3, '\t');
        let path = parts.next().unwrap_or("");
        let state = parts.next().unwrap_or("").to_owned();
        let detail = parts.next().unwrap_or("").to_owned();
        status.insert(relative(&work, Path::new(path)), (state, detail));
    }

    let mut results = Vec::new();
    for record in &order {
        let local = record.get("local").and_then(Value::as_str).unwrap_or("");
        let (mut state, mut detail) = status
            .get(local)
            .cloned()
            .unwrap_or_else(|| ("missing".into(), String::new()));
        let path = work.join(local);
        let text = fs::read(&path)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default();
        let trimmed = text.trim();
        if !trimmed.contains('\n')
            && !trimmed.contains(' ')
            && (trimmed.contains('/') || trimmed.ends_with(".ron"))
        {
            state = "symlink-stub".into();
            detail.clear();
        }
        let mut record = record.as_object().cloned().unwrap_or_default();
        record.insert("status".into(), Value::String(state));
        record.insert("detail".into(), Value::String(detail));
        results.push(Value::Object(record));
    }
    let mut out = String::new();
    for record in &results {
        out.push_str(&serde_json::to_string(record).map_err(|e| e.to_string())?);
        out.push('\n');
    }
    fs::write(work.join("results.jsonl"), out)
        .map_err(|e| format!("{}: {e}", work.join("results.jsonl").display()))?;

    println!(
        "{} files collected, {} duplicates, {} unique",
        collected,
        duplicates,
        results.len()
    );
    let mut sources = std::collections::HashSet::new();
    for record in &results {
        if let Some(source) = record
            .get("repo")
            .or_else(|| record.get("crate"))
            .and_then(Value::as_str)
        {
            sources.insert(source.to_owned());
        }
    }
    println!("{} distinct repositories/crates", sources.len());
    let mut hosts: BTreeMap<String, usize> = BTreeMap::new();
    let mut table: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    for record in &results {
        let host = record
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let state = record
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        *hosts.entry(host.clone()).or_default() += 1;
        *table.entry(state).or_default().entry(host).or_default() += 1;
    }
    println!("by host: {hosts:?}");
    let mut rows: Vec<(String, BTreeMap<String, usize>, usize)> = table
        .into_iter()
        .map(|(state, counts)| {
            let total = counts.values().sum();
            (state, counts, total)
        })
        .collect();
    rows.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    for (state, counts, total) in rows {
        println!("  {state:16} {total:6}  {counts:?}");
    }
    Ok(())
}

fn relative(work: &Path, path: &Path) -> String {
    path.strip_prefix(work)
        .unwrap_or(path)
        .to_string_lossy()
        .into_owned()
}

fn build_harness(harness_dir: &Path) -> Result<(), String> {
    let status = Command::new("cargo")
        .args(["build", "--release", "--manifest-path"])
        .arg(harness_dir.join("Cargo.toml"))
        .status()
        .map_err(|e| format!("cargo: {e}"))?;
    if !status.success() {
        return Err("building tools/wild-corpus/harness failed".into());
    }
    Ok(())
}

fn run_harness(harness: &Path, input: &str) -> Result<String, String> {
    let mut child = Command::new(harness)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|e| format!("{}: {e}", harness.display()))?;
    let mut stdin = child.stdin.take().expect("piped stdin");
    let input = input.to_owned();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
    });
    let mut stdout = Vec::new();
    if let Some(mut pipe) = child.stdout.take() {
        pipe.read_to_end(&mut stdout).map_err(|e| e.to_string())?;
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let _ = writer.join();
    if !status.success() {
        return Err("wild harness failed".into());
    }
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}
