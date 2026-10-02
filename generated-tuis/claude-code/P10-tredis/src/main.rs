//! toolj — a Redis database management TUI.
//!
//! Entry point: terminal setup/teardown, the event loop, and CLI argument
//! handling. Everything the tool displays comes from live Redis commands.

mod actions;
mod app;
mod config;
mod db;
mod help;
mod input;
mod sub;
mod ui;
mod util;

use std::io::{self, Stdout};
use std::panic;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::{App, Level};
use crate::config::{normalize_uri, Config, DEFAULT_URI};
use crate::db::Db;

const USAGE: &str = "\
toolj — Redis database management TUI

USAGE:
    toolj [OPTIONS]

OPTIONS:
    -u, --uri <URI>     Redis URI to connect to (default: redis://localhost:6379/0,
                        or $REDIS_URL when set)
    -n, --name <NAME>   Name to show for the connection (default: derived from the URI)
    -v, --version       Print the version and exit
    -h, --help          Print this help and exit

With no arguments toolj connects to redis://localhost:6379/0 and opens the
:keys view. Press ? inside the tool for the full list of key bindings.
";

/// How long to wait for a key press before redrawing. A short tick keeps
/// Pub/Sub messages flowing into the view without any user input.
const TICK: Duration = Duration::from_millis(200);
/// Periodic reload interval for the Pub/Sub view (channel lists change server
/// side without any local action).
const PUBSUB_POLL: Duration = Duration::from_secs(3);

struct Args {
    uri: String,
    name: Option<String>,
}

fn parse_args() -> Result<Option<Args>> {
    let mut uri = std::env::var("REDIS_URL")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_URI.to_string());
    let mut name: Option<String> = None;
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "-h" | "--help" => {
                print!("{USAGE}");
                return Ok(None);
            }
            "-v" | "--version" => {
                println!("toolj {}", env!("CARGO_PKG_VERSION"));
                return Ok(None);
            }
            "-u" | "--uri" | "--url" => {
                uri = it.next().context("--uri needs a value")?;
            }
            "-n" | "--name" => {
                name = Some(it.next().context("--name needs a value")?);
            }
            other if other.starts_with("redis://") || other.starts_with("rediss://") => {
                uri = other.to_string();
            }
            other => {
                anyhow::bail!("unknown argument `{other}` (try --help)");
            }
        }
    }
    Ok(Some(Args {
        uri: normalize_uri(&uri),
        name,
    }))
}

fn main() {
    let args = match parse_args() {
        Ok(Some(a)) => a,
        Ok(None) => return,
        Err(e) => {
            eprintln!("toolj: {e}");
            std::process::exit(2);
        }
    };

    if let Err(e) = run(args) {
        eprintln!("toolj: {e:#}");
        std::process::exit(1);
    }
}

fn run(args: Args) -> Result<()> {
    let cfg = Config::load();
    // Prefer the saved name for this URI so the header matches the server list.
    let name = args.name.clone().unwrap_or_else(|| {
        cfg.servers
            .iter()
            .find(|s| s.uri == args.uri)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| derive_name(&args.uri))
    });

    // Connect before taking over the terminal so failures are cheap, but do not
    // abort: the Servers view lets the user fix the URI interactively.
    let (db, err) = match Db::connect(&name, &args.uri) {
        Ok(db) => (Some(db), None),
        Err(e) => (None, Some(app::format_err(&e))),
    };

    let mut term = setup_terminal()?;
    let result = event_loop(&mut term, App::new(cfg, db, err));
    restore_terminal(&mut term)?;
    result
}

fn derive_name(uri: &str) -> String {
    let trimmed = uri
        .trim_start_matches("redis://")
        .trim_start_matches("rediss://");
    let host = trimmed.split(['/', '?']).next().unwrap_or(trimmed);
    if host.is_empty() || host.starts_with("localhost") || host.starts_with("127.0.0.1") {
        "local".to_string()
    } else {
        host.to_string()
    }
}

type Term = Terminal<CrosstermBackend<Stdout>>;

fn setup_terminal() -> Result<Term> {
    enable_raw_mode().context("cannot enable raw mode")?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen).context("cannot enter alternate screen")?;

    // A panic while the terminal is in raw mode would leave it unusable.
    let hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        hook(info);
    }));

    let term = Terminal::new(CrosstermBackend::new(out)).context("cannot create terminal")?;
    Ok(term)
}

fn restore_terminal(term: &mut Term) -> Result<()> {
    disable_raw_mode().ok();
    execute!(term.backend_mut(), LeaveAlternateScreen).ok();
    term.show_cursor().ok();
    Ok(())
}

fn event_loop(term: &mut Term, mut app: App) -> Result<()> {
    let mut page: isize = input::PAGE;
    let mut last_poll = Instant::now();

    loop {
        let mut frame_page = page;
        term.draw(|f| {
            frame_page = ui::draw(f, &mut app).page;
        })?;
        page = frame_page;

        if event::poll(TICK)? {
            match event::read()? {
                Event::Key(key) => input::handle_key(&mut app, key, page),
                Event::Resize(_, _) => {}
                _ => {}
            }
        }

        // Surface background subscriber failures once each.
        for e in app.sub.take_errors() {
            app.set_status(Level::Error, format!("subscriber: {e}"));
        }

        // The channel list and subscriber counts are server-side state; poll
        // them while the Pub/Sub view is open so it stays current.
        if app.view == app::View::PubSub
            && app.prompt.is_none()
            && last_poll.elapsed() >= PUBSUB_POLL
        {
            app.refresh_channels();
            last_poll = Instant::now();
        }

        if app.should_quit {
            app.sub.stop_all();
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_from_uris() {
        assert_eq!(derive_name("redis://localhost:6379/0"), "local");
        assert_eq!(derive_name("redis://127.0.0.1:6379"), "local");
        assert_eq!(
            derive_name("redis://cache.example:6379/1"),
            "cache.example:6379"
        );
    }
}
