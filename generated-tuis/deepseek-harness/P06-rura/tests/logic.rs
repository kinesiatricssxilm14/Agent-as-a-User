//! Headless integration tests that exercise the application logic and the
//! ratatui rendering path without needing a real terminal.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;

use toolf::app::App;
use toolf::editor::Editor;
use toolf::ui;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn alt(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::ALT)
}

fn app() -> App {
    App::new(PathBuf::from("/tmp/nonexistent.log"))
}

fn tmp_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(name)
}

#[test]
fn full_run_executes_pipeline() {
    let mut a = app();
    a.editor.set_text("echo hello | tr a-z A-Z");
    a.handle_key(key(KeyCode::Enter));

    let r = a.last_run.expect("a run should have happened");
    assert_eq!(r.code, Some(0));
    assert_eq!(r.stdout, "HELLO\n");
}

#[test]
fn partial_run_executes_prefix_up_to_cursor() {
    let mut a = app();
    a.editor.set_text("echo one | grep one | cat");
    a.editor.move_home();
    // Move the cursor into the first segment so Alt+\ runs just "echo one".
    for _ in 0..4 {
        a.editor.move_right();
    }
    a.handle_key(alt('\\'));

    let r = a.last_run.expect("a partial run should have happened");
    assert_eq!(r.stdout, "one\n");
}

#[test]
fn partial_run_via_ctrl_o_fallback() {
    let mut a = app();
    a.editor.set_text("echo foo | wc -c");
    a.editor.move_home();
    for _ in 0..4 {
        a.editor.move_right();
    }
    a.handle_key(ctrl('o'));
    let r = a.last_run.expect("a partial run should have happened");
    assert_eq!(r.stdout, "foo\n");
}

#[test]
fn save_writes_last_output_to_file() {
    let path = tmp_path("save_result.txt");
    let _ = std::fs::remove_file(&path);

    let mut a = app();
    a.editor.set_text("printf 'saved-content'");
    a.handle_key(key(KeyCode::Enter));

    a.handle_key(ctrl('s'));
    a.save_editor = Editor::new();
    a.save_editor.set_text(path.to_str().unwrap());
    a.handle_key(key(KeyCode::Enter));

    let content = std::fs::read_to_string(&path).expect("file should exist");
    assert_eq!(content, "saved-content\n");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn command_completion_completes_echo() {
    let mut a = app();
    a.editor.set_text("ec");
    a.handle_key(key(KeyCode::Tab));
    assert!(
        a.editor.to_string().starts_with("echo"),
        "expected echo completion, got {:?}",
        a.editor.to_string()
    );
}

#[test]
fn path_completion_completes_unique_file() {
    let dir = tmp_path("comptest");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("unique_log.txt");
    std::fs::write(&file, "x").unwrap();

    let mut a = app();
    let prefix = format!("cat {}/uni", dir.display());
    a.editor.set_text(&prefix);
    a.handle_key(key(KeyCode::Tab));

    assert!(
        a.editor.to_string().contains("unique_log.txt"),
        "expected path completion, got {:?}",
        a.editor.to_string()
    );
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_dir(&dir);
}

#[test]
fn quit_key_sets_should_quit() {
    let mut a = app();
    a.handle_key(ctrl('c'));
    assert!(a.should_quit);
}

#[test]
fn render_shows_command_output_preview_on_one_screen() {
    let mut a = app();
    a.editor.set_text("cat /bench/server.log | grep ERROR | tail -5");

    let backend = TestBackend::new(120, 30);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| ui::draw(f, &mut a)).unwrap();

    let buf = terminal.backend().buffer();
    let text: String = buf.content.iter().map(|c| c.symbol()).collect();

    for needle in ["Output", "Pipeline preview", "Command", "toolf", "cat", "grep ERROR", "tail -5"] {
        assert!(
            text.contains(needle),
            "rendered screen missing {needle:?}\n---\n{text}\n---"
        );
    }
}
