//! Keyboard dispatch. Three modes, checked in order: confirmation, text prompt,
//! then normal browsing.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::app::{App, Focus, PromptKind};

/// Route a key event to the right handler. Returns nothing; `app.should_quit`
/// signals the event loop to exit.
pub fn handle_key(app: &mut App, key: KeyEvent) {
    // On Windows terminals crossterm reports both press and release; ignore
    // everything that is not a press so actions do not fire twice.
    if key.kind == KeyEventKind::Release {
        return;
    }

    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    // Ctrl-C always quits, whatever mode we are in.
    if ctrl && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C')) {
        app.should_quit = true;
        return;
    }

    if app.confirm.is_some() {
        handle_confirm(app, key);
    } else if app.prompt.is_some() {
        handle_prompt(app, key, ctrl);
    } else {
        handle_normal(app, key, ctrl);
    }
}

fn handle_confirm(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => app.confirm_yes(),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => app.confirm_no(),
        // Enter is deliberately not a "yes" for destructive actions.
        _ => {}
    }
}

fn handle_prompt(app: &mut App, key: KeyEvent, ctrl: bool) {
    match key.code {
        KeyCode::Enter => app.submit_prompt(),
        KeyCode::Esc => app.cancel_prompt(),
        KeyCode::Tab => app.complete_prompt(),
        KeyCode::Backspace => {
            if let Some(p) = &mut app.prompt {
                p.backspace();
            }
            app.prompt_changed();
        }
        KeyCode::Delete => {
            if let Some(p) = &mut app.prompt {
                p.delete();
            }
            app.prompt_changed();
        }
        KeyCode::Left => {
            if let Some(p) = &mut app.prompt {
                p.left();
            }
        }
        KeyCode::Right => {
            if let Some(p) = &mut app.prompt {
                p.right();
            }
        }
        KeyCode::Home => {
            if let Some(p) = &mut app.prompt {
                p.home();
            }
        }
        KeyCode::End => {
            if let Some(p) = &mut app.prompt {
                p.end();
            }
        }
        // While filtering, let the arrows drive the list so the user can type
        // a few characters and step straight to a match.
        KeyCode::Down => {
            if app
                .prompt
                .as_ref()
                .is_some_and(|p| p.kind == PromptKind::Filter)
            {
                app.select_next(1);
            }
        }
        KeyCode::Up => {
            if app
                .prompt
                .as_ref()
                .is_some_and(|p| p.kind == PromptKind::Filter)
            {
                app.select_prev(1);
            }
        }
        KeyCode::Char('w') if ctrl => {
            if let Some(p) = &mut app.prompt {
                p.kill_segment();
            }
            app.prompt_changed();
        }
        KeyCode::Char('u') if ctrl => {
            if let Some(p) = &mut app.prompt {
                p.clear();
            }
            app.prompt_changed();
        }
        KeyCode::Char('a') if ctrl => {
            if let Some(p) = &mut app.prompt {
                p.home();
            }
        }
        KeyCode::Char('e') if ctrl => {
            if let Some(p) = &mut app.prompt {
                p.end();
            }
        }
        KeyCode::Char(ch) if !ctrl => {
            if let Some(p) = &mut app.prompt {
                p.insert(ch);
            }
            app.prompt_changed();
        }
        _ => {}
    }
}

fn handle_normal(app: &mut App, key: KeyEvent, ctrl: bool) {
    let preview_focus = app.focus == Focus::Preview;
    let page = if preview_focus {
        app.preview_viewport.saturating_sub(1).max(1)
    } else {
        app.list_viewport.saturating_sub(1).max(1)
    };

    match key.code {
        // ---- quit ----
        KeyCode::Char('q') | KeyCode::Char('Q') => app.should_quit = true,

        // ---- scroll the help column without leaving the current pane ----
        KeyCode::Down | KeyCode::Up if ctrl && app.show_help => {
            app.scroll_help(if key.code == KeyCode::Down { 1 } else { -1 });
        }

        // ---- vertical movement, meaning depends on the focused pane ----
        KeyCode::Down | KeyCode::Char('j') => {
            if preview_focus {
                app.preview_down(1);
            } else {
                app.select_next(1);
            }
        }
        KeyCode::Up | KeyCode::Char('k') => {
            if preview_focus {
                app.preview_up(1);
            } else {
                app.select_prev(1);
            }
        }
        KeyCode::PageDown => {
            if preview_focus {
                app.preview_down(page);
            } else {
                app.select_next(page as usize);
            }
        }
        KeyCode::PageUp => {
            if preview_focus {
                app.preview_up(page);
            } else {
                app.select_prev(page as usize);
            }
        }
        KeyCode::Home => {
            if preview_focus {
                app.preview.scroll_to_top();
            } else {
                app.select_first();
            }
        }
        KeyCode::End => {
            if preview_focus {
                let viewport = app.preview_viewport;
                app.preview.scroll_to_bottom(viewport);
            } else {
                app.select_last();
            }
        }

        // ---- hierarchy ----
        KeyCode::Enter => app.open_selected(),
        KeyCode::Right | KeyCode::Char('l') if !preview_focus => app.open_selected(),
        KeyCode::Left | KeyCode::Backspace => {
            if preview_focus {
                app.preview.scroll_left(8);
            } else {
                app.go_parent();
            }
        }
        KeyCode::Char('u') => app.go_parent(),
        KeyCode::Char('~') => {
            if let Some(home) = std::env::var_os("HOME") {
                app.change_dir(std::path::PathBuf::from(home));
            }
        }
        KeyCode::Tab | KeyCode::BackTab => app.toggle_focus(),

        // ---- preview-only horizontal scrolling ----
        KeyCode::Right if preview_focus => app.preview.scroll_right(8),
        KeyCode::Char('l') if preview_focus => app.toggle_activity(),

        // ---- file operations ----
        KeyCode::Char('c') | KeyCode::Char('C') => app.open_prompt(PromptKind::Copy),
        KeyCode::Char('m') | KeyCode::Char('M') => app.open_prompt(PromptKind::Move),
        KeyCode::Char('r') if !ctrl => app.open_prompt(PromptKind::Rename),
        KeyCode::Char('R') => app.open_prompt(PromptKind::Rename),
        KeyCode::Char('n') | KeyCode::Char('N') => app.open_prompt(PromptKind::MkDir),
        KeyCode::Char('d') | KeyCode::Char('D') | KeyCode::Delete => app.request_delete(),

        // ---- view ----
        KeyCode::Char('/') => app.open_prompt(PromptKind::Filter),
        KeyCode::Char('g') => app.open_prompt(PromptKind::Jump),
        KeyCode::Esc => {
            if !app.listing.filter.is_empty() {
                app.listing.filter.clear();
                app.listing.refilter();
                app.reload();
            } else if app.show_help {
                app.show_help = false;
            }
        }
        KeyCode::Char('H') => app.toggle_hidden(),
        KeyCode::Char('s') => app.cycle_sort(),
        KeyCode::Char('S') => app.toggle_sort_direction(),
        KeyCode::Char('w') | KeyCode::Char('W') => {
            app.preview.toggle_wrap();
        }
        KeyCode::Char('L') => app.toggle_activity(),
        KeyCode::F(5) => app.reload(),
        KeyCode::Char('r') if ctrl => app.reload(),
        KeyCode::Char('?') | KeyCode::F(1) => app.toggle_help(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    fn scratch(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("toolc-keys-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, body: &str) {
        fs::File::create(path)
            .unwrap()
            .write_all(body.as_bytes())
            .unwrap();
    }

    fn press(app: &mut App, code: KeyCode) {
        handle_key(app, KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn press_ctrl(app: &mut App, code: KeyCode) {
        handle_key(app, KeyEvent::new(code, KeyModifiers::CONTROL));
    }

    fn type_str(app: &mut App, text: &str) {
        for ch in text.chars() {
            press(app, KeyCode::Char(ch));
        }
    }

    #[test]
    fn q_quits_and_ctrl_c_quits() {
        let root = scratch("quit");
        let mut app = App::new(root.clone(), false);
        press(&mut app, KeyCode::Char('q'));
        assert!(app.should_quit);

        let mut app = App::new(root.clone(), false);
        press_ctrl(&mut app, KeyCode::Char('c'));
        assert!(app.should_quit);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn arrows_move_the_selection() {
        let root = scratch("arrows");
        write(&root.join("a.txt"), "a");
        write(&root.join("b.txt"), "b");
        write(&root.join("c.txt"), "c");
        let mut app = App::new(root.clone(), false);
        press(&mut app, KeyCode::Down);
        assert_eq!(app.selected_entry().unwrap().name, "b.txt");
        press(&mut app, KeyCode::Char('j'));
        assert_eq!(app.selected_entry().unwrap().name, "c.txt");
        press(&mut app, KeyCode::Up);
        assert_eq!(app.selected_entry().unwrap().name, "b.txt");
        press(&mut app, KeyCode::End);
        assert_eq!(app.selected_entry().unwrap().name, "c.txt");
        press(&mut app, KeyCode::Home);
        assert_eq!(app.selected_entry().unwrap().name, "a.txt");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn full_copy_flow_through_keys_only() {
        let root = scratch("copykeys");
        write(&root.join("src.txt"), "content\n");
        let mut app = App::new(root.clone(), false);
        press(&mut app, KeyCode::Char('c'));
        assert!(app.prompt.is_some());
        type_str(&mut app, "out.txt");
        press(&mut app, KeyCode::Enter);
        assert!(app.prompt.is_none());
        assert_eq!(
            fs::read_to_string(root.join("out.txt")).unwrap(),
            "content\n"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn mkdir_and_move_flow_through_keys_only() {
        let root = scratch("archivekeys");
        write(&root.join("app.log"), "log\n");
        let mut app = App::new(root.clone(), false);
        press(&mut app, KeyCode::Char('n'));
        type_str(&mut app, "archive");
        press(&mut app, KeyCode::Enter);
        assert!(root.join("archive").is_dir());

        press(&mut app, KeyCode::End); // select app.log (last entry)
        assert_eq!(app.selected_entry().unwrap().name, "app.log");
        press(&mut app, KeyCode::Char('m'));
        type_str(&mut app, "archive");
        press(&mut app, KeyCode::Enter);
        assert!(!root.join("app.log").exists());
        assert_eq!(
            fs::read_to_string(root.join("archive/app.log")).unwrap(),
            "log\n"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn delete_needs_y_and_esc_aborts() {
        let root = scratch("delkeys");
        write(&root.join("x.txt"), "x");
        let mut app = App::new(root.clone(), false);
        press(&mut app, KeyCode::Char('d'));
        assert!(app.confirm.is_some());
        press(&mut app, KeyCode::Enter); // Enter must not confirm a delete
        assert!(app.confirm.is_some());
        press(&mut app, KeyCode::Esc);
        assert!(app.confirm.is_none());
        assert!(root.join("x.txt").exists());

        press(&mut app, KeyCode::Char('d'));
        press(&mut app, KeyCode::Char('y'));
        assert!(!root.join("x.txt").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn typed_letters_do_not_trigger_commands_inside_a_prompt() {
        let root = scratch("promptletters");
        write(&root.join("a.txt"), "a");
        let mut app = App::new(root.clone(), false);
        press(&mut app, KeyCode::Char('r')); // rename
        app.prompt.as_mut().unwrap().clear();
        // 'd' and 'q' are commands in normal mode but plain text here.
        type_str(&mut app, "dq.txt");
        assert!(!app.should_quit);
        assert!(app.confirm.is_none());
        assert_eq!(app.prompt.as_ref().unwrap().input, "dq.txt");
        press(&mut app, KeyCode::Enter);
        assert!(root.join("dq.txt").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tab_switches_focus_and_arrows_scroll_the_preview() {
        let root = scratch("focus");
        let body: String = (1..=200).map(|i| format!("line {i}\n")).collect();
        write(&root.join("big.txt"), &body);
        let mut app = App::new(root.clone(), false);
        app.preview_viewport = 10;
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.focus, Focus::Preview);
        press(&mut app, KeyCode::Down);
        assert_eq!(app.preview.scroll, 1);
        press(&mut app, KeyCode::PageDown);
        assert!(app.preview.scroll > 1);
        press(&mut app, KeyCode::Home);
        assert_eq!(app.preview.scroll, 0);
        press(&mut app, KeyCode::Tab);
        assert_eq!(app.focus, Focus::List);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn help_and_view_toggles_respond() {
        let root = scratch("toggles");
        write(&root.join(".dot"), "d");
        write(&root.join("a.txt"), "a");
        let mut app = App::new(root.clone(), false);
        assert_eq!(app.listing.len(), 1);
        press(&mut app, KeyCode::Char('H'));
        assert_eq!(app.listing.len(), 2);
        press(&mut app, KeyCode::Char('?'));
        assert!(app.show_help);
        press(&mut app, KeyCode::Char('?'));
        assert!(!app.show_help);
        press(&mut app, KeyCode::Char('s'));
        assert_eq!(app.listing.sort_key, crate::listing::SortKey::Size);
        press(&mut app, KeyCode::Char('S'));
        assert!(app.listing.sort_reverse);
        press(&mut app, KeyCode::Char('L'));
        assert!(app.show_activity);
        let wrap = app.preview.wrap;
        press(&mut app, KeyCode::Char('w'));
        assert_eq!(app.preview.wrap, !wrap);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn enter_descends_and_left_ascends() {
        let root = scratch("hier");
        fs::create_dir(root.join("sub")).unwrap();
        write(&root.join("sub/f.txt"), "f");
        let mut app = App::new(root.clone(), false);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.cwd(), root.join("sub"));
        press(&mut app, KeyCode::Left);
        assert_eq!(app.cwd(), root);
        press(&mut app, KeyCode::Right);
        assert_eq!(app.cwd(), root.join("sub"));
        press(&mut app, KeyCode::Char('u'));
        assert_eq!(app.cwd(), root);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn filter_key_narrows_then_esc_restores() {
        let root = scratch("filterkeys");
        for n in ["alpha.txt", "beta.txt"] {
            write(&root.join(n), n);
        }
        let mut app = App::new(root.clone(), false);
        press(&mut app, KeyCode::Char('/'));
        type_str(&mut app, "alp");
        assert_eq!(app.listing.len(), 1);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.listing.filter, "alp");
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.listing.len(), 2);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn goto_prompt_changes_directory() {
        let root = scratch("goto");
        fs::create_dir_all(root.join("deep/inner")).unwrap();
        let mut app = App::new(root.clone(), false);
        press(&mut app, KeyCode::Char('g'));
        type_str(&mut app, "deep/inner");
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.cwd(), root.join("deep/inner"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn ctrl_u_clears_the_prompt_line() {
        let root = scratch("ctrlu");
        write(&root.join("a.txt"), "a");
        let mut app = App::new(root.clone(), false);
        press(&mut app, KeyCode::Char('c'));
        type_str(&mut app, "some/path");
        press_ctrl(&mut app, KeyCode::Char('u'));
        assert_eq!(app.prompt.as_ref().unwrap().input, "");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn key_release_events_are_ignored() {
        let root = scratch("release");
        write(&root.join("a.txt"), "a");
        let mut app = App::new(root.clone(), false);
        let mut ev = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        ev.kind = KeyEventKind::Release;
        handle_key(&mut app, ev);
        assert!(!app.should_quit);
        fs::remove_dir_all(&root).unwrap();
    }
}
