use std::{fs, io, path::PathBuf, time::Duration};

use clap::Parser;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
    Frame, Terminal,
};
use regex::{Regex, RegexBuilder};
use unicode_width::UnicodeWidthStr;

const PHONE_REGEX: &str = r"(?:\+?86[- ]?)?1[3-9]\d{9}";

#[derive(Parser, Debug)]
#[command(
    name = "toole",
    version,
    about = "Interactive regular expression tester"
)]
struct Args {
    /// Input text file
    #[arg(short = 'f', long = "file", default_value = "/bench/data/input.txt")]
    file: PathBuf,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Focus {
    Regex,
    Replacement,
    Source,
    Matches,
    Preview,
}

impl Focus {
    fn next(self) -> Self {
        match self {
            Self::Regex => Self::Replacement,
            Self::Replacement => Self::Source,
            Self::Source => Self::Matches,
            Self::Matches => Self::Preview,
            Self::Preview => Self::Regex,
        }
    }
    fn previous(self) -> Self {
        match self {
            Self::Regex => Self::Preview,
            Self::Replacement => Self::Regex,
            Self::Source => Self::Replacement,
            Self::Matches => Self::Source,
            Self::Preview => Self::Matches,
        }
    }
    fn is_input(self) -> bool {
        matches!(self, Self::Regex | Self::Replacement)
    }
}

#[derive(Debug, Clone)]
struct MatchInfo {
    byte_start: usize,
    byte_end: usize,
    char_start: usize,
    char_end: usize,
    text: String,
}

struct App {
    file: PathBuf,
    source: String,
    pattern: String,
    replacement: String,
    pattern_cursor: usize,
    replacement_cursor: usize,
    case_insensitive: bool,
    focus: Focus,
    regex: Option<Regex>,
    regex_error: Option<String>,
    matches: Vec<MatchInfo>,
    replaced: String,
    source_scroll: u16,
    match_scroll: usize,
    preview_scroll: u16,
    help: bool,
}

impl App {
    fn new(file: PathBuf, source: String) -> Self {
        let mut app = Self {
            file,
            source: source.clone(),
            pattern: String::new(),
            replacement: String::new(),
            pattern_cursor: 0,
            replacement_cursor: 0,
            case_insensitive: false,
            focus: Focus::Regex,
            regex: None,
            regex_error: None,
            matches: Vec::new(),
            replaced: source,
            source_scroll: 0,
            match_scroll: 0,
            preview_scroll: 0,
            help: false,
        };
        app.recompute();
        app
    }

    fn recompute(&mut self) {
        self.regex_error = None;
        self.matches.clear();
        self.match_scroll = self.match_scroll.min(self.matches.len().saturating_sub(1));
        if self.pattern.is_empty() {
            self.regex = None;
            self.replaced = self.source.clone();
            return;
        }
        match RegexBuilder::new(&self.pattern)
            .case_insensitive(self.case_insensitive)
            .build()
        {
            Ok(regex) => {
                self.matches = regex
                    .find_iter(&self.source)
                    .map(|m| MatchInfo {
                        byte_start: m.start(),
                        byte_end: m.end(),
                        char_start: byte_to_char(&self.source, m.start()),
                        char_end: byte_to_char(&self.source, m.end()),
                        text: m.as_str().to_owned(),
                    })
                    .collect();
                self.replaced = regex
                    .replace_all(&self.source, self.replacement.as_str())
                    .into_owned();
                self.regex = Some(regex);
            }
            Err(err) => {
                self.regex = None;
                self.replaced = self.source.clone();
                self.regex_error = Some(err.to_string());
            }
        }
    }

    fn active_input_mut(&mut self) -> Option<(&mut String, &mut usize)> {
        match self.focus {
            Focus::Regex => Some((&mut self.pattern, &mut self.pattern_cursor)),
            Focus::Replacement => Some((&mut self.replacement, &mut self.replacement_cursor)),
            _ => None,
        }
    }

    fn insert_char(&mut self, ch: char) {
        if let Some((text, cursor)) = self.active_input_mut() {
            let byte = char_to_byte(text, *cursor);
            text.insert(byte, ch);
            *cursor += 1;
            self.recompute();
        }
    }

    fn backspace(&mut self) {
        if let Some((text, cursor)) = self.active_input_mut() {
            if *cursor > 0 {
                let end = char_to_byte(text, *cursor);
                let start = char_to_byte(text, *cursor - 1);
                text.replace_range(start..end, "");
                *cursor -= 1;
                self.recompute();
            }
        }
    }

    fn delete(&mut self) {
        if let Some((text, cursor)) = self.active_input_mut() {
            if *cursor < text.chars().count() {
                let start = char_to_byte(text, *cursor);
                let end = char_to_byte(text, *cursor + 1);
                text.replace_range(start..end, "");
                self.recompute();
            }
        }
    }

    fn clear_input(&mut self) {
        if let Some((text, cursor)) = self.active_input_mut() {
            text.clear();
            *cursor = 0;
            self.recompute();
        }
    }

    fn scroll(&mut self, delta: i32) {
        match self.focus {
            Focus::Source => self.source_scroll = add_scroll(self.source_scroll, delta),
            Focus::Matches => {
                let max = self.matches.len().saturating_sub(1);
                self.match_scroll =
                    (self.match_scroll as i32 + delta).clamp(0, max as i32) as usize;
            }
            Focus::Preview => self.preview_scroll = add_scroll(self.preview_scroll, delta),
            _ => {}
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let source = fs::read_to_string(&args.file).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("cannot read input file '{}': {e}", args.file.display()),
        )
    })?;
    let mut app = App::new(args.file, source);

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run(&mut terminal, &mut app);
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result?;
    Ok(())
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if handle_key(app, key) {
                    return Ok(());
                }
            }
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return true;
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p') {
        app.pattern = PHONE_REGEX.to_owned();
        app.pattern_cursor = app.pattern.chars().count();
        app.focus = Focus::Regex;
        app.recompute();
        return false;
    }
    if key.code == KeyCode::F(2) {
        app.case_insensitive = !app.case_insensitive;
        app.recompute();
        return false;
    }
    if key.code == KeyCode::Char('?') && !app.focus.is_input() {
        app.help = !app.help;
        return false;
    }
    if key.code == KeyCode::Tab {
        app.focus = if key.modifiers.contains(KeyModifiers::SHIFT) {
            app.focus.previous()
        } else {
            app.focus.next()
        };
        return false;
    }
    if key.code == KeyCode::BackTab {
        app.focus = app.focus.previous();
        return false;
    }
    if key.code == KeyCode::Esc {
        if app.help {
            app.help = false;
        } else if app.focus.is_input() {
            app.focus = Focus::Source;
        } else {
            app.focus = Focus::Regex;
        }
        return false;
    }

    if app.focus.is_input() {
        match key.code {
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                app.clear_input()
            }
            KeyCode::Char(ch)
                if !key.modifiers.contains(KeyModifiers::CONTROL)
                    && !key.modifiers.contains(KeyModifiers::ALT) =>
            {
                app.insert_char(ch)
            }
            KeyCode::Backspace => app.backspace(),
            KeyCode::Delete => app.delete(),
            KeyCode::Left => {
                if let Some((_, cursor)) = app.active_input_mut() {
                    *cursor = cursor.saturating_sub(1);
                }
            }
            KeyCode::Right => {
                if let Some((text, cursor)) = app.active_input_mut() {
                    *cursor = (*cursor + 1).min(text.chars().count());
                }
            }
            KeyCode::Home => {
                if let Some((_, cursor)) = app.active_input_mut() {
                    *cursor = 0;
                }
            }
            KeyCode::End => {
                if let Some((text, cursor)) = app.active_input_mut() {
                    *cursor = text.chars().count();
                }
            }
            _ => {}
        }
    } else {
        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('?') => app.help = !app.help,
            KeyCode::Up | KeyCode::Char('k') => app.scroll(-1),
            KeyCode::Down | KeyCode::Char('j') => app.scroll(1),
            KeyCode::PageUp => app.scroll(-10),
            KeyCode::PageDown => app.scroll(10),
            KeyCode::Home => app.scroll(-i32::MAX),
            KeyCode::End => app.scroll(i32::MAX),
            _ => {}
        }
    }
    false
}

fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < 50 || area.height < 16 {
        frame.render_widget(
            Paragraph::new("Terminal too small\nResize to at least 50×16\nCtrl+C: quit")
                .style(Style::default().fg(Color::Yellow))
                .block(Block::default().title(" toole ").borders(Borders::ALL)),
            area,
        );
        return;
    }

    let help_height = if app.help { 5 } else { 0 };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(7),
            Constraint::Length(help_height),
            Constraint::Length(2),
        ])
        .split(area);

    draw_input(
        frame,
        app,
        rows[0],
        Focus::Regex,
        " Regex pattern (live) ",
        &app.pattern,
        app.pattern_cursor,
    );
    draw_input(
        frame,
        app,
        rows[1],
        Focus::Replacement,
        " Replacement preview rule ($1 supported) ",
        &app.replacement,
        app.replacement_cursor,
    );

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(rows[2]);
    let left = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(cols[0]);
    draw_source(frame, app, left[0]);
    draw_preview(frame, app, left[1]);
    draw_matches(frame, app, cols[1]);

    if app.help {
        let help = Paragraph::new(vec![
            Line::from("Tab/Shift+Tab: focus panels   arrows or j/k: scroll focused panel   PgUp/PgDn/Home/End: navigate"),
            Line::from("F2: case-insensitive toggle   Ctrl+P: mainland China phone preset   Ctrl+U: clear active input"),
            Line::from("Esc: leave input/close help   ?: toggle this help (outside inputs)   q or Ctrl+C: quit"),
        ])
        .style(Style::default().fg(Color::Cyan))
        .block(Block::default().title(" Keyboard help ").borders(Borders::ALL));
        frame.render_widget(help, rows[3]);
    }

    let status = if let Some(err) = &app.regex_error {
        format!(" INVALID REGEX: {}", one_line(err))
    } else if app.pattern.is_empty() {
        " Enter a regex to begin — inline flags such as (?i) are supported".to_owned()
    } else {
        format!(
            " {} match{} • offsets: 0-indexed character ranges [start, end) • {}",
            app.matches.len(),
            if app.matches.len() == 1 { "" } else { "es" },
            app.file.display()
        )
    };
    let status_style = if app.regex_error.is_some() {
        Style::default()
            .fg(Color::White)
            .bg(Color::Red)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Black).bg(Color::Cyan)
    };
    let shortcuts = format!(
        " Tab: focus  Esc: leave input  F2: ignore case [{}]  Ctrl+P: phone regex  ?: help  Ctrl+C: quit",
        if app.case_insensitive { "ON" } else { "OFF" }
    );
    frame.render_widget(
        Paragraph::new(vec![Line::from(status), Line::from(shortcuts)]).style(status_style),
        rows[4],
    );
}

fn draw_input(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    focus: Focus,
    title: &str,
    value: &str,
    cursor: usize,
) {
    let active = app.focus == focus;
    let border = if active { Color::Cyan } else { Color::DarkGray };
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(border).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border));
    let inner_width = area.width.saturating_sub(2) as usize;
    let cursor_byte = char_to_byte(value, cursor);
    let before = &value[..cursor_byte];
    let cursor_col = UnicodeWidthStr::width(before);
    let horizontal = cursor_col.saturating_sub(inner_width.saturating_sub(1));
    frame.render_widget(
        Paragraph::new(value)
            .block(block)
            .scroll((0, horizontal as u16)),
        area,
    );
    if active {
        let visible_col = cursor_col.saturating_sub(horizontal) as u16;
        frame.set_cursor_position((
            area.x + 1 + visible_col.min(area.width.saturating_sub(2)),
            area.y + 1,
        ));
    }
}

fn panel_block(title: String, active: bool) -> Block<'static> {
    let color = if active { Color::Cyan } else { Color::DarkGray };
    Block::default()
        .title(title)
        .title_style(Style::default().fg(color).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(color))
}

fn draw_source(frame: &mut Frame, app: &App, area: Rect) {
    let lines = highlighted_lines(&app.source, &app.matches);
    let total = lines.len();
    let title = format!(
        " Source — highlighted matches ({}/{}) ",
        app.source_scroll.saturating_add(1),
        total.max(1)
    );
    let paragraph = Paragraph::new(lines)
        .block(panel_block(title, app.focus == Focus::Source))
        .scroll((app.source_scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
    draw_scrollbar(frame, area, app.source_scroll as usize, total);
}

fn draw_preview(frame: &mut Frame, app: &App, area: Rect) {
    let lines: Vec<Line> = app.replaced.split('\n').map(Line::from).collect();
    let total = lines.len();
    let title = format!(
        " Complete replacement preview ({}/{}) ",
        app.preview_scroll.saturating_add(1),
        total.max(1)
    );
    let paragraph = Paragraph::new(lines)
        .block(panel_block(title, app.focus == Focus::Preview))
        .scroll((app.preview_scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
    draw_scrollbar(frame, area, app.preview_scroll as usize, total);
}

fn draw_matches(frame: &mut Frame, app: &App, area: Rect) {
    let lines = if app.regex_error.is_some() {
        vec![Line::styled(
            "Fix the regex to see matches.",
            Style::default().fg(Color::Red),
        )]
    } else if app.pattern.is_empty() {
        vec![Line::styled(
            "Matches appear here as you type.",
            Style::default().fg(Color::DarkGray),
        )]
    } else if app.matches.is_empty() {
        vec![Line::styled(
            "No matches.",
            Style::default().fg(Color::Yellow),
        )]
    } else {
        app.matches
            .iter()
            .enumerate()
            .map(|(i, m)| {
                let escaped = m
                    .text
                    .replace('\n', "\\n")
                    .replace('\r', "\\r")
                    .replace('\t', "\\t");
                Line::from(vec![
                    Span::styled(format!("#{:<4}", i + 1), Style::default().fg(Color::Cyan)),
                    Span::styled(
                        format!("[{}, {})  ", m.char_start, m.char_end),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(escaped),
                ])
            })
            .collect()
    };
    let title = format!(" All matches + character offsets ({}) ", app.matches.len());
    let paragraph = Paragraph::new(lines)
        .block(panel_block(title, app.focus == Focus::Matches))
        .scroll((app.match_scroll as u16, 0));
    frame.render_widget(paragraph, area);
    draw_scrollbar(frame, area, app.match_scroll, app.matches.len());
}

fn draw_scrollbar(frame: &mut Frame, area: Rect, position: usize, total: usize) {
    if total > area.height.saturating_sub(2) as usize {
        let mut state = ScrollbarState::new(total).position(position);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            area.inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut state,
        );
    }
}

fn highlighted_lines<'a>(text: &'a str, matches: &[MatchInfo]) -> Vec<Line<'a>> {
    let normal = Style::default();
    let highlight = Style::default()
        .fg(Color::Black)
        .bg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let mut lines: Vec<Line<'a>> = vec![Line::default()];
    let mut cursor = 0;
    for m in matches {
        if m.byte_start > cursor {
            append_segment(&mut lines, &text[cursor..m.byte_start], normal);
        }
        if m.byte_end > m.byte_start {
            append_segment(&mut lines, &text[m.byte_start..m.byte_end], highlight);
        }
        cursor = m.byte_end;
    }
    if cursor < text.len() {
        append_segment(&mut lines, &text[cursor..], normal);
    }
    lines
}

fn append_segment<'a>(lines: &mut Vec<Line<'a>>, segment: &'a str, style: Style) {
    for (i, part) in segment.split('\n').enumerate() {
        if i > 0 {
            lines.push(Line::default());
        }
        if !part.is_empty() {
            lines
                .last_mut()
                .expect("at least one line")
                .spans
                .push(Span::styled(part, style));
        }
    }
}

fn byte_to_char(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}
fn char_to_byte(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(i, _)| i)
        .unwrap_or(text.len())
}
fn add_scroll(value: u16, delta: i32) -> u16 {
    (value as i32)
        .saturating_add(delta)
        .clamp(0, u16::MAX as i32) as u16
}
fn one_line(text: &str) -> String {
    text.lines().next().unwrap_or(text).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_are_character_based() {
        let mut app = App::new("x".into(), "éEnglish-only text abc".into());
        app.pattern = "abc".into();
        app.recompute();
        assert_eq!((app.matches[0].char_start, app.matches[0].char_end), (3, 6));
    }

    #[test]
    fn replacement_keeps_unmatched_content_and_groups() {
        let mut app = App::new("x".into(), "one 12\ntwo 34".into());
        app.pattern = r"(\d+)".into();
        app.replacement = "[$1]".into();
        app.recompute();
        assert_eq!(app.replaced, "one [12]\ntwo [34]");
    }

    #[test]
    fn case_insensitive_and_inline_flags_work() {
        let mut app = App::new("x".into(), "xxFoOyy".into());
        app.pattern = "foo".into();
        app.case_insensitive = true;
        app.recompute();
        assert_eq!(app.matches[0].text, "FoO");
        app.pattern = "(?i)foo".into();
        app.case_insensitive = false;
        app.recompute();
        assert_eq!(app.matches.len(), 1);
    }

    #[test]
    fn phone_preset_matches_mainland_mobile() {
        let re = Regex::new(PHONE_REGEX).unwrap();
        assert_eq!(
            re.find("call +86 13812345678 now").unwrap().as_str(),
            "+86 13812345678"
        );
    }
}
