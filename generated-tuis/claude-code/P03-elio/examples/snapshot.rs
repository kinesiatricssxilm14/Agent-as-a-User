//! Renders frames of the TUI to stdout as plain text, for eyeballing the layout
//! without an interactive terminal.
//!
//!     cargo run --example snapshot -- <directory> [width] [height]
//!
//! Not part of the installed binary; it exists so the layout can be inspected
//! and diffed without driving a real terminal.

use std::path::PathBuf;

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use toolc::app::{App, PromptKind};
use toolc::{cli, ui};

fn frame(app: &mut App, width: u16, height: u16, caption: &str) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();

    println!("\n=== {caption} ({width}x{height}) ===");
    for y in 0..buffer.area.height {
        let mut line = String::new();
        let mut skip = 0usize;
        for x in 0..buffer.area.width {
            if skip > 0 {
                skip -= 1;
                continue;
            }
            let symbol = buffer[(x, y)].symbol();
            // A wide glyph owns the following cell; skip its blank filler.
            skip = unicode_width::UnicodeWidthStr::width(symbol).saturating_sub(1);
            line.push_str(symbol);
        }
        println!("{}", line.trim_end());
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let width: u16 = args.next().and_then(|v| v.parse().ok()).unwrap_or(120);
    let height: u16 = args.next().and_then(|v| v.parse().ok()).unwrap_or(32);

    let (dir, _) = cli::prepare_dir(&dir).unwrap_or_else(|err| {
        eprintln!("snapshot: {err}");
        std::process::exit(1);
    });

    let mut app = App::new(dir, false);
    frame(&mut app, width, height, "launch");

    // Step past the directories so a text file lands in the preview.
    while app.selected_entry().is_some_and(|e| e.is_dir_like())
        && app.selected + 1 < app.listing.len()
    {
        app.select_next(1);
    }
    frame(&mut app, width, height, "text file selected");

    app.open_prompt(PromptKind::Copy);
    for ch in "archive/copy.yml".chars() {
        if let Some(p) = &mut app.prompt {
            p.insert(ch);
        }
    }
    frame(&mut app, width, height, "copy prompt open");
    app.cancel_prompt();

    app.request_delete();
    frame(&mut app, width, height, "delete confirmation");
    app.confirm_no();

    app.toggle_help();
    frame(&mut app, width, height.max(38), "help column open");
    app.toggle_help();

    app.toggle_activity();
    frame(&mut app, width, height, "activity log open");
}
