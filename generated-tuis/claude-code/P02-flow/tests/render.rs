//! Rendering tests.
//!
//! These exist to prove the interface constraints that matter most, by drawing real frames into a
//! `TestBackend` buffer and asserting on what a single screenshot would contain:
//!
//! * every column is visible at once, together with the selected card's full title and complete
//!   body — no pagination, no tab switching, no overlay hiding any of it;
//! * prompts, messages and help shrink the panes instead of covering them;
//! * key hints are always on screen, so the bindings are discoverable inside the tool;
//! * degenerate sizes and card counts do not panic or blank the panes.

use ratatui::backend::TestBackend;
use ratatui::Terminal;

use toolb::app::{App, Focus};
use toolb::config::{Config, RootSource};
use toolb::keymap::Action;
use toolb::store::Store;
use toolb::ui;

/// Render one frame and return the buffer as text, one string per row.
///
/// A double-width glyph occupies two cells in the backend buffer: the glyph itself, then an empty
/// placeholder. Concatenating cells naively would turn "English-only text" into "English-only text English-only text ", so the placeholder
/// immediately following a wide glyph is dropped. Real terminals do the same thing visually.
fn render_rows(app: &mut App, width: u16, height: u16) -> Vec<String> {
    use unicode_width::UnicodeWidthStr;

    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();

    (0..buffer.area.height)
        .map(|y| {
            let mut row = String::new();
            let mut skip_next_blank = false;
            for x in 0..buffer.area.width {
                let symbol = buffer[(x, y)].symbol();
                if skip_next_blank && symbol == " " {
                    skip_next_blank = false;
                    continue;
                }
                skip_next_blank = symbol.width() > 1;
                row.push_str(symbol);
            }
            row
        })
        .collect()
}

/// Render one frame and return the whole screen as a single string.
fn render_screen(app: &mut App, width: u16, height: u16) -> String {
    render_rows(app, width, height).join("\n")
}

fn make_app(root: &std::path::Path) -> App {
    let store = Store::new(root);
    let config = Config::load(root.join("toolb.conf")).unwrap();
    App::new(store, config, RootSource::CommandLine)
}

/// A board with three columns and a card carrying a multi-line body.
fn standard_board() -> (tempfile::TempDir, App) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.init_board().unwrap();

    store.create_card("todo", "item-1", "Fix login bug").unwrap();
    store.append_card_line("todo", "item-1", "Investigate timeout on mobile clients.").unwrap();
    store.append_card_line("todo", "item-1", "Reproduced on Android 14.").unwrap();
    store.append_card_line("todo", "item-1", "Owner: platform team.").unwrap();
    store.create_card("todo", "item-2", "Write release notes").unwrap();
    store.create_card("doing", "item-3", "Refactor the parser").unwrap();
    store.create_card("done", "item-4", "Ship the installer").unwrap();

    let app = make_app(dir.path());
    (dir, app)
}

#[test]
fn one_screen_shows_every_column_and_the_selected_card_in_full() {
    let (_dir, mut app) = standard_board();
    let screen = render_screen(&mut app, 120, 40);

    // Every column display name, on the same screen.
    for name in ["TO DO", "DOING", "DONE"] {
        assert!(screen.contains(name), "column {name:?} missing from:\n{screen}");
    }
    // Every column id, so the disk layout is discoverable too.
    for id in ["todo", "doing", "done"] {
        assert!(screen.contains(id), "column id {id:?} missing from:\n{screen}");
    }
    // Cards from every column, not just the selected one.
    for card in ["item-1", "item-2", "item-3", "item-4"] {
        assert!(screen.contains(card), "card {card:?} missing from:\n{screen}");
    }
    // The selected card's full title and every line of its body, simultaneously.
    assert!(screen.contains("Fix login bug"));
    for line in [
        "Investigate timeout on mobile clients.",
        "Reproduced on Android 14.",
        "Owner: platform team.",
    ] {
        assert!(screen.contains(line), "body line {line:?} missing from:\n{screen}");
    }
}

#[test]
fn card_detail_appears_without_pressing_enter() {
    // The specification requires a selected card to show its title and body; requiring a
    // keystroke first would make the detail a second view rather than the same screen.
    let (_dir, mut app) = standard_board();
    assert_eq!(app.focus, Focus::Board, "no explicit focus action yet");
    let screen = render_screen(&mut app, 120, 40);
    assert!(screen.contains("Fix login bug"));
    assert!(screen.contains("Investigate timeout on mobile clients."));
}

#[test]
fn detail_follows_the_selection_immediately() {
    let (_dir, mut app) = standard_board();
    app.dispatch(Action::CardDown);
    let screen = render_screen(&mut app, 120, 40);
    assert!(screen.contains("Write release notes"), "detail must track the selection:\n{screen}");

    app.dispatch(Action::ColumnRight);
    let screen = render_screen(&mut app, 120, 40);
    assert!(screen.contains("Refactor the parser"));
}

#[test]
fn the_card_file_path_is_shown_so_the_data_is_traceable() {
    let (dir, mut app) = standard_board();
    let rows = render_rows(&mut app, 120, 40);
    let screen = rows.join("\n");

    // The path is long and wraps onto continuation rows, and it can break at any character -
    // including inside "item-1.md". So reassemble the screen with borders and whitespace removed
    // before looking for it: what matters is that the whole path is on screen, not how it is split.
    // How much of the path wraps depends on the temporary directory's length, which varies by
    // platform, so this must not assume any particular split.
    let squashed = |text: &str| -> String {
        text.replace('│', "").split_whitespace().collect::<Vec<_>>().concat()
    };
    let dewrapped = squashed(&rows.concat());

    // Proof the pane reflects a real file on disk rather than an internal model.
    assert!(dewrapped.contains("item-1.md"), "card file name missing from:\n{screen}");

    let expected = squashed(&dir.path().join("cols/todo/item-1.md").display().to_string());
    assert!(
        dewrapped.contains(&expected),
        "the full card path should be visible (wrapped is fine).\nexpected: {expected}\nscreen:\n{screen}"
    );
}

#[test]
fn key_hints_are_always_visible() {
    let (_dir, mut app) = standard_board();
    let screen = render_screen(&mut app, 120, 40);
    // A user must be able to discover the bindings without external documentation.
    for hint in ["new", "help", "quit", "move"] {
        assert!(screen.contains(hint), "hint {hint:?} missing from:\n{screen}");
    }
}

#[test]
fn help_does_not_hide_the_board_or_the_selected_card() {
    let (_dir, mut app) = standard_board();
    app.dispatch(Action::ToggleHelp);
    let screen = render_screen(&mut app, 120, 40);

    // Help is open...
    assert!(screen.contains("HELP"), "help pane missing from:\n{screen}");
    assert!(screen.contains("Navigate"), "help groups missing from:\n{screen}");
    // ...and the board and card are still on the same screen.
    for name in ["TO DO", "DOING", "DONE"] {
        assert!(screen.contains(name), "help hid column {name:?}:\n{screen}");
    }
    assert!(screen.contains("Fix login bug"), "help hid the selected card:\n{screen}");
    assert!(screen.contains("item-3"), "help hid other columns' cards:\n{screen}");
}

#[test]
fn help_documents_the_required_navigation_keys() {
    let (_dir, mut app) = standard_board();
    app.dispatch(Action::ToggleHelp);
    let screen = render_screen(&mut app, 120, 44);
    // Arrow keys, Enter, Esc and Tab are called out in the specification.
    for key in ["←", "↑", "Enter", "Tab"] {
        assert!(screen.contains(key), "help omits {key:?}:\n{screen}");
    }
}

#[test]
fn prompts_shrink_the_panes_instead_of_covering_them() {
    let (_dir, mut app) = standard_board();
    app.dispatch(Action::NewCard);
    let screen = render_screen(&mut app, 120, 40);

    // The prompt is visible...
    assert!(screen.contains("New card title"), "prompt missing from:\n{screen}");
    // ...and so is everything it might otherwise have covered.
    for name in ["TO DO", "DOING", "DONE"] {
        assert!(screen.contains(name), "prompt hid column {name:?}:\n{screen}");
    }
    assert!(screen.contains("Fix login bug"), "prompt hid the card title:\n{screen}");
    assert!(
        screen.contains("Investigate timeout on mobile clients."),
        "prompt hid the card body:\n{screen}"
    );
}

#[test]
fn prompt_hints_replace_the_normal_ones_so_the_mode_is_discoverable() {
    let (_dir, mut app) = standard_board();
    app.dispatch(Action::NewCard);
    let screen = render_screen(&mut app, 120, 40);
    assert!(screen.contains("confirm"), "prompt hints missing from:\n{screen}");
    assert!(screen.contains("cancel"), "prompt hints missing from:\n{screen}");
}

#[test]
fn the_move_target_list_shows_every_candidate_column() {
    let (_dir, mut app) = standard_board();
    app.dispatch(Action::MoveCardPrompt);
    let screen = render_screen(&mut app, 120, 40);

    assert!(screen.contains("Move to column"), "prompt missing from:\n{screen}");
    // Candidates are listed in a pane, not a popup, so all options are visible at once.
    for name in ["TO DO", "DOING", "DONE"] {
        assert!(screen.contains(name), "candidate {name:?} missing from:\n{screen}");
    }
    assert!(screen.contains("Fix login bug"), "the card being moved is still visible:\n{screen}");
}

#[test]
fn delete_confirmation_states_the_target_and_the_keys() {
    let (_dir, mut app) = standard_board();
    app.dispatch(Action::DeleteCard);
    let screen = render_screen(&mut app, 120, 40);
    assert!(screen.contains("item-1.md"), "confirmation must name the file:\n{screen}");
    assert!(screen.contains("[y/N]"), "confirmation must show the keys:\n{screen}");
    // The board stays visible while confirming.
    assert!(screen.contains("DOING"), "confirmation hid the board:\n{screen}");
}

#[test]
fn the_body_editor_keeps_the_board_visible() {
    let (_dir, mut app) = standard_board();
    app.dispatch(Action::EditBody);
    let screen = render_screen(&mut app, 120, 40);
    assert!(screen.contains("Ctrl+S"), "editor must document how to save:\n{screen}");
    assert!(screen.contains("TO DO") && screen.contains("DONE"), "editor hid the board:\n{screen}");
}

#[test]
fn the_sidebar_lists_every_column_with_its_id_and_count() {
    let (_dir, mut app) = standard_board();
    let rows = render_rows(&mut app, 120, 40);
    let screen = rows.join("\n");
    assert!(screen.contains("COLUMNS"), "sidebar missing from:\n{screen}");

    // Name, id and card count together for each column.
    let sidebar_rows: Vec<&String> =
        rows.iter().filter(|r| r.contains("[todo]") || r.contains("[doing]") || r.contains("[done]")).collect();
    assert_eq!(sidebar_rows.len(), 3, "expected one row per column, got: {sidebar_rows:?}");
    assert!(sidebar_rows[0].contains("TO DO"), "got: {}", sidebar_rows[0]);
    assert!(sidebar_rows[0].contains('2'), "TO DO holds two cards: {}", sidebar_rows[0]);
}

#[test]
fn the_configuration_is_visible_on_the_main_screen() {
    let (dir, mut app) = standard_board();
    let screen = render_screen(&mut app, 120, 40);
    // Board root, where it came from, and the config file path.
    assert!(screen.contains("root"), "board root label missing:\n{screen}");
    assert!(
        screen.contains(dir.path().file_name().unwrap().to_str().unwrap()),
        "board root path missing:\n{screen}"
    );
    assert!(screen.contains("command line"), "root source missing:\n{screen}");
    assert!(screen.contains("toolb.conf"), "config path missing:\n{screen}");
}

#[test]
fn configuration_keys_are_listed_once_set() {
    let (_dir, mut app) = standard_board();
    app.config.set_and_save("board_root", "/srv/board").unwrap();
    app.config.set_and_save("theme", "dark").unwrap();
    let screen = render_screen(&mut app, 120, 40);
    assert!(screen.contains("theme"), "config keys must be visible:\n{screen}");
    assert!(screen.contains("dark"), "config values must be visible:\n{screen}");
}

#[test]
fn long_bodies_scroll_without_ever_blanking_the_pane() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.init_board().unwrap();
    store.create_card("todo", "long", "Long card").unwrap();
    for n in 1..=60 {
        store.append_card_line("todo", "long", &format!("body line {n}")).unwrap();
    }
    let mut app = make_app(dir.path());

    // The top of the body is visible initially.
    let screen = render_screen(&mut app, 120, 30);
    assert!(screen.contains("body line 1"), "start of the body missing:\n{screen}");

    // Scroll far past the end: the pane must still show content, never blank out.
    app.dispatch(Action::FocusDetail);
    for _ in 0..200 {
        app.dispatch(Action::CardDown);
    }
    let screen = render_screen(&mut app, 120, 30);
    assert!(screen.contains("body line 60"), "end of the body unreachable:\n{screen}");
    assert!(
        screen.contains("Long card"),
        "the title must stay visible while the body scrolls:\n{screen}"
    );

    // Scrolling back returns to the top.
    for _ in 0..300 {
        app.dispatch(Action::CardUp);
    }
    let screen = render_screen(&mut app, 120, 30);
    assert!(screen.contains("body line 1"), "cannot scroll back to the top:\n{screen}");
}

#[test]
fn a_board_with_many_columns_still_reveals_all_of_them() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    let names: Vec<String> = (1..=12).map(|n| format!("STAGE{n:02}")).collect();
    for (index, name) in names.iter().enumerate() {
        store.create_column(&format!("stage{index}"), name).unwrap();
    }
    let mut app = make_app(dir.path());

    let screen = render_screen(&mut app, 120, 40);
    // The sidebar lists every column even when the board pane cannot show them all.
    for name in &names {
        assert!(screen.contains(name.as_str()), "column {name} is not visible anywhere:\n{screen}");
    }
    // And the user is told the board pane is windowed rather than being silently truncated.
    assert!(screen.contains("column(s)"), "overflow must be reported:\n{screen}");
}

#[test]
fn a_single_column_board_renders_sensibly() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.create_column("only", "ONLY COLUMN").unwrap();
    store.create_card("only", "c1", "Just one").unwrap();
    let mut app = make_app(dir.path());

    let screen = render_screen(&mut app, 120, 40);
    assert!(screen.contains("ONLY COLUMN"));
    assert!(screen.contains("Just one"));
}

#[test]
fn many_cards_in_one_column_scroll_and_report_the_remainder() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.init_board().unwrap();
    for n in 1..=40 {
        store.create_card("todo", &format!("card-{n:02}"), &format!("Task number {n}")).unwrap();
    }
    let mut app = make_app(dir.path());

    let screen = render_screen(&mut app, 120, 20);
    assert!(screen.contains("card-01"), "first card missing:\n{screen}");
    assert!(screen.contains("more"), "hidden cards must be reported:\n{screen}");

    // Jumping to the last card scrolls it into view.
    app.dispatch(Action::CardLast);
    let screen = render_screen(&mut app, 120, 20);
    assert!(screen.contains("card-40"), "last card unreachable:\n{screen}");
    assert!(screen.contains("Task number 40"), "its detail must show too:\n{screen}");
}

#[test]
fn an_empty_board_explains_what_to_do() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = make_app(dir.path());
    let screen = render_screen(&mut app, 100, 30);
    assert!(screen.contains("board.txt"), "must mention the missing file:\n{screen}");
    // The remedy is discoverable from the screen itself.
    assert!(screen.contains('I') && screen.contains('c'), "must offer the keys:\n{screen}");
}

#[test]
fn an_empty_column_is_labelled_rather_than_looking_broken() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.init_board().unwrap();
    let mut app = make_app(dir.path());
    let screen = render_screen(&mut app, 120, 30);
    assert!(screen.contains("(empty)"), "empty columns must say so:\n{screen}");
}

#[test]
fn narrow_terminals_stack_the_panes_and_keep_the_card_readable() {
    let (_dir, mut app) = standard_board();
    let screen = render_screen(&mut app, 60, 30);
    // Board and card content both present, just arranged vertically.
    assert!(screen.contains("TO DO"), "board missing at narrow width:\n{screen}");
    assert!(screen.contains("Fix login bug"), "card missing at narrow width:\n{screen}");
    assert!(screen.contains("item-1"), "card id missing at narrow width:\n{screen}");
}

#[test]
fn tiny_terminals_report_the_problem_instead_of_rendering_garbage() {
    let (_dir, mut app) = standard_board();
    let screen = render_screen(&mut app, 20, 5);
    assert!(screen.contains("too small"), "expected a clear notice, got:\n{screen}");
}

#[test]
fn rendering_at_many_sizes_never_panics() {
    let (_dir, mut app) = standard_board();
    // Includes sizes around every layout threshold, and degenerate ones.
    for (width, height) in [
        (1, 1), (2, 2), (10, 4), (20, 6), (39, 9), (40, 10), (41, 11),
        (60, 12), (75, 20), (76, 20), (77, 20), (100, 30), (120, 40), (200, 60), (400, 100),
    ] {
        let _ = render_screen(&mut app, width, height);
    }
}

#[test]
fn rendering_every_mode_at_a_small_size_never_panics() {
    let (_dir, mut app) = standard_board();
    for action in [
        Action::NewCard, Action::EditTitle, Action::AppendBody, Action::EditBody,
        Action::DeleteCard, Action::MoveCardPrompt, Action::Search, Action::NewColumn,
        Action::RenameColumn, Action::GotoColumn, Action::SetBoardRoot, Action::EditConfig,
        Action::ToggleHelp,
    ] {
        app.dispatch(action);
        // A cramped-but-usable size exercises the layout arithmetic hardest.
        let _ = render_screen(&mut app, 40, 10);
        let _ = render_screen(&mut app, 120, 40);
    }
}

#[test]
fn unicode_content_renders_without_panicking_and_stays_visible() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.create_column("cjk", "English-only text").unwrap();
    store.create_card("cjk", "card-1", "English-only text").unwrap();
    store.append_card_line("cjk", "card-1", "English-only text，English-only text。").unwrap();
    store.create_column("emoji", "🚀 Ship").unwrap();
    store.create_card("emoji", "card-2", "Release 👨‍👩‍👧 family").unwrap();

    let mut app = make_app(dir.path());
    let screen = render_screen(&mut app, 120, 30);
    assert!(screen.contains("English-only text"), "wide-glyph column name missing:\n{screen}");
    assert!(screen.contains("English-only text"), "wide-glyph title missing:\n{screen}");
    // The other column is still listed.
    assert!(screen.contains("Ship"), "second column missing:\n{screen}");
}

#[test]
fn a_search_filter_reports_itself_and_the_match_count() {
    let (_dir, mut app) = standard_board();
    app.search = "release".to_string();
    let screen = render_screen(&mut app, 120, 40);
    assert!(screen.contains("filter"), "the active filter must be visible:\n{screen}");
    assert!(screen.contains("release"), "the filter text must be visible:\n{screen}");
    // Columns remain listed even when they hold no matches, so the board shape is preserved.
    assert!(screen.contains("DOING"), "columns must stay visible while filtering:\n{screen}");
}

#[test]
fn operation_feedback_is_shown_after_a_change() {
    let (_dir, mut app) = standard_board();
    app.dispatch(Action::MoveCardRight);
    let screen = render_screen(&mut app, 120, 40);
    // Names both ends of the move, so the result is unambiguous.
    assert!(screen.contains("Moved"), "missing confirmation:\n{screen}");
    assert!(screen.contains("TO DO") && screen.contains("DOING"), "got:\n{screen}");
}

#[test]
fn errors_are_shown_on_screen_rather_than_swallowed() {
    let (_dir, mut app) = standard_board();
    app.dispatch(Action::GotoColumn);
    // Confirm a name that does not exist.
    for ch in "NOSUCH".chars() {
        app.handle_key(ratatui::crossterm::event::KeyEvent::new(
            ratatui::crossterm::event::KeyCode::Char(ch),
            ratatui::crossterm::event::KeyModifiers::NONE,
        ));
    }
    app.handle_key(ratatui::crossterm::event::KeyEvent::new(
        ratatui::crossterm::event::KeyCode::Enter,
        ratatui::crossterm::event::KeyModifiers::NONE,
    ));
    let screen = render_screen(&mut app, 120, 40);
    assert!(screen.contains("No column named"), "error not surfaced:\n{screen}");
}

#[test]
fn a_headerless_card_file_is_flagged_in_the_detail_pane() {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    store.init_board().unwrap();
    // A file with no "# title" line, as a hand-created card might be.
    std::fs::write(dir.path().join("cols/todo/raw.md"), "just some notes\nand more\n").unwrap();
    let mut app = make_app(dir.path());

    let screen = render_screen(&mut app, 120, 30);
    assert!(screen.contains("just some notes"), "content must still be shown:\n{screen}");
    assert!(screen.contains("no '# ' title line"), "the anomaly must be flagged:\n{screen}");
}

#[test]
fn load_warnings_are_visible_on_the_main_screen() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("board.txt"),
        "col a \"Alpha\"\ncol a \"Duplicate\"\n",
    )
    .unwrap();
    let mut app = make_app(dir.path());
    let screen = render_screen(&mut app, 120, 30);
    assert!(screen.contains("more than once"), "warning not surfaced:\n{screen}");
}
