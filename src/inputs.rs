//! Turning the command line's paths into the inputs to format.

use std::path::{Path, PathBuf};

use crate::Error;

/// One thing to format.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Input {
    /// Standard input, written to standard output.
    Stdin,
    /// A file, formatted in place (or printed with `-d`).
    File(PathBuf),
}

/// Expand the given paths: `-` (or no paths at all) is stdin, a file is
/// itself even if a `.gitignore` would exclude it, and a directory is every
/// `*.ron` file below it that git would not ignore, hidden ones excluded,
/// in sorted order. Paths that do not exist or cannot be walked are
/// returned as errors; the other inputs are still collected.
pub fn collect(paths: &[PathBuf]) -> (Vec<Input>, Vec<Error>) {
    if paths.is_empty() {
        return (vec![Input::Stdin], Vec::new());
    }
    let (mut inputs, mut errors) = (Vec::new(), Vec::new());
    for path in paths {
        if path.as_os_str() == "-" {
            if !inputs.contains(&Input::Stdin) {
                inputs.push(Input::Stdin);
            }
        } else if path.is_dir() {
            let mut found = Vec::new();
            for entry in ignore::WalkBuilder::new(path).build() {
                match entry {
                    Ok(e) if e.file_type().is_some_and(|t| t.is_file()) && is_ron(e.path()) => {
                        found.push(e.into_path());
                    }
                    Ok(_) => {}
                    Err(e) => errors.push(Error::Walk(e)),
                }
            }
            found.sort();
            inputs.extend(found.into_iter().map(Input::File));
        } else if path.exists() {
            inputs.push(Input::File(path.clone()));
        } else {
            errors.push(Error::Missing(path.clone()));
        }
    }
    (inputs, errors)
}

fn is_ron(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "ron")
}
