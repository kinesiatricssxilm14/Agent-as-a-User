//! Application state and keyboard handling.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::input::LineBuffer;
use crate::store::Store;
use crate::task::Task;

#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub project: Option<String>,
    pub context: Option<String>,
    pub search: Option<String>,
}

impl Filter {
    pub fn is_active(&self) -> bool {
        self.project.is_some() || self.context.is_some() || self.search.is_some()
    }

    pub fn matches(&self, t: &Task) -> bool {
        if let Some(p) = &self.project {
            if !t.projects.iter().any(|x| x == p) {
                return false;
            }
        }
        if let Some(c) = &self.context {
            if !t.contexts.iter().any(|x| x == c) {
                return false;
            }
        }
        if let Some(s) = &self.search {
            let needle = s.to_lowercase();
            let in_desc = t.description.to_lowercase().contains(&needle);
            let in_proj = t.projects.iter().any(|p| p.to_lowercase().contains(&needle));
            let in_ctx = t.contexts.iter().any(|c| c.to_lowercase().contains(&needle));
            if !(in_desc || in_proj || in_ctx) {
                return false;
            }
        }
        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Normal,
    Input,
    Help,
    ConfirmDelete(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputKind {
    AddTask,
    EditTask(usize),
    Search,
    FilterProject,
    FilterContext,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub label: &'static str,
    pub value: LineBuffer,
}

#[derive(Debug, Clone)]
pub struct InputState {
    pub kind: InputKind,
    pub title: String,
    pub fields: Vec<Field>,
    pub active: usize,
    pub error: Option<String>,
}

impl InputState {
    pub fn active_field(&mut self) -> &mut LineBuffer {
        &mut self.fields[self.active].value
    }

    pub fn field(&self, i: usize) -> String {
        self.fields[i].value.as_string()
    }

    pub fn next_field(&mut self) {
        if !self.fields.is_empty() {
            self.active = (self.active + 1) % self.fields.len();
        }
        self.error = None;
    }

    pub fn prev_field(&mut self) {
        if !self.fields.is_empty() {
            self.active = (self.active + self.fields.len() - 1) % self.fields.len();
        }
        self.error = None;
    }
}

pub struct App {
    pub store: Store,
    pub tasks: Vec<Task>,
    pub filter: Filter,
    pub selected: usize,
    pub mode: Mode,
    pub input: Option<InputState>,
    pub status: Option<String>,
    pub quit: bool,
    pub list_height: u16,
    search_snapshot: Option<String>,
}

impl App {
    pub fn new(store: Store, tasks: Vec<Task>) -> Self {
        Self {
            store,
            tasks,
            filter: Filter::default(),
            selected: 0,
            mode: Mode::Normal,
            input: None,
            status: None,
            quit: false,
            list_height: 10,
            search_snapshot: None,
        }
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    /// Real task indices matching the current filter, in file order.
    pub fn visible_indices(&self) -> Vec<usize> {
        self.tasks
            .iter()
            .enumerate()
            .filter(|(_, t)| self.filter.matches(t))
            .map(|(i, _)| i)
            .collect()
    }

    /// Real index of the currently selected (visible) task.
    pub fn selected_index(&self) -> Option<usize> {
        self.visible_indices().get(self.selected).copied()
    }

    fn visible_position(&self, real: usize) -> Option<usize> {
        self.visible_indices().iter().position(|&i| i == real)
    }

    fn clamp_selection(&mut self) {
        let len = self.visible_indices().len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    fn page_size(&self) -> usize {
        self.list_height.saturating_sub(1).max(1) as usize
    }

    fn save_and_report(&mut self, msg: &str) {
        match self.store.save(&self.tasks) {
            Ok(()) => {
                self.status = Some(format!(
                    "{msg} · saved {} task(s) to {}",
                    self.tasks.len(),
                    self.store.path().display()
                ));
            }
            Err(e) => {
                self.status = Some(format!(
                    "error saving to {}: {e:#}",
                    self.store.path().display()
                ));
            }
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) {
        if key.kind == crossterm::event::KeyEventKind::Release {
            return;
        }
        match self.mode.clone() {
            Mode::Normal => self.handle_normal(key),
            Mode::Input => self.handle_input(key),
            Mode::Help => self.handle_help(key),
            Mode::ConfirmDelete(_) => self.handle_confirm_delete(key),
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) {
        self.status = None;
        match key.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => self.quit = true,
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Home | KeyCode::Char('g') => self.selected = 0,
            KeyCode::End | KeyCode::Char('G') => {
                let len = self.visible_indices().len();
                self.selected = len.saturating_sub(1);
            }
            KeyCode::PageUp => self.move_selection(-(self.page_size() as isize)),
            KeyCode::PageDown => self.move_selection(self.page_size() as isize),
            KeyCode::Char('a') => self.start_add(),
            KeyCode::Char('e') | KeyCode::Enter => self.start_edit(),
            KeyCode::Char(' ') => self.toggle_complete(),
            KeyCode::Char('x') => self.set_complete(true),
            KeyCode::Char('u') => self.set_complete(false),
            KeyCode::Char('p') => self.change_priority(true),
            KeyCode::Char('P') => self.change_priority(false),
            KeyCode::Char('d') => self.start_delete_confirm(),
            KeyCode::Char('/') => self.start_search(),
            KeyCode::Char('+') => self.start_filter_project(),
            KeyCode::Char('@') => self.start_filter_context(),
            KeyCode::Char('r') | KeyCode::Esc => self.clear_filters(),
            KeyCode::Char('?') | KeyCode::Char('h') => self.mode = Mode::Help,
            _ => {}
        }
    }

    fn move_selection(&mut self, delta: isize) {
        let len = self.visible_indices().len();
        if len == 0 {
            self.selected = 0;
            return;
        }
        let cur = self.selected as isize;
        self.selected = (cur + delta).clamp(0, len as isize - 1) as usize;
    }

    fn start_add(&mut self) {
        self.input = Some(InputState {
            kind: InputKind::AddTask,
            title: "Add Task".into(),
            fields: vec![
                Field { label: "Description", value: LineBuffer::new("") },
                Field { label: "Priority (A-Z)", value: LineBuffer::new("") },
                Field { label: "Projects (+tag)", value: LineBuffer::new("") },
                Field { label: "Contexts (@tag)", value: LineBuffer::new("") },
                Field { label: "Due (YYYY-MM-DD)", value: LineBuffer::new("") },
            ],
            active: 0,
            error: None,
        });
        self.mode = Mode::Input;
    }

    fn start_edit(&mut self) {
        let Some(i) = self.selected_index() else {
            return;
        };
        let t = &self.tasks[i];
        let priority = t.priority.map(|p| p.to_string()).unwrap_or_default();
        let projects = t
            .projects
            .iter()
            .map(|p| format!("+{p}"))
            .collect::<Vec<_>>()
            .join(" ");
        let contexts = t
            .contexts
            .iter()
            .map(|c| format!("@{c}"))
            .collect::<Vec<_>>()
            .join(" ");
        let due = t.due_date().unwrap_or_default().to_string();
        self.input = Some(InputState {
            kind: InputKind::EditTask(i),
            title: "Edit Task".into(),
            fields: vec![
                Field { label: "Description", value: LineBuffer::new(&t.description) },
                Field { label: "Priority (A-Z)", value: LineBuffer::new(&priority) },
                Field { label: "Projects (+tag)", value: LineBuffer::new(&projects) },
                Field { label: "Contexts (@tag)", value: LineBuffer::new(&contexts) },
                Field { label: "Due (YYYY-MM-DD)", value: LineBuffer::new(&due) },
            ],
            active: 0,
            error: None,
        });
        self.mode = Mode::Input;
    }

    fn toggle_complete(&mut self) {
        let Some(i) = self.selected_index() else {
            return;
        };
        let complete = !self.tasks[i].completed;
        self.apply_completion(i, complete);
    }

    fn set_complete(&mut self, complete: bool) {
        let Some(i) = self.selected_index() else {
            return;
        };
        self.apply_completion(i, complete);
    }

    fn apply_completion(&mut self, i: usize, complete: bool) {
        if self.tasks[i].completed == complete {
            self.status = Some(if complete {
                "Task already completed".into()
            } else {
                "Task already open".into()
            });
            return;
        }
        self.tasks[i].completed = complete;
        self.tasks[i].completion_date = if complete {
            Some(crate::task::today())
        } else {
            None
        };
        self.save_and_report(if complete { "Completed task" } else { "Reopened task" });
    }

    fn change_priority(&mut self, raise: bool) {
        let Some(i) = self.selected_index() else {
            return;
        };
        let t = &mut self.tasks[i];
        let old = t.priority;
        let new = match (raise, old) {
            (true, None) => Some('A'),
            (true, Some('A')) => Some('A'),
            (true, Some(c)) => Some((c as u8 - 1) as char),
            (false, None) => Some('Z'),
            (false, Some('Z')) => Some('Z'),
            (false, Some(c)) => Some((c as u8 + 1) as char),
        };
        if old == new {
            self.status = Some("Priority unchanged".into());
            return;
        }
        t.priority = new;
        let msg = match (old, new) {
            (None, Some(n)) => format!("Set priority to ({n})"),
            (Some(o), None) => format!("Removed priority ({o})"),
            (Some(o), Some(n)) => format!("Priority ({o}) → ({n})"),
            (None, None) => "Priority unchanged".into(),
        };
        self.save_and_report(&msg);
    }

    fn start_delete_confirm(&mut self) {
        if let Some(i) = self.selected_index() {
            self.mode = Mode::ConfirmDelete(i);
            self.status = None;
        }
    }

    fn handle_confirm_delete(&mut self, key: KeyEvent) {
        let i = match self.mode {
            Mode::ConfirmDelete(i) => i,
            _ => return,
        };
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                self.tasks.remove(i);
                self.mode = Mode::Normal;
                self.clamp_selection();
                self.save_and_report("Deleted task");
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Char('q') => {
                self.mode = Mode::Normal;
                self.status = Some("Delete cancelled".into());
            }
            _ => {}
        }
    }

    fn start_search(&mut self) {
        self.search_snapshot = self.filter.search.clone();
        let init = self.filter.search.clone().unwrap_or_default();
        self.input = Some(InputState {
            kind: InputKind::Search,
            title: "Search (live filter)".into(),
            fields: vec![Field { label: "Query", value: LineBuffer::new(&init) }],
            active: 0,
            error: None,
        });
        self.mode = Mode::Input;
    }

    fn start_filter_project(&mut self) {
        let prefill = self.filter.project.clone().unwrap_or_default();
        self.input = Some(InputState {
            kind: InputKind::FilterProject,
            title: "Filter by project (+tag)".into(),
            fields: vec![Field { label: "Project", value: LineBuffer::new(&prefill) }],
            active: 0,
            error: None,
        });
        self.mode = Mode::Input;
    }

    fn start_filter_context(&mut self) {
        let prefill = self.filter.context.clone().unwrap_or_default();
        self.input = Some(InputState {
            kind: InputKind::FilterContext,
            title: "Filter by context (@tag)".into(),
            fields: vec![Field { label: "Context", value: LineBuffer::new(&prefill) }],
            active: 0,
            error: None,
        });
        self.mode = Mode::Input;
    }

    fn clear_filters(&mut self) {
        if self.filter.is_active() {
            self.filter = Filter::default();
            self.clamp_selection();
            self.status = Some("Filters cleared".into());
        }
    }

    fn handle_help(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc
            | KeyCode::Char('q')
            | KeyCode::Char('h')
            | KeyCode::Char('?')
            | KeyCode::Enter => self.mode = Mode::Normal,
            _ => {}
        }
    }

    fn handle_input(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.cancel_input(),
            KeyCode::Enter => self.submit_input(),
            KeyCode::Tab => {
                if let Some(inp) = self.input.as_mut() {
                    inp.next_field();
                }
            }
            KeyCode::BackTab => {
                if let Some(inp) = self.input.as_mut() {
                    inp.prev_field();
                }
            }
            KeyCode::Up => {
                if let Some(inp) = self.input.as_mut() {
                    inp.prev_field();
                }
            }
            KeyCode::Down => {
                if let Some(inp) = self.input.as_mut() {
                    inp.next_field();
                }
            }
            KeyCode::Left => {
                if let Some(inp) = self.input.as_mut() {
                    inp.active_field().move_left();
                }
            }
            KeyCode::Right => {
                if let Some(inp) = self.input.as_mut() {
                    inp.active_field().move_right();
                }
            }
            KeyCode::Home => {
                if let Some(inp) = self.input.as_mut() {
                    inp.active_field().move_home();
                }
            }
            KeyCode::End => {
                if let Some(inp) = self.input.as_mut() {
                    inp.active_field().move_end();
                }
            }
            KeyCode::Backspace => {
                if let Some(inp) = self.input.as_mut() {
                    inp.active_field().backspace();
                    inp.error = None;
                }
                self.on_field_changed();
            }
            KeyCode::Delete => {
                if let Some(inp) = self.input.as_mut() {
                    inp.active_field().delete();
                    inp.error = None;
                }
                self.on_field_changed();
            }
            KeyCode::Char(c) => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    if c == 'c' {
                        self.cancel_input();
                    }
                } else {
                    if let Some(inp) = self.input.as_mut() {
                        inp.active_field().insert_char(c);
                        inp.error = None;
                    }
                    self.on_field_changed();
                }
            }
            _ => {}
        }
    }

    fn on_field_changed(&mut self) {
        let Some(inp) = &self.input else {
            return;
        };
        if inp.kind == InputKind::Search {
            let s = inp.field(0);
            self.filter.search = if s.is_empty() { None } else { Some(s) };
            self.clamp_selection();
        }
    }

    fn cancel_input(&mut self) {
        if let Some(inp) = &self.input {
            if inp.kind == InputKind::Search {
                self.filter.search = self.search_snapshot.take();
                self.clamp_selection();
            }
        }
        self.input = None;
        self.mode = Mode::Normal;
        self.status = Some("Cancelled".into());
    }

    fn submit_input(&mut self) {
        let Some(input) = self.input.clone() else {
            return;
        };
        match input.kind {
            InputKind::Search => {
                let s = input.fields[0].value.as_string();
                self.filter.search = if s.is_empty() { None } else { Some(s) };
                self.clamp_selection();
                self.input = None;
                self.mode = Mode::Normal;
                self.status = None;
            }
            InputKind::FilterProject => {
                let raw = input.fields[0].value.as_string().trim().to_string();
                let value = raw
                    .strip_prefix('+')
                    .unwrap_or(&raw)
                    .trim()
                    .to_string();
                self.filter.project = if value.is_empty() { None } else { Some(value) };
                self.clamp_selection();
                self.input = None;
                self.mode = Mode::Normal;
                self.status = None;
            }
            InputKind::FilterContext => {
                let raw = input.fields[0].value.as_string().trim().to_string();
                let value = raw
                    .strip_prefix('@')
                    .unwrap_or(&raw)
                    .trim()
                    .to_string();
                self.filter.context = if value.is_empty() { None } else { Some(value) };
                self.clamp_selection();
                self.input = None;
                self.mode = Mode::Normal;
                self.status = None;
            }
            InputKind::AddTask => self.apply_add(&input),
            InputKind::EditTask(i) => self.apply_edit(i, &input),
        }
    }

    fn set_input_error(&mut self, msg: &str) {
        if let Some(inp) = self.input.as_mut() {
            inp.error = Some(msg.to_string());
        }
    }

    fn apply_add(&mut self, input: &InputState) {
        let desc = input.fields[0].value.as_string().trim().to_string();
        if desc.is_empty() {
            self.set_input_error("Description is required");
            return;
        }
        let priority = match parse_priority_input(&input.fields[1].value.as_string()) {
            Ok(p) => p,
            Err(e) => {
                self.set_input_error(&e);
                return;
            }
        };
        let projects = parse_tags_input(&input.fields[2].value.as_string(), '+');
        let contexts = parse_tags_input(&input.fields[3].value.as_string(), '@');
        let due = match parse_due_input(&input.fields[4].value.as_string()) {
            Ok(d) => d,
            Err(e) => {
                self.set_input_error(&e);
                return;
            }
        };

        let mut task = Task::new(&desc);
        task.priority = priority;
        task.projects = projects;
        task.contexts = contexts;
        task.set_due_date(due.as_deref());

        self.tasks.push(task);
        self.input = None;
        self.mode = Mode::Normal;
        let real = self.tasks.len() - 1;
        self.clamp_selection();
        if let Some(pos) = self.visible_position(real) {
            self.selected = pos;
        }
        self.save_and_report("Added task");
    }

    fn apply_edit(&mut self, real: usize, input: &InputState) {
        let desc = input.fields[0].value.as_string().trim().to_string();
        if desc.is_empty() {
            self.set_input_error("Description is required");
            return;
        }
        let priority = match parse_priority_input(&input.fields[1].value.as_string()) {
            Ok(p) => p,
            Err(e) => {
                self.set_input_error(&e);
                return;
            }
        };
        let projects = parse_tags_input(&input.fields[2].value.as_string(), '+');
        let contexts = parse_tags_input(&input.fields[3].value.as_string(), '@');
        let due = match parse_due_input(&input.fields[4].value.as_string()) {
            Ok(d) => d,
            Err(e) => {
                self.set_input_error(&e);
                return;
            }
        };

        if let Some(t) = self.tasks.get_mut(real) {
            t.description = desc;
            t.priority = priority;
            t.projects = projects;
            t.contexts = contexts;
            t.set_due_date(due.as_deref());
        }
        self.input = None;
        self.mode = Mode::Normal;
        self.clamp_selection();
        self.save_and_report("Updated task");
    }
}

fn parse_priority_input(s: &str) -> Result<Option<char>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if chars.next().is_none() && first.is_ascii_alphabetic() {
        Ok(Some(first.to_ascii_uppercase()))
    } else {
        Err("Priority must be a single letter (A-Z) or empty".into())
    }
}

fn parse_tags_input(s: &str, marker: char) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for tok in s.split_whitespace() {
        let tok = tok.strip_prefix(marker).unwrap_or(tok);
        if !tok.is_empty() && !out.iter().any(|x| x == tok) {
            out.push(tok.to_string());
        }
    }
    out
}

fn parse_due_input(s: &str) -> Result<Option<String>, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(None);
    }
    if crate::task::is_valid_date(s) {
        Ok(Some(s.to_string()))
    } else {
        Err("Due date must be a valid YYYY-MM-DD date".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_matches_project_context_search() {
        let t = Task::parse("(B) Buy groceries +errands @home due:2026-03-15").unwrap();
        assert!(Filter { project: Some("errands".into()), ..Default::default() }.matches(&t));
        assert!(!Filter { project: Some("work".into()), ..Default::default() }.matches(&t));
        assert!(Filter { context: Some("home".into()), ..Default::default() }.matches(&t));
        assert!(Filter { search: Some("grocer".into()), ..Default::default() }.matches(&t));
        assert!(Filter { search: Some("GROCER".into()), ..Default::default() }.matches(&t));
        assert!(!Filter { search: Some("rent".into()), ..Default::default() }.matches(&t));
    }

    #[test]
    fn priority_parsing() {
        assert_eq!(parse_priority_input(""), Ok(None));
        assert_eq!(parse_priority_input("b"), Ok(Some('B')));
        assert_eq!(parse_priority_input("Z"), Ok(Some('Z')));
        assert!(parse_priority_input("AB").is_err());
        assert!(parse_priority_input("1").is_err());
    }

    #[test]
    fn tag_parsing() {
        assert_eq!(parse_tags_input("work personal", '+'), vec!["work", "personal"]);
        assert_eq!(parse_tags_input("+work +personal", '+'), vec!["work", "personal"]);
        assert_eq!(parse_tags_input("@home @phone", '@'), vec!["home", "phone"]);
        assert_eq!(parse_tags_input("", '+'), Vec::<String>::new());
    }

    #[test]
    fn due_parsing() {
        assert_eq!(parse_due_input("2026-03-15"), Ok(Some("2026-03-15".into())));
        assert_eq!(parse_due_input(""), Ok(None));
        assert!(parse_due_input("2026-02-30").is_err());
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::store::Store;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ch(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn type_text(app: &mut App, s: &str) {
        for c in s.chars() {
            app.handle_key(ch(c));
        }
    }

    fn temp_store(name: &str) -> Store {
        let mut p = std::env::temp_dir();
        p.push(format!("tooll_it_{}_{}.txt", name, std::process::id()));
        Store::new(p)
    }

    fn clean(store: &Store) {
        let _ = std::fs::remove_file(store.path());
    }

    #[test]
    fn add_task_persists_to_disk() {
        let store = temp_store("add");
        clean(&store);
        let mut app = App::new(store.clone(), store.load().unwrap());

        app.handle_key(ch('a'));
        type_text(&mut app, "Write report");
        app.handle_key(key(KeyCode::Tab));
        type_text(&mut app, "B");
        app.handle_key(key(KeyCode::Tab));
        type_text(&mut app, "+work +planning");
        app.handle_key(key(KeyCode::Tab));
        type_text(&mut app, "@computer");
        app.handle_key(key(KeyCode::Tab));
        type_text(&mut app, "2026-03-15");
        app.handle_key(key(KeyCode::Enter));

        assert_eq!(app.mode, Mode::Normal);
        assert_eq!(app.tasks.len(), 1);

        let reloaded = store.load().unwrap();
        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].description, "Write report");
        assert_eq!(reloaded[0].priority, Some('B'));
        assert_eq!(reloaded[0].projects, vec!["work", "planning"]);
        assert_eq!(reloaded[0].contexts, vec!["computer"]);
        assert_eq!(reloaded[0].due_date(), Some("2026-03-15"));
        assert!(!reloaded[0].completed);

        clean(&store);
    }

    #[test]
    fn complete_and_priority_persist() {
        let store = temp_store("mutate");
        clean(&store);
        std::fs::write(store.path(), "(B) Buy groceries +errands @home\n").unwrap();

        let mut app = App::new(store.clone(), store.load().unwrap());
        assert_eq!(app.tasks.len(), 1);

        app.handle_key(ch('p')); // raise priority (B) -> (A)
        app.handle_key(ch('x')); // mark complete

        let reloaded = store.load().unwrap();
        assert_eq!(reloaded[0].priority, Some('A'));
        assert!(reloaded[0].completed);
        assert!(reloaded[0].completion_date.is_some());

        clean(&store);
    }

    #[test]
    fn delete_persists() {
        let store = temp_store("delete");
        clean(&store);
        std::fs::write(store.path(), "(A) One\n(B) Two\n").unwrap();

        let mut app = App::new(store.clone(), store.load().unwrap());
        assert_eq!(app.tasks.len(), 2);

        app.handle_key(ch('d'));
        app.handle_key(ch('y'));

        assert_eq!(app.tasks.len(), 1);
        assert_eq!(store.load().unwrap().len(), 1);
        assert_eq!(store.load().unwrap()[0].description, "Two");

        clean(&store);
    }

    #[test]
    fn project_filter_and_search() {
        let store = temp_store("filter");
        clean(&store);
        std::fs::write(
            store.path(),
            "(A) Write report +work @computer\n(B) Buy milk +home @phone\n",
        )
        .unwrap();

        let mut app = App::new(store.clone(), store.load().unwrap());

        // Filter by project "+work" via the '+' prompt.
        app.handle_key(ch('+'));
        type_text(&mut app, "work");
        app.handle_key(key(KeyCode::Enter));
        assert_eq!(app.visible_indices(), vec![0]);

        // Clear the project filter.
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.visible_indices(), vec![0, 1]);

        // Live text search.
        app.handle_key(ch('/'));
        type_text(&mut app, "milk");
        assert_eq!(app.visible_indices(), vec![1]);

        // Cancel search restores the full view.
        app.handle_key(key(KeyCode::Esc));
        assert_eq!(app.visible_indices(), vec![0, 1]);

        clean(&store);
    }
}
