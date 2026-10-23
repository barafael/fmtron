//! Collect .ron file references from GitHub code search, sliced by file size so
//! each query stays under the 1,000-result cap. Writes one JSON line per file
//! to `gh_index.jsonl`: repo, path, blob sha, commit ref, html_url.
//!
//! Code search allows 10 requests/minute, so requests are spaced 6.5 s apart.

use crate::util::{append_jsonl, ensure_dir, get_array, get_str, query_ref, read_jsonl, work_dir};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::Write;
use std::process::Command;
use std::time::{Duration, Instant};

const MAX_PER_REPO: usize = 25;

pub fn run(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err("usage: wild gh-collect coarse <pages> <bounds...> | <lo> <hi> <pages>".into());
    }
    let work = work_dir();
    ensure_dir(&work)?;
    let out_path = work.join("gh_index.jsonl");

    let mut seen = HashSet::new();
    let mut per_repo: HashMap<String, usize> = HashMap::new();
    for record in read_jsonl(&out_path)? {
        let repo = get_str(&record, "repo").unwrap_or("").to_owned();
        let path = get_str(&record, "path").unwrap_or("").to_owned();
        seen.insert((repo.clone(), path));
        *per_repo.entry(repo).or_default() += 1;
    }

    let mut client = GhSearch::default();
    let (slices, pages): (Vec<(usize, usize, Value)>, usize) = if args[0] == "coarse" {
        let pages: usize = args
            .get(1)
            .ok_or("gh-collect coarse: missing page count")?
            .parse()
            .map_err(|_| "gh-collect coarse: pages must be a number")?;
        let bounds = args[2..]
            .iter()
            .map(|arg| arg.parse::<usize>())
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "gh-collect coarse: bounds must be numbers")?;
        (client.coarse(&bounds), pages)
    } else {
        let lo = args[0]
            .parse()
            .map_err(|_| "gh-collect: lo must be a number")?;
        let hi = args
            .get(1)
            .ok_or("gh-collect: missing hi")?
            .parse()
            .map_err(|_| "gh-collect: hi must be a number")?;
        let pages: usize = args
            .get(2)
            .ok_or("gh-collect: missing page count")?
            .parse()
            .map_err(|_| "gh-collect: pages must be a number")?;
        (client.slice_sizes(lo, hi), pages)
    };

    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out_path)
        .map_err(|e| format!("{}: {e}", out_path.display()))?;
    for (a, b, first) in slices {
        let mut added = record(
            &mut out,
            get_array(&first, "items"),
            &mut seen,
            &mut per_repo,
        )?;
        let total = first
            .get("total_count")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        for page in 2..=pages {
            if (page - 1) * 100 >= total.min(1000) as usize {
                break;
            }
            let Some(result) = client.search(&format!("extension:ron size:{a}..{b}"), page) else {
                break;
            };
            let items = get_array(&result, "items");
            if items.is_empty() {
                break;
            }
            added += record(&mut out, items, &mut seen, &mut per_repo)?;
        }
        eprintln!(
            "  {a}..{b}: +{added} (repos {}, files {})",
            per_repo.len(),
            seen.len()
        );
    }
    Ok(())
}

fn record(
    out: &mut impl Write,
    items: &[Value],
    seen: &mut HashSet<(String, String)>,
    per_repo: &mut HashMap<String, usize>,
) -> Result<usize, String> {
    let mut added = 0;
    for item in items {
        let repo = item
            .get("repository")
            .and_then(|repository| get_str(repository, "full_name"))
            .unwrap_or("");
        let path = get_str(item, "path").unwrap_or("");
        if repo.is_empty()
            || seen.contains(&(repo.to_owned(), path.to_owned()))
            || per_repo.get(repo).copied().unwrap_or(0) >= MAX_PER_REPO
        {
            continue;
        }
        append_jsonl(
            out,
            &json!({
                "host": "github.com",
                "repo": repo,
                "path": path,
                "sha": item.get("sha").cloned().unwrap_or(Value::Null),
                "ref": query_ref(get_str(item, "url").unwrap_or("")),
                "html_url": item.get("html_url").cloned().unwrap_or(Value::Null),
            }),
        )?;
        seen.insert((repo.to_owned(), path.to_owned()));
        *per_repo.entry(repo.to_owned()).or_default() += 1;
        added += 1;
    }
    Ok(added)
}

#[derive(Default)]
struct GhSearch {
    last: Option<Instant>,
}

impl GhSearch {
    fn search(&mut self, query: &str, page: usize) -> Option<Value> {
        let wait = Duration::from_millis(6500);
        if let Some(last) = self.last {
            let elapsed = last.elapsed();
            if elapsed < wait {
                std::thread::sleep(wait - elapsed);
            }
        }
        self.last = Some(Instant::now());

        for _ in 0..5 {
            let output = Command::new("gh")
                .args([
                    "api",
                    "-X",
                    "GET",
                    "search/code",
                    "-f",
                    &format!("q={query}"),
                    "-f",
                    "per_page=100",
                    "-f",
                    &format!("page={page}"),
                ])
                .output()
                .ok()?;
            if output.status.success() {
                return serde_json::from_slice(&output.stdout).ok();
            }
            let stderr = String::from_utf8_lossy(&output.stderr).to_lowercase();
            if stderr.contains("rate limit")
                || stderr.contains("403")
                || stderr.contains("secondary")
            {
                std::thread::sleep(Duration::from_secs(60));
                continue;
            }
            eprintln!(
                "ERR {query} {page} {}",
                String::from_utf8_lossy(&output.stderr)
                    .chars()
                    .take(200)
                    .collect::<String>()
            );
            return None;
        }
        None
    }

    /// Byte ranges whose result count is at most 1,000.
    fn slice_sizes(&mut self, lo: usize, hi: usize) -> Vec<(usize, usize, Value)> {
        let mut slices = Vec::new();
        let mut stack = vec![(lo, hi)];
        while let Some((a, b)) = stack.pop() {
            let Some(result) = self.search(&format!("extension:ron size:{a}..{b}"), 1) else {
                continue;
            };
            let total = result
                .get("total_count")
                .and_then(Value::as_u64)
                .unwrap_or(0);
            eprintln!("slice {a}..{b}: {total}");
            if total > 1000 && b > a {
                let middle = (a + b) / 2;
                stack.push((middle + 1, b));
                stack.push((a, middle));
            } else {
                slices.push((a, b, result));
            }
        }
        slices
    }

    /// Fixed slices without bisection: sample each size range evenly.
    fn coarse(&mut self, bounds: &[usize]) -> Vec<(usize, usize, Value)> {
        let mut slices = Vec::new();
        for pair in bounds.windows(2) {
            let (a, b) = (pair[0], pair[1].saturating_sub(1));
            if let Some(result) = self.search(&format!("extension:ron size:{a}..{b}"), 1) {
                let total = result
                    .get("total_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0);
                eprintln!("slice {a}..{b}: {total}");
                slices.push((a, b, result));
            }
        }
        slices
    }
}
