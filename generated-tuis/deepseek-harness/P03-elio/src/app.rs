//! Application state and event handling.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::ops::{self, Entry, Preview};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Input,
    Confirm,
    Help,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    List,
    Preview,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Rename,
    Copy,
    Move,
    Mkdir,
    Search,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum MsgKind {
    Info,
    Success,
    Error,
}

pub struct Message {
    pub text: String,
    pub kind: MsgKind,
}

pub struct App {
    pub cwd: PathBuf,
    pub all_entries: Vec<Entry>,
    pub entries: Vec<Entry>,
    pub selected: usize,
    pub list_offset: usize,
    pub preview: Option<Preview>,
    pub preview_name: String,
    pub preview_scroll: usize,
    pub focus: Focus,
    pub mode: Mode,
    pub action: Option<Action>,
    pub input: String,
    pub filter: String,
    pub message: Option<Message>,
    pub help_scroll: usize,
    pub should_quit: bool,
}

impl App {
    pub fn new(start_dir: PathBuf) -> Self {
        let cwd = start_dir.canonicalize().unwrap_or(start_dir);
        let mut app = App {
            cwd,
            all_entries: Vec::new(),
            entries: Vec::new(),
            selected: 0,
            list_offset: 0,
            preview: None,
            preview_name: String::new(),
            preview_scroll: 0,
            focus: Focus::List,
            mode: Mode::Normal,
            action: None,
            input: String::new(),
            filter: String::new(),
            message: None,
            help_scroll: 0,
            should_quit: false,
        };
        app.refresh();
        app.update_preview();
        app
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.mode {
            Mode::Normal => self.handle_normal(key),
            Mode::Input => self.handle_input(key),
            Mode::Confirm => self.handle_confirm(key),
            Mode::Help => self.handle_help(key),
        }
    }

    // ---- helpers ---------------------------------------------------------

    pub fn selected_entry(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    fn refresh(&mut self) {
        let mut list = match ops::list_dir(&self.cwd) {
            Ok(l) => l,
            Err(e) => {
                self.set_error(e);
                Vec::new()
            }
        };
        list.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
        if self.cwd.parent().is_some() {
            list.insert(
                0,
                Entry {
                    name: "..".to_string(),
                    path: self.cwd.join(".."),
                    is_dir: true,
                    size: 0,
                },
            );
        }
        self.all_entries = list;
        self.apply_filter();
    }

    fn apply_filter(&mut self) {
        let f = self.filter.to_lowercase();
        self.entries = if f.is_empty() {
            self.all_entries.clone()
        } else {
            self.all_entries
                .iter()
                .filter(|e| e.name == ".." || e.name.to_lowercase().contains(&f))
                .cloned()
                .collect()
        };
        if self.entries.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.entries.len() {
            self.selected = self.entries.len() - 1;
        }
        self.list_offset = 0;
    }

    fn refresh_and_select(&mut self, select_name: Option<&str>) {
        self.refresh();
        if let Some(n) = select_name {
            if let Some(i) = self.entries.iter().position(|e| e.name == n) {
                self.selected = i;
            }
        }
        self.update_preview();
    }

    fn update_preview(&mut self) {
        self.preview_scroll = 0;
        let entry = self.selected_entry().cloned();
        match entry {
            Some(e) => {
                self.preview_name = e.name.clone();
                if e.is_dir {
                    self.preview = None;
                } else {
                    self.preview = Some(match ops::read_preview(&e.path) {
                        Ok(p) => p,
                        Err(err) => Preview::Error(err),
                    });
                }
            }
            None => {
                self.preview_name.clear();
                self.preview = None;
            }
        }
    }

    fn set_info(&mut self, text: String) {
        self.message = Some(Message {
            text,
            kind: MsgKind::Info,
        });
    }

    fn set_success(&mut self, text: String) {
        self.message = Some(Message {
            text,
            kind: MsgKind::Success,
        });
    }

    fn set_error(&mut self, text: String) {
        self.message = Some(Message {
            text,
            kind: MsgKind::Error,
        });
    }

    // ---- normal mode -----------------------------------------------------

    fn handle_normal(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') | KeyCode::F(1) => {
                self.mode = Mode::Help;
                self.help_scroll = 0;
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::List => Focus::Preview,
                    Focus::Preview => Focus::List,
                };
            }
            KeyCode::F(2) => self.begin_action(Action::Rename),
            KeyCode::F(5) => self.begin_action(Action::Copy),
            KeyCode::F(6) => self.begin_action(Action::Move),
            KeyCode::F(7) => self.begin_action(Action::Mkdir),
            KeyCode::Delete | KeyCode::F(8) => self.begin_confirm_delete(),
            KeyCode::Char('/') => self.begin_action(Action::Search),
            _ => match self.focus {
                Focus::List => self.handle_list_keys(key),
                Focus::Preview => self.handle_preview_keys(key),
            },
        }
    }

    fn handle_list_keys(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                self.update_preview();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.selected + 1 < self.entries.len() {
                    self.selected += 1;
                }
                self.update_preview();
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.selected = 0;
                self.update_preview();
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.selected = self.entries.len().saturating_sub(1);
                self.update_preview();
            }
            KeyCode::PageUp => {
                self.selected = self.selected.saturating_sub(10);
                self.update_preview();
            }
            KeyCode::PageDown => {
                self.selected = (self.selected + 10).min(self.entries.len().saturating_sub(1));
                self.update_preview();
            }
            KeyCode::Enter | KeyCode::Right => self.enter_selected(),
            KeyCode::Left | KeyCode::Backspace => self.go_parent(),
            _ => {}
        }
    }

    fn handle_preview_keys(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.preview_scroll = self.preview_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.preview_scroll += 1;
            }
            KeyCode::PageUp => {
                self.preview_scroll = self.preview_scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.preview_scroll += 10;
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.preview_scroll = 0;
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.preview_scroll = usize::MAX;
            }
            _ => {}
        }
    }

    fn enter_selected(&mut self) {
        let Some(e) = self.selected_entry().cloned() else {
            return;
        };
        if !e.is_dir {
            return;
        }
        if e.name == ".." {
            self.go_parent();
            return;
        }
        if e.path.is_dir() {
            self.cwd = e.path;
            self.selected = 0;
            self.refresh();
            self.update_preview();
        } else {
            self.set_error(format!("Cannot open '{}': not a directory", e.name));
        }
    }

    fn go_parent(&mut self) {
        if let Some(parent) = self.cwd.parent() {
            self.cwd = parent.to_path_buf();
            self.selected = 0;
            self.refresh();
            self.update_preview();
        }
    }

    fn begin_action(&mut self, action: Action) {
        if self.selected_entry().is_none() {
            return;
        }
        self.action = Some(action);
        self.input.clear();
        match action {
            Action::Rename => {
                if let Some(e) = self.selected_entry() {
                    self.input = e.name.clone();
                }
            }
            Action::Search => {
                self.input = self.filter.clone();
            }
            _ => {}
        }
        self.mode = Mode::Input;
    }

    fn begin_confirm_delete(&mut self) {
        match self.selected_entry() {
            None => {}
            Some(e) if e.name == ".." => self.set_error("Cannot delete the parent entry".to_string()),
            Some(_) => self.mode = Mode::Confirm,
        }
    }

    // ---- input mode ------------------------------------------------------

    fn handle_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.cancel_input(),
            KeyCode::Enter => self.submit_input(),
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.clear();
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                let trimmed = self.input.trim_end();
                let idx = trimmed
                    .rfind(char::is_whitespace)
                    .map(|i| i + 1)
                    .unwrap_or(0);
                self.input.truncate(idx);
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.cancel_input();
            }
            KeyCode::Char(c) => self.input.push(c),
            _ => {}
        }
    }

    fn cancel_input(&mut self) {
        self.mode = Mode::Normal;
        self.action = None;
        self.input.clear();
        self.set_info("Cancelled".to_string());
    }

    fn submit_input(&mut self) {
        let action = match self.action.take() {
            Some(a) => a,
            None => {
                self.mode = Mode::Normal;
                return;
            }
        };
        let value = self.input.clone();
        self.mode = Mode::Normal;
        self.input.clear();
        match action {
            Action::Search => {
                self.filter = value;
                self.selected = 0;
                self.apply_filter();
                self.update_preview();
            }
            Action::Rename => self.do_rename(value),
            Action::Copy => self.do_copy(value),
            Action::Move => self.do_move(value),
            Action::Mkdir => self.do_mkdir(value),
        }
    }

    // ---- operations ------------------------------------------------------

    fn do_rename(&mut self, name: String) {
        let name = name.trim().to_string();
        if name.is_empty() {
            self.set_error("Rename: empty name".into());
            return;
        }
        let Some(e) = self.selected_entry().cloned() else {
            return;
        };
        if e.name == ".." {
            self.set_error("Cannot rename the parent entry".into());
            return;
        }
        let target = resolve_path(&self.cwd, &name);
        if target == e.path {
            self.set_info("No change".into());
            return;
        }
        match ops::rename_path(&e.path, &target) {
            Ok(()) => {
                let new_name = target.file_name().map(|s| s.to_string_lossy().to_string());
                self.set_success(format!("Renamed {} -> {}", e.name, name));
                self.refresh_and_select(new_name.as_deref());
            }
            Err(err) => self.set_error(err),
        }
    }

    fn do_copy(&mut self, dest: String) {
        let dest = dest.trim().to_string();
        if dest.is_empty() {
            self.set_error("Copy: empty destination".into());
            return;
        }
        let Some(e) = self.selected_entry().cloned() else {
            return;
        };
        if e.is_dir {
            self.set_error("Copy: please select a file".into());
            return;
        }
        let dest_path = resolve_path(&self.cwd, &dest);
        let target = if dest_path.is_dir() {
            dest_path.join(&e.name)
        } else {
            dest_path
        };
        match ops::copy_file(&e.path, &target) {
            Ok(()) => {
                self.set_success(format!("Copied {} -> {}", e.name, target.display()));
                self.refresh();
                self.update_preview();
            }
            Err(err) => self.set_error(err),
        }
    }

    fn do_move(&mut self, dest: String) {
        let dest = dest.trim().to_string();
        if dest.is_empty() {
            self.set_error("Move: empty destination".into());
            return;
        }
        let Some(e) = self.selected_entry().cloned() else {
            return;
        };
        if e.name == ".." {
            self.set_error("Cannot move the parent entry".into());
            return;
        }
        let dest_path = resolve_path(&self.cwd, &dest);
        let target = if dest_path.is_dir() {
            dest_path.join(&e.name)
        } else {
            dest_path
        };
        match ops::move_path(&e.path, &target) {
            Ok(()) => {
                self.set_success(format!("Moved {} -> {}", e.name, target.display()));
                self.refresh();
                self.update_preview();
            }
            Err(err) => self.set_error(err),
        }
    }

    fn do_mkdir(&mut self, path: String) {
        let path = path.trim().to_string();
        if path.is_empty() {
            self.set_error("Mkdir: empty path".into());
            return;
        }
        let target = resolve_path(&self.cwd, &path);
        match ops::make_dir(&target) {
            Ok(()) => {
                self.set_success(format!("Created directory {}", target.display()));
                self.refresh();
                self.update_preview();
            }
            Err(err) => self.set_error(err),
        }
    }

    // ---- confirm mode ----------------------------------------------------

    fn handle_confirm(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => self.confirm_delete(),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.set_info("Cancelled".to_string());
            }
            _ => {}
        }
    }

    fn confirm_delete(&mut self) {
        self.mode = Mode::Normal;
        let Some(e) = self.selected_entry().cloned() else {
            return;
        };
        if e.name == ".." {
            return;
        }
        match ops::delete_path(&e.path) {
            Ok(()) => {
                self.set_success(format!("Deleted {}", e.name));
                self.refresh();
                self.update_preview();
            }
            Err(err) => self.set_error(err),
        }
    }

    // ---- help mode -------------------------------------------------------

    fn handle_help(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::F(1) | KeyCode::Char('?') => {
                self.mode = Mode::Normal;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.help_scroll = self.help_scroll.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.help_scroll += 1;
            }
            KeyCode::PageUp => {
                self.help_scroll = self.help_scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.help_scroll += 10;
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.help_scroll = 0;
            }
            _ => {}
        }
    }
}

/// Resolve a user-typed destination against the current directory.
fn resolve_path(cwd: &Path, s: &str) -> PathBuf {
    let p = Path::new(s);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    }
}
