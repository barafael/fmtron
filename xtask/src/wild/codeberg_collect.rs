//! Broad Codeberg (Forgejo) collection for local evaluation.
//!
//! Searches many keywords, keeps repositories written in Rust or mentioning
//! RON, lists each tree at the default branch's head, and downloads up to
//! MAX_PER_REPO .ron files. Also records LICENSE-like files found in the tree,
//! since Codeberg's API does not report a license.
//! Writes `raw/codeberg.org/<owner>__<repo>/<path>` and `codeberg_index.jsonl`.

use crate::util::{
    append_jsonl, ensure_dir, get_array, get_retry, get_str, quote, read_jsonl, work_dir,
};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

const MAX_PER_REPO: usize = 40;
const MAX_FILE: usize = 512 * 1024;
const API: &str = "https://codeberg.org/api/v1";

/// Highest-yield keywords first; RON configs also live in dotfiles repos.
const KEYWORDS: &[&str] = &[
    "bevy",
    "ron",
    "dotfiles",
    "gitui",
    "cosmic",
    "game",
    "gamedev",
    "fyrox",
    "macroquad",
    "roguelike",
    "voxel",
    "config",
    "theme",
    "engine",
    "ecs",
    "serde",
    "egui",
    "wgpu",
    "settings",
    "level",
    "tilemap",
    "sprite",
    "shader",
    "editor",
    "plugin",
    "simulation",
    "tui",
    "cli",
    "tool",
    "nix",
    "hyprland",
    "sway",
    "niri",
    "wayland",
    "launcher",
    "server",
    "bot",
    "emulator",
    "audio",
    "music",
    "synth",
    "parser",
    "compiler",
    "iced",
    "slint",
    "leptos",
];

/// Keywords whose repositories are kept regardless of language.
const ANY_LANGUAGE: &[&str] = &[
    "ron", "dotfiles", "gitui", "cosmic", "config", "theme", "settings",
];

pub fn run() -> Result<(), String> {
    let work = work_dir();
    ensure_dir(&work)?;
    let out_path = work.join("codeberg_index.jsonl");
    let pages: usize = std::env::var("CODEBERG_PAGES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(4);

    let mut done = HashSet::new();
    for index in ["codeberg_index.jsonl", "forge_index.jsonl"] {
        for record in read_jsonl(&work.join(index))? {
            if get_str(&record, "host") == Some("codeberg.org")
                && let Some(repo) = get_str(&record, "repo")
            {
                done.insert(repo.to_owned());
            }
        }
    }
    let mut checked = done.clone();
    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out_path)
        .map_err(|e| format!("{}: {e}", out_path.display()))?;
    let agent = crate::util::agent();

    for keyword in KEYWORDS {
        for page in 1..=pages {
            let url = format!("{API}/repos/search?q={keyword}&limit=50&page={page}");
            let result = match get_retry(&agent, &url, &[429, 502, 503], Duration::from_secs(20), 4)
                .and_then(|body| serde_json::from_slice::<Value>(&body).map_err(|e| e.to_string()))
            {
                Ok(result) => result,
                Err(error) => {
                    eprintln!("search {keyword} {page} {error}");
                    break;
                }
            };
            let repos = get_array(&result, "data");
            if repos.is_empty() {
                break;
            }
            for repository in repos {
                let repo = get_str(repository, "full_name").unwrap_or("");
                let empty = repository
                    .get("empty")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if repo.is_empty() || checked.contains(repo) || empty {
                    continue;
                }
                let text = format!(
                    "{} {}",
                    get_str(repository, "name").unwrap_or(""),
                    get_str(repository, "description").unwrap_or("")
                )
                .to_lowercase();
                let is_rust = get_str(repository, "language") == Some("Rust");
                let mentions_ron = text.split_whitespace().any(|word| word == "ron");
                if !is_rust && !ANY_LANGUAGE.contains(keyword) && !mentions_ron {
                    continue;
                }
                checked.insert(repo.to_owned());
                if let Err(error) = collect_repo(&agent, &work, &mut out, repo, repository) {
                    eprintln!("repo {repo} {error}");
                }
            }
        }
        eprintln!("== keyword {keyword} done; repos checked {}", checked.len());
    }
    Ok(())
}

fn collect_repo(
    agent: &ureq::Agent,
    work: &Path,
    out: &mut impl Write,
    repo: &str,
    repository: &Value,
) -> Result<(), String> {
    let branch = quote(get_str(repository, "default_branch").unwrap_or(""), false);
    let branch_info = get_retry(
        agent,
        &format!("{API}/repos/{repo}/branches/{branch}"),
        &[429, 502, 503],
        Duration::from_secs(20),
        4,
    )?;
    let branch_info: Value = serde_json::from_slice(&branch_info).map_err(|e| e.to_string())?;
    let commit = branch_info
        .get("commit")
        .and_then(|commit| commit.get("id"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    let tree = get_retry(
        agent,
        &format!("{API}/repos/{repo}/git/trees/{commit}?recursive=true&per_page=10000"),
        &[429, 502, 503],
        Duration::from_secs(20),
        4,
    )?;
    let tree: Value = serde_json::from_slice(&tree).map_err(|e| e.to_string())?;
    let blobs: Vec<String> = get_array(&tree, "tree")
        .iter()
        .filter_map(|entry| {
            let kind = get_str(entry, "type").unwrap_or("");
            (kind == "blob").then(|| get_str(entry, "path").unwrap_or("").to_owned())
        })
        .collect();
    let ron_files: Vec<&String> = blobs.iter().filter(|path| path.ends_with(".ron")).collect();
    if ron_files.is_empty() {
        return Ok(());
    }
    let license_files: Vec<String> = blobs
        .iter()
        .filter(|path| {
            !path.contains('/')
                && ["LICENSE", "LICENCE", "COPYING"]
                    .iter()
                    .any(|prefix| path.to_uppercase().starts_with(prefix))
        })
        .cloned()
        .collect();

    for path in ron_files.iter().take(MAX_PER_REPO) {
        let encoded = quote(path, true);
        let data = match get_retry(
            agent,
            &format!("https://codeberg.org/{repo}/raw/commit/{commit}/{encoded}"),
            &[429, 502, 503],
            Duration::from_secs(20),
            4,
        ) {
            Ok(data) => data,
            Err(error) => {
                eprintln!("file {repo}/{path}: {error}");
                continue;
            }
        };
        if data.len() > MAX_FILE {
            continue;
        }
        let dest = work
            .join("raw")
            .join("codeberg.org")
            .join(repo.replace('/', "__"))
            .join(path);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        fs::write(&dest, &data).map_err(|e| format!("{}: {e}", dest.display()))?;
        let local = dest
            .strip_prefix(work)
            .unwrap_or(&dest)
            .to_string_lossy()
            .into_owned();
        let mut record = Map::new();
        record.insert("host".into(), Value::String("codeberg.org".into()));
        record.insert("repo".into(), Value::String(repo.into()));
        record.insert("path".into(), Value::String(path.to_string()));
        record.insert("ref".into(), Value::String(commit.clone()));
        record.insert("license".into(), Value::Null);
        record.insert("license_files".into(), json!(license_files));
        record.insert("local".into(), Value::String(local));
        record.insert(
            "html_url".into(),
            Value::String(format!(
                "https://codeberg.org/{repo}/src/commit/{commit}/{encoded}"
            )),
        );
        append_jsonl(out, &Value::Object(record))?;
    }
    eprintln!(
        "{repo}: {}/{} .ron, license files {:?}",
        ron_files.len().min(MAX_PER_REPO),
        ron_files.len(),
        license_files
    );
    Ok(())
}
