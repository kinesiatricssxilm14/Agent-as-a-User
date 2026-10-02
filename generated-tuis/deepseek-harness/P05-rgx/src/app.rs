//! Application state and core logic: file loading, real-time regex matching,
//! char-offset computation, case-insensitive handling, replacement preview,
//! and keyboard input handling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use regex::Regex;

/// Built-in regex presets. The China phone-number presets exist so that the
/// "phone number matching" capability is available at a keystroke; matching
/// itself is fully generic (any valid regex works on any input).
pub const PRESETS: &[(&str, &str)] = &[
    ("China mobile phone", r"(?:\+?86[- ]?)?1[3-9]\d{9}"),
    ("China landline", r"0\d{2,3}[- ]?\d{7,8}"),
    ("Email", r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}"),
    ("IPv4 address", r"(?:\d{1,3}\.){3}\d{1,3}"),
    ("Date (YYYY-MM-DD)", r"\d{4}-\d{2}-\d{2}"),
    ("URL (http/https)", r"https?://[^\s]+"),
];

/// Which part of the interface currently owns the keyboard.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    Pattern,
    Replacement,
    Results,
    Preview,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Focus::Pattern => Focus::Replacement,
            Focus::Replacement => Focus::Results,
            Focus::Results => Focus::Preview,
            Focus::Preview => Focus::Pattern,
        }
    }

    fn prev(self) -> Self {
        match self {
            Focus::Pattern => Focus::Preview,
            Focus::Replacement => Focus::Pattern,
            Focus::Results => Focus::Replacement,
            Focus::Preview => Focus::Results,
        }
    }
}

/// A single-line text input with a byte-offset cursor.
pub struct InputField {
    pub buf: String,
    /// Cursor position as a byte offset into `buf` (always on a char boundary).
    pub cursor: usize,
}

impl InputField {
    fn new() -> Self {
        Self {
            buf: String::new(),
            cursor: 0,
        }
    }

    fn insert_char(&mut self, c: char) {
        self.buf.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        if let Some((idx, _)) = self.buf[..self.cursor].char_indices().next_back() {
            self.buf.replace_range(idx..self.cursor, "");
            self.cursor = idx;
        }
    }

    fn delete(&mut self) {
        if self.cursor >= self.buf.len() {
            return;
        }
        if let Some(ch) = self.buf[self.cursor..].chars().next() {
            self.buf.replace_range(self.cursor..self.cursor + ch.len_utf8(), "");
        }
    }

    fn move_left(&mut self) {
        if self.cursor == 0 {
            return;
        }
        if let Some((idx, _)) = self.buf[..self.cursor].char_indices().next_back() {
            self.cursor = idx;
        }
    }

    fn move_right(&mut self) {
        if self.cursor >= self.buf.len() {
            return;
        }
        if let Some(ch) = self.buf[self.cursor..].chars().next() {
            self.cursor += ch.len_utf8();
        }
    }

    fn home(&mut self) {
        self.cursor = 0;
    }

    fn end(&mut self) {
        self.cursor = self.buf.len();
    }

    fn clear(&mut self) {
        self.buf.clear();
        self.cursor = 0;
    }

    fn kill_to_end(&mut self) {
        self.buf.truncate(self.cursor);
    }

    pub(crate) fn set(&mut self, s: &str) {
        self.buf = s.to_string();
        self.cursor = self.buf.len();
    }

    /// Cursor position expressed as a character (Unicode scalar) index.
    pub fn cursor_char_index(&self) -> usize {
        self.buf[..self.cursor].chars().count()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

/// One regex match, annotated with 0-indexed global character offsets and the
/// 1-indexed source line it starts on.
pub struct MatchInfo {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub text: String,
}

/// Kind of a preview segment, used to pick its highlight style.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpanKind {
    Plain,
    Match,
    Replacement,
}

/// A text segment inside one preview line.
pub struct PreviewSpan {
    pub text: String,
    pub kind: SpanKind,
}

pub struct App {
    pub file_path: String,
    pub full_text: String,
    line_starts: Vec<usize>,

    pub pattern: InputField,
    pub replacement: InputField,
    pub focus: Focus,
    pub case_insensitive: bool,

    pub matches: Vec<MatchInfo>,
    pub error: Option<String>,
    pub file_error: Option<String>,
    pub status_msg: String,

    /// Preview content split into display lines, each a sequence of styled spans.
    pub preview_lines: Vec<Vec<PreviewSpan>>,

    pub results_scroll: usize,
    pub preview_scroll: usize,
    pub selected: usize,

    pub show_help: bool,
    pub show_presets: bool,
    pub preset_selection: usize,

    pub quit: bool,
}

impl App {
    pub fn new(file_path: String) -> Self {
        let mut app = Self {
            file_path,
            full_text: String::new(),
            line_starts: vec![0],
            pattern: InputField::new(),
            replacement: InputField::new(),
            focus: Focus::Pattern,
            case_insensitive: false,
            matches: Vec::new(),
            error: None,
            file_error: None,
            status_msg: String::new(),
            preview_lines: Vec::new(),
            results_scroll: 0,
            preview_scroll: 0,
            selected: 0,
            show_help: false,
            show_presets: false,
            preset_selection: 0,
            quit: false,
        };
        app.load_file();
        app
    }

    /// Test helper: build an `App` pre-loaded with `text` (no file I/O).
    #[cfg(test)]
    pub fn from_text(text: &str) -> Self {
        let mut app = Self::new("/nonexistent/toole-test.txt".to_string());
        app.full_text = text.to_string();
        app.line_starts = compute_line_starts(&app.full_text);
        app.recompute();
        app
    }

    /// (Re)load the input file from disk and recompute everything.
    pub fn load_file(&mut self) {
        match std::fs::read(&self.file_path) {
            Ok(bytes) => {
                self.file_error = None;
                self.full_text = String::from_utf8_lossy(&bytes).into_owned();
            }
            Err(e) => {
                self.file_error = Some(e.to_string());
                self.full_text = String::new();
            }
        }
        self.line_starts = compute_line_starts(&self.full_text);
        self.recompute();
    }

    /// The pattern actually handed to the regex engine (with `(?i)` prefix when
    /// case-insensitive mode is toggled on).
    pub fn effective_pattern(&self) -> String {
        if self.case_insensitive {
            format!("(?i){}", self.pattern.buf)
        } else {
            self.pattern.buf.clone()
        }
    }

    fn focused_input_mut(&mut self) -> &mut InputField {
        match self.focus {
            Focus::Pattern => &mut self.pattern,
            _ => &mut self.replacement,
        }
    }

    /// Recompute matches and the preview pane from current inputs.
    pub fn recompute(&mut self) {
        self.matches.clear();
        self.error = None;

        let pattern = self.effective_pattern();
        if pattern.is_empty() {
            self.preview_lines = split_plain(&self.full_text);
            self.selected = 0;
            return;
        }

        let re = match Regex::new(&pattern) {
            Ok(r) => r,
            Err(e) => {
                self.error = Some(e.to_string());
                self.preview_lines = split_plain(&self.full_text);
                self.selected = 0;
                return;
            }
        };

        // `find_iter` yields matches in order, non-overlapping.
        let mut byte_ranges: Vec<(usize, usize)> = Vec::new();
        let mut texts: Vec<String> = Vec::new();
        for m in re.find_iter(&self.full_text) {
            byte_ranges.push((m.start(), m.end()));
            texts.push(m.as_str().to_string());
        }

        let char_offsets = char_offsets_for(&self.full_text, &byte_ranges);
        self.matches.reserve(byte_ranges.len());
        for (i, &(bs, _be)) in byte_ranges.iter().enumerate() {
            let (cs, ce) = char_offsets[i];
            let line = self.line_starts.partition_point(|&s| s <= bs);
            self.matches.push(MatchInfo {
                start: cs,
                end: ce,
                line,
                text: texts[i].clone(),
            });
        }

        self.preview_lines = self.build_preview(&re);

        // Keep the selection valid.
        if self.matches.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.matches.len() {
            self.selected = self.matches.len() - 1;
        }
    }

    fn build_preview(&self, re: &Regex) -> Vec<Vec<PreviewSpan>> {
        if self.replacement.is_empty() {
            self.highlight_original(re)
        } else {
            self.replaced_preview(re)
        }
    }

    /// Preview of the original text with each match highlighted.
    fn highlight_original(&self, re: &Regex) -> Vec<Vec<PreviewSpan>> {
        let mut spans: Vec<PreviewSpan> = Vec::new();
        let mut last = 0;
        for m in re.find_iter(&self.full_text) {
            if m.start() > last {
                spans.push(PreviewSpan {
                    text: self.full_text[last..m.start()].to_string(),
                    kind: SpanKind::Plain,
                });
            }
            spans.push(PreviewSpan {
                text: m.as_str().to_string(),
                kind: SpanKind::Match,
            });
            last = m.end();
        }
        if last < self.full_text.len() {
            spans.push(PreviewSpan {
                text: self.full_text[last..].to_string(),
                kind: SpanKind::Plain,
            });
        }
        spans_to_lines(&spans)
    }

    /// Preview of the full file content with each match replaced by the
    /// (capture-expanded) replacement string; replaced regions are highlighted.
    fn replaced_preview(&self, re: &Regex) -> Vec<Vec<PreviewSpan>> {
        let mut spans: Vec<PreviewSpan> = Vec::new();
        let mut last = 0;
        for caps in re.captures_iter(&self.full_text) {
            let m = caps.get(0).expect("group 0 always present");
            if m.start() > last {
                spans.push(PreviewSpan {
                    text: self.full_text[last..m.start()].to_string(),
                    kind: SpanKind::Plain,
                });
            }
            let mut expanded = String::new();
            caps.expand(&self.replacement.buf, &mut expanded);
            spans.push(PreviewSpan {
                text: expanded,
                kind: SpanKind::Replacement,
            });
            last = m.end();
        }
        if last < self.full_text.len() {
            spans.push(PreviewSpan {
                text: self.full_text[last..].to_string(),
                kind: SpanKind::Plain,
            });
        }
        spans_to_lines(&spans)
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        // Ctrl+C always quits (raw mode turns it into a key event, not SIGINT).
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        // Ctrl+R reloads the file from any state.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('r') {
            self.load_file();
            self.status_msg = "file reloaded".to_string();
            return;
        }
        // Function keys never conflict with typing, so they work from any focus.
        match key.code {
            KeyCode::F(1) => {
                self.show_help = true;
                return;
            }
            KeyCode::F(2) => {
                self.show_presets = true;
                self.preset_selection = 0;
                return;
            }
            _ => {}
        }

        if self.show_presets {
            self.handle_presets_key(key);
            return;
        }
        if self.show_help {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('h') | KeyCode::Char('?') | KeyCode::F(1)
            ) {
                self.show_help = false;
            }
            return;
        }

        match self.focus {
            Focus::Pattern | Focus::Replacement => self.handle_input_key(key),
            Focus::Results | Focus::Preview => self.handle_browse_key(key),
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) {
        let before;
        {
            let field = self.focused_input_mut();
            before = field.buf.clone();

            match key.code {
                KeyCode::Char(c)
                    if !key.modifiers.contains(KeyModifiers::CONTROL)
                        && !key.modifiers.contains(KeyModifiers::ALT) =>
                {
                    field.insert_char(c);
                }
                KeyCode::Backspace => field.backspace(),
                KeyCode::Delete => field.delete(),
                KeyCode::Left => field.move_left(),
                KeyCode::Right => field.move_right(),
                KeyCode::Home => field.home(),
                KeyCode::End => field.end(),
                KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => field.home(),
                KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => field.end(),
                KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => field.clear(),
                KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    field.kill_to_end()
                }
                KeyCode::Tab => {
                    self.focus = self.focus.next();
                }
                KeyCode::BackTab => {
                    self.focus = self.focus.prev();
                }
                KeyCode::Enter => {
                    self.focus = Focus::Results;
                }
                KeyCode::Esc => field.clear(),
                _ => {}
            }
        }

        // Only recompute when the buffer actually changed.
        let changed = {
            let field = self.focused_input_mut();
            field.buf != before
        };
        if changed {
            self.recompute();
        }
    }

    fn handle_browse_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('h') | KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('/') => self.focus = Focus::Pattern,
            KeyCode::Char('r') => self.focus = Focus::Replacement,
            KeyCode::Char('i') => {
                self.case_insensitive = !self.case_insensitive;
                self.status_msg = if self.case_insensitive {
                    "case-insensitive matching ON".to_string()
                } else {
                    "case-insensitive matching OFF".to_string()
                };
                self.recompute();
            }
            KeyCode::Tab => self.focus = self.focus.next(),
            KeyCode::BackTab => self.focus = self.focus.prev(),
            KeyCode::Esc => self.focus = Focus::Pattern,

            KeyCode::Up => {
                if self.focus == Focus::Results {
                    self.selected_move(-1);
                } else {
                    self.preview_scroll = self.preview_scroll.saturating_sub(1);
                }
            }
            KeyCode::Down => {
                if self.focus == Focus::Results {
                    self.selected_move(1);
                } else {
                    self.preview_scroll = self.preview_scroll.saturating_add(1);
                }
            }
            KeyCode::PageUp => {
                if self.focus == Focus::Results {
                    self.selected_move(-10);
                } else {
                    self.preview_scroll = self.preview_scroll.saturating_sub(10);
                }
            }
            KeyCode::PageDown => {
                if self.focus == Focus::Results {
                    self.selected_move(10);
                } else {
                    self.preview_scroll = self.preview_scroll.saturating_add(10);
                }
            }
            KeyCode::Home => {
                if self.focus == Focus::Results {
                    if !self.matches.is_empty() {
                        self.selected = 0;
                    }
                } else {
                    self.preview_scroll = 0;
                }
            }
            KeyCode::End => {
                if self.focus == Focus::Results {
                    if !self.matches.is_empty() {
                        self.selected = self.matches.len() - 1;
                    }
                } else {
                    self.preview_scroll = self.preview_lines.len();
                }
            }
            KeyCode::Enter if self.focus == Focus::Results => {
                self.jump_preview_to_selected();
            }
            _ => {}
        }
    }

    fn handle_presets_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::F(2) => self.show_presets = false,
            KeyCode::Up => self.preset_selection = self.preset_selection.saturating_sub(1),
            KeyCode::Down => {
                if self.preset_selection + 1 < PRESETS.len() {
                    self.preset_selection += 1;
                }
            }
            KeyCode::Enter => self.apply_selected_preset(),
            _ => {}
        }
    }

    fn apply_selected_preset(&mut self) {
        if self.preset_selection < PRESETS.len() {
            let (_, pattern) = PRESETS[self.preset_selection];
            self.pattern.set(pattern);
            self.status_msg = format!("preset applied: {}", PRESETS[self.preset_selection].0);
        }
        self.show_presets = false;
        self.focus = Focus::Pattern;
        self.recompute();
    }

    fn selected_move(&mut self, delta: isize) {
        if self.matches.is_empty() {
            return;
        }
        let len = self.matches.len() as isize;
        let cur = self.selected as isize;
        self.selected = (cur + delta).clamp(0, len - 1) as usize;
    }

    fn jump_preview_to_selected(&mut self) {
        if self.matches.is_empty() {
            return;
        }
        let line = self.matches[self.selected].line; // 1-indexed
        self.preview_scroll = line.saturating_sub(1);
    }
}

/// Compute 0-indexed byte offsets where each line begins (used to map a match
/// byte offset to a 1-indexed line number via binary search).
fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut v = vec![0usize];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' {
            v.push(i + 1);
        }
    }
    v
}

/// Convert sorted, non-overlapping byte ranges to 0-indexed character offsets
/// in a single O(n) pass (rather than O(n·m) via `chars().count()` per match).
fn char_offsets_for(text: &str, ranges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(ranges.len());
    let mut byte_cursor = 0usize;
    let mut char_cursor = 0usize;
    for &(bs, be) in ranges {
        while byte_cursor < bs {
            let ch = text[byte_cursor..].chars().next().expect("cursor on char boundary");
            byte_cursor += ch.len_utf8();
            char_cursor += 1;
        }
        let len_chars = text[bs..be].chars().count();
        out.push((char_cursor, char_cursor + len_chars));
        byte_cursor = be;
        char_cursor += len_chars;
    }
    out
}

/// Split the full text into display lines with no highlighting.
fn split_plain(text: &str) -> Vec<Vec<PreviewSpan>> {
    text.split('\n')
        .map(|line| {
            vec![PreviewSpan {
                text: line.to_string(),
                kind: SpanKind::Plain,
            }]
        })
        .collect()
}

/// Turn a flat list of spans (which may contain embedded newlines, e.g. when a
/// match spans a line break) into per-line span vectors.
fn spans_to_lines(spans: &[PreviewSpan]) -> Vec<Vec<PreviewSpan>> {
    let mut lines: Vec<Vec<PreviewSpan>> = vec![Vec::new()];
    for sp in spans {
        let mut current = String::new();
        for c in sp.text.chars() {
            if c == '\n' {
                if !current.is_empty() {
                    lines.last_mut().unwrap().push(PreviewSpan {
                        text: std::mem::take(&mut current),
                        kind: sp.kind,
                    });
                }
                lines.push(Vec::new());
            } else {
                current.push(c);
            }
        }
        if !current.is_empty() {
            lines.last_mut().unwrap().push(PreviewSpan {
                text: current,
                kind: sp.kind,
            });
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_offsets_ascii() {
        let text = "ab cde f";
        // "ab" (0..2) and "cde" (3..6)
        let ranges = vec![(0, 2), (3, 6)];
        assert_eq!(char_offsets_for(text, &ranges), vec![(0, 2), (3, 6)]);
    }

    #[test]
    fn char_offsets_multibyte() {
        let text = "héllo world";
        // "é" occupies bytes 1..3
        let ranges = vec![(1, 3)];
        assert_eq!(char_offsets_for(text, &ranges), vec![(1, 2)]);
    }

    #[test]
    fn line_numbers_are_one_indexed() {
        let text = "one\ntwo\nthree";
        let starts = compute_line_starts(text);
        // byte offset of "three" is 8
        assert_eq!(starts.partition_point(|&s| s <= 8), 3);
        // byte offset of "two" is 4
        assert_eq!(starts.partition_point(|&s| s <= 4), 2);
    }

    #[test]
    fn replace_expands_capture_refs() {
        let re = Regex::new(r"(\d+)").unwrap();
        let caps = re.captures("abc 123 def").unwrap();
        let mut out = String::new();
        caps.expand("[$1]", &mut out);
        assert_eq!(out, "[123]");
    }

    #[test]
    fn replace_supports_whole_match_dollar0() {
        let re = Regex::new(r"\d+").unwrap();
        let caps = re.captures("abc 123").unwrap();
        let mut out = String::new();
        caps.expand("<$0>", &mut out);
        assert_eq!(out, "<123>");
    }

    #[test]
    fn effective_pattern_prepends_flag() {
        let mut app = App::new("/nonexistent/toole-test-file.txt".to_string());
        app.pattern.set(r"\d+");
        app.case_insensitive = true;
        assert_eq!(app.effective_pattern(), r"(?i)\d+");
    }

    #[test]
    fn phone_preset_matches_mainland_mobile() {
        let re = Regex::new(PRESETS[0].1).unwrap();
        assert!(re.is_match("call 13812345678 now"));
        assert!(re.is_match("+86 13812345678"));
        assert!(re.is_match("8613912345678"));
        // A 10-digit number is too short to be a valid 11-digit mobile number.
        assert!(!re.is_match("1234567890"));
        // A 12-digit run starting "123..." has no 11-digit substring that fits 1[3-9].
        assert!(!re.is_match("123456789012"));
    }

    // ---- interactive (key handling) tests ----

    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn typing_pattern_then_case_toggle() {
        let mut app = App::from_text("Hello hello HELLO hElLo\n");
        for c in "hello".chars() {
            app.handle_key(key(KeyCode::Char(c)));
        }
        assert_eq!(app.matches.len(), 1, "case-sensitive 'hello' matches once");
        assert_eq!(app.matches[0].start, 6);

        // Leave the input and toggle case-insensitive matching.
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.focus, Focus::Results);
        app.handle_key(key(KeyCode::Char('i')));
        assert!(app.case_insensitive);
        assert_eq!(app.matches.len(), 4, "(?i)hello matches every variant");
    }

    #[test]
    fn replacement_preview_replaces_and_marks() {
        let mut app = App::from_text("a 13812345678 b 15900001111 c\n");
        app.pattern.set(r"1[3-9]\d{9}");
        app.replacement.set("[X]");
        app.recompute();

        let joined: String = app.preview_lines.iter().flatten().map(|s| s.text.as_str()).collect();
        assert_eq!(joined, "a [X] b [X] c");
        let kinds: Vec<SpanKind> = app.preview_lines[0].iter().map(|s| s.kind).collect();
        assert!(kinds.contains(&SpanKind::Replacement));
        assert!(kinds.contains(&SpanKind::Plain));
    }

    #[test]
    fn tab_cycles_focus_in_order() {
        let mut app = App::from_text("x\n");
        assert_eq!(app.focus, Focus::Pattern);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Replacement);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Results);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Preview);
        app.handle_key(key(KeyCode::Tab));
        assert_eq!(app.focus, Focus::Pattern);
    }

    #[test]
    fn preset_apply_sets_pattern_and_matches() {
        let mut app = App::from_text("call 13812345678\n");
        app.handle_key(key(KeyCode::F(2)));
        assert!(app.show_presets);
        app.handle_key(key(KeyCode::Enter)); // applies the first preset (mobile phone)
        assert!(!app.show_presets);
        assert_eq!(app.pattern.buf, PRESETS[0].1);
        assert_eq!(app.focus, Focus::Pattern);
        assert_eq!(app.matches.len(), 1);
        assert_eq!(app.matches[0].text, "13812345678");
    }

    #[test]
    fn invalid_regex_reports_error_without_panic() {
        let mut app = App::from_text("abc\n");
        app.pattern.set("(");
        app.recompute();
        assert!(app.error.is_some());
        assert!(app.matches.is_empty());
    }

    #[test]
    fn char_offsets_respect_unicode_before_match() {
        // "héllo English-only text 123" -> "123" starts after "héllo English-only text " = 5 + 1 + 2 + 1 = 9 chars.
        let mut app = App::from_text("héllo English-only text 123\n");
        app.pattern.set(r"\d+");
        app.recompute();
        assert_eq!(app.matches.len(), 1);
        assert_eq!((app.matches[0].start, app.matches[0].end), (9, 12));
    }
}
