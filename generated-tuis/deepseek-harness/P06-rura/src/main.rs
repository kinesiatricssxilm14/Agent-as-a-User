//! toolf — interactive shell pipeline debugging TUI.

use std::error::Error;
use std::io;
use std::path::PathBuf;

use crossterm::cursor::{Hide, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use toolf::{app, ui};

const DEFAULT_FILE: &str = "/bench/server.log";

fn print_cli_help() {
    println!("toolf — interactive shell pipeline debugging TUI");
    println!();
    println!("USAGE:");
    println!("    toolf [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    -f, --file <PATH>    Log file to analyze (default: {DEFAULT_FILE})");
    println!("    -h, --help           Print this help message");
    println!("    -V, --version        Print version information");
}

fn parse_args() -> Result<PathBuf, String> {
    let mut file = PathBuf::from(DEFAULT_FILE);
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_cli_help();
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("toolf {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "-f" | "--file" => match it.next() {
                Some(v) => file = PathBuf::from(v),
                None => return Err("--file requires a value".to_string()),
            },
            _ if arg.starts_with("--file=") => {
                file = PathBuf::from(&arg["--file=".len()..]);
            }
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    Ok(file)
}

fn main() -> Result<(), Box<dyn Error>> {
    let file = match parse_args() {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: {e}");
            eprintln!("try 'toolf --help' for more information");
            std::process::exit(2);
        }
    };
    run_tui(file)
}

fn run_tui(file: PathBuf) -> Result<(), Box<dyn Error>> {
    // Restore the terminal even if we panic.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
        hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;

    let result = (|| -> Result<(), Box<dyn Error>> {
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        let mut app = app::App::new(file);

        loop {
            terminal.draw(|f| ui::draw(f, &mut app))?;
            if app.should_quit {
                break;
            }
            if !event::poll(std::time::Duration::from_millis(100))? {
                continue;
            }
            let ev = event::read()?;
            let key = match ev {
                Event::Key(k) => k,
                _ => continue,
            };
            handle_key_with_alt_fallback(&mut app, key)?;
        }
        Ok(())
    })();

    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen, Show)?;
    result
}

/// Some terminals deliver `Alt+\` as an `Esc` followed by `\`. If a plain key
/// arrives right after `Esc`, treat it as `Alt+<key>` so the partial-execution
/// shortcut works everywhere.
fn handle_key_with_alt_fallback(app: &mut app::App, key: KeyEvent) -> Result<(), Box<dyn Error>> {
    if key.code == KeyCode::Esc && key.modifiers == KeyModifiers::NONE {
        if event::poll(std::time::Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(k2)
                    if matches!(k2.code, KeyCode::Char(_))
                        && k2.modifiers == KeyModifiers::NONE =>
                {
                    let c = match k2.code {
                        KeyCode::Char(c) => c,
                        _ => unreachable!(),
                    };
                    app.handle_key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT));
                }
                Event::Key(k2) => {
                    app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
                    app.handle_key(k2);
                }
                _ => app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            }
        } else {
            app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        }
    } else {
        app.handle_key(key);
    }
    Ok(())
}
