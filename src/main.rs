use std::ffi::OsString;

use clap::Parser as ClapParser;
use pest::Parser;
use pest_derive::Parser;

use arguments::Arguments;
use fmtron::Config;

mod arguments;
mod ast;

#[derive(Parser)]
#[grammar = "ron.pest"]
struct RonParser;

fn main() {
    let args = Arguments::parse();
    let config = Config {
        tab_size: args.tab_size,
        max_width: args.width,
    };

    let file = std::fs::read_to_string(&args.input).expect("unable to read file");

    let ron = RonParser::parse(Rule::ron_file, &file)
        .expect("unable to parse RON")
        .next()
        .unwrap();

    if args.debug {
        println!("{}", ast::RonFile::parse_from(ron, &file, config));
    } else {
        let mut backup = OsString::from(&args.input);
        backup.push(".bak");
        std::fs::copy(&args.input, &backup).expect("unable to create backup file");

        std::fs::write(
            args.input,
            format!("{}", ast::RonFile::parse_from(ron, &file, config)),
        )
        .expect("unable to overwrite target file");
    }
}
