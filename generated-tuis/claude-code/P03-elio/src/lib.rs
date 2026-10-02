//! toolc — a dual-pane file manager TUI.
//!
//! Left pane: the directory listing. Right pane: metadata plus the full content
//! of the selected file. Copy, move, rename, mkdir and delete all act on the
//! real filesystem through `std::fs`, and the listing is re-read afterwards, so
//! what the panes show is always the actual on-disk state.
//!
//! The crate is split into a library and a thin binary so the rendering and
//! filesystem layers can be exercised directly by tests and by the `snapshot`
//! example.

pub mod app;
pub mod cli;
pub mod fs_ops;
pub mod keys;
pub mod listing;
pub mod preview;
pub mod ui;

use std::io::{self, Stdout};
use std::panic;
use std::time::Duration;

use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::Terminal;

use app::App;

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Build the app, take over the terminal, and run until the user quits.
pub fn run(
    dir: std::path::PathBuf,
    preselect: Option<String>,
    show_hidden: bool,
) -> io::Result<()> {
    let mut app = App::new(dir, show_hidden);
    if let Some(name) = preselect {
        if let Some(index) = app.listing.index_of_name(&name) {
            app.selected = index;
            app.refresh_preview();
        }
    }

    let mut terminal = setup_terminal()?;
    let result = event_loop(&mut terminal, &mut app);
    // Restore the terminal even if the loop failed, so the shell stays usable.
    restore_terminal(&mut terminal)?;
    result
}

fn setup_terminal() -> io::Result<Tui> {
    // A panic in raw mode would leave the terminal unusable; hook in a restore.
    install_panic_hook();
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout))?;
    terminal.hide_cursor()?;
    terminal.clear()?;
    Ok(terminal)
}

fn restore_terminal(terminal: &mut Tui) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn install_panic_hook() {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        previous(info);
    }));
}

fn event_loop(terminal: &mut Tui, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|frame| ui::draw(frame, app))?;

        // A poll timeout keeps the loop responsive to resizes without busy-waiting.
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => keys::handle_key(app, key),
            // Redraw on resize; the layout is recomputed from scratch each frame.
            Event::Resize(_, _) => {}
            _ => {}
        }
        if app.should_quit {
            return Ok(());
        }
    }
}
