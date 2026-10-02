use std::{
    cmp::Reverse,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug)]
pub struct Node {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub children: Vec<Node>,
}

impl Node {
    pub fn scan(path: &Path) -> io::Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());

        if !metadata.is_dir() {
            return Ok(Self {
                path: path.to_path_buf(),
                name,
                size: metadata.len(),
                is_dir: false,
                children: Vec::new(),
            });
        }

        let mut children = Vec::new();
        let mut size = 0_u64;
        if let Ok(entries) = fs::read_dir(path) {
            for entry in entries.flatten() {
                let child_path = entry.path();
                // Never follow symbolic links: this avoids loops and crossing into unexpected trees.
                let Ok(child_meta) = fs::symlink_metadata(&child_path) else {
                    continue;
                };
                if child_meta.file_type().is_symlink() {
                    children.push(Self {
                        name: entry.file_name().to_string_lossy().into_owned(),
                        path: child_path,
                        size: child_meta.len(),
                        is_dir: false,
                        children: Vec::new(),
                    });
                    size = size.saturating_add(child_meta.len());
                    continue;
                }
                if let Ok(child) = Self::scan(&child_path) {
                    size = size.saturating_add(child.size);
                    children.push(child);
                }
            }
        }

        children.sort_by_key(|node| Reverse(node.size));
        Ok(Self {
            path: path.to_path_buf(),
            name,
            size,
            is_dir: true,
            children,
        })
    }

    pub fn find(&self, path: &Path) -> Option<&Node> {
        if self.path == path {
            return Some(self);
        }
        self.children.iter().find_map(|child| child.find(path))
    }

    pub fn direct_counts(&self) -> (usize, usize) {
        let files = self.children.iter().filter(|node| !node.is_dir).count();
        let dirs = self.children.iter().filter(|node| node.is_dir).count();
        (files, dirs)
    }

    pub fn largest_child(&self) -> Option<&Node> {
        self.children.iter().max_by_key(|node| node.size)
    }

    pub fn collect_files<'a>(&'a self, output: &mut Vec<&'a Node>) {
        if self.is_dir {
            for child in &self.children {
                child.collect_files(output);
            }
        } else {
            output.push(self);
        }
    }
}

pub fn format_size(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;
    let bytes = bytes as f64;
    if bytes >= TB {
        format!("{:.2} TB", bytes / TB)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes / KB)
    } else {
        format!("{bytes:.2} B")
    }
}

pub fn format_gb(bytes: u64) -> String {
    format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
}

pub fn parse_size(input: &str) -> Result<u64, String> {
    let normalized = input.trim().to_ascii_lowercase().replace(' ', "");
    if normalized.is_empty() || normalized == "0" || normalized == "off" {
        return Ok(0);
    }
    let split = normalized
        .find(|ch: char| !ch.is_ascii_digit() && ch != '.')
        .unwrap_or(normalized.len());
    let (number, unit) = normalized.split_at(split);
    let value: f64 = number
        .parse()
        .map_err(|_| "Use a number followed by B, KB, MB, GB, or TB".to_string())?;
    if !value.is_finite() || value < 0.0 {
        return Err("Size must be a non-negative finite number".to_string());
    }
    let multiplier = match unit {
        "" | "b" => 1.0,
        "k" | "kb" | "kib" => 1024.0,
        "m" | "mb" | "mib" => 1024.0 * 1024.0,
        "g" | "gb" | "gib" => 1024.0 * 1024.0 * 1024.0,
        "t" | "tb" | "tib" => 1024.0 * 1024.0 * 1024.0 * 1024.0,
        _ => return Err("Unknown unit; use B, KB, MB, GB, or TB".to_string()),
    };
    let bytes = value * multiplier;
    if bytes > u64::MAX as f64 {
        return Err("Size is too large".to_string());
    }
    Ok(bytes.round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_human_sizes() {
        assert_eq!(parse_size("1 MB").unwrap(), 1_048_576);
        assert_eq!(parse_size("1.5gb").unwrap(), 1_610_612_736);
        assert_eq!(parse_size("off").unwrap(), 0);
    }

    #[test]
    fn formats_with_two_decimals() {
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_gb(0), "0.00 GB");
    }
}
