//! `toolf` — an interactive Shell pipeline debugger for log analysis.
//!
//! The tool centres on one log file (`--file`), lets you build a shell pipeline against it with
//! syntax highlighting and Tab completion, run either the whole pipeline or just the stages
//! before the cursor, and write the resulting output to a file.
//!
//! Every pipeline is executed by a real shell; no output is ever synthesised.

mod app;
mod complete;
mod exec;
mod highlight;
mod pipeline;
mod ui;

use std::io;
use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use app::{App, Level, Prompt};

/// Interactive Shell pipeline debugger for log files.
#[derive(Parser, Debug)]
#[command(
    name = "toolf",
    version,
    about = "Interactive Shell pipeline debugging TUI for log analysis",
    long_about = "toolf lets you build, debug and run shell pipelines against a log file.\n\n\
                  Edit a pipeline with syntax highlighting and Tab completion, run the whole \
                  pipeline with Enter or only the stages before the cursor with Alt+\\, then \
                  save the output to any path with Ctrl+S.\n\n\
                  Press F1 inside the tool for the full key reference."
)]
struct Cli {
    /// Log file to analyse.
    #[arg(short, long, default_value = "/bench/server.log", value_name = "PATH")]
    file: PathBuf,

    /// Pre-fill the command line with this pipeline instead of the default.
    #[arg(short = 'c', long, value_name = "PIPELINE")]
    command: Option<String>,
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    let mut app = App::new(cli.file.clone());
    if let Some(cmd) = cli.command {
        app.clear_line();
        app.insert_str(&cmd);
    }
    if !cli.file.exists() {
        app.set_status(format!("warning: {} does not exist", cli.file.display()), Level::Error);
    }

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app);
    ratatui::restore();
    result
}

/// The event loop.
fn run(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        // A short poll keeps the spinner animating and picks up finished runs promptly.
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            handle_key(app, key);
        }
        app.poll();

        if app.should_quit {
            return Ok(());
        }
    }
}

/// Dispatch one key press.
fn handle_key(app: &mut App, key: KeyEvent) {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    // Quit works from anywhere.
    if matches!(key.code, KeyCode::F(10)) || (ctrl && matches!(key.code, KeyCode::Char('q'))) {
        app.should_quit = true;
        return;
    }

    // While a prompt is open it owns most keys.
    if app.prompt != Prompt::None {
        match key.code {
            KeyCode::Enter => app.prompt_confirm(),
            KeyCode::Esc => app.prompt_cancel(),
            KeyCode::Tab => app.prompt_complete(),
            KeyCode::Backspace => app.prompt_backspace(),
            KeyCode::Delete => app.prompt_delete(),
            KeyCode::Left => app.prompt_move(-1),
            KeyCode::Right => app.prompt_move(1),
            KeyCode::Home => app.prompt_home(),
            KeyCode::End => app.prompt_end(),
            KeyCode::Char('u') if ctrl => {
                app.prompt_input.clear();
                app.prompt_cursor = 0;
            }
            KeyCode::Char(c) if !ctrl => app.prompt_insert(c),
            _ => {}
        }
        return;
    }

    match key.code {
        // ------------------------------------------------------------ running
        KeyCode::Enter => {
            // Enter accepts a completion first, so Tab→Enter behaves as expected.
            if app.completion.is_some() {
                app.completion_accept();
            } else {
                app.run_full();
            }
        }
        // Alt+\ runs the pipeline up to the cursor. Some terminals send the shifted `|`
        // instead of `\`, so accept that as an alias, and F5 as a modifier-free fallback.
        KeyCode::Char('\\') if alt => app.run_partial(),
        KeyCode::Char('|') if alt => app.run_partial(),
        KeyCode::F(5) => app.run_partial(),
        KeyCode::Char('c') if ctrl => {
            if app.running {
                app.cancel_run();
            } else {
                app.set_status("nothing running (F10 or Ctrl+Q quits)", Level::Info);
            }
        }

        // ----------------------------------------------------------- prompts
        KeyCode::Char('s') if ctrl => app.begin_save(),
        KeyCode::Char('f') if ctrl => app.begin_search(),
        KeyCode::F(3) if shift => app.seek_match(false),
        KeyCode::F(3) => app.seek_match(true),

        // -------------------------------------------------------- completion
        KeyCode::Tab => app.complete(),
        KeyCode::BackTab => app.completion_move(-1),
        KeyCode::Esc => app.completion_dismiss(),

        // -------------------------------------------------------------- help
        KeyCode::F(1) => app.toggle_help(),
        KeyCode::Up if alt => app.scroll_help(-1),
        KeyCode::Down if alt => app.scroll_help(1),

        // ---------------------------------------------------------- movement
        KeyCode::Left if alt => app.move_prev_boundary(),
        KeyCode::Right if alt => app.move_next_boundary(),
        KeyCode::Left if ctrl => app.move_word_left(),
        KeyCode::Right if ctrl => app.move_word_right(),
        KeyCode::Left => app.move_left(),
        KeyCode::Right => app.move_right(),
        KeyCode::Home if ctrl => app.scroll_top(),
        KeyCode::End if ctrl => app.scroll_bottom(),
        KeyCode::Home => app.move_home(),
        KeyCode::End => app.move_end(),

        // ------------------------------------------- history / completion list
        KeyCode::Up if ctrl => app.scroll_output(-1),
        KeyCode::Down if ctrl => app.scroll_output(1),
        KeyCode::Up => {
            if app.completion.is_some() {
                app.completion_move(-1);
            } else {
                app.history_prev();
            }
        }
        KeyCode::Down => {
            if app.completion.is_some() {
                app.completion_move(1);
            } else {
                app.history_next();
            }
        }
        KeyCode::Char('p') if ctrl => app.history_prev(),
        KeyCode::Char('n') if ctrl => app.history_next(),

        // --------------------------------------------------- output scrolling
        KeyCode::PageUp => app.scroll_page(-1),
        KeyCode::PageDown => app.scroll_page(1),

        // ------------------------------------------------------------ editing
        KeyCode::Backspace => app.backspace(),
        KeyCode::Delete => app.delete(),
        KeyCode::Char('w') if ctrl => app.delete_word_back(),
        KeyCode::Char('k') if ctrl => app.kill_to_end(),
        KeyCode::Char('u') if ctrl => app.kill_to_start(),
        KeyCode::Char('l') if ctrl => app.clear_line(),
        // Plain printable input. Alt combinations are reserved for navigation.
        KeyCode::Char(c) if !ctrl && !alt => app.insert_char(c),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_mod(code: KeyCode, m: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, m)
    }

    fn app() -> App {
        App::new(PathBuf::from("/tmp/toolf-main.log"))
    }

    fn run_and_wait(a: &mut App) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while a.running && Instant::now() < deadline {
            if !a.poll() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        a.poll();
    }

    #[test]
    fn cli_defaults_to_the_bench_log() {
        let cli = Cli::parse_from(["toolf"]);
        assert_eq!(cli.file, PathBuf::from("/bench/server.log"));
        assert!(cli.command.is_none());
    }

    #[test]
    fn cli_accepts_file_override() {
        let cli = Cli::parse_from(["toolf", "--file", "/var/log/other.log"]);
        assert_eq!(cli.file, PathBuf::from("/var/log/other.log"));
        let cli = Cli::parse_from(["toolf", "-f", "/tmp/a.log"]);
        assert_eq!(cli.file, PathBuf::from("/tmp/a.log"));
    }

    #[test]
    fn typing_inserts_and_editing_keys_work() {
        let mut a = app();
        a.clear_line();
        for c in "cat f".chars() {
            handle_key(&mut a, key(KeyCode::Char(c)));
        }
        assert_eq!(a.line(), "cat f");
        handle_key(&mut a, key(KeyCode::Backspace));
        assert_eq!(a.line(), "cat ");
        handle_key(&mut a, key_mod(KeyCode::Char('l'), KeyModifiers::CONTROL));
        assert_eq!(a.line(), "");
    }

    #[test]
    fn enter_runs_the_pipeline() {
        let mut a = app();
        a.clear_line();
        a.insert_str("echo from-enter");
        handle_key(&mut a, key(KeyCode::Enter));
        run_and_wait(&mut a);
        assert_eq!(a.output_lines(), &["from-enter".to_string()]);
    }

    #[test]
    fn alt_backslash_runs_partial_pipeline() {
        let mut a = app();
        a.clear_line();
        a.insert_str("printf '1\\n2\\n' | wc -l");
        // Put the cursor in stage 1.
        handle_key(&mut a, key(KeyCode::Home));
        handle_key(&mut a, key_mod(KeyCode::Right, KeyModifiers::ALT));
        assert_eq!(a.stage_position(), (1, 2));
        handle_key(&mut a, key_mod(KeyCode::Char('\\'), KeyModifiers::ALT));
        run_and_wait(&mut a);
        // Stage 1 alone prints the two lines rather than the count.
        assert_eq!(a.output_lines(), &["1".to_string(), "2".to_string()]);
    }

    #[test]
    fn tab_opens_completion_and_enter_accepts_it() {
        let dir = std::env::temp_dir().join("toolf-main-tab");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("pick-a.log"), b"x").unwrap();
        std::fs::write(dir.join("pick-b.log"), b"x").unwrap();

        let mut a = app();
        a.clear_line();
        a.insert_str(&format!("cat {}/pi", dir.display()));
        handle_key(&mut a, key(KeyCode::Tab));
        assert!(a.completion.is_some());
        // Enter accepts the highlighted candidate instead of running.
        handle_key(&mut a, key(KeyCode::Enter));
        assert!(a.completion.is_none());
        assert!(!a.running, "Enter should accept the completion, not run");
        assert!(a.line().ends_with("pick-a.log"), "line: {}", a.line());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ctrl_s_opens_save_and_esc_cancels() {
        let mut a = app();
        a.clear_line();
        a.insert_str("echo x");
        handle_key(&mut a, key(KeyCode::Enter));
        run_and_wait(&mut a);
        handle_key(&mut a, key_mod(KeyCode::Char('s'), KeyModifiers::CONTROL));
        assert_eq!(a.prompt, Prompt::Save);
        handle_key(&mut a, key(KeyCode::Esc));
        assert_eq!(a.prompt, Prompt::None);
    }

    #[test]
    fn save_prompt_writes_the_typed_path() {
        let mut a = app();
        a.clear_line();
        a.insert_str("printf 'saved\\n'");
        handle_key(&mut a, key(KeyCode::Enter));
        run_and_wait(&mut a);

        let target = std::env::temp_dir().join("toolf-main-save.txt");
        let _ = std::fs::remove_file(&target);
        handle_key(&mut a, key_mod(KeyCode::Char('s'), KeyModifiers::CONTROL));
        // Clear the pre-filled default, then type the destination key by key.
        handle_key(&mut a, key_mod(KeyCode::Char('u'), KeyModifiers::CONTROL));
        for c in target.to_string_lossy().chars() {
            handle_key(&mut a, key(KeyCode::Char(c)));
        }
        handle_key(&mut a, key(KeyCode::Enter));
        assert_eq!(a.prompt, Prompt::None);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "saved\n");
        let _ = std::fs::remove_file(&target);
    }

    #[test]
    fn history_keys_recall_commands() {
        let mut a = app();
        a.clear_line();
        a.insert_str("echo first");
        handle_key(&mut a, key(KeyCode::Enter));
        run_and_wait(&mut a);
        a.clear_line();
        handle_key(&mut a, key(KeyCode::Up));
        assert_eq!(a.line(), "echo first");
    }

    #[test]
    fn f10_and_ctrl_q_quit() {
        let mut a = app();
        handle_key(&mut a, key(KeyCode::F(10)));
        assert!(a.should_quit);
        let mut a = app();
        handle_key(&mut a, key_mod(KeyCode::Char('q'), KeyModifiers::CONTROL));
        assert!(a.should_quit);
    }

    #[test]
    fn f1_toggles_help_and_alt_arrows_scroll_it() {
        let mut a = app();
        let before = a.show_help;
        handle_key(&mut a, key(KeyCode::F(1)));
        assert_ne!(a.show_help, before);
        handle_key(&mut a, key(KeyCode::F(1)));
        handle_key(&mut a, key_mod(KeyCode::Down, KeyModifiers::ALT));
        assert_eq!(a.help_scroll, 1);
        handle_key(&mut a, key_mod(KeyCode::Up, KeyModifiers::ALT));
        assert_eq!(a.help_scroll, 0);
    }

    #[test]
    fn search_keys_find_and_cycle() {
        let mut a = app();
        a.clear_line();
        a.insert_str("printf 'aa\\nbb\\naa\\n'");
        handle_key(&mut a, key(KeyCode::Enter));
        run_and_wait(&mut a);
        handle_key(&mut a, key_mod(KeyCode::Char('f'), KeyModifiers::CONTROL));
        assert_eq!(a.prompt, Prompt::Search);
        for c in "aa".chars() {
            handle_key(&mut a, key(KeyCode::Char(c)));
        }
        handle_key(&mut a, key(KeyCode::Enter));
        assert_eq!(a.matches, vec![0, 2]);
        handle_key(&mut a, key(KeyCode::F(3)));
        assert_eq!(a.match_pos, 1);
        handle_key(&mut a, key_mod(KeyCode::F(3), KeyModifiers::SHIFT));
        assert_eq!(a.match_pos, 0);
    }

    #[test]
    fn scroll_keys_move_the_output_viewport() {
        let mut a = app();
        a.clear_line();
        a.insert_str("seq 1 200");
        handle_key(&mut a, key(KeyCode::Enter));
        run_and_wait(&mut a);
        a.output_height = 10;
        handle_key(&mut a, key(KeyCode::PageDown));
        assert_eq!(a.output_scroll, 10);
        handle_key(&mut a, key_mod(KeyCode::Down, KeyModifiers::CONTROL));
        assert_eq!(a.output_scroll, 11);
        handle_key(&mut a, key_mod(KeyCode::Home, KeyModifiers::CONTROL));
        assert_eq!(a.output_scroll, 0);
        handle_key(&mut a, key_mod(KeyCode::End, KeyModifiers::CONTROL));
        assert_eq!(a.output_scroll, 190);
    }

    #[test]
    fn ctrl_chars_do_not_leak_into_the_line() {
        let mut a = app();
        a.clear_line();
        // A ctrl combination with no binding must not insert a character.
        handle_key(&mut a, key_mod(KeyCode::Char('z'), KeyModifiers::CONTROL));
        assert_eq!(a.line(), "");
        // Alt combinations are navigation, not text.
        handle_key(&mut a, key_mod(KeyCode::Char('x'), KeyModifiers::ALT));
        assert_eq!(a.line(), "");
    }
}
