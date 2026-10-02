//! toolb — Kanban board management TUI.

mod app;
mod board;
mod config;
mod ui;

use std::path::PathBuf;

use clap::Parser;
use crossterm::{
    event::{self, Event},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};

use app::App;

#[derive(Parser, Debug)]
#[command(
    name = "toolb",
    version,
    about = "Kanban board management TUI",
    long_about = "Interactive kanban board for managing task cards stored as plain files on disk."
)]
struct Args {
    /// Board root directory (overrides the configured default for this run)
    #[arg(short, long, value_name = "DIR")]
    board: Option<PathBuf>,

    /// Configuration file to use (default: $XDG_CONFIG_HOME/toolb/config.toml)
    #[arg(long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Persist DIR as the default board root in the config file and exit
    #[arg(long, value_name = "DIR")]
    set_board_root: Option<PathBuf>,

    /// Print the effective board root and exit
    #[arg(long)]
    show_board_root: bool,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("toolb: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args = Args::parse();
    let cfg_path: Option<PathBuf> = args.config.clone();

    if let Some(dir) = args.set_board_root {
        let mut cfg = config::load_config(cfg_path.as_deref());
        cfg.board_root = dir;
        config::save_config(cfg_path.as_deref(), &cfg)?;
        println!("board_root = {}", cfg.board_root.display());
        return Ok(());
    }

    if args.show_board_root {
        let cfg = config::load_config(cfg_path.as_deref());
        let root = args.board.clone().unwrap_or(cfg.board_root);
        println!("{}", root.display());
        return Ok(());
    }

    let cfg = config::load_config(cfg_path.as_deref());
    let root = args.board.clone().unwrap_or(cfg.board_root);
    run_app(root)
}

fn run_app(root: PathBuf) -> Result<(), String> {
    let mut app = App::new(root)?;

    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(|e| e.to_string())?;
    terminal::enable_raw_mode().map_err(|e| e.to_string())?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).map_err(|e| e.to_string())?;

    let result: Result<(), String> = (|| {
        loop {
            terminal
                .draw(|f| ui::draw(f, &mut app))
                .map_err(|e| e.to_string())?;
            if app.quit {
                break;
            }
            match event::read().map_err(|e| e.to_string())? {
                Event::Key(key) => app.on_key(key),
                _ => {}
            }
        }
        Ok(())
    })();

    let _ = terminal::disable_raw_mode();
    let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
    let _ = terminal.show_cursor();

    result
}
