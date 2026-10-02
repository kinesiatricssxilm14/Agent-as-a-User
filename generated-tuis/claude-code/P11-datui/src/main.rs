//! toolk — an interactive CSV data analysis TUI.
//!
//! Usage: `toolk [PATH]`, defaulting to `/bench/data/employees.csv`.
//!
//! All data operations go through Polars against the real file on disk; nothing
//! about the interface is simulated.

mod app;
mod data;
mod editor;
mod fmtnum;
mod sqllike;
mod ui;

use std::io;
use std::panic;
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event};

use app::App;
use data::Dataset;

/// Data file used when no path is given on the command line.
const DEFAULT_CSV: &str = "/bench/data/employees.csv";

const HELP: &str = "\
toolk — CSV data analysis TUI (Fuzzy / SQL-Like / SQL query modes)

USAGE:
    toolk [OPTIONS] [CSV_PATH]

ARGS:
    CSV_PATH    CSV file to load. Defaults to /bench/data/employees.csv

OPTIONS:
    -h, --help       Show this message and exit
    -V, --version    Show the version and exit

KEYS (also on the in-app help page, opened with ? or F1):
    Tab                   move between the query line and the results table
    F2 / F3 / F4          Fuzzy / SQL-Like / SQL query mode
    Enter                 run the query
    up/down, left/right   move between rows and columns
    s / S / R             sort by column / add a sort key / clear sorting
    a                     analysis panel (statistics, distribution, correlation)
    e / o / r             export result / open a file / reload from disk
    x / F                 cycle table and analysis number formats
    ? or F1               help
    q or Ctrl-c           quit

QUERY EXAMPLES:
    Fuzzy       Sales
    SQL-Like    select where age > 40
    SQL-Like    select where department = 'Engineering' and salary > 10000
    SQL         select * from df where country = 'US' and score > 85
";

fn main() -> io::Result<()> {
    let mut path: Option<PathBuf> = None;

    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print!("{HELP}");
                return Ok(());
            }
            "-V" | "--version" => {
                println!("toolk {}", env!("CARGO_PKG_VERSION"));
                return Ok(());
            }
            // A lone "-" is a valid (if odd) file name; anything else starting
            // with a dash is a typo worth reporting.
            other if other.starts_with('-') && other.len() > 1 => {
                eprintln!("toolk: unknown option `{other}`\nTry `toolk --help`.");
                std::process::exit(2);
            }
            other => {
                if path.is_none() {
                    path = Some(PathBuf::from(other));
                } else {
                    eprintln!("toolk: unexpected extra argument `{other}`\nTry `toolk --help`.");
                    std::process::exit(2);
                }
            }
        }
    }

    let path = path.unwrap_or_else(|| PathBuf::from(DEFAULT_CSV));

    // Load before touching the terminal, so a bad path prints a plain error
    // instead of flashing the alternate screen.
    let ds = match Dataset::load(&path) {
        Ok(ds) => ds,
        Err(e) => {
            eprintln!("toolk: {e}");
            if path == PathBuf::from(DEFAULT_CSV) {
                eprintln!("Pass a CSV path explicitly, e.g. `toolk ./employees.csv`.");
            }
            std::process::exit(1);
        }
    };

    let mut app = App::new(ds);

    // Restore the terminal even if a draw panics; otherwise the user is left
    // with a broken shell and no cursor.
    let hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        ratatui::restore();
        hook(info);
    }));

    // toolk is interactive, so report a missing terminal as a plain message
    // rather than letting the backend panic with a backtrace.
    let mut terminal = match ratatui::try_init() {
        Ok(t) => t,
        Err(e) => {
            ratatui::restore();
            eprintln!("toolk: cannot start the interactive interface: {e}");
            eprintln!("Run toolk in a terminal (it needs a TTY for keyboard input).");
            std::process::exit(1);
        }
    };
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // The poll timeout keeps the loop responsive to resizes without
        // busy-waiting on input.
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        if let Event::Key(key) = event::read()? {
            app.on_key(key);
        }

        if app.should_quit {
            return Ok(());
        }
    }
}
