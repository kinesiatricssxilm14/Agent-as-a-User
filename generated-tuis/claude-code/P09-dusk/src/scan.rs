//! Real filesystem scanning.
//!
//! The scanner walks the tree with `std::fs` only, using `symlink_metadata` so
//! symlinks are never followed (a symlink is reported with its own tiny size,
//! never the size of its target, and cannot introduce a cycle).

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// What a node in the tree is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
    Symlink,
}

impl Kind {
    pub fn is_dir(self) -> bool {
        matches!(self, Kind::Dir)
    }
}

/// One scanned filesystem entry plus its recursively accumulated statistics.
#[derive(Debug, Clone)]
pub struct Node {
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
    /// Recursive apparent size in bytes (own size for files).
    pub size: u64,
    /// Direct children, sorted by descending size then name.
    pub children: Vec<Node>,
    /// Files anywhere below this node (0 for a file).
    pub files_deep: u64,
    /// Directories anywhere below this node (0 for a file).
    pub dirs_deep: u64,
    /// Modification time, when the platform reports one.
    pub modified: Option<SystemTime>,
    /// Set when this directory could not be fully read (e.g. permission denied).
    pub error: Option<String>,
}

impl Node {
    /// Direct children that are files or symlinks.
    pub fn direct_files(&self) -> usize {
        self.children.iter().filter(|c| !c.kind.is_dir()).count()
    }

    /// Direct children that are directories.
    pub fn direct_dirs(&self) -> usize {
        self.children.iter().filter(|c| c.kind.is_dir()).count()
    }

    /// Total direct children.
    pub fn direct_entries(&self) -> usize {
        self.children.len()
    }

    /// Largest direct child, if any.
    pub fn largest_child(&self) -> Option<&Node> {
        self.children.iter().max_by_key(|c| c.size)
    }

    /// Look a node up by absolute path, walking down from this node.
    pub fn find(&self, path: &Path) -> Option<&Node> {
        if self.path == path {
            return Some(self);
        }
        let rest = path.strip_prefix(&self.path).ok()?;
        let mut cur = self;
        for part in rest.iter() {
            cur = cur
                .children
                .iter()
                .find(|c| c.name.as_str() == part.to_string_lossy())?;
        }
        Some(cur)
    }

    /// Mutable variant of [`Node::find`].
    pub fn find_mut(&mut self, path: &Path) -> Option<&mut Node> {
        if self.path == path {
            return Some(self);
        }
        let rest = path.strip_prefix(&self.path).ok()?.to_path_buf();
        let mut cur = self;
        for part in rest.iter() {
            let want = part.to_string_lossy().to_string();
            cur = cur.children.iter_mut().find(|c| c.name == want)?;
        }
        Some(cur)
    }

    /// Every descendant file (not directories), deepest-first order irrelevant.
    pub fn collect_files<'a>(&'a self, out: &mut Vec<&'a Node>) {
        for child in &self.children {
            if child.kind.is_dir() {
                child.collect_files(out);
            } else {
                out.push(child);
            }
        }
    }

    /// Every descendant, files and directories alike.
    pub fn collect_all<'a>(&'a self, out: &mut Vec<&'a Node>) {
        for child in &self.children {
            out.push(child);
            if child.kind.is_dir() {
                child.collect_all(out);
            }
        }
    }

    /// Recompute this node's aggregates from its children. Used after a delete
    /// so the in-memory tree matches the filesystem without a full rescan.
    pub fn recompute(&mut self) {
        if !self.kind.is_dir() {
            return;
        }
        let mut size = 0;
        let mut files = 0;
        let mut dirs = 0;
        for child in &self.children {
            size += child.size;
            if child.kind.is_dir() {
                dirs += 1 + child.dirs_deep;
                files += child.files_deep;
            } else {
                files += 1;
            }
        }
        self.size = size;
        self.files_deep = files;
        self.dirs_deep = dirs;
    }
}

/// Statistics about one scan pass.
#[derive(Debug, Clone, Default)]
pub struct ScanStats {
    pub entries: u64,
    pub errors: u64,
}

/// Recursively scan `root`, returning the tree and scan statistics.
pub fn scan(root: &Path) -> io::Result<(Node, ScanStats)> {
    let meta = fs::symlink_metadata(root)?;
    let mut stats = ScanStats::default();
    let name = display_name(root);

    if !meta.is_dir() {
        stats.entries = 1;
        return Ok((
            Node {
                name,
                path: root.to_path_buf(),
                kind: kind_of(&meta),
                size: meta.len(),
                children: Vec::new(),
                files_deep: 0,
                dirs_deep: 0,
                modified: meta.modified().ok(),
                error: None,
            },
            stats,
        ));
    }

    // Guard against directory cycles created by bind mounts.
    let mut visited = HashSet::new();
    let mut node = walk(root, name, &meta, &mut stats, &mut visited);
    node.children.sort_by(cmp_entries);
    Ok((node, stats))
}

fn walk(
    path: &Path,
    name: String,
    meta: &fs::Metadata,
    stats: &mut ScanStats,
    visited: &mut HashSet<(u64, u64)>,
) -> Node {
    stats.entries += 1;

    let mut node = Node {
        name,
        path: path.to_path_buf(),
        kind: Kind::Dir,
        size: 0,
        children: Vec::new(),
        files_deep: 0,
        dirs_deep: 0,
        modified: meta.modified().ok(),
        error: None,
    };

    if let Some(id) = dir_identity(meta) {
        if !visited.insert(id) {
            node.error = Some("already visited (cycle)".to_string());
            return node;
        }
    }

    let reader = match fs::read_dir(path) {
        Ok(reader) => reader,
        Err(err) => {
            stats.errors += 1;
            node.error = Some(err.to_string());
            return node;
        }
    };

    for entry in reader {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                stats.errors += 1;
                node.error = Some(err.to_string());
                continue;
            }
        };
        let child_path = entry.path();
        let child_name = entry.file_name().to_string_lossy().to_string();
        let child_meta = match fs::symlink_metadata(&child_path) {
            Ok(meta) => meta,
            Err(err) => {
                stats.errors += 1;
                stats.entries += 1;
                node.children.push(Node {
                    name: child_name,
                    path: child_path,
                    kind: Kind::File,
                    size: 0,
                    children: Vec::new(),
                    files_deep: 0,
                    dirs_deep: 0,
                    modified: None,
                    error: Some(err.to_string()),
                });
                node.files_deep += 1;
                continue;
            }
        };

        if child_meta.is_dir() {
            let mut child = walk(&child_path, child_name, &child_meta, stats, visited);
            child.children.sort_by(cmp_entries);
            node.size += child.size;
            node.files_deep += child.files_deep;
            node.dirs_deep += child.dirs_deep + 1;
            node.children.push(child);
        } else {
            stats.entries += 1;
            node.size += child_meta.len();
            node.files_deep += 1;
            node.children.push(Node {
                name: child_name,
                path: child_path,
                kind: kind_of(&child_meta),
                size: child_meta.len(),
                children: Vec::new(),
                files_deep: 0,
                dirs_deep: 0,
                modified: child_meta.modified().ok(),
                error: None,
            });
        }
    }

    node
}

/// Sort order used everywhere a "natural" order is wanted: biggest first, ties
/// broken by case-insensitive name so the display is stable across rescans.
pub fn cmp_entries(a: &Node, b: &Node) -> std::cmp::Ordering {
    b.size
        .cmp(&a.size)
        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        .then_with(|| a.name.cmp(&b.name))
}

fn kind_of(meta: &fs::Metadata) -> Kind {
    if meta.is_dir() {
        Kind::Dir
    } else if meta.file_type().is_symlink() {
        Kind::Symlink
    } else {
        Kind::File
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

#[cfg(unix)]
fn dir_identity(meta: &fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn dir_identity(_meta: &fs::Metadata) -> Option<(u64, u64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    fn tmpdir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tooli-scan-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, bytes: usize) {
        let mut f = File::create(path).unwrap();
        f.write_all(&vec![b'x'; bytes]).unwrap();
    }

    #[test]
    fn sizes_and_counts_aggregate_recursively() {
        let root = tmpdir("agg");
        fs::create_dir_all(root.join("sub/deep")).unwrap();
        write(&root.join("a.bin"), 3000);
        write(&root.join("sub/b.bin"), 2000);
        write(&root.join("sub/deep/c.bin"), 1000);

        let (tree, stats) = scan(&root).unwrap();
        assert_eq!(tree.size, 6000);
        assert_eq!(tree.files_deep, 3);
        assert_eq!(tree.dirs_deep, 2);
        assert_eq!(tree.direct_files(), 1);
        assert_eq!(tree.direct_dirs(), 1);
        assert!(stats.entries >= 6);

        // Children are ordered biggest-first: sub (3000) ties a.bin (3000) so
        // the name breaks the tie.
        let sub = tree.find(&root.join("sub")).unwrap();
        assert_eq!(sub.size, 3000);
        assert_eq!(sub.files_deep, 2);

        let mut files = Vec::new();
        tree.collect_files(&mut files);
        assert_eq!(files.len(), 3);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn recompute_matches_a_fresh_scan_after_delete() {
        let root = tmpdir("recompute");
        write(&root.join("keep.bin"), 500);
        write(&root.join("drop.bin"), 700);

        let (mut tree, _) = scan(&root).unwrap();
        assert_eq!(tree.size, 1200);

        fs::remove_file(root.join("drop.bin")).unwrap();
        tree.children.retain(|c| c.name != "drop.bin");
        tree.recompute();

        let (fresh, _) = scan(&root).unwrap();
        assert_eq!(tree.size, fresh.size);
        assert_eq!(tree.files_deep, fresh.files_deep);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn symlinks_are_not_followed() {
        let root = tmpdir("symlink");
        fs::create_dir_all(root.join("real")).unwrap();
        write(&root.join("real/big.bin"), 4096);
        #[cfg(unix)]
        std::os::unix::fs::symlink(root.join("real"), root.join("link")).unwrap();

        let (tree, _) = scan(&root).unwrap();
        #[cfg(unix)]
        {
            let link = tree.find(&root.join("link")).unwrap();
            assert_eq!(link.kind, Kind::Symlink);
            assert!(link.size < 4096);
        }
        assert_eq!(tree.files_deep, if cfg!(unix) { 2 } else { 1 });

        fs::remove_dir_all(&root).unwrap();
    }
}
