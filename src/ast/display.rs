use super::{Attribute, Field, Kind, RonFile, Value};
use crate::pretty::{Doc, comma, concat, group, hard_line, line, nest, render, soft_line, text};
use std::fmt::{self, Display, Formatter};

impl Display for RonFile {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let Self {
            attributes,
            value,
            dangling,
            config,
        } = self;
        for attr in attributes {
            match attr {
                Attribute::Enable(ids) => writeln!(f, "#![enable({})]", ids.join(", "))?,
                Attribute::Type(s) => writeln!(f, "#![type = {s}]")?,
                Attribute::Schema(s) => writeln!(f, "#![schema = {s}]")?,
            }
        }
        let doc = concat(vec![value_doc(value, config.tab_size), trailing_doc(value)]);
        write!(f, "{}", render(&doc, config.max_width))?;
        // The rendered value ends without a trailing newline. Terminate the
        // value's line before any comments follow — inline trailing comments
        // sit at the end of that line, dangling comments start fresh lines.
        if !value.trailing.is_empty() || !dangling.is_empty() {
            writeln!(f)?;
        }
        for c in dangling {
            writeln!(f, "{c}")?;
        }
        Ok(())
    }
}

/// True if `v` or anything in its subtree carries comments. A container whose
/// subtree has comments must render every element on its own line (hard
/// breaks), otherwise a leading/trailing comment would collide with sibling
/// layout.
fn subtree_has_comments(v: &Value) -> bool {
    if !v.leading.is_empty() || !v.trailing.is_empty() {
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
fn item_doc(segments: Vec<Doc>, v: &Value, force: bool, is_last: bool) -> Doc {
    let mut parts = segments;
    parts.push(item_sep(force, is_last));
    parts.push(trailing_doc(v));
    concat(parts)
}

/// The opening token of a tuple/struct: `Ident(` or just `(`.
fn open_ident(ident: Option<&str>) -> String {
    match ident {
        Some(id) => format!("{id}("),
        None => "(".to_string(),
    }
}

/// Leading comments, each on its own line before the value.
fn leading_doc(v: &Value) -> Doc {
    let mut parts: Vec<Doc> = Vec::new();
    for c in &v.leading {
        parts.push(text(c.clone()));
        parts.push(hard_line());
    }
    concat(parts)
}

/// Trailing comments, inline after the value/comma.
fn trailing_doc(v: &Value) -> Doc {
    concat(v.trailing.iter().map(|c| text(format!(" {c}"))).collect())
}

fn value_doc(v: &Value, tab: usize) -> Doc {
    concat(vec![leading_doc(v), kind_doc(v, tab)])
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
                    item_doc(vec![leading_doc(e), kind_doc(e, tab)], e, force, i + 1 == n)
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
                            leading_doc(k),
                            kind_doc(k, tab),
                            text(": "),
                            leading_doc(val),
                            kind_doc(val, tab),
                        ],
                        val,
                        force,
                        i + 1 == n,
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
            let force = force_break(dangling, values);
            let n = values.len();
            let items: Vec<Doc> = values
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    item_doc(vec![leading_doc(e), kind_doc(e, tab)], e, force, i + 1 == n)
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
                            leading_doc(value),
                            text(format!("{name}: ")),
                            kind_doc(value, tab),
                        ],
                        value,
                        force,
                        i + 1 == n,
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
            inner.push(text(c.clone()));
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
