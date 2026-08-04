mod ast;
pub mod pretty;

pub use ast::{Kind, RonFile, Value};

use pest_derive::Parser;

#[derive(Parser)]
#[grammar = "ron.pest"]
pub struct RonParser;

pub use pest::Parser;

/// Formatting configuration. Threaded through the formatter instead of using
/// process-wide global state.
#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub tab_size: usize,
    pub max_width: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            tab_size: 4,
            max_width: 40,
        }
    }
}

/// Formats a RON string using the internal formatter.
/// Returns Ok with the formatted string, or Err with a message if parsing fails.
pub fn format_ron(input: &str, config: &Config) -> Result<String, String> {
    match RonParser::parse(Rule::ron_file, input) {
        Ok(mut pairs) => {
            if let Some(pair) = pairs.next() {
                Ok(format!(
                    "{}",
                    crate::ast::RonFile::parse_from(pair, input, *config)
                ))
            } else {
                Err("No RON data found".to_string())
            }
        }
        Err(e) => Err(format!("Parse error: {}", e)),
    }
}
