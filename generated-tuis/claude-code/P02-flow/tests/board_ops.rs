//! On-disk format tests.
//!
//! The board files are the contract this tool has to honour, so these tests assert on exact bytes
//! rather than on parsed structures. They cover the documented layout, the awkward inputs a
//! hand-edited or externally generated board can contain, and the states an interrupted write
//! would leave behind.

use std::path::Path;

use toolb::store::Store;

fn board() -> (tempfile::TempDir, Store) {
    let dir = tempfile::tempdir().unwrap();
    let store = Store::new(dir.path());
    (dir, store)
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

// -------------------------------------------------------------------------------------------
// The documented layout
// -------------------------------------------------------------------------------------------

#[test]
fn produces_exactly_the_layout_from_the_specification() {
    let (_dir, store) = board();

    // The example given in the specification: column id "todo", card id "item-1".
    store.create_column("todo", "TO DO").unwrap();
    store.create_card("todo", "item-1", "Fix login bug").unwrap();
    store.append_card_line("todo", "item-1", "Investigate timeout on mobile clients.").unwrap();

    let root = &store.root;
    assert_eq!(read(&root.join("board.txt")), "col todo \"TO DO\"\n");
    assert_eq!(read(&root.join("cols/todo/order.txt")), "item-1\n");
    assert_eq!(
        read(&root.join("cols/todo/item-1.md")),
        "# Fix login bug\nInvestigate timeout on mobile clients.\n"
    );
}

#[test]
fn a_board_written_by_hand_is_read_back_faithfully() {
    let (dir, store) = board();
    let root = dir.path();

    // Exactly what the specification describes, created without going through the tool.
    std::fs::create_dir_all(root.join("cols/todo")).unwrap();
    std::fs::create_dir_all(root.join("cols/done")).unwrap();
    std::fs::write(root.join("board.txt"), "col todo \"TO DO\"\ncol done \"DONE\"\n").unwrap();
    std::fs::write(root.join("cols/todo/order.txt"), "item-1\n").unwrap();
    std::fs::write(
        root.join("cols/todo/item-1.md"),
        "# Fix login bug\nInvestigate timeout on mobile clients.\n",
    )
    .unwrap();

    let loaded = store.load().unwrap();
    assert_eq!(loaded.columns.len(), 2);
    assert_eq!(loaded.columns[0].id, "todo");
    assert_eq!(loaded.columns[0].display_name, "TO DO");
    assert_eq!(loaded.columns[0].cards.len(), 1);

    let card = &loaded.columns[0].cards[0];
    assert_eq!(card.id, "item-1");
    assert_eq!(card.title, "Fix login bug");
    assert_eq!(card.body, "Investigate timeout on mobile clients.");
    assert!(loaded.warnings.is_empty(), "unexpected warnings: {:?}", loaded.warnings);
}

#[test]
fn column_directories_use_the_id_while_the_ui_uses_the_display_name() {
    let (_dir, store) = board();
    // A display name that shares nothing with its id, to prove the two are not conflated.
    store.create_column("c1", "Waiting on review").unwrap();
    store.create_card("c1", "task", "Something").unwrap();

    assert!(store.root.join("cols/c1/task.md").exists());
    assert!(!store.root.join("cols/Waiting on review").exists());

    let loaded = store.load().unwrap();
    assert_eq!(loaded.column_by_display_name("Waiting on review"), Some(0));
}

// -------------------------------------------------------------------------------------------
// Byte-exact edits
// -------------------------------------------------------------------------------------------

#[test]
fn editing_a_title_touches_only_line_one() {
    let (_dir, store) = board();
    store.create_column("c", "C").unwrap();
    let path = store.root.join("cols/c/card.md");
    std::fs::write(&path, "# Old title\nline one\n\nline three\n").unwrap();

    store.set_card_title("c", "card", "New title").unwrap();
    assert_eq!(read(&path), "# New title\nline one\n\nline three\n");
}

#[test]
fn appending_handles_every_trailing_newline_case() {
    let (_dir, store) = board();
    store.create_column("c", "C").unwrap();
    let path = store.root.join("cols/c/card.md");

    // No trailing newline: one must be added before the new line.
    std::fs::write(&path, "# T\nfirst").unwrap();
    store.append_card_line("c", "card", "second").unwrap();
    assert_eq!(read(&path), "# T\nfirst\nsecond\n");

    // Already terminated: no blank line is introduced.
    store.append_card_line("c", "card", "third").unwrap();
    assert_eq!(read(&path), "# T\nfirst\nsecond\nthird\n");

    // A deliberate blank line must survive untouched.
    std::fs::write(&path, "# T\nfirst\n\n").unwrap();
    store.append_card_line("c", "card", "after blank").unwrap();
    assert_eq!(read(&path), "# T\nfirst\n\nafter blank\n");

    // An empty file gains no leading blank line.
    std::fs::write(&path, "").unwrap();
    store.append_card_line("c", "card", "only").unwrap();
    assert_eq!(read(&path), "only\n");
}

#[test]
fn crlf_files_keep_their_line_endings_through_every_operation() {
    let (_dir, store) = board();
    store.create_column("c", "C").unwrap();
    let path = store.root.join("cols/c/card.md");
    std::fs::write(&path, "# Title\r\nbody\r\n").unwrap();

    store.append_card_line("c", "card", "appended").unwrap();
    assert_eq!(read(&path), "# Title\r\nbody\r\nappended\r\n");

    store.set_card_title("c", "card", "Renamed").unwrap();
    assert_eq!(read(&path), "# Renamed\r\nbody\r\nappended\r\n");

    // board.txt too.
    std::fs::write(store.root.join("board.txt"), "col c \"C\"\r\n").unwrap();
    store.rename_column("c", "Renamed column").unwrap();
    assert_eq!(read(&store.root.join("board.txt")), "col c \"Renamed column\"\r\n");
}

#[test]
fn board_txt_comments_and_formatting_survive_a_rewrite() {
    let (_dir, store) = board();
    let original = concat!(
        "# Board configuration\n",
        "\n",
        "col todo \"TO DO\"\n",
        "col weird \"quoted \\\"name\\\" here\"\n",
        "col plain unquoted name\n",
        "\n",
        "# end of file\n",
    );
    std::fs::create_dir_all(&store.root).unwrap();
    std::fs::write(store.root.join("board.txt"), original).unwrap();

    // Renaming one column must not reformat any other line.
    store.rename_column("todo", "BACKLOG").unwrap();
    assert_eq!(
        read(&store.root.join("board.txt")),
        concat!(
            "# Board configuration\n",
            "\n",
            "col todo \"BACKLOG\"\n",
            "col weird \"quoted \\\"name\\\" here\"\n",
            "col plain unquoted name\n",
            "\n",
            "# end of file\n",
        )
    );

    // The escaped and unquoted forms are still understood.
    let loaded = store.load().unwrap();
    assert_eq!(loaded.columns[1].display_name, "quoted \"name\" here");
    assert_eq!(loaded.columns[2].display_name, "unquoted name");
}

// -------------------------------------------------------------------------------------------
// Recovery from interrupted writes
// -------------------------------------------------------------------------------------------

#[test]
fn a_card_file_missing_from_order_txt_is_still_shown() {
    // The state an interrupted create or move leaves: file written, order not yet updated.
    let (_dir, store) = board();
    store.create_column("c", "C").unwrap();
    store.create_card("c", "known", "Known").unwrap();
    std::fs::write(store.root.join("cols/c/orphan.md"), "# Orphan card\nbody\n").unwrap();

    let loaded = store.load().unwrap();
    let ids: Vec<&str> = loaded.columns[0].cards.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec!["known", "orphan"], "the orphan is appended, never dropped");
    assert_eq!(loaded.columns[0].cards[1].title, "Orphan card");
}

#[test]
fn an_order_entry_with_no_file_is_ignored() {
    // The state an interrupted delete leaves: file removed, order not yet updated.
    let (_dir, store) = board();
    store.create_column("c", "C").unwrap();
    store.create_card("c", "real", "Real").unwrap();
    std::fs::write(store.root.join("cols/c/order.txt"), "ghost\nreal\n").unwrap();

    let loaded = store.load().unwrap();
    let ids: Vec<&str> = loaded.columns[0].cards.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec!["real"], "the phantom entry is hidden");
}

#[test]
fn loading_never_modifies_the_board() {
    let (_dir, store) = board();
    store.create_column("c", "C").unwrap();
    store.create_card("c", "a", "A").unwrap();
    // Leave the column in a state that reconciliation would want to repair.
    std::fs::write(store.root.join("cols/c/order.txt"), "ghost\n").unwrap();
    std::fs::write(store.root.join("cols/c/b.md"), "# B\n").unwrap();

    let order = store.root.join("cols/c/order.txt");
    let before_bytes = read(&order);
    let before_mtime = std::fs::metadata(&order).unwrap().modified().unwrap();

    // Several loads, to be sure none of them writes.
    for _ in 0..3 {
        let _ = store.load().unwrap();
    }

    assert_eq!(read(&order), before_bytes, "load rewrote order.txt");
    assert_eq!(
        std::fs::metadata(&order).unwrap().modified().unwrap(),
        before_mtime,
        "load touched order.txt"
    );
}

#[test]
fn the_first_write_persists_the_order_the_user_was_shown() {
    let (_dir, store) = board();
    store.create_column("c", "C").unwrap();
    for id in ["a", "b"] {
        std::fs::write(store.root.join(format!("cols/c/{id}.md")), format!("# {id}\n")).unwrap();
    }
    std::fs::write(store.root.join("cols/c/order.txt"), "b\nghost\n").unwrap();

    // Displayed: b (declared), then a (orphan).
    let loaded = store.load().unwrap();
    let displayed: Vec<&str> = loaded.columns[0].cards.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(displayed, vec!["b", "a"]);

    // Any mutation writes that same order, without the phantom.
    store.create_card("c", "c", "C card").unwrap();
    assert_eq!(read(&store.root.join("cols/c/order.txt")), "b\na\nc\n");
}

// -------------------------------------------------------------------------------------------
// Tolerating awkward input
// -------------------------------------------------------------------------------------------

#[test]
fn order_txt_oddities_are_tolerated() {
    let (_dir, store) = board();
    store.create_column("c", "C").unwrap();
    for id in ["a", "b", "d"] {
        std::fs::write(store.root.join(format!("cols/c/{id}.md")), format!("# {id}\n")).unwrap();
    }
    // Blank lines, comments, indentation, a stray extension, a duplicate, an unsafe entry.
    std::fs::write(
        store.root.join("cols/c/order.txt"),
        "\n  d  \n# a comment\nb.md\nb\n../escape\n",
    )
    .unwrap();

    let loaded = store.load().unwrap();
    let ids: Vec<&str> = loaded.columns[0].cards.iter().map(|c| c.id.as_str()).collect();
    // d and b in declared order (b deduplicated, .md stripped), then the orphan a.
    assert_eq!(ids, vec!["d", "b", "a"]);
}

#[test]
fn non_card_files_in_a_column_directory_are_ignored() {
    let (_dir, store) = board();
    store.create_column("c", "C").unwrap();
    store.create_card("c", "real", "Real").unwrap();

    std::fs::write(store.root.join("cols/c/notes.txt"), "not a card").unwrap();
    std::fs::write(store.root.join("cols/c/.hidden.md"), "# Hidden\n").unwrap();
    std::fs::write(store.root.join("cols/c/README"), "no extension").unwrap();
    std::fs::create_dir(store.root.join("cols/c/subdir.md")).unwrap();

    let loaded = store.load().unwrap();
    let ids: Vec<&str> = loaded.columns[0].cards.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, vec!["real"]);
}

#[test]
fn a_card_with_no_title_line_keeps_all_of_its_content() {
    let (_dir, store) = board();
    store.create_column("c", "C").unwrap();
    let path = store.root.join("cols/c/notes.md");
    std::fs::write(&path, "first line is not a heading\nsecond line\n").unwrap();

    let loaded = store.load().unwrap();
    let card = &loaded.columns[0].cards[0];
    assert_eq!(card.title, "");
    assert_eq!(card.body, "first line is not a heading\nsecond line");
    assert!(card.missing_header);

    // Setting a title prepends a heading rather than overwriting the first line.
    store.set_card_title("c", "notes", "Recovered").unwrap();
    assert_eq!(read(&path), "# Recovered\nfirst line is not a heading\nsecond line\n");
}

#[test]
fn heading_variants_all_parse() {
    let (_dir, store) = board();
    store.create_column("c", "C").unwrap();
    for (id, content, expected) in [
        ("one", "# Spaced\n", "Spaced"),
        ("two", "## Double\n", "Double"),
        ("three", "#Tight\n", "Tight"),
        ("four", "# Trailing spaces   \n", "Trailing spaces   "),
    ] {
        std::fs::write(store.root.join(format!("cols/c/{id}.md")), content).unwrap();
        let loaded = store.load().unwrap();
        let card = loaded.columns[0].cards.iter().find(|c| c.id == id).unwrap();
        assert_eq!(card.title, expected, "for {content:?}");
    }
}

#[test]
fn empty_and_whitespace_only_card_files_load_without_error() {
    let (_dir, store) = board();
    store.create_column("c", "C").unwrap();
    std::fs::write(store.root.join("cols/c/empty.md"), "").unwrap();
    std::fs::write(store.root.join("cols/c/blank.md"), "\n\n").unwrap();

    let loaded = store.load().unwrap();
    assert_eq!(loaded.columns[0].cards.len(), 2);
    assert!(loaded.columns[0].cards.iter().all(|c| c.title.is_empty()));
}

// -------------------------------------------------------------------------------------------
// Moves
// -------------------------------------------------------------------------------------------

#[test]
fn moving_a_card_updates_both_order_files_and_preserves_content() {
    let (_dir, store) = board();
    store.create_column("from", "From").unwrap();
    store.create_column("to", "To").unwrap();
    store.create_card("from", "moving", "Moving card").unwrap();
    store.append_card_line("from", "moving", "body line one").unwrap();
    store.append_card_line("from", "moving", "body line two").unwrap();
    store.create_card("from", "staying", "Staying").unwrap();
    store.create_card("to", "existing", "Existing").unwrap();

    let content_before = read(&store.root.join("cols/from/moving.md"));
    store.move_card("from", "moving", "to", None, None).unwrap();

    assert!(!store.root.join("cols/from/moving.md").exists());
    assert_eq!(read(&store.root.join("cols/to/moving.md")), content_before, "content unchanged");
    assert_eq!(read(&store.root.join("cols/from/order.txt")), "staying\n");
    assert_eq!(read(&store.root.join("cols/to/order.txt")), "existing\nmoving\n");
}

#[test]
fn a_colliding_move_changes_nothing_until_a_new_id_is_given() {
    let (_dir, store) = board();
    store.create_column("a", "A").unwrap();
    store.create_column("b", "B").unwrap();
    store.create_card("a", "dup", "From A").unwrap();
    store.create_card("b", "dup", "From B").unwrap();

    let error = store.move_card("a", "dup", "b", None, None).unwrap_err();
    assert!(error.contains("already exists"), "got: {error}");

    // Neither file was touched.
    assert_eq!(read(&store.root.join("cols/a/dup.md")), "# From A\n");
    assert_eq!(read(&store.root.join("cols/b/dup.md")), "# From B\n");
    assert_eq!(read(&store.root.join("cols/a/order.txt")), "dup\n");
    assert_eq!(read(&store.root.join("cols/b/order.txt")), "dup\n");

    // Retrying with a free id succeeds and leaves both cards intact.
    let final_id = store.move_card("a", "dup", "b", None, Some("dup-2")).unwrap();
    assert_eq!(final_id, "dup-2");
    assert_eq!(read(&store.root.join("cols/b/dup.md")), "# From B\n");
    assert_eq!(read(&store.root.join("cols/b/dup-2.md")), "# From A\n");
    assert_eq!(read(&store.root.join("cols/b/order.txt")), "dup\ndup-2\n");
    assert_eq!(read(&store.root.join("cols/a/order.txt")), "");
}

// -------------------------------------------------------------------------------------------
// Arbitrary input, no hard-coded vocabulary
// -------------------------------------------------------------------------------------------

#[test]
fn arbitrary_valid_ids_and_names_are_supported() {
    let (_dir, store) = board();

    // None of these resemble the defaults, and all must work.
    let columns = [
        ("backlog-2024", "Backlog (2024)"),
        ("UPPER_CASE", "Upper Case Column"),
        ("with.dots", "Dotted"),
        ("English-only text", "English-only text"),
        ("x", "X"),
    ];
    for (id, name) in columns {
        store.create_column(id, name).unwrap();
    }

    let cards = ["simple", "with-dashes", "under_scores", "MiXeD", "English-only text1", "a.b.c"];
    for card in cards {
        store.create_card("backlog-2024", card, &format!("Title of {card}")).unwrap();
    }

    let loaded = store.load().unwrap();
    assert_eq!(loaded.columns.len(), columns.len());
    for (index, (id, name)) in columns.iter().enumerate() {
        assert_eq!(&loaded.columns[index].id, id);
        assert_eq!(&loaded.columns[index].display_name, name);
        assert_eq!(loaded.column_by_display_name(name), Some(index));
    }
    let ids: Vec<&str> = loaded.columns[0].cards.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(ids, cards);
}

#[test]
fn path_traversal_attempts_are_refused() {
    let (_dir, store) = board();
    store.create_column("c", "C").unwrap();

    for bad in ["../escape", "a/b", "..", ".", "", "a\0b", ".hidden"] {
        assert!(store.create_card("c", bad, "T").is_err(), "accepted card id {bad:?}");
        assert!(store.create_column(bad, "Name").is_err(), "accepted column id {bad:?}");
    }
    // Nothing was created outside the column directory.
    assert!(!store.root.parent().unwrap().join("escape.md").exists());
}

#[test]
fn titles_and_names_containing_quotes_round_trip() {
    let (_dir, store) = board();
    // Quotes and backslashes are the characters board.txt has to escape.
    store.create_column("q", "He said \"hello\"").unwrap();
    store.create_card("q", "card", "Title with \"quotes\" and \\backslash").unwrap();

    let loaded = store.load().unwrap();
    assert_eq!(loaded.columns[0].display_name, "He said \"hello\"");
    assert_eq!(loaded.columns[0].cards[0].title, "Title with \"quotes\" and \\backslash");
}

#[test]
fn multi_line_bodies_of_any_length_survive_a_round_trip() {
    let (_dir, store) = board();
    store.create_column("c", "C").unwrap();
    store.create_card("c", "long", "Long card").unwrap();

    for n in 1..=50 {
        store.append_card_line("c", "long", &format!("line {n}")).unwrap();
    }
    let loaded = store.load().unwrap();
    let card = &loaded.columns[0].cards[0];
    let lines: Vec<&str> = card.body.split('\n').collect();
    assert_eq!(lines.len(), 50);
    assert_eq!(lines[0], "line 1");
    assert_eq!(lines[49], "line 50");
}

// -------------------------------------------------------------------------------------------
// Working against a fresh or unusual root
// -------------------------------------------------------------------------------------------

#[test]
fn operations_succeed_against_a_root_that_does_not_exist_yet() {
    let dir = tempfile::tempdir().unwrap();
    // Several levels deep, none of which exist.
    let store = Store::new(dir.path().join("a/b/c/board"));
    assert!(!store.is_initialised());

    store.create_column("todo", "TO DO").unwrap();
    store.create_card("todo", "first", "First card").unwrap();

    assert!(store.is_initialised());
    assert_eq!(read(&store.root.join("board.txt")), "col todo \"TO DO\"\n");
    assert_eq!(read(&store.root.join("cols/todo/first.md")), "# First card\n");
}

#[test]
fn init_creates_the_default_board_and_is_repeatable() {
    let (_dir, store) = board();
    store.init_board().unwrap();
    let after_first = read(&store.root.join("board.txt"));

    store.init_board().unwrap();
    assert_eq!(read(&store.root.join("board.txt")), after_first, "init must not duplicate columns");
    assert_eq!(store.load().unwrap().columns.len(), 3);
}

#[test]
fn a_file_where_the_root_should_be_is_reported_clearly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("not-a-directory");
    std::fs::write(&path, "contents").unwrap();

    let error = Store::new(&path).load().unwrap_err();
    assert!(error.contains("not a directory"), "got: {error}");
}

#[test]
fn a_missing_column_directory_is_reported_but_does_not_stop_the_load() {
    let (_dir, store) = board();
    std::fs::create_dir_all(&store.root).unwrap();
    std::fs::write(store.root.join("board.txt"), "col ghost \"GHOST\"\n").unwrap();

    let loaded = store.load().unwrap();
    assert_eq!(loaded.columns.len(), 1, "the column is still listed");
    assert!(loaded.columns[0].cards.is_empty());
    assert!(
        loaded.warnings.iter().any(|w| w.contains("no directory")),
        "warnings: {:?}",
        loaded.warnings
    );

    // And it becomes usable as soon as a card is added.
    store.create_card("ghost", "first", "First").unwrap();
    assert!(store.root.join("cols/ghost/first.md").exists());
}

#[test]
fn no_temporary_files_are_left_behind_by_any_operation() {
    let (_dir, store) = board();
    store.init_board().unwrap();
    store.create_card("todo", "a", "A").unwrap();
    store.append_card_line("todo", "a", "body").unwrap();
    store.set_card_title("todo", "a", "Renamed").unwrap();
    store.move_card("todo", "a", "doing", None, None).unwrap();
    store.reorder_card("doing", "a", 0).ok();
    store.delete_card("doing", "a").unwrap();
    store.rename_column("todo", "Backlog").unwrap();

    // Walk the whole tree looking for scratch files.
    let mut stack = vec![store.root.clone()];
    let mut leftovers = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().unwrap().is_dir() {
                stack.push(entry.path());
            } else if name.starts_with(".toolb-tmp-") {
                leftovers.push(entry.path());
            }
        }
    }
    assert!(leftovers.is_empty(), "temporary files left behind: {leftovers:?}");
}
