use std::{
    ffi::OsString,
    sync::atomic::{AtomicUsize, Ordering},
};

use clap::Parser as ClapParser;
use pest::Parser;
use pest_derive::Parser;

use arguments::Arguments;

static TAB_SIZE: AtomicUsize = AtomicUsize::new(4);
static MAX_LINE_WIDTH: AtomicUsize = AtomicUsize::new(40);

mod arguments;
mod ast;

#[derive(Parser)]
#[grammar = "ron.pest"]
struct RonParser;

fn main() {
    let args = Arguments::parse();

    TAB_SIZE.store(args.tab_size, Ordering::SeqCst);
    MAX_LINE_WIDTH.store(args.width, Ordering::SeqCst);

    let input = match (args.input, args.input_pos) {
        (Some(input), None) => input,
        (None, Some(input), ..) => input,
        (None, None) => "-".parse().unwrap(),
        (Some(_), Some(_)) => {
            eprintln!("Error: Cannot specify both a positional input and the -i/--input flag");
            std::process::exit(1);
        }
    };

    let file = input
        .clone()
        .contents()
        .expect("unable to read file or stdin");

    let ron = RonParser::parse(Rule::ron_file, &file)
        .expect("unable to parse RON")
        .next()
        .unwrap();

    if args.debug || input.is_stdin() {
        println!("{}", ast::RonFile::parse_from(ron));
    } else {
        let filename = input.filename();
        assert!(
            filename != "-",
            "unexpected filename, this should be unreachable"
        );
        if !args.no_backup {
            let mut backup = OsString::from(filename);
            backup.push(".bak");
            std::fs::copy(filename, &backup).expect("unable to create backup file");
        }
        std::fs::write(filename, format!("{}", ast::RonFile::parse_from(ron)))
            .expect("unable to overwrite target file");
    }
}
