mod app;
mod model;

use anyhow::{Context, Result};
use app::{Action, App};
use clap::Parser;
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(name = "toolb", version, about = "Filesystem-backed Kanban board TUI")]
struct Args {
    /// Board root containing board.txt and cols/
    #[arg(
        short = 'b',
        long = "board",
        value_name = "PATH",
        default_value = "/bench/data/board",
        env = "TOOLB_BOARD_ROOT"
    )]
    board: PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        anyhow::bail!("toolb requires an interactive terminal");
    }
    let board = model::Board::open(&args.board)
        .with_context(|| format!("open board at {}", args.board.display()))?;
    let mut terminal = TerminalGuard::enter()?;
    let mut app = App::new(board);
    let result = run(&mut terminal.terminal, &mut app);
    drop(terminal);
    result
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| app.draw(frame))?;
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == crossterm::event::KeyEventKind::Press
                    && app.handle_key(key) == Action::Quit
                {
                    return Ok(());
                }
            }
        }
    }
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
}

impl TerminalGuard {
    fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(stdout, EnterAlternateScreen) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(mut terminal) => {
                terminal.clear()?;
                Ok(Self { terminal })
            }
            Err(error) => {
                let _ = disable_raw_mode();
                let _ = execute!(io::stdout(), LeaveAlternateScreen);
                Err(error.into())
            }
        }
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(self.terminal.backend_mut(), LeaveAlternateScreen);
        let _ = self.terminal.show_cursor();
    }
}
