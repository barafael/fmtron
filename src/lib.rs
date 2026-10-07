//! A formatter for [RON](https://github.com/ron-rs/ron) files.
//!
//! [`format_ron`] turns RON text into fmtron's layout, keeping every comment
//! and the value's meaning. The `fmtron` binary (the crate's `cli` feature,
//! on by default) wraps it for files, directories, `--check` and editors;
//! this library is the same formatter for programs of your own, such as an
//! editor plugin or a web page that formats RON. Without the `cli` feature
//! it depends on nothing from the file system or the terminal and builds
//! for `wasm32`.
//!
//! ```
//! use fmtron::{Config, FormatError, format_ron};
//!
//! let config = Config::default().with_max_width(40);
//! let formatted = format_ron("Config(name: \"demo\", retries: [1, 2, 3], verbose: true)", &config)?;
//! assert_eq!(
//!     formatted,
//!     "Config(\n    name: \"demo\",\n    retries: [1, 2, 3],\n    verbose: true,\n)"
//! );
//!
//! // A parse error says where and what, in RON terms.
//! let Err(FormatError::Parse(e)) = format_ron("(a: 1 b: 2)", &config) else {
//!     unreachable!()
//! };
//! assert_eq!((e.line(), e.column()), (1, 7));
//! assert_eq!(e.message(), "expected `,` or `)`, found `b`");
//! # Ok::<(), FormatError>(())
//! ```
//!
//! The output has no trailing line break; add [`line_ending`]`(input)` to
//! write a file. Input from an untrusted source is safe to format:
//! [`Config::max_nesting`] bounds the recursion, and the limits are errors,
//! never panics.
//!
//! The layout rules are described in the README, and what may change between
//! releases in `STABILITY.md`.

mod ast;
mod config_file;
mod parse_error;
mod pretty;
#[cfg(test)]
mod reference;

pub use config_file::{CONFIG_FILE_NAMES, ConfigParseError, FileConfig, FileConfigError};
pub use parse_error::ParseError;

use parser::{RonParser, Rule};
use pest::Parser as _;

/// The RON parser the formatter uses. Punctuation is silent here, so it adds
/// nothing to the parse tree; `parse_error` has a variant that names it.
mod parser {
    #[derive(pest_derive::Parser)]
    #[grammar = "ron.pest"]
    #[grammar_inline = r#"
comma = _{ "," }
colon = _{ ":" }
lbracket = _{ "[" }
rbracket = _{ "]" }
lparen = _{ "(" }
rparen = _{ ")" }
lbrace = _{ "{" }
rbrace = _{ "}" }
"#]
    pub struct RonParser;
}

/// Default maximum container-nesting depth. Guards against the stack overflow
/// that pest's recursive descent would otherwise hit on deeply nested input.
pub const MAX_NESTING: usize = 512;

/// Default `Config::max_width`: rustfmt's default, and close to how wide RON
/// is written in practice (a sample of real-world files had a median longest
/// line of 56 columns and a 75th percentile of 89).
pub const DEFAULT_WIDTH: usize = 100;

/// Default upper bound on `Config::tab_size`. Anything larger would emit
/// pathological indentation; the CLI overrides this with `--max-tab`.
pub const MAX_TAB: usize = 1024;

/// Upper bound on the indentation any output line may need: `tab_size` times
/// the input's nesting depth. Input needing more is rejected with
/// [`FormatError::IndentTooWide`] rather than allocating absurd indentation
/// (or overflowing and panicking) when `max_tab` and `max_nesting` are raised.
pub const MAX_INDENT: usize = 1 << 20;

/// Formatting settings. Start from [`Config::default`] and set fields, or
/// chain the `with_*` methods:
///
/// ```
/// use fmtron::{BlankLines, Config};
///
/// let config = Config::default().with_tab_size(2).with_blank_lines(BlankLines::Remove);
/// let mut same = Config::default();
/// same.tab_size = 2;
/// same.blank_lines = BlankLines::Remove;
/// assert_eq!(config, same);
/// ```
///
/// Settings may be added in minor releases, so the struct cannot be built
/// with a literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Config {
    /// Indentation per nesting level, in spaces. Default 4.
    pub tab_size: usize,
    /// The line width to stay within where the layout allows. A soft limit:
    /// a long atom, or a deeply indented one, may overrun it. Default
    /// [`DEFAULT_WIDTH`].
    pub max_width: usize,
    /// Maximum container-nesting depth accepted; deeper input is rejected with
    /// [`FormatError::TooDeep`] instead of overflowing the stack. Default
    /// [`MAX_NESTING`]; lower it for input you do not trust.
    pub max_nesting: usize,
    /// Upper bound enforced on `tab_size`; larger values are clamped. Default
    /// [`MAX_TAB`].
    pub max_tab: usize,
    /// Whether blank lines between elements are kept. Default
    /// [`BlankLines::Keep`].
    pub blank_lines: BlankLines,
    /// The set of layout rules. Default [`StyleEdition::default`], the
    /// latest.
    pub style_edition: StyleEdition,
}

impl Config {
    /// This configuration with [`Config::tab_size`] set.
    #[must_use]
    pub fn with_tab_size(mut self, tab_size: usize) -> Self {
        self.tab_size = tab_size;
        self
    }

    /// This configuration with [`Config::max_width`] set.
    #[must_use]
    pub fn with_max_width(mut self, max_width: usize) -> Self {
        self.max_width = max_width;
        self
    }

    /// This configuration with [`Config::max_nesting`] set.
    #[must_use]
    pub fn with_max_nesting(mut self, max_nesting: usize) -> Self {
        self.max_nesting = max_nesting;
        self
    }

    /// This configuration with [`Config::max_tab`] set.
    #[must_use]
    pub fn with_max_tab(mut self, max_tab: usize) -> Self {
        self.max_tab = max_tab;
        self
    }

    /// This configuration with [`Config::blank_lines`] set.
    #[must_use]
    pub fn with_blank_lines(mut self, blank_lines: BlankLines) -> Self {
        self.blank_lines = blank_lines;
        self
    }

    /// This configuration with [`Config::style_edition`] set.
    #[must_use]
    pub fn with_style_edition(mut self, style_edition: StyleEdition) -> Self {
        self.style_edition = style_edition;
        self
    }
}

/// How blank lines in the input are treated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Deserialize)]
pub enum BlankLines {
    /// Keep one blank line wherever the input has one or more between two
    /// elements, comments or header items. A container holding one always
    /// breaks. Blank lines right after an opening or before a closing bracket
    /// are dropped.
    #[default]
    Keep,
    /// Remove all blank lines.
    Remove,
}

/// A set of layout rules, named by the year it was introduced, like
/// rustfmt's style editions. The rules of an edition do not change once it
/// is released, apart from bug fixes; a change of style is a new edition,
/// which stays opt-in until the next major version of fmtron. There is one
/// edition so far. In a `fmt.ron` or on the command line it is written as
/// the year: `style_edition: 2026`, `--style-edition 2026`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, serde::Deserialize)]
#[serde(try_from = "u16")]
#[non_exhaustive]
pub enum StyleEdition {
    /// The rules of fmtron 1.0, described in the README.
    #[default]
    Edition2026,
}

impl StyleEdition {
    /// The year this edition is named by.
    pub fn year(self) -> u16 {
        match self {
            Self::Edition2026 => 2026,
        }
    }
}

impl TryFrom<u16> for StyleEdition {
    type Error = String;

    fn try_from(year: u16) -> Result<Self, String> {
        match year {
            2026 => Ok(Self::Edition2026),
            _ => Err(format!(
                "unknown style edition {year}; the editions are: 2026"
            )),
        }
    }
}

impl std::str::FromStr for StyleEdition {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        s.parse::<u16>()
            .map_err(|_| format!("a style edition is a year, such as 2026; found `{s}`"))?
            .try_into()
    }
}

impl std::fmt::Display for StyleEdition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.year())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tab_size: 4,
            max_width: DEFAULT_WIDTH,
            max_nesting: MAX_NESTING,
            max_tab: MAX_TAB,
            blank_lines: BlankLines::Keep,
            style_edition: StyleEdition::default(),
        }
    }
}

/// The error type returned by [`format_ron`]. Every variant is a property of
/// the input; formatting never panics.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
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
    /// The input is not valid RON. The error says where and what.
    #[error("parse error: {0}")]
    Parse(#[from] ParseError),
}

/// Formats RON text.
///
/// The output keeps the input's comments, line ending (per [`line_ending`])
/// and, with [`BlankLines::Keep`], its blank lines, and ends without a line
/// break. Formatting it again returns it unchanged.
///
/// # Errors
/// Returns [`FormatError::Empty`] if the input contains no RON value,
/// [`FormatError::TooDeep`] if the input nests deeper than
/// [`Config::max_nesting`], [`FormatError::IndentTooWide`] if indenting it
/// would exceed [`MAX_INDENT`], and [`FormatError::Parse`] if the input cannot
/// be parsed as RON.
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
            Some(pair) => Ok(ast::RonFile::parse_from(pair, input, effective)
                .with_newline(line_ending(input))
                .to_string()),
            None => Err(FormatError::Empty),
        },
        Err(_) if RonParser::parse(Rule::no_value, input).is_ok() => Err(FormatError::Empty),
        Err(e) => Err(ParseError::new(parse_error::improve(input, e)).into()),
    }
}

/// The line ending of `input`: `"\r\n"` if its first line break is CRLF,
/// otherwise `"\n"`. The formatter emits this between output lines, including
/// those inside block comments. A line break inside a string or char literal
/// is part of its value, not of the layout: it is skipped here and kept as
/// written.
pub fn line_ending(input: &str) -> &'static str {
    let b = input.as_bytes();
    let mut i = 0;
    while i < b.len() {
        let skip = match b[i] {
            b'\n' if i > 0 && b[i - 1] == b'\r' => return "\r\n",
            b'\n' => return "\n",
            // Up to the line break that ends the comment, so quotes in it
            // start no literal.
            b'/' if b.get(i + 1) == Some(&b'/') => Some(line_comment_end(b, i)),
            // To its first line break or its end, whichever comes first.
            b'/' if b.get(i + 1) == Some(&b'*') => {
                let (end, _) = block_comment(b, i);
                Some(
                    b[i..end]
                        .iter()
                        .position(|&c| c == b'\n')
                        .map_or(end, |n| i + n),
                )
            }
            _ => literal_end(b, i),
        };
        i = skip.unwrap_or(i + 1);
    }
    "\n"
}

/// The index just past the string, char or raw literal starting at `i`, or
/// `None` when no literal starts there. A byte string `b"…"` or byte char
/// `b'…'` is not recognized as a whole: its `b` is skipped as an ordinary
/// byte and the literal after it is found on the next step, which comes to
/// the same.
pub(crate) fn literal_end(b: &[u8], i: usize) -> Option<usize> {
    match b[i] {
        b'"' => scan_quoted(b, i),
        b'\'' => scan_char(b, i),
        b'b' | b'r' => scan_raw(b, i),
        _ => None,
    }
}

/// The index of the `\n` that ends the line comment starting at `i`, or
/// `b.len()` if none does.
pub(crate) fn line_comment_end(b: &[u8], i: usize) -> usize {
    b[i..]
        .iter()
        .position(|&c| c == b'\n')
        .map_or(b.len(), |n| i + n)
}

/// The block comment starting at `i` (`b[i..]` starts with `/*`): the index
/// just past its end, or `b.len()` if it is unterminated, and how deeply its
/// `/* … */` nest, counting itself.
pub(crate) fn block_comment(b: &[u8], i: usize) -> (usize, usize) {
    let (mut depth, mut deepest) = (0usize, 0usize);
    let mut k = i;
    while k < b.len() {
        if b[k..].starts_with(b"/*") {
            depth += 1;
            deepest = deepest.max(depth);
            k += 2;
        } else if b[k..].starts_with(b"*/") {
            depth -= 1;
            k += 2;
            if depth == 0 {
                return (k, deepest);
            }
        } else {
            k += 1;
        }
    }
    (k, deepest)
}

/// Whitespace between tokens, as in the grammar's `WHITESPACE` and the
/// reference parser: Unicode's `Pattern_White_Space`.
pub(crate) const fn is_whitespace(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t'
            | '\n'
            | '\r'
            | '\u{0B}'
            | '\u{0C}'
            | '\u{85}'
            | '\u{200E}'
            | '\u{200F}'
            | '\u{2028}'
            | '\u{2029}'
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
        if let Some(end) = literal_end(b, i) {
            i = end;
            continue;
        }
        match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                i = line_comment_end(b, i);
                continue;
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                // pest's `block_comment` recurses once per nested `/*`, so
                // comment nesting counts toward the depth budget too.
                let (end, deepest) = block_comment(b, i);
                peak = peak.max(depth + deepest);
                i = end;
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
    fn style_editions_are_years() {
        assert_eq!("2026".parse(), Ok(StyleEdition::Edition2026));
        assert_eq!(StyleEdition::Edition2026.to_string(), "2026");
        assert!("2027".parse::<StyleEdition>().unwrap_err().contains("2026"));
        assert!("latest".parse::<StyleEdition>().is_err());
    }
}
