//! Application state and all keyboard-driven behaviour.
//!
//! The state is deliberately view-model free of rendering concerns: `ui.rs`
//! reads it, and every mutation here corresponds to a real key press.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;

use ratatui::layout::Rect;

use crate::format;
use crate::scan::{self, Kind, Node, ScanStats};

/// Which pane the arrow keys currently drive. All panes stay on screen; focus
/// only decides where the cursor moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pane {
    Tree,
    Top,
    Map,
}

impl Pane {
    pub fn next(self) -> Pane {
        match self {
            Pane::Tree => Pane::Top,
            Pane::Top => Pane::Map,
            Pane::Map => Pane::Tree,
        }
    }

    pub fn prev(self) -> Pane {
        match self {
            Pane::Tree => Pane::Map,
            Pane::Top => Pane::Tree,
            Pane::Map => Pane::Top,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Pane::Tree => "Directory tree",
            Pane::Top => "Top files",
            Pane::Map => "Treemap",
        }
    }
}

/// Ordering applied to the tree rows and the top list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    SizeDesc,
    SizeAsc,
    NameAsc,
    ItemsDesc,
    ModifiedDesc,
}

impl SortMode {
    pub fn label(self) -> &'static str {
        match self {
            SortMode::SizeDesc => "size ↓",
            SortMode::SizeAsc => "size ↑",
            SortMode::NameAsc => "name A→Z",
            SortMode::ItemsDesc => "items ↓",
            SortMode::ModifiedDesc => "modified ↓",
        }
    }

    pub fn next(self) -> SortMode {
        match self {
            SortMode::SizeDesc => SortMode::SizeAsc,
            SortMode::SizeAsc => SortMode::NameAsc,
            SortMode::NameAsc => SortMode::ItemsDesc,
            SortMode::ItemsDesc => SortMode::ModifiedDesc,
            SortMode::ModifiedDesc => SortMode::SizeDesc,
        }
    }
}

/// What the top list enumerates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopScope {
    /// Every file below the current directory, recursively.
    FilesRecursive,
    /// Only the direct children of the current directory (files and dirs).
    DirectChildren,
}

impl TopScope {
    pub fn label(self) -> &'static str {
        match self {
            TopScope::FilesRecursive => "all files (recursive)",
            TopScope::DirectChildren => "direct children",
        }
    }

    pub fn toggled(self) -> TopScope {
        match self {
            TopScope::FilesRecursive => TopScope::DirectChildren,
            TopScope::DirectChildren => TopScope::FilesRecursive,
        }
    }
}

/// A single-line prompt rendered in the bottom bar. Never an overlay: the rest
/// of the screen keeps every value visible while typing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    None,
    MinSize(String),
    Search(String),
    TopN(String),
    ConfirmDelete { path: PathBuf, is_dir: bool },
}

impl Prompt {
    pub fn is_active(&self) -> bool {
        !matches!(self, Prompt::None)
    }
}

/// Severity of the last status message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Success,
    Warn,
    Error,
}

/// One rendered tree row.
#[derive(Debug, Clone)]
pub struct Row {
    pub path: PathBuf,
    pub name: String,
    pub depth: usize,
    pub size: u64,
    pub kind: Kind,
    pub expanded: bool,
    pub has_children: bool,
    /// Share of the current directory total, used for the inline bar.
    pub share: f64,
    pub items: u64,
    pub error: bool,
}

/// One entry of the top-N list.
#[derive(Debug, Clone)]
pub struct TopRow {
    pub rank: usize,
    pub path: PathBuf,
    /// Path relative to the current directory, so the list is readable.
    pub display: String,
    pub size: u64,
    pub kind: Kind,
    pub share: f64,
}

pub struct App {
    /// Scan root, from argv.
    pub root: PathBuf,
    /// Whole scanned tree.
    pub tree: Node,
    /// Directory currently in scope (header, treemap and top list all use it).
    pub scope: PathBuf,
    pub expanded: HashSet<PathBuf>,
    pub selected: Option<PathBuf>,
    pub focus: Pane,
    pub sort: SortMode,
    pub top_scope: TopScope,
    pub top_n: usize,
    pub min_size: u64,
    pub search: String,
    pub show_help: bool,
    pub prompt: Prompt,
    pub status: String,
    pub level: Level,
    pub stats: ScanStats,
    pub last_scan_ms: u128,
    pub should_quit: bool,
    /// Vertical scroll offsets, kept in sync with the selection while rendering.
    pub tree_offset: usize,
    pub top_selected: usize,
    pub top_offset: usize,
    /// Tile rectangles from the most recent frame, used for spatial treemap
    /// navigation with the arrow keys.
    pub tiles: Vec<(PathBuf, Rect)>,
    pub tree_rows_visible: usize,
    pub top_rows_visible: usize,
}

impl App {
    pub fn new(root: PathBuf) -> io::Result<App> {
        let started = Instant::now();
        let (tree, stats) = scan::scan(&root)?;
        let elapsed = started.elapsed().as_millis();
        let scope = tree.path.clone();
        let mut expanded = HashSet::new();
        expanded.insert(scope.clone());

        let mut app = App {
            root,
            tree,
            scope,
            expanded,
            selected: None,
            focus: Pane::Tree,
            sort: SortMode::SizeDesc,
            top_scope: TopScope::FilesRecursive,
            top_n: 10,
            min_size: 0,
            search: String::new(),
            show_help: false,
            prompt: Prompt::None,
            status: String::new(),
            level: Level::Info,
            stats,
            last_scan_ms: elapsed,
            should_quit: false,
            tree_offset: 0,
            top_selected: 0,
            top_offset: 0,
            tiles: Vec::new(),
            tree_rows_visible: 1,
            top_rows_visible: 1,
        };
        app.expand_first_level();
        app.select_first_row();
        app.set_status(
            Level::Info,
            format!(
                "Scanned {} entries in {} ms — press ? for help",
                app.stats.entries, app.last_scan_ms
            ),
        );
        Ok(app)
    }

    // ---------------------------------------------------------------- queries

    /// The node for the directory in scope (falls back to the root if the scope
    /// vanished from under us).
    pub fn scope_node(&self) -> &Node {
        self.tree.find(&self.scope).unwrap_or(&self.tree)
    }

    pub fn selected_node(&self) -> Option<&Node> {
        self.selected.as_ref().and_then(|p| self.tree.find(p))
    }

    /// Total size of the directory in scope.
    pub fn scope_size(&self) -> u64 {
        self.scope_node().size
    }

    /// Largest direct child of the scope after filtering is ignored — the
    /// "largest item" must report the truth about the filesystem.
    pub fn largest_child(&self) -> Option<&Node> {
        self.scope_node().largest_child()
    }

    /// Largest single file anywhere below the scope.
    pub fn largest_file(&self) -> Option<&Node> {
        let mut files = Vec::new();
        self.scope_node().collect_files(&mut files);
        files.into_iter().max_by_key(|f| f.size)
    }

    pub fn filter_active(&self) -> bool {
        self.min_size > 0 || !self.search.is_empty()
    }

    fn passes_size(&self, node: &Node) -> bool {
        node.size >= self.min_size
    }

    fn matches_search(&self, node: &Node) -> bool {
        if self.search.is_empty() {
            return true;
        }
        let needle = self.search.to_lowercase();
        if node.name.to_lowercase().contains(&needle) {
            return true;
        }
        // Keep directories whose descendants match, otherwise a search could
        // never reach anything nested.
        node.children.iter().any(|c| self.matches_search(c))
    }

    fn visible_child(&self, node: &Node) -> bool {
        self.passes_size(node) && self.matches_search(node)
    }

    /// Direct children of `node` in the current sort order, after filtering.
    fn ordered_children<'a>(&self, node: &'a Node) -> Vec<&'a Node> {
        let mut kids: Vec<&Node> = node
            .children
            .iter()
            .filter(|c| self.visible_child(c))
            .collect();
        match self.sort {
            SortMode::SizeDesc => kids.sort_by(|a, b| scan::cmp_entries(a, b)),
            SortMode::SizeAsc => kids.sort_by(|a, b| scan::cmp_entries(b, a)),
            SortMode::NameAsc => {
                kids.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            }
            SortMode::ItemsDesc => kids.sort_by(|a, b| {
                let ai = a.files_deep + a.dirs_deep;
                let bi = b.files_deep + b.dirs_deep;
                bi.cmp(&ai).then_with(|| scan::cmp_entries(a, b))
            }),
            SortMode::ModifiedDesc => kids.sort_by(|a, b| {
                b.modified
                    .cmp(&a.modified)
                    .then_with(|| scan::cmp_entries(a, b))
            }),
        }
        kids
    }

    /// Flatten the scope into displayable rows, honouring expansion state.
    pub fn rows(&self) -> Vec<Row> {
        let scope = self.scope_node();
        let total = scope.size.max(1);
        let mut out = Vec::new();
        self.push_rows(scope, 0, total, &mut out);
        out
    }

    fn push_rows(&self, node: &Node, depth: usize, total: u64, out: &mut Vec<Row>) {
        for child in self.ordered_children(node) {
            let expanded = child.kind.is_dir() && self.expanded.contains(&child.path);
            out.push(Row {
                path: child.path.clone(),
                name: child.name.clone(),
                depth,
                size: child.size,
                kind: child.kind,
                expanded,
                has_children: child.kind.is_dir() && !child.children.is_empty(),
                share: child.size as f64 / total as f64,
                items: child.files_deep + child.dirs_deep,
                error: child.error.is_some(),
            });
            if expanded {
                self.push_rows(child, depth + 1, total, out);
            }
        }
    }

    /// Top-N entries for the current scope and top-list mode.
    pub fn top_rows(&self) -> Vec<TopRow> {
        let scope = self.scope_node();
        let total = scope.size.max(1);
        let mut candidates: Vec<&Node> = Vec::new();
        match self.top_scope {
            TopScope::FilesRecursive => scope.collect_files(&mut candidates),
            TopScope::DirectChildren => candidates.extend(scope.children.iter()),
        }
        candidates.retain(|n| self.visible_child(n));
        candidates.sort_by(|a, b| match self.sort {
            SortMode::SizeAsc => scan::cmp_entries(b, a),
            SortMode::NameAsc => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            SortMode::ModifiedDesc => b
                .modified
                .cmp(&a.modified)
                .then_with(|| scan::cmp_entries(a, b)),
            _ => scan::cmp_entries(a, b),
        });
        candidates
            .into_iter()
            .take(self.top_n)
            .enumerate()
            .map(|(i, n)| TopRow {
                rank: i + 1,
                path: n.path.clone(),
                display: n
                    .path
                    .strip_prefix(&scope.path)
                    .unwrap_or(&n.path)
                    .to_string_lossy()
                    .to_string(),
                size: n.size,
                kind: n.kind,
                share: n.size as f64 / total as f64,
            })
            .collect()
    }

    /// Direct children used for the treemap, biggest first.
    pub fn map_entries(&self) -> Vec<&Node> {
        let mut kids: Vec<&Node> = self
            .scope_node()
            .children
            .iter()
            .filter(|c| self.visible_child(c))
            .collect();
        kids.sort_by(|a, b| scan::cmp_entries(a, b));
        kids
    }

    // -------------------------------------------------------------- mutations

    pub fn set_status(&mut self, level: Level, msg: impl Into<String>) {
        self.level = level;
        self.status = msg.into();
    }

    fn expand_first_level(&mut self) {
        let dirs: Vec<PathBuf> = self
            .tree
            .children
            .iter()
            .filter(|c| c.kind.is_dir())
            .map(|c| c.path.clone())
            .collect();
        // Only auto-expand when the result stays comfortably browsable.
        if dirs.len() <= 8 {
            for d in dirs {
                self.expanded.insert(d);
            }
        }
    }

    fn select_first_row(&mut self) {
        let rows = self.rows();
        self.selected = rows.first().map(|r| r.path.clone());
        self.tree_offset = 0;
    }

    /// Keep the selection pointing at something that still exists and is visible.
    fn reconcile_selection(&mut self) {
        let rows = self.rows();
        let still_there = self
            .selected
            .as_ref()
            .map(|p| rows.iter().any(|r| &r.path == p))
            .unwrap_or(false);
        if !still_there {
            self.selected = rows.first().map(|r| r.path.clone());
        }
        let top = self.top_rows();
        if self.top_selected >= top.len() {
            self.top_selected = top.len().saturating_sub(1);
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        match self.focus {
            Pane::Tree => self.move_tree(delta),
            Pane::Top => self.move_top(delta),
            Pane::Map => self.move_map_linear(delta),
        }
    }

    fn move_tree(&mut self, delta: isize) {
        let rows = self.rows();
        if rows.is_empty() {
            self.selected = None;
            return;
        }
        let cur = self
            .selected
            .as_ref()
            .and_then(|p| rows.iter().position(|r| &r.path == p))
            .unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, rows.len() as isize - 1) as usize;
        self.selected = Some(rows[next].path.clone());
        self.sync_tree_offset(next, rows.len());
    }

    fn sync_tree_offset(&mut self, index: usize, len: usize) {
        let height = self.tree_rows_visible.max(1);
        if index < self.tree_offset {
            self.tree_offset = index;
        } else if index >= self.tree_offset + height {
            self.tree_offset = index + 1 - height;
        }
        let max_offset = len.saturating_sub(height);
        self.tree_offset = self.tree_offset.min(max_offset);
    }

    fn move_top(&mut self, delta: isize) {
        let rows = self.top_rows();
        if rows.is_empty() {
            return;
        }
        let next = (self.top_selected as isize + delta).clamp(0, rows.len() as isize - 1) as usize;
        self.top_selected = next;
        let height = self.top_rows_visible.max(1);
        if next < self.top_offset {
            self.top_offset = next;
        } else if next >= self.top_offset + height {
            self.top_offset = next + 1 - height;
        }
        // Mirror the highlight into the tree so both panes agree.
        self.selected = Some(rows[next].path.clone());
    }

    fn move_map_linear(&mut self, delta: isize) {
        let entries = self.map_entries();
        if entries.is_empty() {
            return;
        }
        let paths: Vec<PathBuf> = entries.iter().map(|n| n.path.clone()).collect();
        let cur = self
            .selected
            .as_ref()
            .and_then(|p| paths.iter().position(|q| q == p))
            .unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, paths.len() as isize - 1) as usize;
        self.selected = Some(paths[next].clone());
    }

    /// Spatial move inside the treemap using the tiles from the last frame.
    pub fn move_map_spatial(&mut self, dx: isize, dy: isize) {
        if self.tiles.is_empty() {
            self.move_map_linear(if dx + dy > 0 { 1 } else { -1 });
            return;
        }
        let current = match self
            .selected
            .as_ref()
            .and_then(|p| self.tiles.iter().find(|(tp, _)| tp == p))
        {
            Some((_, rect)) => *rect,
            None => {
                self.selected = Some(self.tiles[0].0.clone());
                return;
            }
        };
        let cx = current.x as isize * 2 + current.width as isize;
        let cy = current.y as isize * 2 + current.height as isize;

        let mut best: Option<(isize, PathBuf)> = None;
        for (path, rect) in &self.tiles {
            if Some(path) == self.selected.as_ref() {
                continue;
            }
            let tx = rect.x as isize * 2 + rect.width as isize;
            let ty = rect.y as isize * 2 + rect.height as isize;
            let (along, across) = if dx != 0 {
                ((tx - cx) * dx.signum(), (ty - cy).abs())
            } else {
                ((ty - cy) * dy.signum(), (tx - cx).abs())
            };
            if along <= 0 {
                continue;
            }
            // Prefer near in the travel direction, then aligned across it.
            let score = along + across * 3;
            if best.as_ref().map(|(s, _)| score < *s).unwrap_or(true) {
                best = Some((score, path.clone()));
            }
        }
        if let Some((_, path)) = best {
            self.selected = Some(path);
        }
    }

    pub fn select_first(&mut self) {
        match self.focus {
            Pane::Top => {
                self.top_selected = 0;
                self.top_offset = 0;
                if let Some(row) = self.top_rows().first() {
                    self.selected = Some(row.path.clone());
                }
            }
            _ => {
                let rows = self.rows();
                if let Some(row) = rows.first() {
                    self.selected = Some(row.path.clone());
                    self.tree_offset = 0;
                }
            }
        }
    }

    pub fn select_last(&mut self) {
        match self.focus {
            Pane::Top => {
                let rows = self.top_rows();
                if !rows.is_empty() {
                    self.top_selected = rows.len() - 1;
                    self.selected = Some(rows[self.top_selected].path.clone());
                    self.top_offset = rows.len().saturating_sub(self.top_rows_visible.max(1));
                }
            }
            _ => {
                let rows = self.rows();
                if let Some(row) = rows.last() {
                    self.selected = Some(row.path.clone());
                    let len = rows.len();
                    self.sync_tree_offset(len - 1, len);
                }
            }
        }
    }

    /// Expand the selected directory, or collapse it if already expanded.
    pub fn toggle_expand(&mut self) {
        let Some(node) = self.selected_node() else {
            return;
        };
        if !node.kind.is_dir() {
            self.set_status(
                Level::Warn,
                format!("{} is a file — nothing to expand", node.name),
            );
            return;
        }
        let path = node.path.clone();
        let name = node.name.clone();
        if self.expanded.remove(&path) {
            self.set_status(Level::Info, format!("Collapsed {}", name));
        } else {
            self.expanded.insert(path);
            self.set_status(Level::Info, format!("Expanded {}", name));
        }
        self.reconcile_selection();
    }

    pub fn expand_selected(&mut self) {
        if let Some(node) = self.selected_node() {
            if node.kind.is_dir() && !self.expanded.contains(&node.path) {
                let path = node.path.clone();
                self.expanded.insert(path);
                return;
            }
        }
        self.move_selection(1);
    }

    pub fn collapse_selected(&mut self) {
        let Some(node) = self.selected_node() else {
            return;
        };
        let path = node.path.clone();
        if node.kind.is_dir() && self.expanded.contains(&path) {
            self.expanded.remove(&path);
            return;
        }
        // Jump to the parent row when there is nothing to collapse.
        if let Some(parent) = path.parent() {
            if parent != self.scope.as_path() && parent.starts_with(&self.scope) {
                self.selected = Some(parent.to_path_buf());
                return;
            }
        }
        self.leave_directory();
    }

    /// Descend into the selected directory (or the selected file's directory).
    pub fn enter_selected(&mut self) {
        let Some(node) = self.selected_node() else {
            return;
        };
        if node.kind.is_dir() {
            let path = node.path.clone();
            let name = node.name.clone();
            let size = node.size;
            self.scope = path.clone();
            self.expanded.insert(path);
            self.tree_offset = 0;
            self.top_selected = 0;
            self.top_offset = 0;
            self.select_first_row();
            self.set_status(
                Level::Info,
                format!("Entered {} — {}", name, format::size_dual(size)),
            );
        } else {
            let path = node.path.clone();
            let name = node.name.clone();
            let size = node.size;
            if let Some(parent) = path.parent() {
                if parent != self.scope.as_path() && parent.starts_with(&self.root) {
                    self.scope = parent.to_path_buf();
                    self.expanded.insert(parent.to_path_buf());
                    self.selected = Some(path);
                    self.reconcile_selection();
                    self.set_status(
                        Level::Info,
                        format!("Revealed {} ({})", name, format::human(size)),
                    );
                    return;
                }
            }
            self.set_status(
                Level::Info,
                format!("{} — {} ({} bytes)", name, format::human(size), size),
            );
        }
    }

    /// Move the scope up one directory.
    pub fn leave_directory(&mut self) {
        if self.scope == self.tree.path {
            self.set_status(Level::Warn, "Already at the scan root");
            return;
        }
        let child = self.scope.clone();
        if let Some(parent) = child.parent() {
            self.scope = parent.to_path_buf();
            self.selected = Some(child);
            self.tree_offset = 0;
            self.top_selected = 0;
            self.top_offset = 0;
            self.reconcile_selection();
            let path = self.scope.display().to_string();
            let size = self.scope_size();
            self.set_status(
                Level::Info,
                format!("Up to {} — {}", path, format::size_dual(size)),
            );
        }
    }

    pub fn goto_root(&mut self) {
        self.scope = self.tree.path.clone();
        self.tree_offset = 0;
        self.top_selected = 0;
        self.top_offset = 0;
        self.select_first_row();
        self.set_status(Level::Info, "Back at the scan root");
    }

    pub fn cycle_sort(&mut self) {
        self.sort = self.sort.next();
        self.tree_offset = 0;
        self.top_offset = 0;
        self.top_selected = 0;
        self.reconcile_selection();
        self.set_status(Level::Info, format!("Sorting by {}", self.sort.label()));
    }

    pub fn toggle_top_scope(&mut self) {
        self.top_scope = self.top_scope.toggled();
        self.top_selected = 0;
        self.top_offset = 0;
        self.set_status(
            Level::Info,
            format!("Top list now covers {}", self.top_scope.label()),
        );
    }

    pub fn bump_top_n(&mut self, delta: isize) {
        let next = (self.top_n as isize + delta).clamp(1, 999) as usize;
        self.top_n = next;
        self.top_selected = self.top_selected.min(next.saturating_sub(1));
        self.set_status(Level::Info, format!("Top list shows up to {} items", next));
    }

    pub fn clear_filters(&mut self) {
        if !self.filter_active() {
            self.set_status(Level::Info, "No filters were active");
            return;
        }
        self.min_size = 0;
        self.search.clear();
        self.tree_offset = 0;
        self.top_offset = 0;
        self.reconcile_selection();
        self.set_status(Level::Success, "Cleared size and name filters");
    }

    pub fn collapse_all(&mut self) {
        self.expanded.clear();
        self.expanded.insert(self.scope.clone());
        self.tree_offset = 0;
        self.reconcile_selection();
        self.set_status(Level::Info, "Collapsed every directory");
    }

    /// Expand every directory below the scope. Bounded so a huge tree cannot
    /// wedge the UI.
    pub fn expand_all(&mut self) {
        let mut dirs = Vec::new();
        {
            let mut all = Vec::new();
            self.scope_node().collect_all(&mut all);
            for n in all {
                if n.kind.is_dir() {
                    dirs.push(n.path.clone());
                }
                if dirs.len() >= 5000 {
                    break;
                }
            }
        }
        let count = dirs.len();
        for d in dirs {
            self.expanded.insert(d);
        }
        self.set_status(Level::Info, format!("Expanded {} directories", count));
    }

    /// Rescan the root from disk. Scope, expansion and selection survive when
    /// the corresponding paths still exist.
    pub fn refresh(&mut self) {
        let started = Instant::now();
        match scan::scan(&self.root) {
            Ok((tree, stats)) => {
                self.tree = tree;
                self.stats = stats;
                self.last_scan_ms = started.elapsed().as_millis();
                if self.tree.find(&self.scope).is_none() {
                    self.scope = self.tree.path.clone();
                }
                let scope = self.scope.clone();
                self.expanded.retain(|p| self.tree.find(p).is_some());
                self.expanded.insert(scope);
                self.reconcile_selection();
                self.set_status(
                    Level::Success,
                    format!(
                        "Rescanned {} — {} entries, {} in {} ms",
                        self.root.display(),
                        self.stats.entries,
                        format::size_dual(self.tree.size),
                        self.last_scan_ms
                    ),
                );
            }
            Err(err) => self.set_status(
                Level::Error,
                format!("Rescan of {} failed: {}", self.root.display(), err),
            ),
        }
    }

    // ---------------------------------------------------------------- prompts

    pub fn begin_min_size(&mut self) {
        let current = if self.min_size > 0 {
            format::human(self.min_size)
        } else {
            String::new()
        };
        let _ = current;
        self.prompt = Prompt::MinSize(String::new());
        self.set_status(
            Level::Info,
            "Type a minimum size (e.g. 10MB, 1.5G, 4096) then Enter — Esc cancels",
        );
    }

    pub fn begin_search(&mut self) {
        self.prompt = Prompt::Search(self.search.clone());
        self.set_status(Level::Info, "Type part of a name then Enter — Esc cancels");
    }

    pub fn begin_top_n(&mut self) {
        self.prompt = Prompt::TopN(String::new());
        self.set_status(Level::Info, "How many items should the top list show?");
    }

    pub fn begin_delete(&mut self) {
        let Some(node) = self.selected_node() else {
            self.set_status(Level::Warn, "Nothing selected to delete");
            return;
        };
        if node.path == self.tree.path {
            self.set_status(Level::Error, "Refusing to delete the scan root");
            return;
        }
        let is_dir = node.kind.is_dir();
        let path = node.path.clone();
        let size = node.size;
        let inner = node.files_deep + node.dirs_deep;
        self.prompt = Prompt::ConfirmDelete {
            path: path.clone(),
            is_dir,
        };
        let what = if is_dir {
            format!("directory {} and its {} entries", path.display(), inner)
        } else {
            format!("file {}", path.display())
        };
        self.set_status(
            Level::Warn,
            format!(
                "Delete {} ({})? press y to confirm, n or Esc to cancel",
                what,
                format::human(size)
            ),
        );
    }

    pub fn cancel_prompt(&mut self) {
        if self.prompt.is_active() {
            self.prompt = Prompt::None;
            self.set_status(Level::Info, "Cancelled");
        }
    }

    pub fn prompt_push(&mut self, c: char) {
        match &mut self.prompt {
            Prompt::MinSize(buf) | Prompt::Search(buf) | Prompt::TopN(buf) => buf.push(c),
            _ => {}
        }
    }

    pub fn prompt_pop(&mut self) {
        match &mut self.prompt {
            Prompt::MinSize(buf) | Prompt::Search(buf) | Prompt::TopN(buf) => {
                buf.pop();
            }
            _ => {}
        }
    }

    /// Apply whatever the prompt was collecting.
    pub fn prompt_submit(&mut self) {
        let prompt = std::mem::replace(&mut self.prompt, Prompt::None);
        match prompt {
            Prompt::MinSize(buf) => {
                let text = buf.trim().to_string();
                if text.is_empty() {
                    self.min_size = 0;
                    self.reconcile_selection();
                    self.set_status(Level::Success, "Size filter cleared");
                    return;
                }
                match format::parse_size(&text) {
                    Ok(bytes) => {
                        self.min_size = bytes;
                        self.tree_offset = 0;
                        self.top_offset = 0;
                        self.top_selected = 0;
                        self.reconcile_selection();
                        let shown = self.rows().len();
                        self.set_status(
                            Level::Success,
                            format!(
                                "Showing entries ≥ {} ({} bytes) — {} rows match",
                                format::human(bytes),
                                bytes,
                                shown
                            ),
                        );
                    }
                    Err(err) => {
                        self.set_status(Level::Error, format!("Bad size `{}`: {}", text, err))
                    }
                }
            }
            Prompt::Search(buf) => {
                self.search = buf.trim().to_string();
                self.tree_offset = 0;
                self.top_offset = 0;
                self.top_selected = 0;
                self.reconcile_selection();
                if self.search.is_empty() {
                    self.set_status(Level::Success, "Name filter cleared");
                } else {
                    let shown = self.rows().len();
                    self.set_status(
                        Level::Success,
                        format!("Name filter `{}` — {} rows match", self.search, shown),
                    );
                }
            }
            Prompt::TopN(buf) => {
                let text = buf.trim().to_string();
                match text.parse::<usize>() {
                    Ok(n) if n >= 1 && n <= 999 => {
                        self.top_n = n;
                        self.top_selected = self.top_selected.min(n - 1);
                        self.set_status(
                            Level::Success,
                            format!("Top list shows up to {} items", n),
                        );
                    }
                    _ => self.set_status(
                        Level::Error,
                        format!("`{}` is not a count between 1 and 999", text),
                    ),
                }
            }
            Prompt::ConfirmDelete { path, is_dir } => self.delete_now(&path, is_dir),
            Prompt::None => {}
        }
    }

    /// Perform the real deletion, then reconcile the in-memory tree with disk.
    fn delete_now(&mut self, path: &Path, is_dir: bool) {
        let anonymous = self.tree.find(path).map(|n| n.size).unwrap_or(0);
        let result = if is_dir {
            fs::remove_dir_all(path)
        } else {
            fs::remove_file(path)
        };
        match result {
            Ok(()) => {
                self.forget(path);
                let scope_still_there = self.tree.find(&self.scope).is_some();
                if !scope_still_there {
                    let mut candidate = self.scope.clone();
                    while candidate != self.tree.path && self.tree.find(&candidate).is_none() {
                        match candidate.parent() {
                            Some(parent) => candidate = parent.to_path_buf(),
                            None => break,
                        }
                    }
                    self.scope = if self.tree.find(&candidate).is_some() {
                        candidate
                    } else {
                        self.tree.path.clone()
                    };
                }
                self.expanded.retain(|p| self.tree.find(p).is_some());
                let scope = self.scope.clone();
                self.expanded.insert(scope);
                self.reconcile_selection();
                self.set_status(
                    Level::Success,
                    format!(
                        "Deleted {} — anonymous {} ({} bytes); directory now {}",
                        path.display(),
                        format::human(anonymous),
                        anonymous,
                        format::size_dual(self.scope_size())
                    ),
                );
            }
            Err(err) => self.set_status(
                Level::Error,
                format!("Could not delete {}: {}", path.display(), err),
            ),
        }
    }

    /// Drop `path` from the in-memory tree and fix every ancestor aggregate.
    fn forget(&mut self, path: &Path) {
        let Some(parent) = path.parent() else { return };
        let name = match path.file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => return,
        };
        if let Some(parent_node) = self.tree.find_mut(parent) {
            parent_node.children.retain(|c| c.name != name);
        }
        // Walk back up to the root recomputing sizes and counts.
        let mut cursor = Some(parent.to_path_buf());
        while let Some(dir) = cursor {
            if let Some(node) = self.tree.find_mut(&dir) {
                node.recompute();
            }
            if dir == self.tree.path {
                break;
            }
            cursor = dir.parent().map(|p| p.to_path_buf());
            if let Some(next) = &cursor {
                if !next.starts_with(&self.tree.path) {
                    break;
                }
            }
        }
        self.tiles.retain(|(p, _)| self.tree.find(p).is_some());
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        if self.show_help {
            self.set_status(Level::Info, "Key reference open — press ? or F1 to close");
        } else {
            self.set_status(Level::Info, "Key reference closed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn fixture(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tooli-app-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("logs")).unwrap();
        fs::create_dir_all(dir.join("cache/inner")).unwrap();
        write(&dir.join("huge.bin"), 40_000);
        write(&dir.join("small.txt"), 100);
        write(&dir.join("logs/app.log"), 20_000);
        write(&dir.join("logs/old.log"), 5_000);
        write(&dir.join("cache/inner/blob.dat"), 9_000);
        dir
    }

    fn write(path: &Path, bytes: usize) {
        let mut f = File::create(path).unwrap();
        f.write_all(&vec![b'x'; bytes]).unwrap();
    }

    #[test]
    fn header_facts_match_the_filesystem() {
        let dir = fixture("header");
        let app = App::new(dir.clone()).unwrap();
        assert_eq!(app.scope_size(), 74_100);
        assert_eq!(app.scope_node().direct_files(), 2);
        assert_eq!(app.scope_node().direct_dirs(), 2);
        assert_eq!(app.largest_child().unwrap().name, "huge.bin");
        assert_eq!(app.largest_file().unwrap().name, "huge.bin");
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn top_list_is_sorted_and_limited() {
        let dir = fixture("top");
        let mut app = App::new(dir.clone()).unwrap();
        let rows = app.top_rows();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[0].display, "huge.bin");
        assert_eq!(rows[1].size, 20_000);
        assert!(rows.windows(2).all(|w| w[0].size >= w[1].size));

        // 10 -> 7, but only 5 files exist, so the list is capped by reality.
        app.bump_top_n(-3);
        assert_eq!(app.top_n, 7);
        assert_eq!(app.top_rows().len(), 5);

        app.top_scope = TopScope::DirectChildren;
        let direct = app.top_rows();
        assert!(direct.iter().any(|r| r.display == "logs"));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn size_filter_hides_small_entries() {
        let dir = fixture("filter");
        let mut app = App::new(dir.clone()).unwrap();
        app.prompt = Prompt::MinSize("10KB".to_string());
        app.prompt_submit();
        assert_eq!(app.min_size, 10 * format::KIB);
        let names: Vec<String> = app.rows().into_iter().map(|r| r.name).collect();
        assert!(names.contains(&"huge.bin".to_string()));
        assert!(names.contains(&"logs".to_string()));
        assert!(!names.contains(&"small.txt".to_string()));
        assert!(!names.contains(&"old.log".to_string()));

        app.clear_filters();
        let names: Vec<String> = app.rows().into_iter().map(|r| r.name).collect();
        assert!(names.contains(&"small.txt".to_string()));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bad_filter_input_reports_an_error_and_changes_nothing() {
        let dir = fixture("badfilter");
        let mut app = App::new(dir.clone()).unwrap();
        app.prompt = Prompt::MinSize("bogus".to_string());
        app.prompt_submit();
        assert_eq!(app.min_size, 0);
        assert_eq!(app.level, Level::Error);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn delete_removes_the_file_and_updates_every_total() {
        let dir = fixture("delete");
        let mut app = App::new(dir.clone()).unwrap();
        app.selected = Some(dir.join("logs/app.log"));
        app.begin_delete();
        assert!(matches!(app.prompt, Prompt::ConfirmDelete { .. }));
        app.prompt_submit();

        assert_eq!(app.level, Level::Success, "status was {}", app.status);
        assert!(!dir.join("logs/app.log").exists(), "file still on disk");
        assert_eq!(app.scope_size(), 74_100 - 20_000);
        let logs = app.tree.find(&dir.join("logs")).unwrap();
        assert_eq!(logs.size, 5_000);
        assert_eq!(logs.files_deep, 1);

        // A fresh scan agrees with the incremental update.
        app.refresh();
        assert_eq!(app.scope_size(), 54_100);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn delete_directory_is_recursive_and_rescopes() {
        let dir = fixture("deletedir");
        let mut app = App::new(dir.clone()).unwrap();
        app.selected = Some(dir.join("cache"));
        app.enter_selected();
        assert_eq!(app.scope, dir.join("cache"));

        app.selected = Some(dir.join("cache"));
        app.begin_delete();
        app.prompt_submit();
        assert!(!dir.join("cache").exists());
        assert_eq!(app.scope, dir, "scope should fall back to a live directory");
        assert_eq!(app.scope_size(), 74_100 - 9_000);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn root_cannot_be_deleted() {
        let dir = fixture("rootguard");
        let mut app = App::new(dir.clone()).unwrap();
        app.selected = Some(dir.clone());
        app.begin_delete();
        assert_eq!(app.prompt, Prompt::None);
        assert_eq!(app.level, Level::Error);
        assert!(dir.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn navigation_moves_scope_and_selection() {
        let dir = fixture("nav");
        let mut app = App::new(dir.clone()).unwrap();
        app.selected = Some(dir.join("logs"));
        app.enter_selected();
        assert_eq!(app.scope, dir.join("logs"));
        assert_eq!(app.scope_node().direct_files(), 2);

        app.leave_directory();
        assert_eq!(app.scope, dir);
        assert_eq!(app.selected, Some(dir.join("logs")));

        app.move_selection(1);
        assert!(app.selected.is_some());
        app.select_last();
        let last = app.selected.clone();
        app.select_first();
        assert_ne!(last, app.selected);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sorting_changes_row_order() {
        let dir = fixture("sort");
        let mut app = App::new(dir.clone()).unwrap();
        let by_size: Vec<String> = app.rows().into_iter().map(|r| r.name).collect();
        app.sort = SortMode::NameAsc;
        let by_name: Vec<String> = app.rows().into_iter().map(|r| r.name).collect();
        assert_ne!(by_size, by_name);

        // Ordering applies per level, so compare the top level only.
        let top_level: Vec<String> = app
            .rows()
            .into_iter()
            .filter(|r| r.depth == 0)
            .map(|r| r.name)
            .collect();
        let mut expected = top_level.clone();
        expected.sort_by_key(|s| s.to_lowercase());
        assert_eq!(top_level, expected);

        app.sort = SortMode::SizeAsc;
        let asc: Vec<u64> = app
            .rows()
            .into_iter()
            .filter(|r| r.depth == 0)
            .map(|r| r.size)
            .collect();
        let mut sorted = asc.clone();
        sorted.sort();
        assert_eq!(asc, sorted);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn search_keeps_parents_of_matches() {
        let dir = fixture("search");
        let mut app = App::new(dir.clone()).unwrap();
        app.prompt = Prompt::Search("blob".to_string());
        app.prompt_submit();
        app.expand_all();
        let names: Vec<String> = app.rows().into_iter().map(|r| r.name).collect();
        assert!(names.contains(&"cache".to_string()));
        assert!(names.contains(&"blob.dat".to_string()));
        assert!(!names.contains(&"huge.bin".to_string()));
        fs::remove_dir_all(&dir).unwrap();
    }
}
