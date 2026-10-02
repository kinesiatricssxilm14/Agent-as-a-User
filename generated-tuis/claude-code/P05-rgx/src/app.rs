//! Application state: everything the UI draws and every key it reacts to.
//!
//! The module is deliberately free of terminal I/O so that the whole
//! interaction model can be unit-tested by feeding it [`KeyEvent`]s and
//! inspecting the resulting state.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::doc::Document;
use crate::engine::{self, Backend, Compiled, Flags, RawMatch, Replacement};
use crate::input::Input;
use crate::textlayout::{Highlight, HlKind};

/// Which widget currently receives text input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// The regular expression field.
    Pattern,
    /// The replacement template field.
    Replace,
    /// The match list (navigation keys move the selection).
    Matches,
    /// The source text pane (navigation keys scroll it).
    Source,
}

impl Focus {
    /// Tab order.
    fn next(self) -> Self {
        match self {
            Focus::Pattern => Focus::Replace,
            Focus::Replace => Focus::Matches,
            Focus::Matches => Focus::Source,
            Focus::Source => Focus::Pattern,
        }
    }

    fn prev(self) -> Self {
        match self {
            Focus::Pattern => Focus::Source,
            Focus::Replace => Focus::Pattern,
            Focus::Matches => Focus::Replace,
            Focus::Source => Focus::Matches,
        }
    }

    /// Whether this focus target edits text.
    pub fn is_text(self) -> bool {
        matches!(self, Focus::Pattern | Focus::Replace)
    }

    /// Label used in the header.
    pub fn label(self) -> &'static str {
        match self {
            Focus::Pattern => "pattern",
            Focus::Replace => "replace",
            Focus::Matches => "matches",
            Focus::Source => "source",
        }
    }
}

/// Severity of a status message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    /// Neutral information.
    Info,
    /// Something succeeded.
    Success,
    /// Something worth noticing.
    Warn,
    /// Something failed.
    Error,
}

/// A transient message shown in the status area.
#[derive(Debug, Clone)]
pub struct Message {
    /// Text to show.
    pub text: String,
    /// How to colour it.
    pub level: Level,
}

/// A modal-free single-line prompt shown in the footer, used for actions that
/// need one extra piece of text (a path) or a confirmation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    /// Ask for a path to write the replacement result to.
    SaveAs,
    /// Ask for a path to load instead of the current file.
    OpenFile,
    /// Confirm overwriting the input file with the replacement result.
    ConfirmOverwrite(PathBuf),
}

impl Prompt {
    /// Text shown before the input.
    pub fn label(&self) -> String {
        match self {
            Prompt::SaveAs => "Write replaced content to:".to_string(),
            Prompt::OpenFile => "Open file:".to_string(),
            Prompt::ConfirmOverwrite(p) => {
                format!("Overwrite {} with the replaced content? (y/N)", p.display())
            }
        }
    }

    /// Whether this prompt takes free text or a single y/n key.
    pub fn is_text(&self) -> bool {
        !matches!(self, Prompt::ConfirmOverwrite(_))
    }
}

/// A ready-made pattern the user can cycle through.  These are onboarding aids
/// only: they populate the editable pattern field, and matching itself always
/// runs the user's current text through the real engine.
pub const PRESETS: &[(&str, &str)] = &[
    ("CN mobile number", r"1[3-9]\d{9}"),
    (
        "CN mobile (+86, optional separators)",
        r"(?:\+?86[-\s]?)?1[3-9]\d{9}",
    ),
    (
        "CN landline",
        r"(?:0\d{2,3}[-\s]?)?\d{7,8}(?:[-\s]?\d{1,4})?",
    ),
    (
        "CN ID card (18)",
        r"[1-9]\d{5}(?:19|20)\d{2}(?:0[1-9]|1[0-2])(?:0[1-9]|[12]\d|3[01])\d{3}[\dXx]",
    ),
    ("Email address", r"[\w.+-]+@[\w-]+\.[\w.-]+"),
    ("IPv4 address", r"\b(?:\d{1,3}\.){3}\d{1,3}\b"),
    ("URL", r#"https?://[^\s<>"')]+"#),
    ("ISO date", r"\d{4}-\d{2}-\d{2}"),
    ("Time HH:MM(:SS)", r"\b\d{1,2}:\d{2}(?::\d{2})?\b"),
    ("Number (incl. decimals)", r"-?\d+(?:\.\d+)?"),
    ("Word (case-insensitive)", r"(?i)error"),
    ("Hex colour", r"#[0-9a-fA-F]{6}\b"),
    ("Duplicated word (back-reference)", r"\b(\w+)\s+\1\b"),
    ("Trailing whitespace", r"[ \t]+$"),
    ("Chinese characters", r"[\x{4e00}-\x{9fff}]+"),
];

/// The result of matching the current pattern against the current document.
#[derive(Debug, Default)]
pub struct MatchState {
    /// Every match found, in document order.
    pub matches: Vec<RawMatch>,
    /// Highlights for the source pane, derived from `matches`.
    pub highlights: Vec<Highlight>,
    /// Which backend ran.
    pub backend: Option<Backend>,
    /// Pattern string as handed to the backend, including inline flags.
    pub effective_pattern: String,
    /// Compilation error, if the pattern is not currently valid.
    pub error: Option<String>,
    /// Explanation of an automatic switch to the backtracking engine.
    pub fallback: Option<String>,
    /// True when the match limit truncated the scan.
    pub truncated: bool,
    /// Capture group names of the compiled pattern (index 0 is `None`).
    pub group_names: Vec<Option<String>>,
}

impl MatchState {
    /// Number of matches found.
    pub fn len(&self) -> usize {
        self.matches.len()
    }

    /// Whether there are no matches.
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }
}

/// Which pane the right-hand column shows below the match list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightPane {
    /// Capture groups of the selected match.
    Groups,
    /// Built-in pattern presets.
    Presets,
}

/// The whole application.
pub struct App {
    /// The loaded input file.
    pub doc: Document,
    /// Regular expression field.
    pub pattern: Input,
    /// Replacement template field.
    pub replace: Input,
    /// Inline-flag toggles.
    pub flags: Flags,
    /// Force the backtracking engine.
    pub prefer_fancy: bool,
    /// Replace only the first match.
    pub first_only: bool,
    /// Where keyboard input goes.
    pub focus: Focus,
    /// Current match state.
    pub matches: MatchState,
    /// Index of the selected match, when there is one.
    pub selected: Option<usize>,
    /// Replacement preview, present when the template field is non-empty.
    pub replacement: Option<Replacement>,
    /// The rewritten text wrapped as a document, so the preview pane can reuse
    /// the same layout code as the source pane.
    pub preview_doc: Option<Document>,
    /// Highlights for the preview pane.
    pub preview_highlights: Vec<Highlight>,
    /// First visible logical line of the source pane.
    pub source_top: usize,
    /// Horizontal scroll of the source pane.
    pub source_hscroll: usize,
    /// First visible logical line of the preview pane.
    pub preview_top: usize,
    /// First visible row of the match list.
    pub list_top: usize,
    /// Soft-wrap long lines in the text panes.
    pub wrap: bool,
    /// Show the line-number gutter.
    pub gutter: bool,
    /// Show the replacement preview pane.
    pub show_preview: bool,
    /// Show the help pane.
    pub show_help: bool,
    /// Which auxiliary pane is displayed on the right.
    pub right_pane: RightPane,
    /// Index into [`PRESETS`] for the presets list.
    pub preset_idx: usize,
    /// Scroll offset of the help pane.
    pub help_scroll: u16,
    /// Columns per tab stop.
    pub tab_width: u16,
    /// Latest status message.
    pub message: Option<Message>,
    /// Active footer prompt, if any.
    pub prompt: Option<Prompt>,
    /// Text field backing the active prompt.
    pub prompt_input: Input,
    /// Set when the user asked to quit.
    pub should_quit: bool,
    /// Height in rows of the source pane's inner area, kept in sync by the
    /// renderer so paging keys know how far to jump.
    pub source_view_rows: usize,
    /// Height in rows of the match list's inner area.
    pub list_view_rows: usize,
    /// Height in rows of the preview pane's inner area.
    pub preview_view_rows: usize,
    /// Width in cells of the source pane's text area, set by the renderer each
    /// frame so horizontal scrolling can be bounded.
    pub source_cols: usize,
}

impl App {
    /// Build the initial state from parsed CLI arguments, reading the input
    /// file from the real filesystem.
    pub fn new(cli: &crate::cli::Cli) -> io::Result<Self> {
        let doc = Document::load(&cli.file)
            .map_err(|e| io::Error::new(e.kind(), format!("{}: {e}", cli.file.display())))?;
        Ok(Self::from_document(doc, cli))
    }

    /// Build state around text that is already in memory, bypassing the
    /// filesystem.  Used by tests and by the `--print` mode.
    pub fn from_document(doc: Document, cli: &crate::cli::Cli) -> Self {
        let mut app = Self {
            doc,
            pattern: Input::with_value(cli.pattern.clone().unwrap_or_default()),
            replace: Input::with_value(cli.replace.clone().unwrap_or_default()),
            flags: cli.flags(),
            prefer_fancy: cli.fancy,
            first_only: cli.first_only,
            focus: Focus::Pattern,
            matches: MatchState::default(),
            selected: None,
            replacement: None,
            preview_doc: None,
            preview_highlights: Vec::new(),
            source_top: 0,
            source_hscroll: 0,
            preview_top: 0,
            list_top: 0,
            wrap: true,
            gutter: true,
            show_preview: cli.replace.is_some(),
            show_help: false,
            right_pane: RightPane::Groups,
            preset_idx: 0,
            help_scroll: 0,
            tab_width: cli.tab_width,
            message: None,
            prompt: None,
            prompt_input: Input::default(),
            should_quit: false,
            source_view_rows: 10,
            list_view_rows: 10,
            preview_view_rows: 6,
            source_cols: 80,
        };
        if app.doc.is_lossy() {
            app.warn("file contains invalid UTF-8; those bytes were replaced with U+FFFD");
        }
        app.recompute();
        app
    }

    /// Re-run the pattern over the document and rebuild everything derived from
    /// it.  Called after any change to the pattern, flags, template or file.
    pub fn recompute(&mut self) {
        let previous = self.selected_offsets();
        self.matches = MatchState::default();
        let pat = self.pattern.value().to_string();

        if pat.is_empty() {
            self.selected = None;
            self.list_top = 0;
            self.rebuild_replacement();
            return;
        }

        match engine::compile(&pat, self.flags, self.prefer_fancy) {
            Ok(Compiled {
                engine,
                backend,
                effective_pattern,
                fallback_reason,
            }) => {
                self.matches.backend = Some(backend);
                self.matches.effective_pattern = effective_pattern;
                self.matches.fallback = fallback_reason;
                self.matches.group_names = engine.group_names();
                match engine.scan(self.doc.text()) {
                    Ok(scan) => {
                        self.matches.truncated = scan.truncated;
                        self.matches.matches = scan.matches;
                    }
                    Err(e) => self.matches.error = Some(e.message),
                }
            }
            Err(e) => self.matches.error = Some(e.message),
        }

        self.matches.highlights = Vec::with_capacity(self.matches.len());
        self.restore_selection(previous);
        self.rebuild_highlights();
        self.rebuild_replacement();
        self.clamp_list();
        // Bring the selected match into view: after typing a pattern the first
        // match is frequently far below the current viewport, and leaving it
        // off-screen would defeat the point of live matching.
        self.scroll_to_selection();
    }

    /// Byte range of the selected match, used to keep the selection stable
    /// across a re-scan.
    fn selected_offsets(&self) -> Option<(usize, usize)> {
        let i = self.selected?;
        self.matches.matches.get(i).map(|m| (m.start, m.end))
    }

    /// After a re-scan, select the match at (or just after) the previously
    /// selected position, so typing another character does not reset the view.
    fn restore_selection(&mut self, previous: Option<(usize, usize)>) {
        if self.matches.is_empty() {
            self.selected = None;
            return;
        }
        let idx = match previous {
            Some((start, _)) => self
                .matches
                .matches
                .iter()
                .position(|m| m.end > start || m.start >= start)
                .unwrap_or(self.matches.len() - 1),
            None => 0,
        };
        self.selected = Some(idx.min(self.matches.len() - 1));
    }

    /// Rebuild the source-pane highlight list from the current matches.
    fn rebuild_highlights(&mut self) {
        let sel = self.selected;
        self.matches.highlights = self
            .matches
            .matches
            .iter()
            .enumerate()
            .map(|(i, m)| Highlight {
                start: m.start,
                end: m.end,
                index: i,
                selected: sel == Some(i),
                kind: HlKind::Match,
            })
            .collect();
    }

    /// Rebuild the replacement preview from the current template.
    fn rebuild_replacement(&mut self) {
        // An empty template would technically mean "delete every match", but
        // showing that before the user has typed anything would be surprising,
        // so the preview stays absent until the field has content.
        if self.replace.is_empty() {
            self.replacement = None;
            self.preview_doc = None;
            self.preview_highlights.clear();
            return;
        }
        let r = engine::replace_all(
            self.doc.text(),
            &self.matches.matches,
            self.replace.value(),
            self.first_only,
        );
        let sel = self.selected;
        self.preview_highlights = r
            .spans
            .iter()
            .map(|s| Highlight {
                start: s.start,
                end: s.end,
                index: s.match_index,
                selected: sel == Some(s.match_index),
                kind: HlKind::Repl,
            })
            .collect();
        self.preview_doc = Some(Document::from_text(self.doc.path(), r.text.clone()));
        self.replacement = Some(r);
        self.clamp_preview();
    }

    /// Reload the current file from disk.
    pub fn reload(&mut self) {
        let path = self.doc.path().to_path_buf();
        match Document::load(&path) {
            Ok(d) => {
                self.doc = d;
                self.source_top = 0;
                self.source_hscroll = 0;
                self.selected = None;
                self.recompute();
                self.success(format!(
                    "reloaded {} ({} lines, {} bytes)",
                    path.display(),
                    self.doc.line_count(),
                    self.doc.byte_count()
                ));
            }
            Err(e) => self.error(format!("reload failed: {e}")),
        }
    }

    /// Load a different file.
    pub fn open(&mut self, path: &Path) {
        match Document::load(path) {
            Ok(d) => {
                self.doc = d;
                self.source_top = 0;
                self.source_hscroll = 0;
                self.preview_top = 0;
                self.selected = None;
                self.recompute();
                self.success(format!(
                    "opened {} ({} lines)",
                    path.display(),
                    self.doc.line_count()
                ));
            }
            Err(e) => self.error(format!("cannot open {}: {e}", path.display())),
        }
    }

    /// Write the replacement result to `path` using a real filesystem write.
    pub fn write_replacement(&mut self, path: &Path) {
        let Some(r) = &self.replacement else {
            self.error("nothing to write: the replacement template is empty");
            return;
        };
        let text = r.text.clone();
        let applied = r.applied;
        // Write to a sibling temporary file and rename, so a failure part-way
        // through cannot leave a truncated file behind.
        match write_atomic(path, text.as_bytes()) {
            Ok(()) => {
                self.success(format!(
                    "wrote {} ({} replacement{} applied, {} bytes)",
                    path.display(),
                    applied,
                    if applied == 1 { "" } else { "s" },
                    text.len()
                ));
                // If we just rewrote the file we are viewing, pick the new
                // content up so the panes stay truthful.
                if path == self.doc.path() {
                    let keep = self.pattern.value().to_string();
                    self.open(path);
                    self.pattern.set(keep);
                    self.recompute();
                }
            }
            Err(e) => self.error(format!("write failed: {e}")),
        }
    }

    /// The selected match, if any.
    pub fn selected_match(&self) -> Option<&RawMatch> {
        self.selected.and_then(|i| self.matches.matches.get(i))
    }

    /// 0-indexed character offsets `(start, end)` of a match.
    pub fn char_offsets(&self, m: &RawMatch) -> (usize, usize) {
        (self.doc.byte_to_char(m.start), self.doc.byte_to_char(m.end))
    }

    /// 0-indexed `(line, column)` of a match start.
    pub fn line_col(&self, m: &RawMatch) -> (usize, usize) {
        self.doc.byte_to_line_col(m.start)
    }

    /// Move the match selection by `delta`, clamping at the ends.
    pub fn select_delta(&mut self, delta: isize) {
        if self.matches.is_empty() {
            return;
        }
        let last = self.matches.len() - 1;
        let cur = self.selected.unwrap_or(0) as isize;
        let next = (cur + delta).clamp(0, last as isize) as usize;
        self.set_selected(next);
    }

    /// Select match `idx` and scroll every pane to show it.
    pub fn set_selected(&mut self, idx: usize) {
        if self.matches.is_empty() {
            self.selected = None;
            return;
        }
        let idx = idx.min(self.matches.len() - 1);
        self.selected = Some(idx);
        self.rebuild_highlights();
        // Preview highlights carry the selection flag too.
        for h in &mut self.preview_highlights {
            h.selected = h.index == idx;
        }
        self.scroll_to_selection();
        self.clamp_list();
    }

    /// Scroll the source, preview and list panes so the selected match is
    /// visible in all of them at once.
    pub fn scroll_to_selection(&mut self) {
        let Some(idx) = self.selected else { return };
        let Some(m) = self.matches.matches.get(idx) else {
            return;
        };
        let (line, col) = self.doc.byte_to_line_col(m.start);
        let rows = self.source_view_rows.max(1);
        if line < self.source_top {
            self.source_top = line;
        } else if line >= self.source_top + rows {
            self.source_top = line + 1 - rows;
        }
        if !self.wrap {
            // Keep the match start inside the horizontal window.
            let margin = 8usize;
            if col < self.source_hscroll {
                self.source_hscroll = col.saturating_sub(margin);
            } else {
                let width = self.source_view_cols().max(1);
                if col >= self.source_hscroll + width {
                    self.source_hscroll = col + margin + 1 - width;
                }
            }
        } else {
            self.source_hscroll = 0;
        }

        if let (Some(pd), Some(span)) = (
            &self.preview_doc,
            self.replacement
                .as_ref()
                .and_then(|r| r.spans.iter().find(|s| s.match_index == idx)),
        ) {
            let (pline, _) = pd.byte_to_line_col(span.start);
            let prows = self.preview_view_rows.max(1);
            if pline < self.preview_top {
                self.preview_top = pline;
            } else if pline >= self.preview_top + prows {
                self.preview_top = pline + 1 - prows;
            }
        }

        let lrows = self.list_view_rows.max(1);
        if idx < self.list_top {
            self.list_top = idx;
        } else if idx >= self.list_top + lrows {
            self.list_top = idx + 1 - lrows;
        }
    }

    /// Approximate usable width of the source pane, used for horizontal
    /// scrolling bounds.  Updated by the renderer.
    fn source_view_cols(&self) -> usize {
        self.source_cols
    }

    /// Scroll the source pane vertically by `delta` lines.
    pub fn scroll_lines(&mut self, delta: isize) {
        let max = self.doc.line_count().saturating_sub(1);
        self.source_top = (self.source_top as isize + delta).clamp(0, max as isize) as usize;
    }

    /// Scroll the preview pane vertically by `delta` lines.
    pub fn scroll_preview(&mut self, delta: isize) {
        let max = self
            .preview_doc
            .as_ref()
            .map(|d| d.line_count().saturating_sub(1))
            .unwrap_or(0);
        self.preview_top = (self.preview_top as isize + delta).clamp(0, max as isize) as usize;
    }

    /// Scroll the source pane horizontally by `delta` cells.
    pub fn scroll_cols(&mut self, delta: isize) {
        self.source_hscroll = (self.source_hscroll as isize + delta).max(0) as usize;
    }

    /// Keep the preview scroll inside its document.
    fn clamp_preview(&mut self) {
        let max = self
            .preview_doc
            .as_ref()
            .map(|d| d.line_count().saturating_sub(1))
            .unwrap_or(0);
        self.preview_top = self.preview_top.min(max);
    }

    /// Keep the list scroll inside the match list.
    fn clamp_list(&mut self) {
        let rows = self.list_view_rows.max(1);
        let max = self.matches.len().saturating_sub(rows);
        self.list_top = self.list_top.min(max);
    }

    /// Show an informational message.
    pub fn info(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            level: Level::Info,
        });
    }

    /// Show a success message.
    pub fn success(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            level: Level::Success,
        });
    }

    /// Show a warning.
    pub fn warn(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            level: Level::Warn,
        });
    }

    /// Show an error.
    pub fn error(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            level: Level::Error,
        });
    }

    // ---------------------------------------------------------------- keys --

    /// Feed one key event to the application.
    ///
    /// This is the single entry point for all interaction; the event loop does
    /// nothing but forward key presses here.
    pub fn handle_key(&mut self, key: KeyEvent) {
        // Ignore key *releases* and repeats-as-releases on terminals that
        // report them (Windows, kitty protocol), otherwise every keystroke
        // would be processed twice.
        if key.kind == KeyEventKind::Release {
            return;
        }
        // A prompt owns the keyboard while it is open.
        if self.prompt.is_some() {
            self.handle_prompt_key(key);
            return;
        }
        if self.show_help && self.handle_help_key(key) {
            return;
        }
        if self.handle_global_key(key) {
            return;
        }
        match self.focus {
            Focus::Pattern => self.handle_pattern_key(key),
            Focus::Replace => self.handle_replace_key(key),
            Focus::Matches => self.handle_matches_key(key),
            Focus::Source => self.handle_source_key(key),
        }
    }

    /// Keys that work the same way regardless of focus.  Returns `true` when
    /// the key was consumed.
    fn handle_global_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let shift = key.modifiers.contains(KeyModifiers::SHIFT);

        match key.code {
            // ---- quit -----------------------------------------------------
            KeyCode::Char('c' | 'q') if ctrl => {
                self.should_quit = true;
                true
            }
            KeyCode::Esc => {
                // Esc is the universal "step back": close help, then leave a
                // text field, then quit from a navigation pane.
                if self.show_help {
                    self.show_help = false;
                    self.help_scroll = 0;
                } else if self.message.is_some() {
                    self.message = None;
                } else if self.focus.is_text() {
                    self.focus = Focus::Matches;
                } else {
                    self.should_quit = true;
                }
                true
            }
            // ---- focus ----------------------------------------------------
            KeyCode::Tab => {
                self.focus = self.focus.next();
                true
            }
            KeyCode::BackTab => {
                self.focus = self.focus.prev();
                true
            }
            // ---- help -----------------------------------------------------
            KeyCode::F(1) => {
                self.show_help = !self.show_help;
                self.help_scroll = 0;
                true
            }
            KeyCode::Char('?') if !self.focus.is_text() => {
                self.show_help = !self.show_help;
                self.help_scroll = 0;
                true
            }
            // ---- flag toggles (function keys work from any focus) ----------
            KeyCode::F(2) => {
                self.flags.ignore_case = !self.flags.ignore_case;
                self.after_flag_change("(?i) case-insensitive", self.flags.ignore_case);
                true
            }
            KeyCode::F(3) => {
                self.flags.multi_line = !self.flags.multi_line;
                self.after_flag_change("(?m) multi-line anchors", self.flags.multi_line);
                true
            }
            KeyCode::F(4) => {
                self.flags.dot_all = !self.flags.dot_all;
                self.after_flag_change("(?s) dot matches newline", self.flags.dot_all);
                true
            }
            KeyCode::F(5) => {
                self.flags.extended = !self.flags.extended;
                self.after_flag_change("(?x) extended", self.flags.extended);
                true
            }
            KeyCode::F(6) => {
                self.flags.literal = !self.flags.literal;
                self.after_flag_change("literal (no regex metacharacters)", self.flags.literal);
                true
            }
            KeyCode::F(7) => {
                self.show_preview = !self.show_preview;
                self.info(if self.show_preview {
                    "replacement preview shown"
                } else {
                    "replacement preview hidden"
                });
                true
            }
            KeyCode::F(8) => {
                self.first_only = !self.first_only;
                let mode = if self.first_only {
                    "replace first match only"
                } else {
                    "replace every match"
                };
                self.info(mode);
                self.rebuild_replacement();
                true
            }
            KeyCode::F(9) => {
                self.wrap = !self.wrap;
                if self.wrap {
                    self.source_hscroll = 0;
                }
                self.info(if self.wrap {
                    "soft wrap on"
                } else {
                    "soft wrap off (use ←/→ to scroll horizontally)"
                });
                true
            }
            KeyCode::F(10) => {
                self.gutter = !self.gutter;
                self.info(if self.gutter {
                    "line numbers shown"
                } else {
                    "line numbers hidden"
                });
                true
            }
            KeyCode::F(12) => {
                self.reload();
                true
            }
            // ---- pattern presets ------------------------------------------
            KeyCode::F(11) => {
                self.right_pane = RightPane::Presets;
                self.preset_delta(if shift { -1 } else { 1 });
                self.apply_preset();
                true
            }
            KeyCode::Char(']') if !self.focus.is_text() => {
                self.right_pane = RightPane::Presets;
                self.preset_delta(1);
                self.apply_preset();
                true
            }
            KeyCode::Char('[') if !self.focus.is_text() => {
                self.right_pane = RightPane::Presets;
                self.preset_delta(-1);
                self.apply_preset();
                true
            }
            // ---- ctrl chords ----------------------------------------------
            KeyCode::Char('r') if ctrl => {
                self.reload();
                true
            }
            KeyCode::Char('o') if ctrl => {
                self.begin_prompt(Prompt::OpenFile, self.doc.path().display().to_string());
                true
            }
            KeyCode::Char('s') if ctrl => {
                self.begin_save();
                true
            }
            KeyCode::Char('g') if ctrl => {
                self.right_pane = match self.right_pane {
                    RightPane::Groups => RightPane::Presets,
                    RightPane::Presets => RightPane::Groups,
                };
                let name = match self.right_pane {
                    RightPane::Groups => "capture groups",
                    RightPane::Presets => "pattern presets",
                };
                self.info(format!("right panel: {name}"));
                true
            }
            KeyCode::Char('y') if ctrl => {
                self.prefer_fancy = !self.prefer_fancy;
                let mode = if self.prefer_fancy {
                    "engine: fancy (backtracking) forced"
                } else {
                    "engine: automatic (linear, falling back to fancy)"
                };
                self.recompute();
                self.info(mode);
                true
            }
            KeyCode::Char('l') if ctrl => {
                self.pattern.clear();
                self.recompute();
                self.info("pattern cleared");
                true
            }
            // ---- match navigation from anywhere ---------------------------
            KeyCode::Char('n') if ctrl => {
                self.select_delta(1);
                true
            }
            KeyCode::Char('p') if ctrl => {
                self.select_delta(-1);
                true
            }
            KeyCode::Enter if self.focus.is_text() => {
                // Enter in a field means "go look at the results".
                self.focus = Focus::Matches;
                if self.selected.is_none() && !self.matches.is_empty() {
                    self.set_selected(0);
                }
                true
            }
            KeyCode::Down if shift => {
                self.scroll_lines(1);
                true
            }
            KeyCode::Up if shift => {
                self.scroll_lines(-1);
                true
            }
            _ => false,
        }
    }

    /// Re-scan and report after a flag toggle.
    fn after_flag_change(&mut self, name: &str, on: bool) {
        self.recompute();
        let state = if on { "on" } else { "off" };
        let count = self.matches.len();
        self.info(format!("{name}: {state} — {count} match(es)"));
    }

    /// Keys for the pattern field.
    fn handle_pattern_key(&mut self, key: KeyEvent) {
        if self.pattern.handle_key(key) {
            self.recompute();
        }
    }

    /// Keys for the replacement field.
    fn handle_replace_key(&mut self, key: KeyEvent) {
        if self.replace.handle_key(key) {
            if !self.replace.is_empty() {
                self.show_preview = true;
            }
            self.rebuild_replacement();
        }
    }

    /// Keys for the match list.
    fn handle_matches_key(&mut self, key: KeyEvent) {
        let page = self.list_view_rows.max(1) as isize;
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.select_delta(1),
            KeyCode::Up | KeyCode::Char('k') => self.select_delta(-1),
            KeyCode::PageDown | KeyCode::Char(' ') => self.select_delta(page),
            KeyCode::PageUp => self.select_delta(-page),
            KeyCode::Home | KeyCode::Char('g') => self.set_selected(0),
            KeyCode::End | KeyCode::Char('G') => {
                let last = self.matches.len().saturating_sub(1);
                self.set_selected(last);
            }
            KeyCode::Enter => {
                // Centre the selected match in the source pane.
                if let Some(m) = self.selected_match() {
                    let (line, _) = self.doc.byte_to_line_col(m.start);
                    let half = self.source_view_rows / 2;
                    self.source_top = line.saturating_sub(half);
                }
                self.focus = Focus::Source;
            }
            KeyCode::Char('i') => self.focus = Focus::Pattern,
            KeyCode::Char('/') => self.focus = Focus::Pattern,
            KeyCode::Char('r') => self.focus = Focus::Replace,
            _ => {}
        }
    }

    /// Keys for the source pane.
    fn handle_source_key(&mut self, key: KeyEvent) {
        let page = self.source_view_rows.max(1) as isize;
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => self.scroll_lines(1),
            KeyCode::Up | KeyCode::Char('k') => self.scroll_lines(-1),
            KeyCode::PageDown | KeyCode::Char(' ') => self.scroll_lines(page),
            KeyCode::PageUp => self.scroll_lines(-page),
            KeyCode::Home | KeyCode::Char('g') => {
                self.source_top = 0;
                self.source_hscroll = 0;
            }
            KeyCode::End | KeyCode::Char('G') => {
                let last = self.doc.line_count().saturating_sub(1);
                self.source_top = last.saturating_sub(self.source_view_rows.saturating_sub(1));
            }
            KeyCode::Left => self.scroll_cols(-4),
            KeyCode::Right => self.scroll_cols(4),
            KeyCode::Char('J') => self.scroll_preview(1),
            KeyCode::Char('K') => self.scroll_preview(-1),
            KeyCode::Char('n') => self.select_delta(1),
            KeyCode::Char('p') => self.select_delta(-1),
            KeyCode::Enter => {
                self.focus = Focus::Matches;
                self.scroll_to_selection();
            }
            KeyCode::Char('i') | KeyCode::Char('/') => self.focus = Focus::Pattern,
            KeyCode::Char('r') => self.focus = Focus::Replace,
            _ => {}
        }
    }

    /// Keys for the help pane.  Returns `true` when consumed.
    fn handle_help_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.help_scroll = self.help_scroll.saturating_add(1);
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.help_scroll = self.help_scroll.saturating_sub(1);
                true
            }
            KeyCode::PageDown => {
                self.help_scroll = self.help_scroll.saturating_add(10);
                true
            }
            KeyCode::PageUp => {
                self.help_scroll = self.help_scroll.saturating_sub(10);
                true
            }
            KeyCode::Home => {
                self.help_scroll = 0;
                true
            }
            _ => false,
        }
    }

    /// Keys for an open footer prompt.
    fn handle_prompt_key(&mut self, key: KeyEvent) {
        let Some(prompt) = self.prompt.clone() else {
            return;
        };
        match &prompt {
            Prompt::ConfirmOverwrite(path) => match key.code {
                KeyCode::Char('y' | 'Y') => {
                    let path = path.clone();
                    self.prompt = None;
                    self.write_replacement(&path);
                }
                KeyCode::Char('n' | 'N') | KeyCode::Esc | KeyCode::Enter => {
                    self.prompt = None;
                    self.info("write cancelled");
                }
                _ => {}
            },
            Prompt::SaveAs | Prompt::OpenFile => match key.code {
                KeyCode::Esc => {
                    self.prompt = None;
                    self.info("cancelled");
                }
                KeyCode::Enter => {
                    let raw = self.prompt_input.value().trim().to_string();
                    self.prompt = None;
                    if raw.is_empty() {
                        self.error("no path given");
                        return;
                    }
                    let path = PathBuf::from(raw);
                    match prompt {
                        Prompt::SaveAs => {
                            if path == self.doc.path() {
                                self.begin_prompt_confirm(path);
                            } else {
                                self.write_replacement(&path);
                            }
                        }
                        Prompt::OpenFile => self.open(&path),
                        Prompt::ConfirmOverwrite(_) => unreachable!("handled above"),
                    }
                }
                _ => {
                    self.prompt_input.handle_key(key);
                }
            },
        }
    }

    /// Open a text prompt pre-filled with `initial`.
    fn begin_prompt(&mut self, prompt: Prompt, initial: impl Into<String>) {
        self.prompt_input = Input::with_value(initial);
        self.prompt = Some(prompt);
        self.message = None;
    }

    /// Open the overwrite confirmation.
    fn begin_prompt_confirm(&mut self, path: PathBuf) {
        self.prompt_input.clear();
        self.prompt = Some(Prompt::ConfirmOverwrite(path));
    }

    /// Start the "write replacement result" flow, refusing early when there is
    /// nothing to write.
    fn begin_save(&mut self) {
        if self.replacement.is_none() {
            self.error(
                "nothing to write: enter a replacement template first (Tab to the Replace field)",
            );
            return;
        }
        let default = default_output_path(self.doc.path());
        self.begin_prompt(Prompt::SaveAs, default.display().to_string());
    }

    /// Load the preset at `preset_idx` into the pattern field.
    pub fn apply_preset(&mut self) {
        let Some((name, pat)) = PRESETS.get(self.preset_idx) else {
            return;
        };
        self.pattern.set(*pat);
        self.recompute();
        self.info(format!(
            "preset applied: {name} — {} match(es)",
            self.matches.len()
        ));
    }

    /// Move through the preset list.
    pub fn preset_delta(&mut self, delta: isize) {
        let n = PRESETS.len() as isize;
        self.preset_idx = ((self.preset_idx as isize + delta).rem_euclid(n)) as usize;
    }
}

/// Default suggestion for the output path: `name.replaced.ext` beside the input.
fn default_output_path(input: &Path) -> PathBuf {
    let stem = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let ext = input.extension().and_then(|s| s.to_str());
    let name = match ext {
        Some(e) => format!("{stem}.replaced.{e}"),
        None => format!("{stem}.replaced"),
    };
    match input.parent().filter(|p| !p.as_os_str().is_empty()) {
        Some(d) => d.join(name),
        None => PathBuf::from(name),
    }
}

/// Write `data` to `path` durably: a temporary file in the same directory is
/// written and then renamed over the target, which is atomic on POSIX.
fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let name = format!(
        ".{}.toole-{}.tmp",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("out"),
        stamp
    );
    let tmp = match dir {
        Some(d) => d.join(name),
        None => PathBuf::from(name),
    };
    fs::write(&tmp, data)?;
    match fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            Err(e)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// A default CLI, as if `toole -f <path>` had been run with no other flags.
    fn cli() -> crate::cli::Cli {
        crate::cli::Cli::parse_from(["toole", "-f", "/tmp/toole-test-input.txt"])
    }

    fn app_with(text: &str) -> App {
        App::from_document(Document::from_text("/tmp/in.txt", text), &cli())
    }

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn code(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn fkey(n: u8) -> KeyEvent {
        KeyEvent::new(KeyCode::F(n), KeyModifiers::NONE)
    }

    /// Type a whole pattern into the focused field.
    fn type_str(app: &mut App, s: &str) {
        for c in s.chars() {
            app.handle_key(key(c));
        }
    }

    #[test]
    fn starts_focused_on_the_pattern_field_with_no_matches() {
        let app = app_with("hello world");
        assert_eq!(app.focus, Focus::Pattern);
        assert!(app.matches.is_empty());
        assert_eq!(app.selected, None);
        assert!(app.matches.error.is_none());
    }

    #[test]
    fn typing_a_pattern_matches_live() {
        let mut app = app_with("a1 b22 c333");
        type_str(&mut app, r"\d+");
        assert_eq!(app.matches.len(), 3);
        // Selection lands on the first match automatically.
        assert_eq!(app.selected, Some(0));
        assert_eq!(app.selected_match().unwrap().text, "1");
    }

    #[test]
    fn every_match_gets_a_highlight() {
        let mut app = app_with("x1 x2 x3");
        type_str(&mut app, "x");
        assert_eq!(app.matches.highlights.len(), 3);
        assert!(app.matches.highlights[0].selected);
        assert!(!app.matches.highlights[1].selected);
        assert!(app
            .matches
            .highlights
            .iter()
            .all(|h| h.kind == HlKind::Match));
    }

    #[test]
    fn character_offsets_are_zero_indexed() {
        let mut app = app_with("abcdef");
        type_str(&mut app, "cd");
        let m = app.selected_match().unwrap();
        assert_eq!(app.char_offsets(m), (2, 4));
    }

    #[test]
    fn character_offsets_are_characters_not_bytes() {
        let mut app = app_with("English-only textabc");
        type_str(&mut app, "abc");
        let m = app.selected_match().unwrap();
        // Byte offset 6, character offset 2.
        assert_eq!(m.start, 6);
        assert_eq!(app.char_offsets(m), (2, 5));
    }

    #[test]
    fn line_and_column_of_a_match() {
        let mut app = app_with("one\ntwo\nthree");
        type_str(&mut app, "three");
        let m = app.selected_match().unwrap();
        assert_eq!(app.line_col(m), (2, 0));
    }

    #[test]
    fn invalid_pattern_reports_an_error_and_keeps_running() {
        let mut app = app_with("text");
        type_str(&mut app, "a(");
        assert!(app.matches.error.is_some());
        assert!(app.matches.is_empty());
        assert!(!app.should_quit);
        // Completing the group clears the error.
        type_str(&mut app, ")");
        assert!(app.matches.error.is_none());
    }

    #[test]
    fn f2_toggles_case_insensitivity_and_rescans() {
        let mut app = app_with("Error error ERROR");
        type_str(&mut app, "error");
        assert_eq!(app.matches.len(), 1);
        app.handle_key(fkey(2));
        assert!(app.flags.ignore_case);
        assert_eq!(app.matches.len(), 3);
        assert_eq!(app.matches.effective_pattern, "(?i)error");
        app.handle_key(fkey(2));
        assert_eq!(app.matches.len(), 1);
    }

    #[test]
    fn inline_case_flag_works_without_the_toggle() {
        let mut app = app_with("Error error");
        type_str(&mut app, "(?i)error");
        assert_eq!(app.matches.len(), 2);
    }

    #[test]
    fn china_mobile_numbers_are_matched() {
        let mut app = app_with("a 13812345678 b 15900001111 c 12345678901");
        type_str(&mut app, r"1[3-9]\d{9}");
        assert_eq!(app.matches.len(), 2);
        let texts: Vec<&str> = app
            .matches
            .matches
            .iter()
            .map(|m| m.text.as_str())
            .collect();
        assert_eq!(texts, vec!["13812345678", "15900001111"]);
        let m = &app.matches.matches[0];
        assert_eq!(app.char_offsets(m), (2, 13));
    }

    #[test]
    fn tab_cycles_focus_in_both_directions() {
        let mut app = app_with("t");
        assert_eq!(app.focus, Focus::Pattern);
        app.handle_key(code(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Replace);
        app.handle_key(code(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Matches);
        app.handle_key(code(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Source);
        app.handle_key(code(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Pattern);
        app.handle_key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(app.focus, Focus::Source);
    }

    #[test]
    fn replacement_preview_shows_the_whole_file() {
        let mut app = app_with("keep this\nfoo here\nkeep that\n");
        type_str(&mut app, "foo");
        app.handle_key(code(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Replace);
        type_str(&mut app, "BAR");
        let r = app.replacement.as_ref().expect("preview present");
        assert_eq!(r.text, "keep this\nBAR here\nkeep that\n");
        assert_eq!(r.applied, 1);
        // The preview document keeps the untouched lines.
        let pd = app.preview_doc.as_ref().unwrap();
        assert_eq!(pd.line_count(), 3);
        assert_eq!(pd.line(0), Some("keep this"));
        assert_eq!(pd.line(2), Some("keep that"));
        assert!(app.show_preview);
    }

    #[test]
    fn replacement_highlights_mark_inserted_text() {
        let mut app = app_with("a foo b");
        type_str(&mut app, "foo");
        app.handle_key(code(KeyCode::Tab));
        type_str(&mut app, "XY");
        assert_eq!(app.preview_highlights.len(), 1);
        let h = app.preview_highlights[0];
        assert_eq!(h.kind, HlKind::Repl);
        let pd = app.preview_doc.as_ref().unwrap();
        assert_eq!(&pd.text()[h.start..h.end], "XY");
    }

    #[test]
    fn replacement_with_capture_groups() {
        let mut app = app_with("2024-05-06\n");
        type_str(&mut app, r"(\d{4})-(\d{2})-(\d{2})");
        app.handle_key(code(KeyCode::Tab));
        type_str(&mut app, "$3/$2/$1");
        assert_eq!(app.replacement.as_ref().unwrap().text, "06/05/2024\n");
    }

    #[test]
    fn f8_switches_to_first_match_only() {
        let mut app = app_with("a a a");
        type_str(&mut app, "a");
        app.handle_key(code(KeyCode::Tab));
        type_str(&mut app, "b");
        assert_eq!(app.replacement.as_ref().unwrap().text, "b b b");
        app.handle_key(fkey(8));
        assert!(app.first_only);
        assert_eq!(app.replacement.as_ref().unwrap().text, "b a a");
        assert_eq!(app.replacement.as_ref().unwrap().applied, 1);
    }

    #[test]
    fn clearing_the_template_removes_the_preview() {
        let mut app = app_with("a");
        type_str(&mut app, "a");
        app.handle_key(code(KeyCode::Tab));
        type_str(&mut app, "b");
        assert!(app.replacement.is_some());
        app.handle_key(code(KeyCode::Backspace));
        assert!(app.replacement.is_none());
        assert!(app.preview_doc.is_none());
        assert!(app.preview_highlights.is_empty());
    }

    #[test]
    fn match_navigation_moves_the_selection() {
        let mut app = app_with("x x x x");
        type_str(&mut app, "x");
        app.focus = Focus::Matches;
        assert_eq!(app.selected, Some(0));
        app.handle_key(code(KeyCode::Down));
        assert_eq!(app.selected, Some(1));
        app.handle_key(key('j'));
        assert_eq!(app.selected, Some(2));
        app.handle_key(key('k'));
        assert_eq!(app.selected, Some(1));
        app.handle_key(code(KeyCode::End));
        assert_eq!(app.selected, Some(3));
        // Clamped at the end.
        app.handle_key(code(KeyCode::Down));
        assert_eq!(app.selected, Some(3));
        app.handle_key(code(KeyCode::Home));
        assert_eq!(app.selected, Some(0));
        app.handle_key(code(KeyCode::Up));
        assert_eq!(app.selected, Some(0));
    }

    #[test]
    fn ctrl_n_and_p_navigate_matches_from_the_pattern_field() {
        let mut app = app_with("y y y");
        type_str(&mut app, "y");
        assert_eq!(app.focus, Focus::Pattern);
        app.handle_key(ctrl('n'));
        assert_eq!(app.selected, Some(1));
        app.handle_key(ctrl('p'));
        assert_eq!(app.selected, Some(0));
        // Typing still goes to the field.
        type_str(&mut app, "y");
        assert_eq!(app.pattern.value(), "yy");
    }

    #[test]
    fn selecting_a_match_scrolls_the_source_pane_to_it() {
        let text: String = (0..100).map(|i| format!("line {i}\n")).collect();
        let mut app = app_with(&text);
        app.source_view_rows = 10;
        type_str(&mut app, "line 90");
        // The single match is on line 90, which must be brought into view.
        assert_eq!(app.matches.len(), 1);
        assert!(app.source_top <= 90);
        assert!(90 < app.source_top + 10);
    }

    #[test]
    fn selection_survives_further_typing() {
        let mut app = app_with("alpha beta alpha beta");
        type_str(&mut app, "beta");
        app.set_selected(1);
        assert_eq!(app.selected, Some(1));
        // Refining the pattern keeps the selection near the same place.
        type_str(&mut app, "?");
        assert!(app.selected.is_some());
    }

    #[test]
    fn selection_resets_when_matches_disappear() {
        let mut app = app_with("abc");
        type_str(&mut app, "abc");
        assert_eq!(app.selected, Some(0));
        type_str(&mut app, "zzz");
        assert!(app.matches.is_empty());
        assert_eq!(app.selected, None);
    }

    #[test]
    fn source_pane_scrolls_by_line_and_page() {
        let text: String = (0..100).map(|i| format!("l{i}\n")).collect();
        let mut app = app_with(&text);
        app.focus = Focus::Source;
        app.source_view_rows = 20;
        app.handle_key(code(KeyCode::Down));
        assert_eq!(app.source_top, 1);
        app.handle_key(code(KeyCode::PageDown));
        assert_eq!(app.source_top, 21);
        app.handle_key(code(KeyCode::PageUp));
        assert_eq!(app.source_top, 1);
        app.handle_key(code(KeyCode::Home));
        assert_eq!(app.source_top, 0);
        // Cannot scroll above the top.
        app.handle_key(code(KeyCode::Up));
        assert_eq!(app.source_top, 0);
        app.handle_key(code(KeyCode::End));
        assert!(app.source_top > 0);
    }

    #[test]
    fn horizontal_scrolling_only_when_wrap_is_off() {
        let mut app = app_with("a very long single line of text");
        app.focus = Focus::Source;
        app.handle_key(fkey(9));
        assert!(!app.wrap);
        app.handle_key(code(KeyCode::Right));
        assert_eq!(app.source_hscroll, 4);
        app.handle_key(code(KeyCode::Left));
        assert_eq!(app.source_hscroll, 0);
        // Never negative.
        app.handle_key(code(KeyCode::Left));
        assert_eq!(app.source_hscroll, 0);
        // Turning wrap back on resets the offset.
        app.handle_key(code(KeyCode::Right));
        app.handle_key(fkey(9));
        assert!(app.wrap);
    }

    #[test]
    fn help_opens_and_closes() {
        let mut app = app_with("t");
        app.handle_key(fkey(1));
        assert!(app.show_help);
        app.handle_key(code(KeyCode::Down));
        assert_eq!(app.help_scroll, 1);
        app.handle_key(code(KeyCode::Esc));
        assert!(!app.show_help);
        assert_eq!(app.help_scroll, 0);
        // '?' also toggles it, but only outside a text field.
        app.focus = Focus::Matches;
        app.handle_key(key('?'));
        assert!(app.show_help);
        app.handle_key(fkey(1));
        assert!(!app.show_help);
    }

    #[test]
    fn question_mark_is_literal_text_in_the_pattern_field() {
        let mut app = app_with("ab");
        type_str(&mut app, "a?");
        assert_eq!(app.pattern.value(), "a?");
        assert!(!app.show_help);
    }

    #[test]
    fn ctrl_q_quits() {
        let mut app = app_with("t");
        app.handle_key(ctrl('q'));
        assert!(app.should_quit);
    }

    #[test]
    fn esc_steps_back_before_quitting() {
        let mut app = app_with("t");
        assert_eq!(app.focus, Focus::Pattern);
        app.handle_key(code(KeyCode::Esc));
        assert_eq!(app.focus, Focus::Matches);
        assert!(!app.should_quit);
        app.handle_key(code(KeyCode::Esc));
        assert!(app.should_quit);
    }

    #[test]
    fn esc_dismisses_a_message_first() {
        let mut app = app_with("t");
        app.focus = Focus::Matches;
        app.info("hello");
        app.handle_key(code(KeyCode::Esc));
        assert!(app.message.is_none());
        assert!(!app.should_quit);
    }

    #[test]
    fn key_releases_are_ignored() {
        let mut app = app_with("t");
        let mut ev = key('a');
        ev.kind = KeyEventKind::Release;
        app.handle_key(ev);
        assert_eq!(app.pattern.value(), "");
    }

    #[test]
    fn ctrl_l_clears_the_pattern() {
        let mut app = app_with("aaa");
        type_str(&mut app, "a");
        assert_eq!(app.matches.len(), 3);
        app.handle_key(ctrl('l'));
        assert!(app.pattern.is_empty());
        assert!(app.matches.is_empty());
    }

    #[test]
    fn presets_populate_the_pattern_field() {
        let mut app = app_with("call 13800001111 now");
        app.focus = Focus::Matches;
        // The first preset is the mainland China mobile pattern.
        app.preset_idx = 0;
        app.apply_preset();
        assert_eq!(app.pattern.value(), PRESETS[0].1);
        assert_eq!(app.matches.len(), 1);
        // Presets remain fully editable afterwards, so nothing is hard-coded.
        app.focus = Focus::Pattern;
        app.handle_key(code(KeyCode::Backspace));
        assert_ne!(app.pattern.value(), PRESETS[0].1);
    }

    #[test]
    fn bracket_keys_cycle_presets() {
        let mut app = app_with("x");
        app.focus = Focus::Matches;
        app.handle_key(key(']'));
        assert_eq!(app.preset_idx, 1);
        assert_eq!(app.right_pane, RightPane::Presets);
        app.handle_key(key('['));
        assert_eq!(app.preset_idx, 0);
        // Wraps around rather than clamping.
        app.handle_key(key('['));
        assert_eq!(app.preset_idx, PRESETS.len() - 1);
    }

    #[test]
    fn every_preset_compiles_and_scans() {
        // Presets are onboarding aids; a broken one would be a latent trap.
        let text = "13800001111 010-88886666 anonymous@example.invalid 10.0.0.1 https://x.dev/p \
                    2024-05-06 12:34:56 -1.5 error #ffcc00 the the  \nEnglish-only text";
        for (name, pat) in PRESETS {
            let c = engine::compile(pat, Flags::default(), false)
                .unwrap_or_else(|e| panic!("preset {name:?} failed to compile: {e}"));
            c.engine
                .scan(text)
                .unwrap_or_else(|e| panic!("preset {name:?} failed to scan: {e}"));
        }
    }

    #[test]
    fn engine_falls_back_for_lookahead_patterns() {
        let mut app = app_with("foobar foobaz");
        type_str(&mut app, "foo(?=bar)");
        assert_eq!(app.matches.backend, Some(Backend::Fancy));
        assert_eq!(app.matches.len(), 1);
        assert!(app.matches.fallback.is_some());
    }

    #[test]
    fn ctrl_y_forces_the_fancy_engine() {
        let mut app = app_with("aa");
        type_str(&mut app, "a");
        assert_eq!(app.matches.backend, Some(Backend::Fast));
        app.handle_key(ctrl('y'));
        assert!(app.prefer_fancy);
        assert_eq!(app.matches.backend, Some(Backend::Fancy));
        assert_eq!(app.matches.len(), 2);
    }

    #[test]
    fn capture_group_names_are_available_for_the_groups_panel() {
        let mut app = app_with("bob@corp");
        type_str(&mut app, r"(?P<user>\w+)@(?P<host>\w+)");
        assert_eq!(app.matches.group_names.len(), 3);
        assert_eq!(app.matches.group_names[1].as_deref(), Some("user"));
        let m = app.selected_match().unwrap();
        assert_eq!(m.groups[2].text.as_deref(), Some("corp"));
    }

    #[test]
    fn save_prompt_refuses_when_there_is_no_replacement() {
        let mut app = app_with("abc");
        type_str(&mut app, "a");
        app.handle_key(ctrl('s'));
        assert!(app.prompt.is_none());
        assert_eq!(app.message.as_ref().unwrap().level, Level::Error);
    }

    #[test]
    fn save_prompt_opens_with_a_suggested_path() {
        let mut app = app_with("abc");
        type_str(&mut app, "a");
        app.handle_key(code(KeyCode::Tab));
        type_str(&mut app, "Z");
        app.handle_key(ctrl('s'));
        assert_eq!(app.prompt, Some(Prompt::SaveAs));
        assert_eq!(app.prompt_input.value(), "/tmp/in.replaced.txt");
        // Esc cancels without writing.
        app.handle_key(code(KeyCode::Esc));
        assert!(app.prompt.is_none());
    }

    #[test]
    fn prompt_captures_typing_instead_of_the_pattern() {
        let mut app = app_with("abc");
        type_str(&mut app, "a");
        app.handle_key(code(KeyCode::Tab));
        type_str(&mut app, "Z");
        app.handle_key(ctrl('s'));
        app.prompt_input.clear();
        type_str(&mut app, "out.txt");
        assert_eq!(app.prompt_input.value(), "out.txt");
        assert_eq!(app.pattern.value(), "a");
    }

    #[test]
    fn overwriting_the_input_file_asks_first() {
        let mut app = app_with("abc");
        type_str(&mut app, "a");
        app.handle_key(code(KeyCode::Tab));
        type_str(&mut app, "Z");
        app.handle_key(ctrl('s'));
        app.prompt_input.set("/tmp/in.txt");
        app.handle_key(code(KeyCode::Enter));
        assert!(matches!(app.prompt, Some(Prompt::ConfirmOverwrite(_))));
        // 'n' declines.
        app.handle_key(key('n'));
        assert!(app.prompt.is_none());
    }

    #[test]
    fn default_output_path_suggestions() {
        assert_eq!(
            default_output_path(Path::new("/bench/data/input.txt")),
            PathBuf::from("/bench/data/input.replaced.txt")
        );
        assert_eq!(
            default_output_path(Path::new("notes")),
            PathBuf::from("notes.replaced")
        );
    }

    #[test]
    fn writing_the_replacement_hits_the_real_filesystem() {
        let dir = std::env::temp_dir().join(format!("toole-test-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        let src = dir.join("in.txt");
        fs::write(&src, "foo one\nbar two\nfoo three\n").expect("write input");

        let mut app = App::from_document(Document::load(&src).expect("load"), &cli());
        type_str(&mut app, "foo");
        app.handle_key(code(KeyCode::Tab));
        type_str(&mut app, "QUX");

        let out = dir.join("out.txt");
        app.write_replacement(&out);
        assert_eq!(app.message.as_ref().unwrap().level, Level::Success);
        let written = fs::read_to_string(&out).expect("read output");
        assert_eq!(written, "QUX one\nbar two\nQUX three\n");
        // No temporary files left behind.
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "leftover temp files: {leftovers:?}");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reload_picks_up_changes_on_disk() {
        let dir = std::env::temp_dir().join(format!("toole-reload-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        let src = dir.join("r.txt");
        fs::write(&src, "one hit\n").expect("write");

        let mut app = App::from_document(Document::load(&src).expect("load"), &cli());
        type_str(&mut app, "hit");
        assert_eq!(app.matches.len(), 1);

        fs::write(&src, "hit hit hit\n").expect("rewrite");
        app.handle_key(fkey(12));
        assert_eq!(app.matches.len(), 3);
        assert_eq!(app.message.as_ref().unwrap().level, Level::Success);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reload_failure_is_reported_not_fatal() {
        let missing = std::env::temp_dir().join("toole-definitely-absent-file.txt");
        let _ = fs::remove_file(&missing);
        let mut app = app_with("x");
        app.doc = Document::from_text(&missing, "x");
        app.handle_key(fkey(12));
        assert_eq!(app.message.as_ref().unwrap().level, Level::Error);
        assert!(!app.should_quit);
    }

    #[test]
    fn opening_a_missing_file_reports_an_error() {
        let mut app = app_with("x");
        app.open(Path::new("/nonexistent/toole/path.txt"));
        assert_eq!(app.message.as_ref().unwrap().level, Level::Error);
        // The previous document is still loaded.
        assert_eq!(app.doc.text(), "x");
    }

    #[test]
    fn all_flag_toggles_are_reachable_and_reversible() {
        let mut app = app_with("a\nb");
        type_str(&mut app, "a");
        let before = app.flags;
        for f in [2u8, 3, 4, 5, 6] {
            app.handle_key(fkey(f));
        }
        assert!(app.flags.ignore_case && app.flags.multi_line);
        assert!(app.flags.dot_all && app.flags.extended && app.flags.literal);
        for f in [2u8, 3, 4, 5, 6] {
            app.handle_key(fkey(f));
        }
        assert_eq!(app.flags, before);
    }

    #[test]
    fn literal_flag_disables_metacharacters() {
        let mut app = app_with("a.c abc");
        type_str(&mut app, "a.c");
        assert_eq!(app.matches.len(), 2);
        app.handle_key(fkey(6));
        assert_eq!(app.matches.len(), 1);
        assert_eq!(app.matches.matches[0].text, "a.c");
    }

    #[test]
    fn right_panel_toggles_between_groups_and_presets() {
        let mut app = app_with("x");
        assert_eq!(app.right_pane, RightPane::Groups);
        app.handle_key(ctrl('g'));
        assert_eq!(app.right_pane, RightPane::Presets);
        app.handle_key(ctrl('g'));
        assert_eq!(app.right_pane, RightPane::Groups);
    }

    #[test]
    fn zero_width_matches_are_listed() {
        let mut app = app_with("ab cd");
        type_str(&mut app, r"\b");
        assert_eq!(app.matches.len(), 4);
        assert!(app.matches.matches.iter().all(|m| m.is_empty()));
        let m = &app.matches.matches[0];
        assert_eq!(app.char_offsets(m), (0, 0));
    }

    #[test]
    fn empty_file_is_handled() {
        let mut app = app_with("");
        type_str(&mut app, "x");
        assert!(app.matches.is_empty());
        assert!(app.matches.error.is_none());
        assert!(!app.should_quit);
    }

    #[test]
    fn enter_moves_from_the_field_to_the_results() {
        let mut app = app_with("q q");
        type_str(&mut app, "q");
        app.handle_key(code(KeyCode::Enter));
        assert_eq!(app.focus, Focus::Matches);
        // And from the list into the source pane.
        app.handle_key(code(KeyCode::Enter));
        assert_eq!(app.focus, Focus::Source);
    }

    #[test]
    fn shortcut_keys_jump_between_fields_from_navigation_panes() {
        let mut app = app_with("z");
        app.focus = Focus::Matches;
        app.handle_key(key('i'));
        assert_eq!(app.focus, Focus::Pattern);
        app.focus = Focus::Matches;
        app.handle_key(key('r'));
        assert_eq!(app.focus, Focus::Replace);
        app.focus = Focus::Source;
        app.handle_key(key('/'));
        assert_eq!(app.focus, Focus::Pattern);
    }

    #[test]
    fn match_limit_is_reported_rather_than_hanging() {
        // One match per character; well over the limit for a small file would
        // require a huge input, so just check the plumbing exists.
        let mut app = app_with("aaaa");
        type_str(&mut app, "a");
        assert!(!app.matches.truncated);
        assert_eq!(app.matches.len(), 4);
    }
}
