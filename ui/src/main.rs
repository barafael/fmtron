//! fmt.ron.ui: an editor for fmtron's `fmt.ron` configuration file.
//!
//! Opens the `fmt.ron` in the current directory (or the file given), shows
//! its fields as a form beside a preview of their effect, and writes the
//! file back with its comments intact.

#![forbid(unsafe_code)]

mod app;
mod command;
mod document;
mod field;
mod file;
mod patch;
mod state;
mod ui;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let path = match cli::parse(std::env::args_os().skip(1)) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let document = match file::initial(path) {
        Ok(document) => document,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    let viewport = egui::ViewportBuilder::default()
        .with_title("fmt.ron.ui")
        .with_app_id("fmtron-ui")
        .with_inner_size([880.0, 560.0])
        .with_min_inner_size([600.0, 400.0]);
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };
    let result = eframe::run_native(
        "fmt.ron.ui",
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, document)))),
    );
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}

/// The command line: one optional file.
#[cfg(not(target_arch = "wasm32"))]
mod cli {
    use std::ffi::OsString;
    use std::path::PathBuf;

    const USAGE: &str = "\
Usage: fmtron-ui [FILE]

Edit a fmt.ron in a window. Without FILE, edits the fmt.ron (or .fmt.ron)
in the current directory, creating fmt.ron on save if there is none.

  -h, --help     Print this help
  -V, --version  Print the version";

    pub fn parse(args: impl Iterator<Item = OsString>) -> Result<Option<PathBuf>, String> {
        let mut path = None;
        for arg in args {
            match arg.to_str() {
                Some("-h" | "--help") => {
                    println!("{USAGE}");
                    std::process::exit(0);
                }
                Some("-V" | "--version") => {
                    println!("fmtron-ui {}", env!("CARGO_PKG_VERSION"));
                    std::process::exit(0);
                }
                Some(flag) if flag.starts_with('-') && flag != "-" => {
                    return Err(format!("unknown option {flag}\n\n{USAGE}"));
                }
                _ if path.is_some() => return Err(format!("only one FILE is taken\n\n{USAGE}")),
                _ => path = Some(PathBuf::from(arg)),
            }
        }
        Ok(path)
    }
}
