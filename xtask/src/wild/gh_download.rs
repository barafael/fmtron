//! Download the files listed in `gh_index.jsonl` at their pinned commit and
//! look up each repository's license. Re-runnable: skips files already fetched.
//! Writes `raw/github.com/<owner>__<repo>/<path>` and `gh_files.jsonl`.

use crate::util::{agent, append_jsonl, ensure_dir, get_str, quote, read_jsonl, work_dir};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::path::Path;
use std::process::Command;

const MAX_FILE: usize = 512 * 1024;

pub fn run() -> Result<(), String> {
    let work = work_dir();
    ensure_dir(&work)?;
    let out_path = work.join("gh_files.jsonl");
    let license_path = work.join("gh_licenses.json");

    let mut licenses: Map<String, Value> = if license_path.exists() {
        let body =
            fs::read(&license_path).map_err(|e| format!("{}: {e}", license_path.display()))?;
        match serde_json::from_slice::<Value>(&body)
            .map_err(|e| format!("{}: {e}", license_path.display()))?
        {
            Value::Object(map) => map,
            _ => Map::new(),
        }
    } else {
        Map::new()
    };

    let done: HashSet<(String, String)> = read_jsonl(&out_path)?
        .iter()
        .filter_map(|record| {
            Some((
                get_str(record, "repo")?.to_owned(),
                get_str(record, "path")?.to_owned(),
            ))
        })
        .collect();
    let todo: Vec<Value> = read_jsonl(&work.join("gh_index.jsonl"))?
        .into_iter()
        .filter(|record| {
            let key = (
                get_str(record, "repo").unwrap_or("").to_owned(),
                get_str(record, "path").unwrap_or("").to_owned(),
            );
            !done.contains(&key)
        })
        .collect();

    let mut missing: Vec<String> = todo
        .iter()
        .filter_map(|record| get_str(record, "repo").map(str::to_owned))
        .filter(|repo| !licenses.contains_key(repo))
        .collect();
    missing.sort();
    missing.dedup();
    for repo in missing {
        let output = Command::new("gh")
            .args([
                "api",
                &format!("repos/{repo}"),
                "--jq",
                ".license.spdx_id // \"NONE\"",
            ])
            .output();
        let license = match output {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_owned()
            }
            _ => "UNKNOWN".to_owned(),
        };
        licenses.insert(repo, Value::String(license));
    }
    let text = serde_json::to_string_pretty(&Value::Object(licenses.clone()))
        .map_err(|e| e.to_string())?;
    fs::write(&license_path, text).map_err(|e| format!("{}: {e}", license_path.display()))?;

    let agent = agent();
    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out_path)
        .map_err(|e| format!("{}: {e}", out_path.display()))?;

    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .min(8)
        .min(todo.len().max(1));
    let chunk = todo.len().div_ceil(workers).max(1);
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for slice in todo.chunks(chunk) {
            let agent = agent.clone();
            let work = work.clone();
            let licenses = licenses.clone();
            handles.push(scope.spawn(move || {
                slice
                    .iter()
                    .map(|record| fetch(&agent, &work, &licenses, record))
                    .collect::<Vec<_>>()
            }));
        }
        for handle in handles {
            let rows = handle.join().expect("download worker panicked");
            for (record, error) in rows {
                if let Some(error) = error {
                    eprintln!("{error}");
                }
                if let Some(record) = record {
                    append_jsonl(&mut out, &record).expect("write gh_files.jsonl");
                }
            }
        }
    });
    println!(
        "downloaded {} new; repos with license info: {}",
        todo.len(),
        licenses.len()
    );
    Ok(())
}

fn fetch(
    agent: &ureq::Agent,
    work: &Path,
    licenses: &Map<String, Value>,
    record: &Value,
) -> (Option<Value>, Option<String>) {
    let repo = get_str(record, "repo").unwrap_or("");
    let path = get_str(record, "path").unwrap_or("");
    let reference = get_str(record, "ref").unwrap_or("");
    let url = format!(
        "https://raw.githubusercontent.com/{repo}/{reference}/{}",
        quote(path, true)
    );
    let data = match agent.get(&url).call() {
        Ok(mut response) => match response.body_mut().read_to_vec() {
            Ok(data) => data,
            Err(error) => return (None, Some(format!("{url}: {error}"))),
        },
        Err(error) => return (None, Some(format!("{url}: {error}"))),
    };
    if data.len() > MAX_FILE {
        return (None, None);
    }
    let dest = work
        .join("raw")
        .join("github.com")
        .join(repo.replace('/', "__"))
        .join(path);
    if let Some(parent) = dest.parent()
        && let Err(error) = fs::create_dir_all(parent)
    {
        return (None, Some(format!("{}: {error}", parent.display())));
    }
    if let Err(error) = fs::write(&dest, &data) {
        return (None, Some(format!("{}: {error}", dest.display())));
    }
    let local = dest
        .strip_prefix(work)
        .unwrap_or(&dest)
        .to_string_lossy()
        .into_owned();
    let mut merged = record.as_object().cloned().unwrap_or_default();
    merged.insert(
        "license".into(),
        licenses.get(repo).cloned().unwrap_or(Value::Null),
    );
    merged.insert("source_url".into(), Value::String(url));
    merged.insert("local".into(), Value::String(local));
    (Some(Value::Object(merged)), None)
}
