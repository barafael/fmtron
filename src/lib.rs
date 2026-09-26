mod ast;
pub mod pretty;

pub use ast::{Kind, RonFile, Value};

use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "ron.pest"]
pub struct RonParser;

pub use pest::Parser;
use pest::error::LineColLocation;

/// Default maximum container-nesting depth. Guards against the stack overflow
/// that pest's recursive descent would otherwise hit on deeply nested input.
pub const MAX_NESTING: usize = 512;

/// Default upper bound on `Config::tab_size`. Anything larger would emit
/// pathological indentation; the CLI overrides this with `--max-tab`.
pub const MAX_TAB: usize = 1024;

/// Upper bound on the indentation any output line may need: `tab_size` times
/// the input's nesting depth. Input needing more is rejected with
/// [`FormatError::IndentTooWide`] rather than allocating absurd indentation
/// (or overflowing and panicking) when `max_tab` and `max_nesting` are raised.
pub const MAX_INDENT: usize = 1 << 20;

/// Formatting configuration. Threaded through the formatter instead of using
/// process-wide global state.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub tab_size: usize,
    pub max_width: usize,
    /// Maximum container-nesting depth accepted; deeper input is rejected with
    /// [`FormatError::TooDeep`] instead of overflowing the stack.
    pub max_nesting: usize,
    /// Upper bound enforced on `tab_size`; larger values are clamped.
    pub max_tab: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tab_size: 4,
            max_width: 40,
            max_nesting: MAX_NESTING,
            max_tab: MAX_TAB,
        }
    }
}

/// The error type returned by [`format_ron`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormatError {
    /// The input contains no RON value: it is blank, or holds only comments
    /// and attributes.
    #[error("no RON data found")]
    Empty,
    /// The input is too deeply nested; formatting it would overflow the stack.
    /// `max` is the configured [`Config::max_nesting`] limit.
    #[error("input is nested {depth} levels deep, exceeding the limit of {max}")]
    TooDeep { depth: usize, max: usize },
    /// Formatting would need `indent` columns of indentation (`tab_size`
    /// times the nesting depth), more than [`MAX_INDENT`].
    #[error("indentation of up to {indent} columns exceeds the limit of {max}")]
    IndentTooWide { indent: usize, max: usize },
    /// The input is not valid RON. The inner error carries line/column
    /// information and a rendering of the offending input position.
    #[error("parse error: {}", render_parse_error(.0))]
    Parse(#[from] Box<pest::error::Error<Rule>>),
}

/// Formats a RON string using the internal formatter.
///
/// # Errors
/// Returns [`FormatError::Empty`] if the input contains no RON value,
/// [`FormatError::TooDeep`] if the input nests deeper than
/// [`Config::max_nesting`], [`FormatError::IndentTooWide`] if indenting it
/// would exceed [`MAX_INDENT`], and [`FormatError::Parse`] if the input cannot
/// be parsed as RON.
///
/// The output keeps the input's line ending, per [`line_ending`].
pub fn format_ron(input: &str, config: &Config) -> Result<String, FormatError> {
    // Clamp `tab_size` to the configured ceiling so pathological values can
    // never emit absurd indentation, then enforce the nesting bound *before*
    // parsing: the scan below is iterative, so it cannot blow the stack the
    // way pest's recursive descent would on input like `[[[[...`.
    let max_nesting = config.max_nesting.max(1);
    let effective = Config {
        tab_size: config.tab_size.min(config.max_tab.max(1)),
        max_nesting,
        ..*config
    };
    let depth = nesting_depth(input);
    if depth > max_nesting {
        return Err(FormatError::TooDeep {
            depth,
            max: max_nesting,
        });
    }
    let indent = effective.tab_size.saturating_mul(depth);
    if indent > MAX_INDENT {
        return Err(FormatError::IndentTooWide {
            indent,
            max: MAX_INDENT,
        });
    }
    match RonParser::parse(Rule::ron_file, input) {
        Ok(mut pairs) => match pairs.next() {
            Some(pair) => Ok(RonFile::parse_from(pair, input, effective)
                .with_newline(line_ending(input))
                .to_string()),
            None => Err(FormatError::Empty),
        },
        Err(_) if RonParser::parse(Rule::no_value, input).is_ok() => Err(FormatError::Empty),
        Err(e) => Err(Box::new(e).into()),
    }
}

/// The line ending of `input`: `"\r\n"` if its first line break is CRLF,
/// otherwise `"\n"`. The formatter emits this between output lines. Line
/// breaks inside string literals and comments are kept as written.
pub fn line_ending(input: &str) -> &'static str {
    match input.find('\n') {
        Some(i) if input[..i].ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

/// Lines longer than this (in chars) are shown as an excerpt around the error
/// column. pest echoes the whole offending line, padded out to the caret, so
/// one bad byte in a multi-megabyte single-line file would print megabytes.
const MAX_ERROR_LINE: usize = 200;
/// Chars of context shown on each side of the error column in an excerpt.
const EXCERPT_RADIUS: usize = 40;

fn render_parse_error(e: &pest::error::Error<Rule>) -> String {
    let line = e.line().trim_end_matches(['\n', '\r']);
    let len = line.chars().count();
    if len <= MAX_ERROR_LINE {
        return e.to_string();
    }
    let (row, col) = match e.line_col {
        LineColLocation::Pos(p) | LineColLocation::Span(p, _) => p,
    };
    let start = col.saturating_sub(1 + EXCERPT_RADIUS);
    let excerpt: String = line.chars().skip(start).take(2 * EXCERPT_RADIUS).collect();
    let (lead, trail) = (
        if start > 0 { "…" } else { "" },
        if start + 2 * EXCERPT_RADIUS < len {
            "…"
        } else {
            ""
        },
    );
    let pad = " ".repeat(col - 1 - start + lead.chars().count());
    // Same layout as pest's own rendering, gutter sized to the line number.
    let gutter = " ".repeat(row.to_string().len());
    format!(
        "{gutter}--> {row}:{col}\n{gutter} |\n{row} | {lead}{excerpt}{trail}\n\
         {gutter} | {pad}^---\n{gutter} |\n{gutter} = {}",
        e.variant.message()
    )
}

/// The deepest `[`, `(` or `{` nesting reached in `input`, measured lexically.
///
/// String, char, byte-string, raw-string and comment bodies are skipped so
/// brackets inside them don't count; nested `/* /* */ */` comments count as
/// nesting levels, since pest recurses on them too. This is an *iterative* scan — unlike the
/// recursive pest parse and the AST renderer, it cannot overflow the stack, so
/// [`format_ron`] runs it as a cheap guard against adversarial input.
fn nesting_depth(input: &str) -> usize {
    let b = input.as_bytes();
    let mut depth = 0usize;
    let mut peak = 0usize;
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'"' => {
                if let Some(end) = scan_quoted(b, i) {
                    i = end;
                    continue;
                }
            }
            b'\'' => {
                if let Some(end) = scan_char(b, i) {
                    i = end;
                    continue;
                }
            }
            b'b' | b'r' => {
                if let Some(end) = scan_raw(b, i) {
                    i = end;
                    continue;
                }
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                // pest's `block_comment` recurses once per nested `/*`, so
                // comment nesting counts toward the depth budget too.
                let mut d = 1;
                peak = peak.max(depth + d);
                i += 2;
                while i < b.len() && d > 0 {
                    if b[i] == b'/' && b.get(i + 1) == Some(&b'*') {
                        d += 1;
                        peak = peak.max(depth + d);
                        i += 2;
                    } else if b[i] == b'*' && b.get(i + 1) == Some(&b'/') {
                        d -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                continue;
            }
            b'[' | b'(' | b'{' => {
                depth += 1;
                if depth > peak {
                    peak = depth;
                }
            }
            b']' | b')' | b'}' => depth = depth.saturating_sub(1),
            _ => {}
        }
        i += 1;
    }
    peak
}

/// `"…"` (with `\` escapes): index just past the closing quote, or None.
fn scan_quoted(b: &[u8], i: usize) -> Option<usize> {
    let mut k = i + 1;
    while k < b.len() {
        match b[k] {
            b'\\' => k = k.saturating_add(2),
            b'"' => return Some(k + 1),
            _ => k += 1,
        }
    }
    None
}

/// `'…'` (a single char, possibly escaped): index just past the close, or None.
///
/// Only a well-formed char literal is skipped. A stray `'` must not hide the
/// brackets up to some later `'` from the depth count.
fn scan_char(b: &[u8], i: usize) -> Option<usize> {
    let body = i + 1;
    let close = if b.get(body) == Some(&b'\\') {
        // The longest escape is `\u{10FFFF}` (10 bytes); none contains `'`
        // except `\'`, whose `'` sits at `body + 1`.
        (body + 2..(body + 11).min(b.len())).find(|&k| b[k] == b'\'')?
    } else {
        // One unescaped (possibly multi-byte) char, which may itself be `'`.
        body + utf8_len(*b.get(body)?)
    };
    (b.get(close) == Some(&b'\'')).then_some(close + 1)
}

/// Byte length of the UTF-8 sequence starting with `lead`.
fn utf8_len(lead: u8) -> usize {
    match lead {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}

/// `r"…"`, `r#"…"#`, `br#"…"#`: index just past the close, or None. A bare `r`
/// /`b` followed by anything other than `#`* or `"` is a plain identifier
/// (e.g. `return`) and returns None.
fn scan_raw(b: &[u8], i: usize) -> Option<usize> {
    let mut k = i;
    if b[k] == b'b' {
        k += 1;
        if b.get(k) != Some(&b'r') {
            return None;
        }
    } else if b[k] != b'r' {
        return None;
    }
    if b.get(k + 1) != Some(&b'#') && b.get(k + 1) != Some(&b'"') {
        return None;
    }
    k += 1;
    let hash_start = k;
    while b.get(k) == Some(&b'#') {
        k += 1;
    }
    if b.get(k) != Some(&b'"') {
        return None;
    }
    let hashes = k - hash_start;
    let mut j = k + 1;
    while j < b.len() {
        if b[j] == b'"'
            && j + 1 + hashes <= b.len()
            && b[j + 1..j + 1 + hashes].iter().all(|&x| x == b'#')
        {
            return Some(j + 1 + hashes);
        }
        j += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_input_is_a_typed_parse_error() {
        let err = format_ron("not valid ((((", &Config::default()).unwrap_err();
        assert!(matches!(err, FormatError::Parse(_)), "got {err:?}");
        assert!(err.to_string().starts_with("parse error:"));
    }

    #[test]
    fn parse_error_chains_to_the_pest_source() {
        let err = format_ron("((( ", &Config::default()).unwrap_err();
        let source = std::error::Error::source(&err);
        assert!(source.is_some(), "parse errors must expose their source");
    }
}
