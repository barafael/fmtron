//! `fmt.ron`: per-project formatting configuration, like `rustfmt.toml`.
//!
//! ```ron
//! // fmt.ron
//! (
//!     max_width: 100,
//!     tab_size: 4,
//!     blank_lines: Keep, // or Remove
//! )
//! ```
//!
//! Every field is optional; a missing field keeps its default. Unknown
//! fields are an error, so a typo cannot silently do nothing. Values may be
//! written bare (`max_width: 100`) or as `Some(100)`.
//!
//! [`FileConfig::find`] looks for `fmt.ron`, then `.fmt.ron`, in a directory
//! and each of its ancestors; the nearest file wins and files are not merged.

use std::path::{Path, PathBuf};

use ron::extensions::Extensions;
use serde::Deserialize;

use crate::{BlankLines, Config};

/// The file names looked for in each directory, in order of preference.
pub const CONFIG_FILE_NAMES: [&str; 2] = ["fmt.ron", ".fmt.ron"];

/// The settings in a `fmt.ron`. `None` means "not set here".
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileConfig {
    /// Soft maximum line width ([`Config::max_width`], CLI `-w`).
    pub max_width: Option<usize>,
    /// Indentation size in spaces ([`Config::tab_size`], CLI `-t`).
    pub tab_size: Option<usize>,
    /// Blank-line handling ([`Config::blank_lines`], CLI `--blank-lines`).
    pub blank_lines: Option<BlankLines>,
    /// Maximum nesting depth ([`Config::max_nesting`], CLI `--max-depth`).
    pub max_depth: Option<usize>,
    /// Upper bound on the tab size ([`Config::max_tab`], CLI `--max-tab`).
    pub max_tab: Option<usize>,
}

/// A `fmt.ron` that could not be read or parsed.
#[derive(Debug, thiserror::Error)]
pub enum FileConfigError {
    #[error("unable to read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("invalid {}: {source}", path.display())]
    Parse {
        path: PathBuf,
        source: Box<ron::error::SpannedError>,
    },
}

/// Parses the contents of a `fmt.ron`. Fails with the `ron` error, with its
/// position, for malformed input, unknown fields and values of the wrong type.
impl std::str::FromStr for FileConfig {
    type Err = ron::error::SpannedError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        ron::Options::default()
            .with_default_extension(Extensions::IMPLICIT_SOME)
            .from_str(s)
    }
}

impl FileConfig {
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
            source: Box::new(source),
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
            config.max_nesting = v;
        }
        if let Some(v) = max_tab {
            config.max_tab = v;
        }
    }
}

impl Config {
    /// This configuration written as a complete `fmt.ron`.
    pub fn to_file_config_string(&self) -> String {
        format!(
            "(\n    max_width: {},\n    tab_size: {},\n    blank_lines: {:?},\n    \
             max_depth: {},\n    max_tab: {},\n)\n",
            self.max_width, self.tab_size, self.blank_lines, self.max_nesting, self.max_tab
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn printed_config_parses_back() {
        let config = Config {
            max_width: 80,
            blank_lines: BlankLines::Remove,
            ..Config::default()
        };
        let file: FileConfig = config.to_file_config_string().parse().unwrap();
        let mut round = Config::default();
        file.apply(&mut round);
        assert_eq!(
            round.to_file_config_string(),
            config.to_file_config_string()
        );
    }
}
