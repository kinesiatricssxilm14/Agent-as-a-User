//! Application state and input handling.
//!
//! All behaviour that can be tested without a terminal lives here: the line editor, pipeline
//! navigation, completion bookkeeping, output scrolling/searching, and saving to disk. The
//! rendering layer in [`crate::ui`] reads this state, and [`crate::main`] feeds it key events.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use crate::complete::{self, CompKind, common_prefix, quote_if_needed};
use crate::exec::{self, ExecMsg, RunKind, RunResult};
use crate::pipeline::{self, Unterminated};

/// Which inline prompt, if any, is capturing text input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Prompt {
    /// Editing the pipeline itself.
    None,
    /// Entering a destination path for the current output.
    Save,
    /// Entering a substring to locate in the output.
    Search,
}

/// Severity of the transient status message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Success,
    Error,
}

/// Active completion state, shown in the side panel.
#[derive(Debug, Clone)]
pub struct CompletionState {
    pub candidates: Vec<String>,
    pub selected: usize,
    pub start: usize,
    pub end: usize,
    pub kind: CompKind,
}

/// The whole application.
pub struct App {
    /// Log file the session is centred on (`--file`).
    pub file: PathBuf,
    /// Working directory for executed pipelines.
    pub cwd: PathBuf,

    /// Command line being edited, as characters so the cursor is grapheme-safe for indexing.
    pub input: Vec<char>,
    /// Cursor position, in characters, in `0..=input.len()`.
    pub cursor: usize,

    /// Inline prompt buffer (save path or search term).
    pub prompt: Prompt,
    pub prompt_input: String,
    pub prompt_cursor: usize,

    /// Executed commands, oldest first.
    pub history: Vec<String>,
    /// Position while browsing history; `None` means "editing a fresh line".
    history_pos: Option<usize>,
    /// Line stashed when history browsing started.
    history_stash: Option<Vec<char>>,

    /// Result of the most recent execution.
    pub result: Option<RunResult>,
    /// First visible output line.
    pub output_scroll: usize,
    /// Height of the output viewport, updated by the renderer each frame.
    pub output_height: usize,
    /// Last search term and the matching output line indices.
    pub search_term: String,
    pub matches: Vec<usize>,
    pub match_pos: usize,

    /// Live completion list.
    pub completion: Option<CompletionState>,

    /// Whether the help panel is open.
    pub show_help: bool,
    /// Scroll offset inside the help panel.
    pub help_scroll: usize,

    /// Transient status line message.
    pub status: String,
    pub status_level: Level,

    /// Set while a pipeline is running.
    pub running: bool,
    run_started: Option<Instant>,
    cancel: Arc<AtomicBool>,
    tx: Sender<ExecMsg>,
    rx: Receiver<ExecMsg>,

    /// Cleared by the event loop to exit.
    pub should_quit: bool,
}

impl App {
    /// Create the initial state for `file`.
    pub fn new(file: PathBuf) -> Self {
        let (tx, rx) = channel();
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        // Start from a runnable pipeline against the target file so the first Enter shows data.
        let initial = format!("cat {}", quote_if_needed(&file.to_string_lossy()));
        let mut app = Self {
            file,
            cwd,
            input: initial.chars().collect(),
            cursor: initial.chars().count(),
            prompt: Prompt::None,
            prompt_input: String::new(),
            prompt_cursor: 0,
            history: Vec::new(),
            history_pos: None,
            history_stash: None,
            result: None,
            output_scroll: 0,
            output_height: 10,
            search_term: String::new(),
            matches: Vec::new(),
            match_pos: 0,
            completion: None,
            show_help: true,
            help_scroll: 0,
            status: String::new(),
            status_level: Level::Info,
            running: false,
            run_started: None,
            cancel: Arc::new(AtomicBool::new(false)),
            tx,
            rx,
            should_quit: false,
        };
        app.set_status(
            "Enter runs the pipeline · Alt+\\ runs up to the cursor · F1 toggles help".to_string(),
            Level::Info,
        );
        app
    }

    // ---------------------------------------------------------------- status

    pub fn set_status(&mut self, msg: impl Into<String>, level: Level) {
        self.status = msg.into();
        self.status_level = level;
    }

    /// Human-readable size of the log file, read from the real filesystem.
    pub fn file_info(&self) -> String {
        match fs::metadata(&self.file) {
            Ok(m) => {
                let size = m.len();
                if size >= 1 << 20 {
                    format!("{:.1} MiB", size as f64 / (1u64 << 20) as f64)
                } else if size >= 1 << 10 {
                    format!("{:.1} KiB", size as f64 / (1u64 << 10) as f64)
                } else {
                    format!("{size} B")
                }
            }
            Err(e) => format!("unavailable ({})", e.kind()),
        }
    }

    /// Whether the target file exists, for header colouring.
    pub fn file_exists(&self) -> bool {
        self.file.exists()
    }

    // ---------------------------------------------------------------- editing

    /// The command line as a string.
    pub fn line(&self) -> String {
        self.input.iter().collect()
    }

    /// Insert a character at the cursor.
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += 1;
        self.on_edit();
    }

    /// Insert a string at the cursor.
    pub fn insert_str(&mut self, s: &str) {
        for c in s.chars() {
            self.input.insert(self.cursor, c);
            self.cursor += 1;
        }
        self.on_edit();
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.input.remove(self.cursor);
            self.on_edit();
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.input.len() {
            self.input.remove(self.cursor);
            self.on_edit();
        }
    }

    /// Delete the word before the cursor (Ctrl+W).
    pub fn delete_word_back(&mut self) {
        let start = self.word_start();
        if start < self.cursor {
            self.input.drain(start..self.cursor);
            self.cursor = start;
            self.on_edit();
        }
    }

    /// Delete from the cursor to end of line (Ctrl+K).
    pub fn kill_to_end(&mut self) {
        if self.cursor < self.input.len() {
            self.input.truncate(self.cursor);
            self.on_edit();
        }
    }

    /// Delete from start of line to the cursor (Ctrl+U).
    pub fn kill_to_start(&mut self) {
        if self.cursor > 0 {
            self.input.drain(0..self.cursor);
            self.cursor = 0;
            self.on_edit();
        }
    }

    pub fn clear_line(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.on_edit();
    }

    /// Any edit invalidates the completion list and leaves history browsing.
    fn on_edit(&mut self) {
        self.completion = None;
        self.history_pos = None;
    }

    fn word_start(&self) -> usize {
        let mut i = self.cursor;
        while i > 0 && self.input[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !self.input[i - 1].is_whitespace() {
            i -= 1;
        }
        i
    }

    fn word_end(&self) -> usize {
        let mut i = self.cursor;
        let n = self.input.len();
        while i < n && self.input[i].is_whitespace() {
            i += 1;
        }
        while i < n && !self.input[i].is_whitespace() {
            i += 1;
        }
        i
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
        self.completion = None;
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor += 1;
        }
        self.completion = None;
    }

    pub fn move_word_left(&mut self) {
        self.cursor = self.word_start();
        self.completion = None;
    }

    pub fn move_word_right(&mut self) {
        self.cursor = self.word_end();
        self.completion = None;
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
        self.completion = None;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.input.len();
        self.completion = None;
    }

    /// Jump the cursor to the previous pipeline boundary.
    pub fn move_prev_boundary(&mut self) {
        self.cursor = pipeline::prev_boundary(&self.input, self.cursor);
        self.completion = None;
        let (n, total) = self.stage_position();
        self.set_status(format!("cursor at stage {n}/{total}"), Level::Info);
    }

    /// Jump the cursor to the next pipeline boundary.
    pub fn move_next_boundary(&mut self) {
        self.cursor = pipeline::next_boundary(&self.input, self.cursor);
        self.completion = None;
        let (n, total) = self.stage_position();
        self.set_status(format!("cursor at stage {n}/{total}"), Level::Info);
    }

    /// `(stage_number, stage_count)` for the cursor, 1-based.
    pub fn stage_position(&self) -> (usize, usize) {
        let stages = pipeline::split_stages(&self.input);
        let idx = pipeline::stage_at(&stages, self.cursor);
        (idx + 1, stages.len())
    }

    /// Live validation message for the command line, shown as a preview hint.
    pub fn validation(&self) -> Option<String> {
        match pipeline::unterminated(&self.input) {
            Unterminated::Single => Some("unterminated ' quote".into()),
            Unterminated::Double => Some("unterminated \" quote".into()),
            Unterminated::Paren => Some("unclosed ( ".into()),
            Unterminated::No => {
                let stages = pipeline::split_stages(&self.input);
                // Flag an empty stage between two separators, which the shell would reject.
                for (i, s) in stages.iter().enumerate() {
                    if s.text(&self.input).is_empty() && stages.len() > 1 && i + 1 < stages.len() {
                        return Some("empty pipeline stage".into());
                    }
                }
                None
            }
        }
    }

    // ---------------------------------------------------------------- history

    /// Step backwards through history (Up / Ctrl+P).
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next = match self.history_pos {
            None => {
                self.history_stash = Some(self.input.clone());
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(p) => p - 1,
        };
        self.history_pos = Some(next);
        self.input = self.history[next].chars().collect();
        self.cursor = self.input.len();
        self.completion = None;
    }

    /// Step forwards through history (Down / Ctrl+N).
    pub fn history_next(&mut self) {
        let Some(p) = self.history_pos else { return };
        if p + 1 >= self.history.len() {
            // Past the newest entry: restore the line we stashed.
            self.history_pos = None;
            if let Some(stash) = self.history_stash.take() {
                self.input = stash;
                self.cursor = self.input.len();
            }
        } else {
            self.history_pos = Some(p + 1);
            self.input = self.history[p + 1].chars().collect();
            self.cursor = self.input.len();
        }
        self.completion = None;
    }

    fn push_history(&mut self, cmd: &str) {
        if cmd.is_empty() {
            return;
        }
        if self.history.last().map(String::as_str) == Some(cmd) {
            return;
        }
        self.history.push(cmd.to_string());
        self.history_pos = None;
        self.history_stash = None;
    }

    // ------------------------------------------------------------- completion

    /// Handle Tab: extend the word, or open/cycle the candidate list.
    pub fn complete(&mut self) {
        // An open list means Tab cycles through it.
        if let Some(c) = &mut self.completion
            && c.candidates.len() > 1
        {
            c.selected = (c.selected + 1) % c.candidates.len();
            let cand = c.candidates[c.selected].clone();
            let (start, end) = (c.start, c.end);
            self.replace_range_preview(start, end, &cand);
            return;
        }
        let Some(found) = complete::complete(&self.input, self.cursor) else {
            self.set_status("no completions", Level::Info);
            self.completion = None;
            return;
        };
        let word: String = self.input[found.start..found.end].iter().collect();
        if found.candidates.len() == 1 {
            let cand = quote_if_needed(&found.candidates[0]);
            self.replace_range(found.start, found.end, &cand);
            self.completion = None;
            self.set_status(format!("completed {}", found.candidates[0]), Level::Success);
            return;
        }
        // Several candidates: extend to the longest shared prefix, then show the list.
        let prefix = common_prefix(&found.candidates);
        if prefix.chars().count() > word.chars().count() {
            self.replace_range(found.start, found.end, &prefix);
        }
        let end = self.cursor;
        let kind = found.kind;
        let n = found.candidates.len();
        self.completion = Some(CompletionState {
            candidates: found.candidates,
            selected: 0,
            start: found.start,
            end,
            kind,
        });
        self.set_status(
            format!("{n} candidates · Tab/↑↓ to select · Enter to accept · Esc to dismiss"),
            Level::Info,
        );
    }

    /// Move the selection in the completion list.
    pub fn completion_move(&mut self, delta: isize) {
        let Some(c) = &mut self.completion else { return };
        if c.candidates.is_empty() {
            return;
        }
        let n = c.candidates.len() as isize;
        let next = (c.selected as isize + delta).rem_euclid(n) as usize;
        c.selected = next;
        let cand = c.candidates[next].clone();
        let (start, end) = (c.start, c.end);
        self.replace_range_preview(start, end, &cand);
    }

    /// Accept the highlighted candidate.
    pub fn completion_accept(&mut self) {
        let Some(c) = self.completion.take() else { return };
        let Some(cand) = c.candidates.get(c.selected).cloned() else { return };
        let quoted = quote_if_needed(&cand);
        self.replace_range(c.start, c.end, &quoted);
        self.set_status(format!("completed {cand}"), Level::Success);
    }

    pub fn completion_dismiss(&mut self) {
        if self.completion.take().is_some() {
            self.set_status("completion dismissed", Level::Info);
        }
    }

    /// Replace `start..end` with `text`, leaving the cursor after it.
    fn replace_range(&mut self, start: usize, end: usize, text: &str) {
        let end = end.min(self.input.len());
        let start = start.min(end);
        self.input.splice(start..end, text.chars());
        self.cursor = start + text.chars().count();
        self.history_pos = None;
    }

    /// Like [`Self::replace_range`] but keeps the completion list open so the user can keep
    /// cycling; `end` is updated to the new word end.
    fn replace_range_preview(&mut self, start: usize, end: usize, text: &str) {
        let quoted = quote_if_needed(text);
        self.replace_range(start, end, &quoted);
        if let Some(c) = &mut self.completion {
            c.end = self.cursor;
        }
    }

    // ------------------------------------------------------------- execution

    /// Run the entire pipeline (Enter).
    pub fn run_full(&mut self) {
        let cmd = self.line().trim().to_string();
        if cmd.is_empty() {
            self.set_status("nothing to run", Level::Error);
            return;
        }
        if let Some(v) = self.validation() {
            self.set_status(format!("cannot run: {v}"), Level::Error);
            return;
        }
        self.push_history(&cmd);
        self.start(cmd, RunKind::Full);
    }

    /// Run only the stages up to the cursor (Alt+\).
    pub fn run_partial(&mut self) {
        if let Some(v) = self.validation() {
            self.set_status(format!("cannot run: {v}"), Level::Error);
            return;
        }
        let Some((cmd, n, total)) = pipeline::prefix_through_cursor(&self.input, self.cursor)
        else {
            self.set_status("no pipeline segment before the cursor", Level::Error);
            return;
        };
        self.start(cmd, RunKind::Partial(n, total));
    }

    /// Launch `cmd` on the worker thread.
    fn start(&mut self, cmd: String, kind: RunKind) {
        if self.running {
            self.set_status("a pipeline is already running (Ctrl+C cancels)", Level::Error);
            return;
        }
        self.completion = None;
        self.cancel = Arc::new(AtomicBool::new(false));
        self.running = true;
        self.run_started = Some(Instant::now());
        let label = match kind {
            RunKind::Full => "running full pipeline".to_string(),
            RunKind::Partial(n, total) => format!("running stages 1..{n} of {total}"),
        };
        self.set_status(label, Level::Info);
        exec::spawn(
            cmd,
            kind,
            self.cwd.clone(),
            self.tx.clone(),
            Arc::clone(&self.cancel),
        );
    }

    /// Ask the running pipeline to stop (Ctrl+C).
    pub fn cancel_run(&mut self) {
        if self.running {
            self.cancel.store(true, Ordering::Relaxed);
            self.set_status("cancelling…", Level::Error);
        }
    }

    /// How long the current run has been going, for the spinner.
    pub fn run_elapsed(&self) -> Option<Duration> {
        self.run_started.map(|t| t.elapsed())
    }

    /// Drain finished executions from the worker channel. Returns true if state changed.
    pub fn poll(&mut self) -> bool {
        let mut changed = false;
        while let Ok(ExecMsg::Finished(result)) = self.rx.try_recv() {
            changed = true;
            self.running = false;
            self.run_started = None;
            let level = if result.succeeded() { Level::Success } else { Level::Error };
            let stage_note = match result.kind {
                RunKind::Full => "full pipeline".to_string(),
                RunKind::Partial(n, total) => format!("stages 1..{n}/{total}"),
            };
            let msg = if let Some(err) = &result.error {
                format!("{stage_note}: {err}")
            } else {
                format!(
                    "{stage_note}: exit {} · {} line(s) · {} ms",
                    result.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "?".into()),
                    result.line_count(),
                    result.duration.as_millis()
                )
            };
            self.result = Some(*result);
            self.output_scroll = 0;
            self.refresh_matches();
            self.set_status(msg, level);
        }
        changed
    }

    // ---------------------------------------------------------------- output

    /// Lines currently displayed in the output pane (stdout, then stderr labelled).
    pub fn output_lines(&self) -> &[String] {
        self.result.as_ref().map(|r| r.stdout.as_slice()).unwrap_or(&[])
    }

    pub fn scroll_output(&mut self, delta: isize) {
        let len = self.output_lines().len();
        let max = len.saturating_sub(1);
        let next = (self.output_scroll as isize + delta).clamp(0, max as isize);
        self.output_scroll = next as usize;
    }

    pub fn scroll_page(&mut self, pages: isize) {
        let page = self.output_height.max(1) as isize;
        self.scroll_output(pages * page);
    }

    pub fn scroll_top(&mut self) {
        self.output_scroll = 0;
    }

    pub fn scroll_bottom(&mut self) {
        let len = self.output_lines().len();
        self.output_scroll = len.saturating_sub(self.output_height.max(1));
    }

    /// Recompute search matches after new output or a new term.
    fn refresh_matches(&mut self) {
        self.matches.clear();
        self.match_pos = 0;
        if self.search_term.is_empty() {
            return;
        }
        let needle = self.search_term.to_lowercase();
        let hits: Vec<usize> = self
            .output_lines()
            .iter()
            .enumerate()
            .filter(|(_, line)| line.to_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect();
        self.matches = hits;
    }

    /// Jump to the next/previous match, scrolling it into view.
    pub fn seek_match(&mut self, forward: bool) {
        if self.matches.is_empty() {
            self.set_status(
                if self.search_term.is_empty() {
                    "no search term (Ctrl+F to search)".to_string()
                } else {
                    format!("no matches for {:?}", self.search_term)
                },
                Level::Error,
            );
            return;
        }
        let n = self.matches.len();
        self.match_pos = if forward { (self.match_pos + 1) % n } else { (self.match_pos + n - 1) % n };
        let line = self.matches[self.match_pos];
        self.center_on(line);
        self.set_status(
            format!("match {}/{} on line {}", self.match_pos + 1, n, line + 1),
            Level::Success,
        );
    }

    /// Scroll so `line` sits in the middle of the viewport when possible.
    fn center_on(&mut self, line: usize) {
        let h = self.output_height.max(1);
        self.output_scroll = line.saturating_sub(h / 2);
        let len = self.output_lines().len();
        self.output_scroll = self.output_scroll.min(len.saturating_sub(1));
    }

    // ---------------------------------------------------------------- prompts

    /// Open the save prompt, pre-filled with a sensible destination.
    pub fn begin_save(&mut self) {
        if self.result.is_none() {
            self.set_status("no output to save — run a pipeline first", Level::Error);
            return;
        }
        self.prompt = Prompt::Save;
        // Default under the log file's directory so relative work stays together.
        let default = self
            .file
            .parent()
            .map(|p| p.join("data").join("result.txt"))
            .unwrap_or_else(|| PathBuf::from("result.txt"));
        self.prompt_input = default.to_string_lossy().into_owned();
        self.prompt_cursor = self.prompt_input.chars().count();
        self.set_status("save output: type a path, Enter to write, Esc to cancel", Level::Info);
    }

    /// Open the output search prompt.
    pub fn begin_search(&mut self) {
        self.prompt = Prompt::Search;
        self.prompt_input = self.search_term.clone();
        self.prompt_cursor = self.prompt_input.chars().count();
        self.set_status("search output: Enter to find, Esc to cancel", Level::Info);
    }

    pub fn prompt_insert(&mut self, c: char) {
        let idx = self.prompt_byte_index();
        self.prompt_input.insert(idx, c);
        self.prompt_cursor += 1;
    }

    pub fn prompt_backspace(&mut self) {
        if self.prompt_cursor > 0 {
            self.prompt_cursor -= 1;
            let idx = self.prompt_byte_index();
            self.prompt_input.remove(idx);
        }
    }

    pub fn prompt_delete(&mut self) {
        if self.prompt_cursor < self.prompt_input.chars().count() {
            let idx = self.prompt_byte_index();
            self.prompt_input.remove(idx);
        }
    }

    pub fn prompt_move(&mut self, delta: isize) {
        let n = self.prompt_input.chars().count() as isize;
        self.prompt_cursor = (self.prompt_cursor as isize + delta).clamp(0, n) as usize;
    }

    pub fn prompt_home(&mut self) {
        self.prompt_cursor = 0;
    }

    pub fn prompt_end(&mut self) {
        self.prompt_cursor = self.prompt_input.chars().count();
    }

    /// Byte offset of `prompt_cursor` inside `prompt_input`.
    fn prompt_byte_index(&self) -> usize {
        self.prompt_input
            .char_indices()
            .nth(self.prompt_cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.prompt_input.len())
    }

    /// Tab-complete a path inside the save prompt.
    pub fn prompt_complete(&mut self) {
        if self.prompt != Prompt::Save {
            return;
        }
        let chars: Vec<char> = self.prompt_input.chars().collect();
        let Some(found) = complete::complete(&chars, self.prompt_cursor) else {
            self.set_status("no completions", Level::Info);
            return;
        };
        let text = if found.candidates.len() == 1 {
            found.candidates[0].clone()
        } else {
            common_prefix(&found.candidates)
        };
        if text.is_empty() {
            return;
        }
        let mut next: Vec<char> = chars[..found.start].to_vec();
        next.extend(text.chars());
        next.extend(chars[found.end..].iter().copied());
        self.prompt_cursor = found.start + text.chars().count();
        self.prompt_input = next.into_iter().collect();
        if found.candidates.len() > 1 {
            self.set_status(format!("{} candidates", found.candidates.len()), Level::Info);
        }
    }

    /// Apply the open prompt.
    pub fn prompt_confirm(&mut self) {
        match self.prompt {
            Prompt::None => {}
            Prompt::Save => {
                let path = self.prompt_input.trim().to_string();
                self.prompt = Prompt::None;
                if path.is_empty() {
                    self.set_status("save cancelled: empty path", Level::Error);
                    return;
                }
                match self.save_output(Path::new(&path)) {
                    Ok((lines, bytes)) => self.set_status(
                        format!("wrote {lines} line(s), {bytes} byte(s) to {path}"),
                        Level::Success,
                    ),
                    Err(e) => self.set_status(format!("save failed: {e}"), Level::Error),
                }
            }
            Prompt::Search => {
                self.search_term = self.prompt_input.clone();
                self.prompt = Prompt::None;
                self.refresh_matches();
                if self.search_term.is_empty() {
                    self.set_status("search cleared", Level::Info);
                } else if self.matches.is_empty() {
                    self.set_status(format!("no matches for {:?}", self.search_term), Level::Error);
                } else {
                    let line = self.matches[0];
                    self.center_on(line);
                    self.set_status(
                        format!(
                            "{} match(es) · F3 next · Shift+F3 previous",
                            self.matches.len()
                        ),
                        Level::Success,
                    );
                }
            }
        }
    }

    pub fn prompt_cancel(&mut self) {
        if self.prompt != Prompt::None {
            self.prompt = Prompt::None;
            self.set_status("cancelled", Level::Info);
        }
    }

    /// Write the current output to `path`, creating parent directories as needed.
    ///
    /// The bytes written are exactly what the pipeline produced, so the result is identical
    /// to redirecting the same pipeline to that path in a shell.
    ///
    /// Returns `(lines, bytes)` actually written.
    pub fn save_output(&self, path: &Path) -> std::io::Result<(usize, usize)> {
        let Some(result) = &self.result else {
            return Err(std::io::Error::other("no output to save"));
        };
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &result.stdout_raw)?;
        Ok((result.stdout.len(), result.stdout_raw.len()))
    }

    // ------------------------------------------------------------------ help

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        self.help_scroll = 0;
        self.set_status(
            if self.show_help { "help shown (F1 to hide)" } else { "help hidden (F1 to show)" },
            Level::Info,
        );
    }

    pub fn scroll_help(&mut self, delta: isize) {
        let max = HELP_LINES.len().saturating_sub(1) as isize;
        self.help_scroll = (self.help_scroll as isize + delta).clamp(0, max) as usize;
    }
}

/// Key documentation shown in the help panel, so users never need external docs.
pub const HELP_LINES: &[(&str, &str)] = &[
    ("", "RUN"),
    ("Enter", "execute the full pipeline"),
    ("Alt+\\", "execute stages up to the cursor"),
    ("Ctrl+C", "cancel the running pipeline"),
    ("", "EDIT"),
    ("← →", "move cursor"),
    ("Ctrl+← →", "move by word"),
    ("Alt+← →", "jump to pipe boundary"),
    ("Home/End", "start / end of line"),
    ("Backspace", "delete char before cursor"),
    ("Delete", "delete char at cursor"),
    ("Ctrl+W", "delete previous word"),
    ("Ctrl+K", "delete to end of line"),
    ("Ctrl+U", "delete to start of line"),
    ("Ctrl+L", "clear the line"),
    ("Tab", "complete command / path"),
    ("↑ ↓", "browse command history"),
    ("", "OUTPUT"),
    ("PgUp/PgDn", "scroll output by page"),
    ("Ctrl+↑ ↓", "scroll output by line"),
    ("Ctrl+Home", "jump to first output line"),
    ("Ctrl+End", "jump to last output line"),
    ("Ctrl+F", "search the output"),
    ("F3", "next match"),
    ("Shift+F3", "previous match"),
    ("", "FILES"),
    ("Ctrl+S", "save output to a file"),
    ("", "OTHER"),
    ("F1", "show / hide this help"),
    ("Alt+↑ ↓", "scroll this help panel"),
    ("Esc", "dismiss completion / prompt"),
    ("F10 / Ctrl+Q", "quit toolf"),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        App::new(PathBuf::from("/tmp/toolf-test.log"))
    }

    /// Run a command synchronously by polling until the worker reports back.
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
    fn starts_with_runnable_command_for_the_file() {
        let a = app();
        assert_eq!(a.line(), "cat /tmp/toolf-test.log");
        assert_eq!(a.cursor, a.input.len());
    }

    #[test]
    fn editing_primitives_work() {
        let mut a = app();
        a.clear_line();
        a.insert_str("cat f | grep x");
        assert_eq!(a.line(), "cat f | grep x");
        a.backspace();
        assert_eq!(a.line(), "cat f | grep ");
        a.delete_word_back();
        assert_eq!(a.line(), "cat f | ");
        a.move_home();
        a.delete();
        assert_eq!(a.line(), "at f | ");
        a.move_end();
        a.kill_to_start();
        assert_eq!(a.line(), "");

        a.insert_str("abc def");
        a.move_home();
        a.move_word_right();
        assert_eq!(a.cursor, 3);
        a.kill_to_end();
        assert_eq!(a.line(), "abc");
    }

    #[test]
    fn cursor_jumps_between_pipe_boundaries() {
        let mut a = app();
        a.clear_line();
        a.insert_str("cat f | grep E | wc -l");
        a.move_home();
        a.move_next_boundary();
        assert_eq!(a.cursor, 6); // on the first '|'
        assert_eq!(a.stage_position(), (1, 3));
        a.move_next_boundary();
        assert_eq!(a.cursor, 7);
        assert_eq!(a.stage_position(), (2, 3));
        a.move_end();
        assert_eq!(a.stage_position(), (3, 3));
        a.move_prev_boundary();
        assert_eq!(a.cursor, 16);
    }

    #[test]
    fn runs_full_pipeline_and_reports_result() {
        let mut a = app();
        a.clear_line();
        a.insert_str("printf 'a\\nb\\nc\\n' | grep -c ''");
        a.run_full();
        assert!(a.running);
        run_and_wait(&mut a);
        assert!(!a.running);
        assert_eq!(a.output_lines(), &["3".to_string()]);
        assert_eq!(a.status_level, Level::Success);
        assert_eq!(a.history, vec!["printf 'a\\nb\\nc\\n' | grep -c ''"]);
    }

    #[test]
    fn partial_run_executes_only_prefix() {
        let mut a = app();
        a.clear_line();
        a.insert_str("printf '1\\n2\\n3\\n' | tail -n 1 | wc -l");
        // Put the cursor inside the second stage.
        a.move_home();
        a.move_next_boundary();
        a.move_next_boundary();
        assert_eq!(a.stage_position().0, 2);
        a.run_partial();
        run_and_wait(&mut a);
        // Only stages 1..2 ran, so we see tail's output, not wc's.
        assert_eq!(a.output_lines(), &["3".to_string()]);
        assert!(matches!(a.result.as_ref().unwrap().kind, RunKind::Partial(2, 3)));
        // Partial runs are debugging aids and must not pollute history.
        assert!(a.history.is_empty());
    }

    #[test]
    fn refuses_to_run_invalid_or_empty_lines() {
        let mut a = app();
        a.clear_line();
        a.run_full();
        assert_eq!(a.status_level, Level::Error);
        assert!(!a.running);

        a.insert_str("grep 'unclosed");
        assert!(a.validation().is_some());
        a.run_full();
        assert!(!a.running);
        assert_eq!(a.status_level, Level::Error);
    }

    #[test]
    fn detects_empty_stage() {
        let mut a = app();
        a.clear_line();
        a.insert_str("cat f | | wc -l");
        assert_eq!(a.validation().as_deref(), Some("empty pipeline stage"));
    }

    #[test]
    fn history_browsing_restores_draft() {
        let mut a = app();
        a.clear_line();
        a.insert_str("echo one");
        a.run_full();
        run_and_wait(&mut a);
        a.clear_line();
        a.insert_str("echo two");
        a.run_full();
        run_and_wait(&mut a);

        a.clear_line();
        a.insert_str("draft");
        a.history_prev();
        assert_eq!(a.line(), "echo two");
        a.history_prev();
        assert_eq!(a.line(), "echo one");
        a.history_next();
        assert_eq!(a.line(), "echo two");
        a.history_next();
        assert_eq!(a.line(), "draft");
    }

    #[test]
    fn saves_real_output_to_disk_creating_parents() {
        let mut a = app();
        a.clear_line();
        a.insert_str("printf 'x\\ny\\n'");
        a.run_full();
        run_and_wait(&mut a);

        let dir = std::env::temp_dir().join("toolf-save-test");
        let _ = fs::remove_dir_all(&dir);
        let target = dir.join("nested").join("result.txt");

        a.begin_save();
        assert_eq!(a.prompt, Prompt::Save);
        a.prompt_input = target.to_string_lossy().into_owned();
        a.prompt_cursor = a.prompt_input.chars().count();
        a.prompt_confirm();

        assert_eq!(a.prompt, Prompt::None);
        assert_eq!(a.status_level, Level::Success, "status: {}", a.status);
        assert_eq!(fs::read_to_string(&target).unwrap(), "x\ny\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn saved_bytes_match_the_pipeline_exactly() {
        // Tabs are expanded for *display* only; the saved file must be byte-identical to
        // what a shell redirect would produce.
        let mut a = app();
        a.clear_line();
        a.insert_str("printf 'a\\tb\\nc\\n'");
        a.run_full();
        run_and_wait(&mut a);
        // The display line has the tab expanded...
        assert!(a.output_lines()[0].contains("       "), "{:?}", a.output_lines());

        let target = std::env::temp_dir().join("toolf-raw-save.txt");
        let _ = fs::remove_file(&target);
        let (lines, bytes) = a.save_output(&target).unwrap();
        // ...but the file keeps the real tab byte.
        assert_eq!(fs::read(&target).unwrap(), b"a\tb\nc\n");
        assert_eq!((lines, bytes), (2, 6));
        let _ = fs::remove_file(&target);
    }

    #[test]
    fn saving_empty_output_writes_an_empty_file() {
        let mut a = app();
        a.clear_line();
        a.insert_str("true");
        a.run_full();
        run_and_wait(&mut a);
        let target = std::env::temp_dir().join("toolf-empty-save.txt");
        let _ = fs::remove_file(&target);
        let (lines, bytes) = a.save_output(&target).unwrap();
        assert_eq!((lines, bytes), (0, 0));
        assert_eq!(fs::read(&target).unwrap(), b"");
        let _ = fs::remove_file(&target);
    }

    #[test]
    fn save_without_output_is_rejected() {        let mut a = app();
        a.begin_save();
        assert_eq!(a.prompt, Prompt::None);
        assert_eq!(a.status_level, Level::Error);
    }

    #[test]
    fn save_reports_error_for_bad_path() {
        let mut a = app();
        a.clear_line();
        a.insert_str("echo hi");
        a.run_full();
        run_and_wait(&mut a);
        a.begin_save();
        a.prompt_input = "/proc/definitely/not/writable.txt".to_string();
        a.prompt_confirm();
        assert_eq!(a.status_level, Level::Error);
        assert!(a.status.starts_with("save failed"), "{}", a.status);
    }

    #[test]
    fn search_finds_and_cycles_matches() {
        let mut a = app();
        a.clear_line();
        a.insert_str("printf 'alpha\\nbeta\\nalpha again\\n'");
        a.run_full();
        run_and_wait(&mut a);
        a.output_height = 2;

        a.begin_search();
        a.prompt_input = "alpha".to_string();
        a.prompt_confirm();
        assert_eq!(a.matches, vec![0, 2]);
        assert_eq!(a.status_level, Level::Success);
        a.seek_match(true);
        assert_eq!(a.match_pos, 1);
        a.seek_match(true);
        assert_eq!(a.match_pos, 0);
        a.seek_match(false);
        assert_eq!(a.match_pos, 1);

        // Case-insensitive, and a miss is reported.
        a.begin_search();
        a.prompt_input = "BETA".to_string();
        a.prompt_confirm();
        assert_eq!(a.matches, vec![1]);
        a.begin_search();
        a.prompt_input = "zzz".to_string();
        a.prompt_confirm();
        assert!(a.matches.is_empty());
        assert_eq!(a.status_level, Level::Error);
    }

    #[test]
    fn output_scrolling_is_clamped() {
        let mut a = app();
        a.clear_line();
        a.insert_str("seq 1 100");
        a.run_full();
        run_and_wait(&mut a);
        a.output_height = 10;
        assert_eq!(a.output_lines().len(), 100);

        a.scroll_output(-5);
        assert_eq!(a.output_scroll, 0);
        a.scroll_page(1);
        assert_eq!(a.output_scroll, 10);
        a.scroll_bottom();
        assert_eq!(a.output_scroll, 90);
        a.scroll_output(1000);
        assert_eq!(a.output_scroll, 99);
        a.scroll_top();
        assert_eq!(a.output_scroll, 0);
    }

    #[test]
    fn tab_completes_a_path_in_the_command_line() {
        let dir = std::env::temp_dir().join("toolf-tab-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("unique-name.log"), b"x").unwrap();

        let mut a = app();
        a.clear_line();
        a.insert_str(&format!("cat {}/uniq", dir.display()));
        a.complete();
        assert_eq!(a.line(), format!("cat {}/unique-name.log", dir.display()));
        assert!(a.completion.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tab_with_many_candidates_opens_list_and_accepts() {
        let dir = std::env::temp_dir().join("toolf-tab-multi");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("shared-a.log"), b"x").unwrap();
        fs::write(dir.join("shared-b.log"), b"x").unwrap();

        let mut a = app();
        a.clear_line();
        a.insert_str(&format!("cat {}/sh", dir.display()));
        a.complete();
        let c = a.completion.as_ref().expect("list should open");
        assert_eq!(c.candidates.len(), 2);
        // The shared prefix is inserted immediately.
        assert!(a.line().ends_with("shared-"), "line: {}", a.line());

        a.completion_move(1);
        a.completion_accept();
        assert!(a.line().ends_with("shared-b.log"), "line: {}", a.line());
        assert!(a.completion.is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn escape_dismisses_completion_and_prompt() {
        let mut a = app();
        a.clear_line();
        a.insert_str("c");
        a.complete();
        assert!(a.completion.is_some());
        a.completion_dismiss();
        assert!(a.completion.is_none());

        a.begin_search();
        assert_eq!(a.prompt, Prompt::Search);
        a.prompt_cancel();
        assert_eq!(a.prompt, Prompt::None);
    }

    #[test]
    fn prompt_editing_handles_unicode() {
        let mut a = app();
        a.begin_search();
        a.prompt_input.clear();
        a.prompt_cursor = 0;
        for c in "héllo".chars() {
            a.prompt_insert(c);
        }
        assert_eq!(a.prompt_input, "héllo");
        a.prompt_move(-1);
        a.prompt_backspace();
        assert_eq!(a.prompt_input, "hélo");
        a.prompt_home();
        a.prompt_delete();
        assert_eq!(a.prompt_input, "élo");
        a.prompt_end();
        assert_eq!(a.prompt_cursor, 3);
    }

    #[test]
    fn failed_command_surfaces_stderr_and_exit_code() {
        let mut a = app();
        a.clear_line();
        a.insert_str("cat /definitely/no/such/file/toolf");
        a.run_full();
        run_and_wait(&mut a);
        let r = a.result.as_ref().unwrap();
        assert_ne!(r.exit_code, Some(0));
        assert!(!r.stderr.is_empty());
        assert_eq!(a.status_level, Level::Error);
    }

    #[test]
    fn help_toggles_and_scrolls() {
        let mut a = app();
        assert!(a.show_help);
        a.toggle_help();
        assert!(!a.show_help);
        a.toggle_help();
        a.scroll_help(-1);
        assert_eq!(a.help_scroll, 0);
        a.scroll_help(3);
        assert_eq!(a.help_scroll, 3);
        a.scroll_help(10_000);
        assert_eq!(a.help_scroll, HELP_LINES.len() - 1);
    }
}
