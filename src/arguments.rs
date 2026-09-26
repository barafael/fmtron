use clap::{Parser, ValueEnum};
use std::path::PathBuf;

/// Formatting options left unset fall back to the nearest `fmt.ron` (or
/// `.fmt.ron`), searched from each file's directory upward, and then to the
/// built-in defaults.
#[derive(Debug, Parser)]
#[command(author, version, long_about = Some("Utility for autoformatting RON files.\n\n\
    Formats the given files in place; directories are searched for *.ron files, respecting \
    .gitignore. With no paths, or `-`, reads stdin and writes stdout.\n\n\
    Settings come from, in increasing priority: built-in defaults, the nearest fmt.ron \
    (or .fmt.ron) in the file's directory or any parent, and command-line flags."))]
pub struct Arguments {
    /// Files or directories to format; `-` (or no paths) reads stdin
    pub paths: Vec<PathBuf>,

    /// Write nothing; print a diff for each file that would change and exit
    /// with status 1 if any would
    #[arg(long, conflicts_with_all = ["stdout", "backup"])]
    pub check: bool,

    /// Print the formatted output instead of writing it (one input only)
    #[arg(short = 'd', long = "stdout")]
    pub stdout: bool,

    /// Copy each changed file to <file>.bak before replacing it
    #[arg(long)]
    pub backup: bool,

    /// When reading stdin: the path it stands for, used to find fmt.ron and
    /// in messages
    #[arg(long, value_name = "PATH")]
    pub stdin_filepath: Option<PathBuf>,

    /// Sets soft max line width for formatting heuristics [default: 100]
    #[arg(short)]
    pub width: Option<usize>,

    /// Sets indentation size in spaces [default: 4]
    #[arg(short)]
    pub tab_size: Option<usize>,

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

    /// Print the effective configuration (for the first path, or the
    /// current directory) as a fmt.ron and exit
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
