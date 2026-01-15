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

    let file = std::fs::read_to_string(&args.input).expect("unable to read file");

    let ron = RonParser::parse(Rule::ron_file, &file)
        .expect("unable to parse RON")
        .next()
        .unwrap();

    if args.debug {
        println!("{}", ast::RonFile::parse_from(ron));
    } else {
        let mut backup = OsString::from(&args.input);
        backup.push(".bak");
        std::fs::copy(&args.input, &backup).expect("unable to create backup file");

        std::fs::write(args.input, format!("{}", ast::RonFile::parse_from(ron)))
            .expect("unable to overwrite target file");
    }
}

#[test]
fn formats_unit() {
    let content = include_str!("../test_data/unit.ron");
    let ron = RonParser::parse(Rule::ron_file, content)
        .expect("unable to parse RON")
        .next()
        .unwrap();
    println!("{}", ast::RonFile::parse_from(ron));
}
