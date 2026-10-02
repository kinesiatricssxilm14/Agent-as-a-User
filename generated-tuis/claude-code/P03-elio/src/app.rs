//! Application state and the keyboard state machine.
//!
//! All prompts and confirmations live on dedicated bottom lines rather than in
//! floating overlays, so the file list and the preview stay visible on the same
//! screen while the user types a destination path or answers a confirmation.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::fs_ops::{self, Outcome, Target};
use crate::listing::Listing;
use crate::preview::Preview;

/// What the bottom input line is currently collecting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptKind {
    Copy,
    Move,
    Rename,
    MkDir,
    Jump,
    Filter,
}

impl PromptKind {
    pub fn title(&self) -> &'static str {
        match self {
            PromptKind::Copy => "Copy to",
            PromptKind::Move => "Move to",
            PromptKind::Rename => "Rename to",
            PromptKind::MkDir => "New directory",
            PromptKind::Jump => "Go to directory",
            PromptKind::Filter => "Filter",
        }
    }

    pub fn hint(&self) -> &'static str {
        match self {
            PromptKind::Copy => {
                "destination file or existing directory (relative to cwd or absolute)"
            }
            PromptKind::Move => "destination directory or new path",
            PromptKind::Rename => "new name, or a path to move+rename in one step",
            PromptKind::MkDir => "directory name or path; parents are created as needed",
            PromptKind::Jump => "directory to open",
            PromptKind::Filter => "substring match on the current directory; Esc clears",
        }
    }

    /// True when the prompt updates the view on every keystroke.
    pub fn is_live(&self) -> bool {
        matches!(self, PromptKind::Filter)
    }
}

/// An in-progress text prompt with a movable cursor.
#[derive(Debug)]
pub struct Prompt {
    pub kind: PromptKind,
    pub input: String,
    /// Cursor position as a character index into `input`.
    pub cursor: usize,
    /// Path the prompt acts on (the selected entry when the prompt opened).
    pub subject: Option<PathBuf>,
}

impl Prompt {
    fn new(kind: PromptKind, initial: String, subject: Option<PathBuf>) -> Self {
        let cursor = initial.chars().count();
        Self {
            kind,
            input: initial,
            cursor,
            subject,
        }
    }

    fn byte_index(&self, char_index: usize) -> usize {
        self.input
            .char_indices()
            .nth(char_index)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len())
    }

    pub fn insert(&mut self, ch: char) {
        let at = self.byte_index(self.cursor);
        self.input.insert(at, ch);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let at = self.byte_index(self.cursor - 1);
        self.input.remove(at);
        self.cursor -= 1;
    }

    pub fn delete(&mut self) {
        if self.cursor < self.input.chars().count() {
            let at = self.byte_index(self.cursor);
            self.input.remove(at);
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.input.chars().count());
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.input.chars().count();
    }

    pub fn clear(&mut self) {
        self.input.clear();
        self.cursor = 0;
    }

    /// Delete the trailing path segment, like readline's `M-Backspace`.
    pub fn kill_segment(&mut self) {
        let head: String = self.input.chars().take(self.cursor).collect();
        let tail: String = self.input.chars().skip(self.cursor).collect();
        let trimmed = head.trim_end_matches('/');
        let cut = match trimmed.rfind('/') {
            Some(i) => &trimmed[..=i],
            None => "",
        };
        self.cursor = cut.chars().count();
        self.input = format!("{cut}{tail}");
    }
}

/// A destructive action awaiting a y/n answer on the confirmation line.
#[derive(Debug)]
pub enum Confirm {
    Delete {
        path: PathBuf,
        is_dir: bool,
    },
    Overwrite {
        action: PendingAction,
        dest: PathBuf,
    },
}

/// The copy/move that will be retried with `overwrite = true` if confirmed.
#[derive(Debug, Clone)]
pub enum PendingAction {
    Copy { src: PathBuf, target: Target },
    Move { src: PathBuf, target: Target },
}

/// Severity of the status message, which drives its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct Status {
    pub text: String,
    pub level: Level,
}

impl Status {
    fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            level: Level::Info,
        }
    }
    fn success(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            level: Level::Success,
        }
    }
    fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            level: Level::Error,
        }
    }
}

/// Which pane has keyboard focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    List,
    Preview,
}

pub struct App {
    pub listing: Listing,
    pub selected: usize,
    /// First visible row of the list, kept in sync with `selected` during render.
    pub list_offset: usize,
    pub preview: Preview,
    pub focus: Focus,
    pub prompt: Option<Prompt>,
    pub confirm: Option<Confirm>,
    pub status: Status,
    pub show_help: bool,
    /// First visible row of the help column.
    pub help_scroll: u16,
    /// Largest useful `help_scroll`, recomputed by the renderer each frame.
    pub help_max_scroll: u16,
    pub should_quit: bool,
    /// Cursor position remembered per directory, so going up restores the row.
    cursor_memory: HashMap<PathBuf, String>,
    /// Height of the preview viewport from the last render, for PgUp/PgDn.
    pub preview_viewport: u16,
    /// Height of the list viewport from the last render, for PgUp/PgDn.
    pub list_viewport: u16,
    /// Log of completed operations, newest last, shown in the activity pane.
    pub activity: Vec<String>,
    pub show_activity: bool,
}

impl App {
    pub fn new(dir: PathBuf, show_hidden: bool) -> Self {
        let listing = Listing::new(dir, show_hidden);
        let opening_error = listing.error.clone();
        let mut app = Self {
            listing,
            selected: 0,
            list_offset: 0,
            preview: Preview::default(),
            focus: Focus::List,
            prompt: None,
            confirm: None,
            status: Status::info("Press ? for help · arrows/jk to browse · Enter to open"),
            show_help: false,
            help_scroll: 0,
            help_max_scroll: 0,
            should_quit: false,
            cursor_memory: HashMap::new(),
            preview_viewport: 20,
            list_viewport: 20,
            activity: Vec::new(),
            show_activity: false,
        };
        if let Some(err) = opening_error {
            app.status = Status::error(err);
        }
        app.refresh_preview();
        app
    }

    pub fn cwd(&self) -> &Path {
        &self.listing.dir
    }

    pub fn selected_entry(&self) -> Option<&crate::listing::Entry> {
        self.listing.get(self.selected)
    }

    /// Rebuild the preview for the current selection.
    pub fn refresh_preview(&mut self) {
        let wrap = self.preview.wrap;
        match self.listing.get(self.selected) {
            Some(entry) => {
                let same = self.preview.path.as_deref() == Some(entry.path.as_path());
                let mut preview = Preview::load(entry, wrap);
                if same {
                    // Keep the scroll position when the same file is reloaded.
                    preview.scroll = self
                        .preview
                        .scroll
                        .min(preview.max_scroll(self.preview_viewport));
                    preview.hscroll = self.preview.hscroll;
                }
                self.preview = preview;
            }
            None => {
                let reason = if self.listing.error.is_some() {
                    "Directory could not be read."
                } else if self.listing.total() == 0 {
                    "This directory is empty. Press n to create a directory here."
                } else {
                    "No entry matches the filter. Press Esc to clear it."
                };
                self.preview = Preview::empty_selection(reason);
                self.preview.wrap = wrap;
            }
        }
    }

    fn clamp_selection(&mut self) {
        if self.listing.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.listing.len() {
            self.selected = self.listing.len() - 1;
        }
    }

    fn remember_cursor(&mut self) {
        if let Some(entry) = self.listing.get(self.selected) {
            let dir = self.listing.dir.clone();
            let name = entry.name.clone();
            self.cursor_memory.insert(dir, name);
        }
    }

    // ---- navigation ----------------------------------------------------

    pub fn select_next(&mut self, step: usize) {
        if self.listing.is_empty() {
            return;
        }
        self.selected = (self.selected + step).min(self.listing.len() - 1);
        self.refresh_preview();
    }

    pub fn select_prev(&mut self, step: usize) {
        self.selected = self.selected.saturating_sub(step);
        self.refresh_preview();
    }

    pub fn select_first(&mut self) {
        self.selected = 0;
        self.refresh_preview();
    }

    pub fn select_last(&mut self) {
        if !self.listing.is_empty() {
            self.selected = self.listing.len() - 1;
        }
        self.refresh_preview();
    }

    /// Enter the selected directory, or report that a file cannot be entered.
    pub fn open_selected(&mut self) {
        let Some(entry) = self.listing.get(self.selected) else {
            return;
        };
        if entry.is_dir_like() {
            let path = entry.path.clone();
            self.change_dir(path);
        } else {
            let name = entry.name.clone();
            self.status = Status::info(format!(
                "{name} is a file — its full content is shown in the preview pane (Tab to scroll it)"
            ));
        }
    }

    pub fn go_parent(&mut self) {
        let current = self.listing.dir.clone();
        let Some(parent) = current.parent().map(Path::to_path_buf) else {
            self.status = Status::info("already at the filesystem root");
            return;
        };
        self.remember_cursor();
        let leaving = current
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        self.change_dir(parent);
        // Land the cursor on the directory we just came out of.
        if let Some(name) = leaving {
            if let Some(index) = self.listing.index_of_name(&name) {
                self.selected = index;
                self.refresh_preview();
            }
        }
    }

    pub fn change_dir(&mut self, dir: PathBuf) {
        if dir == self.listing.dir {
            return;
        }
        match std::fs::read_dir(&dir) {
            Ok(_) => {}
            Err(err) => {
                self.status = Status::error(format!("cannot open {}: {err}", dir.display()));
                return;
            }
        }
        self.remember_cursor();
        let show_hidden = self.listing.show_hidden;
        let sort_key = self.listing.sort_key;
        let reverse = self.listing.sort_reverse;
        let mut listing = Listing::new(dir.clone(), show_hidden);
        listing.sort_key = sort_key;
        listing.sort_reverse = reverse;
        listing.set_sort(sort_key);
        if reverse != listing.sort_reverse {
            listing.toggle_reverse();
        }
        self.listing = listing;
        self.selected = self
            .cursor_memory
            .get(&dir)
            .and_then(|name| self.listing.index_of_name(name))
            .unwrap_or(0);
        self.list_offset = 0;
        self.focus = Focus::List;
        self.refresh_preview();
        self.status = Status::info(format!("{}", dir.display()));
    }

    /// Re-read the current directory from disk, keeping the cursor on the same name.
    pub fn reload(&mut self) {
        let name = self.listing.get(self.selected).map(|e| e.name.clone());
        self.listing.reload();
        self.clamp_selection();
        if let Some(name) = name {
            if let Some(index) = self.listing.index_of_name(&name) {
                self.selected = index;
            }
        }
        self.refresh_preview();
    }

    /// Reload and place the cursor on `name` (used after create/copy/rename).
    fn reload_and_select(&mut self, name: &str) {
        self.listing.reload();
        if let Some(index) = self.listing.index_of_name(name) {
            self.selected = index;
        } else {
            self.clamp_selection();
        }
        self.refresh_preview();
    }

    // ---- view toggles --------------------------------------------------

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::List => Focus::Preview,
            Focus::Preview => Focus::List,
        };
        self.status = Status::info(match self.focus {
            Focus::List => "focus: file list — up/down moves the selection",
            Focus::Preview => "focus: preview — up/down scrolls the content",
        });
    }

    pub fn toggle_hidden(&mut self) {
        let show = !self.listing.show_hidden;
        let name = self.listing.get(self.selected).map(|e| e.name.clone());
        self.listing.set_hidden(show);
        self.selected = name
            .and_then(|n| self.listing.index_of_name(&n))
            .unwrap_or(0);
        self.clamp_selection();
        self.refresh_preview();
        self.status = Status::info(if show {
            "hidden entries shown"
        } else {
            "hidden entries concealed"
        });
    }

    pub fn cycle_sort(&mut self) {
        let name = self.listing.get(self.selected).map(|e| e.name.clone());
        let next = self.listing.sort_key.next();
        self.listing.set_sort(next);
        self.restore_cursor(name);
        self.status = Status::info(format!("sorted by {}", next.label()));
    }

    pub fn toggle_sort_direction(&mut self) {
        let name = self.listing.get(self.selected).map(|e| e.name.clone());
        self.listing.toggle_reverse();
        self.restore_cursor(name);
        self.status = Status::info(format!(
            "sorted by {} ({})",
            self.listing.sort_key.label(),
            if self.listing.sort_reverse {
                "descending"
            } else {
                "ascending"
            }
        ));
    }

    fn restore_cursor(&mut self, name: Option<String>) {
        self.selected = name
            .and_then(|n| self.listing.index_of_name(&n))
            .unwrap_or(0);
        self.clamp_selection();
        self.refresh_preview();
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        self.help_scroll = 0;
        if self.show_help {
            self.status =
                Status::info("help open — Ctrl-↑/Ctrl-↓ scrolls the key list · ? or Esc closes it");
        }
    }

    /// Scroll the help column. `max` is supplied by the renderer, which knows
    /// how many wrapped rows the panel actually produced.
    pub fn scroll_help(&mut self, delta: i32) {
        let next = self.help_scroll as i32 + delta;
        self.help_scroll = next.clamp(0, self.help_max_scroll as i32) as u16;
    }

    pub fn toggle_activity(&mut self) {
        self.show_activity = !self.show_activity;
    }

    // ---- prompts -------------------------------------------------------

    /// Open a prompt, pre-filling a sensible default for the selected entry.
    pub fn open_prompt(&mut self, kind: PromptKind) {
        let entry = self.listing.get(self.selected);
        let needs_selection = matches!(
            kind,
            PromptKind::Copy | PromptKind::Move | PromptKind::Rename
        );
        if needs_selection && entry.is_none() {
            self.status = Status::error("nothing selected — no entry to act on");
            return;
        }
        let subject = entry.map(|e| e.path.clone());
        let initial = match kind {
            PromptKind::Rename => entry.map(|e| e.name.clone()).unwrap_or_default(),
            PromptKind::Filter => self.listing.filter.clone(),
            _ => String::new(),
        };
        self.status = Status::info(format!("{}: {}", kind.title(), kind.hint()));
        self.prompt = Some(Prompt::new(kind, initial, subject));
    }

    pub fn cancel_prompt(&mut self) {
        if let Some(prompt) = self.prompt.take() {
            if prompt.kind == PromptKind::Filter && !self.listing.filter.is_empty() {
                self.listing.filter.clear();
                self.listing.refilter();
                self.clamp_selection();
                self.refresh_preview();
            }
            self.status = Status::info(format!("{} cancelled", prompt.kind.title()));
        }
    }

    /// Called on every keystroke while a live prompt (filter) is open.
    pub fn prompt_changed(&mut self) {
        let Some(prompt) = &self.prompt else { return };
        if prompt.kind == PromptKind::Filter {
            self.listing.filter = prompt.input.clone();
            self.listing.refilter();
            self.selected = 0;
            self.refresh_preview();
        }
    }

    /// Complete the directory component of the prompt input against the real
    /// filesystem, the way shell tab-completion does.
    pub fn complete_prompt(&mut self) {
        let cwd = self.listing.dir.clone();
        let Some(prompt) = &mut self.prompt else {
            return;
        };
        if prompt.kind == PromptKind::Filter {
            return;
        }
        let input = prompt.input.clone();
        let (dir_part, frag) = match input.rfind('/') {
            Some(i) => (&input[..=i], &input[i + 1..]),
            None => ("", input.as_str()),
        };
        let search_dir = if dir_part.is_empty() {
            cwd.clone()
        } else {
            match fs_ops::resolve(&cwd, dir_part) {
                Ok(t) => t.path,
                Err(_) => return,
            }
        };
        let Ok(iter) = std::fs::read_dir(&search_dir) else {
            self.status = Status::error(format!("cannot complete in {}", search_dir.display()));
            return;
        };
        let mut matches: Vec<(String, bool)> = iter
            .flatten()
            .map(|e| {
                (
                    e.file_name().to_string_lossy().into_owned(),
                    e.path().is_dir(),
                )
            })
            .filter(|(name, _)| name.starts_with(frag))
            .filter(|(name, _)| frag.starts_with('.') || !name.starts_with('.'))
            .collect();
        matches.sort_by(|a, b| crate::listing::natural_cmp(&a.0, &b.0));

        match matches.len() {
            0 => self.status = Status::info(format!("no completion for `{frag}`")),
            1 => {
                let (name, is_dir) = &matches[0];
                let completed = format!("{dir_part}{name}{}", if *is_dir { "/" } else { "" });
                prompt.input = completed;
                prompt.end();
            }
            _ => {
                // Extend to the longest common prefix and list the candidates.
                let prefix = longest_common_prefix(matches.iter().map(|(n, _)| n.as_str()));
                if prefix.len() > frag.len() {
                    prompt.input = format!("{dir_part}{prefix}");
                    prompt.end();
                }
                let names: Vec<&str> = matches.iter().map(|(n, _)| n.as_str()).take(8).collect();
                let more = matches.len().saturating_sub(names.len());
                self.status = Status::info(format!(
                    "{} matches: {}{}",
                    matches.len(),
                    names.join("  "),
                    if more > 0 {
                        format!("  (+{more} more)")
                    } else {
                        String::new()
                    }
                ));
            }
        }
    }

    /// Execute the prompt's action against the real filesystem.
    pub fn submit_prompt(&mut self) {
        let Some(prompt) = self.prompt.take() else {
            return;
        };
        let raw = prompt.input.trim().to_string();
        let cwd = self.listing.dir.clone();

        if prompt.kind == PromptKind::Filter {
            self.listing.filter = raw.clone();
            self.listing.refilter();
            self.clamp_selection();
            self.refresh_preview();
            self.status = if raw.is_empty() {
                Status::info("filter cleared")
            } else {
                Status::success(format!(
                    "filter `{raw}` — {} of {} entries",
                    self.listing.len(),
                    self.listing.total()
                ))
            };
            return;
        }

        if raw.is_empty() {
            self.status = Status::error(format!("{} cancelled — empty input", prompt.kind.title()));
            return;
        }

        match prompt.kind {
            PromptKind::Jump => match fs_ops::resolve(&cwd, &raw) {
                Ok(target) => {
                    let path = if target.path.is_dir() {
                        target.path
                    } else if let Some(parent) = target.path.parent() {
                        if target.path.exists() {
                            parent.to_path_buf()
                        } else {
                            target.path
                        }
                    } else {
                        target.path
                    };
                    self.change_dir(path);
                }
                Err(err) => self.status = Status::error(err),
            },
            PromptKind::MkDir => match fs_ops::resolve(&cwd, &raw) {
                Ok(target) => match fs_ops::create_dir(&target.path) {
                    Ok(path) => {
                        let label = display_relative(&cwd, &path);
                        self.log(format!("mkdir  {}", path.display()));
                        let name = top_level_name(&cwd, &path);
                        self.reload_and_select(&name);
                        self.status = Status::success(format!("created directory {label}"));
                    }
                    Err(err) => self.status = Status::error(err),
                },
                Err(err) => self.status = Status::error(err),
            },
            PromptKind::Copy | PromptKind::Move | PromptKind::Rename => {
                let Some(src) = prompt.subject.clone() else {
                    self.status = Status::error("nothing selected");
                    return;
                };
                if !src.exists() {
                    self.status = Status::error(format!("{} no longer exists", src.display()));
                    self.reload();
                    return;
                }
                let target = match prompt.kind {
                    PromptKind::Rename => {
                        if raw.contains('/') {
                            match fs_ops::resolve(&cwd, &raw) {
                                Ok(t) => t,
                                Err(err) => {
                                    self.status = Status::error(err);
                                    return;
                                }
                            }
                        } else {
                            let parent = src.parent().unwrap_or(&cwd).to_path_buf();
                            match fs_ops::resolve(&parent, &raw) {
                                Ok(t) => t,
                                Err(err) => {
                                    self.status = Status::error(err);
                                    return;
                                }
                            }
                        }
                    }
                    _ => match fs_ops::resolve(&cwd, &raw) {
                        Ok(t) => t,
                        Err(err) => {
                            self.status = Status::error(err);
                            return;
                        }
                    },
                };
                let action = if prompt.kind == PromptKind::Copy {
                    PendingAction::Copy { src, target }
                } else {
                    PendingAction::Move { src, target }
                };
                self.run_action(action, false);
            }
            PromptKind::Filter => unreachable!("handled above"),
        }
    }

    /// Perform a copy/move, asking for confirmation when it would overwrite.
    fn run_action(&mut self, action: PendingAction, overwrite: bool) {
        let cwd = self.listing.dir.clone();
        let result = match &action {
            PendingAction::Copy { src, target } => fs_ops::copy(src, target, overwrite),
            PendingAction::Move { src, target } => fs_ops::move_path(src, target, overwrite),
        };
        let (verb, src) = match &action {
            PendingAction::Copy { src, .. } => ("copied", src.clone()),
            PendingAction::Move { src, .. } => ("moved", src.clone()),
        };
        match result {
            Ok(Outcome::Done(dest)) => {
                self.log(format!(
                    "{:<6} {} -> {}",
                    if verb == "copied" { "copy" } else { "move" },
                    src.display(),
                    dest.display()
                ));
                // Keep the cursor useful: on the new entry when it landed in the
                // current directory, otherwise on whatever replaces the source.
                let landed_here = dest.parent() == Some(cwd.as_path());
                if landed_here {
                    let name = top_level_name(&cwd, &dest);
                    self.reload_and_select(&name);
                } else {
                    self.reload();
                }
                self.status = Status::success(format!(
                    "{verb} {} -> {}",
                    display_relative(&cwd, &src),
                    dest.display()
                ));
            }
            Ok(Outcome::NeedsOverwrite(dest)) => {
                self.status = Status::info(format!(
                    "{} already exists — overwrite? y/n",
                    dest.display()
                ));
                self.confirm = Some(Confirm::Overwrite { action, dest });
            }
            Err(err) => self.status = Status::error(err),
        }
    }

    // ---- confirmations -------------------------------------------------

    pub fn request_delete(&mut self) {
        let Some(entry) = self.listing.get(self.selected) else {
            self.status = Status::error("nothing selected — no entry to delete");
            return;
        };
        let path = entry.path.clone();
        let is_dir = entry.kind == crate::listing::Kind::Dir;
        let count = if is_dir { count_children(&path) } else { 0 };
        self.status = Status::info(if is_dir {
            format!(
                "delete directory {} and its {} item(s)? y/n",
                path.display(),
                count
            )
        } else {
            format!("delete {}? y/n", path.display())
        });
        self.confirm = Some(Confirm::Delete { path, is_dir });
    }

    pub fn confirm_yes(&mut self) {
        let Some(confirm) = self.confirm.take() else {
            return;
        };
        match confirm {
            Confirm::Delete { path, is_dir } => match fs_ops::delete(&path) {
                Ok(()) => {
                    self.log(format!(
                        "delete {}{}",
                        path.display(),
                        if is_dir { "/" } else { "" }
                    ));
                    self.listing.reload();
                    self.clamp_selection();
                    self.refresh_preview();
                    self.status = Status::success(format!("deleted {}", path.display()));
                }
                Err(err) => self.status = Status::error(err),
            },
            Confirm::Overwrite { action, .. } => self.run_action(action, true),
        }
    }

    pub fn confirm_no(&mut self) {
        if self.confirm.take().is_some() {
            self.status = Status::info("cancelled — nothing was changed");
        }
    }

    fn log(&mut self, line: String) {
        self.activity.push(line);
        // Keep the log bounded; the pane only ever shows the tail.
        if self.activity.len() > 200 {
            self.activity.drain(..self.activity.len() - 200);
        }
    }

    // ---- preview scrolling ---------------------------------------------

    pub fn preview_down(&mut self, delta: u16) {
        let viewport = self.preview_viewport;
        self.preview.scroll_down(delta, viewport);
    }

    pub fn preview_up(&mut self, delta: u16) {
        self.preview.scroll_up(delta);
    }
}

/// Longest shared prefix of a set of candidate names.
fn longest_common_prefix<'a>(mut items: impl Iterator<Item = &'a str>) -> String {
    let Some(first) = items.next() else {
        return String::new();
    };
    let mut prefix: Vec<char> = first.chars().collect();
    for item in items {
        let mut shared = 0;
        for (a, b) in prefix.iter().zip(item.chars()) {
            if *a == b {
                shared += 1;
            } else {
                break;
            }
        }
        prefix.truncate(shared);
        if prefix.is_empty() {
            break;
        }
    }
    prefix.into_iter().collect()
}

/// Path relative to `base` when it is inside it, otherwise the absolute path.
pub fn display_relative(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| path.display().to_string())
}

/// First component of `path` below `base` — the entry that appears in the list.
fn top_level_name(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .ok()
        .and_then(|rel| rel.components().next())
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .or_else(|| path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_default()
}

fn count_children(path: &Path) -> usize {
    std::fs::read_dir(path).map(|it| it.count()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;

    fn scratch(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("toolc-app-{tag}-{}", std::process::id()));
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

    fn app_with(root: &Path) -> App {
        App::new(root.to_path_buf(), false)
    }

    fn type_in(app: &mut App, text: &str) {
        for ch in text.chars() {
            if let Some(p) = &mut app.prompt {
                p.insert(ch);
            }
            app.prompt_changed();
        }
    }

    #[test]
    fn selection_drives_preview_content() {
        let root = scratch("select");
        write(&root.join("a.txt"), "alpha\n");
        write(&root.join("b.txt"), "beta\n");
        let mut app = app_with(&root);
        assert_eq!(app.selected_entry().unwrap().name, "a.txt");
        assert_eq!(app.preview.lines, vec!["alpha"]);
        app.select_next(1);
        assert_eq!(app.preview.name, "b.txt");
        assert_eq!(app.preview.lines, vec!["beta"]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn copy_prompt_writes_real_file() {
        let root = scratch("copyflow");
        write(&root.join("src.txt"), "payload\n");
        let mut app = app_with(&root);
        app.open_prompt(PromptKind::Copy);
        type_in(&mut app, "dest.txt");
        app.submit_prompt();
        assert_eq!(app.status.level, Level::Success, "{}", app.status.text);
        assert_eq!(
            fs::read_to_string(root.join("dest.txt")).unwrap(),
            "payload\n"
        );
        assert!(root.join("src.txt").exists());
        // The new file is listed and selected without a manual refresh.
        assert_eq!(app.selected_entry().unwrap().name, "dest.txt");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn copy_over_existing_file_requires_confirmation() {
        let root = scratch("copyconfirm");
        write(&root.join("a.txt"), "new\n");
        write(&root.join("b.txt"), "old\n");
        let mut app = app_with(&root);
        app.open_prompt(PromptKind::Copy);
        type_in(&mut app, "b.txt");
        app.submit_prompt();
        assert!(matches!(app.confirm, Some(Confirm::Overwrite { .. })));
        assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "old\n");

        app.confirm_no();
        assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "old\n");

        app.open_prompt(PromptKind::Copy);
        type_in(&mut app, "b.txt");
        app.submit_prompt();
        app.confirm_yes();
        assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "new\n");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn mkdir_then_move_archives_a_file() {
        let root = scratch("archive");
        write(&root.join("report.log"), "data\n");
        let mut app = app_with(&root);

        app.open_prompt(PromptKind::MkDir);
        type_in(&mut app, "archive");
        app.submit_prompt();
        assert!(root.join("archive").is_dir());
        assert_eq!(app.selected_entry().unwrap().name, "archive");

        // Select the file again and move it into the new directory.
        app.selected = app.listing.index_of_name("report.log").unwrap();
        app.refresh_preview();
        app.open_prompt(PromptKind::Move);
        type_in(&mut app, "archive/");
        app.submit_prompt();
        assert_eq!(app.status.level, Level::Success, "{}", app.status.text);
        assert!(!root.join("report.log").exists());
        assert_eq!(
            fs::read_to_string(root.join("archive/report.log")).unwrap(),
            "data\n"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn rename_updates_listing_and_keeps_content() {
        let root = scratch("rename");
        write(&root.join("old.conf"), "k=v\n");
        let mut app = app_with(&root);
        app.open_prompt(PromptKind::Rename);
        // The prompt pre-fills the current name for easy editing.
        assert_eq!(app.prompt.as_ref().unwrap().input, "old.conf");
        app.prompt.as_mut().unwrap().clear();
        type_in(&mut app, "new.conf");
        app.submit_prompt();
        assert_eq!(app.status.level, Level::Success, "{}", app.status.text);
        assert!(!root.join("old.conf").exists());
        assert_eq!(fs::read_to_string(root.join("new.conf")).unwrap(), "k=v\n");
        assert_eq!(app.selected_entry().unwrap().name, "new.conf");
        assert_eq!(app.preview.lines, vec!["k=v"]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn delete_requires_confirmation() {
        let root = scratch("delete");
        write(&root.join("junk.txt"), "junk\n");
        let mut app = app_with(&root);
        app.request_delete();
        assert!(app.confirm.is_some());
        app.confirm_no();
        assert!(root.join("junk.txt").exists());

        app.request_delete();
        app.confirm_yes();
        assert!(!root.join("junk.txt").exists());
        assert!(app.listing.is_empty());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn navigation_enters_and_leaves_directories() {
        let root = scratch("nav");
        fs::create_dir(root.join("sub")).unwrap();
        write(&root.join("sub/inner.txt"), "inner\n");
        let mut app = app_with(&root);
        app.open_selected();
        assert_eq!(app.cwd(), root.join("sub"));
        assert_eq!(app.selected_entry().unwrap().name, "inner.txt");
        app.go_parent();
        assert_eq!(app.cwd(), root);
        // Cursor returns to the directory we came from.
        assert_eq!(app.selected_entry().unwrap().name, "sub");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn filter_narrows_the_list_live() {
        let root = scratch("filter");
        for name in ["one.txt", "two.txt", "three.txt"] {
            write(&root.join(name), name);
        }
        let mut app = app_with(&root);
        app.open_prompt(PromptKind::Filter);
        // Matching is a plain substring over the whole name, extension included.
        type_in(&mut app, "t");
        assert_eq!(app.listing.len(), 3);
        type_in(&mut app, "hr");
        assert_eq!(app.listing.len(), 1);
        assert_eq!(app.selected_entry().unwrap().name, "three.txt");
        app.submit_prompt();
        assert_eq!(app.status.level, Level::Success);
        app.open_prompt(PromptKind::Filter);
        app.cancel_prompt();
        assert_eq!(app.listing.len(), 3);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tab_completion_expands_unique_prefix() {
        let root = scratch("complete");
        fs::create_dir(root.join("archive")).unwrap();
        write(&root.join("notes.txt"), "n");
        let mut app = app_with(&root);
        app.open_prompt(PromptKind::Move);
        type_in(&mut app, "arc");
        app.complete_prompt();
        assert_eq!(app.prompt.as_ref().unwrap().input, "archive/");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn absolute_destination_outside_cwd_works() {
        let root = scratch("absolute");
        let out = scratch("absolute-dst");
        write(&root.join("f.txt"), "body\n");
        let mut app = app_with(&root);
        app.open_prompt(PromptKind::Copy);
        type_in(&mut app, &out.join("copied.txt").display().to_string());
        app.submit_prompt();
        assert_eq!(app.status.level, Level::Success, "{}", app.status.text);
        assert_eq!(
            fs::read_to_string(out.join("copied.txt")).unwrap(),
            "body\n"
        );
        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&out).unwrap();
    }

    #[test]
    fn errors_are_reported_not_panicked() {
        let root = scratch("errors");
        write(&root.join("f.txt"), "x");
        let mut app = app_with(&root);
        app.open_prompt(PromptKind::Copy);
        type_in(&mut app, "/nonexistent-dir-xyz/out.txt");
        app.submit_prompt();
        assert_eq!(app.status.level, Level::Error);
        assert!(app.status.text.contains("does not exist"));

        app.open_prompt(PromptKind::MkDir);
        type_in(&mut app, "f.txt");
        app.submit_prompt();
        assert_eq!(app.status.level, Level::Error);
        assert!(app.status.text.contains("already exists"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn activity_log_records_operations() {
        let root = scratch("activity");
        write(&root.join("a.txt"), "a");
        let mut app = app_with(&root);
        app.open_prompt(PromptKind::Copy);
        type_in(&mut app, "b.txt");
        app.submit_prompt();
        assert_eq!(app.activity.len(), 1);
        assert!(app.activity[0].starts_with("copy"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn prompt_editing_handles_cursor_moves() {
        let mut p = Prompt::new(PromptKind::Copy, String::new(), None);
        for ch in "abc".chars() {
            p.insert(ch);
        }
        p.left();
        p.insert('X');
        assert_eq!(p.input, "abXc");
        p.home();
        p.delete();
        assert_eq!(p.input, "bXc");
        p.end();
        p.backspace();
        assert_eq!(p.input, "bX");
    }

    #[test]
    fn kill_segment_removes_last_path_component() {
        let mut p = Prompt::new(PromptKind::Copy, "a/b/c".into(), None);
        p.kill_segment();
        assert_eq!(p.input, "a/b/");
        p.kill_segment();
        assert_eq!(p.input, "a/");
        p.kill_segment();
        assert_eq!(p.input, "");
    }

    #[test]
    fn common_prefix_helper() {
        assert_eq!(
            longest_common_prefix(["report1", "report2"].into_iter()),
            "report"
        );
        assert_eq!(longest_common_prefix(["a", "b"].into_iter()), "");
    }
}
