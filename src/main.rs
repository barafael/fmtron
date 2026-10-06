use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser as ClapParser;
use thiserror::Error;

use arguments::Arguments;
use fmtron::{Config, FileConfig, FormatError};
use inputs::Input;

mod arguments;
mod inputs;

/// Errors that can occur while formatting from the command line. Errors about
/// one input name it; the run continues with the other inputs.
#[derive(Debug, Error)]
pub enum Error {
    #[error("unable to read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("unable to read stdin: {0}")]
    Stdin(std::io::Error),
    #[error("{}: no such file or directory", .0.display())]
    Missing(PathBuf),
    #[error("{0}")]
    Walk(ignore::Error),
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
    #[error("{name}: {source}; raise the limit with --max-depth")]
    TooDeep { name: String, source: FormatError },
    #[error("{name}: {source}; use a smaller --tab-size")]
    TooWide { name: String, source: FormatError },
    /// A parse error; the pest excerpt names the input (`--> path:line:col`).
    #[error("unable to parse RON:\n{0}")]
    Parse(FormatError),
    #[error("{name}: {source}")]
    Format { name: String, source: FormatError },
    #[error("unable to write to stdout: {0}")]
    Stdout(std::io::Error),
    #[error("{0}")]
    Usage(String),
    #[error("invalid configuration: {0}")]
    Config(String),
    #[error(transparent)]
    ConfigFile(#[from] fmtron::FileConfigError),
}

fn main() -> ExitCode {
    let args = Arguments::parse();
    match run(&args) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// What formatting one input came to.
enum Outcome {
    /// Already formatted, or printed to stdout.
    Done,
    /// Rewritten in place.
    Changed,
    /// `--check`: would change (a diff was printed).
    WouldChange,
}

/// Format every input. Returns whether everything succeeded (and, with
/// `--check`, was already formatted); per-input errors are printed as they
/// occur. Fails outright only for problems that concern the whole run.
fn run(args: &Arguments) -> Result<bool, Error> {
    let (inputs, errors) = inputs::collect(&args.paths);
    let mut ok = errors.is_empty();
    for e in errors {
        eprintln!("{e}");
    }

    if args.print_config {
        let dir = match inputs.first() {
            Some(Input::File(path)) => config_dir(path),
            _ => stdin_config_dir(args),
        };
        let (config, path) = resolve_config(args, &dir)?;
        let source = path.map_or("none (built-in defaults and flags)".into(), |p| {
            p.display().to_string()
        });
        print_stdout(&format!(
            "// Effective fmtron configuration. Config file: {source}\n{}",
            config.to_file_config_string()
        ))?;
        return Ok(ok);
    }
    if args.stdout && inputs.len() > 1 {
        return Err(Error::Usage(format!(
            "-d/--stdout prints a single input, but {} were given; use --check to review several",
            inputs.len()
        )));
    }

    let mut configs: HashMap<PathBuf, Result<Config, String>> = HashMap::new();
    for input in &inputs {
        let dir = match input {
            Input::File(path) => config_dir(path),
            Input::Stdin => stdin_config_dir(args),
        };
        let config = configs
            .entry(dir)
            .or_insert_with_key(|dir| checked_config(args, dir).map_err(|e| e.to_string()));
        let result = match config {
            Ok(config) => process(args, input, config),
            Err(e) => Err(Error::Config(e.clone())),
        };
        match result {
            Ok(Outcome::Done | Outcome::Changed) => {}
            Ok(Outcome::WouldChange) => ok = false,
            Err(e @ Error::Stdout(_)) => return Err(e),
            Err(e) => {
                eprintln!("{e}");
                ok = false;
            }
        }
    }
    Ok(ok)
}

/// Format one input and deliver the result: a diff with `--check`, stdout
/// for stdin and `-d`, otherwise the file rewritten if it changed.
fn process(args: &Arguments, input: &Input, config: &Config) -> Result<Outcome, Error> {
    let name = display_name(args, input);
    let text = match input {
        Input::Stdin => {
            let mut text = String::new();
            std::io::stdin()
                .read_to_string(&mut text)
                .map_err(Error::Stdin)?;
            text
        }
        Input::File(path) => std::fs::read_to_string(path).map_err(|source| Error::Read {
            path: path.clone(),
            source,
        })?,
    };

    let formatted = format_on_sized_stack(&text, config)?.map_err(|e| match e {
        FormatError::TooDeep { .. } => Error::TooDeep {
            name: name.clone(),
            source: e,
        },
        FormatError::IndentTooWide { .. } => Error::TooWide {
            name: name.clone(),
            source: e,
        },
        FormatError::Parse(e) => Error::Parse(FormatError::Parse(e.with_path(&name))),
        e => Error::Format {
            name: name.clone(),
            source: e,
        },
    })?;
    // Emit a text file: exactly one final newline, whether or not the output
    // ends in a comment line (which the formatter already terminates).
    let formatted = format!(
        "{}{}",
        formatted.trim_end_matches(['\r', '\n']),
        fmtron::line_ending(&text)
    );

    if args.check {
        if formatted == text {
            return Ok(Outcome::Done);
        }
        let diff = similar::TextDiff::from_lines(&text, &formatted);
        print_stdout(&format!(
            "Diff in {name}:\n{}",
            diff.unified_diff().context_radius(3).header(&name, &name)
        ))?;
        return Ok(Outcome::WouldChange);
    }
    match input {
        Input::Stdin => print_stdout(&formatted).map(|()| Outcome::Done),
        Input::File(_) if args.stdout => print_stdout(&formatted).map(|()| Outcome::Done),
        Input::File(_) if formatted == text => Ok(Outcome::Done),
        Input::File(path) => {
            write_in_place(path, &formatted, args.backup)?;
            Ok(Outcome::Changed)
        }
    }
}

/// The name an input goes by in messages and diffs.
fn display_name(args: &Arguments, input: &Input) -> String {
    match input {
        // Walking `.` yields `./a.ron`; name it `a.ron`.
        Input::File(path) => path.strip_prefix(".").unwrap_or(path).display().to_string(),
        Input::Stdin => args
            .stdin_filepath
            .as_ref()
            .map_or("<stdin>".into(), |p| p.display().to_string()),
    }
}

/// Write to stdout. A reader that went away (e.g. `fmtron -d … | head`) is
/// not a failure.
fn print_stdout(s: &str) -> Result<(), Error> {
    let mut stdout = std::io::stdout().lock();
    match stdout.write_all(s.as_bytes()).and_then(|()| stdout.flush()) {
        Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => Ok(()),
        result => result.map_err(Error::Stdout),
    }
}

/// Replace `path`'s contents with `contents` atomically: write a temporary
/// file next to the target, give it the original's permissions, and rename
/// it over the target. A symlink's target is replaced, so the link
/// survives. With `backup`, the original is first copied to `<path>.bak`.
fn write_in_place(path: &Path, contents: &str, backup: bool) -> Result<(), Error> {
    let write_err = |source| Error::Write {
        path: path.to_path_buf(),
        source,
    };
    let target = std::fs::canonicalize(path).map_err(write_err)?;
    let permissions = std::fs::metadata(&target).map_err(write_err)?.permissions();
    if permissions.readonly() {
        return Err(write_err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "file is read-only",
        )));
    }
    if backup {
        let mut bak = OsString::from(path);
        bak.push(".bak");
        std::fs::copy(path, &bak).map_err(|source| Error::Backup {
            backup: bak.into(),
            source,
        })?;
    }
    let dir = target.parent().unwrap_or(Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(write_err)?;
    tmp.write_all(contents.as_bytes()).map_err(write_err)?;
    tmp.as_file().sync_all().map_err(write_err)?;
    tmp.as_file()
        .set_permissions(permissions)
        .map_err(write_err)?;
    tmp.persist(&target).map_err(|e| write_err(e.error))?;
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
) -> Result<Result<String, FormatError>, Error> {
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

/// [`resolve_config`], rejecting a tab size above its ceiling.
fn checked_config(args: &Arguments, dir: &Path) -> Result<Config, Error> {
    let (config, _) = resolve_config(args, dir)?;
    if config.tab_size > config.max_tab {
        return Err(Error::Config(format!(
            "tab size {} (-t / tab_size) exceeds the ceiling of {} (--max-tab / max_tab)",
            config.tab_size, config.max_tab
        )));
    }
    Ok(config)
}

/// Built-in defaults, overridden by the config file (`--config`, or the
/// nearest `fmt.ron` at or above `dir` unless `--no-config`), overridden by
/// flags. Also returns the config file used, if any.
fn resolve_config(args: &Arguments, dir: &Path) -> Result<(Config, Option<PathBuf>), Error> {
    let mut config = Config::default();
    let path = if args.no_config {
        None
    } else if let Some(path) = &args.config {
        Some(path.clone())
    } else {
        FileConfig::find(dir)
    };
    if let Some(path) = &path {
        FileConfig::load(path)?.apply(&mut config);
    }
    let mut flags = FileConfig::default();
    flags.max_width = args.width;
    flags.tab_size = args.tab_size;
    flags.blank_lines = args.blank_lines.map(Into::into);
    flags.max_depth = args.max_depth;
    flags.max_tab = args.max_tab;
    flags.style_edition = args.style_edition;
    flags.apply(&mut config);
    Ok((config, path))
}

/// The directory to search for a file's `fmt.ron` from: its own.
fn config_dir(path: &Path) -> PathBuf {
    absolute(
        path.parent()
            .filter(|d| !d.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
    )
}

/// For stdin: the directory of `--stdin-filepath`, else the current one.
fn stdin_config_dir(args: &Arguments) -> PathBuf {
    args.stdin_filepath
        .as_deref()
        .map_or_else(|| absolute(Path::new(".")), config_dir)
}

fn absolute(dir: &Path) -> PathBuf {
    std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf())
}
