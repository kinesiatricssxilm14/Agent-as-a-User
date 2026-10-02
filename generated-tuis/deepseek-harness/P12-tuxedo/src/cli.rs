//! Command-line argument parsing.

use std::env;
use std::path::PathBuf;

pub const DEFAULT_PATH: &str = "/bench/data/todo.txt";

#[derive(Debug, Clone)]
pub struct Options {
    pub path: PathBuf,
}

#[derive(Debug)]
pub enum Action {
    Run(Options),
    Help,
    Version,
}

pub fn parse_args() -> Action {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut path: Option<PathBuf> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "-h" | "--help" => return Action::Help,
            "-V" | "--version" => return Action::Version,
            "-f" | "--file" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("tooll: --file requires a path argument");
                    return Action::Help;
                }
                path = Some(PathBuf::from(&args[i]));
            }
            s if s.starts_with("--file=") => {
                path = Some(PathBuf::from(&s["--file=".len()..]));
            }
            s if s.starts_with('-') => {
                eprintln!("tooll: unknown option: {s}");
                return Action::Help;
            }
            s => {
                path = Some(PathBuf::from(s));
            }
        }
        i += 1;
    }

    let path = path
        .or_else(|| env::var("TOOLL_FILE").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(DEFAULT_PATH));

    Action::Run(Options { path })
}

pub fn print_help() {
    println!(
        r#"tooll — a task management TUI for todo.txt files

USAGE:
    tooll [FILE]
    tooll --file FILE
    tooll -f FILE

ARGUMENTS:
    FILE             Path to the todo.txt file (default: /bench/data/todo.txt)

OPTIONS:
    -f, --file FILE  Use FILE as the task list
    -h, --help       Print this help and exit
    -V, --version    Print version and exit

ENVIRONMENT:
    TOOLL_FILE       Default task file (overridden by CLI arguments)

Inside the TUI press '?' for the full key binding reference."#
    );
}
