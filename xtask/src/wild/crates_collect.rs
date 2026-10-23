//! Collect .ron files from published crates that depend on `ron`.
//!
//! For each reverse dependency (latest version that depends on ron), download
//! the .crate archive from static.crates.io and keep the .ron files in it.
//! Writes files under `raw/crates.io/<crate>-<version>/` and one JSON line per
//! file to `crates_index.jsonl` (crate, version, license, repository, path).
//! crates.io asks for <= 1 API request/second and a descriptive User-Agent.

use crate::util::{
    agent, append_jsonl, ensure_dir, get_array, get_bytes, get_str, read_jsonl, work_dir,
};
use serde_json::{Map, Value};
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::time::Duration;

const MAX_FILE: u64 = 512 * 1024;
const MAX_PER_CRATE: usize = 25;

pub fn run(args: &[String]) -> Result<(), String> {
    let pages: usize = match args.first() {
        Some(arg) => arg
            .parse()
            .map_err(|_| "crates-collect: pages must be a number")?,
        None => 50,
    };
    let work = work_dir();
    ensure_dir(&work)?;
    let out_path = work.join("crates_index.jsonl");
    let raw = work.join("raw").join("crates.io");

    let done: HashSet<String> = read_jsonl(&out_path)?
        .iter()
        .filter_map(|record| get_str(record, "crate").map(str::to_owned))
        .collect();

    let agent = agent();
    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out_path)
        .map_err(|e| format!("{}: {e}", out_path.display()))?;

    for page in 1..=pages {
        std::thread::sleep(Duration::from_secs(1));
        let url = format!(
            "https://crates.io/api/v1/crates/ron/reverse_dependencies?per_page=100&page={page}"
        );
        let body = match get_bytes(&agent, &url) {
            Ok(body) => body,
            Err(error) => {
                eprintln!("crates.io page {page}: {error}");
                break;
            }
        };
        let data: Value = match serde_json::from_slice(&body) {
            Ok(data) => data,
            Err(error) => {
                eprintln!("crates.io page {page}: {error}");
                break;
            }
        };
        let versions: HashMap<&str, &Value> = get_array(&data, "versions")
            .iter()
            .filter_map(|version| get_str(version, "id").map(|id| (id, version)))
            .collect();
        let dependencies = get_array(&data, "dependencies");
        if dependencies.is_empty() {
            break;
        }
        for dependency in dependencies {
            let Some(version) = get_str(dependency, "version_id").and_then(|id| versions.get(id))
            else {
                continue;
            };
            let name = get_str(version, "crate").unwrap_or("");
            let number = get_str(version, "num").unwrap_or("");
            if name.is_empty() || done.contains(name) {
                continue;
            }
            let blob = match get_bytes(
                &agent,
                &format!("https://static.crates.io/crates/{name}/{name}-{number}.crate"),
            ) {
                Ok(blob) => blob,
                Err(error) => {
                    eprintln!("download failed {name} {number} {error}");
                    continue;
                }
            };
            match extract(&blob, &work, &raw, name, number, &mut out, version) {
                Ok(kept) => println!("page {page} {name}-{number}: {kept} .ron"),
                Err(error) => eprintln!("extract failed {name} {error}"),
            }
        }
    }
    Ok(())
}

fn extract(
    blob: &[u8],
    work: &Path,
    raw: &Path,
    name: &str,
    number: &str,
    out: &mut impl Write,
    version: &Value,
) -> Result<usize, String> {
    let decoder = flate2::read::GzDecoder::new(blob);
    let mut archive = tar::Archive::new(decoder);
    let mut kept = 0;
    let entries = archive.entries().map_err(|e| e.to_string())?;
    for entry in entries {
        let mut entry = entry.map_err(|e| e.to_string())?;
        let is_file = entry.header().entry_type().is_file();
        let size = entry.header().size().unwrap_or(0);
        let path = entry.path().map_err(|e| e.to_string())?.into_owned();
        if !is_file || !path.to_string_lossy().ends_with(".ron") || size > MAX_FILE {
            continue;
        }
        if kept >= MAX_PER_CRATE {
            break;
        }
        let mut components = path.components();
        if components.next().is_none() {
            continue;
        }
        let rel: std::path::PathBuf = components.collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let dest = raw.join(format!("{name}-{number}")).join(&rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(|e| e.to_string())?;
        fs::write(&dest, &data).map_err(|e| format!("{}: {e}", dest.display()))?;
        let local = dest
            .strip_prefix(work)
            .unwrap_or(&dest)
            .to_string_lossy()
            .into_owned();
        let mut record = Map::new();
        record.insert("host".into(), Value::String("crates.io".into()));
        record.insert("crate".into(), Value::String(name.into()));
        record.insert("version".into(), Value::String(number.into()));
        record.insert(
            "license".into(),
            version.get("license").cloned().unwrap_or(Value::Null),
        );
        record.insert(
            "repository".into(),
            version.get("repository").cloned().unwrap_or(Value::Null),
        );
        record.insert(
            "path".into(),
            Value::String(rel.to_string_lossy().into_owned()),
        );
        record.insert("local".into(), Value::String(local));
        record.insert(
            "source_url".into(),
            Value::String(format!(
                "https://static.crates.io/crates/{name}/{name}-{number}.crate"
            )),
        );
        append_jsonl(out, &Value::Object(record))?;
        kept += 1;
    }
    Ok(kept)
}
