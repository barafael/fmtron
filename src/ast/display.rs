use super::{Attribute, Field, HeaderItem, Kind, RonFile, Value};
use crate::MAX_INDENT;
use crate::pretty::{
    Doc, comma, concat, group, hard_line, hug, line, nest, render_with_newline, soft_line, text,
};
use std::fmt::{self, Display, Formatter};

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
                HeaderItem::Attribute(attr) => write!(f, "{}{nl}", AttributeDisplay(attr))?,
            }
        }
        let doc = concat(vec![
            value_doc(value, config.tab_size),
            trailing_doc(value, config.tab_size),
        ]);
        write!(f, "{}", render_with_newline(&doc, config.max_width, nl))?;
        // The rendered value ends without a trailing newline. Terminate the
        // value's line before any comments follow — inline trailing comments
        // sit at the end of that line, dangling comments start fresh lines.
        if !value.trailing.is_empty() || !dangling.is_empty() {
            write!(f, "{nl}")?;
        }
        for c in dangling {
            write!(f, "{}{nl}", comment_lines(c, config.tab_size).join(nl))?;
        }
        Ok(())
    }
}

struct AttributeDisplay<'a>(&'a Attribute);

impl Display for AttributeDisplay<'_> {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.0 {
            Attribute::Enable(ids) => write!(f, "#![enable({})]", ids.join(", ")),
            Attribute::Type(s) => write!(f, "#![type = {s}]"),
            Attribute::Schema(s) => write!(f, "#![schema = {s}]"),
            Attribute::Verbatim(s) => write!(f, "{s}"),
        }
    }
}

/// True if `v` or anything in its subtree carries comments. A container whose
/// subtree has comments must render every element on its own line (hard
/// breaks), otherwise a leading/trailing comment would collide with sibling
/// layout.
fn subtree_has_comments(v: &Value) -> bool {
    if !v.leading.is_empty() || !v.inline.is_empty() || !v.trailing.is_empty() {
        return true;
    }
    match &v.kind {
        Kind::Atom(_) => false,
        Kind::List { values, dangling } => {
            !dangling.is_empty() || values.iter().any(subtree_has_comments)
        }
        Kind::Map { entries, dangling } => {
            !dangling.is_empty()
                || entries
                    .iter()
                    .flat_map(|(k, v)| [k, v])
                    .any(subtree_has_comments)
        }
        Kind::TupleType {
            values, dangling, ..
        } => !dangling.is_empty() || values.iter().any(subtree_has_comments),
        Kind::FieldsType {
            fields, dangling, ..
        } => !dangling.is_empty() || fields.iter().any(|f| subtree_has_comments(&f.value)),
    }
}

/// A container element goes on its own line (hard breaks, plain commas) if
/// the container holds dangling comments or any member subtree has comments.
fn force_break<'a>(dangling: &[String], members: impl IntoIterator<Item = &'a Value>) -> bool {
    !dangling.is_empty() || members.into_iter().any(subtree_has_comments)
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
        Kind::Atom(a) => text(a.clone()),

        Kind::List { values, dangling } => {
            let force = force_break(dangling, values);
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

        Kind::Map { entries, dangling } => {
            let force = force_break(dangling, entries.iter().flat_map(|(k, v)| [k, v]));
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
                    tab,
                );
            }
            let force = force_break(dangling, values);
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
            container(
                force,
                &open_ident(ident.as_deref()),
                ")",
                items,
                dangling,
                tab,
            )
        }

        Kind::FieldsType {
            ident,
            fields,
            dangling,
        } => {
            let force = force_break(dangling, fields.iter().map(|f| &f.value));
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
                && !matches!(only.kind, Kind::Atom(_)) =>
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
