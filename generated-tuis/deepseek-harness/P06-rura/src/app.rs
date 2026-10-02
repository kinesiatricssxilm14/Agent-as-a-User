//! Application state and input handling.

use std::fs;
use std::path::{Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::completion;
use crate::editor::Editor;
use crate::pipeline;
use crate::runner;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    SavePrompt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Header,
    Out,
    Err,
    Status,
    Separator,
    Info,
}

pub struct OutputLine {
    text: String,
    kind: LineKind,
}

impl OutputLine {
    fn header(s: &str) -> Self {
        Self { text: format!("$ {s}"), kind: LineKind::Header }
    }
    fn partial_header(s: &str) -> Self {
        Self { text: format!("$ [partial] {s}"), kind: LineKind::Header }
    }
    fn out(s: &str) -> Self {
        Self { text: s.to_string(), kind: LineKind::Out }
    }
    fn err(s: &str) -> Self {
        Self { text: s.to_string(), kind: LineKind::Err }
    }
    fn status(s: &str) -> Self {
        Self { text: s.to_string(), kind: LineKind::Status }
    }
    fn info(s: &str) -> Self {
        Self { text: s.to_string(), kind: LineKind::Info }
    }
    fn separator() -> Self {
        Self { text: "─".repeat(80), kind: LineKind::Separator }
    }
    pub fn render(&self) -> Line<'static> {
        let style = match self.kind {
            LineKind::Header => Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            LineKind::Out => Style::default().fg(Color::White),
            LineKind::Err => Style::default().fg(Color::LightRed),
            LineKind::Status => Style::default().fg(Color::Cyan),
            LineKind::Separator => Style::default().fg(Color::DarkGray),
            LineKind::Info => Style::default().fg(Color::Gray),
        };
        Line::from(Span::styled(self.text.clone(), style))
    }
}

pub struct App {
    pub editor: Editor,
    pub save_editor: Editor,
    pub file: PathBuf,
    pub outputs: Vec<OutputLine>,
    pub scroll: usize,
    pub follow: bool,
    pub status: String,
    pub help_open: bool,
    pub help_scroll: usize,
    pub mode: Mode,
    pub history: Vec<String>,
    pub history_pos: usize,
    pub draft: String,
    pub completion: Option<completion::CompState>,
    pub save_completion: Option<completion::CompState>,
    pub should_quit: bool,
    pub last_run: Option<runner::RunResult>,
}

impl App {
    pub fn new(file: PathBuf) -> Self {
        let outputs = vec![
            OutputLine::info(&format!("toolf ready — log file: {}", file.display())),
            OutputLine::info(
                "type a pipeline (e.g. cat <file> | grep PATTERN | tail -N) and press Enter; F1 for help",
            ),
        ];
        Self {
            editor: Editor::new(),
            save_editor: Editor::new(),
            file,
            outputs,
            scroll: 0,
            follow: true,
            status: "type a pipeline and press Enter to run · F1 for help".to_string(),
            help_open: false,
            help_scroll: 0,
            mode: Mode::Normal,
            history: Vec::new(),
            history_pos: 0,
            draft: String::new(),
            completion: None,
            save_completion: None,
            should_quit: false,
            last_run: None,
        }
    }

    pub fn preview_segments(&self) -> Vec<pipeline::Segment> {
        pipeline::split_pipeline(&self.editor.to_string())
    }

    pub fn cursor_segment_index(&self) -> Option<usize> {
        pipeline::segment_at(&self.editor.to_string(), self.editor.cursor_byte())
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }

        if self.help_open {
            match key.code {
                KeyCode::Esc | KeyCode::Enter => self.help_open = false,
                KeyCode::F(1) => self.help_open = false,
                KeyCode::Char('q') | KeyCode::Char('h') | KeyCode::Char('?') => self.help_open = false,
                KeyCode::PageDown | KeyCode::Down => self.help_scroll = self.help_scroll.saturating_add(1),
                KeyCode::PageUp | KeyCode::Up => self.help_scroll = self.help_scroll.saturating_sub(1),
                _ => {}
            }
            return;
        }

        match self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::SavePrompt => self.handle_save_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        let code = key.code;
        let mods = key.modifiers;
        let is_tab = matches!(code, KeyCode::Tab | KeyCode::BackTab);
        if !is_tab {
            self.completion = None;
        }

        if mods == KeyModifiers::CONTROL {
            match code {
                KeyCode::Char('c') | KeyCode::Char('q') => self.should_quit = true,
                KeyCode::Char('s') => self.start_save(),
                KeyCode::Char('w') => self.editor.delete_word_before(),
                KeyCode::Char('u') => self.editor.clear(),
                KeyCode::Char('a') => self.editor.move_home(),
                KeyCode::Char('e') => self.editor.move_end(),
                KeyCode::Char('k') => self.editor.delete_to_end(),
                KeyCode::Char('l') => {
                    self.outputs.clear();
                    self.status = "output cleared".to_string();
                }
                KeyCode::Char('o') | KeyCode::Char('p') => self.run_partial(),
                _ => {}
            }
            return;
        }

        if mods == KeyModifiers::ALT {
            match code {
                KeyCode::Char('\\') => self.run_partial(),
                _ => {}
            }
            return;
        }

        match code {
            KeyCode::Enter => self.run_full(),
            KeyCode::Tab | KeyCode::BackTab => {
                self.status = self.do_complete();
            }
            KeyCode::Esc => {}
            KeyCode::Left => self.editor.move_left(),
            KeyCode::Right => self.editor.move_right(),
            KeyCode::Home => self.editor.move_home(),
            KeyCode::End => self.editor.move_end(),
            KeyCode::Up => self.history_up(),
            KeyCode::Down => self.history_down(),
            KeyCode::Backspace => self.editor.backspace(),
            KeyCode::Delete => self.editor.delete(),
            KeyCode::PageUp => {
                self.follow = false;
                self.scroll = self.scroll.saturating_sub(10);
            }
            KeyCode::PageDown => {
                self.follow = false;
                self.scroll = self.scroll.saturating_add(10);
            }
            KeyCode::F(1) => {
                self.help_open = true;
                self.help_scroll = 0;
            }
            KeyCode::Char(c) if mods == KeyModifiers::NONE || mods == KeyModifiers::SHIFT => {
                if c == '?' && mods.contains(KeyModifiers::SHIFT) {
                    self.help_open = true;
                    self.help_scroll = 0;
                } else {
                    self.editor.insert_char(c);
                }
            }
            _ => {}
        }
    }

    fn handle_save_key(&mut self, key: KeyEvent) {
        let code = key.code;
        let mods = key.modifiers;
        let is_tab = matches!(code, KeyCode::Tab | KeyCode::BackTab);
        if !is_tab {
            self.save_completion = None;
        }

        if mods == KeyModifiers::CONTROL {
            match code {
                KeyCode::Char('c') | KeyCode::Char('q') => self.should_quit = true,
                _ => {}
            }
            return;
        }

        match code {
            KeyCode::Enter => self.confirm_save(),
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.status = "save cancelled".to_string();
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.status = self.do_complete();
            }
            KeyCode::Left => self.save_editor.move_left(),
            KeyCode::Right => self.save_editor.move_right(),
            KeyCode::Home => self.save_editor.move_home(),
            KeyCode::End => self.save_editor.move_end(),
            KeyCode::Backspace => self.save_editor.backspace(),
            KeyCode::Delete => self.save_editor.delete(),
            KeyCode::Char(c) if mods == KeyModifiers::NONE || mods == KeyModifiers::SHIFT => {
                self.save_editor.insert_char(c);
            }
            _ => {}
        }
    }

    fn run_full(&mut self) {
        let cmd = self.editor.to_string();
        if cmd.trim().is_empty() {
            self.status = "nothing to run — the command line is empty".to_string();
            return;
        }
        self.execute(cmd.clone(), false);
        if self.history.last().map(|s| s != &cmd).unwrap_or(true) {
            self.history.push(cmd);
        }
        self.history_pos = self.history.len();
    }

    fn run_partial(&mut self) {
        let full = self.editor.to_string();
        if full.trim().is_empty() {
            self.status = "nothing to run — the command line is empty".to_string();
            return;
        }
        let cursor = self.editor.cursor_byte();
        let prefix = pipeline::partial_prefix(&full, cursor);
        let prefix = prefix.trim().to_string();
        if prefix.is_empty() {
            self.status = "nothing to execute before the cursor".to_string();
            return;
        }
        self.execute(prefix, true);
    }

    fn execute(&mut self, cmd: String, partial: bool) {
        self.outputs.push(OutputLine::separator());
        if partial {
            self.outputs.push(OutputLine::partial_header(&cmd));
        } else {
            self.outputs.push(OutputLine::header(&cmd));
        }

        let result = runner::run(&cmd);
        for line in result.stdout.lines() {
            self.outputs.push(OutputLine::out(line));
        }
        for line in result.stderr.lines() {
            self.outputs.push(OutputLine::err(line));
        }
        let code = result
            .code
            .map(|c| c.to_string())
            .unwrap_or_else(|| "terminated by signal".to_string());
        self.outputs.push(OutputLine::status(&format!(
            "exit code: {code}  ({:.3}s)",
            result.duration.as_secs_f64()
        )));
        if result.truncated {
            self.outputs.push(OutputLine::info("output truncated to 1 MiB per stream"));
        }
        self.status = format!("exit code: {code}");
        self.last_run = Some(result);
        self.follow = true;
    }

    fn start_save(&mut self) {
        if self.last_run.is_none() {
            self.status = "nothing to save yet — run a pipeline first".to_string();
            return;
        }
        self.mode = Mode::SavePrompt;
        self.save_editor = Editor::new();
        self.save_completion = None;
        self.status = "enter a path and press Enter to save, Esc to cancel".to_string();
    }

    fn current_output(&self) -> Option<String> {
        let r = self.last_run.as_ref()?;
        let mut s = if r.stdout.trim().is_empty() && !r.stderr.trim().is_empty() {
            r.stderr.clone()
        } else {
            r.stdout.clone()
        };
        if !s.ends_with('\n') {
            s.push('\n');
        }
        Some(s)
    }

    fn confirm_save(&mut self) {
        let path = self.save_editor.to_string();
        let path = path.trim().to_string();
        self.mode = Mode::Normal;

        if path.is_empty() {
            self.status = "save cancelled: empty path".to_string();
            return;
        }
        let Some(content) = self.current_output() else {
            self.status = "nothing to save — run a pipeline first".to_string();
            return;
        };

        let p = Path::new(&path);
        if let Some(parent) = p.parent() {
            if !parent.as_os_str().is_empty() {
                if let Err(e) = fs::create_dir_all(parent) {
                    self.status = format!("save failed: could not create directory: {e}");
                    return;
                }
            }
        }
        match fs::write(p, content.as_bytes()) {
            Ok(()) => self.status = format!("saved {} bytes to {path}", content.len()),
            Err(e) => self.status = format!("save failed: {e}"),
        }
    }

    fn do_complete(&mut self) -> String {
        match self.mode {
            Mode::Normal => {
                let chars = self.editor.chars().to_vec();
                let cursor = self.editor.cursor();
                let res = completion::compute(&chars, cursor);
                completion::apply(&mut self.editor, &mut self.completion, res)
            }
            Mode::SavePrompt => {
                let chars = self.save_editor.chars().to_vec();
                let cursor = self.save_editor.cursor();
                let res = completion::compute_path(&chars, cursor);
                completion::apply(&mut self.save_editor, &mut self.save_completion, res)
            }
        }
    }

    fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        if self.history_pos == self.history.len() {
            self.draft = self.editor.to_string();
        }
        if self.history_pos > 0 {
            self.history_pos -= 1;
            self.editor.set_text(&self.history[self.history_pos]);
        }
        self.status = if self.history_pos < self.history.len() {
            format!("history {}/{}", self.history_pos + 1, self.history.len())
        } else {
            String::new()
        };
    }

    fn history_down(&mut self) {
        if self.history_pos >= self.history.len() {
            return;
        }
        self.history_pos += 1;
        if self.history_pos == self.history.len() {
            self.editor.set_text(&self.draft);
            self.status = "back to current draft".to_string();
        } else {
            self.editor.set_text(&self.history[self.history_pos]);
            self.status = format!("history {}/{}", self.history_pos + 1, self.history.len());
        }
    }
}
