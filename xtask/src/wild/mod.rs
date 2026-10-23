//! The `wild` corpus pipeline: collect real-world RON files, evaluate fmtron
//! on them, and rebuild the redistributable corpus in `test_data/wild/`.

mod build_corpus;
mod codeberg_collect;
mod crates_collect;
mod evaluate;
mod forge_collect;
mod gh_collect;
mod gh_download;

pub fn run(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("gh-collect") => gh_collect::run(&args[1..]),
        Some("gh-download") => gh_download::run(),
        Some("crates-collect") => crates_collect::run(&args[1..]),
        Some("forge-collect") => forge_collect::run(&args[1..]),
        Some("codeberg-collect") => codeberg_collect::run(),
        Some("evaluate") => evaluate::run(),
        Some("build-corpus") => build_corpus::run(),
        Some("help" | "-h" | "--help") | None => {
            print!(
                "usage: cargo xtask wild <subcommand>\n\n\
                 subcommands: gh-collect, gh-download, crates-collect, forge-collect,\n\
                 codeberg-collect, evaluate, build-corpus\n"
            );
            Ok(())
        }
        Some(other) => Err(format!("unknown wild subcommand `{other}`")),
    }
}
