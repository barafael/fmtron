use std::sync::atomic::AtomicUsize;

mod ast;

pub use ast::{Kind, RonFile, Value};

pub static TAB_SIZE: AtomicUsize = AtomicUsize::new(4);
pub static MAX_LINE_WIDTH: AtomicUsize = AtomicUsize::new(40);

use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "ron.pest"]
pub struct RonParser;

pub use pest::Parser;

/// Formats a RON string using the internal formatter.
/// Returns Ok with the formatted string, or Err with a message if parsing fails.
pub fn format_ron(input: &str) -> Result<String, String> {
    // Use the internal RonParser and RonFile, but do not expose pest details
    match RonParser::parse(Rule::ron_file, input) {
        Ok(mut pairs) => {
            if let Some(pair) = pairs.next() {
                Ok(format!("{}", crate::ast::RonFile::parse_from(pair)))
            } else {
                Err("No RON data found".to_string())
            }
        }
        Err(e) => Err(format!("Parse error: {}", e)),
    }
}
