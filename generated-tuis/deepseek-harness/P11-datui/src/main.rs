//! toolk — CSV data analysis TUI.
//!
//! Usage: `toolk [path/to/file.csv]`
//! Defaults to `/bench/data/employees.csv`.

mod analysis;
mod app;
mod format;
mod input;
mod query;
mod report;
mod table;
mod ui;

use crossterm::event::{self, Event, KeyEventKind};
use ratatui::DefaultTerminal;

use app::App;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/bench/data/employees.csv".to_string());

    let mut app = match App::new(&path) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("toolk: failed to load {path}: {e}");
            std::process::exit(1);
        }
    };

    if let Err(e) = run(&mut app) {
        ratatui::restore();
        eprintln!("toolk: error: {e}");
        std::process::exit(1);
    }
}

fn run(app: &mut App) -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal: DefaultTerminal = ratatui::init();
    let res = run_loop(&mut terminal, app);
    ratatui::restore();
    res
}

fn run_loop(
    terminal: &mut DefaultTerminal,
    app: &mut App,
) -> Result<(), Box<dyn std::error::Error>> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        match event::read()? {
            Event::Key(key) => {
                if key.kind == KeyEventKind::Release {
                    continue;
                }
                if app.handle_key(key) {
                    break;
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(())
}
