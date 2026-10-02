//! Real filesystem scanning.
//!
//! Every size shown by the tool comes from `std::fs` metadata, so the numbers
//! always reflect the actual state of the filesystem. Symlinks are never
//! followed: they are treated as plain entries whose size is the length of the
//! link target string, which guarantees the recursion always terminates.

use std::fs;
use std::path::Path;

use crate::model::Node;

/// Recursively scan `path`, appending every discovered node to `nodes`.
///
/// Returns `(id, total_size)` where `total_size` is the recursive size of the
/// subtree rooted at `path`.
pub fn scan_subtree(
    path: &Path,
    name: &str,
    is_dir: bool,
    parent: Option<usize>,
    nodes: &mut Vec<Node>,
) -> (usize, u64) {
    let id = nodes.len();
    // Push a placeholder first so the parent id is stable while we recurse.
    nodes.push(Node {
        name: name.to_string(),
        path: path.to_path_buf(),
        size: 0,
        is_dir,
        parent,
        children: Vec::new(),
    });

    let mut total: u64 = 0;

    if is_dir {
        if let Ok(read_dir) = fs::read_dir(path) {
            let mut entries: Vec<fs::DirEntry> = read_dir.filter_map(|e| e.ok()).collect();
            // Deterministic scan order; display order is controlled separately.
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                let child_path = entry.path();
                let child_name = entry.file_name().to_string_lossy().into_owned();
                let meta = match fs::symlink_metadata(&child_path) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                let child_is_dir = meta.is_dir();
                let (child_id, child_size) =
                    scan_subtree(&child_path, &child_name, child_is_dir, Some(id), nodes);
                nodes[id].children.push(child_id);
                total = total.saturating_add(child_size);
            }
        }
    } else {
        total = fs::symlink_metadata(path).map(|m| m.len()).unwrap_or(0);
    }

    nodes[id].size = total;
    (id, total)
}

/// Find a node id whose `path` equals `path`, searching the subtree rooted at
/// `root` depth-first. Returns `None` when the path no longer exists.
pub fn find_node_by_path(nodes: &[Node], root: usize, path: &Path) -> Option<usize> {
    if nodes[root].path == path {
        return Some(root);
    }
    for &child in &nodes[root].children {
        if let Some(found) = find_node_by_path(nodes, child, path) {
            return Some(found);
        }
    }
    None
}
