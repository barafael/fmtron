use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser as ClapParser;
use thiserror::Error;

use arguments::Arguments;
use fmtron::Config;

mod arguments;

/// Errors that can occur while formatting a file from the command line.
#[derive(Debug, Error)]
enum Error {
    #[error("unable to read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("unable to create backup {}: {source}", backup.display())]
    Backup {
        backup: PathBuf,
        source: std::io::Error,
    },
    #[error("unable to write {}: {source}", path.display())]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{0}; raise the limit with --max-depth")]
    TooDeep(fmtron::FormatError),
    #[error("{0}; use a smaller --tab-size")]
    TooWide(fmtron::FormatError),
    #[error("unable to parse RON:\n{0}")]
    Format(fmtron::FormatError),
    #[error("invalid configuration: {0}")]
    Config(String),
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Error> {
    let args = Arguments::parse();
    if args.tab_size > args.max_tab {
        return Err(Error::Config(format!(
            "--tab-size {} exceeds the --max-tab ceiling of {}",
            args.tab_size, args.max_tab
        )));
    }
    let config = Config {
        tab_size: args.tab_size,
        max_width: args.width,
        max_nesting: args.max_depth,
        max_tab: args.max_tab,
    };

    let file = std::fs::read_to_string(&args.input).map_err(|source| Error::Read {
        path: args.input.clone(),
        source,
    })?;

    let formatted = format_on_sized_stack(&file, &config)?.map_err(|e| match e {
        fmtron::FormatError::TooDeep { .. } => Error::TooDeep(e),
        fmtron::FormatError::IndentTooWide { .. } => Error::TooWide(e),
        e => Error::Format(e),
    })?;
    // Emit a text file: exactly one final newline, whether or not the output
    // ends in a comment line (which the formatter already terminates).
    let formatted = format!(
        "{}{}",
        formatted.trim_end_matches(['\r', '\n']),
        fmtron::line_ending(&file)
    );

    if args.debug {
        print!("{formatted}");
    } else {
        let mut backup = OsString::from(&args.input);
        backup.push(".bak");
        std::fs::copy(&args.input, &backup).map_err(|source| Error::Backup {
            backup: backup.into(),
            source,
        })?;

        std::fs::write(&args.input, formatted).map_err(|source| Error::Write {
            path: args.input.clone(),
            source,
        })?;
    }

    Ok(())
}

/// Stack reserved per level of `--max-depth`. Measured at about 2.7 KiB per
/// nesting level in release builds and 9.8 KiB in debug builds.
const STACK_PER_LEVEL: usize = 16 * 1024;
const STACK_BASE: usize = 1 << 20;

/// Format on a thread whose stack grows with `--max-depth`, so raising the
/// depth limit admits deeper input instead of letting it overflow the stack.
fn format_on_sized_stack(
    input: &str,
    config: &Config,
) -> Result<Result<String, fmtron::FormatError>, Error> {
    let too_big = || {
        Error::Config(format!(
            "--max-depth {} needs more stack than can be reserved",
            config.max_nesting
        ))
    };
    let stack = config
        .max_nesting
        .checked_mul(STACK_PER_LEVEL)
        .and_then(|s| s.checked_add(STACK_BASE))
        .ok_or_else(too_big)?;
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(stack)
            .spawn_scoped(scope, || fmtron::format_ron(input, config))
            .map_err(|_| too_big())?
            .join()
            .map_or_else(|panic| std::panic::resume_unwind(panic), Ok)
    })
}
