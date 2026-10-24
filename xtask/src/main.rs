//! Repository automation, run as `cargo xtask <task>`.
//!
//! All of the repository's tooling lives here, so that it builds and lints
//! with the rest of the workspace.

mod bless;
mod build_matrix_site;
mod compare_styles;
mod matrix_audit;
mod oracle;
mod util;
mod wild;

use std::process::ExitCode;

const USAGE: &str = "\
usage: cargo xtask <task>

tasks:
  bless                              regenerate every golden output after an
                                     intended formatting change
  matrix-audit [--extended] [--dump <dir>]
                                     check every golden-matrix cell and count
                                     layouts worth a look
  build-matrix-site                  copy the golden matrix into www/ and index it
  compare-styles <out> <name>=<bin>... -- <dir>...
                                     compare fmtron builds over a corpus
  wild gh-collect coarse <pages> <bounds...>
  wild gh-collect <lo> <hi> <pages>  index .ron files with GitHub code search
  wild gh-download                   download the indexed GitHub files
  wild crates-collect [pages]        collect .ron files from ron reverse deps
  wild forge-collect <gitlab|codeberg>
  wild codeberg-collect              broad Codeberg collection
  wild evaluate                      run the wild harness and summarize
  wild build-corpus                  rebuild test_data/wild from results.jsonl

The wild tasks use $FMTRON_WILD_WORK (default ./wild-work) as the work
directory; see xtask/src/wild/README.md.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("bless") => bless::run(&args[1..]),
        Some("matrix-audit") => matrix_audit::run(&args[1..]),
        Some("build-matrix-site") => build_matrix_site::run(&args[1..]),
        Some("compare-styles") => compare_styles::run(&args[1..]),
        Some("wild") => wild::run(&args[1..]),
        Some("help" | "-h" | "--help") | None => {
            print!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Some(other) => Err(format!("unknown task `{other}`\n\n{USAGE}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}
