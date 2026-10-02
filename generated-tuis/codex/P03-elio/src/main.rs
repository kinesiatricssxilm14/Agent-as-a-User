use std::{
    cmp::Ordering,
    env,
    fs::{self, DirEntry, FileType, Metadata},
    io::{self, IsTerminal},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState, Wrap,
    },
    Frame, Terminal,
};

const DEFAULT_DIRECTORY: &str = "/bench/data/src";
const PREVIEW_TAB_WIDTH: usize = 4;

fn main() -> Result<()> {
    let requested = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_DIRECTORY));
    let start_dir = fs::canonicalize(&requested)
        .with_context(|| format!("cannot open directory {}", requested.display()))?;
    if !start_dir.is_dir() {
        anyhow::bail!("{} is not a directory", start_dir.display());
    }
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        anyhow::bail!("toolc requires an interactive terminal");
    }

    let mut terminal = TerminalGuard::new()?;
    let mut app = App::new(start_dir);
    app.reload(None);

    let result = run(&mut terminal.terminal, &mut app);
    terminal.restore()?;
    result
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    restored: bool,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
        Ok(Self {
            terminal,
            restored: false,
        })
    }

    fn restore(&mut self) -> Result<()> {
        if !self.restored {
            disable_raw_mode()?;
            execute!(self.terminal.backend_mut(), LeaveAlternateScreen)?;
            self.terminal.show_cursor()?;
            self.restored = true;
        }
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        terminal.draw(|frame| draw(frame, app))?;
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if handle_key(app, key) {
                        break;
                    }
                }
                Event::Resize(_, _) => {}
                _ => {}
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Files,
    Preview,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Operation {
    Copy,
    Move,
    Rename,
    Mkdir,
    Delete,
}

impl Operation {
    fn title(self) -> &'static str {
        match self {
            Self::Copy => "Copy selected file",
            Self::Move => "Move selected item",
            Self::Rename => "Rename selected item",
            Self::Mkdir => "Create directory",
            Self::Delete => "Delete selected item",
        }
    }

    fn prompt(self) -> &'static str {
        match self {
            Self::Copy => "Destination path",
            Self::Move => "Destination directory/path",
            Self::Rename => "New name/path",
            Self::Mkdir => "Directory path",
            Self::Delete => "Type DELETE to confirm",
        }
    }
}

struct InputMode {
    operation: Operation,
    value: String,
    cursor: usize,
    source: Option<PathBuf>,
}

struct Entry {
    path: PathBuf,
    name: String,
    file_type: FileType,
    metadata: Option<Metadata>,
}

impl Entry {
    fn from_dir_entry(entry: DirEntry) -> io::Result<Self> {
        let path = entry.path();
        let file_type = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let metadata = entry.metadata().ok();
        Ok(Self {
            path,
            name,
            file_type,
            metadata,
        })
    }

    fn kind_marker(&self) -> &'static str {
        if self.file_type.is_dir() {
            "d"
        } else if self.file_type.is_symlink() {
            "l"
        } else if self.file_type.is_file() {
            "f"
        } else {
            "?"
        }
    }
}

struct Preview {
    title: String,
    lines: Vec<String>,
    binary: bool,
}

impl Preview {
    fn empty() -> Self {
        Self {
            title: "Preview".to_owned(),
            lines: vec!["Select a file to preview its full content.".to_owned()],
            binary: false,
        }
    }
}

struct App {
    current_dir: PathBuf,
    entries: Vec<Entry>,
    selected: usize,
    list_state: ListState,
    preview: Preview,
    preview_scroll: usize,
    preview_view_height: usize,
    preview_view_width: usize,
    focus: Focus,
    input: Option<InputMode>,
    status: String,
    status_error: bool,
    show_help: bool,
}

impl App {
    fn new(current_dir: PathBuf) -> Self {
        Self {
            current_dir,
            entries: Vec::new(),
            selected: 0,
            list_state: ListState::default(),
            preview: Preview::empty(),
            preview_scroll: 0,
            preview_view_height: 1,
            preview_view_width: 1,
            focus: Focus::Files,
            input: None,
            status: "Ready".to_owned(),
            status_error: false,
            show_help: false,
        }
    }

    fn selected_entry(&self) -> Option<&Entry> {
        self.entries.get(self.selected)
    }

    fn selected_path(&self) -> Option<PathBuf> {
        self.selected_entry().map(|entry| entry.path.clone())
    }

    fn set_status(&mut self, message: impl Into<String>, error: bool) {
        self.status = message.into();
        self.status_error = error;
    }

    fn reload(&mut self, preferred_name: Option<&str>) {
        match read_directory(&self.current_dir) {
            Ok(entries) => {
                self.entries = entries;
                self.selected = preferred_name
                    .and_then(|name| self.entries.iter().position(|entry| entry.name == name))
                    .unwrap_or_else(|| self.selected.min(self.entries.len().saturating_sub(1)));
                self.sync_selection();
                self.load_preview();
            }
            Err(error) => {
                self.entries.clear();
                self.selected = 0;
                self.sync_selection();
                self.preview = Preview::empty();
                self.set_status(
                    format!("Cannot read {}: {error}", self.current_dir.display()),
                    true,
                );
            }
        }
    }

    fn sync_selection(&mut self) {
        if self.entries.is_empty() {
            self.list_state.select(None);
        } else {
            self.selected = self.selected.min(self.entries.len() - 1);
            self.list_state.select(Some(self.selected));
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.entries.is_empty() {
            return;
        }
        let max = self.entries.len() as isize - 1;
        self.selected = (self.selected as isize + delta).clamp(0, max) as usize;
        self.sync_selection();
        self.load_preview();
    }

    fn select_first(&mut self) {
        if !self.entries.is_empty() {
            self.selected = 0;
            self.sync_selection();
            self.load_preview();
        }
    }

    fn select_last(&mut self) {
        if !self.entries.is_empty() {
            self.selected = self.entries.len() - 1;
            self.sync_selection();
            self.load_preview();
        }
    }

    fn load_preview(&mut self) {
        self.preview_scroll = 0;
        let Some(entry) = self.selected_entry() else {
            self.preview = Preview::empty();
            return;
        };

        let title = format!("Preview — {}", entry.path.display());
        if entry.file_type.is_dir() {
            let count = fs::read_dir(&entry.path)
                .map(|iter| iter.filter_map(Result::ok).count())
                .ok();
            self.preview = Preview {
                title,
                lines: vec![
                    format!("Directory: {}", entry.path.display()),
                    count.map_or_else(
                        || "Contents: unavailable".to_owned(),
                        |value| format!("Contents: {value} item(s)"),
                    ),
                    String::new(),
                    "Press Enter or → to open this directory.".to_owned(),
                ],
                binary: false,
            };
            return;
        }

        if !entry.file_type.is_file() {
            let target = fs::read_link(&entry.path)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| "unavailable".to_owned());
            self.preview = Preview {
                title,
                lines: vec![
                    format!("Special file: {}", entry.path.display()),
                    format!("Link target: {target}"),
                ],
                binary: false,
            };
            return;
        }

        match fs::read(&entry.path) {
            Ok(bytes) => {
                let binary = looks_binary(&bytes);
                let text = String::from_utf8_lossy(&bytes);
                let mut lines: Vec<String> = text
                    .split('\n')
                    .map(|line| expand_tabs(line.trim_end_matches('\r')))
                    .collect();
                if lines.is_empty() {
                    lines.push(String::new());
                }
                self.preview = Preview {
                    title,
                    lines,
                    binary,
                };
            }
            Err(error) => {
                self.preview = Preview {
                    title,
                    lines: vec![format!("Unable to read file: {error}")],
                    binary: false,
                };
            }
        }
    }

    fn open_selected(&mut self) {
        let Some(path) = self.selected_path() else {
            return;
        };
        if path.is_dir() {
            match fs::canonicalize(&path) {
                Ok(directory) => {
                    self.current_dir = directory;
                    self.selected = 0;
                    self.reload(None);
                    self.set_status(format!("Opened {}", self.current_dir.display()), false);
                }
                Err(error) => {
                    self.set_status(format!("Cannot open {}: {error}", path.display()), true)
                }
            }
        } else {
            self.focus = Focus::Preview;
            self.set_status(
                "Preview focused; use arrows/PageUp/PageDown to scroll",
                false,
            );
        }
    }

    fn go_parent(&mut self) {
        let old_name = self
            .current_dir
            .file_name()
            .map(|value| value.to_string_lossy().into_owned());
        let Some(parent) = self.current_dir.parent().map(Path::to_path_buf) else {
            self.set_status("Already at filesystem root", false);
            return;
        };
        self.current_dir = parent;
        self.selected = 0;
        self.reload(old_name.as_deref());
        self.set_status(format!("Opened {}", self.current_dir.display()), false);
    }

    fn scroll_preview(&mut self, delta: isize) {
        let visual_line_count =
            wrap_preview_lines(&self.preview.lines, self.preview_view_width).len();
        let max = visual_line_count.saturating_sub(self.preview_view_height.max(1));
        self.preview_scroll =
            (self.preview_scroll as isize + delta).clamp(0, max as isize) as usize;
    }

    fn begin_operation(&mut self, operation: Operation) {
        let source = match operation {
            Operation::Mkdir => None,
            _ => {
                let Some(path) = self.selected_path() else {
                    self.set_status("No item selected", true);
                    return;
                };
                Some(path)
            }
        };

        if operation == Operation::Copy && source.as_ref().is_some_and(|path| !path.is_file()) {
            self.set_status("Copy currently accepts regular files", true);
            return;
        }

        let value = match operation {
            Operation::Rename => source
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            _ => String::new(),
        };
        let cursor = value.len();
        self.input = Some(InputMode {
            operation,
            value,
            cursor,
            source,
        });
        self.set_status("Enter confirms; Esc cancels", false);
    }

    fn submit_operation(&mut self) {
        let Some(input) = self.input.take() else {
            return;
        };
        let value = input.value.trim();
        if value.is_empty() {
            self.set_status("A path or confirmation value is required", true);
            return;
        }

        let result = match input.operation {
            Operation::Copy => input
                .source
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("no source selected"))
                .and_then(|source| copy_file(source, &resolve_path(&self.current_dir, value))),
            Operation::Move => input
                .source
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("no source selected"))
                .and_then(|source| move_item(source, &resolve_path(&self.current_dir, value))),
            Operation::Rename => input
                .source
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("no source selected"))
                .and_then(|source| rename_item(source, &resolve_path(&self.current_dir, value))),
            Operation::Mkdir => create_directory(&resolve_path(&self.current_dir, value)),
            Operation::Delete => {
                if value != "DELETE" {
                    Err(anyhow::anyhow!(
                        "delete cancelled: confirmation did not match"
                    ))
                } else {
                    input
                        .source
                        .as_deref()
                        .ok_or_else(|| anyhow::anyhow!("no source selected"))
                        .and_then(delete_item)
                }
            }
        };

        match result {
            Ok(message) => {
                self.reload(None);
                self.set_status(message, false);
            }
            Err(error) => self.set_status(error.to_string(), true),
        }
    }
}

fn handle_key(app: &mut App, key: KeyEvent) -> bool {
    if app.input.is_some() {
        handle_input_key(app, key);
        return false;
    }

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return true;
    }

    match key.code {
        KeyCode::Char('q') => return true,
        KeyCode::Char('?') | KeyCode::F(1) => app.show_help = !app.show_help,
        KeyCode::Esc if app.show_help => app.show_help = false,
        KeyCode::Tab => {
            app.focus = if app.focus == Focus::Files {
                Focus::Preview
            } else {
                Focus::Files
            }
        }
        KeyCode::Left | KeyCode::Backspace if app.focus == Focus::Files => app.go_parent(),
        KeyCode::Right | KeyCode::Enter if app.focus == Focus::Files => app.open_selected(),
        KeyCode::Up | KeyCode::Char('k') => {
            if app.focus == Focus::Files {
                app.move_selection(-1);
            } else {
                app.scroll_preview(-1);
            }
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if app.focus == Focus::Files {
                app.move_selection(1);
            } else {
                app.scroll_preview(1);
            }
        }
        KeyCode::PageUp => {
            let amount = if app.focus == Focus::Files {
                10
            } else {
                app.preview_view_height.max(1)
            };
            if app.focus == Focus::Files {
                app.move_selection(-(amount as isize));
            } else {
                app.scroll_preview(-(amount as isize));
            }
        }
        KeyCode::PageDown => {
            let amount = if app.focus == Focus::Files {
                10
            } else {
                app.preview_view_height.max(1)
            };
            if app.focus == Focus::Files {
                app.move_selection(amount as isize);
            } else {
                app.scroll_preview(amount as isize);
            }
        }
        KeyCode::Home => {
            if app.focus == Focus::Files {
                app.select_first();
            } else {
                app.preview_scroll = 0;
            }
        }
        KeyCode::End => {
            if app.focus == Focus::Files {
                app.select_last();
            } else {
                app.preview_scroll = wrap_preview_lines(&app.preview.lines, app.preview_view_width)
                    .len()
                    .saturating_sub(app.preview_view_height.max(1));
            }
        }
        KeyCode::Char('r') => {
            app.reload(
                app.selected_entry()
                    .map(|entry| entry.name.clone())
                    .as_deref(),
            );
            app.set_status("Directory refreshed", false);
        }
        KeyCode::Char('c') => app.begin_operation(Operation::Copy),
        KeyCode::Char('m') => app.begin_operation(Operation::Move),
        KeyCode::Char('n') => app.begin_operation(Operation::Mkdir),
        KeyCode::Char('e') => app.begin_operation(Operation::Rename),
        KeyCode::Char('d') => app.begin_operation(Operation::Delete),
        _ => {}
    }
    false
}

fn handle_input_key(app: &mut App, key: KeyEvent) {
    match key.code {
        KeyCode::Esc => {
            app.input = None;
            app.set_status("Operation cancelled", false);
        }
        KeyCode::Enter => app.submit_operation(),
        KeyCode::Left => {
            if let Some(input) = app.input.as_mut() {
                input.cursor = previous_char_boundary(&input.value, input.cursor);
            }
        }
        KeyCode::Right => {
            if let Some(input) = app.input.as_mut() {
                input.cursor = next_char_boundary(&input.value, input.cursor);
            }
        }
        KeyCode::Home => {
            if let Some(input) = app.input.as_mut() {
                input.cursor = 0;
            }
        }
        KeyCode::End => {
            if let Some(input) = app.input.as_mut() {
                input.cursor = input.value.len();
            }
        }
        KeyCode::Backspace => {
            if let Some(input) = app.input.as_mut() {
                let previous = previous_char_boundary(&input.value, input.cursor);
                input.value.replace_range(previous..input.cursor, "");
                input.cursor = previous;
            }
        }
        KeyCode::Delete => {
            if let Some(input) = app.input.as_mut() {
                let next = next_char_boundary(&input.value, input.cursor);
                input.value.replace_range(input.cursor..next, "");
            }
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            if let Some(input) = app.input.as_mut() {
                input.value.clear();
                input.cursor = 0;
            }
        }
        KeyCode::Char(character)
            if !key.modifiers.contains(KeyModifiers::CONTROL)
                && !key.modifiers.contains(KeyModifiers::ALT) =>
        {
            if let Some(input) = app.input.as_mut() {
                input.value.insert(input.cursor, character);
                input.cursor += character.len_utf8();
            }
        }
        _ => {}
    }
}

fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let input_height = if app.input.is_some() { 4 } else { 0 };
    let help_height = if app.show_help {
        7.min(area.height.saturating_sub(6))
    } else {
        0
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(3),
            Constraint::Length(input_height),
            Constraint::Length(help_height),
            Constraint::Length(2),
        ])
        .split(area);

    draw_header(frame, app, rows[0]);
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(rows[1]);
    draw_files(frame, app, panes[0]);
    draw_preview(frame, app, panes[1]);

    if app.input.is_some() {
        draw_input(frame, app, rows[2]);
    }
    if app.show_help {
        draw_help(frame, rows[3]);
    }
    draw_footer(frame, app, rows[4]);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let line = Line::from(vec![
        Span::styled(
            " toolc ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            app.current_dir.display().to_string(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(Color::DarkGray)),
        area,
    );
}

fn draw_files(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Files;
    let border = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let items: Vec<ListItem> = app
        .entries
        .iter()
        .map(|entry| {
            let marker_style = if entry.file_type.is_dir() {
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD)
            } else if entry.file_type.is_symlink() {
                Style::default().fg(Color::Magenta)
            } else {
                Style::default().fg(Color::Green)
            };
            let size = entry
                .metadata
                .as_ref()
                .filter(|_| entry.file_type.is_file())
                .map(|metadata| human_size(metadata.len()))
                .unwrap_or_default();
            ListItem::new(Line::from(vec![
                Span::styled(format!(" {} ", entry.kind_marker()), marker_style),
                Span::raw(entry.name.clone()),
                Span::styled(format!("  {size}"), Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();
    let block = Block::default()
        .title(format!(" Files ({}) ", app.entries.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border));
    let list = List::new(items)
        .block(block)
        .highlight_symbol("›")
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );
    frame.render_stateful_widget(list, area, &mut app.list_state);
}

fn draw_preview(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Preview;
    let border = if focused {
        Color::Cyan
    } else {
        Color::DarkGray
    };
    let inner_height = area.height.saturating_sub(2) as usize;
    let inner_width = area.width.saturating_sub(3) as usize;
    app.preview_view_height = inner_height.max(1);
    app.preview_view_width = inner_width.max(1);
    let wrapped_lines = wrap_preview_lines(&app.preview.lines, app.preview_view_width);
    let max_scroll = wrapped_lines.len().saturating_sub(app.preview_view_height);
    app.preview_scroll = app.preview_scroll.min(max_scroll);

    let visible = wrapped_lines
        .iter()
        .skip(app.preview_scroll)
        .take(app.preview_view_height)
        .cloned()
        .map(Line::from)
        .collect::<Vec<_>>();
    let binary_note = if app.preview.binary {
        " [binary bytes shown lossily]"
    } else {
        ""
    };
    let title = format!(
        " {}{}  line {}/{} ",
        app.preview.title,
        binary_note,
        app.preview_scroll.saturating_add(1),
        wrapped_lines.len()
    );
    let paragraph = Paragraph::new(Text::from(visible))
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border)),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);

    if wrapped_lines.len() > app.preview_view_height {
        let mut state = ScrollbarState::new(wrapped_lines.len())
            .position(app.preview_scroll)
            .viewport_content_length(app.preview_view_height);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            area.inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut state,
        );
    }
}

fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    let Some(input) = app.input.as_ref() else {
        return;
    };
    frame.render_widget(Clear, area);
    let source = input
        .source
        .as_ref()
        .map(|path| format!(" — {}", path.display()))
        .unwrap_or_default();
    let block = Block::default()
        .title(format!(" {}{} ", input.operation.title(), source))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow));
    let line = Line::from(vec![
        Span::styled(
            format!("{}: ", input.operation.prompt()),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::raw(input.value.clone()),
    ]);
    frame.render_widget(Paragraph::new(line).block(block), area);

    let prefix_width = input.operation.prompt().chars().count() as u16 + 2;
    let before_cursor = input.value[..input.cursor].chars().count() as u16;
    let x = area
        .x
        .saturating_add(1)
        .saturating_add(prefix_width)
        .saturating_add(before_cursor)
        .min(area.right().saturating_sub(2));
    frame.set_cursor_position((x, area.y.saturating_add(1)));
}

fn draw_help(frame: &mut Frame, area: Rect) {
    let text = vec![
        Line::from(
            "↑/↓ or j/k select/scroll   Enter/→ open   Backspace/← parent   Tab switch pane",
        ),
        Line::from("c copy   m move/archive   e rename   d delete   n new directory   r refresh"),
        Line::from("PgUp/PgDn, Home/End navigate   ?/F1 help   q/Ctrl-C quit"),
        Line::from(
            "Paths may be absolute or relative to the open directory. Enter confirms; Esc cancels.",
        ),
    ];
    frame.render_widget(
        Paragraph::new(text)
            .block(
                Block::default()
                    .title(" Keyboard help ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Blue)),
            )
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let status_color = if app.status_error {
        Color::Red
    } else {
        Color::Green
    };
    let focus = if app.focus == Focus::Files {
        "FILES"
    } else {
        "PREVIEW"
    };
    let first = Line::from(vec![
        Span::styled(
            format!(" {focus} "),
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(app.status.clone(), Style::default().fg(status_color)),
    ]);
    let second = Line::from(
        " ↑↓ navigate  Enter open  Backspace parent  Tab pane  c copy  m move  e rename  d delete  n mkdir  ? help  q quit",
    );
    frame.render_widget(
        Paragraph::new(vec![first, second]).style(Style::default().bg(Color::Black)),
        area,
    );
}

fn read_directory(path: &Path) -> io::Result<Vec<Entry>> {
    let mut entries = fs::read_dir(path)?
        .filter_map(|result| result.ok())
        .filter_map(|entry| Entry::from_dir_entry(entry).ok())
        .collect::<Vec<_>>();
    entries.sort_by(
        |left, right| match (left.file_type.is_dir(), right.file_type.is_dir()) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            _ => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
        },
    );
    Ok(entries)
}

fn resolve_path(current_dir: &Path, value: &str) -> PathBuf {
    let expanded = if value == "~" {
        env::var_os("HOME").map(PathBuf::from)
    } else if let Some(remainder) = value.strip_prefix("~/") {
        env::var_os("HOME").map(|home| PathBuf::from(home).join(remainder))
    } else {
        None
    };
    let path = expanded.unwrap_or_else(|| PathBuf::from(value));
    if path.is_absolute() {
        path
    } else {
        current_dir.join(path)
    }
}

fn final_destination(source: &Path, requested: &Path) -> Result<PathBuf> {
    if requested.is_dir() {
        let name = source
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("source has no file name"))?;
        Ok(requested.join(name))
    } else {
        Ok(requested.to_path_buf())
    }
}

fn copy_file(source: &Path, requested: &Path) -> Result<String> {
    if !source.is_file() {
        anyhow::bail!("copy source must be a regular file");
    }
    let destination = final_destination(source, requested)?;
    if source == destination {
        anyhow::bail!("source and destination are the same");
    }
    if destination.exists() {
        anyhow::bail!("destination already exists: {}", destination.display());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow::anyhow!("destination has no parent directory"))?;
    if !parent.is_dir() {
        anyhow::bail!("destination directory does not exist: {}", parent.display());
    }
    fs::copy(source, &destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(format!(
        "Copied {} → {}",
        source.display(),
        destination.display()
    ))
}

fn move_item(source: &Path, requested: &Path) -> Result<String> {
    let destination = final_destination(source, requested)?;
    if source == destination {
        anyhow::bail!("source and destination are the same");
    }
    if destination.exists() {
        anyhow::bail!("destination already exists: {}", destination.display());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow::anyhow!("destination has no parent directory"))?;
    if !parent.is_dir() {
        anyhow::bail!("destination directory does not exist: {}", parent.display());
    }
    move_across_filesystems(source, &destination)?;
    Ok(format!(
        "Moved {} → {}",
        source.display(),
        destination.display()
    ))
}

fn rename_item(source: &Path, destination: &Path) -> Result<String> {
    if source == destination {
        anyhow::bail!("new path is unchanged");
    }
    if destination.exists() {
        anyhow::bail!("destination already exists: {}", destination.display());
    }
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow::anyhow!("new path has no parent directory"))?;
    if !parent.is_dir() {
        anyhow::bail!("destination directory does not exist: {}", parent.display());
    }
    move_across_filesystems(source, destination)?;
    Ok(format!(
        "Renamed {} → {}",
        source.display(),
        destination.display()
    ))
}

fn move_across_filesystems(source: &Path, destination: &Path) -> Result<()> {
    match fs::rename(source, destination) {
        Ok(()) => Ok(()),
        Err(rename_error) => {
            if source.is_file() {
                fs::copy(source, destination).with_context(|| {
                    format!(
                        "rename failed ({rename_error}); fallback copy to {} failed",
                        destination.display()
                    )
                })?;
                if let Err(remove_error) = fs::remove_file(source) {
                    let _ = fs::remove_file(destination);
                    anyhow::bail!(
                        "copied but could not remove original {}; rollback attempted: {remove_error}",
                        source.display()
                    );
                }
                Ok(())
            } else {
                Err(rename_error).with_context(|| {
                    format!(
                        "failed to move {} to {} (directories cannot cross filesystems)",
                        source.display(),
                        destination.display()
                    )
                })
            }
        }
    }
}

fn create_directory(path: &Path) -> Result<String> {
    if path.exists() {
        anyhow::bail!("path already exists: {}", path.display());
    }
    fs::create_dir_all(path)
        .with_context(|| format!("failed to create directory {}", path.display()))?;
    Ok(format!("Created directory {}", path.display()))
}

fn delete_item(path: &Path) -> Result<String> {
    if path.is_dir() {
        fs::remove_dir_all(path)
            .with_context(|| format!("failed to delete directory {}", path.display()))?;
    } else {
        fs::remove_file(path)
            .with_context(|| format!("failed to delete file {}", path.display()))?;
    }
    Ok(format!("Deleted {}", path.display()))
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8192).any(|byte| *byte == 0)
}

fn expand_tabs(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut column = 0;
    for character in value.chars() {
        if character == '\t' {
            let spaces = PREVIEW_TAB_WIDTH - (column % PREVIEW_TAB_WIDTH);
            output.extend(std::iter::repeat_n(' ', spaces));
            column += spaces;
        } else {
            output.push(character);
            column += 1;
        }
    }
    output
}

fn wrap_preview_lines(lines: &[String], width: usize) -> Vec<String> {
    let width = width.max(1);
    let mut wrapped = Vec::new();
    for line in lines {
        if line.is_empty() {
            wrapped.push(String::new());
            continue;
        }

        let mut chunk = String::new();
        let mut columns = 0;
        for character in line.chars() {
            if columns == width {
                wrapped.push(chunk);
                chunk = String::new();
                columns = 0;
            }
            chunk.push(character);
            columns += 1;
        }
        wrapped.push(chunk);
    }
    if wrapped.is_empty() {
        wrapped.push(String::new());
    }
    wrapped
}

fn previous_char_boundary(value: &str, cursor: usize) -> usize {
    value[..cursor]
        .char_indices()
        .next_back()
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn next_char_boundary(value: &str, cursor: usize) -> usize {
    value[cursor..]
        .char_indices()
        .nth(1)
        .map(|(index, _)| cursor + index)
        .unwrap_or(value.len())
}

fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes}{}", UNITS[unit])
    } else {
        format!("{size:.1}{}", UNITS[unit])
    }
}

#[allow(dead_code)]
fn _system_time_label(time: SystemTime) -> String {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "-".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn copies_file_byte_identically() {
        let root = tempdir().unwrap();
        let source = root.path().join("source.bin");
        let destination = root.path().join("copy.bin");
        let bytes = b"hello\0binary\n";
        fs::write(&source, bytes).unwrap();

        copy_file(&source, &destination).unwrap();

        assert_eq!(fs::read(destination).unwrap(), bytes);
        assert!(source.exists());
    }

    #[test]
    fn moves_file_into_directory() {
        let root = tempdir().unwrap();
        let archive = root.path().join("archive");
        fs::create_dir(&archive).unwrap();
        let source = root.path().join("report.txt");
        fs::write(&source, "report").unwrap();

        move_item(&source, &archive).unwrap();

        assert!(!source.exists());
        assert_eq!(
            fs::read_to_string(archive.join("report.txt")).unwrap(),
            "report"
        );
    }

    #[test]
    fn renames_without_changing_content() {
        let root = tempdir().unwrap();
        let source = root.path().join("old.txt");
        let destination = root.path().join("new.txt");
        fs::write(&source, "same content").unwrap();

        rename_item(&source, &destination).unwrap();

        assert!(!source.exists());
        assert_eq!(fs::read_to_string(destination).unwrap(), "same content");
    }

    #[test]
    fn creates_nested_directory() {
        let root = tempdir().unwrap();
        let destination = root.path().join("one/two");

        create_directory(&destination).unwrap();

        assert!(destination.is_dir());
    }

    #[test]
    fn deletes_files_and_directories() {
        let root = tempdir().unwrap();
        let file = root.path().join("remove.txt");
        let directory = root.path().join("remove-dir");
        fs::write(&file, "x").unwrap();
        fs::create_dir(&directory).unwrap();
        fs::write(directory.join("child"), "x").unwrap();

        delete_item(&file).unwrap();
        delete_item(&directory).unwrap();

        assert!(!file.exists());
        assert!(!directory.exists());
    }

    #[test]
    fn resolves_relative_and_home_paths() {
        let root = Path::new("/tmp/example");
        assert_eq!(
            resolve_path(root, "archive/item"),
            Path::new("/tmp/example/archive/item")
        );
        assert_eq!(
            resolve_path(root, "/var/tmp/item"),
            Path::new("/var/tmp/item")
        );
    }

    #[test]
    fn unicode_input_boundaries_are_safe() {
        let value = "aéEnglish-only text";
        assert_eq!(next_char_boundary(value, 0), 1);
        assert_eq!(next_char_boundary(value, 1), 3);
        assert_eq!(previous_char_boundary(value, value.len()), 3);
        assert_eq!(previous_char_boundary(value, 3), 1);
    }

    #[test]
    fn preview_wrapping_keeps_every_character_scrollable() {
        let lines = vec!["abcdefgh".to_owned(), String::new(), "ijk".to_owned()];
        assert_eq!(
            wrap_preview_lines(&lines, 3),
            vec!["abc", "def", "gh", "", "ijk"]
        );
    }
}
