//! Loading and saving the todo.txt file.
//!
//! Every mutation in the UI goes through [`TaskStore`], and the store writes the
//! whole file back to disk immediately. There is no in-memory-only state: what
//! the list shows is what the file contains, which is the "authenticity"
//! requirement. Writes go to a sibling temp file and are then renamed over the
//! target, so an interrupted write can never truncate the user's task list.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::date::Ymd;
use crate::task::Task;

/// Line ending style observed in the file, preserved across writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    Lf,
    CrLf,
}

impl LineEnding {
    fn as_str(self) -> &'static str {
        match self {
            LineEnding::Lf => "\n",
            LineEnding::CrLf => "\r\n",
        }
    }
}

/// The task list plus the file it came from.
#[derive(Debug)]
pub struct TaskStore {
    path: PathBuf,
    tasks: Vec<Task>,
    next_id: u32,
    line_ending: LineEnding,
    /// True when the file did not exist at load time (created on first save).
    pub created_new: bool,
}

impl TaskStore {
    /// Read `path`. A missing file is treated as an empty list rather than an
    /// error, so `tooll` can be pointed at a not-yet-created todo file.
    pub fn load(path: impl Into<PathBuf>) -> io::Result<TaskStore> {
        let path = path.into();
        let (raw, created_new) = match fs::read(&path) {
            Ok(bytes) => (bytes, false),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (Vec::new(), true),
            Err(e) => return Err(e),
        };
        // todo.txt is a text format but a stray invalid byte should not stop the
        // tool; replace it rather than refusing to open the file.
        let text = String::from_utf8_lossy(&raw).into_owned();
        let line_ending = if text.contains("\r\n") {
            LineEnding::CrLf
        } else {
            LineEnding::Lf
        };

        let mut tasks = Vec::new();
        let mut next_id = 1;
        for line in text.lines() {
            if line.trim().is_empty() {
                continue; // Blank separators are not tasks.
            }
            tasks.push(Task::parse(line, next_id));
            next_id += 1;
        }

        Ok(TaskStore {
            path,
            tasks,
            next_id,
            line_ending,
            created_new,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    pub fn get(&self, id: u32) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Task> {
        self.tasks.iter_mut().find(|t| t.id == id)
    }

    pub fn index_of(&self, id: u32) -> Option<usize> {
        self.tasks.iter().position(|t| t.id == id)
    }

    /// Append a task parsed from raw todo.txt text and return its id.
    pub fn add_from_text(&mut self, text: &str) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.tasks.push(Task::from_input(text, id));
        id
    }

    /// Remove a task, returning the rendered line for the undo/status message.
    pub fn remove(&mut self, id: u32) -> Option<String> {
        let idx = self.index_of(id)?;
        Some(self.tasks.remove(idx).render())
    }

    /// Insert a previously removed task back at `idx` (used by undo).
    pub fn insert_at(&mut self, idx: usize, text: &str) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let idx = idx.min(self.tasks.len());
        self.tasks.insert(idx, Task::from_input(text, id));
        id
    }

    /// Replace the entire list from raw todo.txt text.
    ///
    /// Used by undo (which snapshots the serialised file) and by reload.
    pub fn replace_from_text(&mut self, text: &str) {
        self.tasks.clear();
        for line in text.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let id = self.next_id;
            self.next_id += 1;
            self.tasks.push(Task::parse(line, id));
        }
    }

    /// Drop every completed task, returning how many were archived away.
    pub fn remove_completed(&mut self) -> usize {
        let before = self.tasks.len();
        self.tasks.retain(|t| !t.completed);
        before - self.tasks.len()
    }

    /// Reorder the file so that incomplete tasks come first, then by priority,
    /// then by due date. Stable, so equal tasks keep their relative order.
    pub fn sort_tasks(&mut self) {
        self.tasks.sort_by(|a, b| {
            a.completed
                .cmp(&b.completed)
                .then(a.priority_rank().cmp(&b.priority_rank()))
                .then(a.due_rank().cmp(&b.due_rank()))
                .then_with(|| a.summary().to_lowercase().cmp(&b.summary().to_lowercase()))
        });
    }

    /// Move the task at `idx` one row up or down within the file.
    pub fn swap(&mut self, idx: usize, other: usize) -> bool {
        if idx >= self.tasks.len() || other >= self.tasks.len() || idx == other {
            return false;
        }
        self.tasks.swap(idx, other);
        true
    }

    /// Every distinct `+project` in the file, sorted case-insensitively.
    pub fn all_projects(&self) -> Vec<String> {
        self.collect_tags(|t| t.projects())
    }

    /// Every distinct `@context` in the file, sorted case-insensitively.
    pub fn all_contexts(&self) -> Vec<String> {
        self.collect_tags(|t| t.contexts())
    }

    fn collect_tags(&self, f: impl Fn(&Task) -> Vec<String>) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for task in &self.tasks {
            for tag in f(task) {
                if !out.iter().any(|e| e.eq_ignore_ascii_case(&tag)) {
                    out.push(tag);
                }
            }
        }
        out.sort_by_key(|s| s.to_lowercase());
        out
    }

    /// How many tasks carry a given project tag.
    pub fn project_count(&self, name: &str) -> usize {
        self.tasks.iter().filter(|t| t.has_project(name)).count()
    }

    /// How many tasks carry a given context tag.
    pub fn context_count(&self, name: &str) -> usize {
        self.tasks.iter().filter(|t| t.has_context(name)).count()
    }

    pub fn completed_count(&self) -> usize {
        self.tasks.iter().filter(|t| t.completed).count()
    }

    /// Tasks that are open and whose due date is in the past.
    pub fn overdue_count(&self, today: Ymd) -> usize {
        let cutoff = crate::date::days_from_civil(today.0, today.1, today.2);
        self.tasks
            .iter()
            .filter(|t| !t.completed && t.due_rank() < cutoff)
            .count()
    }

    /// The exact bytes that [`save`](Self::save) would write.
    pub fn serialize(&self) -> String {
        let eol = self.line_ending.as_str();
        let mut out = String::new();
        for task in &self.tasks {
            out.push_str(&task.render());
            out.push_str(eol);
        }
        out
    }

    /// Write the list back to disk atomically.
    ///
    /// The temp file is created in the same directory as the target so the
    /// rename stays on one filesystem and is therefore atomic.
    pub fn save(&self) -> io::Result<()> {
        let data = self.serialize();
        if let Some(dir) = self.path.parent() {
            if !dir.as_os_str().is_empty() {
                fs::create_dir_all(dir)?;
            }
        }

        let tmp = temp_path(&self.path);
        // Scoped so the handle is closed (and flushed) before the rename.
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(data.as_bytes())?;
            f.flush()?;
            // Durability: the rename below is atomic, but the contents must
            // reach the disk before the name points at them.
            f.sync_all()?;
        }

        match fs::rename(&tmp, &self.path) {
            Ok(()) => Ok(()),
            Err(e) => {
                let _ = fs::remove_file(&tmp);
                Err(e)
            }
        }
    }

    /// Append rendered lines to an archive file, creating it if needed.
    pub fn append_lines(path: &Path, lines: &[String], line_ending: LineEnding) -> io::Result<()> {
        if lines.is_empty() {
            return Ok(());
        }
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                fs::create_dir_all(dir)?;
            }
        }
        let mut body = String::new();
        for l in lines {
            body.push_str(l);
            body.push_str(line_ending.as_str());
        }
        let mut f = fs::OpenOptions::new().create(true).append(true).open(path)?;
        f.write_all(body.as_bytes())?;
        f.flush()
    }

    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    /// Default archive path: `done.txt` beside the task file.
    pub fn archive_path(&self) -> PathBuf {
        self.path.with_file_name(archive_file_name(&self.path))
    }
}

/// `todo.txt` -> `done.txt`; anything else gets a `.done` suffix.
fn archive_file_name(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "todo.txt".to_string());
    if name.eq_ignore_ascii_case("todo.txt") {
        "done.txt".to_string()
    } else if let Some(stem) = name.strip_suffix(".txt") {
        format!("{stem}.done.txt")
    } else {
        format!("{name}.done")
    }
}

/// Sibling temp path used for the atomic write.
fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "todo.txt".to_string());
    // The pid keeps two concurrent `tooll` processes from clobbering each other.
    path.with_file_name(format!(".{name}.tooll-{}.tmp", std::process::id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique scratch directory under the OS temp dir.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tooll-store-{}-{}-{:p}",
            std::process::id(),
            tag,
            &tag
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_loads_as_empty_and_is_created_on_save() {
        let dir = scratch("missing");
        let path = dir.join("nested").join("todo.txt");
        let mut s = TaskStore::load(&path).unwrap();
        assert!(s.is_empty());
        assert!(s.created_new);

        s.add_from_text("(A) First task +work @home");
        s.save().unwrap();

        let back = fs::read_to_string(&path).unwrap();
        assert_eq!(back, "(A) First task +work @home\n");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn round_trips_a_file_byte_for_byte() {
        let dir = scratch("roundtrip");
        let path = dir.join("todo.txt");
        let original = "(B) Buy groceries +errands @home due:2026-03-15\n\
                        x (A) Pay rent +finance @computer due:2026-01-01\n\
                        (C) Call plumber @phone +house\n";
        fs::write(&path, original).unwrap();

        let s = TaskStore::load(&path).unwrap();
        assert_eq!(s.len(), 3);
        assert!(!s.created_new);
        s.save().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn blank_lines_are_ignored_and_crlf_is_preserved() {
        let dir = scratch("crlf");
        let path = dir.join("todo.txt");
        fs::write(&path, "(A) one\r\n\r\n(B) two\r\n").unwrap();

        let s = TaskStore::load(&path).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!(s.line_ending(), LineEnding::CrLf);
        s.save().unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "(A) one\r\n(B) two\r\n");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn mutations_hit_the_disk() {
        let dir = scratch("mutate");
        let path = dir.join("todo.txt");
        fs::write(&path, "(B) Buy groceries +errands @home\n").unwrap();

        let mut s = TaskStore::load(&path).unwrap();
        let id = s.tasks()[0].id;
        s.get_mut(id).unwrap().raise_priority();
        s.get_mut(id).unwrap().add_tag('@', "town");
        s.save().unwrap();

        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "(A) Buy groceries +errands @home @town\n"
        );

        // Reload proves the on-disk state is what the next run would see.
        let s2 = TaskStore::load(&path).unwrap();
        assert_eq!(s2.tasks()[0].priority(), Some('A'));
        assert_eq!(s2.tasks()[0].contexts(), vec!["home", "town"]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn no_temp_files_are_left_behind() {
        let dir = scratch("tmp");
        let path = dir.join("todo.txt");
        let mut s = TaskStore::load(&path).unwrap();
        s.add_from_text("task");
        s.save().unwrap();
        s.save().unwrap();

        let leftovers: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "todo.txt")
            .collect();
        assert!(leftovers.is_empty(), "leftover files: {leftovers:?}");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn collects_and_counts_tags() {
        let dir = scratch("tags");
        let path = dir.join("todo.txt");
        fs::write(
            &path,
            "(A) a +work @home\n(B) b +Work @phone\nc +zeta +alpha @home\n",
        )
        .unwrap();

        let s = TaskStore::load(&path).unwrap();
        assert_eq!(s.all_projects(), vec!["alpha", "work", "zeta"]);
        assert_eq!(s.all_contexts(), vec!["home", "phone"]);
        assert_eq!(s.project_count("work"), 2);
        assert_eq!(s.context_count("home"), 2);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn sorts_open_high_priority_first() {
        let dir = scratch("sort");
        let path = dir.join("todo.txt");
        fs::write(
            &path,
            "x (A) done thing\nno priority\n(B) middle\n(A) top due:2026-01-01\n",
        )
        .unwrap();

        let mut s = TaskStore::load(&path).unwrap();
        s.sort_tasks();
        let lines: Vec<String> = s.tasks().iter().map(|t| t.render()).collect();
        assert_eq!(
            lines,
            vec![
                "(A) top due:2026-01-01",
                "(B) middle",
                "no priority",
                "x (A) done thing",
            ]
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn archives_completed_tasks() {
        let dir = scratch("archive");
        let path = dir.join("todo.txt");
        fs::write(&path, "x done one\nopen one\nx done two\n").unwrap();

        let mut s = TaskStore::load(&path).unwrap();
        let done: Vec<String> = s
            .tasks()
            .iter()
            .filter(|t| t.completed)
            .map(|t| t.render())
            .collect();
        TaskStore::append_lines(&s.archive_path(), &done, s.line_ending()).unwrap();
        assert_eq!(s.remove_completed(), 2);
        s.save().unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "open one\n");
        assert_eq!(
            fs::read_to_string(dir.join("done.txt")).unwrap(),
            "x done one\nx done two\n"
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn archive_path_naming() {
        assert_eq!(archive_file_name(Path::new("/bench/data/todo.txt")), "done.txt");
        assert_eq!(archive_file_name(Path::new("/tmp/tasks.txt")), "tasks.done.txt");
        assert_eq!(archive_file_name(Path::new("/tmp/list")), "list.done");
    }

    #[test]
    fn undo_reinserts_at_position() {
        let dir = scratch("undo");
        let path = dir.join("todo.txt");
        fs::write(&path, "one\ntwo\nthree\n").unwrap();

        let mut s = TaskStore::load(&path).unwrap();
        let id = s.tasks()[1].id;
        let text = s.remove(id).unwrap();
        assert_eq!(text, "two");
        s.insert_at(1, &text);
        assert_eq!(s.serialize(), "one\ntwo\nthree\n");
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn swaps_rows() {
        let dir = scratch("swap");
        let path = dir.join("todo.txt");
        fs::write(&path, "one\ntwo\n").unwrap();
        let mut s = TaskStore::load(&path).unwrap();
        assert!(s.swap(0, 1));
        assert!(!s.swap(0, 0));
        assert!(!s.swap(0, 9));
        assert_eq!(s.serialize(), "two\none\n");
        fs::remove_dir_all(&dir).ok();
    }
}
