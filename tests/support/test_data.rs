//! The tests' data under `test_data/`. It is in fmtron's repository but not in
//! the published crate, so a missing path fails with a hint rather than a bare
//! "No such file", or a directory walk that silently finds nothing to check.

use std::path::Path;

/// `path`, relative to the package root, after checking that it exists.
#[track_caller]
pub fn require<P: AsRef<Path> + ?Sized>(path: &P) -> &P {
    assert!(
        path.as_ref().exists(),
        "{} not found. fmtron's tests need its test data, which is in the \
         repository but not in the published crate: run them from a clone of \
         https://github.com/barafael/fmtron",
        path.as_ref().display()
    );
    path
}
