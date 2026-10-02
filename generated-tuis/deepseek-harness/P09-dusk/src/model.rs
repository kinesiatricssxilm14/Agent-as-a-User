//! Data model for the directory tree.
//!
//! The whole filesystem subtree under the scan root is kept in memory inside an
//! arena (`Vec<Node>`). Each node stores its absolute path, its recursive size,
//! whether it is a directory, a link to its parent and the list of child node
//! ids. Navigating is just moving the `current_id` pointer around the arena.

use std::path::PathBuf;

/// A single file or directory node in the tree.
#[derive(Debug, Clone)]
pub struct Node {
    /// Entry name (basename, without path separators).
    pub name: String,
    /// Absolute path on disk.
    pub path: PathBuf,
    /// Recursive size in bytes: for files the file size, for directories the
    /// sum of every descendant's size.
    pub size: u64,
    /// True for directories, false for files (and symlinks, which are treated
    /// as files so that symlink cycles can never cause infinite recursion).
    pub is_dir: bool,
    /// Arena id of the parent directory, if any.
    pub parent: Option<usize>,
    /// Arena ids of the direct children (only meaningful for directories).
    pub children: Vec<usize>,
}

/// How the current directory's entries are ordered on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortMode {
    /// Largest first.
    SizeDesc,
    /// Smallest first.
    SizeAsc,
    /// Alphabetical (case-insensitive).
    NameAsc,
}

impl SortMode {
    /// Advance to the next sort mode in a cyclic order.
    pub fn next(self) -> Self {
        match self {
            SortMode::SizeDesc => SortMode::SizeAsc,
            SortMode::SizeAsc => SortMode::NameAsc,
            SortMode::NameAsc => SortMode::SizeDesc,
        }
    }

    /// Human readable label used in the header/help.
    pub fn label(self) -> &'static str {
        match self {
            SortMode::SizeDesc => "size \u{2193}",
            SortMode::SizeAsc => "size \u{2191}",
            SortMode::NameAsc => "name \u{2191}",
        }
    }
}

/// The in-memory tree plus the cursor position.
#[derive(Debug)]
pub struct Model {
    /// Arena storage of all nodes.
    pub nodes: Vec<Node>,
    /// Id of the scan root.
    pub root_id: usize,
    /// Id of the directory currently being browsed.
    pub current_id: usize,
}
