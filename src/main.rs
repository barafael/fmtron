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
    #[error("unable to parse RON:\n{0}")]
    Format(#[from] fmtron::FormatError),
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
    let config = Config {
        tab_size: args.tab_size,
        max_width: args.width,
    };

    let file = std::fs::read_to_string(&args.input).map_err(|source| Error::Read {
        path: args.input.clone(),
        source,
    })?;

    let formatted = fmtron::format_ron(&file, &config)?;

    if args.debug {
        println!("{formatted}");
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
