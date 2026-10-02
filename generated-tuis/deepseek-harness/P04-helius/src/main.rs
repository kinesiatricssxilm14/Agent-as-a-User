//! toold — a local-first personal finance ledger TUI.

mod app;
mod db;
mod form;
mod models;
mod ui;
mod util;

use std::error::Error;
use std::io::IsTerminal;
use std::path::PathBuf;

use crossterm::event::{self, Event, KeyEventKind};

use app::App;
use db::Db;

/// Determine the SQLite storage path.
///
/// Resolution order:
///   1. `TOOLD_DB` environment variable (explicit override)
///   2. `$XDG_DATA_HOME/toold/ledger.db`
///   3. `$HOME/.local/share/toold/ledger.db`
///   4. `./toold.db` (fallback)
fn resolve_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("TOOLD_DB") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    if let Ok(x) = std::env::var("XDG_DATA_HOME") {
        if !x.trim().is_empty() {
            return PathBuf::from(x).join("toold").join("ledger.db");
        }
    }
    if let Ok(h) = std::env::var("HOME") {
        if !h.trim().is_empty() {
            return PathBuf::from(h)
                .join(".local")
                .join("share")
                .join("toold")
                .join("ledger.db");
        }
    }
    PathBuf::from("toold.db")
}

fn main() -> Result<(), Box<dyn Error>> {
    let db_path = resolve_db_path();
    let db = Db::open(&db_path)?;
    let mut app = App::new(db, db_path.clone());
    app.reload();

    // Non-interactive (e.g. piped) invocation: still initialize the database
    // and report where it lives, then exit cleanly.
    if !std::io::stdout().is_terminal() {
        println!(
            "toold: initialized ledger database at {}",
            db_path.display()
        );
        println!("toold is an interactive TUI; run it inside a terminal.");
        return Ok(());
    }

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    result?;
    Ok(())
}

fn run(
    terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
    app: &mut App,
) -> std::io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;
        if !app.running {
            break;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => app.handle_key(key),
            Event::Resize(..) => {}
            _ => {}
        }
    }
    Ok(())
}
