use std::ffi::OsString;
use std::process::ExitCode;

use clap::Parser as ClapParser;

use arguments::Arguments;
use fmtron::Config;

mod arguments;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = Arguments::parse();
    let config = Config {
        tab_size: args.tab_size,
        max_width: args.width,
    };

    let file = std::fs::read_to_string(&args.input)
        .map_err(|e| format!("unable to read {}: {e}", args.input.display()))?;

    let formatted =
        fmtron::format_ron(&file, &config).map_err(|e| format!("unable to parse RON:\n{e}"))?;

    if args.debug {
        println!("{formatted}");
    } else {
        let mut backup = OsString::from(&args.input);
        backup.push(".bak");
        std::fs::copy(&args.input, &backup)
            .map_err(|e| format!("unable to create backup {}: {e}", backup.to_string_lossy()))?;

        std::fs::write(&args.input, formatted)
            .map_err(|e| format!("unable to write {}: {e}", args.input.display()))?;
    }

    Ok(())
}
