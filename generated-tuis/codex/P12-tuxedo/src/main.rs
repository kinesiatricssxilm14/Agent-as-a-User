mod app;
mod model;
mod storage;
mod ui;

use anyhow::{bail, Context, Result};
use app::App;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::{
    env,
    io::{self, IsTerminal},
    path::PathBuf,
};

const DEFAULT_PATH: &str = "/bench/data/todo.txt";

fn main() -> Result<()> {
    let path = parse_args()?;
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        bail!("tooll requires an interactive terminal");
    }

    enable_raw_mode().context("failed to enable terminal raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = App::new(path).and_then(|mut app| app.run(&mut terminal));

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn parse_args() -> Result<PathBuf> {
    let mut args = env::args().skip(1);
    let mut path = env::var_os("TOOLL_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_PATH));

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-f" | "--file" => {
                let value = args.next().context("--file requires a path")?;
                path = PathBuf::from(value);
            }
            "-h" | "--help" => {
                println!(
                    "tooll {}\n\nKeyboard-first todo.txt manager\n\nUSAGE:\n    tooll [PATH]\n    tooll --file PATH\n\nENVIRONMENT:\n    TOOLL_FILE    Override the default {}\n",
                    env!("CARGO_PKG_VERSION"),
                    DEFAULT_PATH
                );
                std::process::exit(0);
            }
            "-V" | "--version" => {
                println!("tooll {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            value if value.starts_with('-') => bail!("unknown option: {value}"),
            value => path = PathBuf::from(value),
        }
    }
    Ok(path)
}
