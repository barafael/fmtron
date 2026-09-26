//! Readable parse errors.
//!
//! pest reports what it expected by *rule*, and since `COMMENT` is an
//! implicit rule, a missing `,` or `]` comes out as `expected COMMENT`. On
//! failure, the input is parsed once more with pest's error detail enabled,
//! which records the literal tokens tried at the failing position; those
//! are turned into a message such as ``expected `,` or `]`, found end of
//! input``.

use pest::Parser;
use pest::error::{Error, ErrorVariant};

use crate::{RonParser, Rule};

/// Tokens that close or separate container elements, in reporting order.
const STRUCTURAL: [&str; 5] = [",", ":", "]", ")", "}"];

/// Tokens that can start a value (besides identifiers, which pest reports
/// as a built-in rule, and digits, reported as a range).
const VALUE_START: [&str; 17] = [
    "\"", "'", "(", "[", "{", "+", "-", ".", "true", "false", "inf", "NaN", "b", "b'", "br", "r",
    "r#",
];

/// `error`, reworded where a clearer message can be derived. The position is
/// kept, so pest's excerpt, caret and `with_path` still work.
pub(crate) fn improve(input: &str, error: Error<Rule>) -> Error<Rule> {
    match describe(input, &error) {
        Some((offset, message)) => match pest::Position::new(input, offset) {
            Some(pos) => Error::new_from_pos(ErrorVariant::CustomError { message }, pos),
            None => error,
        },
        None => error,
    }
}

fn describe(input: &str, error: &Error<Rule>) -> Option<(usize, String)> {
    // The flag is process-global: another thread parsing right now merely
    // collects detail it does not need.
    pest::set_error_detail(true);
    let detailed = RonParser::parse(Rule::ron_file, input).err();
    pest::set_error_detail(false);
    let attempts = detailed?.parse_attempts()?;
    let pos = attempts.max_position.min(input.len());

    // Inside a string or char literal, the tokens tried say little; look at
    // the literal itself instead.
    if let Some(found) = enclosing_literal(input, pos).and_then(|s| literal_problem(input, s)) {
        return Some(found);
    }
    if let Some(found) = hash_comment(input, pos).or_else(|| bad_number(input, pos)) {
        return Some(found);
    }
    if input[pos..].starts_with("::") {
        return Some((
            pos,
            "`::` paths are not RON; write the variant alone (`Reset`, not `Color::Reset`)".into(),
        ));
    }

    let expects_eoi = matches!(&error.variant,
        ErrorVariant::ParsingError { positives, .. } if positives.contains(&Rule::EOI));
    // After an identifier or number character, alphanumeric tokens (and `(`,
    // `.`, `_`) would only continue the current token, not start a new one.
    let ident_char = |c: char| c.is_alphanumeric() || c == '_';
    let continues = input[..pos].chars().next_back().is_some_and(ident_char);
    // `Foo (…)` is a struct too: after an identifier, even across
    // whitespace, `(` continues it rather than starting a value.
    let after_ident = input[..pos]
        .trim_end()
        .chars()
        .next_back()
        .is_some_and(ident_char);
    let tokens: Vec<String> = attempts
        .expected_tokens()
        .iter()
        .map(ToString::to_string)
        .filter(|t| !(continues && is_continuation(t)) && !(after_ident && t == "("))
        .collect();

    let mut expected: Vec<String> = STRUCTURAL
        .iter()
        .filter(|s| tokens.iter().any(|t| t == *s))
        .map(|s| format!("`{s}`"))
        .collect();
    let value_start = tokens.iter().any(|t| {
        VALUE_START.contains(&t.as_str()) || t == "0..9" || t.starts_with("BUILTIN") || t == "0"
    });
    if value_start {
        expected.push("a value".into());
    }
    if expects_eoi && expected.is_empty() {
        expected.push("end of input".into());
    }
    if expected.is_empty() {
        return None;
    }
    let expected = join_or(&expected);

    let message = match input[pos..].chars().next() {
        None => format!("expected {expected}, found end of input"),
        Some(c) if is_stray(c) => {
            format!("unexpected character `{}`, expected {expected}", show(c))
        }
        Some(c) => format!("expected {expected}, found `{}`", show(c)),
    };
    Some((pos, message))
}

/// A token that only extends what precedes it: digits, identifier
/// characters, number suffixes and exponents, `(` after a struct name.
fn is_continuation(token: &str) -> bool {
    token == "0..9"
        || token == "("
        || token == "."
        || token.starts_with("BUILTIN")
        || token.chars().all(|c| c.is_alphanumeric() || c == '_')
}

/// A `#` that is not an attribute's `#!`, at or just before `pos`: likely a
/// shell-style comment, which RON does not have.
fn hash_comment(input: &str, pos: usize) -> Option<(usize, String)> {
    // `#` and `!` may be separated by whitespace, so look back past it.
    let before = input[..pos].trim_end();
    let at = if input[pos..].starts_with('#') {
        pos
    } else if before.ends_with('#') {
        before.len() - 1
    } else {
        return None;
    };
    (!input[at..].starts_with("#!")).then(|| {
        (
            at,
            "`#` does not start a comment in RON; use `// …` or `/* … */`".into(),
        )
    })
}

/// A number-like token touching the failing position (`1.5.5`, `0x_FF`,
/// `1e`) that does not parse as a complete value.
fn bad_number(input: &str, pos: usize) -> Option<(usize, String)> {
    let part = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '.';
    let start = input[..pos]
        .char_indices()
        .rev()
        .take_while(|&(_, c)| part(c))
        .last()
        .map_or(pos, |(i, _)| i);
    let end = pos
        + input[pos..]
            .find(|c: char| !part(c))
            .unwrap_or(input.len() - pos);
    let start = if input[..start].ends_with(['-', '+']) {
        start - 1
    } else {
        start
    };
    let token = &input[start..end];
    if !token
        .trim_start_matches(['-', '+'])
        .starts_with(|c: char| c.is_ascii_digit())
    {
        return None;
    }
    let complete =
        RonParser::parse(Rule::value, token).is_ok_and(|pairs| pairs.as_str().len() == token.len());
    (!complete).then(|| (start, format!("invalid number literal `{token}`")))
}

/// `c` for a message: as itself, or escaped if it is a control character.
fn show(c: char) -> String {
    if c.is_control() {
        c.escape_debug().to_string()
    } else {
        c.to_string()
    }
}

/// A character that can never appear outside a literal or comment in RON.
fn is_stray(c: char) -> bool {
    !(c.is_alphanumeric() || c.is_whitespace() || "_\"'()[]{},:+-.#!/".contains(c))
}

/// "a", "a or b", "a, b or c".
fn join_or(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} or {last}", init.join(", ")),
    }
}

/// The start of the string or char literal that contains `pos` (or is
/// unterminated before it), found by lexing from the start of the input.
fn enclosing_literal(input: &str, pos: usize) -> Option<usize> {
    let b = input.as_bytes();
    let mut i = 0;
    while i < b.len() && i <= pos {
        let rest = &input[i..];
        if rest.starts_with("//") {
            i += rest.find('\n').unwrap_or(rest.len());
        } else if rest.starts_with("/*") {
            let mut depth = 0;
            let mut k = i;
            while k < b.len() {
                if input[k..].starts_with("/*") {
                    depth += 1;
                    k += 2;
                } else if input[k..].starts_with("*/") {
                    depth -= 1;
                    k += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    k += 1;
                }
            }
            i = k;
        } else if let Some(end) = literal_end(input, i) {
            match end {
                // `pos == end` counts as inside: pest's furthest attempt
                // on a malformed literal can land just past its close.
                Some(end) if pos > end => i = end,
                _ => return Some(i),
            }
        } else if b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] >= 0x80 {
            // Skip a whole identifier or number, so its `r`/`b` letters are
            // not taken for a raw or byte string prefix.
            let len = rest
                .find(|c: char| !(c.is_alphanumeric() || c == '_'))
                .unwrap_or(rest.len());
            i += len.max(1);
            while !input.is_char_boundary(i) {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    None
}

/// If a string or char literal starts at `i`: `Some(Some(end))` just past
/// it, or `Some(None)` if it is unterminated. `None` if no literal starts
/// here.
fn literal_end(input: &str, i: usize) -> Option<Option<usize>> {
    let rest = &input[i..];
    if rest.starts_with("\'\'\'") {
        return Some(Some(i + 3));
    }
    let (quote, body) = if rest.starts_with('"') || rest.starts_with('\'') {
        (rest.as_bytes()[0], i + 1)
    } else if rest.starts_with("b\"") || rest.starts_with("b'") {
        (rest.as_bytes()[1], i + 2)
    } else {
        let raw = rest.strip_prefix("br").or_else(|| rest.strip_prefix('r'))?;
        let hashes = raw.len() - raw.trim_start_matches('#').len();
        if !raw[hashes..].starts_with('"') {
            return None;
        }
        let close = format!("\"{}", "#".repeat(hashes));
        let body = i + (rest.len() - raw.len()) + hashes + 1;
        return Some(input[body..].find(&close).map(|k| body + k + close.len()));
    };
    let b = input.as_bytes();
    let mut k = body;
    while k < b.len() {
        match b[k] {
            b'\\' => k += 2,
            c if c == quote => return Some(Some(k + 1)),
            _ => k += 1,
        }
    }
    Some(None)
}

/// A string or char literal starting at `pos` that is unterminated or
/// holds an invalid escape: the offset to point at and the message.
fn literal_problem(input: &str, pos: usize) -> Option<(usize, String)> {
    let rest = &input[pos..];
    let (quote, body_start) = if rest.starts_with('"') || rest.starts_with('\'') {
        (rest.as_bytes()[0], pos + 1)
    } else if rest.starts_with("b\"") || rest.starts_with("b'") {
        (rest.as_bytes()[1], pos + 2)
    } else {
        let raw = rest.strip_prefix("br").or_else(|| rest.strip_prefix('r'))?;
        let hashes = raw.len() - raw.trim_start_matches('#').len();
        if !raw[hashes..].starts_with('"') {
            return None;
        }
        let close = format!("\"{}", "#".repeat(hashes));
        let body = &raw[hashes + 1..];
        return (!body.contains(&close)).then(|| (pos, "unterminated raw string literal".into()));
    };
    let kind = if quote == b'"' { "string" } else { "char" };
    let bytes = input.as_bytes();
    let mut i = body_start;
    // Characters in the body so far (an escape counts as one).
    let mut chars = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                if let Some(len) = valid_escape(&input[i + 1..]) {
                    i += 1 + len;
                } else {
                    let esc: String = input[i..].chars().take(2).collect();
                    return Some((i, format!("invalid escape `{esc}` in {kind} literal")));
                }
            }
            b if b == quote => {
                return (kind == "char" && chars != 1).then(|| {
                    (
                        pos,
                        "a char literal holds exactly one character; use \"…\" for a string".into(),
                    )
                });
            }
            _ => i += input[i..].chars().next().map_or(1, char::len_utf8),
        }
        chars += 1;
    }
    Some((pos, format!("unterminated {kind} literal")))
}

/// Length of a valid escape body (after the backslash), if it is one.
fn valid_escape(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    match b.first()? {
        b'n' | b'r' | b't' | b'\\' | b'\'' | b'"' | b'0' => Some(1),
        b'x' if b.len() >= 3 && b[1..3].iter().all(u8::is_ascii_hexdigit) => Some(3),
        b'u' if b.get(1) == Some(&b'{') => {
            let digits = b[2..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
            ((1..=6).contains(&digits) && b.get(2 + digits) == Some(&b'}')).then_some(3 + digits)
        }
        _ => None,
    }
}
