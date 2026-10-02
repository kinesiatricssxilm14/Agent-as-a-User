//! Application state and all keyboard-driven behaviour.
//!
//! The UI layer (`ui.rs`) is a pure function of [`App`], and the event loop in
//! `main.rs` only forwards key events to [`App::on_key`]. Keeping the logic here
//! means every interaction is testable without a terminal, which is what
//! `tests/` exercises.

use std::io;
use std::path::PathBuf;

use crate::config::{Config, FileSource};
use crate::date::{self, Ymd};
use crate::input::TextInput;
use crate::store::TaskStore;
use crate::task::{self, Task};

/// How the list is ordered on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    /// File order — what the todo.txt actually looks like.
    FileOrder,
    /// Open first, then by priority letter.
    Priority,
    /// Open first, then by due date.
    DueDate,
}

impl SortMode {
    pub fn label(self) -> &'static str {
        match self {
            SortMode::FileOrder => "file order",
            SortMode::Priority => "priority",
            SortMode::DueDate => "due date",
        }
    }

    /// Cycle through the modes with the `s` key.
    pub fn next(self) -> SortMode {
        match self {
            SortMode::FileOrder => SortMode::Priority,
            SortMode::Priority => SortMode::DueDate,
            SortMode::DueDate => SortMode::FileOrder,
        }
    }
}

/// Which completion states are listed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoneFilter {
    All,
    OpenOnly,
    DoneOnly,
}

impl DoneFilter {
    pub fn label(self) -> &'static str {
        match self {
            DoneFilter::All => "all",
            DoneFilter::OpenOnly => "open only",
            DoneFilter::DoneOnly => "done only",
        }
    }

    pub fn next(self) -> DoneFilter {
        match self {
            DoneFilter::All => DoneFilter::OpenOnly,
            DoneFilter::OpenOnly => DoneFilter::DoneOnly,
            DoneFilter::DoneOnly => DoneFilter::All,
        }
    }
}

/// Severity of the status-line message, used for colouring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Info,
    Success,
    Warning,
    Error,
}

/// A transient status-line message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub text: String,
    pub kind: MessageKind,
}

/// Which side panel column holds keyboard focus.
///
/// All three columns stay visible at all times; focus only decides which one
/// the arrow keys drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Tasks,
    Projects,
    Contexts,
}

impl Focus {
    pub fn label(self) -> &'static str {
        match self {
            Focus::Tasks => "tasks",
            Focus::Projects => "projects",
            Focus::Contexts => "contexts",
        }
    }

    /// Tab order: tasks -> projects -> contexts -> tasks.
    pub fn next(self) -> Focus {
        match self {
            Focus::Tasks => Focus::Projects,
            Focus::Projects => Focus::Contexts,
            Focus::Contexts => Focus::Tasks,
        }
    }

    pub fn prev(self) -> Focus {
        match self {
            Focus::Tasks => Focus::Contexts,
            Focus::Projects => Focus::Tasks,
            Focus::Contexts => Focus::Projects,
        }
    }
}

/// Which field of the add/edit form is being typed into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum FormField {
    Description,
    Priority,
    Projects,
    Contexts,
    Due,
}

impl FormField {
    pub const ALL: [FormField; 5] = [
        FormField::Description,
        FormField::Priority,
        FormField::Projects,
        FormField::Contexts,
        FormField::Due,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FormField::Description => "Description",
            FormField::Priority => "Priority",
            FormField::Projects => "Projects",
            FormField::Contexts => "Contexts",
            FormField::Due => "Due date",
        }
    }

    /// Inline guidance shown next to the field.
    pub fn hint(self) -> &'static str {
        match self {
            FormField::Description => "what needs doing (plain text)",
            FormField::Priority => "single letter A-Z, empty for none",
            FormField::Projects => "+project names, comma or space separated",
            FormField::Contexts => "@context names, comma or space separated",
            FormField::Due => "YYYY-MM-DD, empty for none",
        }
    }

    fn index(self) -> usize {
        FormField::ALL.iter().position(|f| *f == self).unwrap()
    }

    fn next(self) -> FormField {
        FormField::ALL[(self.index() + 1) % FormField::ALL.len()]
    }

    fn prev(self) -> FormField {
        FormField::ALL[(self.index() + FormField::ALL.len() - 1) % FormField::ALL.len()]
    }
}

/// Whether the form is creating a task or editing an existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormPurpose {
    Add,
    Edit(u32),
}

/// The add/edit task form. Every field is visible on screen simultaneously.
#[derive(Debug, Clone)]
pub struct TaskForm {
    pub purpose: FormPurpose,
    pub field: FormField,
    pub description: TextInput,
    pub priority: TextInput,
    pub projects: TextInput,
    pub contexts: TextInput,
    pub due: TextInput,
    /// Validation error for the focused field, shown under the form.
    pub error: Option<String>,
}

impl TaskForm {
    /// An empty form for a new task, seeded with whatever filters are active so
    /// adding inside a filtered view does the obvious thing.
    pub fn new_add(seed_project: Option<&str>, seed_context: Option<&str>) -> TaskForm {
        TaskForm {
            purpose: FormPurpose::Add,
            field: FormField::Description,
            description: TextInput::new(),
            priority: TextInput::new(),
            projects: TextInput::with_text(seed_project.unwrap_or_default()),
            contexts: TextInput::with_text(seed_context.unwrap_or_default()),
            due: TextInput::new(),
            error: None,
        }
    }

    /// A form pre-filled from an existing task.
    pub fn new_edit(task: &Task) -> TaskForm {
        TaskForm {
            purpose: FormPurpose::Edit(task.id),
            field: FormField::Description,
            description: TextInput::with_text(task.summary()),
            priority: TextInput::with_text(
                task.priority().map(|c| c.to_string()).unwrap_or_default(),
            ),
            projects: TextInput::with_text(task.projects().join(" ")),
            contexts: TextInput::with_text(task.contexts().join(" ")),
            due: TextInput::with_text(task.due_raw().unwrap_or_default()),
            error: None,
        }
    }

    pub fn input(&self, field: FormField) -> &TextInput {
        match field {
            FormField::Description => &self.description,
            FormField::Priority => &self.priority,
            FormField::Projects => &self.projects,
            FormField::Contexts => &self.contexts,
            FormField::Due => &self.due,
        }
    }

    fn input_mut(&mut self, field: FormField) -> &mut TextInput {
        match field {
            FormField::Description => &mut self.description,
            FormField::Priority => &mut self.priority,
            FormField::Projects => &mut self.projects,
            FormField::Contexts => &mut self.contexts,
            FormField::Due => &mut self.due,
        }
    }

    pub fn current_mut(&mut self) -> &mut TextInput {
        let f = self.field;
        self.input_mut(f)
    }

    pub fn title(&self) -> &'static str {
        match self.purpose {
            FormPurpose::Add => "Add task",
            FormPurpose::Edit(_) => "Edit task",
        }
    }

    /// Validate the form, returning the normalised parts.
    fn validate(&self) -> Result<FormValues, (FormField, String)> {
        let desc = self.description.text().trim().to_string();
        if desc.is_empty() {
            return Err((
                FormField::Description,
                "description must not be empty".into(),
            ));
        }

        let pri_raw = self.priority.text().trim();
        let pri_raw = pri_raw.trim_start_matches('(').trim_end_matches(')');
        let priority = if pri_raw.is_empty() {
            None
        } else {
            let mut chars = pri_raw.chars();
            let c = chars.next().unwrap().to_ascii_uppercase();
            if chars.next().is_some() || !c.is_ascii_uppercase() {
                return Err((
                    FormField::Priority,
                    format!("`{pri_raw}` is not a single letter A-Z"),
                ));
            }
            Some(c)
        };

        let due_raw = self.due.text().trim();
        let due = if due_raw.is_empty() {
            None
        } else {
            match date::parse_ymd(due_raw) {
                Some(d) => Some(d),
                None => {
                    return Err((
                        FormField::Due,
                        format!("`{due_raw}` is not a valid YYYY-MM-DD date"),
                    ))
                }
            }
        };

        Ok(FormValues {
            description: desc,
            priority,
            projects: task::parse_tag_list(self.projects.text(), '+'),
            contexts: task::parse_tag_list(self.contexts.text(), '@'),
            due,
        })
    }
}

/// Normalised, validated form contents.
struct FormValues {
    description: String,
    priority: Option<char>,
    projects: Vec<String>,
    contexts: Vec<String>,
    due: Option<Ymd>,
}

/// A one-line prompt overlaid on the status bar (never hides the list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    /// Incremental search; the list filters as you type.
    Search,
    /// Set the priority of task `id` to a typed letter.
    Priority(u32),
    /// Replace the `@context` list of task `id`.
    Contexts(u32),
    /// Replace the `+project` list of task `id`.
    Projects(u32),
    /// Set the `due:` field of task `id`.
    Due(u32),
    /// Filter by project name typed free-form.
    FilterProject,
    /// Filter by context name typed free-form.
    FilterContext,
    /// Confirm deletion of task `id`.
    ConfirmDelete(u32),
    /// Confirm archiving all completed tasks.
    ConfirmArchive,
}

impl Prompt {
    /// Label shown at the left of the prompt line.
    pub fn label(&self) -> String {
        match self {
            Prompt::Search => "Search".into(),
            Prompt::Priority(_) => "Priority (A-Z, or - to clear)".into(),
            Prompt::Contexts(_) => "Contexts (@tags, comma or space separated)".into(),
            Prompt::Projects(_) => "Projects (+tags, comma or space separated)".into(),
            Prompt::Due(_) => "Due date (YYYY-MM-DD, empty to clear)".into(),
            Prompt::FilterProject => "Filter by project".into(),
            Prompt::FilterContext => "Filter by context".into(),
            Prompt::ConfirmDelete(_) => "Delete this task? (y/n)".into(),
            Prompt::ConfirmArchive => "Archive all completed tasks to done.txt? (y/n)".into(),
        }
    }

    /// Confirmations consume a single keypress instead of a text line.
    pub fn is_confirm(&self) -> bool {
        matches!(self, Prompt::ConfirmDelete(_) | Prompt::ConfirmArchive)
    }

    /// True when the list should re-filter on every keystroke.
    pub fn is_live(&self) -> bool {
        matches!(self, Prompt::Search)
    }
}

/// The screen currently accepting input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Normal browsing.
    Normal,
    /// A one-line prompt is active.
    Prompt(Prompt),
    /// The add/edit form is active.
    Form,
    /// The help screen is showing.
    Help,
}

/// One row of the visible task list.
#[derive(Debug, Clone)]
pub struct Row {
    pub task_id: u32,
    /// Index into the store, i.e. the line number in the file (0-based).
    pub file_index: usize,
}

/// A snapshot of the file taken before a mutation, for single-level undo.
#[derive(Debug, Clone)]
struct UndoState {
    serialized: String,
    /// Which task was selected, so undo restores the cursor too.
    selected_index: usize,
    description: String,
}

/// The whole application.
pub struct App {
    pub store: TaskStore,
    pub mode: Mode,
    pub focus: Focus,
    /// Selected row within the *filtered* task list.
    pub selected: usize,
    /// Selected row in the projects panel (0 = "all projects").
    pub project_cursor: usize,
    /// Selected row in the contexts panel (0 = "all contexts").
    pub context_cursor: usize,
    /// Active `+project` filter.
    pub project_filter: Option<String>,
    /// Active `@context` filter.
    pub context_filter: Option<String>,
    /// Active free-text search.
    pub search: String,
    pub sort_mode: SortMode,
    pub done_filter: DoneFilter,
    pub message: Option<Message>,
    pub form: Option<TaskForm>,
    /// Text buffer backing the active prompt.
    pub prompt_input: TextInput,
    /// Scroll offset of the help screen.
    pub help_scroll: u16,
    /// Scroll offset of the task list, kept in sync with `selected` by the UI.
    pub list_scroll: usize,
    /// Height in rows of the task list viewport, reported by the UI each frame so
    /// PageUp/PageDown move by what the user can actually see.
    pub viewport_rows: usize,
    /// Horizontal scroll of the task list, in characters. Lets `<`/`>` reveal the
    /// tail of lines too long for the terminal instead of silently clipping them.
    pub h_scroll: usize,
    /// Largest useful [`h_scroll`](Self::h_scroll), reported by the UI each frame.
    pub max_h_scroll: usize,
    /// Set when the user asks to quit.
    pub should_quit: bool,
    /// Today's date, captured at startup and used for completion stamps.
    pub today: Ymd,
    pub file_source: FileSource,
    pub config_path: Option<PathBuf>,
    undo: Option<UndoState>,
    /// Cached filtered row list; rebuilt whenever filters or tasks change.
    rows: Vec<Row>,
}

impl App {
    /// Build the application from a resolved configuration, loading the file.
    pub fn new(cfg: Config) -> io::Result<App> {
        let mut store = TaskStore::load(&cfg.file)?;
        let created_new = store.created_new;
        if cfg.sort_on_load {
            store.sort_tasks();
        }

        let mut app = App {
            store,
            mode: Mode::Normal,
            focus: Focus::Tasks,
            selected: 0,
            project_cursor: 0,
            context_cursor: 0,
            project_filter: None,
            context_filter: None,
            search: String::new(),
            sort_mode: SortMode::FileOrder,
            done_filter: if cfg.hide_done {
                DoneFilter::OpenOnly
            } else {
                DoneFilter::All
            },
            message: None,
            form: None,
            prompt_input: TextInput::new(),
            help_scroll: 0,
            list_scroll: 0,
            viewport_rows: 10,
            h_scroll: 0,
            max_h_scroll: 0,
            should_quit: false,
            today: date::today(),
            file_source: cfg.file_source,
            config_path: cfg.config_path,
            undo: None,
            rows: Vec::new(),
        };
        app.rebuild_rows();

        let path = app.store.path().display().to_string();
        app.message = Some(if created_new {
            Message {
                text: format!("{path} does not exist yet - it will be created on the first change. Press ? for help."),
                kind: MessageKind::Warning,
            }
        } else {
            Message {
                text: format!(
                    "Loaded {} task{} from {path}. Press ? for help.",
                    app.store.len(),
                    if app.store.len() == 1 { "" } else { "s" }
                ),
                kind: MessageKind::Info,
            }
        });
        Ok(app)
    }

    // ---------------------------------------------------------------- filtering

    /// The rows currently visible in the task panel.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// Recompute the filtered/sorted row list, keeping the selection on the same
    /// task where possible.
    pub fn rebuild_rows(&mut self) {
        let keep_id = self.selected_task_id();
        let needle = self.search.to_lowercase();

        let mut rows: Vec<Row> = self
            .store
            .tasks()
            .iter()
            .enumerate()
            .filter(|(_, t)| match self.done_filter {
                DoneFilter::All => true,
                DoneFilter::OpenOnly => !t.completed,
                DoneFilter::DoneOnly => t.completed,
            })
            .filter(|(_, t)| match &self.project_filter {
                Some(p) => t.has_project(p),
                None => true,
            })
            .filter(|(_, t)| match &self.context_filter {
                Some(c) => t.has_context(c),
                None => true,
            })
            .filter(|(_, t)| t.matches_query(&needle))
            .map(|(i, t)| Row {
                task_id: t.id,
                file_index: i,
            })
            .collect();

        match self.sort_mode {
            SortMode::FileOrder => {}
            SortMode::Priority => {
                let store = &self.store;
                rows.sort_by(|a, b| {
                    let ta = store.get(a.task_id).unwrap();
                    let tb = store.get(b.task_id).unwrap();
                    ta.completed
                        .cmp(&tb.completed)
                        .then(ta.priority_rank().cmp(&tb.priority_rank()))
                        .then(a.file_index.cmp(&b.file_index))
                });
            }
            SortMode::DueDate => {
                let store = &self.store;
                rows.sort_by(|a, b| {
                    let ta = store.get(a.task_id).unwrap();
                    let tb = store.get(b.task_id).unwrap();
                    ta.completed
                        .cmp(&tb.completed)
                        .then(ta.due_rank().cmp(&tb.due_rank()))
                        .then(a.file_index.cmp(&b.file_index))
                });
            }
        }

        self.rows = rows;

        // Restore the selection by task identity, not by row number.
        self.selected = match keep_id.and_then(|id| self.rows.iter().position(|r| r.task_id == id)) {
            Some(i) => i,
            None => self.selected.min(self.rows.len().saturating_sub(1)),
        };
        if self.rows.is_empty() {
            self.selected = 0;
        }
    }

    pub fn visible_count(&self) -> usize {
        self.rows.len()
    }

    /// The id of the task under the cursor, if any.
    pub fn selected_task_id(&self) -> Option<u32> {
        self.rows.get(self.selected).map(|r| r.task_id)
    }

    /// The task under the cursor, if any.
    pub fn selected_task(&self) -> Option<&Task> {
        self.selected_task_id().and_then(|id| self.store.get(id))
    }

    /// Project names for the side panel, each with its task count.
    pub fn project_entries(&self) -> Vec<(String, usize)> {
        self.store
            .all_projects()
            .into_iter()
            .map(|p| {
                let n = self.store.project_count(&p);
                (p, n)
            })
            .collect()
    }

    /// Context names for the side panel, each with its task count.
    pub fn context_entries(&self) -> Vec<(String, usize)> {
        self.store
            .all_contexts()
            .into_iter()
            .map(|c| {
                let n = self.store.context_count(&c);
                (c, n)
            })
            .collect()
    }

    /// True when any filter or search narrows the list.
    pub fn is_filtered(&self) -> bool {
        self.project_filter.is_some()
            || self.context_filter.is_some()
            || !self.search.is_empty()
            || self.done_filter != DoneFilter::All
    }

    // ----------------------------------------------------------------- messages

    fn info(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            kind: MessageKind::Info,
        });
    }

    fn success(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            kind: MessageKind::Success,
        });
    }

    fn warn(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            kind: MessageKind::Warning,
        });
    }

    fn error(&mut self, text: impl Into<String>) {
        self.message = Some(Message {
            text: text.into(),
            kind: MessageKind::Error,
        });
    }

    // ------------------------------------------------------------ persistence

    /// Snapshot the current file contents so the next change can be undone.
    fn snapshot(&mut self, description: impl Into<String>) {
        self.undo = Some(UndoState {
            serialized: self.store.serialize(),
            selected_index: self.selected,
            description: description.into(),
        });
    }

    /// Persist the store, reporting failures in the status bar.
    ///
    /// Returns `false` if the write failed, in which case the caller has already
    /// mutated memory; the message tells the user the disk is stale.
    fn save(&mut self, what: &str) -> bool {
        match self.store.save() {
            Ok(()) => {
                self.success(format!("{what} - saved to {}", self.store.path().display()));
                true
            }
            Err(e) => {
                self.error(format!(
                    "Could not write {}: {e}. The change is not on disk.",
                    self.store.path().display()
                ));
                false
            }
        }
    }

    /// Re-read the file from disk, discarding nothing (all changes are already
    /// written eagerly).
    pub fn reload(&mut self) {
        let path = self.store.path().to_path_buf();
        match TaskStore::load(&path) {
            Ok(store) => {
                let keep = self.selected_task_id().and_then(|id| self.store.index_of(id));
                self.store = store;
                self.undo = None;
                self.rebuild_rows();
                // Selection is restored by file position, since ids are fresh.
                if let Some(idx) = keep {
                    if let Some(pos) = self.rows.iter().position(|r| r.file_index >= idx) {
                        self.selected = pos;
                    }
                }
                self.info(format!(
                    "Reloaded {} task{} from {}",
                    self.store.len(),
                    if self.store.len() == 1 { "" } else { "s" },
                    path.display()
                ));
            }
            Err(e) => self.error(format!("Could not read {}: {e}", path.display())),
        }
    }

    /// Restore the snapshot taken before the last mutation.
    pub fn undo(&mut self) {
        let Some(state) = self.undo.take() else {
            self.warn("Nothing to undo");
            return;
        };
        self.store.replace_from_text(&state.serialized);
        self.rebuild_rows();
        self.selected = state.selected_index.min(self.rows.len().saturating_sub(1));
        if self.store.save().is_err() {
            self.error(format!(
                "Undid \"{}\" in memory but could not write {}",
                state.description,
                self.store.path().display()
            ));
        } else {
            self.success(format!("Undid: {}", state.description));
        }
    }

    // ------------------------------------------------------------- navigation

    fn move_selection(&mut self, delta: isize) {
        match self.focus {
            Focus::Tasks => {
                let len = self.rows.len();
                if len == 0 {
                    return;
                }
                self.selected = clamp_move(self.selected, delta, len);
            }
            Focus::Projects => {
                let len = self.project_entries().len() + 1; // + "all"
                self.project_cursor = clamp_move(self.project_cursor, delta, len);
            }
            Focus::Contexts => {
                let len = self.context_entries().len() + 1;
                self.context_cursor = clamp_move(self.context_cursor, delta, len);
            }
        }
    }

    fn jump_to(&mut self, first: bool) {
        match self.focus {
            Focus::Tasks => {
                self.selected = if first {
                    0
                } else {
                    self.rows.len().saturating_sub(1)
                }
            }
            Focus::Projects => {
                self.project_cursor = if first {
                    0
                } else {
                    self.project_entries().len()
                }
            }
            Focus::Contexts => {
                self.context_cursor = if first {
                    0
                } else {
                    self.context_entries().len()
                }
            }
        }
    }

    /// Move a page at a time, using the viewport height the UI last reported.
    fn page(&mut self, down: bool) {
        let step = self.viewport_rows.max(1) as isize;
        self.move_selection(if down { step } else { -step });
    }

    // ---------------------------------------------------------------- actions

    /// Raise or lower the priority of the selected task by one step.
    pub fn bump_priority(&mut self, up: bool) {
        let Some(id) = self.selected_task_id() else {
            self.warn("No task selected");
            return;
        };
        let before = self.store.get(id).unwrap().priority();
        self.snapshot("priority change");
        let changed = {
            let t = self.store.get_mut(id).unwrap();
            if up {
                t.raise_priority()
            } else {
                t.lower_priority()
            }
        };
        if !changed {
            self.undo = None;
            self.warn(if up {
                "Already at the highest priority (A)"
            } else {
                "Task has no priority to lower"
            });
            return;
        }
        let after = self.store.get(id).unwrap().priority();
        if self.save(&format!(
            "Priority {} -> {}",
            fmt_priority(before),
            fmt_priority(after)
        )) {
            self.rebuild_rows();
        }
    }

    /// Set the priority of a task to an explicit letter (or clear it).
    pub fn set_priority(&mut self, id: u32, priority: Option<char>) {
        let Some(task) = self.store.get(id) else {
            self.error("That task no longer exists");
            return;
        };
        let before = task.priority();
        self.snapshot("priority change");
        let changed = self.store.get_mut(id).unwrap().set_priority(priority);
        if !changed {
            self.undo = None;
            self.info(format!("Priority already {}", fmt_priority(before)));
            return;
        }
        if self.save(&format!(
            "Priority {} -> {}",
            fmt_priority(before),
            fmt_priority(priority)
        )) {
            self.rebuild_rows();
        }
    }

    /// Toggle the completion state of the selected task.
    pub fn toggle_complete(&mut self) {
        let Some(id) = self.selected_task_id() else {
            self.warn("No task selected");
            return;
        };
        self.snapshot("completion toggle");
        let today = self.today;
        let (was_done, summary) = {
            let t = self.store.get_mut(id).unwrap();
            let was = t.completed;
            t.toggle_complete(today);
            (was, t.summary())
        };
        let label = truncate_label(&summary, 40);
        let what = if was_done {
            format!("Reopened \"{label}\"")
        } else {
            format!("Completed \"{label}\"")
        };
        if self.save(&what) {
            self.rebuild_rows();
        }
    }

    /// Replace the `@context` list of a task.
    pub fn set_contexts(&mut self, id: u32, names: &[String]) {
        if self.store.get(id).is_none() {
            self.error("That task no longer exists");
            return;
        }
        self.snapshot("context change");
        if !self.store.get_mut(id).unwrap().set_contexts(names) {
            self.undo = None;
            self.info("Contexts unchanged");
            return;
        }
        let shown = self.store.get(id).unwrap().contexts();
        let what = if shown.is_empty() {
            "Cleared contexts".to_string()
        } else {
            format!(
                "Contexts set to {}",
                shown
                    .iter()
                    .map(|c| format!("@{c}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        };
        if self.save(&what) {
            self.rebuild_rows();
        }
    }

    /// Replace the `+project` list of a task.
    pub fn set_projects(&mut self, id: u32, names: &[String]) {
        if self.store.get(id).is_none() {
            self.error("That task no longer exists");
            return;
        }
        self.snapshot("project change");
        if !self.store.get_mut(id).unwrap().set_projects(names) {
            self.undo = None;
            self.info("Projects unchanged");
            return;
        }
        let shown = self.store.get(id).unwrap().projects();
        let what = if shown.is_empty() {
            "Cleared projects".to_string()
        } else {
            format!(
                "Projects set to {}",
                shown
                    .iter()
                    .map(|p| format!("+{p}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            )
        };
        if self.save(&what) {
            self.rebuild_rows();
        }
    }

    /// Set or clear the `due:` field of a task.
    pub fn set_due(&mut self, id: u32, due: Option<Ymd>) {
        if self.store.get(id).is_none() {
            self.error("That task no longer exists");
            return;
        }
        self.snapshot("due date change");
        if !self.store.get_mut(id).unwrap().set_due(due) {
            self.undo = None;
            self.info("Due date unchanged");
            return;
        }
        let what = match due {
            Some(d) => format!("Due date set to {}", date::format_ymd(d)),
            None => "Cleared due date".to_string(),
        };
        if self.save(&what) {
            self.rebuild_rows();
        }
    }

    /// Append a task built from the form (or from raw todo.txt text).
    pub fn add_task(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            self.warn("Nothing to add");
            return;
        }
        self.snapshot("task added");
        let id = self.store.add_from_text(line);
        self.rebuild_rows();
        // Put the cursor on the new task if the current filters show it.
        if let Some(pos) = self.rows.iter().position(|r| r.task_id == id) {
            self.focus = Focus::Tasks;
            self.selected = pos;
        }
        let label = truncate_label(line, 48);
        if !self.save(&format!("Added \"{label}\"")) {
            return;
        }
        if self.rows.iter().all(|r| r.task_id != id) {
            self.warn(format!(
                "Added \"{label}\" but the active filter hides it - press F to clear filters"
            ));
        }
    }

    /// Apply an edited form to an existing task.
    fn apply_edit(
        &mut self,
        id: u32,
        description: &str,
        priority: Option<char>,
        projects: &[String],
        contexts: &[String],
        due: Option<Ymd>,
    ) {
        if self.store.get(id).is_none() {
            self.error("That task no longer exists");
            return;
        }
        self.snapshot("task edited");
        let before = self.store.get(id).unwrap().render();
        {
            let t = self.store.get_mut(id).unwrap();
            // Rebuild the description from the prose, then re-attach metadata so
            // the field order stays predictable after an edit.
            t.set_description(description);
            t.set_priority(priority);
            t.set_projects(projects);
            t.set_contexts(contexts);
            t.set_due(due);
        }
        let after = self.store.get(id).unwrap().render();
        if before == after {
            self.undo = None;
            self.info("No changes");
            return;
        }
        if self.save(&format!("Saved \"{}\"", truncate_label(description, 40))) {
            self.rebuild_rows();
        }
    }

    /// Delete a task outright.
    pub fn delete_task(&mut self, id: u32) {
        let Some(task) = self.store.get(id) else {
            self.error("That task no longer exists");
            return;
        };
        let label = truncate_label(&task.summary(), 40);
        self.snapshot("task deleted");
        self.store.remove(id);
        self.rebuild_rows();
        if self.selected >= self.rows.len() {
            self.selected = self.rows.len().saturating_sub(1);
        }
        self.save(&format!("Deleted \"{label}\" (u to undo)"));
    }

    /// Move completed tasks out of the task file and into the archive file.
    pub fn archive_completed(&mut self) {
        let done: Vec<String> = self
            .store
            .tasks()
            .iter()
            .filter(|t| t.completed)
            .map(|t| t.render())
            .collect();
        if done.is_empty() {
            self.warn("No completed tasks to archive");
            return;
        }
        let archive = self.store.archive_path();
        if let Err(e) = TaskStore::append_lines(&archive, &done, self.store.line_ending()) {
            self.error(format!("Could not write {}: {e}", archive.display()));
            return;
        }
        self.snapshot("archive completed tasks");
        let n = self.store.remove_completed();
        self.rebuild_rows();
        if self.selected >= self.rows.len() {
            self.selected = self.rows.len().saturating_sub(1);
        }
        self.save(&format!(
            "Archived {n} completed task{} to {}",
            if n == 1 { "" } else { "s" },
            archive.display()
        ));
    }

    /// Reorder the file itself: move the selected task one line up or down.
    pub fn move_task_in_file(&mut self, up: bool) {
        if self.focus != Focus::Tasks {
            return;
        }
        let Some(id) = self.selected_task_id() else {
            self.warn("No task selected");
            return;
        };
        if self.sort_mode != SortMode::FileOrder {
            self.warn(format!(
                "Reordering needs file order; press s to switch (currently {})",
                self.sort_mode.label()
            ));
            return;
        }
        let Some(idx) = self.store.index_of(id) else {
            return;
        };
        let target = if up {
            match idx.checked_sub(1) {
                Some(t) => t,
                None => {
                    self.warn("Already the first line in the file");
                    return;
                }
            }
        } else if idx + 1 < self.store.len() {
            idx + 1
        } else {
            self.warn("Already the last line in the file");
            return;
        };
        self.snapshot("task moved");
        self.store.swap(idx, target);
        self.rebuild_rows();
        self.save(&format!("Moved task to line {}", target + 1));
    }

    /// Sort the file on disk into the current display order.
    pub fn sort_file(&mut self) {
        if self.store.is_empty() {
            self.warn("Nothing to sort");
            return;
        }
        self.snapshot("file sorted");
        self.store.sort_tasks();
        self.rebuild_rows();
        self.save("Sorted the file: open tasks first, then by priority and due date");
    }

    // ---------------------------------------------------------------- filters

    /// Apply the project filter, or clear it with `None`.
    pub fn set_project_filter(&mut self, name: Option<String>) {
        match &name {
            Some(n) => self.info(format!("Filtering by project +{n}")),
            None => self.info("Project filter cleared"),
        }
        // Keep the panel cursor in step with the applied filter.
        self.project_cursor = match &name {
            None => 0,
            Some(n) => self
                .project_entries()
                .iter()
                .position(|(p, _)| p.eq_ignore_ascii_case(n))
                .map(|i| i + 1)
                .unwrap_or(0),
        };
        self.project_filter = name;
        self.selected = 0;
        self.rebuild_rows();
        self.report_empty_filter();
    }

    /// Apply the context filter, or clear it with `None`.
    pub fn set_context_filter(&mut self, name: Option<String>) {
        match &name {
            Some(n) => self.info(format!("Filtering by context @{n}")),
            None => self.info("Context filter cleared"),
        }
        self.context_cursor = match &name {
            None => 0,
            Some(n) => self
                .context_entries()
                .iter()
                .position(|(c, _)| c.eq_ignore_ascii_case(n))
                .map(|i| i + 1)
                .unwrap_or(0),
        };
        self.context_filter = name;
        self.selected = 0;
        self.rebuild_rows();
        self.report_empty_filter();
    }

    /// Warn when the combination of filters leaves nothing on screen, so an
    /// empty list is never mistaken for an empty file.
    fn report_empty_filter(&mut self) {
        if self.rows.is_empty() && !self.store.is_empty() {
            self.warn(format!(
                "No tasks match {} - press F to clear all filters",
                self.filter_summary()
            ));
        }
    }

    /// Human description of the active filters, for the header and messages.
    pub fn filter_summary(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if let Some(p) = &self.project_filter {
            parts.push(format!("+{p}"));
        }
        if let Some(c) = &self.context_filter {
            parts.push(format!("@{c}"));
        }
        if !self.search.is_empty() {
            parts.push(format!("\"{}\"", self.search));
        }
        if self.done_filter != DoneFilter::All {
            parts.push(self.done_filter.label().to_string());
        }
        if parts.is_empty() {
            "no filter".to_string()
        } else {
            parts.join(" + ")
        }
    }

    /// Drop every filter and the search term.
    pub fn clear_filters(&mut self) {
        let had = self.is_filtered();
        self.project_filter = None;
        self.context_filter = None;
        self.search.clear();
        self.done_filter = DoneFilter::All;
        self.project_cursor = 0;
        self.context_cursor = 0;
        self.h_scroll = 0;
        self.rebuild_rows();
        if had {
            self.success("Cleared all filters");
        } else {
            self.info("No filters were active");
        }
    }

    /// Apply the highlighted entry in the projects or contexts panel.
    fn activate_panel_entry(&mut self) {
        match self.focus {
            Focus::Projects => {
                if self.project_cursor == 0 {
                    self.set_project_filter(None);
                } else {
                    let entries = self.project_entries();
                    match entries.get(self.project_cursor - 1) {
                        Some((name, _)) => {
                            let name = name.clone();
                            // Selecting the active filter again toggles it off.
                            if self
                                .project_filter
                                .as_deref()
                                .is_some_and(|f| f.eq_ignore_ascii_case(&name))
                            {
                                self.set_project_filter(None);
                            } else {
                                self.set_project_filter(Some(name));
                            }
                        }
                        None => self.warn("No project there"),
                    }
                }
            }
            Focus::Contexts => {
                if self.context_cursor == 0 {
                    self.set_context_filter(None);
                } else {
                    let entries = self.context_entries();
                    match entries.get(self.context_cursor - 1) {
                        Some((name, _)) => {
                            let name = name.clone();
                            if self
                                .context_filter
                                .as_deref()
                                .is_some_and(|f| f.eq_ignore_ascii_case(&name))
                            {
                                self.set_context_filter(None);
                            } else {
                                self.set_context_filter(Some(name));
                            }
                        }
                        None => self.warn("No context there"),
                    }
                }
            }
            Focus::Tasks => {
                // Enter on a task opens it for editing.
                self.open_edit_form();
            }
        }
    }

    /// Filter by the first project of the selected task (quick drill-down).
    fn filter_by_selected_project(&mut self) {
        let Some(task) = self.selected_task() else {
            self.warn("No task selected");
            return;
        };
        match task.projects().first() {
            Some(p) => {
                let p = p.clone();
                self.set_project_filter(Some(p));
            }
            None => self.warn("Selected task has no +project tag"),
        }
    }

    /// Filter by the first context of the selected task (quick drill-down).
    fn filter_by_selected_context(&mut self) {
        let Some(task) = self.selected_task() else {
            self.warn("No task selected");
            return;
        };
        match task.contexts().first() {
            Some(c) => {
                let c = c.clone();
                self.set_context_filter(Some(c));
            }
            None => self.warn("Selected task has no @context tag"),
        }
    }

    // ------------------------------------------------------------ mode entry

    fn open_prompt(&mut self, prompt: Prompt, initial: &str) {
        self.prompt_input = TextInput::with_text(initial);
        self.mode = Mode::Prompt(prompt);
    }

    fn open_add_form(&mut self) {
        self.form = Some(TaskForm::new_add(
            self.project_filter.as_deref(),
            self.context_filter.as_deref(),
        ));
        self.mode = Mode::Form;
        self.info("Tab / Shift-Tab move between fields, Enter saves, Esc cancels");
    }

    fn open_edit_form(&mut self) {
        let Some(task) = self.selected_task() else {
            self.warn("No task selected");
            return;
        };
        self.form = Some(TaskForm::new_edit(task));
        self.mode = Mode::Form;
        self.info("Tab / Shift-Tab move between fields, Enter saves, Esc cancels");
    }

    // ------------------------------------------------------------ key handling

    /// Handle one key press. This is the single entry point for all input.
    pub fn on_key(&mut self, key: Key) {
        match self.mode.clone() {
            Mode::Normal => self.on_key_normal(key),
            Mode::Help => self.on_key_help(key),
            Mode::Form => self.on_key_form(key),
            Mode::Prompt(p) => self.on_key_prompt(key, p),
        }
    }

    fn on_key_normal(&mut self, key: Key) {
        match key {
            // ---- quit and help
            Key::Char('q') | Key::Esc => {
                if self.is_filtered() {
                    // Esc with filters active clears them rather than quitting,
                    // so a stray Esc never loses the user's place.
                    if key == Key::Esc {
                        self.clear_filters();
                        return;
                    }
                }
                self.should_quit = true;
            }
            Key::CtrlChar('c') => self.should_quit = true,
            Key::Char('?') | Key::F(1) => {
                self.help_scroll = 0;
                self.mode = Mode::Help;
            }

            // ---- navigation
            Key::Up | Key::Char('k') => self.move_selection(-1),
            Key::Down | Key::Char('j') => self.move_selection(1),
            Key::PageUp => self.page(false),
            Key::PageDown => self.page(true),
            Key::Home | Key::Char('g') => self.jump_to(true),
            Key::End | Key::Char('G') => self.jump_to(false),
            Key::Tab | Key::Right | Key::Char('l') => self.focus = self.focus.next(),
            Key::BackTab | Key::Left | Key::Char('h') => self.focus = self.focus.prev(),
            Key::Char('1') => self.focus = Focus::Tasks,
            Key::Char('2') => self.focus = Focus::Projects,
            Key::Char('3') => self.focus = Focus::Contexts,
            Key::Enter => self.activate_panel_entry(),
            // Reveal the tail of lines wider than the terminal.
            Key::Char('>') | Key::Char('.') => {
                if self.max_h_scroll == 0 {
                    self.info("Every task line already fits on screen");
                } else {
                    self.h_scroll = (self.h_scroll + 8).min(self.max_h_scroll);
                }
            }
            Key::Char('<') | Key::Char(',') => self.h_scroll = self.h_scroll.saturating_sub(8),

            // ---- task mutations
            Key::Char('a') => self.open_add_form(),
            Key::Char('e') => self.open_edit_form(),
            Key::Char(' ') | Key::Char('x') => self.toggle_complete(),
            Key::Char('A') => {
                // Shift-A raises priority; kept next to `p` for discoverability.
                self.bump_priority(true)
            }
            Key::Char('+') | Key::Char('=') => self.bump_priority(true),
            Key::Char('-') | Key::Char('_') => self.bump_priority(false),
            Key::Char('p') => match self.selected_task_id() {
                Some(id) => {
                    let cur = self
                        .store
                        .get(id)
                        .and_then(|t| t.priority())
                        .map(|c| c.to_string())
                        .unwrap_or_default();
                    self.open_prompt(Prompt::Priority(id), &cur);
                }
                None => self.warn("No task selected"),
            },
            Key::Char('c') => match self.selected_task_id() {
                Some(id) => {
                    let cur = self
                        .store
                        .get(id)
                        .map(|t| t.contexts().join(" "))
                        .unwrap_or_default();
                    self.open_prompt(Prompt::Contexts(id), &cur);
                }
                None => self.warn("No task selected"),
            },
            Key::Char('P') => match self.selected_task_id() {
                Some(id) => {
                    let cur = self
                        .store
                        .get(id)
                        .map(|t| t.projects().join(" "))
                        .unwrap_or_default();
                    self.open_prompt(Prompt::Projects(id), &cur);
                }
                None => self.warn("No task selected"),
            },
            Key::Char('d') => match self.selected_task_id() {
                Some(id) => {
                    let cur = self
                        .store
                        .get(id)
                        .and_then(|t| t.due_raw())
                        .unwrap_or_default()
                        .to_string();
                    self.open_prompt(Prompt::Due(id), &cur);
                }
                None => self.warn("No task selected"),
            },
            Key::Char('D') | Key::Delete => match self.selected_task_id() {
                Some(id) => self.open_prompt(Prompt::ConfirmDelete(id), ""),
                None => self.warn("No task selected"),
            },
            Key::Char('X') => self.open_prompt(Prompt::ConfirmArchive, ""),
            Key::Char('u') => self.undo(),
            Key::CtrlChar('k') => self.move_task_in_file(true),
            Key::CtrlChar('j') => self.move_task_in_file(false),
            Key::Char('S') => self.sort_file(),

            // ---- filtering and search
            Key::Char('/') => self.open_prompt(Prompt::Search, &self.search.clone()),
            Key::Char('f') => {
                let cur = self.project_filter.clone().unwrap_or_default();
                self.open_prompt(Prompt::FilterProject, &cur);
            }
            Key::Char('@') => {
                let cur = self.context_filter.clone().unwrap_or_default();
                self.open_prompt(Prompt::FilterContext, &cur);
            }
            Key::Char('F') => self.clear_filters(),
            Key::Char('t') => self.filter_by_selected_project(),
            Key::Char('T') => self.filter_by_selected_context(),
            Key::Char('s') => {
                self.sort_mode = self.sort_mode.next();
                self.rebuild_rows();
                self.info(format!("Sorting by {}", self.sort_mode.label()));
            }
            Key::Char('v') => {
                self.done_filter = self.done_filter.next();
                self.selected = 0;
                self.rebuild_rows();
                self.info(format!("Showing {} tasks", self.done_filter.label()));
                self.report_empty_filter();
            }
            Key::Char('r') | Key::F(5) => self.reload(),

            _ => {}
        }
    }

    fn on_key_help(&mut self, key: Key) {
        match key {
            Key::Char('?') | Key::Esc | Key::Char('q') | Key::Enter | Key::F(1) => {
                self.mode = Mode::Normal;
                self.help_scroll = 0;
            }
            Key::Down | Key::Char('j') => self.help_scroll = self.help_scroll.saturating_add(1),
            Key::Up | Key::Char('k') => self.help_scroll = self.help_scroll.saturating_sub(1),
            Key::PageDown | Key::Char(' ') => {
                self.help_scroll = self.help_scroll.saturating_add(10)
            }
            Key::PageUp => self.help_scroll = self.help_scroll.saturating_sub(10),
            Key::Home | Key::Char('g') => self.help_scroll = 0,
            Key::CtrlChar('c') => self.should_quit = true,
            _ => {}
        }
    }

    fn on_key_form(&mut self, key: Key) {
        let Some(form) = self.form.as_mut() else {
            self.mode = Mode::Normal;
            return;
        };
        match key {
            Key::Esc => {
                self.form = None;
                self.mode = Mode::Normal;
                self.info("Cancelled");
            }
            Key::CtrlChar('c') => self.should_quit = true,
            Key::Tab | Key::Down => {
                form.field = form.field.next();
                form.error = None;
            }
            Key::BackTab | Key::Up => {
                form.field = form.field.prev();
                form.error = None;
            }
            Key::Enter => self.submit_form(),
            Key::Left => form.current_mut().left(),
            Key::Right => form.current_mut().right(),
            Key::Home => form.current_mut().home(),
            Key::End => form.current_mut().end(),
            Key::Backspace => {
                form.current_mut().backspace();
                form.error = None;
            }
            Key::Delete => {
                form.current_mut().delete();
                form.error = None;
            }
            Key::CtrlChar('w') => {
                form.current_mut().delete_word_before();
                form.error = None;
            }
            Key::CtrlChar('u') => {
                form.current_mut().delete_to_start();
                form.error = None;
            }
            Key::Char(c) => {
                form.current_mut().insert(c);
                form.error = None;
            }
            _ => {}
        }
    }

    /// Validate and commit the add/edit form.
    fn submit_form(&mut self) {
        let Some(form) = self.form.as_ref() else { return };
        match form.validate() {
            Err((field, msg)) => {
                if let Some(f) = self.form.as_mut() {
                    f.field = field;
                    f.error = Some(msg.clone());
                }
                self.error(format!("{}: {msg}", field.label()));
            }
            Ok(values) => {
                let purpose = form.purpose;
                self.form = None;
                self.mode = Mode::Normal;
                match purpose {
                    FormPurpose::Add => {
                        let line = build_line(&values);
                        self.add_task(&line);
                    }
                    FormPurpose::Edit(id) => self.apply_edit(
                        id,
                        &values.description,
                        values.priority,
                        &values.projects,
                        &values.contexts,
                        values.due,
                    ),
                }
            }
        }
    }

    fn on_key_prompt(&mut self, key: Key, prompt: Prompt) {
        if prompt.is_confirm() {
            match key {
                Key::Char('y') | Key::Char('Y') => {
                    self.mode = Mode::Normal;
                    match prompt {
                        Prompt::ConfirmDelete(id) => self.delete_task(id),
                        Prompt::ConfirmArchive => self.archive_completed(),
                        _ => {}
                    }
                }
                Key::CtrlChar('c') => self.should_quit = true,
                Key::Char('n') | Key::Char('N') | Key::Esc | Key::Enter => {
                    self.mode = Mode::Normal;
                    self.info("Cancelled");
                }
                _ => {}
            }
            return;
        }

        match key {
            Key::Esc => {
                // Abandoning a live search restores the previous list.
                if prompt.is_live() {
                    self.search.clear();
                    self.rebuild_rows();
                }
                self.mode = Mode::Normal;
                self.prompt_input.clear();
                self.info("Cancelled");
            }
            Key::CtrlChar('c') => self.should_quit = true,
            Key::Enter => {
                let text = self.prompt_input.text().to_string();
                self.mode = Mode::Normal;
                self.prompt_input.clear();
                self.commit_prompt(prompt, &text);
            }
            Key::Left => self.prompt_input.left(),
            Key::Right => self.prompt_input.right(),
            Key::Home => self.prompt_input.home(),
            Key::End => self.prompt_input.end(),
            Key::Up | Key::Down => {
                // Let the user keep browsing the list while a live search is open.
                if prompt.is_live() {
                    self.move_selection(if key == Key::Down { 1 } else { -1 });
                }
            }
            Key::Backspace => {
                self.prompt_input.backspace();
                self.refresh_live_prompt(&prompt);
            }
            Key::Delete => {
                self.prompt_input.delete();
                self.refresh_live_prompt(&prompt);
            }
            Key::CtrlChar('w') => {
                self.prompt_input.delete_word_before();
                self.refresh_live_prompt(&prompt);
            }
            Key::CtrlChar('u') => {
                self.prompt_input.delete_to_start();
                self.refresh_live_prompt(&prompt);
            }
            Key::Char(c) => {
                self.prompt_input.insert(c);
                self.refresh_live_prompt(&prompt);
            }
            _ => {}
        }
    }

    /// Re-filter as the user types in an incremental prompt.
    fn refresh_live_prompt(&mut self, prompt: &Prompt) {
        if prompt.is_live() {
            self.search = self.prompt_input.text().to_string();
            self.selected = 0;
            self.rebuild_rows();
        }
    }

    /// Apply a prompt's typed value.
    fn commit_prompt(&mut self, prompt: Prompt, text: &str) {
        let trimmed = text.trim();
        match prompt {
            Prompt::Search => {
                self.search = trimmed.to_string();
                self.selected = 0;
                self.rebuild_rows();
                if self.search.is_empty() {
                    self.info("Search cleared");
                } else {
                    let n = self.rows.len();
                    self.info(format!(
                        "{n} task{} match \"{}\"",
                        if n == 1 { "" } else { "s" },
                        self.search
                    ));
                    self.focus = Focus::Tasks;
                }
            }
            Prompt::Priority(id) => {
                let cleaned = trimmed.trim_start_matches('(').trim_end_matches(')');
                if cleaned.is_empty() || cleaned == "-" {
                    self.set_priority(id, None);
                    return;
                }
                let mut chars = cleaned.chars();
                let c = chars.next().unwrap().to_ascii_uppercase();
                if chars.next().is_some() || !c.is_ascii_uppercase() {
                    self.error(format!(
                        "`{cleaned}` is not a priority - use a single letter A-Z, or - to clear"
                    ));
                    return;
                }
                self.set_priority(id, Some(c));
            }
            Prompt::Contexts(id) => {
                let names = task::parse_tag_list(trimmed, '@');
                self.set_contexts(id, &names);
            }
            Prompt::Projects(id) => {
                let names = task::parse_tag_list(trimmed, '+');
                self.set_projects(id, &names);
            }
            Prompt::Due(id) => {
                if trimmed.is_empty() {
                    self.set_due(id, None);
                    return;
                }
                match date::parse_ymd(trimmed) {
                    Some(d) => self.set_due(id, Some(d)),
                    None => self.error(format!(
                        "`{trimmed}` is not a valid date - use YYYY-MM-DD, or leave empty to clear"
                    )),
                }
            }
            Prompt::FilterProject => {
                let name = task::normalize_tag_name(trimmed, '+');
                match name {
                    None => self.set_project_filter(None),
                    Some(n) => {
                        if self.store.project_count(&n) == 0 {
                            self.warn(format!("No task carries +{n}"));
                        }
                        self.set_project_filter(Some(n));
                    }
                }
            }
            Prompt::FilterContext => {
                let name = task::normalize_tag_name(trimmed, '@');
                match name {
                    None => self.set_context_filter(None),
                    Some(n) => {
                        if self.store.context_count(&n) == 0 {
                            self.warn(format!("No task carries @{n}"));
                        }
                        self.set_context_filter(Some(n));
                    }
                }
            }
            Prompt::ConfirmDelete(_) | Prompt::ConfirmArchive => {}
        }
    }
}

/// Assemble a todo.txt line from validated form values.
fn build_line(v: &FormValues) -> String {
    let mut out = String::new();
    if let Some(p) = v.priority {
        out.push('(');
        out.push(p);
        out.push_str(") ");
    }
    out.push_str(&v.description);
    for p in &v.projects {
        out.push(' ');
        out.push('+');
        out.push_str(p);
    }
    for c in &v.contexts {
        out.push(' ');
        out.push('@');
        out.push_str(c);
    }
    if let Some(d) = v.due {
        out.push_str(" due:");
        out.push_str(&date::format_ymd(d));
    }
    out
}

/// Clamp a relative cursor move to `[0, len)`.
fn clamp_move(current: usize, delta: isize, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let next = current as isize + delta;
    next.clamp(0, len as isize - 1) as usize
}

/// `(A)` or `none`, for status messages.
fn fmt_priority(p: Option<char>) -> String {
    match p {
        Some(c) => format!("({c})"),
        None => "none".to_string(),
    }
}

/// Shorten a label for a status message, on a char boundary.
fn truncate_label(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}

/// A backend-independent key event.
///
/// `crossterm` events are translated into this in `main.rs`, which keeps `App`
/// free of terminal types and lets the tests drive it directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    CtrlChar(char),
    Enter,
    Esc,
    Backspace,
    Delete,
    Tab,
    BackTab,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    F(u8),
}
