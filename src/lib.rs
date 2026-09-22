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

/// The error type returned by [`format_ron`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FormatError {
    /// The input contains no RON value.
    #[error("no RON data found")]
    Empty,
    /// The input is not valid RON. The inner error carries line/column
    /// information and a rendering of the offending input position.
    #[error("parse error: {0}")]
    Parse(#[from] Box<pest::error::Error<Rule>>),
}

/// Formats a RON string using the internal formatter.
///
/// # Errors
/// Returns [`FormatError::Empty`] if the input contains no RON value, and
/// [`FormatError::Parse`] if the input cannot be parsed as RON.
pub fn format_ron(input: &str, config: &Config) -> Result<String, FormatError> {
    match RonParser::parse(Rule::ron_file, input) {
        Ok(mut pairs) => match pairs.next() {
            Some(pair) => Ok(RonFile::parse_from(pair, input, *config).to_string()),
            None => Err(FormatError::Empty),
        },
        Err(e) => Err(Box::new(e).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_input_is_a_typed_parse_error() {
        let err = format_ron("not valid ((((", &Config::default()).unwrap_err();
        assert!(matches!(err, FormatError::Parse(_)), "got {err:?}");
        assert!(err.to_string().starts_with("parse error:"));
    }

    #[test]
    fn parse_error_chains_to_the_pest_source() {
        let err = format_ron("((( ", &Config::default()).unwrap_err();
        let source = std::error::Error::source(&err);
        assert!(source.is_some(), "parse errors must expose their source");
    }
}
