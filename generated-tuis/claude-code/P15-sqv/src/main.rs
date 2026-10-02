//! toolo — an interactive SQLite database browser for the terminal.
//!
//! `main` only owns the terminal lifecycle and the event loop. Application
//! state lives in [`app`]; drawing lives in [`ui`].

mod app;
mod db;
mod filter;
mod grid;
mod input;
mod keymap;
mod ui;
mod value;

use std::io::{self, stdout};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::App;
use crate::db::Database;

const DEFAULT_DB: &str = "/bench/data/bench.db";

fn print_usage() {
    println!("toolo — SQLite database browser TUI");
    println!();
    println!("USAGE:");
    println!("    toolo [DATABASE_PATH]");
    println!();
    println!("If DATABASE_PATH is omitted, {DEFAULT_DB} is used.");
    println!();
    println!("OPTIONS:");
    println!("    -h, --help       show this help");
    println!("    -V, --version    show version");
}

fn db_path_from_args() -> Result<Option<PathBuf>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage();
        return Ok(None);
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("toolo 0.1.0");
        return Ok(None);
    }
    let path = args
        .into_iter()
        .find(|a| !a.starts_with('-'))
        .unwrap_or_else(|| DEFAULT_DB.to_string());
    Ok(Some(PathBuf::from(path)))
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(e) => {
            eprintln!("toolo: {e:#}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<()> {
    let Some(path) = db_path_from_args()? else {
        return Ok(());
    };
    let db = Database::open(&path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut app = App::new(db)?;

    enable_raw_mode().context("enable raw mode")?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen).context("enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("create terminal")?;
    terminal.hide_cursor().ok();

    let result = event_loop(&mut terminal, &mut app);

    let _ = terminal.show_cursor();
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);

    result
}

fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;
        if app.should_quit {
            break;
        }
        if !event::poll(Duration::from_millis(200))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => app.on_key(key),
            Event::Resize(_, _) => {}
            _ => {}
        }
        if app.should_quit {
            break;
        }
    }
    Ok(())
}
