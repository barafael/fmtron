use super::{Attribute, BLANK_LINE, Field, HeaderItem, Kind, RonFile, Value};
use crate::MAX_INDENT;
use crate::pretty::{
    Doc, FillItem, comma, concat, fill, group, hard_line, hug, line, nest, render_with_newline,
    soft_line, text,
};
use std::fmt::{self, Display, Formatter};
use unicode_width::UnicodeWidthStr;

impl Display for RonFile {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let Self {
            header,
            value,
            dangling,
            config,
            newline: nl,
        } = self;
        for item in header {
            match item {
                HeaderItem::Comment(text) => {
                    write!(f, "{}{nl}", comment_lines(text, config.tab_size).join(nl))?;
                }
                HeaderItem::Attribute(attr) => {
                    write!(f, "{}{nl}", AttributeDisplay(attr, nl))?;
                }
            }
        }
        let doc = concat(vec![
            value_doc(value, config.tab_size),
            trailing_doc(value, config.tab_size),
        ]);
        write!(f, "{}", render_with_newline(&doc, config.max_width, nl))?;
        // The rendered value ends without a line break, and so does the whole
        // output: the value's line needs one only when dangling comments come
        // after it (inline trailing comments sit at its end).
        if !dangling.is_empty() {
            write!(f, "{nl}")?;
            let comments: Vec<String> = dangling
                .iter()
                .map(|c| comment_lines(c, config.tab_size).join(nl))
                .collect();
            write!(f, "{}", comments.join(nl))?;
        }
        Ok(())
    }
}

struct AttributeDisplay<'a>(&'a Attribute, &'a str);

impl Display for AttributeDisplay<'_> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.0 {
            Attribute::Enable(ids) => write!(f, "#![enable({})]", ids.join(", ")),
            Attribute::Type(s) => write!(f, "#![type = {s}]"),
            Attribute::Schema(s) => write!(f, "#![schema = {s}]"),
            Attribute::Verbatim(s) => write!(f, "{}", normalized_breaks(s, self.1)),
        }
    }
}

/// A verbatim attribute's text with its line breaks rewritten to `nl` and the
/// spaces before them dropped, so that an attribute cannot leave a line
/// ending, or a trailing space, of its own behind. A break inside a string,
/// char or raw literal is part of that literal's value and is copied as it
/// stands; one inside a comment is layout and follows the file.
fn normalized_breaks(text: &str, nl: &str) -> String {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    // The run of ordinary text not copied to `out` yet. It is flushed only at
    // a byte that is ASCII, which is always a character boundary, so slicing
    // `text[keep..i]` is safe however the scan got to `i`.
    let mut keep = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\n' || (b[i] == b'\r' && b.get(i + 1) == Some(&b'\n')) {
            out.push_str(&text[keep..i]);
            drop_spaces_before_break(&mut out);
            out.push_str(nl);
            i += if b[i] == b'\r' { 2 } else { 1 };
            keep = i;
        } else if let Some(end) = literal_end(b, i) {
            i = end;
        } else if b[i..].starts_with(b"//") {
            // Stop before the break that ends the comment, so that a quote in
            // it starts no literal; the break itself is handled above.
            let mut stop = b[i..]
                .iter()
                .position(|&c| c == b'\n')
                .map_or(b.len(), |n| i + n);
            if stop > i && b[stop - 1] == b'\r' {
                stop -= 1;
            }
            i = stop;
        } else if b[i..].starts_with(b"/*") {
            out.push_str(&text[keep..i]);
            i += push_block_comment(&text[i..], nl, &mut out);
            keep = i;
        } else {
            i += 1;
        }
    }
    out.push_str(&text[keep..]);
    out
}

/// The index just past the string, char or raw literal starting at `i`, or
/// `None` when no literal starts there.
fn literal_end(b: &[u8], i: usize) -> Option<usize> {
    match b[i] {
        b'"' => crate::scan_quoted(b, i),
        b'\'' => crate::scan_char(b, i),
        b'b' | b'r' => crate::scan_raw(b, i),
        _ => None,
    }
}

/// Copies the block comment that `text` starts with, with its line breaks
/// rewritten to `nl` and the spaces before them dropped, and returns the
/// number of bytes consumed — its whole length, or the end of `text` if it is
/// unterminated.
fn push_block_comment(text: &str, nl: &str, out: &mut String) -> usize {
    let b = text.as_bytes();
    let mut depth = 0usize;
    let mut keep = 0;
    let mut i = 0;
    while i < b.len() {
        if b[i..].starts_with(b"/*") {
            depth += 1;
            i += 2;
        } else if b[i..].starts_with(b"*/") {
            depth = depth.saturating_sub(1);
            i += 2;
            if depth == 0 {
                out.push_str(&text[keep..i]);
                return i;
            }
        } else if b[i] == b'\n' || (b[i] == b'\r' && b.get(i + 1) == Some(&b'\n')) {
            out.push_str(&text[keep..i]);
            drop_spaces_before_break(out);
            out.push_str(nl);
            i += if b[i] == b'\r' { 2 } else { 1 };
            keep = i;
        } else {
            i += 1;
        }
    }
    out.push_str(&text[keep..]);
    i
}

/// Drops the spaces a line ends with, just before its break.
fn drop_spaces_before_break(out: &mut String) {
    while out.ends_with(' ') || out.ends_with('\t') {
        out.pop();
    }
}

/// True if `v` or anything in its subtree must break: it carries comments,
/// or is a struct or map the input breaks after its opening bracket. A
/// container whose subtree must break renders every element on its own line
/// (hard breaks), otherwise a leading/trailing comment would collide with
/// sibling layout.
fn subtree_breaks(v: &Value) -> bool {
    if !v.leading.is_empty() || !v.inline.is_empty() || !v.trailing.is_empty() {
        return true;
    }
    match &v.kind {
        Kind::Atom { .. } => false,
        Kind::List { values, dangling } => {
            !dangling.is_empty()
                || packing(values, dangling) == Packing::Grid
                || values.iter().any(subtree_breaks)
        }
        Kind::Map {
            entries,
            dangling,
            broken,
        } => {
            *broken
                || !dangling.is_empty()
                || entries.iter().flat_map(|(k, v)| [k, v]).any(subtree_breaks)
        }
        Kind::TupleType {
            values, dangling, ..
        } => {
            !dangling.is_empty()
                || packing(values, dangling) == Packing::Grid
                || values.iter().any(subtree_breaks)
        }
        Kind::FieldsType {
            fields,
            dangling,
            broken,
            ..
        } => *broken || !dangling.is_empty() || fields.iter().any(|f| subtree_breaks(&f.value)),
    }
}

/// A container element goes on its own line (hard breaks, plain commas) if
/// the container holds dangling comments or any member subtree must break.
fn force_break<'a>(dangling: &[String], members: impl IntoIterator<Item = &'a Value>) -> bool {
    !dangling.is_empty() || members.into_iter().any(subtree_breaks)
}

/// The widest list element, in columns, that may share a line with its
/// siblings (see [`packing`]). Numbers, bools and chars may be any width.
const SHORT_ITEM_WIDTH: usize = 16;

/// True if `v` may share a line with its siblings in a list or tuple: a
/// number, bool or char, or another atom or a tuple (named or not) of atoms
/// at most [`SHORT_ITEM_WIDTH`] columns wide; without comments. A blank line
/// before it is fine.
fn short_item(v: &Value) -> bool {
    let plain = |v: &Value| v.inline.is_empty() && v.trailing.is_empty();
    // The width on one line: measured from the text, since a tuple the input
    // writes as a grid renders on several lines.
    let width = match &v.kind {
        Kind::Atom { scalar: true, .. } => Some(0),
        Kind::Atom { text, .. } => Some(text.width()),
        Kind::TupleType {
            ident,
            values,
            dangling,
        } if dangling.is_empty()
            && !values.is_empty()
            && values.iter().all(|x| x.leading.is_empty() && plain(x)) =>
        {
            values.iter().try_fold(
                ident.as_deref().map_or(0, str::width)
                    + "()".len()
                    + ", ".len() * (values.len() - 1),
                |w, x| match &x.kind {
                    Kind::Atom { text, .. } => Some(w + text.width()),
                    _ => None,
                },
            )
        }
        _ => None,
    };
    width.is_some_and(|w| w <= SHORT_ITEM_WIDTH)
        && plain(v)
        && v.leading.iter().all(|c| c == BLANK_LINE)
}

/// True if `v` is a number, bool or char, or a tuple of only those.
fn scalar_item(v: &Value) -> bool {
    let scalar = |k: &Kind| matches!(k, Kind::Atom { scalar: true, .. });
    match &v.kind {
        Kind::TupleType { values, .. } => values.iter().all(|x| scalar(&x.kind)),
        k => scalar(k),
    }
}

/// Whether a list or tuple's elements are packed several to a line (see
/// [`packing`]).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Packing {
    No,
    /// Packed by width alone.
    Fill,
    /// Packed, keeping every line break the input has between elements when
    /// broken.
    Rows,
    /// Like `Rows`, and always broken.
    Grid,
}

/// How a list or tuple of short elements (see [`short_item`]) is packed:
///
/// - If the input already puts two or more of them on one line, they stay
///   packed, and when the container breaks, every line break the input has
///   between them is kept: rows stay rows, and an edit to one row never
///   reflows the others. Only a row that grows too long is wrapped. If the
///   input also breaks the line right after the opening bracket, it is a
///   grid: it stays broken even if it would fit on one line.
/// - Otherwise, numbers, bools and chars (and tuples of those) are packed
///   by width, even when written one per line.
/// - Anything else, such as strings written one per line, is not packed: one
///   element per line when broken.
///
/// A blank line between elements is kept and starts a new line.
fn packing(values: &[Value], dangling: &[String]) -> Packing {
    if values.len() < 2 || !dangling.is_empty() || !values.iter().all(short_item) {
        return Packing::No;
    }
    // Some input line holds two elements: one other than the first that
    // neither starts a line nor follows a blank line.
    let packed = values[1..]
        .iter()
        .any(|e| !e.starts_line && e.leading.is_empty());
    let rows = values[1..].iter().any(|e| e.starts_line);
    match (packed, rows) {
        (true, true) if values[0].starts_line => Packing::Grid,
        (true, true) => Packing::Rows,
        (true, false) => Packing::Fill,
        (false, _) if values.iter().all(scalar_item) => Packing::Fill,
        (false, _) => Packing::No,
    }
}

/// A list or tuple packed per [`packing`], or `None` if it is not packed.
fn packed(
    values: &[Value],
    dangling: &[String],
    force: bool,
    open: &str,
    close: &str,
    tab: usize,
) -> Option<Doc> {
    let packing = packing(values, dangling);
    if packing == Packing::No {
        return None;
    }
    let items = values
        .iter()
        .map(|e| FillItem {
            blank_before: !e.leading.is_empty(),
            starts_line: matches!(packing, Packing::Rows | Packing::Grid) && e.starts_line,
            doc: kind_doc(e, tab),
        })
        .collect();
    // A grid or a blank line between elements: always broken.
    let force = force || packing == Packing::Grid;
    let brk = || if force { hard_line() } else { soft_line() };
    let doc = concat(vec![
        text(open),
        nest(tab, concat(vec![brk(), fill(items, force)])),
        brk(),
        text(close),
    ]);
    Some(if force { doc } else { group(doc) })
}

/// Comma after an element: plain when the container is forced to break or the
/// element is not last; conditional on the group breaking otherwise.
fn item_sep(force: bool, is_last: bool) -> Doc {
    if force || !is_last {
        text(",")
    } else {
        comma()
    }
}

/// One container element: its rendered segments plus separator and inline
/// trailing comments.
fn item_doc(segments: Vec<Doc>, v: &Value, force: bool, is_last: bool, tab: usize) -> Doc {
    let mut parts = segments;
    parts.push(item_sep(force, is_last));
    parts.push(trailing_doc(v, tab));
    concat(parts)
}

/// The opening token of a tuple/struct: `Ident(` or just `(`.
fn open_ident(ident: Option<&str>) -> String {
    match ident {
        Some(id) => format!("{id}("),
        None => "(".to_string(),
    }
}

/// The lines of a comment. A later line's leading tabs (its indentation
/// relative to the comment's line) become `tab` spaces each, like all other
/// indentation, unless that would exceed [`MAX_INDENT`].
fn comment_lines(c: &str, tab: usize) -> Vec<String> {
    c.split('\n')
        .enumerate()
        .map(|(i, line)| {
            let lead = &line[..line.len() - line.trim_start_matches([' ', '\t']).len()];
            let width = lead.chars().fold(0usize, |w, c| {
                w.saturating_add(if c == '\t' { tab } else { 1 })
            });
            if i == 0 || !lead.contains('\t') || width > MAX_INDENT {
                line.to_string()
            } else {
                format!("{}{}", " ".repeat(width), &line[lead.len()..])
            }
        })
        .collect()
}

/// A comment. The lines of a multi-line block comment are joined by hard
/// breaks, so they take the current indentation and the file's line ending.
fn comment_doc(c: &str, tab: usize) -> Doc {
    let mut parts: Vec<Doc> = Vec::new();
    for (i, line) in comment_lines(c, tab).into_iter().enumerate() {
        if i > 0 {
            parts.push(hard_line());
        }
        parts.push(text(line));
    }
    concat(parts)
}

/// Leading comments, each on its own line before the value.
fn leading_doc(v: &Value, tab: usize) -> Doc {
    let mut parts: Vec<Doc> = Vec::new();
    for c in &v.leading {
        parts.push(comment_doc(c, tab));
        parts.push(hard_line());
    }
    concat(parts)
}

/// Line comments between a key/name and its value, hoisted onto their own
/// lines before the entry: each must end its line, and the value belongs on
/// the key's line.
fn hoisted_doc(v: &Value) -> Doc {
    let mut parts: Vec<Doc> = Vec::new();
    for c in v.inline.iter().filter(|c| !c.starts_with("/*")) {
        parts.push(text(c.clone()));
        parts.push(hard_line());
    }
    concat(parts)
}

/// Block comments between a key/name and its value, kept inline after the
/// colon: `delay: /* seconds */ 5`.
fn inline_doc(v: &Value, tab: usize) -> Doc {
    concat(
        v.inline
            .iter()
            .filter(|c| c.starts_with("/*"))
            .map(|c| concat(vec![comment_doc(c, tab), text(" ")]))
            .collect(),
    )
}

/// Trailing comments, inline after the value/comma.
fn trailing_doc(v: &Value, tab: usize) -> Doc {
    concat(
        v.trailing
            .iter()
            .map(|c| concat(vec![text(" "), comment_doc(c, tab)]))
            .collect(),
    )
}

fn value_doc(v: &Value, tab: usize) -> Doc {
    concat(vec![leading_doc(v, tab), kind_doc(v, tab)])
}

fn kind_doc(v: &Value, tab: usize) -> Doc {
    match &v.kind {
        Kind::Atom { text: a, .. } => text(a.clone()),

        Kind::List { values, dangling } => {
            let force = force_break(dangling, values);
            if let Some(doc) = packed(values, dangling, force, "[", "]", tab) {
                return doc;
            }
            let n = values.len();
            let items: Vec<Doc> = values
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    item_doc(
                        vec![leading_doc(e, tab), kind_doc(e, tab)],
                        e,
                        force,
                        i + 1 == n,
                        tab,
                    )
                })
                .collect();
            container(force, "[", "]", items, dangling, tab)
        }

        Kind::Map {
            entries,
            dangling,
            broken,
        } => {
            let force = *broken || force_break(dangling, entries.iter().flat_map(|(k, v)| [k, v]));
            let n = entries.len();
            let items: Vec<Doc> = entries
                .iter()
                .enumerate()
                .map(|(i, (k, val))| {
                    item_doc(
                        vec![
                            leading_doc(k, tab),
                            hoisted_doc(val),
                            kind_doc(k, tab),
                            text(": "),
                            inline_doc(val, tab),
                            kind_doc(val, tab),
                        ],
                        val,
                        force,
                        i + 1 == n,
                        tab,
                    )
                })
                .collect();
            container(force, "{", "}", items, dangling, tab)
        }

        Kind::TupleType {
            ident,
            values,
            dangling,
        } => {
            if let Some(only) = hug_target(values, dangling) {
                return hug(
                    text(open_ident(ident.as_deref())),
                    kind_doc(only, tab),
                    text(")"),
                );
            }
            let force = force_break(dangling, values);
            let open = open_ident(ident.as_deref());
            if let Some(doc) = packed(values, dangling, force, &open, ")", tab) {
                return doc;
            }
            let n = values.len();
            let items: Vec<Doc> = values
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    item_doc(
                        vec![leading_doc(e, tab), kind_doc(e, tab)],
                        e,
                        force,
                        i + 1 == n,
                        tab,
                    )
                })
                .collect();
            container(force, &open, ")", items, dangling, tab)
        }

        Kind::FieldsType {
            ident,
            fields,
            dangling,
            broken,
        } => {
            let force = *broken || force_break(dangling, fields.iter().map(|f| &f.value));
            let n = fields.len();
            let items: Vec<Doc> = fields
                .iter()
                .enumerate()
                .map(|(i, Field { name, value })| {
                    item_doc(
                        vec![
                            leading_doc(value, tab),
                            hoisted_doc(value),
                            text(format!("{name}: ")),
                            inline_doc(value, tab),
                            kind_doc(value, tab),
                        ],
                        value,
                        force,
                        i + 1 == n,
                        tab,
                    )
                })
                .collect();
            container(
                force,
                &open_ident(ident.as_deref()),
                ")",
                items,
                dangling,
                tab,
            )
        }
    }
}

/// The sole element of a one-element tuple or tuple variant that should be
/// hugged when it must break (see [`hug`]): `Some((…))` or `Some([…])` then
/// breaks inside the element, as `ron`'s own pretty-printer writes it. Only a
/// container element hugs (an atom such as a long string still breaks onto
/// its own line), and only when no comment sits at the joins.
fn hug_target<'a>(values: &'a [Value], dangling: &[String]) -> Option<&'a Value> {
    match values {
        [only]
            if dangling.is_empty()
                && only.leading.is_empty()
                && only.trailing.is_empty()
                && !matches!(only.kind, Kind::Atom { .. }) =>
        {
            Some(only)
        }
        _ => None,
    }
}

/// A container. With `force` (comments present) every element goes on its own
/// line via hard breaks and plain commas; otherwise a `group` decides flat
/// vs. broken with a real `fits()` check and a conditional trailing comma.
fn container(
    force: bool,
    open: &str,
    close: &str,
    items: Vec<Doc>,
    dangling: &[String],
    tab: usize,
) -> Doc {
    if items.is_empty() && dangling.is_empty() {
        return concat(vec![text(open), text(close)]);
    }
    if force {
        let mut inner: Vec<Doc> = Vec::new();
        for (i, item) in items.into_iter().enumerate() {
            if i > 0 {
                inner.push(hard_line());
            }
            inner.push(item);
        }
        for (i, c) in dangling.iter().enumerate() {
            if i > 0 || !inner.is_empty() {
                inner.push(hard_line());
            }
            inner.push(comment_doc(c, tab));
        }
        concat(vec![
            text(open),
            nest(tab, concat(vec![hard_line(), concat(inner)])),
            hard_line(),
            text(close),
        ])
    } else {
        let mut inner: Vec<Doc> = Vec::new();
        for (i, item) in items.into_iter().enumerate() {
            if i > 0 {
                inner.push(line());
            }
            inner.push(item);
        }
        group(concat(vec![
            text(open),
            nest(tab, concat(vec![soft_line(), concat(inner)])),
            soft_line(),
            text(close),
        ]))
    }
}
