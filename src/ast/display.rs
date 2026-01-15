use super::{Kind, RonFile, Value};
use crate::{MAX_LINE_WIDTH, TAB_SIZE};
use itertools::Itertools;
use std::fmt::Write;
use std::ops::Not;
use std::{
    fmt::{self, Display, Formatter},
    sync::atomic::Ordering,
};

impl Display for RonFile {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let Self(extensions, value) = self;
        if extensions.is_empty().not() {
            writeln!(f, "#![enable({})]", extensions.iter().join(", "))?;
        }
        write!(f, "{}", value.to_string_rec(0))
    }
}

fn space(level: usize) -> String {
    " ".repeat(TAB_SIZE.load(Ordering::SeqCst) * level)
}

impl Value {
    fn to_string_rec(&self, tabs: usize) -> String {
        if tabs * TAB_SIZE.load(Ordering::SeqCst) + self.0 > MAX_LINE_WIDTH.load(Ordering::SeqCst) {
            self.multiline(tabs)
        } else {
            self.single_line()
        }
    }

    fn multiline(&self, tabs: usize) -> String {
        match &self.1 {
            Kind::Atom(atom) => atom.clone(),

            Kind::List(values) => {
                let elements = values
                    .iter()
                    .map(|e| space(tabs + 1) + &e.to_string_rec(tabs + 1) + ",\n")
                    .collect::<String>();

                format!("[\n{}{}]", elements, space(tabs))
            }

            Kind::Map(entries) => {
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

            Kind::TupleType(ident, values) => {
                let ident = ident.clone().unwrap_or_default();
                let elements = values
                    .iter()
                    .map(|e| space(tabs + 1) + &e.to_string_rec(tabs + 1) + ",\n")
                    .collect::<String>();

                format!("{}(\n{}{})", ident, elements, space(tabs))
            }

            Kind::FieldsType(ident, fields) => {
                let ident = ident.clone().unwrap_or_default();
                let fields = fields.iter().fold(String::new(), |mut s, (k, v)| {
                    writeln!(s, "{}: {},", space(tabs + 1) + k, v.to_string_rec(tabs + 1))
                        .expect("`write!`ing to a `String` never fails");
                    s
                });

                format!("{}(\n{}{})", ident, fields, space(tabs))
            }
        }
    }

    fn single_line(&self) -> String {
        match &self.1 {
            Kind::Atom(atom) => atom.clone(),

            Kind::List(elements) => {
                format!("[{}]", elements.iter().map(Self::single_line).join(", "))
            }

            Kind::Map(entries) => format!(
                "{{{}}}",
                entries
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k.single_line(), v.single_line()))
                    .join(", ")
            ),

            Kind::TupleType(ident, elements) => {
                let ident = ident.clone().unwrap_or_default();
                format!(
                    "{}({})",
                    ident,
                    elements.iter().map(Self::single_line).join(", ")
                )
            }

            Kind::FieldsType(ident, fields) => {
                let ident = ident.clone().unwrap_or_default();
                let fields = fields
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, v.single_line()))
                    .join(", ");
                format!("{ident}({fields})")
            }
        }
    }
}
