//! Syntax colouring of RON: comments, strings, numbers, keywords, field
//! names, and everything else, as a [`LayoutJob`] for a label or an editor.

use egui::text::{LayoutJob, TextFormat};
use egui::{Color32, FontId};

use super::theme::P;

/// What a stretch of RON is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Comment,
    String,
    Number,
    /// `true`, `false`, `Some`, `None`, `inf`, `NaN`.
    Keyword,
    /// An identifier before `:`.
    Field,
    /// Any other identifier: a struct or variant name.
    Ident,
    /// Brackets and separators.
    Punct,
    Plain,
}

impl Kind {
    fn color(self) -> Color32 {
        match self {
            Self::Comment => P.comment,
            Self::String => P.green,
            Self::Number => P.number,
            Self::Keyword => P.red,
            Self::Field => P.label,
            Self::Ident => P.yellow,
            Self::Punct => P.weak,
            Self::Plain => P.text,
        }
    }
}

/// Lays `text` out in `font`, coloured, wrapping at `wrap_width`.
pub fn layout(text: &str, font: FontId, wrap_width: f32) -> LayoutJob {
    let mut job = LayoutJob {
        wrap: egui::text::TextWrapping {
            max_width: wrap_width,
            ..Default::default()
        },
        ..Default::default()
    };
    for (kind, range) in tokens(text) {
        job.append(
            &text[range],
            0.0,
            TextFormat {
                font_id: font.clone(),
                color: kind.color(),
                ..Default::default()
            },
        );
    }
    job
}

/// The text cut into stretches of one kind each, covering all of it.
fn tokens(text: &str) -> Vec<(Kind, std::ops::Range<usize>)> {
    let b = text.as_bytes();
    let mut out: Vec<(Kind, std::ops::Range<usize>)> = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let start = i;
        let kind = match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                Kind::Comment
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                i = block_comment_end(b, i);
                Kind::Comment
            }
            b'"' => {
                i = string_end(b, i);
                Kind::String
            }
            b'r' if raw_string_hashes(b, i).is_some() => {
                i = raw_string_end(b, i);
                Kind::String
            }
            b'\'' => {
                i = char_end(b, i);
                Kind::String
            }
            c if c.is_ascii_digit() || (matches!(c, b'-' | b'+') && next_is_digit(b, i)) => {
                i += 1;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || matches!(b[i], b'.' | b'_')) {
                    i += 1;
                }
                Kind::Number
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                let word = &text[start..i];
                if matches!(word, "true" | "false" | "Some" | "None" | "inf" | "NaN") {
                    Kind::Keyword
                } else if followed_by_colon(b, i) {
                    Kind::Field
                } else {
                    Kind::Ident
                }
            }
            b'(' | b')' | b'[' | b']' | b'{' | b'}' | b',' | b':' => {
                i += 1;
                Kind::Punct
            }
            _ => {
                // One UTF-8 character of anything else.
                i += utf8_len(b[i]);
                Kind::Plain
            }
        };
        // Merge with the previous stretch of the same kind.
        match out.last_mut() {
            Some((last, range)) if *last == kind => range.end = i,
            _ => out.push((kind, start..i)),
        }
    }
    out
}

fn utf8_len(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

fn next_is_digit(b: &[u8], i: usize) -> bool {
    b.get(i + 1).is_some_and(u8::is_ascii_digit)
}

fn followed_by_colon(b: &[u8], mut i: usize) -> bool {
    while i < b.len() && (b[i] == b' ' || b[i] == b'\t') {
        i += 1;
    }
    b.get(i) == Some(&b':')
}

/// Past the nested `/* … */` at `i`, or to the end if unterminated.
fn block_comment_end(b: &[u8], mut i: usize) -> usize {
    i += 2;
    let mut depth = 1;
    while depth > 0 && i < b.len() {
        match (b[i], b.get(i + 1)) {
            (b'/', Some(b'*')) => {
                depth += 1;
                i += 2;
            }
            (b'*', Some(b'/')) => {
                depth -= 1;
                i += 2;
            }
            _ => i += 1,
        }
    }
    i
}

/// Past the `"…"` at `i`, or to the end if unterminated.
fn string_end(b: &[u8], mut i: usize) -> usize {
    i += 1;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    b.len()
}

/// The number of `#` in the raw string starting at `i`, if one starts there.
fn raw_string_hashes(b: &[u8], mut i: usize) -> Option<usize> {
    i += 1;
    let mut hashes = 0;
    while b.get(i) == Some(&b'#') {
        hashes += 1;
        i += 1;
    }
    (b.get(i) == Some(&b'"')).then_some(hashes)
}

/// Past the `r#"…"#` at `i`, or to the end if unterminated.
fn raw_string_end(b: &[u8], mut i: usize) -> usize {
    let hashes = raw_string_hashes(b, i).unwrap_or(0);
    i += 2 + hashes;
    while i < b.len() {
        if b[i] == b'"' && b[i + 1..].iter().take_while(|&&c| c == b'#').count() >= hashes {
            return i + 1 + hashes;
        }
        i += 1;
    }
    b.len()
}

/// Past the `'…'` at `i`, or to the end if unterminated.
fn char_end(b: &[u8], mut i: usize) -> usize {
    i += 1;
    if b.get(i) == Some(&b'\\') {
        i += 2;
    }
    while i < b.len() {
        if b[i] == b'\'' {
            return i + 1;
        }
        i += 1;
    }
    b.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<(Kind, &str)> {
        tokens(text)
            .into_iter()
            .map(|(k, r)| (k, &text[r]))
            .collect()
    }

    #[test]
    fn classifies_a_struct() {
        let text = "Level(name: \"x\", n: -1.5, ok: true) // c";
        assert_eq!(
            kinds(text),
            [
                (Kind::Ident, "Level"),
                (Kind::Punct, "("),
                (Kind::Field, "name"),
                (Kind::Punct, ":"),
                (Kind::Plain, " "),
                (Kind::String, "\"x\""),
                (Kind::Punct, ","),
                (Kind::Plain, " "),
                (Kind::Field, "n"),
                (Kind::Punct, ":"),
                (Kind::Plain, " "),
                (Kind::Number, "-1.5"),
                (Kind::Punct, ","),
                (Kind::Plain, " "),
                (Kind::Field, "ok"),
                (Kind::Punct, ":"),
                (Kind::Plain, " "),
                (Kind::Keyword, "true"),
                (Kind::Punct, ")"),
                (Kind::Plain, " "),
                (Kind::Comment, "// c"),
            ]
        );
    }

    #[test]
    fn strings_comments_and_chars_contain_anything() {
        let text = "[\"a\\\"b\", r#\"c\"d\"#, '\\'', /* x /* y */ z */ 0x1f]";
        let k = kinds(text);
        assert!(k.contains(&(Kind::String, "\"a\\\"b\"")), "{k:?}");
        assert!(k.contains(&(Kind::String, "r#\"c\"d\"#")), "{k:?}");
        assert!(k.contains(&(Kind::String, "'\\''")), "{k:?}");
        assert!(k.contains(&(Kind::Comment, "/* x /* y */ z */")), "{k:?}");
        assert!(k.contains(&(Kind::Number, "0x1f")), "{k:?}");
    }

    #[test]
    fn covers_the_whole_text_including_unterminated_pieces() {
        for text in ["\"open", "/* open", "r#\"open", "'o", "héllo: 1"] {
            let covered: usize = tokens(text).iter().map(|(_, r)| r.len()).sum();
            assert_eq!(covered, text.len(), "{text}");
        }
    }
}
