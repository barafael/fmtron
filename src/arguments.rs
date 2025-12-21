use clap::Parser;
use clap_stdin::FileOrStdin;

#[derive(Debug, Parser)]
#[command(author, version, long_about = Some("Utility for autoformatting RON files."))]
pub struct Arguments {
    /// Sets which file to format, if not specified will read from stdin
    #[arg(short, long)]
    pub input: Option<FileOrStdin<String>>,

    /// Positional input file argument.
    pub input_pos: Option<FileOrStdin<String>>,

    /// Sets soft max line width for formatting heuristics
    #[arg(short, default_value_t = 40)]
    pub width: usize,

    /// Sets indentation size in spaces
    #[arg(short, default_value_t = 4)]
    pub tab_size: usize,

    /// Prints output to console instead of overwriting the input file
    #[arg(short, default_value_t = false)]
    pub debug: bool,

    /// Prevents making a backup 'filename.bak' file
    #[arg(long, default_value_t = false)]
    pub no_backup: bool,
}
