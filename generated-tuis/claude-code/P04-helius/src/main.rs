//! `toold` — a local-first personal finance ledger TUI.

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event};

use toold::app::App;
use toold::config::{self, Startup};
use toold::db::Store;
use toold::{input, ui};

fn main() -> io::Result<()> {
    match config::startup_from_args(std::env::args().skip(1)) {
        Startup::Help => {
            print!("{}", config::help_text());
            Ok(())
        }
        Startup::Version => {
            println!("{} {}", config::APP_NAME, env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        Startup::WhereIs(cfg) => {
            println!("{}", cfg.db_path.display());
            println!("(resolved from the {})", cfg.db_source.describe());
            Ok(())
        }
        Startup::Error(message) => {
            eprintln!("{}: {message}", config::APP_NAME);
            std::process::exit(2);
        }
        Startup::Run(cfg) => run(cfg),
    }
}

fn run(cfg: config::Config) -> io::Result<()> {
    // Open the ledger before touching the terminal, so a failure here prints a
    // plain error instead of being swallowed by the alternate screen.
    let store = match Store::open(&cfg.db_path) {
        Ok(store) => store,
        Err(e) => {
            eprintln!(
                "{}: cannot open the ledger at {}: {e}",
                config::APP_NAME,
                cfg.db_path.display()
            );
            eprintln!("Set TOOLD_DB or pass --db <PATH> to use a different location.");
            std::process::exit(1);
        }
    };

    let mut terminal = ratatui::init();
    let mut app = App::new(store, cfg);
    app.info(format!(
        "Welcome to toold — press ? for help. Ledger: {}",
        app.store.path().display()
    ));

    let result = event_loop(&mut terminal, &mut app);

    // Always restore the terminal, even if the loop failed.
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    while !app.should_quit {
        terminal.draw(|frame| ui::draw(frame, app))?;

        // A poll timeout keeps the loop responsive without spinning on an idle
        // screen.
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) => input::handle_key(app, key),
            // Resize needs no work here: the next iteration redraws.
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    Ok(())
}
