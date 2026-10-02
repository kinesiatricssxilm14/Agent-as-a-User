mod app;
mod cli;
mod input;
mod store;
mod task;
mod ui;

use std::io::IsTerminal;

use anyhow::Result;

fn main() {
    if let Err(e) = real_main() {
        eprintln!("tooll: error: {e:#}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    match cli::parse_args() {
        cli::Action::Help => {
            cli::print_help();
            Ok(())
        }
        cli::Action::Version => {
            println!("tooll {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        cli::Action::Run(opts) => {
            if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
                anyhow::bail!("tooll is an interactive TUI and requires a terminal");
            }

            let store = store::Store::new(opts.path.clone());
            let tasks = store.load()?;
            let mut app = app::App::new(store, tasks);
            run_tui(&mut app)
        }
    }
}

fn run_tui(app: &mut app::App) -> Result<()> {
    let mut terminal = ratatui::try_init()?;

    let result = (|| -> Result<()> {
        loop {
            terminal.draw(|frame| ui::render(frame, app))?;
            if app.should_quit() {
                break;
            }
            match crossterm::event::read()? {
                crossterm::event::Event::Key(key) => app.handle_key(key),
                _ => {}
            }
        }
        Ok(())
    })();

    ratatui::restore();
    result
}
