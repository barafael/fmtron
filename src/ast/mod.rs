mod display;

use crate::{BlankLines, Config, Rule, is_whitespace};
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
    /// Comments between a map key or field name and the colon after it
    /// (`/* c */` in `key /* c */ : value`).
    before_colon: Vec<String>,
    /// Comments between the colon after a map key or field name and this
    /// value (`/* d */` in `key: /* d */ value`).
    inline: Vec<String>,
    /// Comments that end the line of a container's opening bracket
    /// (`Scene( // c`), before its first element.
    after_open: Vec<String>,
    trailing: Vec<String>,
    /// The input breaks the line before this element of a list or tuple:
    /// after the element before it, or for the first element, after the
    /// opening bracket. False for anything else.
    starts_line: bool,
    kind: Kind,
}

pub enum Kind {
    Atom {
        text: String,
        /// A number, bool or char (or byte char), as opposed to a string or
        /// an identifier.
        scalar: bool,
    },
    List {
        values: Vec<Value>,
        dangling: Vec<String>,
    },
    Map {
        entries: Vec<(Value, Value)>,
        dangling: Vec<String>,
        /// The input breaks the line right after the opening bracket.
        broken: bool,
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
        /// The input breaks the line right after the opening bracket.
        broken: bool,
    },
}

pub struct Field {
    name: String,
    value: Value,
}

/// A comment as written, without trailing whitespace (which, for a line
/// comment, includes the line break ending it).
fn raw_comment<'i>(p: &Pair<'i, Rule>) -> &'i str {
    p.as_str().trim_end_matches(is_whitespace)
}

/// A comment's text, ready to render. A block comment spanning several
/// lines gets `\n` line breaks (the renderer emits the file's line ending),
/// and its later lines lose the indentation of the line it starts on, so
/// that they move with that line when it is re-indented. Their indentation
/// relative to it, and all other text, is kept.
fn comment_text(p: &Pair<Rule>) -> String {
    let text = raw_comment(p);
    if !text.contains('\n') {
        return text.to_string();
    }
    let span = p.as_span();
    let src = span.get_input();
    let line = &src[src[..span.start()].rfind('\n').map_or(0, |i| i + 1)..];
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let mut lines = text.split('\n').map(|l| l.strip_suffix('\r').unwrap_or(l));
    let mut out = lines.next().unwrap_or_default().to_string();
    for l in lines {
        out.push('\n');
        if !l.chars().all(is_whitespace) {
            let lead = l.len() - l.trim_start_matches([' ', '\t']).len();
            out.push_str(&l[lead.min(indent)..]);
        }
    }
    out
}

/// Marks a blank line in a list of comments (leading, dangling, header): the
/// empty string, which no real comment can be. Rendered as an empty line.
pub(crate) const BLANK_LINE: &str = "";

/// True if `src[from..to]` (whitespace and commas between two items) holds a
/// blank line: two line breaks with only whitespace between them.
fn blank_line_between(src: &str, from: usize, to: usize) -> bool {
    let (from, to) = (from.min(src.len()), to.min(src.len()));
    let mut after_newline = false;
    for c in src[from.min(to)..to].chars() {
        match c {
            '\n' if after_newline => return true,
            '\n' => after_newline = true,
            c if is_whitespace(c) => {}
            _ => after_newline = false,
        }
    }
    false
}

/// Where a comment's text ends: a line comment's span includes its `\n`,
/// which belongs to the gap after it.
fn comment_end(p: &Pair<Rule>) -> usize {
    p.as_span().start() + raw_comment(p).len()
}

fn newline_between(src: &str, from: usize, to: usize) -> bool {
    let (from, to) = (from.min(src.len()), to.min(src.len()));
    from < to && src[from..to].contains('\n')
}

/// The index just past a container's opening bracket: the first `(`, `[` or
/// `{` outside the container's inner pairs (its ident, comments and
/// children).
fn opener_end(pair: &Pair<Rule>, src: &str) -> usize {
    let mut pos = pair.as_span().start();
    for p in pair.clone().into_inner() {
        let start = p.as_span().start();
        if let Some(i) = src[pos..start].find(['(', '[', '{']) {
            return pos + i + 1;
        }
        pos = p.as_span().end();
    }
    let end = pair.as_span().end();
    src[pos..end]
        .find(['(', '[', '{'])
        .map_or(end, |i| pos + i + 1)
}

/// True if the input breaks the line between a container's opening bracket
/// and its first item (child or comment).
fn opens_with_newline(pair: &Pair<Rule>, src: &str) -> bool {
    let opener = opener_end(pair, src);
    pair.clone()
        .into_inner()
        .map(|p| p.as_span().start())
        .find(|&start| start >= opener)
        .is_some_and(|start| newline_between(src, opener, start))
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
        // End of the previous item (attribute, comment or value), to find
        // blank lines between items.
        let mut prev_end: Option<usize> = None;
        let blank_since = |prev: Option<usize>, start: usize| {
            prev.is_some_and(|e| blank_line_between(src, e, start))
        };
        let mut value: Option<Box<Value>> = None;
        let mut value_end: Option<usize> = None;
        let mut trailing: Vec<String> = Vec::new();
        let mut dangling: Vec<String> = Vec::new();

        for p in pair.into_inner() {
            match p.as_rule() {
                Rule::attribute => {
                    if blank_since(prev_end, p.as_span().start()) {
                        header.push(HeaderItem::Comment(BLANK_LINE.into()));
                    }
                    prev_end = Some(p.as_span().end());
                    header.push(HeaderItem::Attribute(Attribute::from(p)));
                }
                Rule::value => {
                    if blank_since(prev_end, p.as_span().start()) {
                        header.push(HeaderItem::Comment(BLANK_LINE.into()));
                    }
                    value_end = Some(p.as_span().end());
                    prev_end = value_end;
                    value = Some(Box::new(Value::from(p, src)));
                }
                Rule::COMMENT => {
                    let txt = comment_text(&p);
                    let cs = p.as_span().start();
                    let blank = blank_since(prev_end, cs);
                    prev_end = Some(comment_end(&p));
                    match (value.is_some(), value_end) {
                        (false, _) => {
                            if blank {
                                header.push(HeaderItem::Comment(BLANK_LINE.into()));
                            }
                            header.push(HeaderItem::Comment(txt));
                        }
                        (true, Some(ve)) => {
                            if !newline_between(src, ve, cs) {
                                trailing.push(txt);
                            } else {
                                if blank {
                                    dangling.push(BLANK_LINE.into());
                                }
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
        if config.blank_lines == BlankLines::Remove {
            header.retain(|h| !matches!(h, HeaderItem::Comment(c) if c == BLANK_LINE));
            dangling.retain(|c| c != BLANK_LINE);
            value.remove_blank_lines();
        }

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
    /// A value of `kind` with no comments attached.
    fn of(kind: Kind) -> Self {
        Self {
            leading: vec![],
            before_colon: vec![],
            inline: vec![],
            after_open: vec![],
            trailing: vec![],
            starts_line: false,
            kind,
        }
    }

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
                let text = pair.as_str().to_string();
                let scalar = !matches!(
                    pair.as_rule(),
                    Rule::string | Rule::byte_string | Rule::unit_type
                );
                Self::of(Kind::Atom { text, scalar })
            }

            Rule::list => {
                let opener = opener_end(&pair, src);
                let (mut values, c) = collect_values(pair.clone().into_inner(), src, opener);
                if let Some(first) = values.first_mut() {
                    first.starts_line = opens_with_newline(&pair, src);
                }
                Self::of(Kind::List {
                    values,
                    dangling: c.dangling,
                })
                .after_open(c.after_open)
            }

            Rule::map => {
                let broken = opens_with_newline(&pair, src);
                let opener = opener_end(&pair, src);
                let (entries, c) = collect_entries(pair.clone().into_inner(), src, opener);
                Self::of(Kind::Map {
                    entries,
                    dangling: c.dangling,
                    broken,
                })
                .after_open(c.after_open)
            }

            Rule::tuple_type => {
                let opener = opener_end(&pair, src);
                let (ident, mut values, c) =
                    collect_named_values(pair.clone().into_inner(), src, opener);
                if let Some(first) = values.first_mut() {
                    first.starts_line = opens_with_newline(&pair, src);
                }
                Self::of(Kind::TupleType {
                    ident,
                    values,
                    dangling: c.dangling,
                })
                .after_open(c.after_open)
            }

            Rule::fields_type => {
                let broken = opens_with_newline(&pair, src);
                let opener = opener_end(&pair, src);
                let (ident, fields, c) = collect_fields(pair.clone().into_inner(), src, opener);
                Self::of(Kind::FieldsType {
                    ident,
                    fields,
                    dangling: c.dangling,
                    broken,
                })
                .after_open(c.after_open)
            }

            Rule::value => Value::from(pair.into_inner().next().unwrap(), src),

            _ => unreachable!(),
        }
    }

    fn after_open(mut self, comments: Vec<String>) -> Self {
        self.after_open = comments;
        self
    }

    /// Drop every blank-line marker in this subtree.
    fn remove_blank_lines(&mut self) {
        self.leading.retain(|c| c != BLANK_LINE);
        let (children, dangling): (Vec<&mut Value>, &mut Vec<String>) = match &mut self.kind {
            Kind::Atom { .. } => return,
            Kind::List { values, dangling }
            | Kind::TupleType {
                values, dangling, ..
            } => (values.iter_mut().collect(), dangling),
            Kind::Map {
                entries, dangling, ..
            } => (
                entries.iter_mut().flat_map(|(k, v)| [k, v]).collect(),
                dangling,
            ),
            Kind::FieldsType {
                fields, dangling, ..
            } => (fields.iter_mut().map(|f| &mut f.value).collect(), dangling),
        };
        dangling.retain(|c| c != BLANK_LINE);
        for child in children {
            child.remove_blank_lines();
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

/// The `value` pair of an entry/field, plus the comments between its head
/// (key/name, ending at `head_end`) and the value: those written before the
/// colon, and those after it.
fn value_and_inline<'a>(
    inner: impl Iterator<Item = Pair<'a, Rule>>,
    head_end: usize,
    src: &str,
) -> (Pair<'a, Rule>, Vec<String>, Vec<String>) {
    let (mut before, mut after) = (Vec::new(), Vec::new());
    // The gaps between the head, the comments and the value hold only
    // whitespace and the colon.
    let mut pos = head_end;
    let mut colon_seen = false;
    for c in inner {
        let start = c.as_span().start();
        colon_seen |= src[pos.min(start)..start].contains(':');
        match c.as_rule() {
            Rule::value => return (c, before, after),
            Rule::COMMENT if colon_seen => after.push(comment_text(&c)),
            Rule::COMMENT => before.push(comment_text(&c)),
            _ => {}
        }
        pos = c.as_span().end();
    }
    unreachable!("an entry or field has a value")
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

/// The comments of a container that belong to no child.
struct Loose {
    /// After the last child (or in an empty container).
    dangling: Vec<String>,
    /// On the opening bracket's line, ending it, before the first child.
    after_open: Vec<String>,
}

/// Collect the children of a container (list values, map entries, or struct
/// fields), attaching comments: a comment on the same line as the preceding
/// child becomes that child's trailing comment; a comment on its own line
/// becomes the next child's leading comment; comments after the last child
/// are dangling. Comments on the line of the opening bracket (which ends at
/// `opener`) stay with it if they end that line, and lead the first child if
/// it follows on the same line. A blank line between two items (children or
/// own-line comments) is recorded as a [`BLANK_LINE`] marker in those lists;
/// blank lines after the opening or before the closing bracket are dropped.
fn collect_children<'a>(
    inner: impl Iterator<Item = Pair<'a, Rule>>,
    src: &str,
    opener: usize,
) -> (Vec<Child>, Loose) {
    let mut children: Vec<Child> = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    let mut after_open: Vec<String> = Vec::new();
    // Where the comments on the opening bracket's line end.
    let mut after_open_end = opener;
    let mut last_end: Option<usize> = None;
    // End of the previous item of any kind, for blank-line detection.
    let mut prev_end: Option<usize> = None;
    for p in inner {
        let start = p.as_span().start();
        let blank = prev_end.is_some_and(|e| blank_line_between(src, e, start));
        match p.as_rule() {
            Rule::COMMENT => {
                let txt = comment_text(&p);
                let cs = p.as_span().start();
                prev_end = Some(comment_end(&p));
                if let Some(le) = last_end
                    && !newline_between(src, le, cs)
                {
                    let idx = children.len() - 1;
                    children[idx].trailing().trailing.push(txt);
                    continue;
                }
                if last_end.is_none()
                    && pending.is_empty()
                    && cs >= opener
                    && !newline_between(src, opener, cs)
                {
                    after_open_end = comment_end(&p);
                    after_open.push(txt);
                    continue;
                }
                if blank && !right_after_open(last_end, &pending) {
                    pending.push(BLANK_LINE.into());
                }
                pending.push(txt);
            }
            Rule::value | Rule::map_entry | Rule::field => {
                if blank && !right_after_open(last_end, &pending) {
                    pending.push(BLANK_LINE.into());
                }
                if last_end.is_none()
                    && pending.is_empty()
                    && !newline_between(src, after_open_end, start)
                {
                    // The first child shares the bracket's line: the comments
                    // before it lead it.
                    pending = std::mem::take(&mut after_open);
                }
                let starts_line = last_end.is_some_and(|e| newline_between(src, e, start));
                let (mut child, end) = parse_child(p, src);
                if let Child::Value(v) = &mut child {
                    v.starts_line = starts_line;
                }
                prev_end = Some(end);
                if !pending.is_empty() {
                    prepend_leading(child.leading(), std::mem::take(&mut pending));
                }
                last_end = Some(end);
                children.push(child);
            }
            _ => {}
        }
    }
    let dangling = if children.is_empty() {
        // No child to keep them apart from the closing bracket.
        after_open.append(&mut pending);
        std::mem::take(&mut after_open)
    } else {
        pending
    };
    (
        children,
        Loose {
            dangling,
            after_open,
        },
    )
}

/// True if nothing but comments on the opening bracket's line came before:
/// a blank line here follows the bracket's line, and is dropped like any
/// blank line right after an opening bracket.
fn right_after_open(last_end: Option<usize>, pending: &[String]) -> bool {
    last_end.is_none() && pending.is_empty()
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
            let (vp, before_colon, inline) = value_and_inline(inner, kp.as_span().end(), src);
            let end = vp.as_span().end();
            let k = Value::from(kp, src);
            let mut v = Value::from(vp, src);
            v.before_colon = before_colon;
            v.inline = inline;
            (Child::Entry(k, v), end)
        }
        Rule::field => {
            let mut inner = p.into_inner();
            let np = inner.next().unwrap();
            let name = np.as_str().to_string();
            let (vp, before_colon, inline) = value_and_inline(inner, np.as_span().end(), src);
            let end = vp.as_span().end();
            let mut v = Value::from(vp, src);
            v.before_colon = before_colon;
            v.inline = inline;
            (Child::Field { name, value: v }, end)
        }
        _ => unreachable!("unexpected container child: {:?}", p.as_rule()),
    }
}

/// Collect `value` children (with attached comments) plus loose comments.
fn collect_values<'a, I: Iterator<Item = Pair<'a, Rule>>>(
    inner: I,
    src: &str,
    opener: usize,
) -> (Vec<Value>, Loose) {
    let (children, loose) = collect_children(inner, src, opener);
    let values = children
        .into_iter()
        .map(|c| match c {
            Child::Value(v) => v,
            _ => unreachable!("list/tuple children are `value` pairs"),
        })
        .collect();
    (values, loose)
}

/// Collect map `entry` children (key, value) with attached comments.
fn collect_entries(
    inner: pest::iterators::Pairs<Rule>,
    src: &str,
    opener: usize,
) -> (Vec<(Value, Value)>, Loose) {
    let (children, loose) = collect_children(inner, src, opener);
    let entries = children
        .into_iter()
        .map(|c| match c {
            Child::Entry(k, v) => (k, v),
            _ => unreachable!("map children are `map_entry` pairs"),
        })
        .collect();
    (entries, loose)
}

/// Collect an optional leading `ident` then `value` children (tuple struct/variant).
fn collect_named_values(
    inner: pest::iterators::Pairs<Rule>,
    src: &str,
    opener: usize,
) -> (Option<String>, Vec<Value>, Loose) {
    let mut iter = inner.peekable();
    let ident = take_ident(&mut iter);
    let (values, loose) = collect_values(iter, src, opener);
    (ident, values, loose)
}

/// Collect an optional leading `ident` then `field` children (named struct).
fn collect_fields(
    inner: pest::iterators::Pairs<Rule>,
    src: &str,
    opener: usize,
) -> (Option<String>, Vec<Field>, Loose) {
    let mut iter = inner.peekable();
    let ident = take_ident(&mut iter);
    let (children, loose) = collect_children(iter, src, opener);
    let fields = children
        .into_iter()
        .map(|c| match c {
            Child::Field { name, value } => Field { name, value },
            _ => unreachable!("struct children are `field` pairs"),
        })
        .collect();
    (ident, fields, loose)
}
