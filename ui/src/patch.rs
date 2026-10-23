//! Textual edits to a `fmt.ron` that keep everything but the edited entry:
//! comments, blank lines, the other entries, attributes.
//!
//! The file is one RON struct: optional `#![enable(..)]` attributes, an
//! optional name, `(`, `name: value` entries separated by commas, `)`.
//! [`scan`] finds the byte spans of the entries; [`set`] and [`remove`]
//! splice the text. The result is meant to be run through fmtron, which
//! tidies the layout and keeps the comments.

use std::ops::Range;

/// One `name: value` entry of the struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    /// From the first byte of the name to just past the trailing comma, or
    /// past the value if there is none.
    pub span: Range<usize>,
    /// The value alone, without surrounding whitespace or comments.
    pub value: Range<usize>,
}

/// The struct's brackets and entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Struct {
    /// Index of `(`.
    pub open: usize,
    /// Index of `)`.
    pub close: usize,
    pub entries: Vec<Entry>,
}

/// Indentation used for an entry added to a struct that has none to copy.
const INDENT: &str = "    ";

/// Locates the entries of the struct in `text`. `None` if the text is not
/// such a struct; the caller then has nothing to patch.
pub fn scan(text: &str) -> Option<Struct> {
    let b = text.as_bytes();
    let mut i = skip_trivia(b, 0);
    while b.get(i) == Some(&b'#') && b.get(i + 1) == Some(&b'!') {
        i = skip_bracketed(b, i + 2)?;
        i = skip_trivia(b, i);
    }
    while i < b.len() && is_ident(b[i]) {
        i += 1;
    }
    i = skip_trivia(b, i);
    if b.get(i) != Some(&b'(') {
        return None;
    }
    let open = i;
    i += 1;
    let mut entries = Vec::new();
    loop {
        i = skip_trivia(b, i);
        if *b.get(i)? == b')' {
            return Some(Struct {
                open,
                close: i,
                entries,
            });
        }
        let name_start = i;
        while i < b.len() && is_ident(b[i]) {
            i += 1;
        }
        if i == name_start {
            return None;
        }
        let name = text[name_start..i].to_string();
        i = skip_trivia(b, i);
        if b.get(i) != Some(&b':') {
            return None;
        }
        i = skip_trivia(b, i + 1);
        let value_start = i;
        let mut value_end = i;
        let mut depth = 0usize;
        loop {
            let j = skip_trivia(b, i);
            if j != i {
                i = j;
                continue;
            }
            match *b.get(i)? {
                b',' | b')' if depth == 0 => break,
                b']' | b'}' if depth == 0 => return None,
                b'(' | b'[' | b'{' => {
                    depth += 1;
                    i += 1;
                }
                b')' | b']' | b'}' => {
                    depth -= 1;
                    i += 1;
                }
                b'"' => i = skip_string(b, i)?,
                b'r' if is_raw_string(b, i) => i = skip_raw_string(b, i)?,
                b'\'' => i = skip_char(b, i)?,
                _ => i += 1,
            }
            value_end = i;
        }
        if value_end == value_start {
            return None;
        }
        let mut span_end = value_end;
        if b.get(i) == Some(&b',') {
            i += 1;
            span_end = i;
        }
        entries.push(Entry {
            name,
            span: name_start..span_end,
            value: value_start..value_end,
        });
    }
}

/// `text` with the entry `name` set to `literal`: its value replaced, or
/// the entry added at the end of the struct. A `Some(..)` around the old
/// value is kept.
pub fn set(text: &str, name: &str, literal: &str) -> Option<String> {
    let s = scan(text)?;
    let mut out = text.to_string();
    if let Some(e) = s.entries.iter().find(|e| e.name == name) {
        let old = &text[e.value.clone()];
        let new = if is_some_wrapped(old) {
            format!("Some({literal})")
        } else {
            literal.to_string()
        };
        out.replace_range(e.value.clone(), &new);
        return Some(out);
    }
    let indent = s
        .entries
        .last()
        .map_or(INDENT, |e| line_indent(text, e.span.start));
    let mut close = s.close;
    if let Some(last) = s.entries.last()
        && !text[last.span.clone()].ends_with(',')
    {
        out.insert(last.span.end, ',');
        close += 1;
    }
    // Insert on a line of its own just before `)`, so that a comment after
    // the last entry stays with that entry.
    let mut at = close;
    while at > 0 && matches!(out.as_bytes()[at - 1], b' ' | b'\t') {
        at -= 1;
    }
    let at_line_start = at == 0 || out.as_bytes()[at - 1] == b'\n';
    let piece = if at_line_start {
        format!("{indent}{name}: {literal},\n")
    } else {
        format!("\n{indent}{name}: {literal},\n")
    };
    out.insert_str(at, &piece);
    Some(out)
}

/// `text` without the entry `name`. An entry alone on its line takes the
/// line with it, including a comment after it on that line; otherwise only
/// the entry goes. `None` if the text is not a struct; the text unchanged if
/// the entry is absent.
pub fn remove(text: &str, name: &str) -> Option<String> {
    let s = scan(text)?;
    let mut out = text.to_string();
    let Some(e) = s.entries.iter().find(|e| e.name == name) else {
        return Some(out);
    };
    let line_start = text[..e.span.start].rfind('\n').map_or(0, |n| n + 1);
    let alone_before = text[line_start..e.span.start].trim().is_empty();
    let line_end = text[e.span.end..]
        .find('\n')
        .map_or(text.len(), |n| e.span.end + n);
    let rest = text[e.span.end..line_end].trim_start();
    // Everything after the entry on this line is trivia (whitespace or
    // comments), possibly with nested block comments.
    let only_comment_after = skip_trivia(rest.as_bytes(), 0) == rest.len();
    if alone_before && only_comment_after {
        let end = if line_end < text.len() {
            line_end + 1
        } else {
            line_end
        };
        out.replace_range(line_start..end, "");
    } else {
        let mut end = e.span.end;
        while matches!(text.as_bytes().get(end), Some(b' ' | b'\t')) {
            end += 1;
        }
        out.replace_range(e.span.start..end, "");
    }
    Some(out)
}

fn is_some_wrapped(value: &str) -> bool {
    value
        .strip_prefix("Some")
        .is_some_and(|rest| rest.trim_start().starts_with('('))
}

/// The whitespace before `pos` on its line, if nothing else precedes it
/// there; else the standard indentation.
fn line_indent(text: &str, pos: usize) -> &str {
    let line_start = text[..pos].rfind('\n').map_or(0, |n| n + 1);
    let before = &text[line_start..pos];
    if before.trim().is_empty() {
        before
    } else {
        INDENT
    }
}

fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Past whitespace and comments (line, and nested block).
fn skip_trivia(b: &[u8], mut i: usize) -> usize {
    loop {
        match (b.get(i), b.get(i + 1)) {
            (Some(c), _) if c.is_ascii_whitespace() => i += 1,
            (Some(b'/'), Some(b'/')) => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
            }
            (Some(b'/'), Some(b'*')) => {
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
            }
            _ => return i,
        }
    }
}

/// Past a `[..]` starting at `i`, with nesting and strings; `None` if
/// unterminated.
fn skip_bracketed(b: &[u8], mut i: usize) -> Option<usize> {
    if b.get(i) != Some(&b'[') {
        return None;
    }
    let mut depth = 0usize;
    loop {
        let j = skip_trivia(b, i);
        if j != i {
            i = j;
            continue;
        }
        match *b.get(i)? {
            b'[' => {
                depth += 1;
                i += 1;
            }
            b']' => {
                depth -= 1;
                i += 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            b'"' => i = skip_string(b, i)?,
            b'r' if is_raw_string(b, i) => i = skip_raw_string(b, i)?,
            _ => i += 1,
        }
    }
}

/// Past the `"…"` starting at `i`.
fn skip_string(b: &[u8], mut i: usize) -> Option<usize> {
    i += 1;
    loop {
        match *b.get(i)? {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
}

fn is_raw_string(b: &[u8], mut i: usize) -> bool {
    i += 1;
    while b.get(i) == Some(&b'#') {
        i += 1;
    }
    b.get(i) == Some(&b'"')
}

/// Past the `r#"…"#` starting at `i`.
fn skip_raw_string(b: &[u8], mut i: usize) -> Option<usize> {
    i += 1;
    let mut hashes = 0;
    while b.get(i) == Some(&b'#') {
        hashes += 1;
        i += 1;
    }
    i += 1;
    loop {
        if *b.get(i)? == b'"'
            && b[i + 1..]
                .iter()
                .take(hashes)
                .filter(|&&c| c == b'#')
                .count()
                == hashes
        {
            return Some(i + 1 + hashes);
        }
        i += 1;
    }
}

/// Past the `'…'` starting at `i`.
fn skip_char(b: &[u8], mut i: usize) -> Option<usize> {
    i += 1;
    if b.get(i) == Some(&b'\\') {
        i += 2;
    }
    loop {
        match *b.get(i)? {
            b'\'' => return Some(i + 1),
            _ => i += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "\
// fmt.ron
(
    max_width: 100, // -w
    tab_size: Some(2),
    /* Keep | Remove */ blank_lines: Keep,
)
";

    #[test]
    fn scans_entries_with_comments_around() {
        let s = scan(FILE).unwrap();
        let names: Vec<_> = s.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["max_width", "tab_size", "blank_lines"]);
        assert_eq!(&FILE[s.entries[0].value.clone()], "100");
        assert_eq!(&FILE[s.entries[1].value.clone()], "Some(2)");
        assert_eq!(&FILE[s.entries[2].span.clone()], "blank_lines: Keep,");
        assert_eq!(&FILE[s.open..=s.open], "(");
        assert_eq!(&FILE[s.close..=s.close], ")");
    }

    #[test]
    fn scans_attributes_name_and_nested_values() {
        let text =
            "#![enable(implicit_some)] FileConfig(a: (1, \"),\"), b: [')', r#\")\"#] /* , */)";
        let s = scan(text).unwrap();
        assert_eq!(&text[s.entries[0].value.clone()], "(1, \"),\")");
        assert_eq!(&text[s.entries[1].value.clone()], "[')', r#\")\"#]");
        assert_eq!(s.entries[1].span.end, s.entries[1].value.end);
    }

    #[test]
    fn scans_attributes_with_raw_strings_and_comments() {
        for text in [
            "#![type = r#\"a\" ] b\"#] (a: 1)",
            "#![type = r###\"a\"## ] b\"###] (a: 1)",
            "#![type /* ] */ = \"a]b\"] (a: 1)",
            "#![type // ]\n = \"a]b\"] (a: 1)",
        ] {
            let s = scan(text).unwrap();
            assert_eq!(&text[s.entries[0].value.clone()], "1", "{text}");
        }
    }

    #[test]
    fn not_a_struct_is_none() {
        assert_eq!(scan(""), None);
        assert_eq!(scan("[1, 2]"), None);
        assert_eq!(scan("(a: 1"), None);
        assert_eq!(scan("(a 1)"), None);
    }

    #[test]
    fn replacing_keeps_comments_and_some() {
        let out = set(FILE, "max_width", "80").unwrap();
        assert!(out.contains("max_width: 80, // -w"), "{out}");
        let out = set(FILE, "tab_size", "4").unwrap();
        assert!(out.contains("tab_size: Some(4),"), "{out}");
        assert!(out.contains("/* Keep | Remove */ blank_lines: Keep,"));
    }

    #[test]
    fn adding_goes_after_the_last_entry_and_its_comment() {
        let out = set("(\n    a: 1, // about a\n)\n", "b", "2").unwrap();
        assert_eq!(out, "(\n    a: 1, // about a\n    b: 2,\n)\n");
        let out = set("(a: 1)", "b", "2").unwrap();
        assert_eq!(out, "(a: 1,\n    b: 2,\n)");
        let out = set("()", "a", "1").unwrap();
        assert_eq!(out, "(\n    a: 1,\n)");
        let out = set("(\n  a: 1\n  )", "b", "2").unwrap();
        assert_eq!(out, "(\n  a: 1,\n  b: 2,\n  )");
    }

    #[test]
    fn removing_an_entry_on_its_own_line_takes_the_line() {
        let out = remove(FILE, "max_width").unwrap();
        assert!(
            !out.contains("max_width") && !out.contains("// -w"),
            "{out}"
        );
        assert!(out.contains("// fmt.ron\n(\n    tab_size"), "{out}");
        let out = remove(FILE, "blank_lines").unwrap();
        assert!(!out.contains("blank_lines"), "{out}");
        assert!(out.contains("/* Keep | Remove */ "), "{out}");
        assert_eq!(remove(FILE, "missing").unwrap(), FILE);
    }

    #[test]
    fn removing_an_entry_takes_a_nested_block_comment_after_it() {
        let text = "(\n    a: 1, /* outer /* inner */ still outer */\n    b: 2,\n)\n";
        assert_eq!(remove(text, "a").unwrap(), "(\n    b: 2,\n)\n");
    }

    #[test]
    fn removing_an_inline_entry_leaves_the_rest_of_the_line() {
        assert_eq!(remove("(a: 1, b: 2)", "a").unwrap(), "(b: 2)");
        assert_eq!(remove("(a: 1, b: 2)", "b").unwrap(), "(a: 1, )");
        assert_eq!(
            remove("(a: 1, /* x */ b: 2,\n c: 3)", "a").unwrap(),
            "(/* x */ b: 2,\n c: 3)"
        );
    }
}
