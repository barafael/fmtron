use std::ffi::OsString;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser as ClapParser;
use thiserror::Error;

use arguments::Arguments;
use fmtron::{Config, FileConfig};

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
    #[error("unable to write to stdout: {0}")]
    Stdout(std::io::Error),
    #[error("unable to parse RON:\n{0}")]
    Format(fmtron::FormatError),
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error(transparent)]
    ConfigFile(#[from] fmtron::FileConfigError),
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
    let (config, config_path) = resolve_config(&args)?;
    if config.tab_size > config.max_tab {
        return Err(Error::Config(format!(
            "tab size {} (-t / tab_size) exceeds the ceiling of {} (--max-tab / max_tab)",
            config.tab_size, config.max_tab
        )));
    }
    if args.print_config {
        let source = config_path.map_or("none (built-in defaults and flags)".into(), |p| {
            p.display().to_string()
        });
        print!(
            "// Effective fmtron configuration. Config file: {source}\n{}",
            config.to_file_config_string()
        );
        return Ok(());
    }
    // `--input` is required unless `--print-config` is given.
    let input = args.input.expect("clap enforces --input");

    let file = std::fs::read_to_string(&input).map_err(|source| Error::Read {
        path: input.clone(),
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
        let mut stdout = std::io::stdout().lock();
        match stdout
            .write_all(formatted.as_bytes())
            .and_then(|()| stdout.flush())
        {
            // The reader went away (e.g. `fmtron -d … | head`): not a failure.
            Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {}
            result => result.map_err(Error::Stdout)?,
        }
    } else {
        let mut backup = OsString::from(&input);
        backup.push(".bak");
        std::fs::copy(&input, &backup).map_err(|source| Error::Backup {
            backup: backup.into(),
            source,
        })?;

        std::fs::write(&input, formatted).map_err(|source| Error::Write {
            path: input.clone(),
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

/// Built-in defaults, overridden by the config file (`--config`, or the
/// nearest `fmt.ron` above the input unless `--no-config`), overridden by
/// flags. Also returns the config file used, if any.
fn resolve_config(args: &Arguments) -> Result<(Config, Option<PathBuf>), Error> {
    let mut config = Config::default();
    let path = if args.no_config {
        None
    } else if let Some(path) = &args.config {
        Some(path.clone())
    } else {
        FileConfig::find(&search_start(args.input.as_deref()))
    };
    if let Some(path) = &path {
        FileConfig::load(path)?.apply(&mut config);
    }
    let flags = FileConfig {
        max_width: args.width,
        tab_size: args.tab_size,
        blank_lines: args.blank_lines.map(Into::into),
        max_depth: args.max_depth,
        max_tab: args.max_tab,
    };
    flags.apply(&mut config);
    Ok((config, path))
}

/// The directory to search for `fmt.ron` from: the input file's directory,
/// or the current directory without an input.
fn search_start(input: Option<&Path>) -> PathBuf {
    let dir = input
        .and_then(Path::parent)
        .filter(|d| !d.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf())
}
