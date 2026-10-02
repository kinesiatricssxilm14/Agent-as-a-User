//! Rendering tests using ratatui's `TestBackend`.
//!
//! These assert on the actual character grid, which is the only way to check the
//! "same-screen visibility" requirement: after each operation, the task lines,
//! the project column, the context column and the selected task's fields must
//! all be present in one buffer — no tabs, no modal that hides the list.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use tooll::app::{App, Key};
use tooll::config::{Config, FileSource};
use tooll::ui;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir =
            std::env::temp_dir().join(format!("tooll-render-{}-{tag}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Scratch { dir }
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn app_with(tag: &str, contents: &str) -> (Scratch, App) {
    let scratch = Scratch::new(tag);
    let path = scratch.dir.join("todo.txt");
    fs::write(&path, contents).unwrap();
    let cfg = Config {
        file: path,
        file_source: FileSource::Argument,
        config_path: None,
        hide_done: false,
        sort_on_load: false,
    };
    (scratch, App::new(cfg).unwrap())
}

/// Render one frame at `w`x`h` and return the screen as newline-joined text.
fn render(app: &mut App, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The default screen used by most tests: roomy enough for the whole layout.
fn screen(app: &mut App) -> String {
    render(app, 120, 32)
}

const SAMPLE: &str = "\
(B) Buy groceries +errands @home due:2026-03-15
(A) Pay rent +finance @computer due:2026-01-01
Call the plumber +house @phone
x (C) Renew passport +admin @computer
(D) Read ratatui docs +learning @computer due:2029-12-12
";

#[test]
fn the_list_shows_priority_projects_and_contexts_together() {
    let (_s, mut app) = app_with("list", SAMPLE);
    let out = screen(&mut app);

    // Every task is readable on the one screen.
    for text in [
        "Buy groceries",
        "Pay rent",
        "Call the plumber",
        "Renew passport",
        "Read ratatui docs",
    ] {
        assert!(out.contains(text), "missing task {text:?} in:\n{out}");
    }
    // With its priority, project and context metadata.
    for meta in ["(A)", "(B)", "(C)", "(D)", "+errands", "@home", "+finance", "@computer"] {
        assert!(out.contains(meta), "missing metadata {meta:?} in:\n{out}");
    }
    // And the completion marker for the done task.
    assert!(out.contains("[x]"), "missing a completed marker in:\n{out}");
    assert!(out.contains("[ ]"), "missing an open marker in:\n{out}");
}

#[test]
fn all_three_panels_and_the_detail_are_on_one_screen() {
    let (_s, mut app) = app_with("panels", SAMPLE);
    let out = screen(&mut app);

    assert!(out.contains("Tasks"), "no task panel:\n{out}");
    assert!(out.contains("Projects"), "no projects panel:\n{out}");
    assert!(out.contains("Contexts"), "no contexts panel:\n{out}");
    assert!(out.contains("Selected task"), "no detail panel:\n{out}");

    // The side panels list the real tags with counts.
    assert!(out.contains("+admin"), "{out}");
    assert!(out.contains("@phone"), "{out}");
}

#[test]
fn the_header_shows_the_file_path_and_counts() {
    let (_s, mut app) = app_with("header", SAMPLE);
    let path = app.store.path().display().to_string();
    let out = screen(&mut app);

    // The path can be long; check the file name at least is visible.
    assert!(
        out.contains("todo.txt") || out.contains(&path),
        "the task file is not shown:\n{out}"
    );
    assert!(out.contains("5/5"), "counts missing:\n{out}");
    assert!(out.contains("open 4"), "open count missing:\n{out}");
    assert!(out.contains("done 1"), "done count missing:\n{out}");
}

#[test]
fn the_footer_always_advertises_keys() {
    let (_s, mut app) = app_with("footer", SAMPLE);
    let out = screen(&mut app);

    // Discoverability: the core keys are labelled without opening anything.
    for hint in ["add", "edit", "done", "priority", "search", "help", "quit"] {
        assert!(out.contains(hint), "footer hint {hint:?} missing:\n{out}");
    }
}

#[test]
fn the_detail_pane_breaks_out_every_field_of_the_selected_task() {
    let (_s, mut app) = app_with("detail", SAMPLE);
    app.selected = 1; // Pay rent
    let out = screen(&mut app);

    for label in ["Status", "Priority", "Projects", "Contexts", "Due", "Raw"] {
        assert!(out.contains(label), "detail label {label:?} missing:\n{out}");
    }
    assert!(out.contains("+finance"), "{out}");
    assert!(out.contains("due:2026-01-01"), "{out}");
}

#[test]
fn the_help_screen_lists_every_documented_key() {
    let (_s, mut app) = app_with("help", SAMPLE);
    app.on_key(Key::Char('?'));
    let out = render(&mut app, 120, 60);

    assert!(out.contains("keys and format"), "no help title:\n{out}");
    for section in ["Moving around", "Changing tasks", "Filtering and searching"] {
        assert!(out.contains(section), "section {section:?} missing:\n{out}");
    }
    // The format itself is documented in the TUI, not only in a README.
    assert!(out.contains("todo.txt line format"), "{out}");
    assert!(out.contains("project tag"), "{out}");
    assert!(out.contains("context tag"), "{out}");
}

#[test]
fn a_prompt_keeps_the_task_list_visible() {
    let (_s, mut app) = app_with("prompt-visible", SAMPLE);
    app.selected = 0;
    app.on_key(Key::Char('p'));
    let out = screen(&mut app);

    assert!(out.contains("Priority"), "no prompt label:\n{out}");
    // Critically, the list is not covered by the prompt.
    for text in ["Buy groceries", "Pay rent", "Call the plumber"] {
        assert!(
            out.contains(text),
            "prompt hid the task {text:?}:\n{out}"
        );
    }
    assert!(out.contains("Projects"), "prompt hid the side panel:\n{out}");
}

#[test]
fn the_form_keeps_the_task_list_visible_and_shows_all_fields() {
    let (_s, mut app) = app_with("form-visible", SAMPLE);
    app.on_key(Key::Char('a'));
    let out = screen(&mut app);

    // All five fields on screen at once - no wizard, no paging.
    for field in ["Description", "Priority", "Projects", "Contexts", "Due date"] {
        assert!(out.contains(field), "form field {field:?} missing:\n{out}");
    }
    assert!(out.contains("Will write"), "no live preview:\n{out}");
    // And the list is still there behind it.
    assert!(out.contains("Buy groceries"), "form hid the list:\n{out}");
}

#[test]
fn the_form_preview_updates_as_you_type() {
    let (_s, mut app) = app_with("preview", "");
    app.on_key(Key::Char('a'));
    for c in "Ship the release".chars() {
        app.on_key(Key::Char(c));
    }
    app.on_key(Key::Tab);
    app.on_key(Key::Char('a'));
    app.on_key(Key::Tab);
    for c in "work".chars() {
        app.on_key(Key::Char(c));
    }
    let out = screen(&mut app);

    assert!(
        out.contains("(A) Ship the release +work"),
        "the preview does not reflect the typed fields:\n{out}"
    );
}

#[test]
fn a_search_prompt_narrows_the_visible_rows_on_screen() {
    let (_s, mut app) = app_with("search-render", SAMPLE);
    app.on_key(Key::Char('/'));
    for c in "rent".chars() {
        app.on_key(Key::Char(c));
    }
    let out = screen(&mut app);

    assert!(out.contains("Pay rent"), "{out}");
    assert!(
        !out.contains("Buy groceries"),
        "the search did not narrow the list:\n{out}"
    );
    // The search term itself is echoed so the user knows why.
    assert!(out.contains("rent"), "{out}");
}

#[test]
fn an_active_filter_is_named_in_the_header() {
    let (_s, mut app) = app_with("filter-render", SAMPLE);
    app.set_project_filter(Some("finance".into()));
    let out = screen(&mut app);

    assert!(out.contains("+finance"), "the filter is not shown:\n{out}");
    assert!(out.contains("1/5"), "the filtered count is wrong:\n{out}");
}

#[test]
fn an_empty_file_explains_what_to_do() {
    let (_s, mut app) = app_with("empty-render", "");
    let out = screen(&mut app);

    assert!(out.contains("empty"), "{out}");
    assert!(out.contains("add your first task"), "{out}");
    assert!(out.contains("key binding"), "{out}");
}

#[test]
fn an_over_filtered_list_explains_itself_rather_than_looking_empty() {
    let (_s, mut app) = app_with("overfiltered", SAMPLE);
    app.set_project_filter(Some("nonexistent".into()));
    let out = screen(&mut app);

    assert!(out.contains("No task matches"), "{out}");
    assert!(out.contains("+nonexistent"), "{out}");
    assert!(out.contains("clear every filter"), "{out}");
}

#[test]
fn a_confirmation_shows_what_is_about_to_happen() {
    let (_s, mut app) = app_with("confirm", SAMPLE);
    app.selected = 2;
    app.on_key(Key::Char('D'));
    let out = screen(&mut app);

    assert!(out.contains("Delete this task?"), "{out}");
    assert!(
        out.contains("Call the plumber"),
        "the confirmation does not name the task:\n{out}"
    );
    assert!(out.contains("yes"), "no y/n hint:\n{out}");
}

#[test]
fn overdue_tasks_are_flagged_in_the_header() {
    // A date safely in the past relative to any plausible run date.
    let (_s, mut app) = app_with("overdue", "(A) Ancient task +old @here due:2000-01-01\n");
    let out = screen(&mut app);

    assert!(out.contains("overdue 1"), "no overdue count:\n{out}");
    assert!(out.contains("overdue by"), "no relative wording:\n{out}");
}

#[test]
fn a_status_message_reports_the_last_operation() {
    let (_s, mut app) = app_with("status", SAMPLE);
    app.selected = 0;
    app.on_key(Key::Char('+'));
    let out = screen(&mut app);

    assert!(out.contains("Priority"), "{out}");
    assert!(out.contains("saved to"), "no save confirmation:\n{out}");
}

#[test]
fn renders_at_small_and_large_sizes_without_panicking() {
    let (_s, mut app) = app_with("sizes", SAMPLE);
    for (w, h) in [(40, 12), (60, 16), (80, 24), (120, 40), (200, 60), (30, 10)] {
        let out = render(&mut app, w, h);
        assert!(!out.is_empty(), "{w}x{h} rendered nothing");
    }
}

#[test]
fn renders_every_mode_without_panicking() {
    let (_s, mut app) = app_with("modes", SAMPLE);
    // Each entry key, rendered at a cramped size to catch layout arithmetic bugs.
    for k in [
        Key::Char('?'),
        Key::Esc,
        Key::Char('a'),
        Key::Esc,
        Key::Char('e'),
        Key::Esc,
        Key::Char('p'),
        Key::Esc,
        Key::Char('c'),
        Key::Esc,
        Key::Char('P'),
        Key::Esc,
        Key::Char('d'),
        Key::Esc,
        Key::Char('/'),
        Key::Esc,
        Key::Char('f'),
        Key::Esc,
        Key::Char('@'),
        Key::Esc,
        Key::Char('D'),
        Key::Char('n'),
        Key::Char('X'),
        Key::Char('n'),
    ] {
        app.on_key(k);
        let out = render(&mut app, 34, 11);
        assert!(!out.is_empty(), "empty render after {k:?}");
    }
}

#[test]
fn a_long_list_scrolls_and_keeps_the_selection_on_screen() {
    let many: String = (1..=60)
        .map(|i| format!("(A) Task number {i} +bulk @screen\n"))
        .collect();
    let (_s, mut app) = app_with("scroll", &many);

    // Jump to the end; the last task must be rendered.
    app.on_key(Key::End);
    let out = render(&mut app, 100, 24);
    assert!(out.contains("Task number 60"), "{out}");
    assert!(!out.contains("Task number 1 "), "the view did not scroll:\n{out}");

    // Back to the top.
    app.on_key(Key::Home);
    let out = render(&mut app, 100, 24);
    assert!(out.contains("Task number 1 "), "{out}");
}

#[test]
fn file_line_numbers_anchor_the_view_to_the_file() {
    let (_s, mut app) = app_with("linenos", SAMPLE);
    let out = screen(&mut app);
    // Line numbers let the user match a row to a line in the file on disk.
    assert!(out.contains("  1 "), "{out}");
    assert!(out.contains("  5 "), "{out}");
}

#[test]
fn the_header_degrades_gracefully_instead_of_truncating_mid_word() {
    let (_s, mut app) = app_with("narrow-header", SAMPLE);
    // Narrow enough that not everything fits.
    let out = render(&mut app, 56, 20);

    let header = out.lines().nth(2).unwrap();
    assert!(header.contains("showing 5/5"), "the count must survive:\n{out}");
    // Whole items are dropped, so no label is left dangling with no value.
    assert!(
        !header.trim_end().ends_with("filter:") && !header.trim_end().ends_with("sort:"),
        "a label was cut from its value: {header:?}"
    );
}

#[test]
fn form_labels_never_touch_their_values() {
    let (_s, mut app) = app_with("form-spacing", "");
    app.on_key(Key::Char('a'));
    let out = screen(&mut app);

    // The longest label is "Description"; it must be followed by whitespace.
    for label in ["Description", "Priority", "Projects", "Contexts", "Due date"] {
        let line = out
            .lines()
            .find(|l| l.contains(label))
            .unwrap_or_else(|| panic!("no line for {label}"));
        let after = &line[line.find(label).unwrap() + label.len()..];
        assert!(
            after.starts_with(' '),
            "{label:?} abuts its value in {line:?}"
        );
    }
}

#[test]
fn long_task_lines_are_marked_as_clipped_and_can_be_scrolled() {
    let long = "(A) A task with a deliberately very long description that will not fit \
                on a narrow terminal at all +someproject @somecontext due:2026-12-31\n";
    let (_s, mut app) = app_with("clip", long);

    let out = render(&mut app, 70, 20);
    assert!(
        out.contains('›'),
        "a clipped line must say so rather than just ending:\n{out}"
    );

    // Scrolling right reveals the tail, and the left marker appears.
    app.on_key(Key::Char('>'));
    let out = render(&mut app, 70, 20);
    assert!(out.contains('‹'), "no left marker after scrolling:\n{out}");

    // Scroll far right: the end of the line becomes visible.
    for _ in 0..20 {
        app.on_key(Key::Char('>'));
    }
    let out = render(&mut app, 70, 20);
    assert!(out.contains("due:2026-12-31"), "the tail is unreachable:\n{out}");

    // And back.
    for _ in 0..40 {
        app.on_key(Key::Char('<'));
    }
    let out = render(&mut app, 70, 20);
    assert!(out.contains("(A) A task with"), "cannot scroll back:\n{out}");
}

#[test]
fn the_line_number_and_checkbox_stay_pinned_while_scrolling() {
    let long: String = (1..=3)
        .map(|i| format!("(A) Task {i} with an extremely long tail of text that overflows the pane +p{i} @c{i}\n"))
        .collect();
    let (_s, mut app) = app_with("pinned", &long);

    // Tall enough for all three rows to be in the viewport at once.
    render(&mut app, 60, 22); // establish the viewport metrics
    for _ in 0..3 {
        app.on_key(Key::Char('>'));
    }
    let out = render(&mut app, 60, 22);

    // Line numbers are the anchor to the file, so they must never scroll away.
    for n in ["  1 ", "  2 ", "  3 "] {
        assert!(out.contains(n), "line number {n:?} scrolled away:\n{out}");
    }
    assert!(out.contains("[ ]"), "the checkbox scrolled away:\n{out}");
}

#[test]
fn the_side_panels_yield_to_the_task_text_on_a_narrow_terminal() {
    let (_s, mut app) = app_with("yield", SAMPLE);

    // Roomy: all three panels.
    let wide = render(&mut app, 120, 24);
    assert!(wide.contains("Projects"), "{wide}");

    // Very narrow: the task list takes the whole width so text stays readable.
    let narrow = render(&mut app, 50, 20);
    assert!(narrow.contains("Tasks"), "{narrow}");
    assert!(narrow.contains("Buy groceries"), "{narrow}");
}

#[test]
fn a_very_short_terminal_still_shows_tasks_and_keys() {
    let (_s, mut app) = app_with("short", SAMPLE);
    let out = render(&mut app, 90, 11);

    assert!(out.contains("Buy groceries"), "no tasks on a short screen:\n{out}");
    // Even with no room for the detail pane, the key hints survive.
    assert!(out.contains("help") || out.contains("?"), "no key hint:\n{out}");
}
