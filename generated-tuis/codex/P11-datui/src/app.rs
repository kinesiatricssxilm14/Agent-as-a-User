use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use polars::prelude::*;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{
        Block, Borders, Cell, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation,
        ScrollbarState, Table, TableState, Wrap,
    },
};

use crate::data::{self, Analysis};

const ACCENT: Color = Color::Rgb(86, 182, 194);
const GOLD: Color = Color::Rgb(229, 192, 123);
const MUTED: Color = Color::Rgb(128, 139, 153);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryMode {
    Fuzzy,
    SqlLike,
    Sql,
}

impl QueryMode {
    fn label(self) -> &'static str {
        match self {
            Self::Fuzzy => "FUZZY",
            Self::SqlLike => "SQL-LIKE",
            Self::Sql => "SQL",
        }
    }
    fn hint(self) -> &'static str {
        match self {
            Self::Fuzzy => "type a keyword (substring or subsequence match across every column)",
            Self::SqlLike => "select where age > 40 and department = 'Sales'",
            Self::Sql => "select * from df where country = 'US' and score > 85",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum InputPurpose {
    Query,
    Sort,
    Analysis,
    Export,
    Open,
}

pub struct App {
    source: DataFrame,
    result: DataFrame,
    path: PathBuf,
    mode: QueryMode,
    query: String,
    input: String,
    input_purpose: Option<InputPurpose>,
    selected: usize,
    row_offset: usize,
    col_offset: usize,
    detail_scroll: u16,
    status: String,
    error: bool,
    help: bool,
    analysis: Option<Analysis>,
    sort_descending: bool,
    should_quit: bool,
}

impl App {
    pub fn new(path: PathBuf) -> Result<Self> {
        let source = data::load_csv(&path)?;
        let status = format!(
            "Loaded {} rows × {} columns from {}",
            source.height(),
            source.width(),
            path.display()
        );
        Ok(Self {
            result: source.clone(),
            source,
            path,
            mode: QueryMode::Fuzzy,
            query: String::new(),
            input: String::new(),
            input_purpose: None,
            selected: 0,
            row_offset: 0,
            col_offset: 0,
            detail_scroll: 0,
            status,
            error: false,
            help: false,
            analysis: None,
            sort_descending: false,
            should_quit: false,
        })
    }

    pub fn run(&mut self, terminal: &mut ratatui::DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| self.draw(frame))?;
            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key);
                }
            }
        }
        Ok(())
    }

    fn handle_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return;
        }
        if self.help {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
            ) {
                self.help = false;
            }
            return;
        }
        if self.input_purpose.is_some() {
            self.handle_input(key);
            return;
        }
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.help = true,
            KeyCode::Char('1') => self.set_mode(QueryMode::Fuzzy),
            KeyCode::Char('2') => self.set_mode(QueryMode::SqlLike),
            KeyCode::Char('3') => self.set_mode(QueryMode::Sql),
            KeyCode::Char('/') | KeyCode::Char('i') => self.begin_input(InputPurpose::Query),
            KeyCode::Char('s') => self.begin_input(InputPurpose::Sort),
            KeyCode::Char('a') => self.begin_input(InputPurpose::Analysis),
            KeyCode::Char('e') => self.begin_input(InputPurpose::Export),
            KeyCode::Char('o') => self.begin_input(InputPurpose::Open),
            KeyCode::Char('r') => self.reset(),
            KeyCode::Char('d') => {
                self.sort_descending = !self.sort_descending;
                self.set_status(
                    format!(
                        "Sort direction: {}",
                        if self.sort_descending {
                            "descending"
                        } else {
                            "ascending"
                        }
                    ),
                    false,
                );
            }
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::PageDown => self.move_selection(10),
            KeyCode::PageUp => self.move_selection(-10),
            KeyCode::Home => {
                self.selected = 0;
                self.row_offset = 0;
            }
            KeyCode::End => {
                self.selected = self.result.height().saturating_sub(1);
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.col_offset = (self.col_offset + 1).min(self.result.width().saturating_sub(1))
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.col_offset = self.col_offset.saturating_sub(1)
            }
            KeyCode::Char(']') => self.detail_scroll = self.detail_scroll.saturating_add(1),
            KeyCode::Char('[') => self.detail_scroll = self.detail_scroll.saturating_sub(1),
            _ => {}
        }
    }

    fn handle_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.input_purpose = None;
                self.input.clear();
                self.set_status("Input cancelled".into(), false);
            }
            KeyCode::Enter => self.submit_input(),
            KeyCode::Backspace => {
                self.input.pop();
                if self.input_purpose == Some(InputPurpose::Query) && self.mode == QueryMode::Fuzzy
                {
                    self.run_query();
                }
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.push(c);
                if self.input_purpose == Some(InputPurpose::Query) && self.mode == QueryMode::Fuzzy
                {
                    self.run_query();
                }
            }
            _ => {}
        }
    }

    fn begin_input(&mut self, purpose: InputPurpose) {
        self.input = match purpose {
            InputPurpose::Query => self.query.clone(),
            _ => String::new(),
        };
        self.input_purpose = Some(purpose);
    }

    fn submit_input(&mut self) {
        let purpose = self.input_purpose.take().unwrap();
        match purpose {
            InputPurpose::Query => self.run_query(),
            InputPurpose::Sort => {
                let columns = split_columns(&self.input);
                match data::sort_sql(&self.result, &columns, self.sort_descending) {
                    Ok(df) => {
                        self.result = df;
                        self.after_result(format!(
                            "Sorted by {} ({})",
                            columns.join(", "),
                            if self.sort_descending {
                                "descending"
                            } else {
                                "ascending"
                            }
                        ));
                    }
                    Err(e) => self.set_status(e.to_string(), true),
                }
            }
            InputPurpose::Analysis => {
                let columns = split_columns(&self.input);
                match data::analyze(&self.result, &columns) {
                    Ok(analysis) => {
                        self.analysis = Some(analysis);
                        self.set_status(
                            format!("Analysis ready for {}", columns.join(", ")),
                            false,
                        );
                    }
                    Err(e) => self.set_status(e.to_string(), true),
                }
            }
            InputPurpose::Export => {
                let requested = if self.input.trim().is_empty() {
                    "results.csv"
                } else {
                    self.input.trim()
                };
                let path = expand_path(requested);
                match data::export_csv(&self.result, &path) {
                    Ok(()) => self.set_status(
                        format!(
                            "Exported {} rows to {}",
                            self.result.height(),
                            path.display()
                        ),
                        false,
                    ),
                    Err(e) => self.set_status(e.to_string(), true),
                }
            }
            InputPurpose::Open => {
                let path = expand_path(self.input.trim());
                match data::load_csv(&path) {
                    Ok(df) => {
                        self.path = path;
                        self.source = df.clone();
                        self.result = df;
                        self.query.clear();
                        self.analysis = None;
                        self.after_result(format!("Loaded {}", self.path.display()));
                    }
                    Err(e) => self.set_status(e.to_string(), true),
                }
            }
        }
        self.input.clear();
    }

    fn run_query(&mut self) {
        self.query = self.input.clone();
        let outcome = match self.mode {
            QueryMode::Fuzzy => data::fuzzy(&self.source, &self.query),
            QueryMode::SqlLike => data::execute_sql_like(&self.source, &self.query),
            QueryMode::Sql => data::execute_sql(&self.source, &self.query),
        };
        match outcome {
            Ok(df) => {
                self.result = df;
                self.after_result(format!(
                    "{} match(es) in {} mode",
                    self.result.height(),
                    self.mode.label()
                ));
            }
            Err(e) => self.set_status(e.to_string(), true),
        }
    }

    fn reset(&mut self) {
        self.result = self.source.clone();
        self.query.clear();
        self.analysis = None;
        self.after_result("Reset to complete CSV".into());
    }
    fn set_mode(&mut self, mode: QueryMode) {
        self.mode = mode;
        self.query.clear();
        self.input.clear();
        self.set_status(format!("{} mode — press / to query", mode.label()), false);
    }
    fn after_result(&mut self, message: String) {
        self.selected = 0;
        self.row_offset = 0;
        self.col_offset = 0;
        self.detail_scroll = 0;
        self.analysis = None;
        self.set_status(message, false);
    }
    fn set_status(&mut self, message: String, error: bool) {
        self.status = message;
        self.error = error;
    }
    fn move_selection(&mut self, delta: isize) {
        if self.result.height() == 0 {
            return;
        }
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.result.height() - 1);
    }

    fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(9),
            Constraint::Length(2),
        ])
        .split(area);
        self.draw_header(frame, chunks[0]);
        self.draw_query(frame, chunks[1]);
        self.draw_content(frame, chunks[2]);
        self.draw_footer(frame, chunks[3]);
        if self.help {
            self.draw_help(frame, area);
        }
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let title = Line::from(vec![
            Span::styled(
                " toolk ",
                Style::default().fg(Color::Black).bg(ACCENT).bold(),
            ),
            Span::raw("  CSV DATA WORKBENCH"),
        ]);
        let right = format!(
            "{} rows × {} cols  │  {}",
            self.result.height(),
            self.result.width(),
            self.path.display()
        );
        let block = Block::default()
            .borders(Borders::BOTTOM)
            .border_style(Style::default().fg(MUTED));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        frame.render_widget(Paragraph::new(title), inner);
        frame.render_widget(
            Paragraph::new(right)
                .alignment(Alignment::Right)
                .style(Style::default().fg(MUTED)),
            inner,
        );
    }

    fn draw_query(&self, frame: &mut Frame, area: Rect) {
        let editing = self.input_purpose.is_some();
        let value = if editing { &self.input } else { &self.query };
        let (title, hint) = match self.input_purpose.as_ref() {
            Some(InputPurpose::Sort) => (
                format!(
                    " SORT {} • ENTER APPLY • ESC CANCEL ",
                    if self.sort_descending {
                        "DESCENDING"
                    } else {
                        "ASCENDING"
                    }
                ),
                "comma-separated columns, e.g. department, salary",
            ),
            Some(InputPurpose::Analysis) => (
                " ANALYZE • ENTER APPLY • ESC CANCEL ".into(),
                "one or two comma-separated columns, e.g. salary, score",
            ),
            Some(InputPurpose::Export) => (
                " EXPORT CSV • ENTER WRITE • ESC CANCEL ".into(),
                "output path (blank uses results.csv)",
            ),
            Some(InputPurpose::Open) => (
                " OPEN CSV • ENTER LOAD • ESC CANCEL ".into(),
                "path to a CSV file",
            ),
            _ => (
                format!(
                    " {} QUERY {} ",
                    self.mode.label(),
                    if editing {
                        "• EDITING"
                    } else {
                        "• / TO EDIT"
                    }
                ),
                self.mode.hint(),
            ),
        };
        let shown = if value.is_empty() { hint } else { value };
        let style = if value.is_empty() {
            Style::default().fg(MUTED)
        } else {
            Style::default().fg(Color::White)
        };
        frame.render_widget(
            Paragraph::new(shown).style(style).block(
                Block::bordered()
                    .title(title)
                    .border_style(Style::default().fg(if editing { GOLD } else { ACCENT })),
            ),
            area,
        );
        if editing {
            let x = area
                .x
                .saturating_add(1 + value.chars().count() as u16)
                .min(area.right().saturating_sub(2));
            frame.set_cursor_position((x, area.y + 1));
        }
    }

    fn draw_content(&mut self, frame: &mut Frame, area: Rect) {
        let lower_height = if self.analysis.is_some() {
            11
        } else {
            (area.height / 3).clamp(5, 9)
        };
        let chunks =
            Layout::vertical([Constraint::Min(5), Constraint::Length(lower_height)]).split(area);
        self.draw_table(frame, chunks[0]);
        if let Some(analysis) = &self.analysis {
            self.draw_analysis_and_detail(frame, chunks[1], analysis);
        } else {
            self.draw_detail(frame, chunks[1]);
        }
    }

    fn draw_table(&mut self, frame: &mut Frame, area: Rect) {
        let visible_rows = area.height.saturating_sub(3) as usize;
        if self.selected < self.row_offset {
            self.row_offset = self.selected;
        }
        if self.selected >= self.row_offset + visible_rows.max(1) {
            self.row_offset = self.selected + 1 - visible_rows.max(1);
        }
        let names = self.result.get_column_names();
        let available_cols = ((area.width.saturating_sub(4)) / 18).max(1) as usize;
        let end = (self.col_offset + available_cols).min(names.len());
        let shown_names = &names[self.col_offset..end];
        let header = Row::new(
            shown_names
                .iter()
                .map(|n| Cell::from(n.as_str()).style(Style::default().fg(GOLD).bold())),
        )
        .height(1)
        .bottom_margin(1);
        let rows = (self.row_offset..self.result.height().min(self.row_offset + visible_rows)).map(
            |idx| {
                let cells = (self.col_offset..end).map(|col_idx| {
                    self.result
                        .select_at_idx(col_idx)
                        .and_then(|c| c.get(idx).ok())
                        .map(|v| data::display_value(&v))
                        .unwrap_or_else(|| "?".into())
                });
                Row::new(cells)
            },
        );
        let widths = vec![Constraint::Length(17); shown_names.len()];
        let table = Table::new(rows, widths)
            .header(header)
            .column_spacing(1)
            .row_highlight_style(
                Style::default()
                    .bg(Color::Rgb(40, 54, 65))
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ")
            .block(
                Block::bordered()
                    .title(format!(
                        " RESULTS • {} matches • columns {}–{} of {} ",
                        self.result.height(),
                        if names.is_empty() {
                            0
                        } else {
                            self.col_offset + 1
                        },
                        end,
                        names.len()
                    ))
                    .border_style(Style::default().fg(ACCENT)),
            );
        let mut state = TableState::default().with_selected(
            (self.result.height() > 0).then_some(self.selected.saturating_sub(self.row_offset)),
        );
        frame.render_stateful_widget(table, area, &mut state);
        if self.result.height() > visible_rows {
            let mut scroll = ScrollbarState::new(self.result.height()).position(self.selected);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                area,
                &mut scroll,
            );
        }
    }

    fn detail_text(&self) -> Text<'static> {
        if self.result.height() == 0 {
            return Text::from(Line::styled("No matching rows", Style::default().fg(MUTED)));
        }
        let names = self.result.get_column_names();
        let row = match self.result.get_row(self.selected) {
            Ok(row) => row,
            Err(_) => return Text::from("Unable to read selected row"),
        };
        let mut lines = Vec::new();
        for (name, value) in names.iter().zip(row.0.iter()) {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{}: ", name.as_str()),
                    Style::default().fg(GOLD).bold(),
                ),
                Span::raw(data::display_value(value)),
            ]));
        }
        Text::from(lines)
    }

    fn draw_detail(&self, frame: &mut Frame, area: Rect) {
        frame.render_widget(
            Paragraph::new(self.detail_text())
                .scroll((self.detail_scroll, 0))
                .wrap(Wrap { trim: false })
                .block(
                    Block::bordered()
                        .title(format!(
                            " SELECTED ROW {} OF {} • all fields • [/] scroll ",
                            if self.result.height() == 0 {
                                0
                            } else {
                                self.selected + 1
                            },
                            self.result.height()
                        ))
                        .border_style(Style::default().fg(Color::Blue)),
                ),
            area,
        );
    }

    fn draw_analysis_and_detail(&self, frame: &mut Frame, area: Rect, analysis: &Analysis) {
        let parts = Layout::horizontal([Constraint::Percentage(58), Constraint::Percentage(42)])
            .split(area);
        let mut lines = Vec::new();
        for stat in &analysis.stats {
            lines.push(Line::styled(
                format!("{} ({})", stat.name, stat.dtype),
                Style::default().fg(GOLD).bold(),
            ));
            lines.push(Line::raw(format!(
                "count {}  null {}  unique {}",
                stat.count, stat.nulls, stat.unique
            )));
            lines.push(Line::raw(format!(
                "min {}  max {}  mean {}  std {}",
                stat.min,
                stat.max,
                stat.mean.map(data::fixed2).unwrap_or_else(|| "—".into()),
                stat.std_dev.map(data::fixed2).unwrap_or_else(|| "—".into())
            )));
        }
        if let Some((a, b, r)) = &analysis.correlation {
            lines.push(Line::styled(
                format!("Pearson r ({a}, {b}) = {}", data::fixed2(*r)),
                Style::default().fg(ACCENT).bold(),
            ));
        }
        if !analysis.histogram.is_empty() {
            lines.push(Line::styled(
                "Distribution (bin=count)",
                Style::default().fg(GOLD).bold(),
            ));
            let max = analysis
                .histogram
                .iter()
                .map(|b| b.count)
                .max()
                .unwrap_or(1);
            let bins = analysis
                .histogram
                .iter()
                .map(|bin| format!("{}={}", bin.label, bin.count))
                .collect::<Vec<_>>()
                .join("  ");
            let bars = analysis
                .histogram
                .iter()
                .map(|bin| {
                    let width = ((bin.count * 6) / max.max(1)).max((bin.count > 0) as usize);
                    format!("{}", "█".repeat(width))
                })
                .collect::<Vec<_>>()
                .join(" ");
            lines.push(Line::raw(bins));
            lines.push(Line::styled(bars, Style::default().fg(Color::Green)));
        }
        frame.render_widget(
            Paragraph::new(lines).wrap(Wrap { trim: false }).block(
                Block::bordered()
                    .title(" ANALYSIS • fixed 2-decimal metrics ")
                    .border_style(Style::default().fg(Color::Green)),
            ),
            parts[0],
        );
        self.draw_detail(frame, parts[1]);
    }

    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        let status_style = if self.error {
            Style::default().fg(Color::Red).bold()
        } else {
            Style::default().fg(ACCENT)
        };
        let shortcuts = "1 Fuzzy  2 SQL-Like  3 SQL  / Query  ↑↓ Rows  ←→ Cols  s Sort  d Dir  a Analyze  e Export  o Open  r Reset  ? Help  q Quit";
        let lines = vec![
            Line::styled(&self.status, status_style),
            Line::styled(shortcuts, Style::default().fg(MUTED)),
        ];
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn draw_help(&self, frame: &mut Frame, area: Rect) {
        let width = area.width.saturating_sub(6).min(90);
        let height = area.height.saturating_sub(4).min(30);
        let popup = centered(area, width, height);
        frame.render_widget(Clear, popup);
        let text = vec![
            Line::styled("TOOLK KEYBOARD GUIDE", Style::default().fg(ACCENT).bold()),
            Line::raw(""),
            Line::styled("Query modes", Style::default().fg(GOLD).bold()),
            Line::raw(
                "1 Fuzzy: case-insensitive substring/subsequence across all cells; updates while typing",
            ),
            Line::raw("2 SQL-Like: select where <condition>   (and/or, =, !=, >, <, >=, <=)"),
            Line::raw("3 SQL: standard Polars SQL; the loaded table is named df"),
            Line::raw("/ or i edit query • Enter execute • Esc cancel input"),
            Line::raw(""),
            Line::styled("Navigation", Style::default().fg(GOLD).bold()),
            Line::raw("↑/k ↓/j select row • PgUp/PgDn jump • Home/End first/last"),
            Line::raw("←/h →/l reveal all dynamic columns • [/] scroll selected-row fields"),
            Line::raw(""),
            Line::styled("Data operations", Style::default().fg(GOLD).bold()),
            Line::raw("s sort: comma-separated columns • d toggle ascending/descending"),
            Line::raw("a analyze: one/two columns (statistics, distribution, Pearson correlation)"),
            Line::raw("e export current result as a real CSV • o load another CSV • r reset"),
            Line::raw(""),
            Line::styled("Display", Style::default().fg(GOLD).bold()),
            Line::raw(
                "Selected-row pane exposes every field. Floating values and analysis metrics keep two decimals.",
            ),
            Line::raw("? close help • q quit • Ctrl-C emergency quit"),
        ];
        frame.render_widget(
            Paragraph::new(text).wrap(Wrap { trim: false }).block(
                Block::bordered()
                    .title(" HELP ")
                    .border_style(Style::default().fg(ACCENT)),
            ),
            popup,
        );
    }
}

fn split_columns(input: &str) -> Vec<String> {
    input
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
fn expand_path(input: &str) -> PathBuf {
    if let Some(rest) = input.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return Path::new(&home).join(rest);
        }
    }
    PathBuf::from(input)
}
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((area.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Min(0),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length((area.width.saturating_sub(width)) / 2),
            Constraint::Length(width),
            Constraint::Min(0),
        ])
        .split(vertical[1])[1]
}

pub fn missing_file_message(path: &Path) -> anyhow::Error {
    anyhow!(
        "CSV file not found: {}\nUsage: toolk [CSV_PATH]\nDefault: /bench/data/employees.csv",
        path.display()
    )
}
