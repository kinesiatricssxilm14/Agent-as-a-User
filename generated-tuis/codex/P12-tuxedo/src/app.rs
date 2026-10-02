use crate::{
    model::{parse_tags, valid_due_date, Task},
    storage, ui,
};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{backend::Backend, Terminal};
use std::{path::PathBuf, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Search,
    ProjectFilter,
    ContextFilter,
    Add,
    EditTags,
    Priority,
    Help,
    ConfirmDelete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AddField {
    Description,
    Priority,
    Projects,
    Contexts,
    Due,
}

impl AddField {
    pub fn next(self) -> Self {
        match self {
            Self::Description => Self::Priority,
            Self::Priority => Self::Projects,
            Self::Projects => Self::Contexts,
            Self::Contexts => Self::Due,
            Self::Due => Self::Description,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::Description => Self::Due,
            Self::Priority => Self::Description,
            Self::Projects => Self::Priority,
            Self::Contexts => Self::Projects,
            Self::Due => Self::Contexts,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Self::Description => 0,
            Self::Priority => 1,
            Self::Projects => 2,
            Self::Contexts => 3,
            Self::Due => 4,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct AddForm {
    pub description: String,
    pub priority: String,
    pub projects: String,
    pub contexts: String,
    pub due: String,
}

impl AddForm {
    fn value_mut(&mut self, field: AddField) -> &mut String {
        match field {
            AddField::Description => &mut self.description,
            AddField::Priority => &mut self.priority,
            AddField::Projects => &mut self.projects,
            AddField::Contexts => &mut self.contexts,
            AddField::Due => &mut self.due,
        }
    }
}

pub struct App {
    pub path: PathBuf,
    pub tasks: Vec<Task>,
    pub selected: usize,
    pub mode: Mode,
    pub search: String,
    pub project_filter: String,
    pub context_filter: String,
    pub input: String,
    pub add_form: AddForm,
    pub add_field: AddField,
    pub message: String,
    pub should_quit: bool,
}

impl App {
    pub fn new(path: PathBuf) -> Result<Self> {
        let tasks = storage::load(&path)?;
        let message = format!("Loaded {} task(s) from {}", tasks.len(), path.display());
        Ok(Self {
            path,
            tasks,
            selected: 0,
            mode: Mode::Normal,
            search: String::new(),
            project_filter: String::new(),
            context_filter: String::new(),
            input: String::new(),
            add_form: AddForm::default(),
            add_field: AddField::Description,
            message,
            should_quit: false,
        })
    }

    pub fn run<B: Backend>(&mut self, terminal: &mut Terminal<B>) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| ui::draw(frame, self))?;
            if event::poll(Duration::from_millis(200))? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key)?;
                }
            }
        }
        Ok(())
    }

    pub fn visible_indices(&self) -> Vec<usize> {
        let search = if self.mode == Mode::Search {
            &self.input
        } else {
            &self.search
        };
        let project = if self.mode == Mode::ProjectFilter {
            &self.input
        } else {
            &self.project_filter
        };
        let context = if self.mode == Mode::ContextFilter {
            &self.input
        } else {
            &self.context_filter
        };
        self.tasks
            .iter()
            .enumerate()
            .filter(|(_, task)| {
                task.matches(
                    search.trim(),
                    project.trim().trim_start_matches('+'),
                    context.trim().trim_start_matches('@'),
                )
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub fn selected_task_index(&self) -> Option<usize> {
        self.visible_indices().get(self.selected).copied()
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return Ok(());
        }

        match self.mode {
            Mode::Normal => self.handle_normal(key)?,
            Mode::Search | Mode::ProjectFilter | Mode::ContextFilter => {
                self.handle_filter_input(key)
            }
            Mode::Add => self.handle_add(key)?,
            Mode::EditTags => self.handle_tag_edit(key)?,
            Mode::Priority => self.handle_priority(key)?,
            Mode::Help => {
                if matches!(
                    key.code,
                    KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
                ) {
                    self.mode = Mode::Normal;
                }
            }
            Mode::ConfirmDelete => self.handle_delete_confirmation(key)?,
        }
        self.clamp_selection();
        Ok(())
    }

    fn handle_normal(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Home | KeyCode::Char('g') => self.selected = 0,
            KeyCode::End | KeyCode::Char('G') => {
                self.selected = self.visible_indices().len().saturating_sub(1)
            }
            KeyCode::Char('a') => {
                self.add_form = AddForm::default();
                self.add_field = AddField::Description;
                self.mode = Mode::Add;
            }
            KeyCode::Char('p') => {
                if self.selected_task_index().is_some() {
                    self.mode = Mode::Priority;
                }
            }
            KeyCode::Char('t') => {
                if let Some(index) = self.selected_task_index() {
                    let task = &self.tasks[index];
                    self.input = format!(
                        "{} | {}",
                        task.projects.join(", "),
                        task.contexts.join(", ")
                    );
                    self.mode = Mode::EditTags;
                }
            }
            KeyCode::Char('x') | KeyCode::Enter => self.toggle_complete()?,
            KeyCode::Char('d') => {
                if self.selected_task_index().is_some() {
                    self.mode = Mode::ConfirmDelete;
                }
            }
            KeyCode::Char('/') => {
                self.input = self.search.clone();
                self.mode = Mode::Search;
            }
            KeyCode::Char('P') => {
                self.input = self.project_filter.clone();
                self.mode = Mode::ProjectFilter;
            }
            KeyCode::Char('C') => {
                self.input = self.context_filter.clone();
                self.mode = Mode::ContextFilter;
            }
            KeyCode::Char('c') => {
                self.search.clear();
                self.project_filter.clear();
                self.context_filter.clear();
                self.selected = 0;
                self.message = "All filters cleared".to_string();
            }
            KeyCode::Char('r') => {
                self.tasks = storage::load(&self.path)?;
                self.message = format!("Reloaded {} task(s) from disk", self.tasks.len());
            }
            KeyCode::Char('?') => self.mode = Mode::Help,
            _ => {}
        }
        Ok(())
    }

    fn handle_filter_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.input.clear();
            }
            KeyCode::Enter => {
                let value = self.input.trim().to_string();
                match self.mode {
                    Mode::Search => self.search = value,
                    Mode::ProjectFilter => {
                        self.project_filter = value.trim_start_matches('+').to_string()
                    }
                    Mode::ContextFilter => {
                        self.context_filter = value.trim_start_matches('@').to_string()
                    }
                    _ => {}
                }
                self.selected = 0;
                self.message = "Filter updated".to_string();
                self.mode = Mode::Normal;
                self.input.clear();
            }
            KeyCode::Backspace => {
                self.input.pop();
                self.selected = 0;
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.push(character);
                self.selected = 0;
            }
            _ => {}
        }
    }

    fn handle_add(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.message = "Add cancelled".to_string();
            }
            KeyCode::Tab | KeyCode::Down => self.add_field = self.add_field.next(),
            KeyCode::BackTab | KeyCode::Up => self.add_field = self.add_field.previous(),
            KeyCode::Enter => {
                if key.modifiers.contains(KeyModifiers::CONTROL) || self.add_field == AddField::Due
                {
                    self.submit_add()?;
                } else {
                    self.add_field = self.add_field.next();
                }
            }
            KeyCode::Backspace => {
                self.add_form.value_mut(self.add_field).pop();
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.add_field == AddField::Priority {
                    if character.is_ascii_alphabetic() {
                        let value = self.add_form.value_mut(self.add_field);
                        value.clear();
                        value.push(character.to_ascii_uppercase());
                    }
                } else {
                    self.add_form.value_mut(self.add_field).push(character);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn submit_add(&mut self) -> Result<()> {
        let description = self.add_form.description.trim().to_string();
        if description.is_empty() {
            self.message = "Description is required".to_string();
            self.add_field = AddField::Description;
            return Ok(());
        }
        let due = self.add_form.due.trim();
        if !valid_due_date(due) {
            self.message = "Due date must be YYYY-MM-DD".to_string();
            self.add_field = AddField::Due;
            return Ok(());
        }
        let priority = self
            .add_form
            .priority
            .chars()
            .next()
            .map(|c| c.to_ascii_uppercase())
            .filter(|c| c.is_ascii_uppercase());
        self.tasks.push(Task::new(
            description,
            priority,
            parse_tags(&self.add_form.projects, '+'),
            parse_tags(&self.add_form.contexts, '@'),
            (!due.is_empty()).then(|| due.to_string()),
        ));
        self.persist("Task added")?;
        self.search.clear();
        self.project_filter.clear();
        self.context_filter.clear();
        self.selected = self.tasks.len().saturating_sub(1);
        self.mode = Mode::Normal;
        Ok(())
    }

    fn handle_priority(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => self.mode = Mode::Normal,
            KeyCode::Char('-') | KeyCode::Delete | KeyCode::Backspace => {
                if let Some(index) = self.selected_task_index() {
                    self.tasks[index].set_priority(None);
                    self.persist("Priority removed")?;
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Char(value) if value.is_ascii_alphabetic() => {
                if let Some(index) = self.selected_task_index() {
                    let priority = value.to_ascii_uppercase();
                    self.tasks[index].set_priority(Some(priority));
                    self.persist(&format!("Priority set to ({priority})"))?;
                }
                self.mode = Mode::Normal;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_tag_edit(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                self.mode = Mode::Normal;
                self.input.clear();
            }
            KeyCode::Enter => {
                let Some(index) = self.selected_task_index() else {
                    self.mode = Mode::Normal;
                    return Ok(());
                };
                let (projects, contexts) = self.input.split_once('|').unwrap_or((&self.input, ""));
                self.tasks[index].projects = parse_tags(projects, '+');
                self.tasks[index].contexts = parse_tags(contexts, '@');
                self.persist("Tags updated")?;
                self.mode = Mode::Normal;
                self.input.clear();
            }
            KeyCode::Backspace => {
                self.input.pop();
            }
            KeyCode::Char(character) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input.push(character)
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_delete_confirmation(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                if let Some(index) = self.selected_task_index() {
                    self.tasks.remove(index);
                    self.persist("Task deleted")?;
                }
                self.mode = Mode::Normal;
            }
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => self.mode = Mode::Normal,
            _ => {}
        }
        Ok(())
    }

    fn toggle_complete(&mut self) -> Result<()> {
        if let Some(index) = self.selected_task_index() {
            self.tasks[index].completed = !self.tasks[index].completed;
            let message = if self.tasks[index].completed {
                "Task completed"
            } else {
                "Task reopened"
            };
            self.persist(message)?;
        }
        Ok(())
    }

    fn persist(&mut self, message: &str) -> Result<()> {
        storage::save(&self.path, &self.tasks)?;
        self.message = format!("{message} · saved to {}", self.path.display());
        Ok(())
    }

    fn move_selection(&mut self, delta: isize) {
        let count = self.visible_indices().len();
        if count == 0 {
            self.selected = 0;
        } else {
            self.selected = (self.selected as isize + delta)
                .clamp(0, count.saturating_sub(1) as isize) as usize;
        }
    }

    fn clamp_selection(&mut self) {
        let count = self.visible_indices().len();
        self.selected = self.selected.min(count.saturating_sub(1));
    }
}
