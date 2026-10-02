mod app;
mod data;

use std::{
    io::{self, IsTerminal},
    path::PathBuf,
};

use anyhow::Result;
use app::App;

fn main() -> Result<()> {
    let path = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/bench/data/employees.csv"));
    if !path.is_file() {
        return Err(app::missing_file_message(&path));
    }
    if !io::stdout().is_terminal() {
        anyhow::bail!("toolk requires an interactive terminal");
    }
    let mut app = App::new(path)?;
    let mut terminal = ratatui::init();
    let result = app.run(&mut terminal);
    ratatui::restore();
    result
}
