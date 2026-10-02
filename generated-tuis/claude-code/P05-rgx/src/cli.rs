//! Command line interface definition for `toole`.

use std::path::PathBuf;

use clap::Parser;

/// Default input file, as mandated by the deployment environment.
pub const DEFAULT_INPUT: &str = "/bench/data/input.txt";

#[derive(Debug, Parser)]
#[command(
    name = "toole",
    version,
    about = "Interactive regular expression testing TUI (live matching, highlighting, offsets, replacement preview)",
    long_about = "toole loads a text file and lets you develop a regular expression against it \
interactively.  Matches are highlighted with a background colour, every match is listed with its \
0-indexed start/end character offsets, and a replacement template can be previewed against the \
complete file content.\n\nRun `toole -f FILE` and press F1 inside the program for the key map."
)]
pub struct Cli {
    /// Input text file to load.
    #[arg(short = 'f', long = "file", value_name = "PATH", default_value = DEFAULT_INPUT)]
    pub file: PathBuf,

    /// Start with this regular expression already entered.
    ///
    /// Hyphen-leading values are allowed, so patterns like `-?\d+` work without
    /// a `--` separator.
    #[arg(
        short = 'e',
        long = "pattern",
        value_name = "REGEX",
        allow_hyphen_values = true
    )]
    pub pattern: Option<String>,

    /// Start with this replacement template (enables the replacement preview).
    #[arg(
        short = 'r',
        long = "replace",
        value_name = "TEMPLATE",
        allow_hyphen_values = true
    )]
    pub replace: Option<String>,

    /// Case-insensitive matching (same as the (?i) inline flag).
    #[arg(short = 'i', long = "ignore-case")]
    pub ignore_case: bool,

    /// Multi-line mode: ^ and $ match at line boundaries (inline flag (?m)).
    #[arg(short = 'M', long = "multi-line")]
    pub multi_line: bool,

    /// Dot matches newline (inline flag (?s)).
    #[arg(short = 's', long = "dot-all")]
    pub dot_all: bool,

    /// Extended mode: whitespace in the pattern is ignored (inline flag (?x)).
    #[arg(short = 'x', long = "extended")]
    pub extended: bool,

    /// Treat the pattern as a literal string instead of a regex.
    #[arg(short = 'F', long = "literal")]
    pub literal: bool,

    /// Force the backtracking engine (look-around, back-references).
    #[arg(long = "fancy")]
    pub fancy: bool,

    /// Replace only the first match instead of every match.
    #[arg(long = "first-only")]
    pub first_only: bool,

    /// Do not start the TUI: print the match report (and replacement) to stdout.
    #[arg(short = 'p', long = "print")]
    pub print: bool,

    /// List the built-in pattern presets and exit.
    #[arg(long = "list-presets")]
    pub list_presets: bool,

    /// Number of columns a tab character occupies on screen.
    #[arg(long = "tab-width", value_name = "N", default_value_t = 4, value_parser = clap::value_parser!(u16).range(1..=16))]
    pub tab_width: u16,
}

impl Cli {
    /// Collect the inline-flag toggles into the engine's flag set.
    pub fn flags(&self) -> crate::engine::Flags {
        crate::engine::Flags {
            ignore_case: self.ignore_case,
            multi_line: self.multi_line,
            dot_all: self.dot_all,
            extended: self.extended,
            literal: self.literal,
        }
    }
}
