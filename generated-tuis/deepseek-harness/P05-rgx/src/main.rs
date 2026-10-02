//! toole — interactive regular expression testing TUI.
//!
//! Entry point: argument parsing, terminal setup, and the main event loop.

mod app;
mod ui;

use crossterm::{
    cursor,
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, Stdout};

const DEFAULT_FILE: &str = "/bench/data/input.txt";

struct Args {
    file: String,
}

fn parse_args() -> Args {
    let mut file = DEFAULT_FILE.to_string();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "-f" | "--file" => {
                if i + 1 < args.len() {
                    i += 1;
                    file = args[i].clone();
                } else {
                    eprintln!("toole: option '{}' requires a file path argument", args[i]);
                    eprintln!("Try 'toole --help' for more information.");
                    std::process::exit(2);
                }
            }
            "-h" | "--help" => {
                print_help();
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("toole {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            other => {
                eprintln!("toole: unrecognized argument '{}'", other);
                eprintln!("Try 'toole --help' for more information.");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    Args { file }
}

fn print_help() {
    println!(
        "toole {} — interactive regular expression testing TUI\n\n\
         USAGE:\n    toole [-f <file>]\n\n\
         OPTIONS:\n\
         \x20   -f, --file <file>    Input text file to load (default: /bench/data/input.txt)\n\
         \x20   -h, --help           Print this help text and exit\n\
         \x20   -V, --version        Print version information and exit\n\n\
         KEYBOARD (inside the TUI):\n\
         \x20   Tab / Shift+Tab      Cycle focus between pattern, replacement, matches, preview\n\
         \x20   /                    Focus the regex pattern input\n\
         \x20   r                    Focus the replacement input\n\
         \x20   i                    Toggle case-insensitive matching (adds (?i))\n\
         \x20   F2                   Open the regex preset menu (incl. China phone numbers)\n\
         \x20   Ctrl+R               Reload the input file from disk\n\
         \x20   F1 / ? / h           Show the full help overlay\n\
         \x20   q / Ctrl+C           Quit\n\
         \x20   ↑ ↓ / PgUp / PgDn    Scroll or move selection\n",
        env!("CARGO_PKG_VERSION")
    );
}

fn main() -> io::Result<()> {
    let args = parse_args();
    let mut app = app::App::new(args.file);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, cursor::Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let res = run(&mut terminal, &mut app);

    // Restore the terminal regardless of how the loop ended.
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, cursor::Show)?;
    res
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut app::App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;
        if app.quit {
            break;
        }
        match event::read()? {
            Event::Key(key) => app.handle_key(key),
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(())
}
