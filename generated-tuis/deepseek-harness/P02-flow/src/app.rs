//! Application state machine: navigation, actions, and key handling.

use std::path::PathBuf;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::board::{self, Board, Card, Column};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Board,
    Detail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Input,
    Confirm,
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    NewCard,
    EditTitle,
    AppendBody,
    NewColumn,
    Filter,
}

impl InputAction {
    pub fn prompt(&self) -> String {
        match self {
            InputAction::NewCard => "New card title".to_string(),
            InputAction::EditTitle => "Edit title".to_string(),
            InputAction::AppendBody => "Append body line".to_string(),
            InputAction::NewColumn => "New column name".to_string(),
            InputAction::Filter => "Filter (substring)".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InputState {
    pub action: InputAction,
    pub buffer: String,
    pub cursor: usize,
}

impl Default for InputState {
    fn default() -> Self {
        InputState {
            action: InputAction::NewCard,
            buffer: String::new(),
            cursor: 0,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConfirmAction {
    DeleteCard {
        col_id: String,
        card_id: String,
        title: String,
    },
    DeleteColumn {
        col_id: String,
        name: String,
        count: usize,
    },
}

pub struct App {
    pub root: PathBuf,
    pub board: Board,
    pub focus: Focus,
    pub col_index: usize,
    pub card_index: usize,
    pub col_scroll: usize,
    pub card_scrolls: Vec<usize>,
    pub detail_scroll: u16,
    pub mode: Mode,
    pub input: InputState,
    pub confirm: Option<ConfirmAction>,
    pub filter: Option<String>,
    pub message: Option<String>,
    pub error: bool,
    pub quit: bool,
}

impl App {
    pub fn new(root: PathBuf) -> Result<Self, String> {
        let board = board::load_board(&root)?;
        let mut app = App {
            root,
            board,
            focus: Focus::Board,
            col_index: 0,
            card_index: 0,
            col_scroll: 0,
            card_scrolls: Vec::new(),
            detail_scroll: 0,
            mode: Mode::Normal,
            input: InputState::default(),
            confirm: None,
            filter: None,
            message: None,
            error: false,
            quit: false,
        };
        app.clamp();
        Ok(app)
    }

    // -- selection helpers -------------------------------------------------

    pub fn selected_column(&self) -> Option<&Column> {
        self.board.columns.get(self.col_index)
    }

    pub fn selected_card(&self) -> Option<&Card> {
        let col = self.board.columns.get(self.col_index)?;
        let vis = self.visible_indices(self.col_index);
        let real = *vis.get(self.card_index)?;
        col.cards.get(real)
    }

    pub fn visible_indices(&self, col: usize) -> Vec<usize> {
        let Some(column) = self.board.columns.get(col) else {
            return Vec::new();
        };
        match &self.filter {
            None => (0..column.cards.len()).collect(),
            Some(f) => {
                let f = f.to_lowercase();
                column
                    .cards
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| {
                        c.title.to_lowercase().contains(&f) || c.body.to_lowercase().contains(&f)
                    })
                    .map(|(i, _)| i)
                    .collect()
            }
        }
    }

    /// Clamp selection indices to valid ranges and keep scroll state sized.
    pub fn clamp(&mut self) {
        let n = self.board.columns.len();
        if self.card_scrolls.len() != n {
            self.card_scrolls.resize(n, 0);
        }
        if n == 0 {
            self.col_index = 0;
            self.card_index = 0;
            return;
        }
        if self.col_index >= n {
            self.col_index = n - 1;
        }
        let vis = self.visible_indices(self.col_index);
        if self.card_index >= vis.len() {
            self.card_index = vis.len().saturating_sub(1);
        }
    }

    pub fn confirm_text(&self) -> String {
        match &self.confirm {
            Some(ConfirmAction::DeleteCard { title, .. }) => {
                format!(" Delete card \"{}\"?  [y]es / [n]o", title)
            }
            Some(ConfirmAction::DeleteColumn { name, count, .. }) => {
                format!(" Delete column \"{}\" and its {} card(s)?  [y]es / [n]o", name, count)
            }
            None => String::new(),
        }
    }

    // -- key dispatch ------------------------------------------------------

    pub fn on_key(&mut self, key: KeyEvent) {
        // Ctrl+C quits from anywhere.
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        match self.mode {
            Mode::Help => self.mode = Mode::Normal,
            Mode::Confirm => self.handle_confirm(key),
            Mode::Input => self.handle_input(key),
            Mode::Normal => self.handle_normal(key),
        }
    }

    fn handle_confirm(&mut self, key: KeyEvent) {
        let action = self.confirm.take();
        self.mode = Mode::Normal;
        let yes = matches!(
            key.code,
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter
        );
        if yes {
            if let Some(a) = action {
                self.run_confirm(&a);
            }
        }
    }

    fn handle_input(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('u') => {
                    self.input.buffer.clear();
                    self.input.cursor = 0;
                    return;
                }
                KeyCode::Char('w') => {
                    self.delete_prev_word();
                    return;
                }
                _ => {}
            }
            return;
        }
        match key.code {
            KeyCode::Enter => {
                let text = self.input.buffer.clone();
                self.mode = Mode::Normal;
                self.submit_input(&text);
            }
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Backspace => {
                if self.input.cursor > 0 {
                    let pos = self.input.cursor - 1;
                    self.input.buffer = remove_char_at(&self.input.buffer, pos);
                    self.input.cursor = pos;
                }
            }
            KeyCode::Delete => {
                if self.input.cursor < self.input.buffer.chars().count() {
                    self.input.buffer = remove_char_at(&self.input.buffer, self.input.cursor);
                }
            }
            KeyCode::Left => self.input.cursor = self.input.cursor.saturating_sub(1),
            KeyCode::Right => {
                self.input.cursor = (self.input.cursor + 1).min(self.input.buffer.chars().count())
            }
            KeyCode::Home => self.input.cursor = 0,
            KeyCode::End => self.input.cursor = self.input.buffer.chars().count(),
            KeyCode::Char(c) => {
                let len = self.input.buffer.chars().count();
                if self.input.cursor > len {
                    self.input.cursor = len;
                }
                self.input.buffer = insert_char_at(&self.input.buffer, self.input.cursor, c);
                self.input.cursor += 1;
            }
            _ => {}
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('?') => self.mode = Mode::Help,
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Board => Focus::Detail,
                    Focus::Detail => Focus::Board,
                }
            }
            KeyCode::Char('n') => self.start_input(InputAction::NewCard, ""),
            KeyCode::Char('e') | KeyCode::Enter => match self.selected_card() {
                None => self.set_error("No card selected".to_string()),
                Some(c) => {
                    let title = c.title.clone();
                    self.start_input(InputAction::EditTitle, &title);
                }
            },
            KeyCode::Char('a') => {
                if self.selected_card().is_none() {
                    self.set_error("No card selected".to_string());
                } else {
                    self.start_input(InputAction::AppendBody, "");
                }
            }
            KeyCode::Char('d') => self.confirm_delete_card(),
            KeyCode::Char('c') => self.start_input(InputAction::NewColumn, ""),
            KeyCode::Char('D') => self.confirm_delete_column(),
            KeyCode::Char('>') => self.move_card_to(1),
            KeyCode::Char('<') => self.move_card_to(-1),
            KeyCode::Char('/') => {
                let cur = self.filter.clone().unwrap_or_default();
                self.start_input(InputAction::Filter, &cur);
            }
            KeyCode::Char('r') => {
                self.reload();
                self.set_info("Reloaded board from disk".to_string());
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if self.focus == Focus::Board {
                    self.move_column(-1);
                }
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if self.focus == Focus::Board {
                    self.move_column(1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => match self.focus {
                Focus::Board => self.move_card(-1),
                Focus::Detail => self.detail_scroll = self.detail_scroll.saturating_sub(1),
            },
            KeyCode::Down | KeyCode::Char('j') => match self.focus {
                Focus::Board => self.move_card(1),
                Focus::Detail => self.detail_scroll = self.detail_scroll.saturating_add(1),
            },
            KeyCode::Char('g') => {
                if self.focus == Focus::Board {
                    self.card_index = 0;
                }
            }
            KeyCode::Char('G') => {
                if self.focus == Focus::Board {
                    let vis = self.visible_indices(self.col_index);
                    self.card_index = vis.len().saturating_sub(1);
                }
            }
            _ => {}
        }
    }

    // -- navigation --------------------------------------------------------

    fn move_column(&mut self, delta: isize) {
        let n = self.board.columns.len() as isize;
        if n == 0 {
            return;
        }
        let cur = self.col_index as isize;
        self.col_index = (cur + delta).clamp(0, n - 1) as usize;
        self.card_index = 0;
        self.clamp();
    }

    fn move_card(&mut self, delta: isize) {
        let n = self.visible_indices(self.col_index).len() as isize;
        if n == 0 {
            return;
        }
        let cur = self.card_index as isize;
        self.card_index = (cur + delta).clamp(0, n - 1) as usize;
    }

    // -- actions -----------------------------------------------------------

    fn start_input(&mut self, action: InputAction, prefill: &str) {
        let cursor = prefill.chars().count();
        self.input = InputState {
            action,
            buffer: prefill.to_string(),
            cursor,
        };
        self.mode = Mode::Input;
    }

    fn submit_input(&mut self, text: &str) {
        match self.input.action.clone() {
            InputAction::NewCard => {
                if text.trim().is_empty() {
                    self.set_error("Card title must not be empty".to_string());
                    return;
                }
                let Some(col_id) = self.current_col_id() else {
                    self.set_error("No column selected".to_string());
                    return;
                };
                match board::create_card(&self.root, &col_id, text, "") {
                    Ok(id) => {
                        self.reload();
                        self.select_card_by_id(&col_id, &id);
                        self.set_info(format!("Created card \"{}\"", text.trim()));
                    }
                    Err(e) => self.set_error(e),
                }
            }
            InputAction::EditTitle => {
                if text.trim().is_empty() {
                    self.set_error("Card title must not be empty".to_string());
                    return;
                }
                let Some((col_id, card_id)) = self.selected_ids() else {
                    self.set_error("No card selected".to_string());
                    return;
                };
                match board::edit_title(&self.root, &col_id, &card_id, text) {
                    Ok(()) => {
                        self.reload();
                        self.select_card_by_id(&col_id, &card_id);
                        self.set_info("Title updated".to_string());
                    }
                    Err(e) => self.set_error(e),
                }
            }
            InputAction::AppendBody => {
                if text.is_empty() {
                    self.set_error("Nothing to append".to_string());
                    return;
                }
                let Some((col_id, card_id)) = self.selected_ids() else {
                    self.set_error("No card selected".to_string());
                    return;
                };
                match board::append_body(&self.root, &col_id, &card_id, text) {
                    Ok(()) => {
                        self.reload();
                        self.select_card_by_id(&col_id, &card_id);
                        self.set_info("Body line appended".to_string());
                    }
                    Err(e) => self.set_error(e),
                }
            }
            InputAction::NewColumn => {
                if text.trim().is_empty() {
                    self.set_error("Column name must not be empty".to_string());
                    return;
                }
                match board::create_column(&self.root, text) {
                    Ok(id) => {
                        self.reload();
                        self.select_column_by_id(&id);
                        self.set_info(format!("Created column \"{}\"", text.trim()));
                    }
                    Err(e) => self.set_error(e),
                }
            }
            InputAction::Filter => {
                self.filter = if text.trim().is_empty() {
                    None
                } else {
                    Some(text.trim().to_string())
                };
                self.card_index = 0;
                self.clamp();
                match &self.filter {
                    Some(f) => self.set_info(format!("Filter active: \"{}\"", f)),
                    None => self.set_info("Filter cleared".to_string()),
                }
            }
        }
    }

    fn confirm_delete_card(&mut self) {
        let Some(card) = self.selected_card().cloned() else {
            self.set_error("No card selected".to_string());
            return;
        };
        let Some(col_id) = self.current_col_id() else {
            return;
        };
        self.confirm = Some(ConfirmAction::DeleteCard {
            col_id,
            card_id: card.id,
            title: card.title,
        });
        self.mode = Mode::Confirm;
    }

    fn confirm_delete_column(&mut self) {
        let Some(col) = self.selected_column().cloned() else {
            self.set_error("No column selected".to_string());
            return;
        };
        let count = col.cards.len();
        self.confirm = Some(ConfirmAction::DeleteColumn {
            col_id: col.id,
            name: col.name,
            count,
        });
        self.mode = Mode::Confirm;
    }

    fn run_confirm(&mut self, action: &ConfirmAction) {
        match action {
            ConfirmAction::DeleteCard { col_id, card_id, .. } => {
                match board::delete_card(&self.root, col_id, card_id) {
                    Ok(()) => {
                        self.reload();
                        self.set_info(format!("Deleted card \"{}\"", card_id));
                    }
                    Err(e) => self.set_error(e),
                }
            }
            ConfirmAction::DeleteColumn { col_id, .. } => {
                match board::delete_column(&self.root, col_id) {
                    Ok(()) => {
                        self.col_index = 0;
                        self.card_index = 0;
                        self.reload();
                        self.set_info(format!("Deleted column \"{}\"", col_id));
                    }
                    Err(e) => self.set_error(e),
                }
            }
        }
    }

    fn move_card_to(&mut self, delta: isize) {
        let Some(card) = self.selected_card().cloned() else {
            self.set_error("No card selected".to_string());
            return;
        };
        let Some(from) = self.current_col_id() else {
            return;
        };
        let to_index = self.col_index as isize + delta;
        if to_index < 0 || to_index >= self.board.columns.len() as isize {
            self.set_error("No adjacent column in that direction".to_string());
            return;
        }
        let to_col = self.board.columns[to_index as usize].clone();
        match board::move_card(&self.root, &from, &to_col.id, &card.id) {
            Ok(()) => {
                self.reload();
                self.select_card_by_id(&to_col.id, &card.id);
                self.set_info(format!("Moved \"{}\" to {}", card.title, to_col.name));
            }
            Err(e) => self.set_error(e),
        }
    }

    fn reload(&mut self) {
        match board::load_board(&self.root) {
            Ok(b) => self.board = b,
            Err(e) => self.set_error(e),
        }
        self.clamp();
    }

    // -- selection lookups -------------------------------------------------

    fn current_col_id(&self) -> Option<String> {
        self.selected_column().map(|c| c.id.clone())
    }

    fn selected_ids(&self) -> Option<(String, String)> {
        let col = self.selected_column()?;
        let card = self.selected_card()?;
        Some((col.id.clone(), card.id.clone()))
    }

    fn select_column_by_id(&mut self, id: &str) {
        if let Some(i) = self.board.columns.iter().position(|c| c.id == id) {
            self.col_index = i;
            self.card_index = 0;
            self.clamp();
        }
    }

    fn select_card_by_id(&mut self, col_id: &str, card_id: &str) {
        if let Some(ci) = self.board.columns.iter().position(|c| c.id == col_id) {
            self.col_index = ci;
            let vis = self.visible_indices(ci);
            if let Some(pos) = vis
                .iter()
                .position(|&ri| self.board.columns[ci].cards[ri].id == card_id)
            {
                self.card_index = pos;
            }
            self.clamp();
        }
    }

    // -- status ------------------------------------------------------------

    fn set_info(&mut self, msg: String) {
        self.message = Some(msg);
        self.error = false;
    }

    fn set_error(&mut self, msg: String) {
        self.message = Some(msg);
        self.error = true;
    }

    fn delete_prev_word(&mut self) {
        let pos = self.input.cursor;
        let chars: Vec<char> = self.input.buffer.chars().collect();
        let mut start = pos;
        while start > 0 && chars[start - 1].is_whitespace() {
            start -= 1;
        }
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        let head: String = chars[..start].iter().collect();
        let tail: String = chars[pos..].iter().collect();
        self.input.buffer = head + &tail;
        self.input.cursor = start;
    }
}

fn insert_char_at(s: &str, pos: usize, c: char) -> String {
    let mut out = String::with_capacity(s.len() + c.len_utf8());
    let mut i = 0;
    for ch in s.chars() {
        if i == pos {
            out.push(c);
        }
        out.push(ch);
        i += 1;
    }
    if pos >= i {
        out.push(c);
    }
    out
}

fn remove_char_at(s: &str, pos: usize) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    for ch in s.chars() {
        if i != pos {
            out.push(ch);
        }
        i += 1;
    }
    out
}
