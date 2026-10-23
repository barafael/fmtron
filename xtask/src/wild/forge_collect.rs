//! Collect .ron files from GitLab.com and Codeberg (Forgejo) via their public
//! APIs: search projects by keyword, list each repository tree, download up to
//! MAX_PER_REPO .ron files. Writes files under `raw/<host>/<owner>__<repo>/`
//! and one JSON line per file to `forge_index.jsonl`.

use crate::util::{
    append_jsonl, ensure_dir, get_array, get_retry, get_str, quote, read_jsonl, work_dir,
};
use serde_json::{Map, Value};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::time::Duration;

const MAX_PER_REPO: usize = 25;
const MAX_FILE: usize = 512 * 1024;
const KEYWORDS: &[&str] = &[
    "bevy",
    "ron",
    "game",
    "engine",
    "config",
    "rust",
    "veloren",
    "amethyst",
    "roguelike",
    "gamedev",
    "ecs",
    "serde",
    "egui",
    "wgpu",
    "cosmic",
    "voxel",
    "tui",
    "editor",
    "plugin",
    "simulation",
];

pub fn run(args: &[String]) -> Result<(), String> {
    let host = match args.first().map(String::as_str) {
        Some("gitlab") => "gitlab.com",
        Some("codeberg") => "codeberg.org",
        _ => return Err("usage: wild forge-collect <gitlab|codeberg>".into()),
    };
    let work = work_dir();
    ensure_dir(&work)?;
    let out_path = work.join("forge_index.jsonl");
    let done: HashSet<(String, String)> = read_jsonl(&out_path)?
        .iter()
        .filter_map(|record| {
            Some((
                get_str(record, "host")?.to_owned(),
                get_str(record, "repo")?.to_owned(),
            ))
        })
        .collect();
    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out_path)
        .map_err(|e| format!("{}: {e}", out_path.display()))?;
    let agent = crate::util::agent();

    match host {
        "gitlab.com" => gitlab(&agent, &work, &done, &mut out),
        _ => codeberg(&agent, &work, &done, &mut out),
    }
    Ok(())
}

fn save(
    work: &Path,
    out: &mut impl Write,
    host: &str,
    repo: &str,
    path: &str,
    data: &[u8],
    meta: Map<String, Value>,
) -> Result<(), String> {
    let dest = work
        .join("raw")
        .join(host)
        .join(repo.replace('/', "__"))
        .join(path);
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    fs::write(&dest, data).map_err(|e| format!("{}: {e}", dest.display()))?;
    let local = dest
        .strip_prefix(work)
        .unwrap_or(&dest)
        .to_string_lossy()
        .into_owned();
    let mut record = Map::new();
    record.insert("host".into(), Value::String(host.into()));
    record.insert("repo".into(), Value::String(repo.into()));
    record.insert("path".into(), Value::String(path.into()));
    record.insert("local".into(), Value::String(local));
    record.extend(meta);
    append_jsonl(out, &Value::Object(record))
}

fn get_json(
    agent: &ureq::Agent,
    url: &str,
    retry: &[u16],
    wait: Duration,
) -> Result<Value, String> {
    let body = get_retry(agent, url, retry, wait, 4)?;
    serde_json::from_slice(&body).map_err(|e| format!("{url}: {e}"))
}

fn gitlab(
    agent: &ureq::Agent,
    work: &Path,
    done: &HashSet<(String, String)>,
    out: &mut impl Write,
) {
    const G: &str = "https://gitlab.com/api/v4";
    let mut seen = HashSet::new();
    for keyword in KEYWORDS {
        for page in 1..=3 {
            let url = format!(
                "{G}/projects?search={keyword}&with_programming_language=Rust&per_page=100&page={page}&order_by=star_count"
            );
            let projects = match get_json(agent, &url, &[429], Duration::from_secs(30)) {
                Ok(Value::Array(projects)) => projects,
                Ok(_) => break,
                Err(error) => {
                    eprintln!("gitlab search {keyword} {error}");
                    break;
                }
            };
            if projects.is_empty() {
                break;
            }
            for project in projects {
                let repo = get_str(&project, "path_with_namespace").unwrap_or("");
                let id = project.get("id").cloned().unwrap_or(Value::Null);
                let branch = get_str(&project, "default_branch").unwrap_or("");
                if repo.is_empty()
                    || seen.contains(repo)
                    || done.contains(&("gitlab.com".to_owned(), repo.to_owned()))
                    || branch.is_empty()
                {
                    continue;
                }
                seen.insert(repo.to_owned());
                if let Err(error) = gitlab_repo(agent, work, out, repo, &id, branch) {
                    eprintln!("gitlab repo {repo} {error}");
                }
            }
        }
    }
}

fn gitlab_repo(
    agent: &ureq::Agent,
    work: &Path,
    out: &mut impl Write,
    repo: &str,
    id: &Value,
    branch: &str,
) -> Result<(), String> {
    const G: &str = "https://gitlab.com/api/v4";
    let mut ron_files = Vec::new();
    for page in 1..30 {
        let url =
            format!("{G}/projects/{id}/repository/tree?recursive=true&per_page=100&page={page}");
        let tree = get_json(agent, &url, &[429], Duration::from_secs(30))?;
        let entries = tree.as_array().map(Vec::as_slice).unwrap_or(&[]);
        ron_files.extend(entries.iter().filter_map(|entry| {
            let kind = get_str(entry, "type").unwrap_or("");
            let path = get_str(entry, "path").unwrap_or("");
            (kind == "blob" && path.ends_with(".ron")).then(|| path.to_owned())
        }));
        if entries.len() < 100 {
            break;
        }
    }
    if ron_files.is_empty() {
        return Ok(());
    }
    let info = get_json(
        agent,
        &format!("{G}/projects/{id}?license=true"),
        &[429],
        Duration::from_secs(30),
    )?;
    let license = info
        .get("license")
        .and_then(|license| license.get("key"))
        .cloned()
        .unwrap_or(Value::Null);
    let commit = get_json(
        agent,
        &format!(
            "{G}/projects/{id}/repository/commits/{}",
            quote(branch, false)
        ),
        &[429],
        Duration::from_secs(30),
    )?
    .get("id")
    .and_then(Value::as_str)
    .unwrap_or("")
    .to_owned();

    for path in ron_files.iter().take(MAX_PER_REPO) {
        let encoded = quote(path, false);
        let data = get_retry(
            agent,
            &format!("{G}/projects/{id}/repository/files/{encoded}/raw?ref={commit}"),
            &[429],
            Duration::from_secs(30),
            4,
        )?;
        if data.len() > MAX_FILE {
            continue;
        }
        let mut meta = Map::new();
        meta.insert("ref".into(), Value::String(commit.clone()));
        meta.insert("license".into(), license.clone());
        meta.insert(
            "html_url".into(),
            Value::String(format!("https://gitlab.com/{repo}/-/blob/{commit}/{path}")),
        );
        save(work, out, "gitlab.com", repo, path, &data, meta)?;
    }
    eprintln!(
        "gitlab {repo}: {}/{} .ron ({})",
        ron_files.len().min(MAX_PER_REPO),
        ron_files.len(),
        license.as_str().unwrap_or("None")
    );
    Ok(())
}

fn codeberg(
    agent: &ureq::Agent,
    work: &Path,
    done: &HashSet<(String, String)>,
    out: &mut impl Write,
) {
    const C: &str = "https://codeberg.org/api/v1";
    let mut seen = HashSet::new();
    for keyword in KEYWORDS {
        for page in 1..=5 {
            let url = format!("{C}/repos/search?q={keyword}&limit=50&page={page}");
            let result = match get_json(agent, &url, &[429, 502, 503], Duration::from_secs(20)) {
                Ok(result) => result,
                Err(error) => {
                    eprintln!("codeberg search {keyword} {error}");
                    break;
                }
            };
            let data = get_array(&result, "data");
            if data.is_empty() {
                break;
            }
            for repository in data {
                let repo = get_str(repository, "full_name").unwrap_or("");
                let empty = repository
                    .get("empty")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                if repo.is_empty()
                    || seen.contains(repo)
                    || done.contains(&("codeberg.org".to_owned(), repo.to_owned()))
                    || empty
                {
                    continue;
                }
                seen.insert(repo.to_owned());
                if let Err(error) = codeberg_repo(agent, work, out, repo, repository) {
                    eprintln!("codeberg repo {repo} {error}");
                }
            }
        }
    }
}

fn codeberg_repo(
    agent: &ureq::Agent,
    work: &Path,
    out: &mut impl Write,
    repo: &str,
    repository: &Value,
) -> Result<(), String> {
    const C: &str = "https://codeberg.org/api/v1";
    let branch = get_str(repository, "default_branch").unwrap_or("");
    let commit = get_json(
        agent,
        &format!("{C}/repos/{repo}/branches/{}", quote(branch, false)),
        &[429, 502, 503],
        Duration::from_secs(20),
    )?
    .get("commit")
    .and_then(|commit| commit.get("id"))
    .and_then(Value::as_str)
    .unwrap_or("")
    .to_owned();
    let tree = get_json(
        agent,
        &format!("{C}/repos/{repo}/git/trees/{commit}?recursive=true&per_page=10000"),
        &[429, 502, 503],
        Duration::from_secs(20),
    )?;
    let ron_files: Vec<String> = get_array(&tree, "tree")
        .iter()
        .filter_map(|entry| {
            let kind = get_str(entry, "type").unwrap_or("");
            let path = get_str(entry, "path").unwrap_or("");
            (kind == "blob" && path.ends_with(".ron")).then(|| path.to_owned())
        })
        .collect();
    if ron_files.is_empty() {
        return Ok(());
    }
    let licenses: Vec<String> = get_array(repository, "licenses")
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    let license = if licenses.is_empty() {
        Value::Null
    } else {
        Value::String(licenses.join(","))
    };

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
                eprintln!("codeberg file {repo}/{path}: {error}");
                continue;
            }
        };
        if data.len() > MAX_FILE {
            continue;
        }
        let mut meta = Map::new();
        meta.insert("ref".into(), Value::String(commit.clone()));
        meta.insert("license".into(), license.clone());
        meta.insert(
            "html_url".into(),
            Value::String(format!(
                "https://codeberg.org/{repo}/src/commit/{commit}/{encoded}"
            )),
        );
        save(work, out, "codeberg.org", repo, path, &data, meta)?;
    }
    eprintln!(
        "codeberg {repo}: {}/{} .ron ({})",
        ron_files.len().min(MAX_PER_REPO),
        ron_files.len(),
        license.as_str().unwrap_or("None")
    );
    Ok(())
}
