//! tooli — disk space visualization TUI.
//!
//! Usage: `tooli [/path/to/scan]`
//! The first command-line argument is the scan root and defaults to `/bench/data`.

mod app;
mod model;
mod scan;
mod size;
mod treemap;
mod ui;

use std::io::{self, Stdout};
use std::path::PathBuf;

use crossterm::{
    event::{self, Event},
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::App;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/bench/data"));

    // Make the root absolute and resolve symlinks so that path comparisons
    // used after a rescan stay stable.
    let root = if root.is_absolute() {
        root
    } else {
        std::env::current_dir()?.join(root)
    };
    let root = std::fs::canonicalize(&root).unwrap_or(root);

    let mut app = App::new(root);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("tooli error: {}", e);
    }
    Ok(())
}

fn run(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;
        if app.quit {
            break;
        }
        if let Event::Key(key) = event::read()? {
            app.handle_key(key);
        }
        // Resize and other events simply trigger a redraw on the next loop.
    }
    Ok(())
}
