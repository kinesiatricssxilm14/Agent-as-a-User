//! Render tests over a real `ratatui` frame buffer.
//!
//! These assert on the drawn characters, which is what enforces the
//! same-screen requirement: the file list, the selected file's name, and its
//! content must all appear in a single frame — including while a prompt, a
//! confirmation, or the help panel is on screen.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use toolc::app::{App, Focus, PromptKind};
use toolc::ui;

/// Render one frame and flatten the buffer into lines of text.
///
/// A double-width glyph occupies two cells; the second is a filler that
/// `TestBackend` reports as a blank. Skipping it keeps the reconstructed line
/// identical to what the terminal actually shows, so width assertions and
/// substring searches both behave.
fn render(app: &mut App, width: u16, height: u16) -> Vec<String> {
    use unicode_width::UnicodeWidthStr;

    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| ui::draw(frame, app))
        .expect("draw succeeds");
    let buffer = terminal.backend().buffer().clone();
    (0..buffer.area.height)
        .map(|y| {
            let mut line = String::new();
            let mut skip = 0usize;
            for x in 0..buffer.area.width {
                if skip > 0 {
                    skip -= 1;
                    continue;
                }
                let symbol = buffer[(x, y)].symbol();
                skip = UnicodeWidthStr::width(symbol).saturating_sub(1);
                line.push_str(symbol);
            }
            line.trim_end().to_string()
        })
        .collect()
}

fn screen(app: &mut App, width: u16, height: u16) -> String {
    render(app, width, height).join("\n")
}

mod support {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    pub fn scratch(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("toolc-render-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub fn write(path: &Path, body: &str) {
        fs::write(path, body).unwrap();
    }

    pub fn app_at(root: &Path) -> App {
        App::new(root.to_path_buf(), false)
    }
}

use std::fs;
use support::{app_at, scratch, write};

#[test]
fn list_and_preview_appear_in_the_same_frame() {
    let root = scratch("dual");
    write(&root.join("config.yml"), "server: prod\nport: 8080\n");
    write(&root.join("other.txt"), "unrelated\n");
    fs::create_dir(root.join("subdir")).unwrap();

    let mut app = app_at(&root);
    // Directories sort above files, so step the cursor onto the file to read it.
    app.selected = app.listing.index_of_name("config.yml").unwrap();
    app.refresh_preview();
    let out = screen(&mut app, 120, 30);

    // Both pane titles are drawn.
    assert!(out.contains("Files"), "file list pane missing:\n{out}");
    assert!(out.contains("Preview"), "preview pane missing:\n{out}");
    // Every sibling entry is listed.
    for name in ["config.yml", "other.txt", "subdir"] {
        assert!(out.contains(name), "entry {name} missing:\n{out}");
    }
    // The selected file's name and its full content are on the same screen.
    assert!(
        out.contains("server: prod"),
        "content line 1 missing:\n{out}"
    );
    assert!(out.contains("port: 8080"), "content line 2 missing:\n{out}");
    // The working directory is visible in the header.
    assert!(out.contains("cwd"), "header missing:\n{out}");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn selected_file_name_is_shown_with_its_content() {
    let root = scratch("name");
    write(&root.join("alpha.conf"), "alpha body\n");
    write(&root.join("beta.conf"), "beta body\n");
    let mut app = app_at(&root);

    let out = screen(&mut app, 120, 30);
    assert!(out.contains("alpha.conf"));
    assert!(out.contains("alpha body"));

    app.select_next(1);
    let out = screen(&mut app, 120, 30);
    assert!(out.contains("beta.conf"));
    assert!(out.contains("beta body"));
    // The preview switched over; the other file's body is gone.
    assert!(!out.contains("alpha body"), "stale preview content:\n{out}");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn full_content_of_a_multi_line_file_is_reachable_by_scrolling() {
    let root = scratch("scroll");
    let body: String = (1..=120).map(|i| format!("entry-{i}\n")).collect();
    write(&root.join("long.log"), &body);
    let mut app = app_at(&root);

    let first = screen(&mut app, 120, 30);
    assert!(first.contains("entry-1"));
    assert!(
        !first.contains("entry-120"),
        "tall file should not fit at once"
    );

    // Scroll the preview to the end; the last line becomes visible.
    app.focus = Focus::Preview;
    let viewport = app.preview_viewport;
    app.preview.scroll_to_bottom(viewport);
    let last = screen(&mut app, 120, 30);
    assert!(
        last.contains("entry-120"),
        "end of file unreachable:\n{last}"
    );
    // The file list is still on screen while the preview is scrolled.
    assert!(last.contains("long.log"));
    assert!(last.contains("Files"));
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn prompt_does_not_hide_the_list_or_the_preview() {
    let root = scratch("prompt");
    write(&root.join("data.txt"), "important payload\n");
    write(&root.join("sibling.txt"), "sibling\n");
    let mut app = app_at(&root);
    app.open_prompt(PromptKind::Copy);
    for ch in "backup/data.txt".chars() {
        app.prompt.as_mut().unwrap().insert(ch);
    }

    let out = screen(&mut app, 120, 30);
    // The prompt is visible, with what the user typed.
    assert!(out.contains("Copy to"), "prompt title missing:\n{out}");
    assert!(
        out.contains("backup/data.txt"),
        "typed text missing:\n{out}"
    );
    // ...and it did not cover the panes.
    assert!(
        out.contains("important payload"),
        "preview hidden by prompt:\n{out}"
    );
    assert!(out.contains("sibling.txt"), "list hidden by prompt:\n{out}");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn confirmation_keeps_the_panes_visible() {
    let root = scratch("confirm");
    write(&root.join("doomed.txt"), "delete me\n");
    let mut app = app_at(&root);
    app.request_delete();

    let out = screen(&mut app, 120, 30);
    assert!(out.contains("Confirm"), "confirm bar missing:\n{out}");
    assert!(out.contains("[y]"), "yes key not labelled:\n{out}");
    assert!(out.contains("doomed.txt"), "list hidden by confirm:\n{out}");
    assert!(
        out.contains("delete me"),
        "preview hidden by confirm:\n{out}"
    );
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn help_panel_is_a_column_not_an_overlay() {
    let root = scratch("help");
    write(&root.join("visible.txt"), "still here\n");
    let mut app = app_at(&root);
    app.toggle_help();

    let out = screen(&mut app, 140, 34);
    // Help content is present. Long descriptions soft-wrap inside the narrow
    // column, so assert on the headings and short entries that stay on one row.
    assert!(out.contains("Keys"), "help panel missing:\n{out}");
    assert!(out.contains("Navigate"), "help heading missing:\n{out}");
    assert!(
        out.contains("File operations"),
        "help heading missing:\n{out}"
    );
    assert!(out.contains("previous entry"), "help text missing:\n{out}");
    // And the panes it sits beside are untouched.
    assert!(out.contains("visible.txt"), "list hidden by help:\n{out}");
    assert!(out.contains("still here"), "preview hidden by help:\n{out}");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn help_column_scrolls_to_reveal_later_groups() {
    let root = scratch("helpscroll");
    write(&root.join("f.txt"), "body\n");
    let mut app = app_at(&root);
    app.toggle_help();

    // A short terminal cannot show every group at once.
    let top = screen(&mut app, 140, 20);
    assert!(top.contains("Navigate"), "first group missing:\n{top}");
    assert!(
        app.help_max_scroll > 0,
        "help should need scrolling at 140x20"
    );

    // Scrolling down brings the later groups into view.
    for _ in 0..app.help_max_scroll {
        app.scroll_help(1);
    }
    let bottom = screen(&mut app, 140, 20);
    assert!(bottom.contains("quit"), "last group unreachable:\n{bottom}");
    // The panes remain visible throughout.
    assert!(bottom.contains("f.txt"));
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn key_hints_are_always_on_screen() {
    let root = scratch("keybar");
    write(&root.join("f.txt"), "x\n");
    let mut app = app_at(&root);
    let out = screen(&mut app, 130, 30);
    // Each core operation is discoverable from the bar without opening help.
    for hint in [
        "copy", "move", "rename", "new dir", "delete", "help", "quit",
    ] {
        assert!(out.contains(hint), "key bar missing `{hint}`:\n{out}");
    }
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn metadata_panel_shows_size_and_permissions() {
    let root = scratch("meta");
    write(&root.join("sized.bin"), "0123456789");
    let mut app = app_at(&root);
    let out = screen(&mut app, 120, 30);
    assert!(out.contains("10 bytes"), "byte count missing:\n{out}");
    assert!(out.contains("regular file"), "type missing:\n{out}");
    // The rwx string for a normal file starts with '-'.
    assert!(out.contains("-rw"), "permissions missing:\n{out}");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn directory_selection_previews_children() {
    let root = scratch("dirprev");
    fs::create_dir(root.join("bundle")).unwrap();
    write(&root.join("bundle/inner.txt"), "inner\n");
    let mut app = app_at(&root);
    let out = screen(&mut app, 120, 30);
    assert!(out.contains("bundle"));
    assert!(
        out.contains("inner.txt"),
        "directory children not previewed:\n{out}"
    );
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn empty_directory_renders_without_panicking() {
    let root = scratch("emptydir");
    let mut app = app_at(&root);
    let out = screen(&mut app, 100, 24);
    assert!(
        out.contains("empty directory"),
        "empty state missing:\n{out}"
    );
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn narrow_and_short_terminals_are_handled() {
    let root = scratch("small");
    write(&root.join("a.txt"), "body\n");
    let mut app = app_at(&root);
    // Below the minimum: a readable notice instead of a broken layout.
    let out = screen(&mut app, 30, 8);
    assert!(
        out.contains("too small"),
        "no small-terminal notice:\n{out}"
    );
    // At a tight but workable size the panes still render.
    let out = screen(&mut app, 60, 14);
    assert!(out.contains("a.txt"), "list missing at 60x14:\n{out}");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn wide_unicode_content_does_not_break_the_layout() {
    let root = scratch("unicode");
    write(&root.join("English-only text.txt"), "English-only textのテキスト\nsecond line\n");
    let mut app = app_at(&root);
    let out = screen(&mut app, 120, 30);
    assert!(out.contains("English-only text"), "unicode name missing:\n{out}");
    assert!(
        out.contains("second line"),
        "unicode content missing:\n{out}"
    );
    // Every rendered row must stay inside the terminal width.
    for line in render(&mut app, 120, 30) {
        assert!(
            unicode_width::UnicodeWidthStr::width(line.as_str()) <= 120,
            "row overflows the terminal: {line}"
        );
    }
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn binary_file_shows_a_hex_dump() {
    let root = scratch("hex");
    fs::write(root.join("blob.bin"), [0u8, 1, 2, 3, 0xff]).unwrap();
    let mut app = app_at(&root);
    let out = screen(&mut app, 120, 30);
    assert!(out.contains("00000000"), "hex offset missing:\n{out}");
    assert!(out.contains("binary"), "binary label missing:\n{out}");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn status_line_reports_a_completed_operation() {
    let root = scratch("status");
    write(&root.join("src.txt"), "payload\n");
    let mut app = app_at(&root);
    app.open_prompt(PromptKind::Copy);
    for ch in "dst.txt".chars() {
        app.prompt.as_mut().unwrap().insert(ch);
    }
    app.submit_prompt();

    let out = screen(&mut app, 120, 30);
    assert!(out.contains("copied"), "success not reported:\n{out}");
    // The new file shows up in the same frame, straight from disk.
    assert!(out.contains("dst.txt"), "new file not listed:\n{out}");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn activity_log_renders_completed_operations() {
    let root = scratch("activity");
    write(&root.join("a.txt"), "a\n");
    let mut app = app_at(&root);
    app.open_prompt(PromptKind::MkDir);
    for ch in "archive".chars() {
        app.prompt.as_mut().unwrap().insert(ch);
    }
    app.submit_prompt();
    app.toggle_activity();

    let out = screen(&mut app, 120, 34);
    assert!(out.contains("Activity"), "activity pane missing:\n{out}");
    assert!(out.contains("mkdir"), "logged operation missing:\n{out}");
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn filter_state_is_advertised_in_the_header() {
    let root = scratch("filterhdr");
    for name in ["keep.log", "drop.txt"] {
        write(&root.join(name), name);
    }
    let mut app = app_at(&root);
    app.open_prompt(PromptKind::Filter);
    for ch in "keep".chars() {
        app.prompt.as_mut().unwrap().insert(ch);
        app.prompt_changed();
    }
    app.submit_prompt();

    let out = screen(&mut app, 120, 30);
    assert!(
        out.contains("filter:keep"),
        "filter not shown in header:\n{out}"
    );
    assert!(out.contains("keep.log"));
    assert!(
        !out.contains("drop.txt"),
        "filtered entry still listed:\n{out}"
    );
    fs::remove_dir_all(&root).unwrap();
}
