//! `toole` — an interactive regular expression tester for the terminal.
//!
//! Loads a text file, matches a regular expression against it as you type,
//! highlights every match in place, lists the 0-indexed character offsets of
//! each one, and previews the result of a replacement over the whole file.

mod app;
mod cli;
mod doc;
mod engine;
mod help;
mod input;
mod textlayout;
mod theme;
mod ui;

use std::io::{self, IsTerminal, Write};
use std::panic;
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use crossterm::event::{self, Event};
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use crossterm::{execute, ExecutableCommand};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use app::App;
use cli::Cli;

fn main() -> ExitCode {
    let cli = Cli::parse();

    if cli.list_presets {
        print_presets();
        return ExitCode::SUCCESS;
    }

    match run(&cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // The terminal has already been restored by this point, so a plain
            // message on stderr is visible.
            let _ = writeln!(io::stderr(), "toole: {e}");
            ExitCode::FAILURE
        }
    }
}

/// List the built-in presets on stdout.
fn print_presets() {
    let width = app::PRESETS.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
    println!("Built-in pattern presets (F11, or [ ] inside the TUI):\n");
    for (name, pat) in app::PRESETS {
        println!("  {name:<width$}  {pat}");
    }
}

/// Load the file, then either print a report or run the TUI.
fn run(cli: &Cli) -> io::Result<()> {
    let mut app = App::new(cli)?;

    // `--print`, and any non-interactive stdout, produce a plain-text report
    // instead of trying to drive a terminal that is not there.
    if cli.print || !io::stdout().is_terminal() {
        return print_report(&app, cli);
    }

    let mut term = setup_terminal()?;
    let result = event_loop(&mut term, &mut app);
    // Restore the terminal even if the loop failed, then report the loop's
    // error in preference to any restore error.
    let restored = restore_terminal(&mut term);
    result.and(restored)
}

/// Enter the alternate screen and raw mode, installing a panic hook that
/// restores the terminal first so a crash cannot leave it unusable.
fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<io::Stdout>>> {
    let hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = io::stdout().execute(LeaveAlternateScreen);
        let _ = io::stdout().execute(crossterm::cursor::Show);
        hook(info);
    }));

    enable_raw_mode()?;
    let mut out = io::stdout();
    execute!(out, EnterAlternateScreen)?;
    let mut term = Terminal::new(CrosstermBackend::new(out))?;
    term.clear()?;
    Ok(term)
}

/// Leave the alternate screen and raw mode.
fn restore_terminal(term: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen)?;
    term.show_cursor()?;
    Ok(())
}

/// Draw, wait for a key, repeat.
fn event_loop(term: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        // The cursor is positioned by the focused input field during drawing;
        // hide it first so it does not flicker in the previous position.
        term.hide_cursor()?;
        term.draw(|f| ui::draw(f, app))?;

        // Polling with a timeout keeps the loop responsive to resizes without
        // spinning on the CPU.
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) => {
                app.handle_key(key);
                if app.should_quit {
                    return Ok(());
                }
            }
            // A resize just needs a redraw, which the next iteration does.
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
}

/// Write a plain-text match report to stdout.
///
/// This is what runs when stdout is not a terminal, and it makes the same
/// matching and replacement logic scriptable and testable end to end.
fn print_report(app: &App, cli: &Cli) -> io::Result<()> {
    let mut out = io::stdout().lock();

    writeln!(out, "file: {}", app.doc.path().display())?;
    writeln!(
        out,
        "lines: {}  chars: {}  bytes: {}",
        app.doc.line_count(),
        app.doc.char_count(),
        app.doc.byte_count()
    )?;

    if app.pattern.is_empty() {
        writeln!(out, "\nno pattern given: pass -e PATTERN to match")?;
        return Ok(());
    }

    writeln!(out, "pattern: {}", app.pattern.value())?;
    // The effective pattern and backend only exist once compilation succeeded.
    if !app.matches.effective_pattern.is_empty() {
        writeln!(out, "effective: {}", app.matches.effective_pattern)?;
    }
    if let Some(b) = app.matches.backend {
        writeln!(out, "engine: {}", b.label())?;
    }

    if let Some(e) = &app.matches.error {
        writeln!(out, "\nerror: {e}")?;
        // An invalid pattern is a usage error, so exit non-zero.
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid pattern: {e}"),
        ));
    }

    writeln!(out, "\nmatches: {}", app.matches.len())?;
    if app.matches.truncated {
        writeln!(out, "(stopped at the {} match limit)", engine::MATCH_LIMIT)?;
    }
    writeln!(
        out,
        "{:>5} {:>8} {:>8} {:>9}  text",
        "#", "start", "end", "line:col"
    )?;
    for (i, m) in app.matches.matches.iter().enumerate() {
        let (cs, ce) = app.char_offsets(m);
        let (line, col) = app.line_col(m);
        writeln!(
            out,
            "{:>5} {:>8} {:>8} {:>9}  {}",
            i + 1,
            cs,
            ce,
            format!("{line}:{col}"),
            ui::display_snippet(&m.text, 120)
        )?;
        for g in &m.groups {
            if g.index == 0 {
                continue;
            }
            let name = g
                .name
                .as_ref()
                .map(|n| format!(" ({n})"))
                .unwrap_or_default();
            match (&g.range, &g.text) {
                (Some((s, e)), Some(t)) => writeln!(
                    out,
                    "        ${}{} {}..{}  {}",
                    g.index,
                    name,
                    app.doc.byte_to_char(*s),
                    app.doc.byte_to_char(*e),
                    ui::display_snippet(t, 100)
                )?,
                _ => writeln!(out, "        ${}{} —", g.index, name)?,
            }
        }
    }

    if let Some(r) = &app.replacement {
        writeln!(
            out,
            "\nreplacement: {}  ({} applied{})",
            cli.replace.as_deref().unwrap_or(""),
            r.applied,
            if cli.first_only { ", first only" } else { "" }
        )?;
        writeln!(out, "--- replaced content ---")?;
        out.write_all(r.text.as_bytes())?;
        if !r.text.ends_with('\n') {
            writeln!(out)?;
        }
        writeln!(out, "--- end ---")?;
    }

    Ok(())
}
