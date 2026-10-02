//! toolo — a keyboard-driven SQLite database browser for the terminal.

mod app;
mod db;
mod model;
mod ui;

use std::io;

use crossterm::event::{read, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::{Backend, CrosstermBackend};
use ratatui::Terminal;

const DEFAULT_DB: &str = "/bench/data/bench.db";

fn print_usage() {
    println!("toolo — SQLite database browser TUI");
    println!();
    println!("USAGE:");
    println!("    toolo [DATABASE_PATH]");
    println!();
    println!("If DATABASE_PATH is omitted, {} is used.", DEFAULT_DB);
    println!();
    println!("OPTIONS:");
    println!("    -h, --help       show this help");
    println!("    -V, --version    show version");
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.iter().any(|a| a == "-h" || a == "--help") {
        print_usage();
        return Ok(());
    }
    if args.iter().any(|a| a == "-V" || a == "--version") {
        println!("toolo 0.1.0");
        return Ok(());
    }

    let db_path = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .cloned()
        .unwrap_or_else(|| DEFAULT_DB.to_string());

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    let result = run_app(&mut terminal, &db_path);

    let _ = terminal.show_cursor();
    let _ = disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);

    if let Err(e) = result {
        eprintln!("toolo error: {}", e);
    }
    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, db_path: &str) -> io::Result<()> {
    let mut app = app::App::new(db_path.to_string());
    app.init();

    loop {
        terminal.draw(|f| ui::draw(f, &mut app))?;
        if !app.running {
            break;
        }
        if let Event::Key(key) = read()? {
            if key.kind == KeyEventKind::Release {
                continue;
            }
            app.handle_key(key);
        }
    }
    Ok(())
}
