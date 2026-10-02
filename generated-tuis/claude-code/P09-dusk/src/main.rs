//! tooli — a disk space visualiser for the terminal.
//!
//! Usage: `tooli [DIRECTORY]` (defaults to `/bench/data`).

mod app;
mod format;
mod scan;
mod treemap;
mod ui;

use std::io::{self, Write};
use std::panic;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use app::{App, Level, Pane, Prompt};

const DEFAULT_ROOT: &str = "/bench/data";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("tooli: {}", err);
            ExitCode::FAILURE
        }
    }
}

fn run() -> io::Result<()> {
    let root = match parse_args()? {
        Some(root) => root,
        None => return Ok(()), // --help / --version already printed
    };

    let root = root.canonicalize().unwrap_or(root);
    if !root.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("{} does not exist", root.display()),
        ));
    }
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{} is not a directory", root.display()),
        ));
    }

    // Scanning happens before the alternate screen so a slow first scan is
    // visible as plain output instead of a frozen blank terminal.
    eprintln!("tooli: scanning {} ...", root.display());
    let mut app = App::new(root)?;

    let mut terminal = setup_terminal()?;
    let result = event_loop(&mut terminal, &mut app);
    restore_terminal()?;
    result
}

fn parse_args() -> io::Result<Option<PathBuf>> {
    let mut root: Option<PathBuf> = None;
    for arg in std::env::args().skip(1) {
        match arg.as_str() {
            "-h" | "--help" => {
                print_help();
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("tooli {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            other if other.len() > 1 && other.starts_with('-') => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown option `{}` (try --help)", other),
                ));
            }
            other => {
                if root.is_some() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!("unexpected extra argument `{}`", other),
                    ));
                }
                root = Some(PathBuf::from(other));
            }
        }
    }
    Ok(Some(root.unwrap_or_else(|| PathBuf::from(DEFAULT_ROOT))))
}

fn print_help() {
    println!(
        "tooli {} — disk space visualiser for the terminal

USAGE:
    tooli [DIRECTORY]

ARGS:
    DIRECTORY    Directory to scan (default: {})

OPTIONS:
    -h, --help       Print this help
    -V, --version    Print the version

The interface documents every key in its bottom bar; press ? inside tooli for
the full reference.",
        env!("CARGO_PKG_VERSION"),
        DEFAULT_ROOT
    );
}

type Term = Terminal<CrosstermBackend<io::Stdout>>;

fn setup_terminal() -> io::Result<Term> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    // Leave the terminal usable if we panic mid-frame.
    let hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        hook(info);
    }));

    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal() -> io::Result<()> {
    let mut stdout = io::stdout();
    let _ = execute!(stdout, DisableMouseCapture, LeaveAlternateScreen);
    let _ = disable_raw_mode();
    stdout.flush()
}

fn event_loop(terminal: &mut Term, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // Poll so a resize repaints promptly without busy-waiting.
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => handle_key(app, key),
            _ => {}
        }
        if app.should_quit {
            return Ok(());
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent) {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') => {
                app.should_quit = true;
                return;
            }
            KeyCode::Char('d') => {
                app.move_selection(10);
                return;
            }
            KeyCode::Char('u') => {
                app.move_selection(-10);
                return;
            }
            _ => {}
        }
    }

    // While a prompt is open it owns the keyboard, so typed text is never
    // mistaken for a command.
    if app.prompt.is_active() {
        handle_prompt_key(app, key);
        return;
    }

    match key.code {
        KeyCode::Char('q') | KeyCode::Char('Q') => app.should_quit = true,
        KeyCode::Char('?') | KeyCode::F(1) => app.toggle_help(),

        KeyCode::Up | KeyCode::Char('k') => app.move_selection(-1),
        KeyCode::Down | KeyCode::Char('j') => app.move_selection(1),
        KeyCode::Right | KeyCode::Char('l') => match app.focus {
            Pane::Map => app.move_map_spatial(1, 0),
            _ => app.expand_selected(),
        },
        KeyCode::Left | KeyCode::Char('h') => match app.focus {
            Pane::Map => app.move_map_spatial(-1, 0),
            _ => app.collapse_selected(),
        },
        KeyCode::Enter => app.enter_selected(),
        KeyCode::Char(' ') => app.toggle_expand(),
        KeyCode::Esc | KeyCode::Char('u') | KeyCode::Backspace => app.leave_directory(),
        KeyCode::Char('R') => app.goto_root(),

        KeyCode::Tab => {
            app.focus = app.focus.next();
            let title = app.focus.title();
            app.set_status(Level::Info, format!("Focus: {}", title));
        }
        KeyCode::BackTab => {
            app.focus = app.focus.prev();
            let title = app.focus.title();
            app.set_status(Level::Info, format!("Focus: {}", title));
        }
        KeyCode::Char('1') => focus(app, Pane::Tree),
        KeyCode::Char('2') => focus(app, Pane::Top),
        KeyCode::Char('3') => focus(app, Pane::Map),

        KeyCode::Home | KeyCode::Char('g') => app.select_first(),
        KeyCode::End | KeyCode::Char('G') => app.select_last(),
        KeyCode::PageDown => app.move_selection(page(app)),
        KeyCode::PageUp => app.move_selection(-page(app)),

        KeyCode::Char('s') => app.cycle_sort(),
        KeyCode::Char('f') => app.begin_min_size(),
        KeyCode::Char('/') => app.begin_search(),
        KeyCode::Char('c') => app.clear_filters(),
        KeyCode::Char('n') => app.begin_top_n(),
        KeyCode::Char('t') => app.toggle_top_scope(),
        KeyCode::Char('+') | KeyCode::Char('=') => app.bump_top_n(1),
        KeyCode::Char('-') | KeyCode::Char('_') => app.bump_top_n(-1),
        KeyCode::Char('e') => app.expand_all(),
        KeyCode::Char('E') => app.collapse_all(),

        KeyCode::Char('d') | KeyCode::Delete => app.begin_delete(),
        KeyCode::Char('r') | KeyCode::F(5) => app.refresh(),

        _ => {}
    }
}

fn focus(app: &mut App, pane: Pane) {
    app.focus = pane;
    app.set_status(Level::Info, format!("Focus: {}", pane.title()));
}

fn page(app: &App) -> isize {
    let rows = match app.focus {
        Pane::Top => app.top_rows_visible,
        _ => app.tree_rows_visible,
    };
    rows.max(1) as isize
}

fn handle_prompt_key(app: &mut App, key: KeyEvent) {
    let confirming = matches!(app.prompt, Prompt::ConfirmDelete { .. });
    match key.code {
        KeyCode::Esc => app.cancel_prompt(),
        KeyCode::Enter if confirming => {
            // Enter is deliberately inert on a delete confirmation: destructive
            // actions require the explicit y key.
            app.set_status(
                Level::Warn,
                "Press y to confirm the delete, or n / Esc to cancel",
            );
        }
        KeyCode::Enter => app.prompt_submit(),
        KeyCode::Backspace if !confirming => app.prompt_pop(),
        KeyCode::Char(c) if confirming => match c {
            'y' | 'Y' => app.prompt_submit(),
            'n' | 'N' | 'q' => app.cancel_prompt(),
            _ => {}
        },
        KeyCode::Char(c) => app.prompt_push(c),
        _ => {}
    }
}
