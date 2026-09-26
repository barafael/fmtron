mod display;

use crate::{Config, Rule};
use pest::iterators::Pair;

pub struct RonFile {
    header: Vec<HeaderItem>,
    value: Box<Value>,
    dangling: Vec<String>,
    config: Config,
    /// Line ending emitted between output lines.
    newline: &'static str,
}

/// Everything before the value, in source order.
pub enum HeaderItem {
    Attribute(Attribute),
    /// A comment, rendered on its own line (layout-independent, like every
    /// other leading comment).
    Comment(String),
}

pub enum Attribute {
    Enable(Vec<String>),
    Type(String),
    Schema(String),
    /// An attribute with comments inside it, kept exactly as written:
    /// normalizing it would have to relocate or drop those comments.
    Verbatim(String),
}

pub struct Value {
    leading: Vec<String>,
    /// Comments between a map key or field name and this value
    /// (`key /* c */ : /* d */ value`).
    inline: Vec<String>,
    trailing: Vec<String>,
    kind: Kind,
}

pub enum Kind {
    Atom(String),
    List {
        values: Vec<Value>,
        dangling: Vec<String>,
    },
    Map {
        entries: Vec<(Value, Value)>,
        dangling: Vec<String>,
    },
    TupleType {
        ident: Option<String>,
        values: Vec<Value>,
        dangling: Vec<String>,
    },
    FieldsType {
        ident: Option<String>,
        fields: Vec<Field>,
        dangling: Vec<String>,
    },
}

pub struct Field {
    name: String,
    value: Value,
}

fn comment_text(p: &Pair<Rule>) -> String {
    p.as_str()
        .trim_end_matches(['\n', '\r', ' ', '\t'])
        .to_string()
}

fn newline_between(src: &str, from: usize, to: usize) -> bool {
    let (from, to) = (from.min(src.len()), to.min(src.len()));
    from < to && src[from..to].contains('\n')
}

fn prepend_leading(v: &mut Value, mut pending: Vec<String>) {
    pending.append(&mut v.leading);
    v.leading = pending;
}

impl Attribute {
    fn from(pair: Pair<Rule>) -> Self {
        assert!(pair.as_rule() == Rule::attribute, "expected attribute pair");
        if pair
            .clone()
            .into_inner()
            .flatten()
            .any(|p| p.as_rule() == Rule::COMMENT)
        {
            return Attribute::Verbatim(pair.as_str().into());
        }
        let inner = pair.into_inner().next().unwrap();
        match inner.as_rule() {
            Rule::enable_attr => {
                Attribute::Enable(inner.into_inner().map(|p| p.as_str().into()).collect())
            }
            Rule::type_attr => Attribute::Type(inner.into_inner().next().unwrap().as_str().into()),
            Rule::schema_attr => {
                Attribute::Schema(inner.into_inner().next().unwrap().as_str().into())
            }
            _ => unreachable!(),
        }
    }
}

impl RonFile {
    pub fn parse_from(pair: Pair<Rule>, src: &str, config: Config) -> Self {
        assert!(pair.as_rule() == Rule::ron_file, "expected ron_file pair");

        let mut header: Vec<HeaderItem> = Vec::new();
        let mut value: Option<Box<Value>> = None;
        let mut value_end: Option<usize> = None;
        let mut trailing: Vec<String> = Vec::new();
        let mut dangling: Vec<String> = Vec::new();

        for p in pair.into_inner() {
            match p.as_rule() {
                Rule::attribute => header.push(HeaderItem::Attribute(Attribute::from(p))),
                Rule::value => {
                    value_end = Some(p.as_span().end());
                    value = Some(Box::new(Value::from(p, src)));
                }
                Rule::COMMENT => {
                    let txt = comment_text(&p);
                    let cs = p.as_span().start();
                    match (value.is_some(), value_end) {
                        (false, _) => header.push(HeaderItem::Comment(txt)),
                        (true, Some(ve)) => {
                            if !newline_between(src, ve, cs) {
                                trailing.push(txt);
                            } else {
                                dangling.push(txt);
                            }
                        }
                        _ => dangling.push(txt),
                    }
                }
                _ => {}
            }
        }

        let mut value = value.expect("ron_file must contain a value");
        value.trailing.append(&mut trailing);

        Self {
            header,
            value,
            dangling,
            config,
            newline: "\n",
        }
    }

    /// Emit `newline` (`"\n"` or `"\r\n"`) between output lines.
    pub(crate) fn with_newline(mut self, newline: &'static str) -> Self {
        self.newline = newline;
        self
    }
}

impl Value {
    fn from(pair: Pair<Rule>, src: &str) -> Self {
        match pair.as_rule() {
            Rule::bool
            | Rule::char
            | Rule::string
            | Rule::byte_string
            | Rule::byte_char
            | Rule::signed_int
            | Rule::float
            | Rule::unit_type => {
                let a = pair.as_str().to_string();
                Self {
                    leading: vec![],
                    inline: vec![],
                    trailing: vec![],
                    kind: Kind::Atom(a),
                }
            }

            Rule::list => {
                let (values, dangling) = collect_values(pair.clone().into_inner(), src);
                Self {
                    leading: vec![],
                    inline: vec![],
                    trailing: vec![],
                    kind: Kind::List { values, dangling },
                }
            }

            Rule::map => {
                let (entries, dangling) = collect_entries(pair.clone().into_inner(), src);
                Self {
                    leading: vec![],
                    inline: vec![],
                    trailing: vec![],
                    kind: Kind::Map { entries, dangling },
                }
            }

            Rule::tuple_type => {
                let (ident, values, dangling) =
                    collect_named_values(pair.clone().into_inner(), src);
                Self {
                    leading: vec![],
                    inline: vec![],
                    trailing: vec![],
                    kind: Kind::TupleType {
                        ident,
                        values,
                        dangling,
                    },
                }
            }

            Rule::fields_type => {
                let (ident, fields, dangling) = collect_fields(pair.clone().into_inner(), src);
                Self {
                    leading: vec![],
                    inline: vec![],
                    trailing: vec![],
                    kind: Kind::FieldsType {
                        ident,
                        fields,
                        dangling,
                    },
                }
            }

            Rule::value => Value::from(pair.into_inner().next().unwrap(), src),

            _ => unreachable!(),
        }
    }
}

/// One child of a container as parsed: a bare value, a `key: value` entry,
/// or a `name: value` field.
enum Child {
    Value(Value),
    Entry(Value, Value),
    Field { name: String, value: Value },
}

impl Child {
    /// Previous-line comments attach here as leading comments: the child's
    /// rendering prefix — the key for entries, the value otherwise.
    fn leading(&mut self) -> &mut Value {
        match self {
            Child::Value(v) | Child::Field { value: v, .. } => v,
            Child::Entry(key, _) => key,
        }
    }

    /// A same-line comment after this child attaches here as trailing.
    fn trailing(&mut self) -> &mut Value {
        match self {
            Child::Value(v) | Child::Entry(_, v) | Child::Field { value: v, .. } => v,
        }
    }
}

/// The `value` pair of an entry/field, plus comments appearing between its
/// head (key/name) and the value.
fn value_and_inline<'a>(
    inner: impl Iterator<Item = Pair<'a, Rule>>,
) -> (Pair<'a, Rule>, Vec<String>) {
    let mut inline: Vec<String> = Vec::new();
    let mut vp = None;
    for c in inner {
        match c.as_rule() {
            Rule::value => vp = Some(c),
            Rule::COMMENT => inline.push(comment_text(&c)),
            _ => {}
        }
    }
    (vp.unwrap(), inline)
}

/// An optional leading `ident`, e.g. the `Ident` of `Ident(..)`.
fn take_ident<'a, I: Iterator<Item = Pair<'a, Rule>>>(
    iter: &mut std::iter::Peekable<I>,
) -> Option<String> {
    if iter.peek().is_some_and(|p| p.as_rule() == Rule::ident) {
        Some(iter.next().unwrap().as_str().to_string())
    } else {
        None
    }
}

/// Collect the children of a container (list values, map entries, or struct
/// fields), attaching comments: a comment on the same line as the preceding
/// child becomes that child's trailing comment; a comment on its own line
/// becomes the next child's leading comment; comments after the last child
/// are dangling.
fn collect_children<'a>(
    inner: impl Iterator<Item = Pair<'a, Rule>>,
    src: &str,
) -> (Vec<Child>, Vec<String>) {
    let mut children: Vec<Child> = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    let mut last_end: Option<usize> = None;
    for p in inner {
        match p.as_rule() {
            Rule::COMMENT => {
                let txt = comment_text(&p);
                let cs = p.as_span().start();
                if let Some(le) = last_end
                    && !newline_between(src, le, cs)
                {
                    let idx = children.len() - 1;
                    children[idx].trailing().trailing.push(txt);
                    continue;
                }
                pending.push(txt);
            }
            Rule::value | Rule::map_entry | Rule::field => {
                let (mut child, end) = parse_child(p, src);
                if !pending.is_empty() {
                    prepend_leading(child.leading(), std::mem::take(&mut pending));
                }
                last_end = Some(end);
                children.push(child);
            }
            _ => {}
        }
    }
    let dangling = pending;
    (children, dangling)
}

/// Parse one container child pair into a `Child`, returned with the byte
/// offset where it ends (used to classify following comments).
fn parse_child(p: Pair<'_, Rule>, src: &str) -> (Child, usize) {
    match p.as_rule() {
        Rule::value => {
            let end = p.as_span().end();
            (Child::Value(Value::from(p, src)), end)
        }
        Rule::map_entry => {
            let mut inner = p.into_inner();
            let kp = inner.next().unwrap();
            let (vp, inline) = value_and_inline(inner);
            let end = vp.as_span().end();
            let k = Value::from(kp, src);
            let mut v = Value::from(vp, src);
            v.inline = inline;
            (Child::Entry(k, v), end)
        }
        Rule::field => {
            let mut inner = p.into_inner();
            let name = inner.next().unwrap().as_str().to_string();
            let (vp, inline) = value_and_inline(inner);
            let end = vp.as_span().end();
            let mut v = Value::from(vp, src);
            v.inline = inline;
            (Child::Field { name, value: v }, end)
        }
        _ => unreachable!("unexpected container child: {:?}", p.as_rule()),
    }
}

/// Collect `value` children (with attached comments) plus dangling comments.
fn collect_values<'a, I: Iterator<Item = Pair<'a, Rule>>>(
    inner: I,
    src: &str,
) -> (Vec<Value>, Vec<String>) {
    let (children, dangling) = collect_children(inner, src);
    let values = children
        .into_iter()
        .map(|c| match c {
            Child::Value(v) => v,
            _ => unreachable!("list/tuple children are `value` pairs"),
        })
        .collect();
    (values, dangling)
}

/// Collect map `entry` children (key, value) with attached comments.
fn collect_entries(
    inner: pest::iterators::Pairs<Rule>,
    src: &str,
) -> (Vec<(Value, Value)>, Vec<String>) {
    let (children, dangling) = collect_children(inner, src);
    let entries = children
        .into_iter()
        .map(|c| match c {
            Child::Entry(k, v) => (k, v),
            _ => unreachable!("map children are `map_entry` pairs"),
        })
        .collect();
    (entries, dangling)
}

/// Collect an optional leading `ident` then `value` children (tuple struct/variant).
fn collect_named_values(
    inner: pest::iterators::Pairs<Rule>,
    src: &str,
) -> (Option<String>, Vec<Value>, Vec<String>) {
    let mut iter = inner.peekable();
    let ident = take_ident(&mut iter);
    let (values, dangling) = collect_values(iter, src);
    (ident, values, dangling)
}

/// Collect an optional leading `ident` then `field` children (named struct).
fn collect_fields(
    inner: pest::iterators::Pairs<Rule>,
    src: &str,
) -> (Option<String>, Vec<Field>, Vec<String>) {
    let mut iter = inner.peekable();
    let ident = take_ident(&mut iter);
    let (children, dangling) = collect_children(iter, src);
    let fields = children
        .into_iter()
        .map(|c| match c {
            Child::Field { name, value } => Field { name, value },
            _ => unreachable!("struct children are `field` pairs"),
        })
        .collect();
    (ident, fields, dangling)
}
