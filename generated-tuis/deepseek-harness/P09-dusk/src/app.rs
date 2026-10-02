//! Application state and event handling.

use std::fs;
use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::model::{Model, Node, SortMode};
use crate::scan::{find_node_by_path, scan_subtree};
use crate::size::parse_size;

/// Which panel currently has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    List,
    Treemap,
}

/// The modal state of the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Normal browsing.
    Normal,
    /// Typing a size threshold for the filter.
    Filter,
    /// Confirming a deletion.
    ConfirmDelete,
}

pub struct App {
    pub model: Model,
    /// Index into `visible` of the highlighted entry.
    pub selected: usize,
    /// Node ids of the current directory's children after sort + filter.
    pub visible: Vec<usize>,
    pub sort_mode: SortMode,
    /// Minimum size in bytes for an entry to be shown (0 = no filter).
    pub filter_bytes: u64,
    pub focus: Focus,
    pub help_visible: bool,
    pub input: String,
    pub input_mode: InputMode,
    /// Transient one-shot status message shown in the footer.
    pub message: Option<String>,
    pub quit: bool,
    /// Number of rows in the top-N panel.
    pub top_n: usize,
}

impl App {
    /// Build the app by scanning `root` recursively.
    pub fn new(root: PathBuf) -> Self {
        let name = root
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| root.display().to_string());

        let mut nodes = Vec::new();
        let (root_id, _) = scan_subtree(&root, &name, true, None, &mut nodes);

        let mut app = App {
            model: Model {
                nodes,
                root_id,
                current_id: root_id,
            },
            selected: 0,
            visible: Vec::new(),
            sort_mode: SortMode::SizeDesc,
            filter_bytes: 0,
            focus: Focus::List,
            help_visible: false,
            input: String::new(),
            input_mode: InputMode::Normal,
            message: Some("Welcome — press h or ? for help".to_string()),
            quit: false,
            top_n: 5,
        };
        app.recompute_visible();
        app
    }

    /// Current directory node.
    pub fn current(&self) -> &Node {
        &self.model.nodes[self.model.current_id]
    }

    /// Rebuild `visible` from the current directory applying filter + sort.
    pub fn recompute_visible(&mut self) {
        let cur = self.model.current_id;
        let mut visible: Vec<usize> = self
            .model
            .nodes
            .get(cur)
            .map(|n| {
                n.children
                    .iter()
                    .copied()
                    .filter(|&c| self.model.nodes[c].size >= self.filter_bytes)
                    .collect()
            })
            .unwrap_or_default();

        match self.sort_mode {
            SortMode::SizeDesc => {
                visible.sort_by(|&a, &b| self.model.nodes[b].size.cmp(&self.model.nodes[a].size))
            }
            SortMode::SizeAsc => {
                visible.sort_by(|&a, &b| self.model.nodes[a].size.cmp(&self.model.nodes[b].size))
            }
            SortMode::NameAsc => visible.sort_by(|&a, &b| {
                self.model.nodes[a]
                    .name
                    .to_lowercase()
                    .cmp(&self.model.nodes[b].name.to_lowercase())
            }),
        }

        self.visible = visible;
        if self.selected >= self.visible.len() {
            self.selected = self.visible.len().saturating_sub(1);
        }
    }

    /// Rescan the whole tree from the root, restoring the current directory by
    /// path. Used for refresh and after deletions so the screen always matches
    /// the real filesystem.
    pub fn refresh(&mut self) {
        let root_path = self.model.nodes[self.model.root_id].path.clone();
        let cur_path = self.model.nodes[self.model.current_id].path.clone();

        let name = root_path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| root_path.display().to_string());

        let mut nodes = Vec::new();
        let (root_id, _) = scan_subtree(&root_path, &name, true, None, &mut nodes);
        self.model.nodes = nodes;
        self.model.root_id = root_id;
        self.model.current_id =
            find_node_by_path(&self.model.nodes, root_id, &cur_path).unwrap_or(root_id);
        self.recompute_visible();
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.visible.len();
        if len == 0 {
            self.selected = 0;
            return;
        }
        let cur = self.selected as isize;
        let next = (cur + delta).clamp(0, len as isize - 1);
        self.selected = next as usize;
    }

    fn enter_selected(&mut self) {
        if self.visible.is_empty() {
            return;
        }
        let id = self.visible[self.selected];
        if self.model.nodes[id].is_dir {
            self.model.current_id = id;
            self.selected = 0;
            self.recompute_visible();
        } else {
            self.message = Some(format!("'{}' is a file", self.model.nodes[id].name));
        }
    }

    fn go_up(&mut self) {
        if let Some(parent) = self.model.nodes[self.model.current_id].parent {
            let came_from = self.model.current_id;
            self.model.current_id = parent;
            self.recompute_visible();
            if let Some(pos) = self.visible.iter().position(|&c| c == came_from) {
                self.selected = pos;
            } else {
                self.selected = 0;
            }
        } else {
            self.message = Some("Already at the scan root".to_string());
        }
    }

    fn cycle_sort(&mut self) {
        self.sort_mode = self.sort_mode.next();
        self.recompute_visible();
        self.message = Some(format!("Sort: {}", self.sort_mode.label()));
    }

    fn apply_filter(&mut self) {
        let input = self.input.trim().to_string();
        if input.is_empty() {
            self.filter_bytes = 0;
            self.message = Some("Filter cleared".to_string());
        } else {
            match parse_size(&input) {
                Some(bytes) => {
                    self.filter_bytes = bytes;
                    self.message = Some(format!("Filter: entries >= {}", crate::size::format_size(bytes)));
                }
                None => {
                    self.message = Some(format!("Invalid size: '{}' (try 100M, 2G, 500KB)", input));
                }
            }
        }
        self.recompute_visible();
    }

    fn start_delete(&mut self) {
        if self.visible.is_empty() {
            self.message = Some("Nothing selected to delete".to_string());
            return;
        }
        self.input_mode = InputMode::ConfirmDelete;
    }

    /// Perform the actual filesystem deletion of the selected entry.
    fn do_delete(&mut self) {
        if self.visible.is_empty() {
            return;
        }
        let id = self.visible[self.selected];
        let path = self.model.nodes[id].path.clone();
        let is_dir = self.model.nodes[id].is_dir;
        let name = self.model.nodes[id].name.clone();

        let result = if is_dir {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };

        match result {
            Ok(()) => {
                self.message = Some(format!("Deleted '{}'", name));
                self.refresh();
            }
            Err(e) => {
                self.message = Some(format!("Delete failed: {}", e));
            }
        }
    }

    /// Handle a single key press, returning `true` when the app should quit.
    pub fn handle_key(&mut self, key: KeyEvent) {
        self.message = None;

        match self.input_mode {
            InputMode::Filter => {
                match key.code {
                    KeyCode::Enter => {
                        self.apply_filter();
                        self.input.clear();
                        self.input_mode = InputMode::Normal;
                    }
                    KeyCode::Esc => {
                        self.input.clear();
                        self.input_mode = InputMode::Normal;
                    }
                    KeyCode::Char(c) => self.input.push(c),
                    KeyCode::Backspace => {
                        self.input.pop();
                    }
                    _ => {}
                }
                return;
            }
            InputMode::ConfirmDelete => {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => {
                        self.input_mode = InputMode::Normal;
                        self.do_delete();
                    }
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        self.input_mode = InputMode::Normal;
                    }
                    _ => {}
                }
                return;
            }
            InputMode::Normal => {}
        }

        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => self.quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Enter => self.enter_selected(),
            KeyCode::Left | KeyCode::Backspace | KeyCode::Esc => self.go_up(),
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::List => Focus::Treemap,
                    Focus::Treemap => Focus::List,
                };
            }
            KeyCode::Char('s') => self.cycle_sort(),
            KeyCode::Char('f') => {
                self.input.clear();
                self.input_mode = InputMode::Filter;
            }
            KeyCode::Char('d') => self.start_delete(),
            KeyCode::Char('r') => {
                self.refresh();
                self.message = Some("Rescanned".to_string());
            }
            KeyCode::Char('h') | KeyCode::Char('?') => self.help_visible = !self.help_visible,
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Convenience: grow the top-N list.
                self.top_n = (self.top_n + 1).min(15);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_root() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tooli_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn scans_sorts_filters_navigates_and_deletes() {
        let root = tmp_root();
        fs::create_dir_all(root.join("sub1")).unwrap();
        fs::create_dir_all(root.join("sub2")).unwrap();
        fs::write(root.join("big.bin"), vec![0u8; 3_000_000]).unwrap();
        fs::write(root.join("sub1/med.bin"), vec![0u8; 1_000_000]).unwrap();
        fs::write(root.join("sub1/small.txt"), b"hello").unwrap();
        fs::write(root.join("sub2/note.txt"), b"world").unwrap();

        let mut app = App::new(root.clone());

        // Total = 3,000,000 + 1,000,000 + 5 + 5 = 4,000,010.
        assert_eq!(app.current().size, 4_000_010);

        // Direct children counts: one file, two directories.
        let files = app
            .current()
            .children
            .iter()
            .filter(|&&c| !app.model.nodes[c].is_dir)
            .count();
        assert_eq!(files, 1);
        assert_eq!(app.current().children.len() - files, 2);

        // Default sort (size desc) puts big.bin first.
        assert_eq!(app.model.nodes[app.visible[0]].name, "big.bin");

        // Filter >= 2 MB leaves only big.bin.
        app.filter_bytes = 2_000_000;
        app.recompute_visible();
        assert_eq!(app.visible.len(), 1);
        assert_eq!(app.model.nodes[app.visible[0]].name, "big.bin");

        // Clear filter and navigate into sub1, then back up.
        app.filter_bytes = 0;
        app.recompute_visible();
        let sub1 = app
            .visible
            .iter()
            .position(|&c| app.model.nodes[c].name == "sub1")
            .unwrap();
        app.selected = sub1;
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert_eq!(app.current().name, "sub1");
        app.handle_key(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE));
        assert_eq!(
            app.current().name,
            root.file_name().unwrap().to_string_lossy().to_string()
        );

        // Delete big.bin for real through the keyboard flow.
        let big = app
            .visible
            .iter()
            .position(|&c| app.model.nodes[c].name == "big.bin")
            .unwrap();
        app.selected = big;
        app.handle_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::NONE));
        assert_eq!(app.input_mode, InputMode::ConfirmDelete);
        app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert!(!root.join("big.bin").exists());
        // Rescan after deletion updated the total.
        assert_eq!(app.current().size, 1_000_010);
    }
}
