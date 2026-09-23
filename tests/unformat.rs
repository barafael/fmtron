//! Canonicality / uniformity harness ("un-format" test).
//!
//! A formatter is *canonical* when its output depends only on the token
//! stream, never on the input's incidental whitespace. To verify that, every
//! corpus file is deliberately re-messed ("un-formatted") into many different
//! layouts, and each layout must format to byte-identical output as the
//! original file. If any layout disagrees, the printer is reading something
//! from the source layout that it should not.
//!
//! Layouts per file:
//!   - `Cram`:        every token glued together, zero whitespace
//!   - `Spread`:      one token per line, indented by nesting depth
//!   - `BlankLines`:  blank line between every token
//!   - `Random`:      seeded scrambles with random spaces / newlines
//!
//! Per (file, layout, config) we assert:
//!   - uniformity:  format(layout) == format(original)
//!   - idempotency: format(format(layout)) == format(layout)
//!   - semantics:   ron::from_str(format(layout)) == ron::from_str(original)
//!   - comments:    same comment texts, in the same order, survive
//!
//! Comment attachment is layout-sensitive: whether a comment trails or leads a
//! value is decided from newlines. The scrambler therefore preserves the
//! newline-ness between each comment and the preceding value exactly, while
//! jittering everything else, so the formatter sees the same attachment.
//!
//! Deterministic: default seed `0xCAFE_D00D`, override via `FMTRON_FUZZ_SEED`.

use fmtron::{Config, format_ron};

const DEFAULT_SEED: u64 = 0xCAFE_D00D;
const SCRAMBLES_PER_FILE: usize = 30;

fn seed() -> u64 {
    match std::env::var("FMTRON_FUZZ_SEED") {
        Ok(s) => {
            let stripped = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"));
            match stripped {
                Some(hex) => u64::from_str_radix(hex, 16),
                None => s.parse::<u64>(),
            }
            .unwrap_or_else(|_| panic!("FMTRON_FUZZ_SEED must be an integer, got {s:?}"))
        }
        Err(_) => DEFAULT_SEED,
    }
}

/// xorshift64* PRNG — deterministic across platforms.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

// ---------------------------------------------------------------------------
// Lexer: splits a RON document into tokens that can be re-joined arbitrarily.
//
// Tokens are emitted verbatim from the source, so only whitespace/comments may
// sit between them; strings, numbers, idents etc. are never split. This keeps
// every re-layout valid RON with identical semantics by construction.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokKind {
    Punct,
    Content,
    LineComment,
    BlockComment,
}

#[derive(Debug, Clone)]
struct Tok {
    kind: TokKind,
    text: String,
    start: usize,
    end: usize,
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b >= 0x80
}

fn is_ident_cont(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80
}

fn is_num_cont(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'+' | b'-')
}

/// k points at the opening `"`; returns the index just past the closing `"`.
fn scan_quoted(bytes: &[u8], k: usize) -> Option<usize> {
    let mut k = k + 1;
    while k < bytes.len() {
        match bytes[k] {
            b'\\' => k = k.saturating_add(2),
            b'"' => return Some(k + 1),
            _ => k += 1,
        }
    }
    None
}

/// Returns the end index of a raw string `r#"…"#` / `br#"…"#` starting at `i`.
fn scan_raw(bytes: &[u8], i: usize) -> Option<usize> {
    let mut k = i;
    if bytes[k] == b'b' {
        k += 1;
        if bytes.get(k) != Some(&b'r') {
            return None;
        }
    } else if bytes[k] != b'r' {
        return None;
    }
    k += 1;
    let hash_start = k;
    while bytes.get(k) == Some(&b'#') {
        k += 1;
    }
    if bytes.get(k) != Some(&b'"') {
        return None;
    }
    let hashes = k - hash_start;
    let mut j = k + 1;
    while j < bytes.len() {
        if bytes[j] == b'"'
            && j + 1 + hashes <= bytes.len()
            && bytes[j + 1..j + 1 + hashes].iter().all(|&b| b == b'#')
        {
            return Some(j + 1 + hashes);
        }
        j += 1;
    }
    None
}

/// `'-' | '+' | digit | '.'-digit` … a number-ish run; may start with `-inf`.
fn scan_number(bytes: &[u8], i: usize) -> Option<usize> {
    let b = bytes[i];
    let starts = b.is_ascii_digit()
        || (b == b'.' && bytes.get(i + 1).is_some_and(|c| c.is_ascii_digit()))
        || ((b == b'+' || b == b'-')
            && bytes
                .get(i + 1)
                .is_some_and(|&c| c.is_ascii_digit() || c == b'i' || c == b'N' || c == b'.'));
    if !starts {
        return None;
    }
    let mut k = i + 1;
    while k < bytes.len() && is_num_cont(bytes[k]) {
        k += 1;
    }
    Some(k)
}

fn scan_ident(bytes: &[u8], i: usize) -> Option<usize> {
    let mut k = i;
    if bytes[k] == b'r' && bytes.get(k + 1) == Some(&b'#') {
        k += 2;
    }
    if !is_ident_start(bytes[k]) {
        return None;
    }
    k += 1;
    while k < bytes.len() && is_ident_cont(bytes[k]) {
        k += 1;
    }
    Some(k)
}

fn newline_since(src: &str, from: usize, to: usize) -> bool {
    let (from, to) = (from.min(src.len()), to.min(src.len()));
    from < to && src[from..to].contains('\n')
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let bytes = src.as_bytes();
    let mut toks: Vec<Tok> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if matches!(b, b' ' | b'\t' | b'\r' | b'\n') {
            i += 1;
            continue;
        }
        if b == b'/' && bytes.get(i + 1) == Some(&b'/') {
            let start = i;
            i += 2;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            toks.push(Tok {
                kind: TokKind::LineComment,
                text: src[start..i].to_string(),
                start,
                end: i,
            });
            continue;
        }
        if b == b'/' && bytes.get(i + 1) == Some(&b'*') {
            let start = i;
            i += 2;
            let mut depth = 1usize;
            while i < bytes.len() && depth > 0 {
                if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
                    depth += 1;
                    i += 2;
                } else if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if depth != 0 {
                return Err("unterminated block comment".into());
            }
            toks.push(Tok {
                kind: TokKind::BlockComment,
                text: src[start..i].to_string(),
                start,
                end: i,
            });
            continue;
        }
        if let Some(end) = scan_raw(bytes, i) {
            toks.push(Tok {
                kind: TokKind::Content,
                text: src[i..end].to_string(),
                start: i,
                end,
            });
            i = end;
            continue;
        }
        if b == b'b'
            && bytes.get(i + 1) == Some(&b'"')
            && let Some(end) = scan_quoted(bytes, i + 1)
        {
            toks.push(Tok {
                kind: TokKind::Content,
                text: src[i..end].to_string(),
                start: i,
                end,
            });
            i = end;
            continue;
        }
        if b == b'"'
            && let Some(end) = scan_quoted(bytes, i)
        {
            toks.push(Tok {
                kind: TokKind::Content,
                text: src[i..end].to_string(),
                start: i,
                end,
            });
            i = end;
            continue;
        }
        if b == b'\'' {
            let mut k = i + 1;
            let mut end = None;
            while k < bytes.len() {
                match bytes[k] {
                    b'\\' => k = k.saturating_add(2),
                    b'\'' => {
                        end = Some(k + 1);
                        break;
                    }
                    _ => k += 1,
                }
            }
            if let Some(end) = end {
                toks.push(Tok {
                    kind: TokKind::Content,
                    text: src[i..end].to_string(),
                    start: i,
                    end,
                });
                i = end;
                continue;
            }
        }
        if let Some(end) = scan_number(bytes, i) {
            toks.push(Tok {
                kind: TokKind::Content,
                text: src[i..end].to_string(),
                start: i,
                end,
            });
            i = end;
            continue;
        }
        if let Some(end) = scan_ident(bytes, i) {
            toks.push(Tok {
                kind: TokKind::Content,
                text: src[i..end].to_string(),
                start: i,
                end,
            });
            i = end;
            continue;
        }
        if matches!(
            b,
            b'(' | b')' | b'[' | b']' | b'{' | b'}' | b',' | b':' | b'#' | b'!' | b'='
        ) {
            toks.push(Tok {
                kind: TokKind::Punct,
                text: (b as char).to_string(),
                start: i,
                end: i + 1,
            });
            i += 1;
            continue;
        }
        return Err(format!("unexpected byte 0x{b:02x} at offset {i}"));
    }
    Ok(toks)
}

// ---------------------------------------------------------------------------
// Layout generation.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    Random,
    Cram,
    Spread,
    BlankLines,
}

fn sep_inline(layout: Layout, rng: &mut Rng) -> String {
    match layout {
        Layout::Random => [" ", "  ", "\t"][rng.below(3)].to_string(),
        _ => " ".to_string(),
    }
}

fn sep_newline(layout: Layout, depth: usize, rng: &mut Rng) -> String {
    match layout {
        Layout::Cram => "\n".to_string(),
        Layout::Spread => format!("\n{}", "  ".repeat(depth)),
        Layout::BlankLines => format!("\n\n{}", "  ".repeat(depth)),
        Layout::Random => match rng.below(3) {
            0 => "\n".to_string(),
            1 => format!("\n{}", " ".repeat(rng.below(7))),
            _ => "\n\n".to_string(),
        },
    }
}

fn sep_free(layout: Layout, depth: usize, rng: &mut Rng) -> String {
    match layout {
        Layout::Cram => String::new(),
        Layout::Spread => format!("\n{}", "  ".repeat(depth)),
        Layout::BlankLines => format!("\n\n{}", "  ".repeat(depth)),
        Layout::Random => match rng.below(10) {
            0 => " ".to_string(),
            1 => "  ".to_string(),
            2 => "\t".to_string(),
            3 => format!("\n{}", " ".repeat(rng.below(7))),
            4 => "\n\n".to_string(),
            _ => " ".repeat(1 + rng.below(4)),
        },
    }
}

fn emit(src: &str, toks: &[Tok], layout: Layout, rng: &mut Rng) -> String {
    let n = toks.len();
    // req_before[i]: newline requirement for the gap before token i, driven by
    // the nearest comment that follows it. `Some(true)` = must contain a
    // newline, `Some(false)` = must not. Leftmost comment wins (a trailing
    // comment pins its own gap to no-newline; a later leading comment's
    // newline must then come after it).
    let mut req: Vec<Option<bool>> = vec![None; n];
    let mut last_content: Option<usize> = None;
    for (i, t) in toks.iter().enumerate() {
        if matches!(t.kind, TokKind::LineComment | TokKind::BlockComment)
            && let Some(pc) = last_content
        {
            let need = newline_since(src, toks[pc].end, t.start);
            for slot in req.iter_mut().take(i + 1).skip(pc + 1) {
                if slot.is_none() {
                    *slot = Some(need);
                }
            }
        }
        if t.kind == TokKind::Content {
            last_content = Some(i);
        }
    }

    let mut out = String::new();
    let mut depth = 0usize;
    for i in 0..n {
        if i > 0 {
            let force_newline = toks[i - 1].kind == TokKind::LineComment;
            let sep = match (force_newline, req[i]) {
                (true, _) | (_, Some(true)) => sep_newline(layout, depth, rng),
                (_, Some(false)) => sep_inline(layout, rng),
                (_, None) => sep_free(layout, depth, rng),
            };
            out.push_str(&sep);
        }
        match toks[i].text.as_str() {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth = depth.saturating_sub(1),
            _ => {}
        }
        out.push_str(&toks[i].text);
    }
    out
}

// ---------------------------------------------------------------------------
// The harness.
// ---------------------------------------------------------------------------

fn normalize(s: &str) -> String {
    s.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
}

fn comment_seq(toks: &[Tok]) -> Vec<String> {
    toks.iter()
        .filter(|t| matches!(t.kind, TokKind::LineComment | TokKind::BlockComment))
        .map(|t| t.text.trim_end_matches(['\n', '\r', ' ', '\t']).to_string())
        .collect()
}

#[test]
fn corpus_formats_canonically_regardless_of_layout() {
    let s = seed();
    let mut rng = Rng(s);
    let configs = [
        Config {
            tab_size: 4,
            max_width: 40,
            ..Config::default()
        },
        Config {
            tab_size: 2,
            max_width: 80,
            ..Config::default()
        },
        Config {
            tab_size: 4,
            max_width: 24,
            ..Config::default()
        },
    ];
    let mut failures: Vec<String> = Vec::new();
    let mut files = 0usize;
    let mut layouts = 0usize;
    let mut saw_diversity = false;

    for dir in [
        "test_data/ron_corpus",
        "test_data/unformatted",
        "test_data/gaps/unformatted",
        "test_data/synthetic",
    ] {
        let mut paths: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("cannot read {dir}: {e}"))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "ron"))
            .collect();
        paths.sort();

        for path in paths {
            files += 1;
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let original = std::fs::read_to_string(&path).unwrap();
            let toks = match lex(&original) {
                Ok(t) => t,
                Err(e) => {
                    failures.push(format!("{dir}/{name}: lexer failed: {e}"));
                    continue;
                }
            };
            let comments = comment_seq(&toks);
            let oracle_before = ron::from_str::<ron::Value>(&original).ok();

            let mut base: Vec<String> = Vec::new();
            for cfg in &configs {
                match format_ron(&original, cfg) {
                    Ok(g) => base.push(g),
                    // Gap cases may legitimately fail to parse yet; their
                    // expected failure is asserted in `gap_validation.rs`.
                    Err(_) if dir.contains("gaps") => break,
                    Err(e) => {
                        failures.push(format!("{dir}/{name}: format_ron failed on original: {e}"));
                        break;
                    }
                }
            }
            if base.len() != configs.len() {
                continue;
            }

            let mut layouts_for_file: Vec<String> = Vec::new();
            for _ in 0..SCRAMBLES_PER_FILE {
                layouts_for_file.push(emit(&original, &toks, Layout::Random, &mut rng));
            }
            let mut dummy = Rng(0);
            for layout in [Layout::Cram, Layout::Spread, Layout::BlankLines] {
                layouts_for_file.push(emit(&original, &toks, layout, &mut dummy));
            }
            if layouts_for_file.iter().any(|l| l != &original) {
                saw_diversity = true;
            }

            for layout_text in &layouts_for_file {
                for (ci, cfg) in configs.iter().enumerate() {
                    layouts += 1;
                    let cfg_label = format!("tab={} width={}", cfg.tab_size, cfg.max_width);
                    let case = format!("{dir}/{name} ({cfg_label})");

                    let out = match format_ron(layout_text, cfg) {
                        Ok(o) => o,
                        Err(e) => {
                            failures.push(format!(
                                "{case}: format_ron failed on un-formatted input:\n{e}\n--- input ---\n{layout_text}"
                            ));
                            continue;
                        }
                    };

                    let canonical = &base[ci];
                    if normalize(&out) != normalize(canonical) {
                        failures.push(format!(
                            "{case}: non-canonical output\n--- canonical ---\n{}\n--- actual ---\n{}\n--- input ---\n{layout_text}",
                            normalize(canonical),
                            normalize(&out)
                        ));
                    }

                    let out_toks = lex(&out).unwrap_or_else(|e| {
                        panic!("{case}: lexer failed on formatter output: {e}\n{out}")
                    });
                    if comment_seq(&out_toks) != comments {
                        failures.push(format!(
                            "{case}: comments altered\n--- input ---\n{layout_text}\n--- output ---\n{out}"
                        ));
                    }

                    if let Ok(twice) = format_ron(&out, cfg) {
                        if normalize(&twice) != normalize(&out) {
                            failures.push(format!("{case}: not idempotent\n--- out ---\n{out}"));
                        }
                    } else {
                        failures.push(format!("{case}: idempotency second pass failed"));
                    }

                    if let Some(before) = &oracle_before {
                        match ron::from_str::<ron::Value>(&out) {
                            Ok(after) => {
                                if &after != before {
                                    failures.push(format!(
                                        "{case}: semantic drift\n--- input ---\n{layout_text}\n--- output ---\n{out}"
                                    ));
                                }
                            }
                            Err(e) => failures
                                .push(format!("{case}: output not accepted by oracle: {e}\n{out}")),
                        }
                    }
                }
            }
        }
    }

    assert!(files > 0, "no corpus files found");
    assert!(layouts > 0, "no layouts generated");
    assert!(
        saw_diversity,
        "no layout differed from its input — harness is vacuous"
    );
    if !failures.is_empty() {
        panic!(
            "canonicality harness (seed {s:#x}): {} failure(s) across {files} files:\n\n{}",
            failures.len(),
            failures.join("\n\n")
        );
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    fn kinds(toks: &[Tok]) -> Vec<TokKind> {
        toks.iter().map(|t| t.kind).collect()
    }

    #[test]
    fn lexer_splits_all_token_kinds() {
        let src = r##"#![enable(a, b)] /* head */ Foo(x: 1, y: -1.5e-2f32, s: r#"raw "q""#, t: b"bytes", u: 'x', v: 0xFFu8, w: inf) // tail
"##;
        let toks = lex(src).unwrap();
        let texts: Vec<&str> = toks.iter().map(|t| t.text.as_str()).collect();
        assert_eq!(
            texts,
            &[
                "#",
                "!",
                "[",
                "enable",
                "(",
                "a",
                ",",
                "b",
                ")",
                "]",
                "/* head */",
                "Foo",
                "(",
                "x",
                ":",
                "1",
                ",",
                "y",
                ":",
                "-1.5e-2f32",
                ",",
                "s",
                ":",
                r##"r#"raw "q""#"##,
                ",",
                "t",
                ":",
                "b\"bytes\"",
                ",",
                "u",
                ":",
                "'x'",
                ",",
                "v",
                ":",
                "0xFFu8",
                ",",
                "w",
                ":",
                "inf",
                ")",
                "// tail",
            ]
        );
        assert_eq!(
            kinds(&toks)
                .iter()
                .filter(|k| **k == TokKind::Content)
                .count(),
            18
        );
        assert_eq!(
            kinds(&toks)
                .iter()
                .filter(|k| **k == TokKind::BlockComment)
                .count(),
            1
        );
        assert_eq!(
            kinds(&toks)
                .iter()
                .filter(|k| **k == TokKind::LineComment)
                .count(),
            1
        );
    }

    #[test]
    fn cram_removes_all_safe_whitespace() {
        let src = "[ 1 ,\n 2 /* c */ , 3 ]";
        let toks = lex(src).unwrap();
        let mut rng = Rng(1);
        let crammed = emit(src, &toks, Layout::Cram, &mut rng);
        // a single space is kept before inline comments so they cannot fuse
        // with the preceding token; everything else is glued together
        assert_eq!(crammed, "[1,2 /* c */,3]");
    }

    #[test]
    fn trailing_comment_stays_on_its_value_line() {
        let src = "[\n    a,\n    // lead b\n    b, // trail b\n]";
        let toks = lex(src).unwrap();
        let mut rng = Rng(2);
        let out = emit(src, &toks, Layout::Spread, &mut rng);
        assert!(
            out.lines()
                .any(|l| l.contains("// trail b") && l.contains("b")),
            "trailing comment moved off its value line:\n{out}"
        );
        assert!(
            out.lines().any(|l| l.trim() == "// lead b"),
            "leading comment should sit on its own line:\n{out}"
        );
    }
}
