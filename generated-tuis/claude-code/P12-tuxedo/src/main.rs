//! Terminal setup, the event loop, and the non-interactive `--list` mode.
//!
//! Everything interesting lives in the library modules; this file only owns the
//! terminal. It is deliberately careful about restoring the terminal, including
//! on panic, because leaving a user in raw mode with a hidden cursor is the
//! worst thing a TUI can do.

use std::io::{self, IsTerminal, Write};
use std::panic;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use tooll::app::{App, Key};
use tooll::config::{self, Invocation, SystemEnv};
use tooll::store::TaskStore;
use tooll::ui;

fn main() {
    let code = match real_main() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("tooll: {e}");
            1
        }
    };
    std::process::exit(code);
}

fn real_main() -> Result<(), Box<dyn std::error::Error>> {
    let env = SystemEnv;
    let args: Vec<String> = std::env::args().skip(1).collect();

    match config::parse_args(args, &env)? {
        Invocation::Help => {
            print!("{}", config::HELP);
            io::stdout().flush()?;
            Ok(())
        }
        Invocation::Version => {
            println!("tooll {}", config::VERSION);
            Ok(())
        }
        Invocation::List(cfg) => {
            // A plain dump of the file, useful for scripting and for confirming
            // from outside the TUI that the disk matches what was on screen.
            let mut store = TaskStore::load(&cfg.file)
                .map_err(|e| format!("could not read {}: {e}", cfg.file.display()))?;
            if cfg.sort_on_load {
                store.sort_tasks();
            }
            let mut out = io::stdout().lock();
            for (i, task) in store.tasks().iter().enumerate() {
                if cfg.hide_done && task.completed {
                    continue;
                }
                writeln!(out, "{:>3}  {}", i + 1, task.render())?;
            }
            out.flush()?;
            Ok(())
        }
        Invocation::Run(cfg) => {
            if !io::stdout().is_terminal() {
                return Err(format!(
                    "stdout is not a terminal; use `tooll --list` for non-interactive output\n\
                     (task file would have been {})",
                    cfg.file.display()
                )
                .into());
            }
            let app = App::new(cfg.clone())
                .map_err(|e| format!("could not read {}: {e}", cfg.file.display()))?;
            run_tui(app)
        }
    }
}

/// Enter the alternate screen, run the loop, and always restore the terminal.
fn run_tui(mut app: App) -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;

    // A panic inside the loop must not leave the terminal unusable.
    let hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = restore_terminal();
        hook(info);
    }));

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.hide_cursor()?;

    let result = event_loop(&mut terminal, &mut app);

    restore_terminal()?;
    let _ = panic::take_hook();

    result.map_err(Into::into)
}

fn event_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // A poll rather than a blocking read so a resize repaints promptly.
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => {
                if let Some(k) = translate_key(key) {
                    app.on_key(k);
                }
            }
            Event::Resize(_, _) => { /* the next draw picks up the new size */ }
            _ => {}
        }
        if app.should_quit {
            return Ok(());
        }
    }
}

/// Map a crossterm key event onto the backend-independent [`Key`].
///
/// Returns `None` for events the application does not act on (key releases,
/// modifier-only presses, unsupported combinations).
fn translate_key(ev: KeyEvent) -> Option<Key> {
    // On terminals that report press *and* release (Windows, kitty protocol),
    // acting on both would double every keystroke.
    if ev.kind == KeyEventKind::Release {
        return None;
    }

    let ctrl = ev.modifiers.contains(KeyModifiers::CONTROL);
    let shift = ev.modifiers.contains(KeyModifiers::SHIFT);

    Some(match ev.code {
        KeyCode::Char(c) if ctrl => Key::CtrlChar(c.to_ascii_lowercase()),
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Delete => Key::Delete,
        KeyCode::Tab if shift => Key::BackTab,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Home => Key::Home,
        KeyCode::End => Key::End,
        KeyCode::PageUp => Key::PageUp,
        KeyCode::PageDown => Key::PageDown,
        KeyCode::F(n) => Key::F(n),
        _ => return None,
    })
}

/// Undo everything [`run_tui`] set up. Safe to call more than once.
fn restore_terminal() -> io::Result<()> {
    let mut stdout = io::stdout();
    execute!(stdout, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    execute!(stdout, crossterm::cursor::Show)?;
    stdout.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(code: KeyCode, mods: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, mods)
    }

    #[test]
    fn translates_plain_keys() {
        assert_eq!(
            translate_key(ev(KeyCode::Char('a'), KeyModifiers::NONE)),
            Some(Key::Char('a'))
        );
        assert_eq!(
            translate_key(ev(KeyCode::Enter, KeyModifiers::NONE)),
            Some(Key::Enter)
        );
        assert_eq!(
            translate_key(ev(KeyCode::F(1), KeyModifiers::NONE)),
            Some(Key::F(1))
        );
    }

    #[test]
    fn translates_control_and_shift() {
        assert_eq!(
            translate_key(ev(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Key::CtrlChar('c'))
        );
        // Terminals may report Ctrl-J as an uppercase char; normalise it.
        assert_eq!(
            translate_key(ev(KeyCode::Char('J'), KeyModifiers::CONTROL)),
            Some(Key::CtrlChar('j'))
        );
        assert_eq!(
            translate_key(ev(KeyCode::Tab, KeyModifiers::SHIFT)),
            Some(Key::BackTab)
        );
        assert_eq!(
            translate_key(ev(KeyCode::BackTab, KeyModifiers::SHIFT)),
            Some(Key::BackTab)
        );
        // Shift-A must stay a distinct character, not become `a`.
        assert_eq!(
            translate_key(ev(KeyCode::Char('A'), KeyModifiers::SHIFT)),
            Some(Key::Char('A'))
        );
    }

    #[test]
    fn ignores_releases_and_unknown_codes() {
        let mut e = ev(KeyCode::Char('a'), KeyModifiers::NONE);
        e.kind = KeyEventKind::Release;
        assert_eq!(translate_key(e), None);
        assert_eq!(translate_key(ev(KeyCode::Insert, KeyModifiers::NONE)), None);
    }
}
