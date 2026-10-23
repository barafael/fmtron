//! Small helpers shared by the xtask subcommands.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// APIs such as crates.io ask for an identifying User-Agent.
pub const UA: &str = "fmtron-corpus-collector (https://github.com/barafael/fmtron)";

pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives in the repository")
        .to_path_buf()
}

/// Collected data goes to `$FMTRON_WILD_WORK`, default `./wild-work` relative
/// to the current directory. The path is made absolute like before.
pub fn work_dir() -> PathBuf {
    let dir = std::env::var_os("FMTRON_WILD_WORK")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("wild-work"));
    if dir.is_absolute() {
        dir
    } else {
        std::env::current_dir()
            .expect("current directory")
            .join(dir)
    }
}

pub fn ensure_dir(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(|e| format!("{}: {e}", path.display()))
}

pub fn read_jsonl(path: &Path) -> Result<Vec<Value>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(format!("{}: {e}", path.display())),
    };
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(|e| format!("{}: {e}", path.display())))
        .collect()
}

pub fn append_jsonl(writer: &mut impl Write, value: &Value) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, value).map_err(|e| e.to_string())?;
    writer.write_all(b"\n").map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())
}

/// JSON string syntax is valid RON, including its escapes.
pub fn ron_string(value: &str) -> String {
    serde_json::to_string(value).expect("strings serialize")
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

pub fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .user_agent(UA)
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .into()
}

pub fn get_bytes(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>, String> {
    let mut response = agent.get(url).call().map_err(|e| format!("{url}: {e}"))?;
    response
        .body_mut()
        .read_to_vec()
        .map_err(|e| format!("{url}: {e}"))
}

/// Retries the HTTP status codes in `retry`, waiting between attempts.
pub fn get_retry(
    agent: &ureq::Agent,
    url: &str,
    retry: &[u16],
    wait: Duration,
    attempts: usize,
) -> Result<Vec<u8>, String> {
    let mut last = String::new();
    for attempt in 0..attempts {
        match agent.get(url).call() {
            Ok(mut response) => {
                return response
                    .body_mut()
                    .read_to_vec()
                    .map_err(|e| format!("{url}: {e}"));
            }
            Err(ureq::Error::StatusCode(code)) if retry.contains(&code) => {
                last = format!("{url}: HTTP {code}");
                if attempt + 1 < attempts {
                    std::thread::sleep(wait);
                }
            }
            Err(e) => return Err(format!("{url}: {e}")),
        }
    }
    Err(format!("{last} (giving up)"))
}

/// Percent-encode like Python's `urllib.parse.quote`: unreserved bytes stay,
/// and `/` stays when `keep_slash` is set.
pub fn quote(value: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        let safe = byte.is_ascii_alphanumeric()
            || matches!(byte, b'-' | b'_' | b'.' | b'~')
            || (keep_slash && byte == b'/');
        if safe {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    None => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The `ref` query parameter of a GitHub API URL, as `gh_collect` recorded it.
pub fn query_ref(url: &str) -> String {
    let Some(start) = url.find("?ref=").or_else(|| url.find("&ref=")) else {
        return String::new();
    };
    let rest = &url[start + 5..];
    let end = rest.find(['&', '#']).unwrap_or(rest.len());
    unquote(&rest[..end])
}

pub fn now_date() -> String {
    let days = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0) as i64;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}")
}

/// Every `.ron` file under `dir`, recursively, sorted by path.
pub fn ron_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    fn visit(dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let path = entry?.path();
            if path.is_dir() {
                visit(&path, out)?;
            } else if path.extension().is_some_and(|ext| ext == "ron") {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    visit(dir, &mut files).map_err(|e| format!("{}: {e}", dir.display()))?;
    files.sort();
    Ok(files)
}

pub fn get_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

pub fn get_array<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}
