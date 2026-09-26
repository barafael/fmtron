use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// Formatting options left unset fall back to the nearest `fmt.ron` (or
/// `.fmt.ron`), searched from the input file's directory upward, and then to
/// the built-in defaults.
#[derive(Debug, Parser)]
#[command(author, version, long_about = Some("Utility for autoformatting RON files.\n\n\
    Settings come from, in increasing priority: built-in defaults, the nearest fmt.ron \
    (or .fmt.ron) in the input file's directory or any parent, and command-line flags."))]
pub struct Arguments {
    /// Sets which file to format
    #[arg(short, long, required_unless_present = "print_config")]
    pub input: Option<PathBuf>,

    /// Sets soft max line width for formatting heuristics [default: 100]
    #[arg(short)]
    pub width: Option<usize>,

    /// Sets indentation size in spaces [default: 4]
    #[arg(short)]
    pub tab_size: Option<usize>,

    /// Prints output to console instead of overwriting the input file
    #[arg(short, default_value_t = false)]
    pub debug: bool,

    /// Maximum container-nesting depth accepted before the input is rejected
    /// (guards against stack overflow on adversarial input) [default: 512]
    #[arg(long)]
    pub max_depth: Option<usize>,

    /// Upper bound enforced on the indentation size; a larger --tab-size is
    /// rejected instead of emitting pathologically wide indentation
    /// [default: 1024]
    #[arg(long)]
    pub max_tab: Option<usize>,

    /// Whether blank lines between elements are kept (runs collapse to one)
    /// or removed [default: keep]
    #[arg(long, value_enum)]
    pub blank_lines: Option<BlankLines>,

    /// Use this configuration file instead of searching for fmt.ron
    #[arg(long, value_name = "PATH", conflicts_with = "no_config")]
    pub config: Option<PathBuf>,

    /// Ignore any fmt.ron: use only flags and built-in defaults
    #[arg(long)]
    pub no_config: bool,

    /// Print the effective configuration as a fmt.ron and exit
    #[arg(long)]
    pub print_config: bool,
}

/// The `--blank-lines` choices, mirroring [`fmtron::BlankLines`].
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum BlankLines {
    Keep,
    Remove,
}

impl From<BlankLines> for fmtron::BlankLines {
    fn from(b: BlankLines) -> Self {
        match b {
            BlankLines::Keep => Self::Keep,
            BlankLines::Remove => Self::Remove,
        }
    }
}
