use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::model::{format_size, parse_size, Node};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ViewMode {
    Tree,
    List,
}

impl ViewMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tree => "TREE",
            Self::List => "LIST",
        }
    }
}

#[derive(Clone, Debug)]
pub struct VisibleEntry {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub depth: usize,
}

#[derive(Clone, Debug)]
pub enum InputMode {
    Normal,
    Filter(String),
    ConfirmDelete(PathBuf),
}

pub struct App {
    pub root: Node,
    pub root_path: PathBuf,
    pub current_path: PathBuf,
    pub selected: usize,
    pub scroll: usize,
    pub filter_bytes: u64,
    pub descending: bool,
    pub view_mode: ViewMode,
    pub input_mode: InputMode,
    pub status: String,
    pub show_help: bool,
    pub top_n: usize,
    pub quit: bool,
}

impl App {
    pub fn new(path: PathBuf) -> Result<Self> {
        let root_path = path
            .canonicalize()
            .with_context(|| format!("cannot open {}", path.display()))?;
        let root = Node::scan(&root_path)
            .with_context(|| format!("cannot scan {}", root_path.display()))?;
        if !root.is_dir {
            anyhow::bail!("scan root must be a directory: {}", root_path.display());
        }
        Ok(Self {
            current_path: root_path.clone(),
            root_path,
            root,
            selected: 0,
            scroll: 0,
            filter_bytes: 0,
            descending: true,
            view_mode: ViewMode::Tree,
            input_mode: InputMode::Normal,
            status: "Scan complete".to_string(),
            show_help: false,
            top_n: 8,
            quit: false,
        })
    }

    pub fn current(&self) -> &Node {
        self.root.find(&self.current_path).unwrap_or(&self.root)
    }

    pub fn visible_entries(&self) -> Vec<VisibleEntry> {
        let mut output = Vec::new();
        match self.view_mode {
            ViewMode::Tree => self.flatten_tree(self.current(), 0, &mut output),
            ViewMode::List => {
                let mut children: Vec<&Node> = self
                    .current()
                    .children
                    .iter()
                    .filter(|node| node.size >= self.filter_bytes)
                    .collect();
                self.sort_nodes(&mut children);
                output.extend(children.into_iter().map(|node| VisibleEntry {
                    path: node.path.clone(),
                    name: node.name.clone(),
                    size: node.size,
                    is_dir: node.is_dir,
                    depth: 0,
                }));
            }
        }
        output
    }

    fn flatten_tree(&self, node: &Node, depth: usize, output: &mut Vec<VisibleEntry>) {
        let mut children: Vec<&Node> = node
            .children
            .iter()
            .filter(|child| child.size >= self.filter_bytes)
            .collect();
        self.sort_nodes(&mut children);
        for child in children {
            output.push(VisibleEntry {
                path: child.path.clone(),
                name: child.name.clone(),
                size: child.size,
                is_dir: child.is_dir,
                depth,
            });
            if child.is_dir {
                self.flatten_tree(child, depth + 1, output);
            }
        }
    }

    fn sort_nodes(&self, nodes: &mut Vec<&Node>) {
        if self.descending {
            nodes.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
        } else {
            nodes.sort_by(|a, b| a.size.cmp(&b.size).then_with(|| a.name.cmp(&b.name)));
        }
    }

    pub fn selected_entry(&self) -> Option<VisibleEntry> {
        self.visible_entries().get(self.selected).cloned()
    }

    pub fn top_files(&self) -> Vec<&Node> {
        let mut files = Vec::new();
        self.current().collect_files(&mut files);
        files.retain(|file| file.size >= self.filter_bytes);
        files.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.name.cmp(&b.name)));
        files.truncate(self.top_n);
        files
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match self.input_mode.clone() {
            InputMode::Normal => self.handle_normal(key),
            InputMode::Filter(value) => self.handle_filter(key, value),
            InputMode::ConfirmDelete(path) => self.handle_delete_confirmation(key, path),
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if self.show_help {
            match key.code {
                KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') => self.show_help = false,
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Home | KeyCode::Char('g') => {
                self.selected = 0;
                self.scroll = 0;
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.selected = self.visible_entries().len().saturating_sub(1);
            }
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') => self.enter_selected(),
            KeyCode::Left | KeyCode::Backspace | KeyCode::Char('h') | KeyCode::Esc => {
                self.go_parent()
            }
            KeyCode::Tab | KeyCode::Char('v') => {
                self.view_mode = match self.view_mode {
                    ViewMode::Tree => ViewMode::List,
                    ViewMode::List => ViewMode::Tree,
                };
                self.selected = 0;
                self.scroll = 0;
                self.status = format!("{} view", self.view_mode.label());
            }
            KeyCode::Char('s') => {
                self.descending = !self.descending;
                self.selected = 0;
                self.status = if self.descending {
                    "Sorted largest first".to_string()
                } else {
                    "Sorted smallest first".to_string()
                };
            }
            KeyCode::Char('f') => self.input_mode = InputMode::Filter(String::new()),
            KeyCode::Char('r') => self.rescan(),
            KeyCode::Char('d') | KeyCode::Delete => self.request_delete(),
            KeyCode::Char('[') => {
                self.top_n = self.top_n.saturating_sub(1).max(1);
                self.status = format!("Showing top {} files", self.top_n);
            }
            KeyCode::Char(']') => {
                self.top_n = (self.top_n + 1).min(50);
                self.status = format!("Showing top {} files", self.top_n);
            }
            _ => {}
        }
    }

    fn handle_filter(&mut self, key: KeyEvent, mut value: String) {
        match key.code {
            KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.status = "Filter unchanged".to_string();
            }
            KeyCode::Enter => match parse_size(&value) {
                Ok(bytes) => {
                    self.filter_bytes = bytes;
                    self.selected = 0;
                    self.scroll = 0;
                    self.input_mode = InputMode::Normal;
                    self.status = if bytes == 0 {
                        "Size filter cleared".to_string()
                    } else {
                        format!("Showing entries at least {}", format_size(bytes))
                    };
                }
                Err(error) => {
                    self.status = error;
                    self.input_mode = InputMode::Filter(value);
                }
            },
            KeyCode::Backspace => {
                value.pop();
                self.input_mode = InputMode::Filter(value);
            }
            KeyCode::Char(ch) => {
                value.push(ch);
                self.input_mode = InputMode::Filter(value);
            }
            _ => self.input_mode = InputMode::Filter(value),
        }
    }

    fn handle_delete_confirmation(&mut self, key: KeyEvent, path: PathBuf) {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                match fs::remove_file(&path) {
                    Ok(()) => {
                        self.status = format!("Deleted {}", path.display());
                        self.rescan_preserving_status();
                    }
                    Err(error) => {
                        self.status = format!("Delete failed: {error}");
                    }
                }
                self.input_mode = InputMode::Normal;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.input_mode = InputMode::Normal;
                self.status = "Deletion cancelled".to_string();
            }
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.visible_entries().len();
        if len == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected as isize + delta).clamp(0, len as isize - 1) as usize;
    }

    fn enter_selected(&mut self) {
        let Some(entry) = self.selected_entry() else {
            return;
        };
        if entry.is_dir {
            self.current_path = entry.path;
            self.selected = 0;
            self.scroll = 0;
            self.status = "Opened directory".to_string();
        } else {
            self.status = format!(
                "File: {} ({})",
                entry.path.display(),
                format_size(entry.size)
            );
        }
    }

    fn go_parent(&mut self) {
        if self.current_path == self.root_path {
            self.status = "Already at scan root".to_string();
            return;
        }
        if let Some(parent) = self.current_path.parent() {
            if parent.starts_with(&self.root_path) {
                self.current_path = parent.to_path_buf();
                self.selected = 0;
                self.scroll = 0;
                self.status = "Moved to parent".to_string();
            }
        }
    }

    fn request_delete(&mut self) {
        match self.selected_entry() {
            Some(entry) if !entry.is_dir => {
                self.status = format!(
                    "Delete {} ({})? y/Enter confirm, n/Esc cancel",
                    entry.path.display(),
                    format_size(entry.size)
                );
                self.input_mode = InputMode::ConfirmDelete(entry.path);
            }
            Some(_) => self.status = "Deletion is limited to files; select a file".to_string(),
            None => self.status = "Nothing selected".to_string(),
        }
    }

    fn rescan(&mut self) {
        let started = Instant::now();
        match Node::scan(&self.root_path) {
            Ok(root) => {
                self.root = root;
                if self.root.find(&self.current_path).is_none() {
                    self.current_path = self.root_path.clone();
                }
                self.clamp_selection();
                self.status = format!("Rescan complete in {:.2?}", started.elapsed());
            }
            Err(error) => self.status = format!("Rescan failed: {error}"),
        }
    }

    fn rescan_preserving_status(&mut self) {
        let status = self.status.clone();
        self.rescan();
        self.status = status;
    }

    pub fn clamp_selection(&mut self) {
        self.selected = self
            .selected
            .min(self.visible_entries().len().saturating_sub(1));
    }

    pub fn ensure_selection_visible(&mut self, height: usize) {
        if height == 0 {
            return;
        }
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + height {
            self.scroll = self.selected + 1 - height;
        }
    }

    pub fn relative_path<'a>(&self, path: &'a Path) -> &'a Path {
        path.strip_prefix(&self.current_path).unwrap_or(path)
    }
}
