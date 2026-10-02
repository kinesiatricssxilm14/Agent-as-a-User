//! Application state and the event loop.
//!
//! The board is re-read from disk after every mutation, so what the UI shows is always what the
//! filesystem actually contains rather than an optimistically updated copy.

use std::path::PathBuf;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::config::{self, Config, RootSource};
use crate::input::TextInput;
use crate::keymap::{self, Action, ModeKind};
use crate::model::Board;
use crate::slug;
use crate::store::Store;

/// Severity of a status message, which drives its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Success,
    Error,
}

/// A piece of feedback shown in the status area.
#[derive(Debug, Clone)]
pub struct Message {
    pub kind: MessageKind,
    pub text: String,
}

/// What a prompt will do with the text once it is confirmed.
///
/// Multi-step flows (title then id) carry the earlier answers along, so no partially applied
/// state is left behind if the user cancels halfway.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptKind {
    NewCardTitle,
    /// Card id, carrying the title already entered.
    NewCardId { title: String },
    EditTitle,
    AppendBody,
    NewColumnName,
    /// Column id, carrying the display name already entered.
    NewColumnId { display_name: String },
    RenameColumn,
    GotoColumn,
    MoveCard,
    /// Retry a move that hit an id collision in the target column.
    MoveCardNewId { to_column: String },
    Search,
    SetBoardRoot,
    ConfigKey,
    /// Value for a configuration key already chosen.
    ConfigValue { key: String },
}

/// What a pending confirmation will do if accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmKind {
    DeleteCard { column_id: String, card_id: String },
}

/// The current interaction mode.
#[derive(Debug, Clone)]
pub enum Mode {
    Normal,
    Prompt { kind: PromptKind, label: String, input: TextInput },
    BodyEditor { column_id: String, card_id: String, input: TextInput },
    Confirm { kind: ConfirmKind, question: String },
}

impl Mode {
    pub fn kind(&self) -> ModeKind {
        match self {
            Mode::Normal => ModeKind::Normal,
            Mode::Prompt { .. } => ModeKind::Prompt,
            Mode::BodyEditor { .. } => ModeKind::BodyEditor,
            Mode::Confirm { .. } => ModeKind::Confirm,
        }
    }
}

/// Which pane keyboard focus is in. Only affects what ↑/↓ scroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Board,
    Detail,
}

/// The whole application state.
pub struct App {
    pub store: Store,
    pub config: Config,
    pub root_source: RootSource,
    pub board: Board,

    /// Index into `board.columns`.
    pub selected_column: usize,
    /// Index into the *filtered* card list of the selected column.
    pub selected_card: usize,

    pub mode: Mode,
    pub focus: Focus,
    pub show_help: bool,
    pub message: Option<Message>,
    /// Active search text; empty means no filter.
    pub search: String,

    /// Vertical scroll offset of the detail body, in wrapped display lines.
    pub detail_scroll: u16,
    /// First visible column, for boards too wide to fit.
    pub column_offset: usize,
    /// Per-column vertical scroll offsets, parallel to `board.columns`.
    pub card_scroll: Vec<usize>,

    pub should_quit: bool,
}

impl App {
    /// Build the initial state by loading the board from disk.
    pub fn new(store: Store, config: Config, root_source: RootSource) -> Self {
        let (board, message) = match store.load() {
            Ok(board) => {
                let message = if !store.is_initialised() {
                    Some(Message {
                        kind: MessageKind::Info,
                        text: format!(
                            "No board.txt in {} yet - press I to create one, or c to add a column.",
                            store.root.display()
                        ),
                    })
                } else {
                    board_warning_message(&board)
                };
                (board, message)
            }
            Err(e) => (
                Board { root: store.root.clone(), columns: Vec::new(), warnings: vec![e.clone()] },
                Some(Message { kind: MessageKind::Error, text: e }),
            ),
        };

        let card_scroll = vec![0; board.columns.len()];
        Self {
            store,
            config,
            root_source,
            board,
            selected_column: 0,
            selected_card: 0,
            mode: Mode::Normal,
            focus: Focus::Board,
            show_help: false,
            message,
            search: String::new(),
            detail_scroll: 0,
            column_offset: 0,
            card_scroll,
            should_quit: false,
        }
    }

    // --- selection helpers ---------------------------------------------------------------

    /// Cards of a column that match the active search.
    ///
    /// Returns indices into the column's full card list so callers can map back to real cards.
    pub fn filtered_indices(&self, column_index: usize) -> Vec<usize> {
        let Some(column) = self.board.columns.get(column_index) else {
            return Vec::new();
        };
        if self.search.is_empty() {
            return (0..column.cards.len()).collect();
        }
        let needle = self.search.to_lowercase();
        column
            .cards
            .iter()
            .enumerate()
            .filter(|(_, card)| {
                card.id.to_lowercase().contains(&needle)
                    || card.title.to_lowercase().contains(&needle)
                    || card.body.to_lowercase().contains(&needle)
            })
            .map(|(i, _)| i)
            .collect()
    }

    /// Number of visible cards in the selected column.
    pub fn visible_card_count(&self) -> usize {
        self.filtered_indices(self.selected_column).len()
    }

    /// The currently selected card, if any.
    pub fn current_card(&self) -> Option<&crate::model::Card> {
        let column = self.board.columns.get(self.selected_column)?;
        let indices = self.filtered_indices(self.selected_column);
        let real = *indices.get(self.selected_card)?;
        column.cards.get(real)
    }

    pub fn current_column_id(&self) -> Option<String> {
        self.board.columns.get(self.selected_column).map(|c| c.id.clone())
    }

    pub fn current_column_name(&self) -> Option<String> {
        self.board.columns.get(self.selected_column).map(|c| c.display_name.clone())
    }

    /// Clamp selection indices after the board or filter changes.
    fn clamp_selection(&mut self) {
        if self.board.columns.is_empty() {
            self.selected_column = 0;
            self.selected_card = 0;
            return;
        }
        self.selected_column = self.selected_column.min(self.board.columns.len() - 1);
        let count = self.visible_card_count();
        self.selected_card = if count == 0 { 0 } else { self.selected_card.min(count - 1) };
        if self.card_scroll.len() != self.board.columns.len() {
            self.card_scroll.resize(self.board.columns.len(), 0);
        }
    }

    // --- disk access --------------------------------------------------------------------

    /// Re-read the board, keeping the selection on the same card id where possible.
    pub fn reload(&mut self) {
        let previous_column = self.current_column_id();
        let previous_card = self.current_card().map(|c| c.id.clone());

        match self.store.load() {
            Ok(board) => {
                self.board = board;
                self.card_scroll.resize(self.board.columns.len(), 0);

                if let Some(column_id) = previous_column {
                    if let Some(index) = self.board.column_index(&column_id) {
                        self.selected_column = index;
                    }
                }
                self.clamp_selection();
                if let Some(card_id) = previous_card {
                    self.select_card_by_id(&card_id);
                }
            }
            Err(e) => self.set_error(e),
        }
    }

    /// Move the selection to a card id in the current column, if it is visible.
    fn select_card_by_id(&mut self, card_id: &str) {
        let indices = self.filtered_indices(self.selected_column);
        if let Some(column) = self.board.columns.get(self.selected_column) {
            if let Some(position) = indices
                .iter()
                .position(|&real| column.cards.get(real).is_some_and(|c| c.id == card_id))
            {
                self.selected_card = position;
            }
        }
    }

    /// Select a card by id anywhere on the board, switching columns if needed.
    /// Used after a move so the card stays under the cursor.
    fn select_card_anywhere(&mut self, card_id: &str) {
        for index in 0..self.board.columns.len() {
            let indices = self.filtered_indices(index);
            let column = &self.board.columns[index];
            if let Some(position) = indices
                .iter()
                .position(|&real| column.cards.get(real).is_some_and(|c| c.id == card_id))
            {
                self.selected_column = index;
                self.selected_card = position;
                return;
            }
        }
    }

    // --- messages -----------------------------------------------------------------------

    pub fn set_info(&mut self, text: impl Into<String>) {
        self.message = Some(Message { kind: MessageKind::Info, text: text.into() });
    }

    pub fn set_success(&mut self, text: impl Into<String>) {
        self.message = Some(Message { kind: MessageKind::Success, text: text.into() });
    }

    pub fn set_error(&mut self, text: impl Into<String>) {
        self.message = Some(Message { kind: MessageKind::Error, text: text.into() });
    }

    /// Report the outcome of an operation: success text, or the error verbatim.
    fn report(&mut self, result: crate::store::Result<()>, success: impl Into<String>) {
        match result {
            Ok(()) => {
                self.set_success(success);
                self.reload();
            }
            Err(e) => self.set_error(e),
        }
    }

    // --- event loop ---------------------------------------------------------------------

    /// Wait for one event and apply it. Blocking, so an idle application uses no CPU.
    pub fn handle_next_event(&mut self) -> std::io::Result<()> {
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => self.handle_key(key),
            // Resize redraws on the next loop iteration; mouse and focus events are ignored,
            // since the specification requires the tool to be usable without a mouse.
            _ => {}
        }
        Ok(())
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        match &self.mode {
            Mode::Normal => self.handle_normal_key(key),
            Mode::Prompt { .. } => self.handle_prompt_key(key),
            Mode::BodyEditor { .. } => self.handle_body_key(key),
            Mode::Confirm { .. } => self.handle_confirm_key(key),
        }
    }

    fn handle_normal_key(&mut self, key: KeyEvent) {
        let Some(action) = keymap::resolve_normal(key) else { return };
        self.dispatch(action);
    }

    /// Apply a resolved action. Split out so tests can drive the application by action.
    pub fn dispatch(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::ToggleHelp => self.show_help = !self.show_help,
            Action::Reload => {
                self.reload();
                self.set_info(format!("Reloaded {}", self.store.root.display()));
            }

            Action::ColumnLeft => self.move_column(-1),
            Action::ColumnRight => self.move_column(1),
            Action::CardUp => self.move_card_selection(-1),
            Action::CardDown => self.move_card_selection(1),
            Action::CardFirst => self.jump_card(true),
            Action::CardLast => self.jump_card(false),
            Action::CardPageUp => self.page_selection(-1),
            Action::CardPageDown => self.page_selection(1),
            Action::FocusDetail => {
                if self.current_card().is_some() {
                    self.focus = Focus::Detail;
                    self.set_info("Card pane focused - ↑/↓ scroll the body, Esc returns to the board");
                } else {
                    self.set_info("No card selected");
                }
            }
            Action::ClearSearch => {
                if self.focus == Focus::Detail {
                    self.focus = Focus::Board;
                    self.set_info("Board focused");
                } else if !self.search.is_empty() {
                    self.search.clear();
                    self.clamp_selection();
                    self.set_info("Search cleared");
                }
            }

            Action::NewCard => self.start_new_card(),
            Action::EditTitle => self.start_edit_title(),
            Action::AppendBody => self.start_append_body(),
            Action::EditBody => self.start_body_editor(),
            Action::DeleteCard => self.start_delete_card(),

            Action::MoveCardPrompt => self.start_move_card(),
            Action::MoveCardLeft => self.move_card_to_adjacent(-1),
            Action::MoveCardRight => self.move_card_to_adjacent(1),
            Action::ReorderUp => self.reorder(-1),
            Action::ReorderDown => self.reorder(1),

            Action::NewColumn => self.start_new_column(),
            Action::RenameColumn => self.start_rename_column(),
            Action::GotoColumn => self.start_goto_column(),
            Action::InitBoard => self.initialise_board(),
            Action::NormaliseOrder => self.normalise_order(),

            Action::Search => self.start_search(),
            Action::SearchNext => self.step_match(1),
            Action::SearchPrev => self.step_match(-1),

            Action::SetBoardRoot => self.start_set_board_root(),
            Action::EditConfig => self.start_config_key(),

            // Confirm and Cancel only mean something inside a prompt.
            Action::Confirm | Action::Cancel => {}
        }
    }

    // --- navigation ---------------------------------------------------------------------

    fn move_column(&mut self, delta: isize) {
        if self.board.columns.is_empty() {
            return;
        }
        let count = self.board.columns.len() as isize;
        // Wrap around, so Tab cycles.
        let next = (self.selected_column as isize + delta).rem_euclid(count);
        self.selected_column = next as usize;
        self.selected_card = 0;
        self.detail_scroll = 0;
        self.clamp_selection();
    }

    fn move_card_selection(&mut self, delta: isize) {
        // With the card pane focused, ↑/↓ scroll the body instead of changing selection.
        if self.focus == Focus::Detail {
            self.scroll_detail(delta);
            return;
        }
        let count = self.visible_card_count();
        if count == 0 {
            return;
        }
        let next = (self.selected_card as isize + delta).clamp(0, count as isize - 1);
        if next as usize != self.selected_card {
            self.selected_card = next as usize;
            // A new card means a new body: start reading it from the top.
            self.detail_scroll = 0;
        }
    }

    fn jump_card(&mut self, first: bool) {
        let count = self.visible_card_count();
        if count == 0 {
            return;
        }
        self.selected_card = if first { 0 } else { count - 1 };
        self.detail_scroll = 0;
    }

    fn page_selection(&mut self, direction: isize) {
        if self.focus == Focus::Detail {
            self.scroll_detail(direction * 10);
            return;
        }
        self.move_card_selection(direction * 10);
    }

    fn scroll_detail(&mut self, delta: isize) {
        let next = self.detail_scroll as isize + delta;
        // Upper bound is clamped at render time, where the wrapped line count is known.
        self.detail_scroll = next.max(0) as u16;
    }

    /// Move to the next or previous card matching the search, wrapping across columns.
    fn step_match(&mut self, delta: isize) {
        if self.search.is_empty() {
            self.set_info("No active search - press / to search");
            return;
        }
        let count = self.visible_card_count();
        if count > 0 {
            let next = self.selected_card as isize + delta;
            if next >= 0 && next < count as isize {
                self.selected_card = next as usize;
                self.detail_scroll = 0;
                return;
            }
        }
        // Ran off the end of this column: find the next column with a match.
        let columns = self.board.columns.len();
        if columns == 0 {
            return;
        }
        for step in 1..=columns {
            let index = (self.selected_column as isize + delta * step as isize)
                .rem_euclid(columns as isize) as usize;
            let matches = self.filtered_indices(index);
            if !matches.is_empty() {
                self.selected_column = index;
                self.selected_card = if delta > 0 { 0 } else { matches.len() - 1 };
                self.detail_scroll = 0;
                return;
            }
        }
        self.set_info(format!("No other card matches '{}'", self.search));
    }

    // --- prompt plumbing ----------------------------------------------------------------

    fn open_prompt(&mut self, kind: PromptKind, label: impl Into<String>, initial: &str) {
        self.mode = Mode::Prompt {
            kind,
            label: label.into(),
            input: TextInput::with_text(initial),
        };
    }

    fn handle_prompt_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let Mode::Prompt { kind, input, .. } = &mut self.mode else { return };

        if ctrl {
            match key.code {
                KeyCode::Char('u') => input.delete_line(),
                KeyCode::Char('w') => input.delete_word(),
                // Ctrl+C aborts the prompt rather than the program: less destructive, and Esc
                // is documented for the same thing.
                KeyCode::Char('c') => self.mode = Mode::Normal,
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Enter => {
                let kind = kind.clone();
                let text = input.text().to_string();
                self.mode = Mode::Normal;
                self.submit_prompt(kind, text);
            }
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.set_info("Cancelled");
            }
            KeyCode::Tab => self.complete_prompt(),
            KeyCode::Backspace => input.backspace(),
            KeyCode::Delete => input.delete(),
            KeyCode::Left => input.move_left(),
            KeyCode::Right => input.move_right(),
            KeyCode::Home => input.move_home(),
            KeyCode::End => input.move_end(),
            // Any printable character is text here, so command letters cannot fire.
            KeyCode::Char(ch) => input.insert(ch),
            _ => {}
        }
    }

    /// Complete a partially typed column display name, where the prompt expects one.
    fn complete_prompt(&mut self) {
        let (kind, typed) = match &self.mode {
            Mode::Prompt { kind, input, .. } => (kind.clone(), input.text().to_string()),
            _ => return,
        };
        if !matches!(kind, PromptKind::GotoColumn | PromptKind::MoveCard) {
            return;
        }
        let needle = typed.trim().to_lowercase();
        let matches: Vec<String> = self
            .board
            .columns
            .iter()
            .filter(|c| c.display_name.to_lowercase().starts_with(&needle))
            .map(|c| c.display_name.clone())
            .collect();

        match matches.len() {
            0 => self.set_info("No column name starts with that"),
            1 => {
                if let Mode::Prompt { input, .. } = &mut self.mode {
                    *input = TextInput::with_text(matches[0].clone());
                }
            }
            _ => {
                let joined = matches.join(", ");
                self.set_info(format!("Matches: {joined}"));
            }
        }
    }

    /// Apply a confirmed prompt.
    fn submit_prompt(&mut self, kind: PromptKind, text: String) {
        match kind {
            PromptKind::NewCardTitle => {
                let title = text.trim().to_string();
                if title.is_empty() {
                    self.set_error("Card title must not be empty");
                    return;
                }
                let Some(column_id) = self.current_column_id() else { return };
                let suggested = self.store.suggest_card_id(&column_id, &title);
                self.open_prompt(
                    PromptKind::NewCardId { title },
                    "Card id (file name without .md)",
                    &suggested,
                );
            }
            PromptKind::NewCardId { title } => {
                let Some(column_id) = self.current_column_id() else { return };
                match slug::normalize_and_validate(&text) {
                    Ok(card_id) => match self.store.create_card(&column_id, &card_id, &title) {
                        Ok(created) => {
                            self.set_success(format!(
                                "Created {}/{}.md",
                                self.store.root.join("cols").join(&column_id).display(),
                                created
                            ));
                            self.reload();
                            self.select_card_by_id(&created);
                        }
                        Err(e) => self.set_error(e),
                    },
                    Err(e) => self.set_error(format!("Invalid card id: {e}")),
                }
            }

            PromptKind::EditTitle => {
                let Some(column_id) = self.current_column_id() else { return };
                let Some(card_id) = self.current_card().map(|c| c.id.clone()) else { return };
                let result = self.store.set_card_title(&column_id, &card_id, text.trim());
                self.report(result, format!("Updated the title of {card_id}"));
            }
            PromptKind::AppendBody => {
                if text.is_empty() {
                    self.set_info("Nothing appended");
                    return;
                }
                let Some(column_id) = self.current_column_id() else { return };
                let Some(card_id) = self.current_card().map(|c| c.id.clone()) else { return };
                let result = self.store.append_card_line(&column_id, &card_id, &text);
                self.report(result, format!("Appended a line to {card_id}.md"));
            }

            PromptKind::NewColumnName => {
                let display_name = text.trim().to_string();
                if display_name.is_empty() {
                    self.set_error("Column name must not be empty");
                    return;
                }
                let suggested = self.suggest_column_id(&display_name);
                self.open_prompt(
                    PromptKind::NewColumnId { display_name },
                    "Column id (directory name under cols/)",
                    &suggested,
                );
            }
            PromptKind::NewColumnId { display_name } => match slug::normalize_and_validate(&text) {
                Ok(id) => {
                    let result = self.store.create_column(&id, &display_name);
                    let created = id.clone();
                    self.report(result, format!("Created column '{display_name}' (cols/{created}/)"));
                    if let Some(index) = self.board.column_index(&created) {
                        self.selected_column = index;
                        self.selected_card = 0;
                    }
                }
                Err(e) => self.set_error(format!("Invalid column id: {e}")),
            },
            PromptKind::RenameColumn => {
                let Some(column_id) = self.current_column_id() else { return };
                let result = self.store.rename_column(&column_id, text.trim());
                self.report(result, format!("Renamed column to '{}'", text.trim()));
            }

            PromptKind::GotoColumn => match self.board.column_by_display_name(&text) {
                Some(index) => {
                    self.selected_column = index;
                    self.selected_card = 0;
                    self.detail_scroll = 0;
                    let name = self.board.columns[index].display_name.clone();
                    self.set_info(format!("Column '{name}'"));
                }
                None => self.set_error(format!("No column named '{}'", text.trim())),
            },

            PromptKind::MoveCard => self.perform_move(&text, None),
            PromptKind::MoveCardNewId { to_column } => {
                let target = to_column.clone();
                self.perform_move_by_id(&target, Some(text.trim()));
            }

            PromptKind::Search => {
                self.search = text.trim().to_string();
                self.selected_card = 0;
                self.detail_scroll = 0;
                self.clamp_selection();
                if self.search.is_empty() {
                    self.set_info("Search cleared");
                } else {
                    let total: usize =
                        (0..self.board.columns.len()).map(|i| self.filtered_indices(i).len()).sum();
                    self.set_info(format!("{total} card(s) match '{}'", self.search));
                }
            }

            PromptKind::SetBoardRoot => {
                let path = text.trim();
                if path.is_empty() {
                    self.set_error("Board root must not be empty");
                    return;
                }
                let root = PathBuf::from(path);
                match self.config.set_and_save(config::KEY_BOARD_ROOT, path) {
                    Ok(()) => {
                        self.store = Store::new(&root);
                        self.root_source = RootSource::ConfigFile;
                        self.selected_column = 0;
                        self.selected_card = 0;
                        self.column_offset = 0;
                        self.detail_scroll = 0;
                        self.reload();
                        self.set_success(format!(
                            "Board root set to {} and saved to {}",
                            root.display(),
                            self.config.path.display()
                        ));
                    }
                    Err(e) => self.set_error(e),
                }
            }

            PromptKind::ConfigKey => {
                let key = text.trim().to_string();
                if key.is_empty() {
                    self.set_error("Configuration key must not be empty");
                    return;
                }
                let current = self.config.get(&key).unwrap_or_default().to_string();
                self.open_prompt(PromptKind::ConfigValue { key: key.clone() }, format!("Value for '{key}'"), &current);
            }
            PromptKind::ConfigValue { key } => {
                let value = text.trim().to_string();
                match self.config.set_and_save(&key, &value) {
                    Ok(()) => {
                        self.set_success(format!(
                            "Set {key} = {value} in {}",
                            self.config.path.display()
                        ));
                        // Changing the root through the generic editor must take effect too.
                        if key == config::KEY_BOARD_ROOT && !value.is_empty() {
                            self.store = Store::new(PathBuf::from(&value));
                            self.root_source = RootSource::ConfigFile;
                            self.selected_column = 0;
                            self.selected_card = 0;
                            self.reload();
                        }
                    }
                    Err(e) => self.set_error(e),
                }
            }
        }
    }

    fn suggest_column_id(&self, display_name: &str) -> String {
        let existing: Vec<String> = self.board.columns.iter().map(|c| c.id.clone()).collect();
        let taken = |id: &str| existing.iter().any(|e| e == id);
        let slug = slug::slugify(display_name);
        if slug.is_empty() {
            let mut n = 1;
            loop {
                let candidate = format!("col-{n}");
                if !taken(&candidate) {
                    return candidate;
                }
                n += 1;
            }
        } else {
            slug::unique_id(&slug, taken)
        }
    }

    // --- body editor --------------------------------------------------------------------

    fn handle_body_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let Mode::BodyEditor { column_id, card_id, input } = &mut self.mode else { return };

        if ctrl {
            match key.code {
                KeyCode::Char('s') => {
                    let (column_id, card_id, body) =
                        (column_id.clone(), card_id.clone(), input.text().to_string());
                    self.mode = Mode::Normal;
                    let result = self.store.set_card_body(&column_id, &card_id, &body);
                    self.report(result, format!("Saved the body of {card_id}.md"));
                }
                KeyCode::Char('u') => input.delete_line(),
                KeyCode::Char('w') => input.delete_word(),
                KeyCode::Char('c') => {
                    self.mode = Mode::Normal;
                    self.set_info("Edit discarded");
                }
                _ => {}
            }
            return;
        }

        match key.code {
            // Enter inserts a newline here; Ctrl+S saves. Otherwise a multi-line body would be
            // impossible to type.
            KeyCode::Enter => input.insert('\n'),
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.set_info("Edit discarded");
            }
            KeyCode::Backspace => input.backspace(),
            KeyCode::Delete => input.delete(),
            KeyCode::Left => input.move_left(),
            KeyCode::Right => input.move_right(),
            KeyCode::Up => input.move_up(),
            KeyCode::Down => input.move_down(),
            KeyCode::Home => input.move_home(),
            KeyCode::End => input.move_end(),
            KeyCode::Tab => input.insert_str("    "),
            KeyCode::Char(ch) => input.insert(ch),
            _ => {}
        }
    }

    // --- confirmation -------------------------------------------------------------------

    fn handle_confirm_key(&mut self, key: KeyEvent) {
        let Mode::Confirm { kind, .. } = &self.mode else { return };
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                let kind = kind.clone();
                self.mode = Mode::Normal;
                self.apply_confirm(kind);
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.set_info("Cancelled");
            }
            _ => {}
        }
    }

    fn apply_confirm(&mut self, kind: ConfirmKind) {
        match kind {
            ConfirmKind::DeleteCard { column_id, card_id } => {
                let result = self.store.delete_card(&column_id, &card_id);
                self.report(result, format!("Deleted {card_id}.md"));
            }
        }
    }

    // --- operation entry points ---------------------------------------------------------

    /// Guard used by every card operation, so a missing selection reports rather than panics.
    fn require_card(&mut self) -> Option<(String, String)> {
        let Some(column_id) = self.current_column_id() else {
            self.set_error("There are no columns yet - press c to create one");
            return None;
        };
        match self.current_card().map(|c| c.id.clone()) {
            Some(card_id) => Some((column_id, card_id)),
            None => {
                self.set_error("No card selected - press n to create one");
                None
            }
        }
    }

    fn start_new_card(&mut self) {
        if self.board.columns.is_empty() {
            self.set_error("There are no columns yet - press c to create one");
            return;
        }
        let name = self.current_column_name().unwrap_or_default();
        self.open_prompt(PromptKind::NewCardTitle, format!("New card title in '{name}'"), "");
    }

    fn start_edit_title(&mut self) {
        let Some((_, card_id)) = self.require_card() else { return };
        let current = self.current_card().map(|c| c.title.clone()).unwrap_or_default();
        self.open_prompt(PromptKind::EditTitle, format!("Title of {card_id}"), &current);
    }

    fn start_append_body(&mut self) {
        let Some((_, card_id)) = self.require_card() else { return };
        self.open_prompt(PromptKind::AppendBody, format!("Append a line to {card_id}.md"), "");
    }

    fn start_body_editor(&mut self) {
        let Some((column_id, card_id)) = self.require_card() else { return };
        let body = self.current_card().map(|c| c.body.clone()).unwrap_or_default();
        self.mode = Mode::BodyEditor {
            column_id,
            card_id,
            input: TextInput::with_text(body),
        };
    }

    fn start_delete_card(&mut self) {
        let Some((column_id, card_id)) = self.require_card() else { return };
        let title = self.current_card().map(|c| c.display_title().to_string()).unwrap_or_default();
        self.mode = Mode::Confirm {
            kind: ConfirmKind::DeleteCard { column_id, card_id: card_id.clone() },
            question: format!("Delete {card_id}.md ('{title}')?"),
        };
    }

    fn start_move_card(&mut self) {
        let Some(_) = self.require_card() else { return };
        if self.board.columns.len() < 2 {
            self.set_error("There is only one column - press c to create another");
            return;
        }
        self.open_prompt(PromptKind::MoveCard, "Move to column (display name)", "");
    }

    /// Resolve a display name and move the selected card there.
    fn perform_move(&mut self, target_name: &str, new_id: Option<&str>) {
        let Some(index) = self.board.column_by_display_name(target_name) else {
            let names: Vec<String> =
                self.board.columns.iter().map(|c| c.display_name.clone()).collect();
            self.set_error(format!(
                "No column named '{}'. Columns: {}",
                target_name.trim(),
                names.join(", ")
            ));
            return;
        };
        let to_column = self.board.columns[index].id.clone();
        self.perform_move_by_id(&to_column, new_id);
    }

    fn perform_move_by_id(&mut self, to_column: &str, new_id: Option<&str>) {
        let Some((from_column, card_id)) = self.require_card() else { return };
        if from_column == to_column && new_id.is_none() {
            self.set_info("Card is already in that column");
            return;
        }
        let target_name = self
            .board
            .column_index(to_column)
            .map(|i| self.board.columns[i].display_name.clone())
            .unwrap_or_else(|| to_column.to_string());
        let source_name = self
            .board
            .column_index(&from_column)
            .map(|i| self.board.columns[i].display_name.clone())
            .unwrap_or_else(|| from_column.clone());

        match self.store.move_card(&from_column, &card_id, to_column, None, new_id) {
            Ok(final_id) => {
                self.set_success(format!("Moved {final_id} from '{source_name}' to '{target_name}'"));
                self.reload();
                self.select_card_anywhere(&final_id);
            }
            Err(e) => {
                // An id collision is recoverable: offer a new id rather than dead-ending.
                if e.contains("already exists in the target column") {
                    let suggested = self.store.suggest_card_id(to_column, &card_id);
                    self.set_error(e);
                    self.open_prompt(
                        PromptKind::MoveCardNewId { to_column: to_column.to_string() },
                        format!("New id for the card in '{target_name}'"),
                        &suggested,
                    );
                } else {
                    self.set_error(e);
                }
            }
        }
    }

    /// Move the selected card one column left or right.
    fn move_card_to_adjacent(&mut self, delta: isize) {
        if self.require_card().is_none() {
            return;
        }
        let count = self.board.columns.len();
        if count < 2 {
            self.set_error("There is only one column - press c to create another");
            return;
        }
        let target = (self.selected_column as isize + delta).rem_euclid(count as isize) as usize;
        let to_column = self.board.columns[target].id.clone();
        self.perform_move_by_id(&to_column, None);
    }

    fn reorder(&mut self, delta: isize) {
        let Some((column_id, card_id)) = self.require_card() else { return };
        if !self.search.is_empty() {
            self.set_info("Clear the search (Esc) before reordering, so positions are unambiguous");
            return;
        }
        match self.store.reorder_card(&column_id, &card_id, delta) {
            Ok(()) => {
                self.set_success(format!("Moved {card_id} {}", if delta < 0 { "up" } else { "down" }));
                self.reload();
                self.select_card_by_id(&card_id);
            }
            Err(e) => self.set_error(e),
        }
    }

    fn start_new_column(&mut self) {
        self.open_prompt(PromptKind::NewColumnName, "New column display name", "");
    }

    fn start_rename_column(&mut self) {
        let Some(name) = self.current_column_name() else {
            self.set_error("There are no columns yet - press c to create one");
            return;
        };
        self.open_prompt(PromptKind::RenameColumn, "New display name", &name);
    }

    fn start_goto_column(&mut self) {
        if self.board.columns.is_empty() {
            self.set_error("There are no columns yet - press c to create one");
            return;
        }
        self.open_prompt(PromptKind::GotoColumn, "Go to column (display name, Tab completes)", "");
    }

    fn start_search(&mut self) {
        let current = self.search.clone();
        self.open_prompt(PromptKind::Search, "Search cards (id, title or body)", &current);
    }

    fn start_set_board_root(&mut self) {
        let current = self.store.root.display().to_string();
        self.open_prompt(PromptKind::SetBoardRoot, "Board root directory", &current);
    }

    fn start_config_key(&mut self) {
        self.open_prompt(PromptKind::ConfigKey, "Configuration key to set", "");
    }

    fn initialise_board(&mut self) {
        match self.store.init_board() {
            Ok(()) => {
                self.set_success(format!(
                    "Initialised {} with columns TO DO, DOING, DONE",
                    self.store.root.display()
                ));
                self.reload();
            }
            Err(e) => self.set_error(e),
        }
    }

    fn normalise_order(&mut self) {
        let Some(column_id) = self.current_column_id() else {
            self.set_error("There are no columns yet - press c to create one");
            return;
        };
        let result = self.store.normalise_order(&column_id);
        self.report(result, format!("Rewrote cols/{column_id}/order.txt to match the display"));
    }
}

/// Turn load warnings into a status message.
fn board_warning_message(board: &Board) -> Option<Message> {
    if board.warnings.is_empty() {
        return None;
    }
    let first = board.warnings[0].clone();
    let text = if board.warnings.len() > 1 {
        format!("{first} (+{} more)", board.warnings.len() - 1)
    } else {
        first
    };
    Some(Message { kind: MessageKind::Error, text })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;

    /// An app on a temporary board with three columns and a few cards.
    fn app_with_board() -> (tempfile::TempDir, App) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        store.init_board().unwrap();
        store.create_card("todo", "item-1", "Fix login bug").unwrap();
        store.append_card_line("todo", "item-1", "Investigate timeout on mobile.").unwrap();
        store.create_card("todo", "item-2", "Write docs").unwrap();
        store.create_card("doing", "item-3", "Refactor parser").unwrap();

        let config = Config::load(dir.path().join("config.conf")).unwrap();
        let app = App::new(store, config, RootSource::CommandLine);
        (dir, app)
    }

    #[test]
    fn loads_the_board_on_startup() {
        let (_dir, app) = app_with_board();
        assert_eq!(app.board.columns.len(), 3);
        assert_eq!(app.board.total_cards(), 3);
        assert_eq!(app.current_card().unwrap().id, "item-1");
        assert_eq!(app.current_card().unwrap().title, "Fix login bug");
    }

    #[test]
    fn empty_root_prompts_the_user_to_initialise() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let config = Config::load(dir.path().join("c.conf")).unwrap();
        let app = App::new(store, config, RootSource::Default);
        let message = app.message.unwrap();
        assert!(message.text.contains("No board.txt"), "got: {}", message.text);
        assert!(message.text.contains("press I"), "must say how to fix it");
    }

    #[test]
    fn init_action_creates_the_board() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let config = Config::load(dir.path().join("c.conf")).unwrap();
        let mut app = App::new(store, config, RootSource::Default);
        app.dispatch(Action::InitBoard);
        assert_eq!(app.board.columns.len(), 3);
        assert!(dir.path().join("board.txt").is_file());
    }

    #[test]
    fn column_navigation_wraps_and_resets_card_selection() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::CardDown);
        assert_eq!(app.selected_card, 1);
        app.dispatch(Action::ColumnRight);
        assert_eq!(app.selected_column, 1);
        assert_eq!(app.selected_card, 0, "new column starts at its first card");
        app.dispatch(Action::ColumnLeft);
        assert_eq!(app.selected_column, 0);
        // Wrapping backwards from the first column reaches the last.
        app.dispatch(Action::ColumnLeft);
        assert_eq!(app.selected_column, 2);
    }

    #[test]
    fn card_navigation_clamps_at_the_ends() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::CardUp);
        assert_eq!(app.selected_card, 0, "already at the top");
        app.dispatch(Action::CardDown);
        app.dispatch(Action::CardDown);
        assert_eq!(app.selected_card, 1, "only two cards in this column");
        app.dispatch(Action::CardFirst);
        assert_eq!(app.selected_card, 0);
        app.dispatch(Action::CardLast);
        assert_eq!(app.selected_card, 1);
    }

    #[test]
    fn selecting_a_card_resets_body_scroll() {
        let (_dir, mut app) = app_with_board();
        app.detail_scroll = 5;
        app.dispatch(Action::CardDown);
        assert_eq!(app.detail_scroll, 0, "a new body must be read from its top");
    }

    #[test]
    fn detail_focus_redirects_vertical_keys_to_scrolling() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::FocusDetail);
        assert_eq!(app.focus, Focus::Detail);
        app.dispatch(Action::CardDown);
        assert_eq!(app.selected_card, 0, "selection must not move while scrolling");
        assert_eq!(app.detail_scroll, 1);
        app.dispatch(Action::CardUp);
        assert_eq!(app.detail_scroll, 0);
        app.dispatch(Action::CardUp);
        assert_eq!(app.detail_scroll, 0, "scroll must not go negative");
        // Esc returns focus to the board.
        app.dispatch(Action::ClearSearch);
        assert_eq!(app.focus, Focus::Board);
    }

    /// Drive a prompt to completion the way a user would.
    fn type_prompt(app: &mut App, text: &str) {
        for ch in text.chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
    }

    #[test]
    fn creating_a_card_writes_the_specified_file_format() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::NewCard);
        assert!(matches!(app.mode, Mode::Prompt { .. }));
        type_prompt(&mut app, "Deploy to staging");
        // Second step: the id, prefilled with a slug.
        match &app.mode {
            Mode::Prompt { kind, input, .. } => {
                assert!(matches!(kind, PromptKind::NewCardId { .. }));
                assert_eq!(input.text(), "deploy-to-staging", "id prefilled from the title");
            }
            _ => panic!("expected the id prompt, got {:?}", app.mode.kind()),
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        let path = app.store.root.join("cols/todo/deploy-to-staging.md");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# Deploy to staging\n");
        assert_eq!(app.current_card().unwrap().id, "deploy-to-staging", "selection follows the new card");
        assert!(matches!(app.message, Some(Message { kind: MessageKind::Success, .. })));
    }

    #[test]
    fn command_letters_are_literal_text_inside_a_prompt() {
        let (_dir, mut app) = app_with_board();
        let before = app.board.total_cards();
        app.dispatch(Action::NewCard);
        // "dnq" would delete, create and quit in normal mode.
        for ch in "dnq".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        match &app.mode {
            Mode::Prompt { input, .. } => assert_eq!(input.text(), "dnq"),
            _ => panic!("prompt closed unexpectedly"),
        }
        assert!(!app.should_quit, "q must not quit while typing");
        assert_eq!(app.board.total_cards(), before);
    }

    #[test]
    fn escape_cancels_a_prompt_without_touching_the_board() {
        let (_dir, mut app) = app_with_board();
        let before = app.board.total_cards();
        app.dispatch(Action::NewCard);
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(matches!(app.mode, Mode::Normal));
        assert_eq!(app.board.total_cards(), before);
    }

    #[test]
    fn cancelling_the_id_step_creates_nothing() {
        let (_dir, mut app) = app_with_board();
        let before = app.board.total_cards();
        app.dispatch(Action::NewCard);
        type_prompt(&mut app, "Half finished");
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(app.board.total_cards(), before, "no partial card on disk");
    }

    #[test]
    fn empty_card_title_is_rejected() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::NewCard);
        type_prompt(&mut app, "   ");
        assert!(matches!(app.mode, Mode::Normal));
        assert!(matches!(app.message, Some(Message { kind: MessageKind::Error, .. })));
    }

    #[test]
    fn editing_a_title_keeps_the_body() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::EditTitle);
        match &app.mode {
            Mode::Prompt { input, .. } => assert_eq!(input.text(), "Fix login bug", "prefilled"),
            _ => panic!("expected a prompt"),
        }
        // Clear and retype.
        app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        type_prompt(&mut app, "Fix logout bug");

        let path = app.store.root.join("cols/todo/item-1.md");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# Fix logout bug\nInvestigate timeout on mobile.\n"
        );
        assert_eq!(app.current_card().unwrap().title, "Fix logout bug");
    }

    #[test]
    fn appending_a_line_grows_the_body() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::AppendBody);
        type_prompt(&mut app, "Also affects tablets.");
        let path = app.store.root.join("cols/todo/item-1.md");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# Fix login bug\nInvestigate timeout on mobile.\nAlso affects tablets.\n"
        );
        assert_eq!(app.current_card().unwrap().body_lines().len(), 2);
    }

    #[test]
    fn body_editor_saves_multiline_text_on_ctrl_s() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::EditBody);
        assert!(matches!(app.mode, Mode::BodyEditor { .. }));
        app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        for ch in "line one".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        // Enter inserts a newline rather than submitting.
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(matches!(app.mode, Mode::BodyEditor { .. }), "Enter must not save");
        for ch in "line two".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        app.handle_key(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL));

        let path = app.store.root.join("cols/todo/item-1.md");
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# Fix login bug\nline one\nline two\n"
        );
    }

    #[test]
    fn body_editor_discards_on_escape() {
        let (_dir, mut app) = app_with_board();
        let before = std::fs::read_to_string(app.store.root.join("cols/todo/item-1.md")).unwrap();
        app.dispatch(Action::EditBody);
        for ch in "junk".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        app.handle_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert_eq!(
            std::fs::read_to_string(app.store.root.join("cols/todo/item-1.md")).unwrap(),
            before
        );
    }

    #[test]
    fn deleting_requires_confirmation() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::DeleteCard);
        match &app.mode {
            Mode::Confirm { question, .. } => assert!(question.contains("item-1.md")),
            _ => panic!("expected a confirmation"),
        }
        // Declining leaves the file alone.
        app.handle_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(app.store.root.join("cols/todo/item-1.md").exists());

        app.dispatch(Action::DeleteCard);
        app.handle_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert!(!app.store.root.join("cols/todo/item-1.md").exists());
        assert_eq!(std::fs::read_to_string(app.store.root.join("cols/todo/order.txt")).unwrap(), "item-2\n");
    }

    #[test]
    fn moving_a_card_by_display_name_relocates_the_file() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::MoveCardPrompt);
        type_prompt(&mut app, "DONE");

        assert!(!app.store.root.join("cols/todo/item-1.md").exists());
        assert!(app.store.root.join("cols/done/item-1.md").exists());
        assert_eq!(app.selected_column, 2, "selection follows the card");
        assert_eq!(app.current_card().unwrap().id, "item-1");
    }

    #[test]
    fn moving_to_an_unknown_column_lists_the_valid_names() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::MoveCardPrompt);
        type_prompt(&mut app, "NOPE");
        let text = app.message.as_ref().unwrap().text.clone();
        assert!(text.contains("No column named"), "got: {text}");
        assert!(text.contains("TO DO") && text.contains("DONE"), "must list the options: {text}");
        assert!(app.store.root.join("cols/todo/item-1.md").exists(), "nothing moved");
    }

    #[test]
    fn shortcut_moves_card_to_the_next_column() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::MoveCardRight);
        assert!(app.store.root.join("cols/doing/item-1.md").exists());
        let text = app.message.as_ref().unwrap().text.clone();
        assert!(text.contains("TO DO") && text.contains("DOING"), "feedback names both: {text}");
        // And back again.
        app.dispatch(Action::MoveCardLeft);
        assert!(app.store.root.join("cols/todo/item-1.md").exists());
    }

    #[test]
    fn move_collision_opens_a_rename_prompt_and_can_be_completed() {
        let (_dir, mut app) = app_with_board();
        // Same id in two columns.
        app.store.create_card("done", "item-1", "Other").unwrap();
        app.reload();
        app.dispatch(Action::MoveCardRight); // todo -> doing, no clash
        app.dispatch(Action::MoveCardRight); // doing -> done, clashes
        match &app.mode {
            Mode::Prompt { kind, input, .. } => {
                assert!(matches!(kind, PromptKind::MoveCardNewId { .. }));
                assert_eq!(input.text(), "item-1-2", "suggests a free id");
            }
            _ => panic!("expected a rename prompt, got {:?}", app.mode.kind()),
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.store.root.join("cols/done/item-1-2.md").exists());
        assert!(app.store.root.join("cols/done/item-1.md").exists(), "the original is untouched");
    }

    #[test]
    fn reordering_rewrites_order_txt() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::CardDown);
        app.dispatch(Action::ReorderUp);
        assert_eq!(
            std::fs::read_to_string(app.store.root.join("cols/todo/order.txt")).unwrap(),
            "item-2\nitem-1\n"
        );
        assert_eq!(app.current_card().unwrap().id, "item-2", "selection follows the card");
    }

    #[test]
    fn reordering_is_refused_while_a_search_filters_the_list() {
        let (_dir, mut app) = app_with_board();
        app.search = "item".to_string();
        app.dispatch(Action::ReorderDown);
        // Positions would be ambiguous, so nothing is written.
        assert_eq!(
            std::fs::read_to_string(app.store.root.join("cols/todo/order.txt")).unwrap(),
            "item-1\nitem-2\n"
        );
    }

    #[test]
    fn creating_a_column_writes_board_txt_and_makes_the_directory() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::NewColumn);
        type_prompt(&mut app, "In Review");
        match &app.mode {
            Mode::Prompt { input, .. } => assert_eq!(input.text(), "in-review", "id from the name"),
            _ => panic!("expected the id prompt"),
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));

        let board_txt = std::fs::read_to_string(app.store.root.join("board.txt")).unwrap();
        assert!(board_txt.ends_with("col in-review \"In Review\"\n"), "got: {board_txt}");
        assert!(app.store.root.join("cols/in-review").is_dir());
        assert_eq!(app.board.columns.len(), 4);
        assert_eq!(app.selected_column, 3, "selection moves to the new column");
    }

    #[test]
    fn columns_with_non_ascii_names_get_a_usable_id() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::NewColumn);
        type_prompt(&mut app, "English-only text");
        match &app.mode {
            Mode::Prompt { input, .. } => assert_eq!(input.text(), "col-1", "fallback id"),
            _ => panic!("expected the id prompt"),
        }
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.store.root.join("cols/col-1").is_dir());
        assert_eq!(app.board.columns.last().unwrap().display_name, "English-only text");
    }

    #[test]
    fn renaming_a_column_keeps_its_id_and_files() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::RenameColumn);
        app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        type_prompt(&mut app, "BACKLOG");
        assert_eq!(app.board.columns[0].display_name, "BACKLOG");
        assert_eq!(app.board.columns[0].id, "todo");
        assert!(app.store.root.join("cols/todo/item-1.md").exists());
    }

    #[test]
    fn goto_column_jumps_by_display_name() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::GotoColumn);
        type_prompt(&mut app, "DONE");
        assert_eq!(app.selected_column, 2);
    }

    #[test]
    fn tab_completes_a_unique_column_name() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::GotoColumn);
        for ch in "DOI".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        match &app.mode {
            Mode::Prompt { input, .. } => assert_eq!(input.text(), "DOING"),
            _ => panic!("expected a prompt"),
        }
    }

    #[test]
    fn tab_with_several_matches_lists_them_instead_of_guessing() {
        let (_dir, mut app) = app_with_board();
        app.store.create_column("doing-2", "DOING LATER").unwrap();
        app.reload();
        app.dispatch(Action::GotoColumn);
        for ch in "DOING".chars() {
            app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE));
        }
        app.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE));
        let text = app.message.as_ref().unwrap().text.clone();
        assert!(text.contains("DOING") && text.contains("DOING LATER"), "got: {text}");
    }

    #[test]
    fn search_filters_cards_across_columns() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::Search);
        type_prompt(&mut app, "refactor");
        assert_eq!(app.filtered_indices(0).len(), 0, "no match in TO DO");
        assert_eq!(app.filtered_indices(1).len(), 1, "matches in DOING");
    }

    #[test]
    fn search_matches_body_text_too() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::Search);
        type_prompt(&mut app, "mobile");
        // "mobile" only appears in item-1's body.
        assert_eq!(app.filtered_indices(0).len(), 1);
    }

    #[test]
    fn search_is_case_insensitive_and_clearable() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::Search);
        type_prompt(&mut app, "FIX LOGIN");
        assert_eq!(app.filtered_indices(0).len(), 1);
        app.dispatch(Action::ClearSearch);
        assert!(app.search.is_empty());
        assert_eq!(app.filtered_indices(0).len(), 2);
    }

    #[test]
    fn search_next_walks_matches_across_columns() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::Search);
        type_prompt(&mut app, "item");
        app.dispatch(Action::SearchNext);
        assert_eq!(app.selected_card, 1);
        // Past the end of TO DO, on to the next matching column.
        app.dispatch(Action::SearchNext);
        assert_eq!(app.selected_column, 1);
        assert_eq!(app.current_card().unwrap().id, "item-3");
    }

    #[test]
    fn selection_survives_a_filter_that_hides_the_current_card() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::CardDown);
        assert_eq!(app.current_card().unwrap().id, "item-2");
        app.dispatch(Action::Search);
        type_prompt(&mut app, "Fix login");
        // item-2 is filtered out; the selection must land on a visible card, not dangle.
        assert_eq!(app.selected_card, 0);
        assert_eq!(app.current_card().unwrap().id, "item-1");
    }

    #[test]
    fn operations_on_an_empty_column_report_instead_of_failing() {
        let (_dir, mut app) = app_with_board();
        app.selected_column = 2; // DONE is empty
        for action in [Action::EditTitle, Action::AppendBody, Action::DeleteCard, Action::EditBody, Action::MoveCardPrompt] {
            app.dispatch(action);
            assert!(matches!(app.mode, Mode::Normal), "{action:?} should not open a prompt");
            let message = app.message.as_ref().expect("an explanation is required");
            assert_eq!(message.kind, MessageKind::Error, "{action:?}");
        }
    }

    #[test]
    fn operations_on_a_board_with_no_columns_are_safe() {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::new(dir.path());
        let config = Config::load(dir.path().join("c.conf")).unwrap();
        let mut app = App::new(store, config, RootSource::Default);
        // Nothing here may panic on an empty board.
        for action in [
            Action::NewCard, Action::EditTitle, Action::CardDown, Action::ColumnRight,
            Action::MoveCardRight, Action::ReorderUp, Action::RenameColumn, Action::GotoColumn,
            Action::NormaliseOrder, Action::FocusDetail, Action::CardLast, Action::CardPageDown,
        ] {
            app.dispatch(action);
        }
        assert!(app.current_card().is_none());
        assert!(!app.should_quit);
    }

    #[test]
    fn changing_the_board_root_persists_and_reloads() {
        let (dir, mut app) = app_with_board();
        let other = dir.path().join("other-board");
        let other_store = Store::new(&other);
        other_store.init_board().unwrap();
        other_store.create_card("todo", "only-card", "Elsewhere").unwrap();

        app.dispatch(Action::SetBoardRoot);
        app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        type_prompt(&mut app, other.to_str().unwrap());

        assert_eq!(app.store.root, other);
        assert_eq!(app.current_card().unwrap().id, "only-card", "board reloaded from the new root");
        // And it was written to the config file, so the next launch uses it.
        let reread = Config::load(app.config.path.clone()).unwrap();
        assert_eq!(reread.board_root(), Some(other));
    }

    #[test]
    fn arbitrary_configuration_keys_can_be_set() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::EditConfig);
        type_prompt(&mut app, "my_setting");
        type_prompt(&mut app, "my value");
        assert_eq!(app.config.get("my_setting"), Some("my value"));
        let reread = Config::load(app.config.path.clone()).unwrap();
        assert_eq!(reread.get("my_setting"), Some("my value"));
    }

    #[test]
    fn setting_board_root_through_the_config_editor_takes_effect() {
        let (dir, mut app) = app_with_board();
        let other = dir.path().join("second");
        Store::new(&other).init_board().unwrap();

        app.dispatch(Action::EditConfig);
        type_prompt(&mut app, "board_root");
        app.handle_key(KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL));
        type_prompt(&mut app, other.to_str().unwrap());
        assert_eq!(app.store.root, other);
    }

    #[test]
    fn help_toggles_without_disturbing_the_board() {
        let (_dir, mut app) = app_with_board();
        assert!(!app.show_help);
        app.dispatch(Action::ToggleHelp);
        assert!(app.show_help);
        assert_eq!(app.current_card().unwrap().id, "item-1", "selection is untouched");
        app.dispatch(Action::ToggleHelp);
        assert!(!app.show_help);
    }

    #[test]
    fn quit_is_reachable_and_ctrl_c_also_quits() {
        let (_dir, mut app) = app_with_board();
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.should_quit);

        let (_dir2, mut app2) = app_with_board();
        app2.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app2.should_quit);
    }

    #[test]
    fn key_releases_are_ignored_so_actions_do_not_fire_twice() {
        let (_dir, mut app) = app_with_board();
        // The loop filters on KeyEventKind::Press; assert the loop's own guard rather than
        // handle_key, which is only reached for presses.
        let release = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(release.kind, KeyEventKind::Press, "constructor default");
        app.handle_key(release);
        assert!(app.should_quit);
    }

    #[test]
    fn reload_picks_up_changes_made_outside_the_application() {
        let (_dir, mut app) = app_with_board();
        // Simulate another process (or vim) editing the board.
        app.store.create_card("todo", "external", "Added elsewhere").unwrap();
        app.dispatch(Action::Reload);
        assert_eq!(app.board.columns[0].cards.len(), 3);
    }

    #[test]
    fn reload_keeps_the_selection_on_the_same_card() {
        let (_dir, mut app) = app_with_board();
        app.dispatch(Action::CardDown);
        let selected = app.current_card().unwrap().id.clone();
        app.dispatch(Action::Reload);
        assert_eq!(app.current_card().unwrap().id, selected);
    }

    #[test]
    fn normalise_order_writes_the_displayed_order() {
        let (_dir, mut app) = app_with_board();
        let order = app.store.root.join("cols/todo/order.txt");
        std::fs::write(&order, "phantom\nitem-2\n").unwrap();
        app.reload();
        app.dispatch(Action::NormaliseOrder);
        assert_eq!(std::fs::read_to_string(&order).unwrap(), "item-2\nitem-1\n");
    }

    #[test]
    fn load_warnings_are_surfaced_to_the_user() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path()).unwrap();
        std::fs::write(dir.path().join("board.txt"), "col a \"A\"\ncol a \"Dup\"\n").unwrap();
        let store = Store::new(dir.path());
        let config = Config::load(dir.path().join("c.conf")).unwrap();
        let app = App::new(store, config, RootSource::Default);
        let message = app.message.expect("warnings must be shown");
        assert_eq!(message.kind, MessageKind::Error);
        assert!(message.text.contains("more than once"), "got: {}", message.text);
    }
}
