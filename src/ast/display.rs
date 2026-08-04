use super::{Attribute, Field, Kind, RonFile, Value};
use crate::{MAX_LINE_WIDTH, TAB_SIZE};
use itertools::Itertools;
use std::fmt::Write;
use std::{
    fmt::{self, Display, Formatter},
    sync::atomic::Ordering,
};

fn space(level: usize) -> String {
    " ".repeat(TAB_SIZE.load(Ordering::SeqCst) * level)
}

impl Display for RonFile {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let Self {
            attributes,
            value,
            dangling,
        } = self;
        for attr in attributes {
            match attr {
                Attribute::Enable(ids) => writeln!(f, "#![enable({})]", ids.join(", "))?,
                Attribute::Type(s) => writeln!(f, "#![type = {}]", s)?,
                Attribute::Schema(s) => writeln!(f, "#![schema = {}]", s)?,
            }
        }
        for c in &value.leading {
            writeln!(f, "{c}")?;
        }
        write!(f, "{}", value.to_string_rec(0))?;
        for c in &value.trailing {
            write!(f, " {c}")?;
        }
        if !value.trailing.is_empty() {
            writeln!(f)?;
        }
        for c in dangling {
            writeln!(f, "{c}")?;
        }
        Ok(())
    }
}

impl Value {
    fn has_own_comments(&self) -> bool {
        !self.leading.is_empty() || !self.trailing.is_empty()
    }

    fn internals_have_comments(&self) -> bool {
        match &self.kind {
            Kind::Atom(_) => false,
            Kind::List { values, dangling } => {
                !dangling.is_empty() || values.iter().any(Self::has_own_comments)
            }
            Kind::Map { entries, dangling } => {
                !dangling.is_empty()
                    || entries
                        .iter()
                        .any(|(k, v)| k.has_own_comments() || v.has_own_comments())
            }
            Kind::TupleType { values, dangling, .. } => {
                !dangling.is_empty() || values.iter().any(Self::has_own_comments)
            }
            Kind::FieldsType { fields, dangling, .. } => {
                !dangling.is_empty() || fields.iter().any(|f| f.value.has_own_comments())
            }
        }
    }

    fn to_string_rec(&self, tabs: usize) -> String {
        if self.internals_have_comments() {
            self.multiline_comments(tabs)
        } else if tabs * TAB_SIZE.load(Ordering::SeqCst) + self.len
            > MAX_LINE_WIDTH.load(Ordering::SeqCst)
        {
            self.multiline(tabs)
        } else {
            self.single_line()
        }
    }

    /// Render an element line: leading comments (own lines), main content,
    /// comma, trailing comments (inline after the comma).
    fn emit_item(
        out: &mut String,
        tabs: usize,
        leading: &[String],
        main: &str,
        trailing: &[String],
    ) {
        let ind = space(tabs);
        for c in leading {
            out.push_str(&ind);
            out.push_str(c);
            out.push('\n');
        }
        out.push_str(&ind);
        out.push_str(main);
        out.push(',');
        for c in trailing {
            out.push(' ');
            out.push_str(c);
        }
        out.push('\n');
    }

    fn dangling_block(dangling: &[String], tabs: usize) -> String {
        let mut s = String::new();
        let ind = space(tabs);
        for c in dangling {
            s.push_str(&ind);
            s.push_str(c);
            s.push('\n');
        }
        s
    }

    fn multiline_comments(&self, tabs: usize) -> String {
        let child_tabs = tabs + 1;
        match &self.kind {
            Kind::Atom(a) => a.clone(),
            Kind::List { values, dangling } => {
                let mut s = String::from("[\n");
                for v in values {
                    let main = v.to_string_rec(child_tabs);
                    Self::emit_item(&mut s, child_tabs, &v.leading, &main, &v.trailing);
                }
                s.push_str(&Self::dangling_block(dangling, child_tabs));
                s.push_str(&space(tabs));
                s.push(']');
                s
            }
            Kind::Map { entries, dangling } => {
                let mut s = String::from("{\n");
                for (k, v) in entries {
                    let main = format!(
                        "{}: {}",
                        k.to_string_rec(child_tabs),
                        v.to_string_rec(child_tabs)
                    );
                    Self::emit_item(&mut s, child_tabs, &k.leading, &main, &v.trailing);
                }
                s.push_str(&Self::dangling_block(dangling, child_tabs));
                s.push_str(&space(tabs));
                s.push('}');
                s
            }
            Kind::TupleType {
                ident, values, dangling,
            } => {
                let id = ident.clone().unwrap_or_default();
                let mut s = format!("{id}(\n");
                for v in values {
                    let main = v.to_string_rec(child_tabs);
                    Self::emit_item(&mut s, child_tabs, &v.leading, &main, &v.trailing);
                }
                s.push_str(&Self::dangling_block(dangling, child_tabs));
                s.push_str(&space(tabs));
                s.push(')');
                s
            }
            Kind::FieldsType {
                ident, fields, dangling,
            } => {
                let id = ident.clone().unwrap_or_default();
                let mut s = format!("{id}(\n");
                for Field { name, value } in fields {
                    let main = format!("{}: {}", name, value.to_string_rec(child_tabs));
                    Self::emit_item(&mut s, child_tabs, &value.leading, &main, &value.trailing);
                }
                s.push_str(&Self::dangling_block(dangling, child_tabs));
                s.push_str(&space(tabs));
                s.push(')');
                s
            }
        }
    }

    fn multiline(&self, tabs: usize) -> String {
        match &self.kind {
            Kind::Atom(atom) => atom.clone(),

            Kind::List { values, .. } => {
                let elements = values
                    .iter()
                    .map(|e| space(tabs + 1) + &e.to_string_rec(tabs + 1) + ",\n")
                    .collect::<String>();
                format!("[\n{}{}]", elements, space(tabs))
            }

            Kind::Map { entries, .. } => {
                let entries = entries.iter().fold(String::new(), |mut s, (k, v)| {
                    writeln!(
                        s,
                        "{}: {},",
                        space(tabs + 1) + &k.to_string_rec(tabs + 1),
                        v.to_string_rec(tabs + 1)
                    )
                    .expect("`write!`ing to a `String` never fails");
                    s
                });
                format!("{{\n{}{}}}", entries, space(tabs))
            }

            Kind::TupleType {
                ident, values, ..
            } => {
                let ident = ident.clone().unwrap_or_default();
                let elements = values
                    .iter()
                    .map(|e| space(tabs + 1) + &e.to_string_rec(tabs + 1) + ",\n")
                    .collect::<String>();
                format!("{}(\n{}{})", ident, elements, space(tabs))
            }

            Kind::FieldsType {
                ident, fields, ..
            } => {
                let ident = ident.clone().unwrap_or_default();
                let fields = fields.iter().fold(String::new(), |mut s, f| {
                    writeln!(
                        s,
                        "{}: {},",
                        space(tabs + 1) + &f.name,
                        f.value.to_string_rec(tabs + 1)
                    )
                    .expect("`write!`ing to a `String` never fails");
                    s
                });
                format!("{}(\n{}{})", ident, fields, space(tabs))
            }
        }
    }

    fn single_line(&self) -> String {
        match &self.kind {
            Kind::Atom(atom) => atom.clone(),

            Kind::List { values, .. } => {
                format!("[{}]", values.iter().map(Self::single_line).join(", "))
            }

            Kind::Map { entries, .. } => format!(
                "{{{}}}",
                entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k.single_line(), v.single_line()))
                    .join(", ")
            ),

            Kind::TupleType {
                ident, values, ..
            } => {
                let ident = ident.clone().unwrap_or_default();
                format!(
                    "{}({})",
                    ident,
                    values.iter().map(Self::single_line).join(", ")
                )
            }

            Kind::FieldsType {
                ident, fields, ..
            } => {
                let ident = ident.clone().unwrap_or_default();
                let fields = fields
                    .iter()
                    .map(|f| format!("{}: {}", f.name, f.value.single_line()))
                    .join(", ");
                format!("{ident}({fields})")
            }
        }
    }
}
