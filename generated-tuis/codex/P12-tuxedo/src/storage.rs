use crate::model::Task;
use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

pub fn load(path: &Path) -> Result<Vec<Task>> {
    if !path.exists() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        File::create(path).with_context(|| format!("could not create {}", path.display()))?;
        return Ok(Vec::new());
    }

    let text =
        fs::read_to_string(path).with_context(|| format!("could not read {}", path.display()))?;
    Ok(text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(Task::parse)
        .collect())
}

pub fn save(path: &Path, tasks: &[Task]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }

    let temp_path = temporary_path(path);
    {
        let file = File::create(&temp_path)
            .with_context(|| format!("could not create {}", temp_path.display()))?;
        let mut writer = BufWriter::new(file);
        for task in tasks {
            writeln!(writer, "{task}")?;
        }
        writer.flush()?;
        writer.get_ref().sync_all()?;
    }
    if let Err(error) = fs::rename(&temp_path, path) {
        // Windows does not replace an existing destination with rename. The
        // benchmark target is Debian, but this fallback keeps local use sane.
        if path.exists() {
            fs::remove_file(path)
                .with_context(|| format!("could not replace {}", path.display()))?;
            fs::rename(&temp_path, path)
                .with_context(|| format!("could not replace {}", path.display()))?;
        } else {
            return Err(error).with_context(|| format!("could not replace {}", path.display()));
        }
    }
    Ok(())
}

fn temporary_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("todo.txt");
    path.with_file_name(format!(".{file_name}.tooll.tmp"))
}
