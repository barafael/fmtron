//! `fmt.ron`: per-project formatting configuration, like `rustfmt.toml`.
//!
//! ```ron
//! // fmt.ron
//! (
//!     max_width: 100,
//!     tab_size: 4,
//!     blank_lines: Keep, // or Remove
//!     max_depth: 512,
//!     max_tab: 1024,
//!     style_edition: 2026,
//! )
//! ```
//!
//! Every field is optional; a missing field keeps its default. Unknown
//! fields are an error, so a typo cannot silently do nothing. Values may be
//! written bare (`max_width: 100`) or as `Some(100)`.
//!
//! Sizes must be usable: `max_width`, `tab_size`, and `max_tab` are at least
//! 1, and `tab_size` may not exceed `max_tab`. The library still clamps these
//! values defensively when a [`Config`] is built programmatically, but a
//! `fmt.ron` rejects them so a typo cannot silently change the layout.
//!
//! [`FileConfig::find`] looks for `fmt.ron`, then `.fmt.ron`, in a directory
//! and each of its ancestors; the nearest file wins and files are not merged.

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};

use ron::extensions::Extensions;
use serde::Deserialize;

use crate::{BlankLines, Config, StyleEdition};

/// The file names looked for in each directory, in order of preference.
pub const CONFIG_FILE_NAMES: [&str; 2] = ["fmt.ron", ".fmt.ron"];

/// The settings in a `fmt.ron`. `None` means "not set here". Settings may be
/// added in minor releases, so the struct cannot be built with a literal:
/// start from [`FileConfig::default`].
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct FileConfig {
    /// Soft maximum line width ([`Config::max_width`], CLI `-w`). At least 1.
    pub max_width: Option<usize>,
    /// Indentation size in spaces ([`Config::tab_size`], CLI `-t`). At least
    /// 1, and at most `max_tab`.
    pub tab_size: Option<usize>,
    /// Blank-line handling ([`Config::blank_lines`], CLI `--blank-lines`).
    pub blank_lines: Option<BlankLines>,
    /// Maximum nesting depth ([`Config::max_nesting`], CLI `--max-depth`).
    /// Non-zero, so that `0` is an error here rather than silently acting
    /// as `1`.
    pub max_depth: Option<NonZeroUsize>,
    /// Upper bound on the tab size ([`Config::max_tab`], CLI `--max-tab`).
    /// At least 1, and at least `tab_size`.
    pub max_tab: Option<usize>,
    /// The layout rules ([`Config::style_edition`], CLI `--style-edition`).
    pub style_edition: Option<StyleEdition>,
}

/// A `fmt.ron` that could not be read or parsed.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum FileConfigError {
    #[error("unable to read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        source: ConfigParseError,
    },
}

/// Why the text of a `fmt.ron` is not a configuration: malformed RON, an
/// unknown field, a value of the wrong type, or a setting the formatter
/// cannot use. Its `Display` form names the position, as
/// `3:5: Unexpected field named `max_widht``.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigParseError {
    kind: ConfigParseErrorKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ConfigParseErrorKind {
    Ron(Box<ron::error::SpannedError>),
    Invalid {
        line: usize,
        column: usize,
        message: String,
    },
}

impl ConfigParseError {
    fn ron(error: ron::error::SpannedError) -> Self {
        Self {
            kind: ConfigParseErrorKind::Ron(Box::new(error)),
        }
    }

    fn invalid(text: &str, field: &str, message: String) -> Self {
        let (line, column) = field_position(text, field);
        Self {
            kind: ConfigParseErrorKind::Invalid {
                line,
                column,
                message,
            },
        }
    }

    /// The line of the error, counting from 1.
    pub fn line(&self) -> usize {
        match &self.kind {
            ConfigParseErrorKind::Ron(error) => error.span.start.line,
            ConfigParseErrorKind::Invalid { line, .. } => *line,
        }
    }

    /// The column of the error, counting from 1, in characters.
    pub fn column(&self) -> usize {
        match &self.kind {
            ConfigParseErrorKind::Ron(error) => error.span.start.col,
            ConfigParseErrorKind::Invalid { column, .. } => *column,
        }
    }
}

impl std::fmt::Display for ConfigParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ConfigParseErrorKind::Ron(error) => write!(f, "{error}"),
            ConfigParseErrorKind::Invalid {
                line,
                column,
                message,
            } => write!(f, "{line}:{column}: {message}"),
        }
    }
}

impl std::error::Error for ConfigParseError {}

/// Parses the contents of a `fmt.ron`.
///
/// # Errors
/// See [`ConfigParseError`], and [`FileConfig::validate`] for the settings
/// rules.
impl std::str::FromStr for FileConfig {
    type Err = ConfigParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let file: Self = ron::Options::default()
            .with_default_extension(Extensions::IMPLICIT_SOME)
            .from_str(s)
            .map_err(ConfigParseError::ron)?;
        file.check()
            .map_err(|invalid| ConfigParseError::invalid(s, invalid.field, invalid.message))?;
        Ok(file)
    }
}

/// A setting that parses but cannot be used.
struct InvalidSetting {
    field: &'static str,
    message: String,
}

/// The first occurrence of `field` in `text`, as a 1-based line and column.
fn field_position(text: &str, field: &str) -> (usize, usize) {
    for (line_index, line) in text.lines().enumerate() {
        if let Some(byte) = line.find(field) {
            return (line_index + 1, line[..byte].chars().count() + 1);
        }
    }
    (1, 1)
}

impl FileConfig {
    /// Whether this file's settings are usable.
    ///
    /// Every size is at least 1, and `tab_size` does not exceed `max_tab`.
    /// `max_depth` is already non-zero by its type.
    ///
    /// # Errors
    /// A message naming the setting and the rule it breaks.
    pub fn validate(&self) -> Result<(), String> {
        self.check().map_err(|invalid| invalid.message)
    }

    fn check(&self) -> Result<(), InvalidSetting> {
        let defaults = Config::default();
        let max_width = self.max_width.unwrap_or(defaults.max_width);
        let tab_size = self.tab_size.unwrap_or(defaults.tab_size);
        let max_tab = self.max_tab.unwrap_or(defaults.max_tab);
        if max_width == 0 {
            return Err(InvalidSetting {
                field: "max_width",
                message: "max_width must be at least 1".into(),
            });
        }
        if tab_size == 0 {
            return Err(InvalidSetting {
                field: "tab_size",
                message: "tab_size must be at least 1".into(),
            });
        }
        if max_tab == 0 {
            return Err(InvalidSetting {
                field: "max_tab",
                message: "max_tab must be at least 1".into(),
            });
        }
        if tab_size > max_tab {
            let field = if self.max_tab.is_some() {
                "max_tab"
            } else {
                "tab_size"
            };
            return Err(InvalidSetting {
                field,
                message: format!("tab_size {tab_size} exceeds max_tab {max_tab}"),
            });
        }
        Ok(())
    }

    /// Read and parse the `fmt.ron` at `path`.
    ///
    /// # Errors
    /// See [`FileConfigError`].
    pub fn load(path: &Path) -> Result<Self, FileConfigError> {
        let text = std::fs::read_to_string(path).map_err(|source| FileConfigError::Read {
            path: path.to_path_buf(),
            source,
        })?;
        text.parse().map_err(|source| FileConfigError::Parse {
            path: path.to_path_buf(),
            source,
        })
    }

    /// The nearest config file for files in `dir`: `dir`'s own `fmt.ron` or
    /// `.fmt.ron`, else its parent's, and so on up to the root.
    pub fn find(dir: &Path) -> Option<PathBuf> {
        dir.ancestors()
            .flat_map(|d| CONFIG_FILE_NAMES.iter().map(move |name| d.join(name)))
            .find(|p| p.is_file())
    }

    /// Overwrite the settings of `config` that this file sets.
    pub fn apply(&self, config: &mut Config) {
        let Self {
            max_width,
            tab_size,
            blank_lines,
            max_depth,
            max_tab,
            style_edition,
        } = *self;
        if let Some(v) = max_width {
            config.max_width = v;
        }
        if let Some(v) = tab_size {
            config.tab_size = v;
        }
        if let Some(v) = blank_lines {
            config.blank_lines = v;
        }
        if let Some(v) = max_depth {
            config.max_nesting = v.get();
        }
        if let Some(v) = max_tab {
            config.max_tab = v;
        }
        if let Some(v) = style_edition {
            config.style_edition = v;
        }
    }
}

impl Config {
    /// This configuration written as a complete `fmt.ron`.
    pub fn to_file_config_string(&self) -> String {
        format!(
            "(\n    max_width: {},\n    tab_size: {},\n    blank_lines: {:?},\n    \
             max_depth: {},\n    max_tab: {},\n    style_edition: {},\n)\n",
            self.max_width,
            self.tab_size,
            self.blank_lines,
            self.max_nesting,
            self.max_tab,
            self.style_edition
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printed_config_parses_back() {
        let config = Config::default()
            .with_max_width(80)
            .with_blank_lines(BlankLines::Remove);
        let file: FileConfig = config.to_file_config_string().parse().unwrap();
        let mut round = Config::default();
        file.apply(&mut round);
        assert_eq!(
            round.to_file_config_string(),
            config.to_file_config_string()
        );
    }

    #[test]
    fn a_zero_max_depth_is_an_error_with_a_position() {
        let err = "(\n    max_depth: 0,\n)".parse::<FileConfig>().unwrap_err();
        assert_eq!(err.line(), 2, "{err}");
        assert!(err.to_string().contains("nonzero"), "{err}");
    }

    #[test]
    fn an_unknown_style_edition_is_an_error_with_a_position() {
        let err = "(\n    style_edition: 2027,\n)"
            .parse::<FileConfig>()
            .unwrap_err();
        assert_eq!((err.line(), err.column()), (2, 20));
        assert!(
            err.to_string().contains("unknown style edition 2027"),
            "{err}"
        );
    }

    #[test]
    fn zero_sizes_are_errors_with_a_position() {
        for (field, text) in [
            ("max_width", "(\n    max_width: 0,\n)"),
            ("tab_size", "(\n    tab_size: 0,\n)"),
            ("max_tab", "(\n    max_tab: 0,\n)"),
        ] {
            let err = text.parse::<FileConfig>().unwrap_err();
            assert_eq!(err.line(), 2, "{field}: {err}");
            assert_eq!(err.column(), 5, "{field}: {err}");
            assert!(
                err.to_string()
                    .contains(&format!("{field} must be at least 1")),
                "{field}: {err}"
            );
        }
    }

    #[test]
    fn a_tab_size_above_its_ceiling_is_an_error_with_a_position() {
        let err = "(\n    tab_size: 8,\n    max_tab: 4,\n)"
            .parse::<FileConfig>()
            .unwrap_err();
        assert_eq!((err.line(), err.column()), (3, 5), "{err}");
        assert!(
            err.to_string().contains("tab_size 8 exceeds max_tab 4"),
            "{err}"
        );
    }

    #[test]
    fn validate_reports_the_same_rules() {
        let file = FileConfig {
            max_width: Some(0),
            ..FileConfig::default()
        };
        assert_eq!(file.validate().unwrap_err(), "max_width must be at least 1");
    }
}
