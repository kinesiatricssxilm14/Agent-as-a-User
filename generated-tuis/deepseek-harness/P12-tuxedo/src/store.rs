//! Real filesystem-backed storage for the task list.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::task::Task;

#[derive(Clone)]
pub struct Store {
    path: PathBuf,
}

impl Store {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load all tasks from disk. A missing file is treated as an empty list.
    pub fn load(&self) -> Result<Vec<Task>> {
        let content = match fs::read_to_string(&self.path) {
            Ok(c) => c,
            Err(e) if e.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => {
                return Err(e).with_context(|| format!("failed to read {}", self.path.display()));
            }
        };

        let mut tasks = Vec::new();
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Some(t) = Task::parse(line) {
                tasks.push(t);
            }
        }
        Ok(tasks)
    }

    /// Persist all tasks, creating parent directories as needed.
    pub fn save(&self, tasks: &[Task]) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)
                    .with_context(|| format!("failed to create directory {}", parent.display()))?;
            }
        }

        let mut out = String::new();
        for t in tasks {
            out.push_str(&t.format());
            out.push('\n');
        }
        fs::write(&self.path, out)
            .with_context(|| format!("failed to write {}", self.path.display()))
    }
}
