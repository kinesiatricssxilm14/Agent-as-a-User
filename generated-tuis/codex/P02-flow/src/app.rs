use crate::model::Board;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation,
    ScrollbarState, Wrap,
};
use ratatui::Frame;
use std::cmp::min;
use unicode_width::UnicodeWidthStr;

const MIN_COLUMN_WIDTH: u16 = 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Continue,
    Quit,
}

#[derive(Debug, Clone)]
enum Mode {
    Browse,
    Input(InputFlow),
    ConfirmDelete,
    Move { target: usize },
}

#[derive(Debug, Clone)]
enum InputFlow {
    NewCardId { value: String },
    NewCardTitle { id: String, value: String },
    NewColumnId { value: String },
    NewColumnName { id: String, value: String },
    SelectColumn { value: String },
    EditTitle { value: String },
    AppendBody { value: String },
}

pub struct App {
    pub board: Board,
    selected_column: usize,
    selected_cards: Vec<usize>,
    first_visible_column: usize,
    detail_scroll: u16,
    mode: Mode,
    status: String,
    status_error: bool,
    show_help: bool,
}

impl App {
    pub fn new(board: Board) -> Self {
        let selected_cards = vec![0; board.columns.len()];
        Self {
            board,
            selected_column: 0,
            selected_cards,
            first_visible_column: 0,
            detail_scroll: 0,
            mode: Mode::Browse,
            status: "Ready — changes are written immediately to disk".to_owned(),
            status_error: false,
            show_help: false,
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Action {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return Action::Quit;
        }
        match self.mode.clone() {
            Mode::Browse => self.handle_browse(key),
            Mode::Input(flow) => self.handle_input(key, flow),
            Mode::ConfirmDelete => self.handle_delete_confirmation(key),
            Mode::Move { target } => self.handle_move(key, target),
        }
    }

    fn handle_browse(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('q') => return Action::Quit,
            KeyCode::Left | KeyCode::Char('h') if !self.show_help => self.select_previous_column(),
            KeyCode::Right | KeyCode::Char('l') if !self.show_help => self.select_next_column(),
            KeyCode::Up | KeyCode::Char('k') => self.select_previous_card(),
            KeyCode::Down | KeyCode::Char('j') => self.select_next_card(),
            KeyCode::Home => self.select_first_card(),
            KeyCode::End => self.select_last_card(),
            KeyCode::PageUp | KeyCode::Char('[') => {
                self.detail_scroll = self.detail_scroll.saturating_sub(5)
            }
            KeyCode::PageDown | KeyCode::Char(']') => {
                self.detail_scroll = self.detail_scroll.saturating_add(5)
            }
            KeyCode::Char('c') | KeyCode::Char('n') => self.start_new_card(),
            KeyCode::Char('C') | KeyCode::Char('N') => {
                self.mode = Mode::Input(InputFlow::NewColumnId {
                    value: String::new(),
                })
            }
            KeyCode::Char('/') | KeyCode::Char('g') if !self.show_help => {
                self.mode = Mode::Input(InputFlow::SelectColumn {
                    value: String::new(),
                })
            }
            KeyCode::Char('e') => self.start_edit(),
            KeyCode::Char('a') => self.start_append(),
            KeyCode::Char('d') | KeyCode::Delete => self.start_delete(),
            KeyCode::Char('m') => self.start_move(),
            KeyCode::Char('r') => self.reload(),
            KeyCode::Char('?') | KeyCode::F(1) => self.show_help = !self.show_help,
            KeyCode::Esc if self.show_help => self.show_help = false,
            _ => {}
        }
        Action::Continue
    }

    fn handle_input(&mut self, key: KeyEvent, mut flow: InputFlow) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Browse;
                self.set_status("Cancelled", false);
            }
            KeyCode::Enter => self.submit_input(flow),
            KeyCode::Backspace => {
                flow.value_mut().pop();
                self.mode = Mode::Input(flow);
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                flow.value_mut().push(ch);
                self.mode = Mode::Input(flow);
            }
            _ => self.mode = Mode::Input(flow),
        }
        Action::Continue
    }

    fn submit_input(&mut self, flow: InputFlow) {
        match flow {
            InputFlow::NewCardId { value } => {
                if value.trim().is_empty() {
                    self.set_status("Card id cannot be empty", true);
                    self.mode = Mode::Input(InputFlow::NewCardId { value });
                } else {
                    self.mode = Mode::Input(InputFlow::NewCardTitle {
                        id: value,
                        value: String::new(),
                    });
                }
            }
            InputFlow::NewCardTitle { id, value } => {
                let column = self.selected_column;
                match self.board.create_card(column, &id, &value) {
                    Ok(()) => {
                        self.sync_selection();
                        self.selected_cards[column] = self.board.columns[column].cards.len() - 1;
                        self.detail_scroll = 0;
                        self.mode = Mode::Browse;
                        self.set_status(format!("Created card '{id}'"), false);
                    }
                    Err(error) => {
                        self.set_status(error.to_string(), true);
                        self.mode = Mode::Input(InputFlow::NewCardTitle { id, value });
                    }
                }
            }
            InputFlow::NewColumnId { value } => {
                if value.trim().is_empty() {
                    self.set_status("Column id cannot be empty", true);
                    self.mode = Mode::Input(InputFlow::NewColumnId { value });
                } else {
                    self.mode = Mode::Input(InputFlow::NewColumnName {
                        id: value,
                        value: String::new(),
                    });
                }
            }
            InputFlow::NewColumnName { id, value } => match self.board.create_column(&id, &value) {
                Ok(()) => {
                    self.sync_selection();
                    self.selected_column = self.board.columns.len() - 1;
                    self.detail_scroll = 0;
                    self.mode = Mode::Browse;
                    self.set_status(format!("Created column '{value}'"), false);
                }
                Err(error) => {
                    self.set_status(error.to_string(), true);
                    self.mode = Mode::Input(InputFlow::NewColumnName { id, value });
                }
            },
            InputFlow::SelectColumn { value } => {
                let query = value.trim();
                let lowered = query.to_lowercase();
                let exact = self
                    .board
                    .columns
                    .iter()
                    .position(|column| column.name.eq_ignore_ascii_case(query));
                let partial = self
                    .board
                    .columns
                    .iter()
                    .position(|column| column.name.to_lowercase().contains(&lowered));
                if let Some(index) = exact.or(partial).filter(|_| !query.is_empty()) {
                    self.selected_column = index;
                    self.detail_scroll = 0;
                    self.mode = Mode::Browse;
                    self.set_status(
                        format!("Selected column '{}'", self.board.columns[index].name),
                        false,
                    );
                } else {
                    self.set_status(format!("No column display name matches '{query}'"), true);
                    self.mode = Mode::Input(InputFlow::SelectColumn { value });
                }
            }
            InputFlow::EditTitle { value } => {
                let column = self.selected_column;
                let Some(card) = self.selected_card_index() else {
                    self.set_status("No card selected", true);
                    self.mode = Mode::Browse;
                    return;
                };
                match self.board.edit_title(column, card, &value) {
                    Ok(()) => {
                        self.mode = Mode::Browse;
                        self.detail_scroll = 0;
                        self.set_status("Title updated", false);
                    }
                    Err(error) => {
                        self.set_status(error.to_string(), true);
                        self.mode = Mode::Input(InputFlow::EditTitle { value });
                    }
                }
            }
            InputFlow::AppendBody { value } => {
                if let Some(card) = self.selected_card_index() {
                    match self
                        .board
                        .append_body_line(self.selected_column, card, &value)
                    {
                        Ok(()) => {
                            self.mode = Mode::Browse;
                            self.set_status("Body line appended", false);
                        }
                        Err(error) => {
                            self.set_status(error.to_string(), true);
                            self.mode = Mode::Input(InputFlow::AppendBody { value });
                        }
                    }
                } else {
                    self.set_status("No card selected", true);
                    self.mode = Mode::Browse;
                }
            }
        }
    }

    fn handle_delete_confirmation(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                if let Some(card) = self.selected_card_index() {
                    match self.board.delete_card(self.selected_column, card) {
                        Ok(()) => {
                            self.sync_selection();
                            self.detail_scroll = 0;
                            self.set_status("Card deleted", false);
                        }
                        Err(error) => self.set_status(error.to_string(), true),
                    }
                }
                self.mode = Mode::Browse;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Browse;
                self.set_status("Delete cancelled", false);
            }
            _ => {}
        }
        Action::Continue
    }

    fn handle_move(&mut self, key: KeyEvent, mut target: usize) -> Action {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Browse;
                self.set_status("Move cancelled", false);
            }
            KeyCode::Left | KeyCode::Char('h') => target = target.saturating_sub(1),
            KeyCode::Right | KeyCode::Char('l') => {
                target = min(target + 1, self.board.columns.len().saturating_sub(1))
            }
            KeyCode::Home => target = 0,
            KeyCode::End => target = self.board.columns.len().saturating_sub(1),
            KeyCode::Enter => {
                if let Some(card) = self.selected_card_index() {
                    let from = self.selected_column;
                    match self.board.move_card(from, card, target) {
                        Ok(()) => {
                            self.sync_selection();
                            self.selected_column = target;
                            self.selected_cards[target] =
                                self.board.columns[target].cards.len() - 1;
                            self.detail_scroll = 0;
                            self.set_status("Card moved", false);
                        }
                        Err(error) => self.set_status(error.to_string(), true),
                    }
                }
                self.mode = Mode::Browse;
            }
            _ => {}
        }
        if matches!(self.mode, Mode::Move { .. }) {
            self.mode = Mode::Move { target };
        }
        Action::Continue
    }

    fn start_new_card(&mut self) {
        if self.board.columns.is_empty() {
            self.set_status("Create a column first with C", true);
        } else {
            self.mode = Mode::Input(InputFlow::NewCardId {
                value: String::new(),
            });
        }
    }

    fn start_edit(&mut self) {
        if let Some(card) = self.selected_card() {
            self.mode = Mode::Input(InputFlow::EditTitle {
                value: card.title.clone(),
            });
        } else {
            self.set_status("No card selected", true);
        }
    }

    fn start_append(&mut self) {
        if self.selected_card().is_some() {
            self.mode = Mode::Input(InputFlow::AppendBody {
                value: String::new(),
            });
        } else {
            self.set_status("No card selected", true);
        }
    }

    fn start_delete(&mut self) {
        if self.selected_card().is_some() {
            self.mode = Mode::ConfirmDelete;
        } else {
            self.set_status("No card selected", true);
        }
    }

    fn start_move(&mut self) {
        if self.selected_card().is_none() {
            self.set_status("No card selected", true);
        } else if self.board.columns.len() < 2 {
            self.set_status("Create another column before moving a card", true);
        } else {
            self.mode = Mode::Move {
                target: self.selected_column,
            };
        }
    }

    fn reload(&mut self) {
        match self.board.reload() {
            Ok(()) => {
                self.sync_selection();
                self.detail_scroll = 0;
                self.set_status("Reloaded board from disk", false);
            }
            Err(error) => self.set_status(error.to_string(), true),
        }
    }

    fn select_previous_column(&mut self) {
        self.selected_column = self.selected_column.saturating_sub(1);
        self.detail_scroll = 0;
    }

    fn select_next_column(&mut self) {
        if self.selected_column + 1 < self.board.columns.len() {
            self.selected_column += 1;
            self.detail_scroll = 0;
        }
    }

    fn select_previous_card(&mut self) {
        if let Some(selected) = self.selected_cards.get_mut(self.selected_column) {
            *selected = selected.saturating_sub(1);
            self.detail_scroll = 0;
        }
    }

    fn select_next_card(&mut self) {
        if let Some(column) = self.board.columns.get(self.selected_column) {
            if let Some(selected) = self.selected_cards.get_mut(self.selected_column) {
                *selected = min(*selected + 1, column.cards.len().saturating_sub(1));
                self.detail_scroll = 0;
            }
        }
    }

    fn select_first_card(&mut self) {
        if let Some(selected) = self.selected_cards.get_mut(self.selected_column) {
            *selected = 0;
            self.detail_scroll = 0;
        }
    }

    fn select_last_card(&mut self) {
        if let Some(column) = self.board.columns.get(self.selected_column) {
            if let Some(selected) = self.selected_cards.get_mut(self.selected_column) {
                *selected = column.cards.len().saturating_sub(1);
                self.detail_scroll = 0;
            }
        }
    }

    fn selected_card_index(&self) -> Option<usize> {
        let column = self.board.columns.get(self.selected_column)?;
        if column.cards.is_empty() {
            None
        } else {
            Some(min(
                *self.selected_cards.get(self.selected_column).unwrap_or(&0),
                column.cards.len() - 1,
            ))
        }
    }

    fn selected_card(&self) -> Option<&crate::model::Card> {
        let index = self.selected_card_index()?;
        self.board
            .columns
            .get(self.selected_column)?
            .cards
            .get(index)
    }

    fn sync_selection(&mut self) {
        self.selected_cards.resize(self.board.columns.len(), 0);
        if self.board.columns.is_empty() {
            self.selected_column = 0;
            self.first_visible_column = 0;
            return;
        }
        self.selected_column = min(self.selected_column, self.board.columns.len() - 1);
        for (index, column) in self.board.columns.iter().enumerate() {
            self.selected_cards[index] = min(
                self.selected_cards[index],
                column.cards.len().saturating_sub(1),
            );
        }
    }

    fn set_status(&mut self, message: impl Into<String>, error: bool) {
        self.status = message.into();
        self.status_error = error;
    }

    pub fn draw(&mut self, frame: &mut Frame) {
        let area = frame.area();
        if area.width < 40 || area.height < 15 {
            frame.render_widget(
                Paragraph::new("toolb needs a terminal at least 40×15\nResize the terminal, or press q to quit.")
                    .alignment(Alignment::Center)
                    .block(Block::default().borders(Borders::ALL).title(" toolb ")),
                area,
            );
            return;
        }

        let input_height = if matches!(self.mode, Mode::Browse) {
            0
        } else {
            4
        };
        let help_height = if self.show_help { 7 } else { 0 };
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(7),
                Constraint::Length(max_detail_height(area.height, input_height, help_height)),
                Constraint::Length(help_height),
                Constraint::Length(input_height),
                Constraint::Length(2),
            ])
            .split(area);

        self.draw_header(frame, chunks[0]);
        self.draw_board(frame, chunks[1]);
        self.draw_details(frame, chunks[2]);
        if self.show_help {
            self.draw_help(frame, chunks[3]);
        }
        if !matches!(self.mode, Mode::Browse) {
            self.draw_mode_panel(frame, chunks[4]);
        }
        self.draw_footer(frame, chunks[5]);
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let title = Line::from(vec![
            Span::styled(
                " toolb ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Kanban Board  "),
            Span::styled(
                self.board.root.display().to_string(),
                Style::default().fg(Color::DarkGray),
            ),
        ]);
        frame.render_widget(
            Paragraph::new(title).block(Block::default().borders(Borders::BOTTOM)),
            area,
        );
    }

    fn draw_board(&mut self, frame: &mut Frame, area: Rect) {
        if self.board.columns.is_empty() {
            frame.render_widget(
                Paragraph::new("No columns yet. Press C to create the first column.")
                    .alignment(Alignment::Center)
                    .block(Block::default().borders(Borders::ALL).title(" Board ")),
                area,
            );
            return;
        }
        let usable = area.width.saturating_sub(2).max(1);
        let visible_count =
            ((usable / MIN_COLUMN_WIDTH).max(1) as usize).min(self.board.columns.len());
        if self.selected_column < self.first_visible_column {
            self.first_visible_column = self.selected_column;
        }
        if self.selected_column >= self.first_visible_column + visible_count {
            self.first_visible_column = self.selected_column + 1 - visible_count;
        }
        let max_start = self.board.columns.len().saturating_sub(visible_count);
        self.first_visible_column = min(self.first_visible_column, max_start);

        let board_block = Block::default().borders(Borders::ALL).title(format!(
            " Board — columns {}–{} of {} ",
            self.first_visible_column + 1,
            self.first_visible_column + visible_count,
            self.board.columns.len()
        ));
        let inner = board_block.inner(area);
        frame.render_widget(board_block, area);
        let constraints = vec![Constraint::Ratio(1, visible_count as u32); visible_count];
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(constraints)
            .split(inner);

        let move_target = match self.mode {
            Mode::Move { target } => Some(target),
            _ => None,
        };
        for (slot, column_index) in
            (self.first_visible_column..self.first_visible_column + visible_count).enumerate()
        {
            let column = &self.board.columns[column_index];
            let selected = column_index == self.selected_column;
            let target = move_target == Some(column_index);
            let border_style = if target {
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let marker = if target {
                " ⇥"
            } else if selected {
                " ●"
            } else {
                ""
            };
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(format!(
                    " {} ({}){} ",
                    column.name,
                    column.cards.len(),
                    marker
                ));
            let items: Vec<ListItem> = if column.cards.is_empty() {
                vec![ListItem::new(Span::styled(
                    "(empty)",
                    Style::default().fg(Color::DarkGray),
                ))]
            } else {
                column
                    .cards
                    .iter()
                    .map(|card| {
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                format!("{} ", card.id),
                                Style::default().fg(Color::Yellow),
                            ),
                            Span::raw(card.title.clone()),
                        ]))
                    })
                    .collect()
            };
            let list = List::new(items)
                .block(block)
                .highlight_style(
                    Style::default()
                        .bg(Color::Blue)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("› ");
            let mut state = ListState::default();
            if selected && !column.cards.is_empty() {
                state.select(Some(self.selected_cards[column_index]));
            }
            frame.render_stateful_widget(list, columns[slot], &mut state);
        }
    }

    fn draw_details(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Selected card — full details ");
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let Some(card) = self.selected_card() else {
            frame.render_widget(
                Paragraph::new("No card selected. Press c to create one in the selected column.")
                    .style(Style::default().fg(Color::DarkGray)),
                inner,
            );
            return;
        };
        let column = &self.board.columns[self.selected_column];
        let text = Text::from(vec![
            Line::from(vec![
                Span::styled("Column: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&column.name, Style::default().fg(Color::Cyan)),
                Span::raw("    "),
                Span::styled("ID: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&card.id, Style::default().fg(Color::Yellow)),
            ]),
            Line::from(vec![
                Span::styled("Title: ", Style::default().fg(Color::DarkGray)),
                Span::styled(&card.title, Style::default().add_modifier(Modifier::BOLD)),
            ]),
            Line::from(""),
            Line::from(if card.body.is_empty() {
                "(empty body)"
            } else {
                &card.body
            }),
        ]);
        let paragraph = Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((self.detail_scroll, 0));
        frame.render_widget(paragraph, inner);

        let estimated_lines = estimate_wrapped_lines(card, inner.width.max(1));
        if estimated_lines > inner.height as usize {
            let mut scrollbar_state = ScrollbarState::new(estimated_lines)
                .position(self.detail_scroll as usize)
                .viewport_content_length(inner.height as usize);
            frame.render_stateful_widget(
                Scrollbar::new(ScrollbarOrientation::VerticalRight),
                area.inner(Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );
        }
    }

    fn draw_help(&self, frame: &mut Frame, area: Rect) {
        let help = Text::from(vec![
            Line::from("←/→ or h/l column   ↑/↓ or j/k card   Home/End first/last   PgUp/PgDn or [/] details"),
            Line::from("c/n new card   C/N new column   e edit title   a append body line   d/Delete delete"),
            Line::from("/ or g jump by display name   m move   r reload   ?/F1 help   Esc cancel   q/Ctrl-C quit"),
            Line::from("Input: type text, Backspace edits, Enter confirms. Move: ←/→ chooses target, Enter moves."),
            Line::from("All successful mutations are persisted under the board root immediately."),
        ]);
        frame.render_widget(
            Paragraph::new(help).wrap(Wrap { trim: false }).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Keyboard help "),
            ),
            area,
        );
    }

    fn draw_mode_panel(&self, frame: &mut Frame, area: Rect) {
        let (title, content, style) = match &self.mode {
            Mode::Input(flow) => {
                let (label, context, value) = flow.presentation();
                (
                    format!(" {label} "),
                    Line::from(vec![
                        Span::styled(context, Style::default().fg(Color::DarkGray)),
                        Span::raw(value),
                        Span::styled("▏", Style::default().fg(Color::Cyan)),
                    ]),
                    Style::default().fg(Color::Cyan),
                )
            }
            Mode::ConfirmDelete => {
                let name = self
                    .selected_card()
                    .map(|card| card.id.as_str())
                    .unwrap_or("");
                (
                    " Confirm delete ".to_owned(),
                    Line::from(format!(
                        "Delete card '{name}' permanently?  y/Enter yes · n/Esc no"
                    )),
                    Style::default().fg(Color::Red),
                )
            }
            Mode::Move { target } => {
                let name = self
                    .board
                    .columns
                    .get(*target)
                    .map(|column| column.name.as_str())
                    .unwrap_or("");
                (
                    " Move card ".to_owned(),
                    Line::from(format!(
                        "Target: {name}  ·  ←/→ choose · Enter move · Esc cancel"
                    )),
                    Style::default().fg(Color::Magenta),
                )
            }
            Mode::Browse => return,
        };
        frame.render_widget(Clear, area);
        frame.render_widget(
            Paragraph::new(content).block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(style)
                    .title(title),
            ),
            area,
        );
    }

    fn draw_footer(&self, frame: &mut Frame, area: Rect) {
        let status_style = if self.status_error {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };
        let shortcuts = " arrows navigate · / find column · c card · C column · e edit · a append · m move · d delete · ? help · q quit ";
        let status_width = area.width.saturating_sub(shortcuts.width() as u16 + 1) as usize;
        let status = truncate(&self.status, status_width);
        let line = Line::from(vec![
            Span::styled(format!(" {status}"), status_style),
            Span::raw(" "),
            Span::styled(
                shortcuts,
                Style::default().fg(Color::Black).bg(Color::White),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), area);
    }
}

impl InputFlow {
    fn value_mut(&mut self) -> &mut String {
        match self {
            Self::NewCardId { value }
            | Self::NewCardTitle { value, .. }
            | Self::NewColumnId { value }
            | Self::NewColumnName { value, .. }
            | Self::SelectColumn { value }
            | Self::EditTitle { value }
            | Self::AppendBody { value } => value,
        }
    }

    fn presentation(&self) -> (&'static str, String, &str) {
        match self {
            Self::NewCardId { value } => ("New card — step 1/2: card id", "ID: ".to_owned(), value),
            Self::NewCardTitle { id, value } => (
                "New card — step 2/2: title",
                format!("ID {id} · Title: "),
                value,
            ),
            Self::NewColumnId { value } => {
                ("New column — step 1/2: disk id", "ID: ".to_owned(), value)
            }
            Self::NewColumnName { id, value } => (
                "New column — step 2/2: display name",
                format!("ID {id} · Name: "),
                value,
            ),
            Self::SelectColumn { value } => (
                "Select column by display name",
                "Display name: ".to_owned(),
                value,
            ),
            Self::EditTitle { value } => ("Edit card title", "Title: ".to_owned(), value),
            Self::AppendBody { value } => ("Append one body line", "Line: ".to_owned(), value),
        }
    }
}

fn max_detail_height(total: u16, input: u16, help: u16) -> u16 {
    // Leave useful space for both board and details on normal terminals.
    let fixed = 3 + input + help + 2;
    let available = total.saturating_sub(fixed);
    (available / 2).max(5)
}

fn estimate_wrapped_lines(card: &crate::model::Card, width: u16) -> usize {
    let width = width.max(1) as usize;
    let wrapped = |line: &str| line.width().max(1).div_ceil(width);
    3 + wrapped(&card.title) + card.body.lines().map(wrapped).sum::<usize>().max(1)
}

fn truncate(value: &str, width: usize) -> String {
    if value.width() <= width {
        return value.to_owned();
    }
    if width <= 1 {
        return "…".chars().take(width).collect();
    }
    let mut result = String::new();
    for ch in value.chars() {
        if result.width() + ch.to_string().width() + 1 > width {
            break;
        }
        result.push(ch);
    }
    result.push('…');
    result
}
