mod display;

use crate::{Config, Rule};
use pest::iterators::Pair;

pub struct RonFile {
    attributes: Vec<Attribute>,
    value: Box<Value>,
    dangling: Vec<String>,
    config: Config,
}

pub enum Attribute {
    Enable(Vec<String>),
    Type(String),
    Schema(String),
}

pub struct Value {
    leading: Vec<String>,
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
        let inner = pair.into_inner().next().unwrap();
        match inner.as_rule() {
            Rule::enable_attr => {
                Attribute::Enable(inner.into_inner().map(|p| p.as_str().into()).collect())
            }
            Rule::type_attr => {
                Attribute::Type(inner.into_inner().next().unwrap().as_str().into())
            }
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

        let mut attributes = Vec::new();
        let mut pre_comments: Vec<String> = Vec::new();
        let mut value: Option<Box<Value>> = None;
        let mut value_end: Option<usize> = None;
        let mut trailing: Vec<String> = Vec::new();
        let mut dangling: Vec<String> = Vec::new();

        for p in pair.into_inner() {
            match p.as_rule() {
                Rule::attribute => attributes.push(Attribute::from(p)),
                Rule::value => {
                    value_end = Some(p.as_span().end());
                    value = Some(Box::new(Value::from(p, src)));
                }
                Rule::COMMENT => {
                    let txt = comment_text(&p);
                    let cs = p.as_span().start();
                    match (value.is_some(), value_end) {
                        (false, _) => pre_comments.push(txt),
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
        value.leading = { let mut v = pre_comments; v.append(&mut value.leading); v };
        value.trailing.append(&mut trailing);

        Self {
            attributes,
            value,
            dangling,
            config,
        }
    }
}

impl Value {
    fn from(pair: Pair<Rule>, src: &str) -> Self {
        match pair.as_rule() {
            Rule::bool
            | Rule::char
            | Rule::string
            | Rule::byte_string
            | Rule::signed_int
            | Rule::float
            | Rule::unit_type => {
                let a = pair.as_str().to_string();
                Self {
                    leading: vec![],
                    trailing: vec![],
                    kind: Kind::Atom(a),
                }
            }

            Rule::list => {
                let (values, dangling) = collect_values(pair.clone().into_inner(), src);
                Self {
                    leading: vec![],
                    trailing: vec![],
                    kind: Kind::List { values, dangling },
                }
            }

            Rule::map => {
                let (entries, dangling) = collect_entries(pair.clone().into_inner(), src);
                Self {
                    leading: vec![],
                    trailing: vec![],
                    kind: Kind::Map { entries, dangling },
                }
            }

            Rule::tuple_type => {
                let (ident, values, dangling) = collect_named_values(pair.clone().into_inner(), src);
                Self {
                    leading: vec![],
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

/// Collect `value` children (with attached comments) plus dangling comments.
fn collect_values<'a, I: Iterator<Item = Pair<'a, Rule>>>(
    inner: I,
    src: &str,
) -> (Vec<Value>, Vec<String>) {
    let mut values: Vec<Value> = Vec::new();
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
                    let idx = values.len() - 1;
                    values[idx].trailing.push(txt);
                    continue;
                }
                pending.push(txt);
            }
            Rule::value => {
                let end = p.as_span().end();
                let mut v = Value::from(p, src);
                if !pending.is_empty() {
                    prepend_leading(&mut v, std::mem::take(&mut pending));
                }
                last_end = Some(end);
                values.push(v);
            }
            _ => {}
        }
    }
    let dangling = pending;
    (values, dangling)
}

/// Collect map `entry` children (key, value) with attached comments.
fn collect_entries(
    inner: pest::iterators::Pairs<Rule>,
    src: &str,
) -> (Vec<(Value, Value)>, Vec<String>) {
    let mut entries: Vec<(Value, Value)> = Vec::new();
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
                    let idx = entries.len() - 1;
                    entries[idx].1.trailing.push(txt);
                    continue;
                }
                pending.push(txt);
            }
            Rule::map_entry => {
                let mut inner = p.into_inner();
                let kp = inner.next().unwrap();
                let mut inline: Vec<String> = Vec::new();
                let mut vp = None;
                for c in inner {
                    match c.as_rule() {
                        Rule::value => vp = Some(c),
                        Rule::COMMENT => inline.push(comment_text(&c)),
                        _ => {}
                    }
                }
                let vp = vp.unwrap();
                let entry_end = vp.as_span().end();
                let mut k = Value::from(kp, src);
                let mut v = Value::from(vp, src);
                if !inline.is_empty() {
                    prepend_leading(&mut v, inline);
                }
                if !pending.is_empty() {
                    prepend_leading(&mut k, std::mem::take(&mut pending));
                }
                last_end = Some(entry_end);
                entries.push((k, v));
            }
            _ => {}
        }
    }
    let dangling = pending;
    (entries, dangling)
}

/// Collect an optional leading `ident` then `value` children (tuple struct/variant).
fn collect_named_values(
    inner: pest::iterators::Pairs<Rule>,
    src: &str,
) -> (Option<String>, Vec<Value>, Vec<String>) {
    let mut iter = inner.peekable();
    let ident = if iter.peek().is_some_and(|p| p.as_rule() == Rule::ident) {
        Some(iter.next().unwrap().as_str().to_string())
    } else {
        None
    };
    let (values, dangling) = collect_values(iter, src);
    (ident, values, dangling)
}

/// Collect an optional leading `ident` then `field` children (named struct).
fn collect_fields(
    inner: pest::iterators::Pairs<Rule>,
    src: &str,
) -> (Option<String>, Vec<Field>, Vec<String>) {
    let mut iter = inner.peekable();
    let ident = if iter.peek().is_some_and(|p| p.as_rule() == Rule::ident) {
        Some(iter.next().unwrap().as_str().to_string())
    } else {
        None
    };
    let mut fields: Vec<Field> = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    let mut last_end: Option<usize> = None;
    for p in iter {
        match p.as_rule() {
            Rule::COMMENT => {
                let txt = comment_text(&p);
                let cs = p.as_span().start();
                if let Some(le) = last_end
                    && !newline_between(src, le, cs)
                {
                    let idx = fields.len() - 1;
                    fields[idx].value.trailing.push(txt);
                    continue;
                }
                pending.push(txt);
            }
            Rule::field => {
                let mut inner = p.into_inner();
                let name = inner.next().unwrap().as_str().to_string();
                let mut inline: Vec<String> = Vec::new();
                let mut vp = None;
                for c in inner {
                    match c.as_rule() {
                        Rule::value => vp = Some(c),
                        Rule::COMMENT => inline.push(comment_text(&c)),
                        _ => {}
                    }
                }
                let vp = vp.unwrap();
                let field_end = vp.as_span().end();
                let mut v = Value::from(vp, src);
                if !inline.is_empty() {
                    prepend_leading(&mut v, inline);
                }
                if !pending.is_empty() {
                    prepend_leading(&mut v, std::mem::take(&mut pending));
                }
                last_end = Some(field_end);
                fields.push(Field { name, value: v });
            }
            _ => {}
        }
    }
    let dangling = pending;
    (ident, fields, dangling)
}
