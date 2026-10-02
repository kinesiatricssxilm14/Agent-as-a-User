//! Real filesystem operations backing the TUI.
//!
//! Every function in this module performs an actual syscall-level operation via
//! `std::fs` — nothing here is simulated. User-facing errors are returned as
//! `String` so the UI can display them verbatim in the status line.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

pub type OpResult<T> = Result<T, String>;

/// Result of an operation that may need the user to confirm an overwrite.
#[derive(Debug)]
pub enum Outcome {
    /// The operation completed; the payload is the resulting path.
    Done(PathBuf),
    /// Nothing was written because `PathBuf` already exists.
    NeedsOverwrite(PathBuf),
}

/// Expand a leading `~` (or `~/...`) using `$HOME`.
fn expand_tilde(input: &str) -> String {
    if input == "~" || input.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            let home = PathBuf::from(home);
            let rest = input
                .strip_prefix('~')
                .unwrap_or("")
                .trim_start_matches('/');
            return home.join(rest).to_string_lossy().into_owned();
        }
    }
    input.to_string()
}

/// Lexically normalise a path: drop `.` components and fold `..` where possible.
/// This never touches the filesystem, so it is predictable for the user typing
/// into the prompt, and it keeps absolute paths absolute.
pub fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                match out.components().next_back() {
                    // A real component above us: `..` cancels it out.
                    Some(Component::Normal(_)) => {
                        out.pop();
                    }
                    // `/..` is `/`, so drop it once we are at an absolute root.
                    Some(Component::RootDir) => {}
                    // Relative so far (empty, or already a run of `..`): keep it.
                    _ => out.push(".."),
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        out.push(".");
    }
    out
}

/// A destination as typed by the user.
#[derive(Debug, Clone)]
pub struct Target {
    /// Absolute, lexically normalised path.
    pub path: PathBuf,
    /// True when the user typed a trailing `/`, i.e. explicitly asked for a directory.
    pub dir_hint: bool,
}

/// Turn raw user input into an absolute target, relative to `cwd`.
pub fn resolve(cwd: &Path, input: &str) -> OpResult<Target> {
    let raw = input.trim();
    if raw.is_empty() {
        return Err("path is empty".into());
    }
    let dir_hint = raw.ends_with('/') || raw.ends_with("/.");
    let expanded = expand_tilde(raw);
    let candidate = Path::new(&expanded);
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        cwd.join(candidate)
    };
    Ok(Target {
        path: normalize(&joined),
        dir_hint,
    })
}

fn name_of(path: &Path) -> OpResult<&std::ffi::OsStr> {
    path.file_name()
        .ok_or_else(|| format!("{} has no file name", path.display()))
}

fn io_err(action: &str, path: &Path, err: &io::Error) -> String {
    format!("{action} {}: {err}", path.display())
}

/// Resolve where a copy/move of `src` should land given the user's `target`.
///
/// Mirrors `cp`/`mv` semantics: if the destination is an existing directory the
/// source keeps its name inside it, otherwise the destination *is* the new path.
fn final_destination(src: &Path, target: &Target) -> OpResult<PathBuf> {
    let dst_meta = fs::symlink_metadata(&target.path);
    match dst_meta {
        Ok(meta) if meta.is_dir() => Ok(target.path.join(name_of(src)?)),
        Ok(_) if target.dir_hint => Err(format!(
            "{} exists but is not a directory",
            target.path.display()
        )),
        Ok(_) => Ok(target.path.clone()),
        Err(_) if target.dir_hint => Err(format!(
            "directory {} does not exist (create it first with `n`)",
            target.path.display()
        )),
        Err(_) => {
            let parent = target.path.parent().unwrap_or(Path::new("/"));
            if parent.is_dir() {
                Ok(target.path.clone())
            } else {
                Err(format!("directory {} does not exist", parent.display()))
            }
        }
    }
}

/// True when `ancestor` is `descendant` or one of its parents (lexically).
fn is_ancestor(ancestor: &Path, descendant: &Path) -> bool {
    let a = fs::canonicalize(ancestor).unwrap_or_else(|_| normalize(ancestor));
    let d = fs::canonicalize(descendant).unwrap_or_else(|_| normalize(descendant));
    d.starts_with(&a)
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Copy `src` to the user-supplied `target`. Files are copied byte-for-byte
/// (`std::fs::copy`), directories are copied recursively.
pub fn copy(src: &Path, target: &Target, overwrite: bool) -> OpResult<Outcome> {
    let dst = final_destination(src, target)?;
    let src_meta = fs::symlink_metadata(src).map_err(|e| io_err("cannot read", src, &e))?;

    if same_file(src, &dst) {
        return Err(format!(
            "{} and {} are the same file",
            src.display(),
            dst.display()
        ));
    }
    if src_meta.is_dir() && is_ancestor(src, &dst) {
        return Err(format!("cannot copy {} into itself", src.display()));
    }

    let exists = fs::symlink_metadata(&dst).is_ok();
    if exists && !overwrite {
        return Ok(Outcome::NeedsOverwrite(dst));
    }
    if exists {
        let dst_is_dir = fs::symlink_metadata(&dst)
            .map(|m| m.is_dir())
            .unwrap_or(false);
        if dst_is_dir != src_meta.is_dir() {
            return Err(format!(
                "refusing to replace {} ({} vs {})",
                dst.display(),
                if dst_is_dir { "directory" } else { "file" },
                if src_meta.is_dir() {
                    "directory"
                } else {
                    "file"
                }
            ));
        }
        if !dst_is_dir {
            fs::remove_file(&dst).map_err(|e| io_err("cannot replace", &dst, &e))?;
        }
    }

    if src_meta.is_dir() {
        copy_dir_recursive(src, &dst)?;
    } else {
        fs::copy(src, &dst)
            .map_err(|e| format!("cannot copy {} -> {}: {e}", src.display(), dst.display()))?;
    }
    Ok(Outcome::Done(dst))
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> OpResult<()> {
    fs::create_dir_all(dst).map_err(|e| io_err("cannot create", dst, &e))?;
    let entries = fs::read_dir(src).map_err(|e| io_err("cannot read", src, &e))?;
    for entry in entries {
        let entry = entry.map_err(|e| io_err("cannot read entry in", src, &e))?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        let meta = fs::symlink_metadata(&from).map_err(|e| io_err("cannot read", &from, &e))?;
        if meta.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if meta.file_type().is_symlink() {
            copy_symlink(&from, &to)?;
        } else {
            if fs::symlink_metadata(&to).is_ok() {
                let _ = fs::remove_file(&to);
            }
            fs::copy(&from, &to)
                .map_err(|e| format!("cannot copy {} -> {}: {e}", from.display(), to.display()))?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn copy_symlink(from: &Path, to: &Path) -> OpResult<()> {
    let link = fs::read_link(from).map_err(|e| io_err("cannot read link", from, &e))?;
    if fs::symlink_metadata(to).is_ok() {
        let _ = fs::remove_file(to);
    }
    std::os::unix::fs::symlink(&link, to).map_err(|e| io_err("cannot create link", to, &e))
}

#[cfg(not(unix))]
fn copy_symlink(from: &Path, to: &Path) -> OpResult<()> {
    fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| io_err("cannot copy", from, &e))
}

/// Move `src` to `target`. Uses `rename(2)` and falls back to copy+delete when
/// the two paths live on different filesystems (EXDEV), exactly like `mv`.
pub fn move_path(src: &Path, target: &Target, overwrite: bool) -> OpResult<Outcome> {
    let dst = final_destination(src, target)?;
    if same_file(src, &dst) {
        return Err(format!(
            "{} and {} are the same file",
            src.display(),
            dst.display()
        ));
    }
    let src_meta = fs::symlink_metadata(src).map_err(|e| io_err("cannot read", src, &e))?;
    if src_meta.is_dir() && is_ancestor(src, &dst) {
        return Err(format!("cannot move {} into itself", src.display()));
    }

    let exists = fs::symlink_metadata(&dst).is_ok();
    if exists && !overwrite {
        return Ok(Outcome::NeedsOverwrite(dst));
    }
    if exists {
        delete(&dst)?;
    }

    match fs::rename(src, &dst) {
        Ok(()) => Ok(Outcome::Done(dst)),
        Err(err) if is_cross_device(&err) => {
            // Different mount points: copy then remove the original.
            if src_meta.is_dir() {
                copy_dir_recursive(src, &dst)?;
            } else {
                fs::copy(src, &dst).map_err(|e| {
                    format!("cannot copy {} -> {}: {e}", src.display(), dst.display())
                })?;
            }
            delete(src)?;
            Ok(Outcome::Done(dst))
        }
        Err(err) => Err(format!(
            "cannot move {} -> {}: {err}",
            src.display(),
            dst.display()
        )),
    }
}

fn is_cross_device(err: &io::Error) -> bool {
    // EXDEV is 18 on Linux and macOS.
    err.raw_os_error() == Some(18)
}

/// Create a directory (and any missing parents), like `mkdir -p`.
pub fn create_dir(target: &Path) -> OpResult<PathBuf> {
    if let Ok(meta) = fs::symlink_metadata(target) {
        return Err(format!(
            "{} already exists ({})",
            target.display(),
            if meta.is_dir() { "directory" } else { "file" }
        ));
    }
    fs::create_dir_all(target).map_err(|e| io_err("cannot create directory", target, &e))?;
    Ok(target.to_path_buf())
}

/// Delete a file, symlink, or directory tree.
pub fn delete(path: &Path) -> OpResult<()> {
    let meta = fs::symlink_metadata(path).map_err(|e| io_err("cannot read", path, &e))?;
    if path.parent().is_none() {
        return Err("refusing to delete the filesystem root".into());
    }
    if meta.is_dir() && !meta.file_type().is_symlink() {
        fs::remove_dir_all(path).map_err(|e| io_err("cannot delete directory", path, &e))
    } else {
        fs::remove_file(path).map_err(|e| io_err("cannot delete", path, &e))
    }
}

/// Human readable byte count.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if value < 10.0 {
        format!("{value:.2} {}", UNITS[unit])
    } else if value < 100.0 {
        format!("{value:.1} {}", UNITS[unit])
    } else {
        format!("{value:.0} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Minimal unique scratch directory (avoids pulling in a temp-dir crate).
    fn scratch(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let uniq = format!(
            "toolc-test-{tag}-{}-{:p}",
            std::process::id(),
            &tag as *const _
        );
        dir.push(uniq);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, body: &[u8]) {
        let mut f = fs::File::create(path).unwrap();
        f.write_all(body).unwrap();
    }

    #[test]
    fn copy_produces_byte_identical_file() {
        let root = scratch("copy");
        let src = root.join("a.bin");
        let body: Vec<u8> = (0u8..=255).cycle().take(5000).collect();
        write(&src, &body);

        let target = resolve(&root, "nested/b.bin").unwrap();
        fs::create_dir_all(root.join("nested")).unwrap();
        let out = copy(&src, &target, false).unwrap();
        let dst = match out {
            Outcome::Done(p) => p,
            other => panic!("unexpected {other:?}"),
        };
        assert_eq!(fs::read(&dst).unwrap(), body);
        assert!(src.exists(), "copy must keep the source");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn copy_into_existing_directory_keeps_name() {
        let root = scratch("copydir");
        let src = root.join("note.txt");
        write(&src, b"hello");
        fs::create_dir_all(root.join("archive")).unwrap();

        let target = resolve(&root, "archive").unwrap();
        let Outcome::Done(dst) = copy(&src, &target, false).unwrap() else {
            panic!("expected completion");
        };
        assert_eq!(dst, root.join("archive/note.txt"));
        assert_eq!(fs::read_to_string(&dst).unwrap(), "hello");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn existing_destination_requests_overwrite() {
        let root = scratch("overwrite");
        write(&root.join("a.txt"), b"new");
        write(&root.join("b.txt"), b"old");
        let target = resolve(&root, "b.txt").unwrap();
        match copy(&root.join("a.txt"), &target, false).unwrap() {
            Outcome::NeedsOverwrite(p) => assert_eq!(p, root.join("b.txt")),
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "old");
        let Outcome::Done(_) = copy(&root.join("a.txt"), &target, true).unwrap() else {
            panic!("expected completion");
        };
        assert_eq!(fs::read_to_string(root.join("b.txt")).unwrap(), "new");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn move_removes_original_path() {
        let root = scratch("move");
        let src = root.join("report.log");
        write(&src, b"payload");
        let dest_dir = root.join("archive");
        create_dir(&dest_dir).unwrap();

        let target = resolve(&root, "archive/").unwrap();
        let Outcome::Done(dst) = move_path(&src, &target, false).unwrap() else {
            panic!("expected completion");
        };
        assert_eq!(dst, dest_dir.join("report.log"));
        assert!(!src.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "payload");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn rename_keeps_content() {
        let root = scratch("rename");
        let src = root.join("old.conf");
        write(&src, b"key=value\n");
        // A rename is a move to a new name inside the same directory.
        let target = resolve(&root, "new.conf").unwrap();
        let Outcome::Done(dst) = move_path(&src, &target, false).unwrap() else {
            panic!("expected completion");
        };
        assert_eq!(dst, root.join("new.conf"));
        assert!(!src.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "key=value\n");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn delete_handles_files_and_trees() {
        let root = scratch("delete");
        let file = root.join("x.txt");
        write(&file, b"x");
        delete(&file).unwrap();
        assert!(!file.exists());

        let tree = root.join("tree/inner");
        fs::create_dir_all(&tree).unwrap();
        write(&tree.join("deep.txt"), b"deep");
        delete(&root.join("tree")).unwrap();
        assert!(!root.join("tree").exists());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn recursive_copy_walks_the_tree() {
        let root = scratch("rcopy");
        fs::create_dir_all(root.join("src/inner")).unwrap();
        write(&root.join("src/top.txt"), b"top");
        write(&root.join("src/inner/leaf.txt"), b"leaf");
        let target = resolve(&root, "copy").unwrap();
        let Outcome::Done(dst) = copy(&root.join("src"), &target, false).unwrap() else {
            panic!("expected completion");
        };
        assert_eq!(fs::read_to_string(dst.join("top.txt")).unwrap(), "top");
        assert_eq!(
            fs::read_to_string(dst.join("inner/leaf.txt")).unwrap(),
            "leaf"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn resolve_handles_absolute_relative_and_dotdot() {
        let cwd = Path::new("/bench/data/src");
        assert_eq!(
            resolve(cwd, "a.txt").unwrap().path,
            Path::new("/bench/data/src/a.txt")
        );
        assert_eq!(resolve(cwd, "/tmp/a").unwrap().path, Path::new("/tmp/a"));
        assert_eq!(
            resolve(cwd, "../dst/a").unwrap().path,
            Path::new("/bench/data/dst/a")
        );
        assert!(resolve(cwd, "arch/").unwrap().dir_hint);
        assert!(resolve(cwd, "   ").is_err());
    }

    #[test]
    fn cannot_copy_directory_into_itself() {
        let root = scratch("selfcopy");
        fs::create_dir_all(root.join("d")).unwrap();
        let target = resolve(&root, "d/inner").unwrap();
        assert!(copy(&root.join("d"), &target, false).is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn normalize_folds_dot_and_dotdot() {
        assert_eq!(normalize(Path::new("/a/./b")), Path::new("/a/b"));
        assert_eq!(normalize(Path::new("/a/b/../c")), Path::new("/a/c"));
        // `..` cannot escape the root.
        assert_eq!(normalize(Path::new("/../..")), Path::new("/"));
        // Relative paths keep leading `..` because it cannot be resolved lexically.
        assert_eq!(normalize(Path::new("../x")), Path::new("../x"));
        assert_eq!(normalize(Path::new("../../x")), Path::new("../../x"));
        assert_eq!(normalize(Path::new(".")), Path::new("."));
    }

    #[test]
    fn human_size_scales() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1024), "1.00 KiB");
        assert_eq!(human_size(1024 * 1024 * 3 / 2), "1.50 MiB");
    }
}
