//! Copy the redistributable, valid subset of the collected files into
//! fmtron's `test_data/wild/` and write `test_data/wild/manifest.ron` with full
//! attribution (host, repository/crate, commit or version, original path,
//! source URL, license, sha256).
//!
//! Kept: unique files that the `ron` crate or fmtron accepts, from sources
//! whose declared license is a recognized open-source license.

use crate::util::{now_date, read_jsonl, repo_root, ron_string, work_dir};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;
use std::process::Command;

const MAX_PER_SOURCE: usize = 8;
const MAX_FILE: usize = 32 * 1024;

const OPEN: &[&str] = &[
    "mit",
    "apache-2.0",
    "bsd-2-clause",
    "bsd-3-clause",
    "0bsd",
    "isc",
    "zlib",
    "mpl-2.0",
    "unlicense",
    "cc0-1.0",
    "cc-by-4.0",
    "cc-by-sa-4.0",
    "gpl-2.0",
    "gpl-3.0",
    "lgpl-2.1",
    "lgpl-3.0",
    "agpl-3.0",
    "bsl-1.0",
    "wtfpl",
    "epl-2.0",
    "ofl-1.1",
    "artistic-2.0",
    "eupl-1.2",
    "gpl-3.0-or-later",
    "gpl-2.0-or-later",
    "lgpl-3.0-or-later",
    "agpl-3.0-or-later",
    "gpl-3.0-only",
    "gpl-2.0-only",
    "lgpl-2.1-or-later",
    "mit-0",
    "apache-2.0 with llvm-exception",
];

const VALID: &[&str] = &[
    "ok",
    "content-changed",
    "not-idempotent",
    "semantic",
    "output-invalid",
    "rejects-valid",
    "accepts-invalid",
    "panic",
];

pub fn run() -> Result<(), String> {
    let root = repo_root();
    let work = work_dir();
    let destination = root.join("test_data").join("wild");

    let records = {
        let path = work.join("results.jsonl");
        if !path.is_file() {
            return Err(format!(
                "{}: missing; run `cargo xtask wild evaluate` first",
                path.display()
            ));
        }
        read_jsonl(&path)?
    };
    let mut kept: Vec<(String, Value)> = Vec::new();
    let mut per_source: HashMap<String, usize> = HashMap::new();
    let mut skipped: BTreeMap<&str, usize> = BTreeMap::new();

    for record in records {
        let source = record
            .get("repo")
            .and_then(Value::as_str)
            .filter(|repo| !repo.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                format!(
                    "{}-{}",
                    record.get("crate").and_then(Value::as_str).unwrap_or(""),
                    record.get("version").and_then(Value::as_str).unwrap_or("")
                )
            });
        let local = record.get("local").and_then(Value::as_str).unwrap_or("");
        let bytes = fs::read(work.join(local)).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        let status = record.get("status").and_then(Value::as_str).unwrap_or("");
        let detail = record.get("detail").and_then(Value::as_str).unwrap_or("");
        let reason = if !VALID.contains(&status) {
            Some("invalid or not UTF-8")
        } else if status == "accepts-invalid" && !detail.contains("No RON extension") {
            Some("invalid (fmtron lenient)")
        } else if bytes.len() > MAX_FILE {
            Some("over 32 KiB")
        } else if is_symlink_stub(&text) {
            Some("symlink stub")
        } else if !open_license(record.get("license").and_then(Value::as_str)) {
            Some("no open license")
        } else if per_source.get(&source).copied().unwrap_or(0) >= MAX_PER_SOURCE {
            Some("per-source cap")
        } else {
            None
        };
        if let Some(reason) = reason {
            *skipped.entry(reason).or_default() += 1;
        } else {
            *per_source.entry(source.clone()).or_default() += 1;
            kept.push((source, record));
        }
    }

    if destination.is_dir() {
        fs::remove_dir_all(&destination).map_err(|e| format!("{}: {e}", destination.display()))?;
    }
    let mut by_source: BTreeMap<(String, String), Vec<(String, Value)>> = BTreeMap::new();
    for (source, record) in kept {
        let host = record
            .get("host")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned();
        let slug = source.replace('/', "__");
        let path = record.get("path").and_then(Value::as_str).unwrap_or("");
        let local = format!("{host}/{slug}/{path}");
        let destination_file = destination.join(&local);
        if let Some(parent) = destination_file.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::copy(work.join(record_local(&record)), &destination_file)
            .map_err(|e| format!("{}: {e}", destination_file.display()))?;
        by_source
            .entry((host, source))
            .or_default()
            .push((local, record));
    }

    let total_files: usize = by_source.values().map(Vec::len).sum();
    let mut lines = vec![
        "(".to_owned(),
        "    description: \"Real-world RON files collected from public forges and crates.io to exercise fmtron on RON as people actually write it. Every file keeps its original content and is listed with its source for attribution.\",".to_owned(),
        format!("    generated_on: \"{}\",", now_date()),
        format!("    total_files: {total_files},"),
        format!("    total_sources: {},", by_source.len()),
        format!(
            "    selection: \"Unique by sha256; accepted by the ron crate (0.12) or fmtron; source declares an open-source license; at most {MAX_PER_SOURCE} files per source, each at most 32 KiB. Collected via GitHub code search, the GitLab.com and Codeberg APIs, and .crate archives of crates.io reverse dependencies of ron.\","
        ),
        "    layout: \"Each file is stored at <host>/<source with / replaced by __>/<original_path>.\",".to_owned(),
        "    sources: [".to_owned(),
    ];
    for ((host, source), files) in &by_source {
        let first = &files[0].1;
        lines.push("        (".into());
        lines.push(format!("            host: {},", ron_string(host)));
        lines.push(format!("            source: {},", ron_string(source)));
        if host == "crates.io" {
            lines.push(format!(
                "            crate: {},",
                ron_string(first.get("crate").and_then(Value::as_str).unwrap_or(""))
            ));
            lines.push(format!(
                "            version: {},",
                ron_string(first.get("version").and_then(Value::as_str).unwrap_or(""))
            ));
            lines.push(format!(
                "            url: {},",
                ron_string(&format!(
                    "https://crates.io/crates/{}",
                    first.get("crate").and_then(Value::as_str).unwrap_or("")
                ))
            ));
            if let Some(repository) = first.get("repository").and_then(Value::as_str) {
                lines.push(format!(
                    "            repository: {},",
                    ron_string(repository)
                ));
            }
        } else {
            lines.push(format!(
                "            url: {},",
                ron_string(&format!("https://{host}/{source}"))
            ));
            lines.push(format!(
                "            commit: {},",
                ron_string(first.get("ref").and_then(Value::as_str).unwrap_or(""))
            ));
        }
        lines.push(format!(
            "            license: {},",
            ron_value(first.get("license"))
        ));
        lines.push("            files: [".into());
        for (_, record) in files {
            let url = ["html_url", "source_url"]
                .iter()
                .find_map(|key| record.get(*key).filter(|value| value.as_str() != Some("")));
            lines.push(format!(
                "                (original_path: {}, source_url: {}),",
                ron_string(record.get("path").and_then(Value::as_str).unwrap_or("")),
                ron_value(url)
            ));
        }
        lines.push("            ],".into());
        lines.push("        ),".into());
    }
    lines.push("    ],".into());
    lines.push(")".into());
    lines.push(String::new());

    let manifest = destination.join("manifest.ron");
    fs::write(&manifest, lines.join("\n")).map_err(|e| format!("{}: {e}", manifest.display()))?;
    // Dogfood: keep the manifest in fmtron's own canonical format.
    let status = Command::new(root.join("target").join("release").join("fmtron"))
        .args(["-w", "100"])
        .arg(&manifest)
        .status()
        .map_err(|e| format!("fmtron: {e}"))?;
    if !status.success() {
        return Err("formatting manifest.ron failed".into());
    }

    let mut size = 0_u64;
    collect_size(&destination, &mut size);
    let mut by_host: BTreeMap<String, usize> = BTreeMap::new();
    for ((host, _), files) in &by_source {
        *by_host.entry(host.clone()).or_default() += files.len();
    }
    println!(
        "kept {total_files} files from {} sources ({:.1} MB); skipped: {skipped:?}",
        by_source.len(),
        size as f64 / 1e6
    );
    println!("by host: {by_host:?}");
    Ok(())
}

fn record_local(record: &Value) -> &str {
    record.get("local").and_then(Value::as_str).unwrap_or("")
}

fn open_license(license: Option<&str>) -> bool {
    let Some(license) = license else {
        return false;
    };
    if ["NONE", "NOASSERTION", "UNKNOWN", "OTHER"]
        .iter()
        .any(|other| license.eq_ignore_ascii_case(other))
    {
        return false;
    }
    split_license(license)
        .iter()
        .any(|part| OPEN.contains(&part.trim_matches(['(', ')', ' ']).to_lowercase().as_str()))
}

/// Splits an SPDX-ish expression on `/`, `,`, and whitespace-delimited
/// `OR`/`AND`, like the old Python regex did.
fn split_license(license: &str) -> Vec<String> {
    let mut parts = Vec::new();
    for chunk in license.trim_matches(['(', ')', ' ']).split(['/', ',']) {
        let mut current = String::new();
        for word in chunk.split_whitespace() {
            if matches!(word, "OR" | "AND" | "or" | "and") {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
            } else {
                if !current.is_empty() {
                    current.push(' ');
                }
                current.push_str(word);
            }
        }
        if !current.is_empty() {
            parts.push(current);
        }
    }
    parts
}

fn is_symlink_stub(text: &str) -> bool {
    let text = text.trim();
    !text.contains('\n') && !text.contains(' ') && (text.contains('/') || text.ends_with(".ron"))
}

fn ron_value(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => ron_string(text),
        Some(other) => other.to_string(),
        None => "null".to_owned(),
    }
}

fn collect_size(dir: &Path, total: &mut u64) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_size(&path, total);
        } else if let Ok(metadata) = entry.metadata() {
            *total += metadata.len();
        }
    }
}
