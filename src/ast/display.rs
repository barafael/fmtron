use super::{Attribute, Field, Kind, RonFile, Value};
use crate::pretty::{self, Doc};
use std::fmt::{self, Display, Formatter};

fn text(s: impl Into<String>) -> Doc {
    pretty::text(s)
}

fn line() -> Doc {
    pretty::line()
}

fn soft_line() -> Doc {
    pretty::soft_line()
}

fn hard_line() -> Doc {
    pretty::hard_line()
}

fn comma() -> Doc {
    pretty::comma()
}

fn nest(n: usize, d: Doc) -> Doc {
    pretty::nest(n, d)
}

fn group(d: Doc) -> Doc {
    pretty::group(d)
}

fn concat(docs: Vec<Doc>) -> Doc {
    pretty::concat(docs)
}

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
                Attribute::Type(s) => writeln!(f, "#![type = {}]", s)?,
                Attribute::Schema(s) => writeln!(f, "#![schema = {}]", s)?,
            }
        }
        let doc = concat(vec![value_doc(value, config.tab_size), trailing_doc(value)]);
        write!(f, "{}", pretty::render(&doc, config.max_width))?;
        if !value.trailing.is_empty() {
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
                    .any(|(k, val)| subtree_has_comments(k) || subtree_has_comments(val))
        }
        Kind::TupleType { values, dangling, .. } => {
            !dangling.is_empty() || values.iter().any(subtree_has_comments)
        }
        Kind::FieldsType { fields, dangling, .. } => {
            !dangling.is_empty() || fields.iter().any(|f| subtree_has_comments(&f.value))
        }
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
            let force = !dangling.is_empty() || values.iter().any(subtree_has_comments);
            let n = values.len();
            let items: Vec<Doc> = values
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    let sep = if force || i < n - 1 { text(",") } else { comma() };
                    concat(vec![
                        leading_doc(e),
                        kind_doc(e, tab),
                        sep,
                        trailing_doc(e),
                    ])
                })
                .collect();
            container(force, "[", "]", items, dangling, tab)
        }

        Kind::Map { entries, dangling } => {
            let force = !dangling.is_empty()
                || entries
                    .iter()
                    .any(|(k, val)| subtree_has_comments(k) || subtree_has_comments(val));
            let n = entries.len();
            let items: Vec<Doc> = entries
                .iter()
                .enumerate()
                .map(|(i, (k, val))| {
                    let sep = if force || i < n - 1 { text(",") } else { comma() };
                    concat(vec![
                        leading_doc(k),
                        kind_doc(k, tab),
                        text(": "),
                        leading_doc(val),
                        kind_doc(val, tab),
                        sep,
                        trailing_doc(val),
                    ])
                })
                .collect();
            container(force, "{", "}", items, dangling, tab)
        }

        Kind::TupleType {
            ident, values, dangling,
        } => {
            let force = !dangling.is_empty() || values.iter().any(subtree_has_comments);
            let n = values.len();
            let open = format!("{}(", ident.clone().unwrap_or_default());
            let items: Vec<Doc> = values
                .iter()
                .enumerate()
                .map(|(i, e)| {
                    let sep = if force || i < n - 1 { text(",") } else { comma() };
                    concat(vec![
                        leading_doc(e),
                        kind_doc(e, tab),
                        sep,
                        trailing_doc(e),
                    ])
                })
                .collect();
            container(force, &open, ")", items, dangling, tab)
        }

        Kind::FieldsType {
            ident, fields, dangling,
        } => {
            let force = !dangling.is_empty() || fields.iter().any(|f| subtree_has_comments(&f.value));
            let n = fields.len();
            let open = format!("{}(", ident.clone().unwrap_or_default());
            let items: Vec<Doc> = fields
                .iter()
                .enumerate()
                .map(|(i, Field { name, value })| {
                    let sep = if force || i < n - 1 { text(",") } else { comma() };
                    concat(vec![
                        leading_doc(value),
                        text(format!("{name}: ")),
                        kind_doc(value, tab),
                        sep,
                        trailing_doc(value),
                    ])
                })
                .collect();
            container(force, &open, ")", items, dangling, tab)
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
