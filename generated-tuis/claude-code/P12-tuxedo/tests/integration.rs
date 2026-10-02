//! End-to-end tests that drive the real application through key events and
//! then assert against the bytes on disk.
//!
//! Nothing here is mocked: each test creates a temporary `todo.txt`, builds a
//! real [`App`] against it, feeds it the same [`Key`] values the terminal would
//! produce, and reads the file back. That is the only way to check the
//! "interface state must match disk file content" requirement.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use tooll::app::{App, DoneFilter, Focus, Key, Mode, SortMode};
use tooll::config::{Config, FileSource};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A temporary directory that cleans itself up.
struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(tag: &str) -> Scratch {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("tooll-it-{}-{tag}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create scratch dir");
        Scratch { dir }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("todo.txt")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Build an app over a scratch file seeded with `contents`.
fn app_with(tag: &str, contents: &str) -> (Scratch, App) {
    let scratch = Scratch::new(tag);
    let path = scratch.file();
    fs::write(&path, contents).expect("seed todo.txt");
    let cfg = Config {
        file: path,
        file_source: FileSource::Argument,
        config_path: None,
        hide_done: false,
        sort_on_load: false,
    };
    let app = App::new(cfg).expect("load app");
    (scratch, app)
}

/// The current on-disk contents of the task file.
fn on_disk(app: &App) -> String {
    fs::read_to_string(app.store.path()).expect("read todo.txt back")
}

/// Send a sequence of keys.
fn keys(app: &mut App, seq: &[Key]) {
    for k in seq {
        app.on_key(*k);
    }
}

/// Type a string, one `Key::Char` at a time.
fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.on_key(Key::Char(c));
    }
}

/// Move the task cursor to the row whose rendered line contains `needle`.
fn select_containing(app: &mut App, needle: &str) {
    app.focus = Focus::Tasks;
    let pos = app
        .rows()
        .iter()
        .position(|r| app.store.get(r.task_id).unwrap().render().contains(needle))
        .unwrap_or_else(|| panic!("no visible row contains {needle:?}"));
    app.selected = pos;
}

const SAMPLE: &str = "\
(B) Buy groceries +errands @home due:2026-03-15
(A) Pay rent +finance @computer due:2026-01-01
Call the plumber +house @phone
x (C) Renew passport +admin @computer
(D) Read ratatui docs +learning @computer due:2029-12-12
";

// ------------------------------------------------------------------- loading

#[test]
fn loads_every_task_and_shows_them_together() {
    let (_s, app) = app_with("load", SAMPLE);
    assert_eq!(app.store.len(), 5);
    // Same-screen visibility: no filter is applied at startup, so every task is
    // in the single list view.
    assert_eq!(app.visible_count(), 5);
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.focus, Focus::Tasks);
    assert!(!app.is_filtered());
}

#[test]
fn loading_does_not_rewrite_the_file() {
    let (_s, app) = app_with("noop", SAMPLE);
    assert_eq!(on_disk(&app), SAMPLE, "opening a file must not change it");
}

#[test]
fn a_missing_file_is_created_on_the_first_change() {
    let scratch = Scratch::new("create");
    let path = scratch.dir.join("sub").join("todo.txt");
    let cfg = Config {
        file: path.clone(),
        file_source: FileSource::Default,
        config_path: None,
        hide_done: false,
        sort_on_load: false,
    };
    let mut app = App::new(cfg).expect("load app for a missing file");
    assert_eq!(app.store.len(), 0);
    assert!(!path.exists());

    keys(&mut app, &[Key::Char('a')]);
    type_text(&mut app, "First task");
    keys(&mut app, &[Key::Enter]);

    assert_eq!(fs::read_to_string(&path).unwrap(), "First task\n");
}

// ------------------------------------------------------------------ priority

#[test]
fn raising_priority_writes_through_to_disk() {
    let (_s, mut app) = app_with("raise", SAMPLE);
    select_containing(&mut app, "Buy groceries");

    keys(&mut app, &[Key::Char('+')]);

    assert!(on_disk(&app).contains("(A) Buy groceries +errands @home due:2026-03-15"));
    // The interface reflects the same change.
    assert_eq!(app.selected_task().unwrap().priority(), Some('A'));
}

#[test]
fn lowering_priority_writes_through_to_disk() {
    let (_s, mut app) = app_with("lower", SAMPLE);
    select_containing(&mut app, "Pay rent");

    keys(&mut app, &[Key::Char('-')]);

    assert!(on_disk(&app).contains("(B) Pay rent"));
    assert_eq!(app.selected_task().unwrap().priority(), Some('B'));
}

#[test]
fn priority_prompt_sets_an_arbitrary_letter() {
    let (_s, mut app) = app_with("pri-prompt", SAMPLE);
    select_containing(&mut app, "Call the plumber");

    keys(&mut app, &[Key::Char('p')]);
    assert!(matches!(app.mode, Mode::Prompt(_)));
    type_text(&mut app, "E");
    keys(&mut app, &[Key::Enter]);

    assert_eq!(app.mode, Mode::Normal);
    assert!(on_disk(&app).contains("(E) Call the plumber +house @phone"));
}

#[test]
fn priority_prompt_clears_with_a_dash() {
    let (_s, mut app) = app_with("pri-clear", SAMPLE);
    select_containing(&mut app, "Pay rent");

    keys(&mut app, &[Key::Char('p')]);
    // The prompt is pre-filled with the current value; clear it first.
    keys(&mut app, &[Key::CtrlChar('u')]);
    type_text(&mut app, "-");
    keys(&mut app, &[Key::Enter]);

    let disk = on_disk(&app);
    assert!(disk.contains("Pay rent +finance @computer due:2026-01-01"));
    assert!(!disk.contains("(A) Pay rent"));
}

#[test]
fn priority_prompt_rejects_nonsense_and_leaves_the_file_alone() {
    let (_s, mut app) = app_with("pri-bad", SAMPLE);
    let before = on_disk(&app);
    select_containing(&mut app, "Buy groceries");

    keys(&mut app, &[Key::Char('p'), Key::CtrlChar('u')]);
    type_text(&mut app, "AB");
    keys(&mut app, &[Key::Enter]);

    assert_eq!(on_disk(&app), before, "an invalid priority must not write");
    let msg = app.message.as_ref().expect("an error message");
    assert!(msg.text.contains("not a priority"), "{}", msg.text);
}

#[test]
fn priority_cannot_go_above_a() {
    let (_s, mut app) = app_with("pri-ceiling", SAMPLE);
    select_containing(&mut app, "Pay rent");
    let before = on_disk(&app);

    keys(&mut app, &[Key::Char('+')]);

    assert_eq!(on_disk(&app), before);
    assert!(app
        .message
        .as_ref()
        .unwrap()
        .text
        .contains("highest priority"));
}

// ------------------------------------------------------------------ complete

#[test]
fn completing_a_task_writes_the_x_prefix() {
    let (_s, mut app) = app_with("complete", SAMPLE);
    select_containing(&mut app, "Buy groceries");

    keys(&mut app, &[Key::Char(' ')]);

    let disk = on_disk(&app);
    let line = disk
        .lines()
        .find(|l| l.contains("Buy groceries"))
        .expect("the task is still in the file");
    assert!(line.starts_with("x "), "expected an `x ` prefix in {line:?}");
    assert!(line.contains("(B)"), "priority is kept: {line:?}");
    assert!(app.selected_task().unwrap().completed);
}

#[test]
fn completion_stamps_todays_date() {
    let (_s, mut app) = app_with("complete-date", "Water the plants\n");
    let today = tooll::date::format_ymd(app.today);

    keys(&mut app, &[Key::Char(' ')]);

    let disk = on_disk(&app);
    assert!(disk.contains(&today), "expected {today} in {disk:?}");
    assert_eq!(disk, format!("x {today} {today} Water the plants\n"));
}

#[test]
fn completion_toggles_back_off() {
    let (_s, mut app) = app_with("uncomplete", SAMPLE);
    select_containing(&mut app, "Renew passport");
    assert!(app.selected_task().unwrap().completed);

    keys(&mut app, &[Key::Char('x')]);

    let disk = on_disk(&app);
    let line = disk.lines().find(|l| l.contains("Renew passport")).unwrap();
    assert!(!line.starts_with("x "), "{line:?}");
    assert!(!app.selected_task().unwrap().completed);
}

// ------------------------------------------------------------------ contexts

#[test]
fn context_prompt_replaces_the_tags() {
    let (_s, mut app) = app_with("ctx", SAMPLE);
    select_containing(&mut app, "Buy groceries");

    keys(&mut app, &[Key::Char('c'), Key::CtrlChar('u')]);
    type_text(&mut app, "town, errands");
    keys(&mut app, &[Key::Enter]);

    let disk = on_disk(&app);
    let line = disk.lines().find(|l| l.contains("Buy groceries")).unwrap();
    assert!(line.contains("@town"), "{line:?}");
    assert!(line.contains("@errands"), "{line:?}");
    assert!(!line.contains("@home"), "the old context is gone: {line:?}");
    // The project tag is untouched.
    assert!(line.contains("+errands"), "{line:?}");
    assert_eq!(
        app.selected_task().unwrap().contexts(),
        vec!["town", "errands"]
    );
}

#[test]
fn context_prompt_accepts_a_leading_sigil() {
    let (_s, mut app) = app_with("ctx-sigil", "Task one\n");

    keys(&mut app, &[Key::Char('c')]);
    type_text(&mut app, "@office");
    keys(&mut app, &[Key::Enter]);

    assert_eq!(on_disk(&app), "Task one @office\n");
}

#[test]
fn clearing_the_context_prompt_removes_all_contexts() {
    let (_s, mut app) = app_with("ctx-clear", SAMPLE);
    select_containing(&mut app, "Call the plumber");

    keys(&mut app, &[Key::Char('c'), Key::CtrlChar('u'), Key::Enter]);

    let disk = on_disk(&app);
    let line = disk.lines().find(|l| l.contains("Call the plumber")).unwrap();
    assert_eq!(line, "Call the plumber +house");
}

#[test]
fn project_prompt_replaces_the_tags() {
    let (_s, mut app) = app_with("proj", SAMPLE);
    select_containing(&mut app, "Call the plumber");

    keys(&mut app, &[Key::Char('P'), Key::CtrlChar('u')]);
    type_text(&mut app, "+home-repair");
    keys(&mut app, &[Key::Enter]);

    let disk = on_disk(&app);
    let line = disk.lines().find(|l| l.contains("Call the plumber")).unwrap();
    assert!(line.contains("+home-repair"), "{line:?}");
    assert!(!line.contains("+house"), "{line:?}");
    assert!(line.contains("@phone"), "context untouched: {line:?}");
}

// ----------------------------------------------------------------- due dates

#[test]
fn due_prompt_sets_and_clears_the_date() {
    let (_s, mut app) = app_with("due", SAMPLE);
    select_containing(&mut app, "Call the plumber");

    keys(&mut app, &[Key::Char('d')]);
    type_text(&mut app, "2027-06-30");
    keys(&mut app, &[Key::Enter]);
    assert!(on_disk(&app).contains("due:2027-06-30"));

    keys(&mut app, &[Key::Char('d'), Key::CtrlChar('u'), Key::Enter]);
    let disk = on_disk(&app);
    let line = disk.lines().find(|l| l.contains("Call the plumber")).unwrap();
    assert!(!line.contains("due:"), "{line:?}");
}

#[test]
fn due_prompt_rejects_an_impossible_date() {
    let (_s, mut app) = app_with("due-bad", SAMPLE);
    let before = on_disk(&app);
    select_containing(&mut app, "Call the plumber");

    keys(&mut app, &[Key::Char('d')]);
    type_text(&mut app, "2026-02-30");
    keys(&mut app, &[Key::Enter]);

    assert_eq!(on_disk(&app), before);
    assert!(app.message.as_ref().unwrap().text.contains("not a valid date"));
}

// ----------------------------------------------------------------- adding

#[test]
fn the_add_form_builds_a_complete_todo_txt_line() {
    let (_s, mut app) = app_with("add", "");

    keys(&mut app, &[Key::Char('a')]);
    assert_eq!(app.mode, Mode::Form);

    type_text(&mut app, "Submit the tax return");
    keys(&mut app, &[Key::Tab]);
    type_text(&mut app, "A");
    keys(&mut app, &[Key::Tab]);
    type_text(&mut app, "finance");
    keys(&mut app, &[Key::Tab]);
    type_text(&mut app, "computer");
    keys(&mut app, &[Key::Tab]);
    type_text(&mut app, "2026-04-30");
    keys(&mut app, &[Key::Enter]);

    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(
        on_disk(&app),
        "(A) Submit the tax return +finance @computer due:2026-04-30\n"
    );
    assert_eq!(app.visible_count(), 1);
}

#[test]
fn the_add_form_appends_without_disturbing_existing_lines() {
    let (_s, mut app) = app_with("add-append", SAMPLE);

    keys(&mut app, &[Key::Char('a')]);
    type_text(&mut app, "Water the plants");
    keys(&mut app, &[Key::Enter]);

    let disk = on_disk(&app);
    assert!(disk.starts_with(SAMPLE), "existing lines are preserved");
    assert!(disk.ends_with("Water the plants\n"));
    assert_eq!(app.store.len(), 6);
}

#[test]
fn the_add_form_rejects_an_empty_description() {
    let (_s, mut app) = app_with("add-empty", "");

    keys(&mut app, &[Key::Char('a'), Key::Enter]);

    assert_eq!(app.mode, Mode::Form, "the form stays open on an error");
    assert!(app
        .form
        .as_ref()
        .unwrap()
        .error
        .as_ref()
        .unwrap()
        .contains("empty"));
    assert_eq!(on_disk(&app), "");
}

#[test]
fn the_add_form_rejects_a_bad_priority_and_focuses_that_field() {
    let (_s, mut app) = app_with("add-bad-pri", "");

    keys(&mut app, &[Key::Char('a')]);
    type_text(&mut app, "Something");
    keys(&mut app, &[Key::Tab]);
    type_text(&mut app, "9");
    keys(&mut app, &[Key::Enter]);

    assert_eq!(app.mode, Mode::Form);
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.field, tooll::app::FormField::Priority);
    assert!(form.error.is_some());
    assert_eq!(on_disk(&app), "");
}

#[test]
fn escaping_the_add_form_writes_nothing() {
    let (_s, mut app) = app_with("add-esc", SAMPLE);

    keys(&mut app, &[Key::Char('a')]);
    type_text(&mut app, "Never saved");
    keys(&mut app, &[Key::Esc]);

    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(on_disk(&app), SAMPLE);
}

#[test]
fn adding_inside_a_filter_seeds_the_form_with_that_tag() {
    let (_s, mut app) = app_with("add-seed", SAMPLE);
    app.set_project_filter(Some("finance".into()));
    app.set_context_filter(Some("computer".into()));

    keys(&mut app, &[Key::Char('a')]);
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.projects.text(), "finance");
    assert_eq!(form.contexts.text(), "computer");

    type_text(&mut app, "File the quarterly report");
    keys(&mut app, &[Key::Enter]);

    assert!(on_disk(&app).contains("File the quarterly report +finance @computer"));
}

// ------------------------------------------------------------------- editing

#[test]
fn the_edit_form_is_prefilled_and_rewrites_the_line() {
    let (_s, mut app) = app_with("edit", SAMPLE);
    select_containing(&mut app, "Buy groceries");

    keys(&mut app, &[Key::Char('e')]);
    let form = app.form.as_ref().unwrap();
    assert_eq!(form.description.text(), "Buy groceries");
    assert_eq!(form.priority.text(), "B");
    assert_eq!(form.projects.text(), "errands");
    assert_eq!(form.contexts.text(), "home");
    assert_eq!(form.due.text(), "2026-03-15");

    // Change the description and the priority only.
    keys(&mut app, &[Key::CtrlChar('u')]);
    type_text(&mut app, "Buy groceries and milk");
    keys(&mut app, &[Key::Tab, Key::CtrlChar('u')]);
    type_text(&mut app, "A");
    keys(&mut app, &[Key::Enter]);

    let disk = on_disk(&app);
    assert!(
        disk.contains("(A) Buy groceries and milk +errands @home due:2026-03-15"),
        "{disk}"
    );
    assert_eq!(app.store.len(), 5, "editing must not add a line");
}

#[test]
fn enter_on_a_task_opens_the_editor() {
    let (_s, mut app) = app_with("enter-edit", SAMPLE);
    select_containing(&mut app, "Pay rent");

    keys(&mut app, &[Key::Enter]);

    assert_eq!(app.mode, Mode::Form);
    assert_eq!(app.form.as_ref().unwrap().description.text(), "Pay rent");
}

// ------------------------------------------------------------------ filtering

#[test]
fn filters_by_project_from_the_side_panel() {
    let (_s, mut app) = app_with("filter-panel", SAMPLE);

    // Tab to the projects panel and pick the first project.
    keys(&mut app, &[Key::Tab]);
    assert_eq!(app.focus, Focus::Projects);
    // Entry 0 is "all"; step down to the first real project.
    keys(&mut app, &[Key::Down, Key::Enter]);

    let filter = app.project_filter.clone().expect("a project filter");
    assert_eq!(filter, app.project_entries()[0].0);
    // Every visible task carries the filter tag.
    assert!(app.visible_count() > 0);
    for row in app.rows() {
        assert!(app.store.get(row.task_id).unwrap().has_project(&filter));
    }
}

#[test]
fn filters_by_project_typed_into_the_prompt() {
    let (_s, mut app) = app_with("filter-typed", SAMPLE);

    keys(&mut app, &[Key::Char('f')]);
    type_text(&mut app, "computer");
    keys(&mut app, &[Key::Enter]);
    // `computer` is a context, not a project, so nothing matches - and the app
    // says so rather than silently showing an empty list.
    assert_eq!(app.visible_count(), 0);

    keys(&mut app, &[Key::Char('f'), Key::CtrlChar('u')]);
    type_text(&mut app, "+finance");
    keys(&mut app, &[Key::Enter]);

    assert_eq!(app.project_filter.as_deref(), Some("finance"));
    assert_eq!(app.visible_count(), 1);
    assert!(app.selected_task().unwrap().render().contains("Pay rent"));
}

#[test]
fn filters_by_context_typed_into_the_prompt() {
    let (_s, mut app) = app_with("filter-ctx", SAMPLE);

    keys(&mut app, &[Key::Char('@')]);
    type_text(&mut app, "@computer");
    keys(&mut app, &[Key::Enter]);

    assert_eq!(app.context_filter.as_deref(), Some("computer"));
    assert_eq!(app.visible_count(), 3);
    for row in app.rows() {
        assert!(app.store.get(row.task_id).unwrap().has_context("computer"));
    }
}

#[test]
fn project_and_context_filters_combine() {
    let (_s, mut app) = app_with("filter-both", SAMPLE);

    app.set_project_filter(Some("finance".into()));
    app.set_context_filter(Some("computer".into()));

    assert_eq!(app.visible_count(), 1);
    assert!(app.selected_task().unwrap().render().contains("Pay rent"));
    assert!(app.filter_summary().contains("+finance"));
    assert!(app.filter_summary().contains("@computer"));
}

#[test]
fn filtering_never_touches_the_file() {
    let (_s, mut app) = app_with("filter-readonly", SAMPLE);

    app.set_project_filter(Some("finance".into()));
    app.set_context_filter(Some("phone".into()));
    keys(&mut app, &[Key::Char('v'), Key::Char('s'), Key::Char('F')]);

    assert_eq!(on_disk(&app), SAMPLE, "viewing must not rewrite the file");
}

#[test]
fn clearing_filters_restores_the_full_list() {
    let (_s, mut app) = app_with("filter-clear", SAMPLE);
    app.set_project_filter(Some("finance".into()));
    assert_eq!(app.visible_count(), 1);

    keys(&mut app, &[Key::Char('F')]);

    assert_eq!(app.visible_count(), 5);
    assert!(!app.is_filtered());
    assert!(app.project_filter.is_none());
}

#[test]
fn selecting_the_active_project_again_toggles_it_off() {
    let (_s, mut app) = app_with("filter-toggle", SAMPLE);
    app.focus = Focus::Projects;
    keys(&mut app, &[Key::Down, Key::Enter]);
    assert!(app.project_filter.is_some());

    keys(&mut app, &[Key::Enter]);

    assert!(app.project_filter.is_none());
    assert_eq!(app.visible_count(), 5);
}

#[test]
fn drills_down_from_the_selected_task() {
    let (_s, mut app) = app_with("drill", SAMPLE);
    select_containing(&mut app, "Pay rent");

    keys(&mut app, &[Key::Char('t')]);
    assert_eq!(app.project_filter.as_deref(), Some("finance"));

    keys(&mut app, &[Key::Char('F')]);
    select_containing(&mut app, "Call the plumber");
    keys(&mut app, &[Key::Char('T')]);
    assert_eq!(app.context_filter.as_deref(), Some("phone"));
}

#[test]
fn visibility_cycles_between_open_and_done() {
    let (_s, mut app) = app_with("visibility", SAMPLE);
    assert_eq!(app.done_filter, DoneFilter::All);

    keys(&mut app, &[Key::Char('v')]);
    assert_eq!(app.done_filter, DoneFilter::OpenOnly);
    assert_eq!(app.visible_count(), 4);

    keys(&mut app, &[Key::Char('v')]);
    assert_eq!(app.done_filter, DoneFilter::DoneOnly);
    assert_eq!(app.visible_count(), 1);
    assert!(app.selected_task().unwrap().completed);

    keys(&mut app, &[Key::Char('v')]);
    assert_eq!(app.done_filter, DoneFilter::All);
    assert_eq!(app.visible_count(), 5);
}

// -------------------------------------------------------------------- search

#[test]
fn search_filters_the_list_as_you_type() {
    let (_s, mut app) = app_with("search", SAMPLE);

    keys(&mut app, &[Key::Char('/')]);
    type_text(&mut app, "plumb");
    // The list has already narrowed, before Enter is pressed.
    assert_eq!(app.visible_count(), 1);
    assert!(app.selected_task().unwrap().render().contains("plumber"));

    keys(&mut app, &[Key::Enter]);
    assert_eq!(app.search, "plumb");
    assert_eq!(app.visible_count(), 1);
}

#[test]
fn search_is_case_insensitive_and_covers_metadata() {
    let (_s, mut app) = app_with("search-meta", SAMPLE);

    keys(&mut app, &[Key::Char('/')]);
    type_text(&mut app, "@COMPUTER");
    assert_eq!(app.visible_count(), 3);

    keys(&mut app, &[Key::CtrlChar('u')]);
    type_text(&mut app, "due:2029");
    assert_eq!(app.visible_count(), 1);
}

#[test]
fn escaping_a_search_restores_the_list() {
    let (_s, mut app) = app_with("search-esc", SAMPLE);

    keys(&mut app, &[Key::Char('/')]);
    type_text(&mut app, "rent");
    assert_eq!(app.visible_count(), 1);

    keys(&mut app, &[Key::Esc]);

    assert_eq!(app.mode, Mode::Normal);
    assert!(app.search.is_empty());
    assert_eq!(app.visible_count(), 5);
}

#[test]
fn backspace_widens_a_live_search() {
    let (_s, mut app) = app_with("search-bs", SAMPLE);

    keys(&mut app, &[Key::Char('/')]);
    type_text(&mut app, "computerz");
    assert_eq!(app.visible_count(), 0);

    keys(&mut app, &[Key::Backspace]);
    assert_eq!(app.visible_count(), 3);
}

// ------------------------------------------------------------------- deleting

#[test]
fn deleting_asks_first_then_removes_the_line() {
    let (_s, mut app) = app_with("delete", SAMPLE);
    select_containing(&mut app, "Call the plumber");

    keys(&mut app, &[Key::Char('D')]);
    assert!(matches!(app.mode, Mode::Prompt(_)));
    // Declining leaves the file alone.
    keys(&mut app, &[Key::Char('n')]);
    assert_eq!(on_disk(&app), SAMPLE);

    keys(&mut app, &[Key::Char('D'), Key::Char('y')]);
    let disk = on_disk(&app);
    assert!(!disk.contains("Call the plumber"), "{disk}");
    assert_eq!(disk.lines().count(), 4);
}

#[test]
fn undo_restores_a_deleted_task() {
    let (_s, mut app) = app_with("undo-delete", SAMPLE);
    select_containing(&mut app, "Call the plumber");

    keys(&mut app, &[Key::Char('D'), Key::Char('y')]);
    assert!(!on_disk(&app).contains("Call the plumber"));

    keys(&mut app, &[Key::Char('u')]);

    assert_eq!(on_disk(&app), SAMPLE, "undo restores the exact file");
}

#[test]
fn undo_reverts_a_priority_change() {
    let (_s, mut app) = app_with("undo-pri", SAMPLE);
    select_containing(&mut app, "Buy groceries");

    keys(&mut app, &[Key::Char('+')]);
    assert!(on_disk(&app).contains("(A) Buy groceries"));

    keys(&mut app, &[Key::Char('u')]);
    assert_eq!(on_disk(&app), SAMPLE);
}

#[test]
fn undo_with_nothing_to_undo_says_so() {
    let (_s, mut app) = app_with("undo-empty", SAMPLE);

    keys(&mut app, &[Key::Char('u')]);

    assert_eq!(on_disk(&app), SAMPLE);
    assert!(app.message.as_ref().unwrap().text.contains("Nothing to undo"));
}

// ------------------------------------------------------------------ archiving

#[test]
fn archiving_moves_completed_tasks_to_done_txt() {
    let (_s, mut app) = app_with("archive", SAMPLE);
    let archive = app.store.archive_path();

    keys(&mut app, &[Key::Char('X'), Key::Char('y')]);

    let disk = on_disk(&app);
    assert!(!disk.contains("Renew passport"), "{disk}");
    assert_eq!(disk.lines().count(), 4);

    let archived = fs::read_to_string(&archive).expect("done.txt exists");
    assert!(archived.contains("x (C) Renew passport +admin @computer"));
}

#[test]
fn archiving_with_nothing_completed_is_a_no_op() {
    let (_s, mut app) = app_with("archive-none", "(A) open one\n(B) open two\n");

    keys(&mut app, &[Key::Char('X'), Key::Char('y')]);

    assert_eq!(on_disk(&app), "(A) open one\n(B) open two\n");
    assert!(!app.store.archive_path().exists());
}

// ------------------------------------------------------------------- ordering

#[test]
fn sorting_the_view_does_not_reorder_the_file() {
    let (_s, mut app) = app_with("sort-view", SAMPLE);

    keys(&mut app, &[Key::Char('s')]);
    assert_eq!(app.sort_mode, SortMode::Priority);
    // The top row is now the (A) task even though it is line 2 in the file.
    assert!(app.rows()[0].file_index == 1);
    assert_eq!(on_disk(&app), SAMPLE);
}

#[test]
fn sorting_the_file_rewrites_it_in_order() {
    let (_s, mut app) = app_with("sort-file", SAMPLE);

    keys(&mut app, &[Key::Char('S')]);

    let disk = on_disk(&app);
    let lines: Vec<&str> = disk.lines().collect();
    assert!(lines[0].starts_with("(A)"), "{lines:?}");
    assert!(lines[1].starts_with("(B)"), "{lines:?}");
    assert!(lines.last().unwrap().starts_with("x "), "{lines:?}");
}

#[test]
fn moving_a_task_changes_its_line_number() {
    let (_s, mut app) = app_with("move", "one\ntwo\nthree\n");
    select_containing(&mut app, "three");

    keys(&mut app, &[Key::CtrlChar('k')]);

    assert_eq!(on_disk(&app), "one\nthree\ntwo\n");
    // The cursor followed the task.
    assert!(app.selected_task().unwrap().render() == "three");
}

// ------------------------------------------------------------------ reloading

#[test]
fn reload_picks_up_an_external_edit() {
    let (_s, mut app) = app_with("reload", SAMPLE);
    assert_eq!(app.store.len(), 5);

    fs::write(app.store.path(), "(A) written by another tool +ext @cli\n").unwrap();
    keys(&mut app, &[Key::Char('r')]);

    assert_eq!(app.store.len(), 1);
    assert_eq!(app.visible_count(), 1);
    assert!(app
        .selected_task()
        .unwrap()
        .render()
        .contains("written by another tool"));
}

// ------------------------------------------------------------- modes and help

#[test]
fn help_opens_and_closes_without_quitting() {
    let (_s, mut app) = app_with("help", SAMPLE);

    keys(&mut app, &[Key::Char('?')]);
    assert_eq!(app.mode, Mode::Help);
    assert!(!app.should_quit);

    keys(&mut app, &[Key::Down, Key::Down]);
    assert_eq!(app.help_scroll, 2);

    keys(&mut app, &[Key::Esc]);
    assert_eq!(app.mode, Mode::Normal);
    assert!(!app.should_quit);
    assert_eq!(app.help_scroll, 0);
}

#[test]
fn q_quits_and_esc_clears_filters_first() {
    let (_s, mut app) = app_with("quit", SAMPLE);

    app.set_project_filter(Some("finance".into()));
    keys(&mut app, &[Key::Esc]);
    assert!(!app.should_quit, "Esc clears the filter instead of quitting");
    assert!(app.project_filter.is_none());

    keys(&mut app, &[Key::Esc]);
    assert!(app.should_quit, "a second Esc with no filter quits");

    let (_s2, mut app2) = app_with("quit2", SAMPLE);
    keys(&mut app2, &[Key::Char('q')]);
    assert!(app2.should_quit);
}

#[test]
fn ctrl_c_quits_from_every_mode() {
    for entry in [Key::Char('a'), Key::Char('?'), Key::Char('/')] {
        let (_s, mut app) = app_with("ctrlc", SAMPLE);
        keys(&mut app, &[entry, Key::CtrlChar('c')]);
        assert!(app.should_quit, "Ctrl-C did not quit after {entry:?}");
    }
}

#[test]
fn tab_cycles_the_three_panels() {
    let (_s, mut app) = app_with("tab", SAMPLE);
    assert_eq!(app.focus, Focus::Tasks);
    keys(&mut app, &[Key::Tab]);
    assert_eq!(app.focus, Focus::Projects);
    keys(&mut app, &[Key::Tab]);
    assert_eq!(app.focus, Focus::Contexts);
    keys(&mut app, &[Key::Tab]);
    assert_eq!(app.focus, Focus::Tasks);
    keys(&mut app, &[Key::BackTab]);
    assert_eq!(app.focus, Focus::Contexts);
    // Number keys jump straight to a panel.
    keys(&mut app, &[Key::Char('1')]);
    assert_eq!(app.focus, Focus::Tasks);
}

#[test]
fn navigation_stays_inside_the_list() {
    let (_s, mut app) = app_with("nav", SAMPLE);

    keys(&mut app, &[Key::Up, Key::Up]);
    assert_eq!(app.selected, 0, "cannot move above the first row");

    keys(&mut app, &[Key::End]);
    assert_eq!(app.selected, 4);
    keys(&mut app, &[Key::Down]);
    assert_eq!(app.selected, 4, "cannot move past the last row");

    keys(&mut app, &[Key::Home]);
    assert_eq!(app.selected, 0);
}

#[test]
fn an_empty_list_absorbs_every_action_without_panicking() {
    let (_s, mut app) = app_with("empty", "");

    keys(
        &mut app,
        &[
            Key::Down,
            Key::Up,
            Key::Char(' '),
            Key::Char('+'),
            Key::Char('-'),
            Key::Char('p'),
            Key::Char('c'),
            Key::Char('P'),
            Key::Char('d'),
            Key::Char('D'),
            Key::Char('e'),
            Key::Char('t'),
            Key::Char('T'),
            Key::Char('u'),
            Key::Char('S'),
            Key::CtrlChar('j'),
            Key::CtrlChar('k'),
            Key::Enter,
        ],
    );

    assert_eq!(on_disk(&app), "");
    assert!(!app.should_quit);
}

// ------------------------------------------------------- authenticity overall

#[test]
fn the_view_always_matches_the_file_after_a_long_session() {
    let (_s, mut app) = app_with("authentic", SAMPLE);

    // A representative mix of every mutating operation.
    select_containing(&mut app, "Buy groceries");
    keys(&mut app, &[Key::Char('+')]);
    keys(&mut app, &[Key::Char('c'), Key::CtrlChar('u')]);
    type_text(&mut app, "town");
    keys(&mut app, &[Key::Enter]);

    select_containing(&mut app, "Read ratatui docs");
    keys(&mut app, &[Key::Char(' ')]);

    keys(&mut app, &[Key::Char('a')]);
    type_text(&mut app, "Book a dentist appointment");
    keys(&mut app, &[Key::Tab]);
    type_text(&mut app, "B");
    keys(&mut app, &[Key::Tab]);
    type_text(&mut app, "health");
    keys(&mut app, &[Key::Tab]);
    type_text(&mut app, "phone");
    keys(&mut app, &[Key::Enter]);

    // What the app holds in memory is exactly what a fresh read returns.
    assert_eq!(on_disk(&app), app.store.serialize());

    let reloaded = tooll::store::TaskStore::load(app.store.path()).unwrap();
    let a: Vec<String> = app.store.tasks().iter().map(|t| t.render()).collect();
    let b: Vec<String> = reloaded.tasks().iter().map(|t| t.render()).collect();
    assert_eq!(a, b);

    let disk = on_disk(&app);
    assert!(disk.contains("(A) Buy groceries +errands @town due:2026-03-15"), "{disk}");
    assert!(disk.contains("(B) Book a dentist appointment +health @phone"), "{disk}");
    assert!(disk.lines().filter(|l| l.starts_with("x ")).count() == 2, "{disk}");
}

#[test]
fn arbitrary_tag_names_are_supported_not_hard_coded() {
    // "Task-driven": any valid tag must work, including unusual ones.
    let (_s, mut app) = app_with("arbitrary", "Do the thing\n");

    for (key, sigil, name) in [
        (Key::Char('P'), '+', "Q3_roadmap.v2"),
        (Key::Char('c'), '@', "café"),
    ] {
        keys(&mut app, &[key, Key::CtrlChar('u')]);
        type_text(&mut app, name);
        keys(&mut app, &[Key::Enter]);
        let disk = on_disk(&app);
        assert!(disk.contains(&format!("{sigil}{name}")), "{disk}");
    }

    keys(&mut app, &[Key::Char('p'), Key::CtrlChar('u')]);
    type_text(&mut app, "Z");
    keys(&mut app, &[Key::Enter]);
    assert_eq!(
        on_disk(&app),
        "(Z) Do the thing +Q3_roadmap.v2 @café\n"
    );
}

#[test]
fn unusual_but_legal_files_survive_a_round_trip() {
    let odd = "\
x 2026-01-02 2025-12-30 Already done with both dates
2026-02-01 Created but not done
(A) key:value pairs and a due:2026-05-05 date
no metadata at all
x
(a) lowercase parens are just text
";
    let (_s, mut app) = app_with("odd", odd);
    assert_eq!(app.store.len(), 6);

    // Touch an unrelated task; every other line must come back unchanged.
    select_containing(&mut app, "no metadata at all");
    keys(&mut app, &[Key::Char('+')]);

    let disk = on_disk(&app);
    for line in odd.lines() {
        if line == "no metadata at all" {
            continue;
        }
        assert!(disk.contains(line), "lost line {line:?} from\n{disk}");
    }
    assert!(disk.contains("(A) no metadata at all"));
}
