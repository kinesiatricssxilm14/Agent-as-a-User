//! Directory listing model: reads a directory, sorts it, and applies the filter.

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Dir,
    File,
    Symlink,
    Other,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub kind: Kind,
    /// Byte size for files; `None` for directories and unreadable entries.
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    /// Unix permission bits, when available.
    pub mode: Option<u32>,
    /// Symlink target, when the entry is a symlink.
    pub link_target: Option<PathBuf>,
}

impl Entry {
    /// True when this entry behaves like a directory (including a symlink to one).
    pub fn is_dir_like(&self) -> bool {
        match self.kind {
            Kind::Dir => true,
            Kind::Symlink => self.path.is_dir(),
            _ => false,
        }
    }

    pub fn permissions(&self) -> String {
        match self.mode {
            Some(mode) => format_mode(mode, self.kind),
            None => "?????????".into(),
        }
    }
}

#[cfg(unix)]
fn format_mode(mode: u32, kind: Kind) -> String {
    let type_char = match kind {
        Kind::Dir => 'd',
        Kind::Symlink => 'l',
        Kind::File => '-',
        Kind::Other => '?',
    };
    let mut out = String::with_capacity(10);
    out.push(type_char);
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 0o7;
        out.push(if bits & 0o4 != 0 { 'r' } else { '-' });
        out.push(if bits & 0o2 != 0 { 'w' } else { '-' });
        out.push(if bits & 0o1 != 0 { 'x' } else { '-' });
    }
    out
}

#[cfg(not(unix))]
fn format_mode(_mode: u32, kind: Kind) -> String {
    match kind {
        Kind::Dir => "d---------".into(),
        _ => "----------".into(),
    }
}

/// Which column the list is ordered by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Name,
    Size,
    Modified,
}

impl SortKey {
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Name => "name",
            SortKey::Size => "size",
            SortKey::Modified => "mtime",
        }
    }

    pub fn next(self) -> Self {
        match self {
            SortKey::Name => SortKey::Size,
            SortKey::Size => SortKey::Modified,
            SortKey::Modified => SortKey::Name,
        }
    }
}

/// One directory's contents plus the view state applied to it.
#[derive(Debug)]
pub struct Listing {
    pub dir: PathBuf,
    /// Everything read from disk, already sorted.
    all: Vec<Entry>,
    /// Indices into `all` that pass the current filter.
    visible: Vec<usize>,
    pub filter: String,
    pub show_hidden: bool,
    pub sort_key: SortKey,
    pub sort_reverse: bool,
    /// Error captured while reading the directory, if any.
    pub error: Option<String>,
}

impl Listing {
    pub fn new(dir: PathBuf, show_hidden: bool) -> Self {
        let mut listing = Self {
            dir,
            all: Vec::new(),
            visible: Vec::new(),
            filter: String::new(),
            show_hidden,
            sort_key: SortKey::Name,
            sort_reverse: false,
            error: None,
        };
        listing.reload();
        listing
    }

    /// Re-read the directory from disk. Called after every mutating operation so
    /// the view always reflects real filesystem state.
    pub fn reload(&mut self) {
        self.all.clear();
        self.error = None;
        match fs::read_dir(&self.dir) {
            Ok(iter) => {
                for entry in iter.flatten() {
                    self.all.push(build_entry(&entry));
                }
            }
            Err(err) => {
                self.error = Some(format!("cannot read {}: {err}", self.dir.display()));
            }
        }
        self.sort();
        self.refilter();
    }

    fn sort(&mut self) {
        let key = self.sort_key;
        self.all.sort_by(|a, b| {
            // Directories always float above files, in both sort directions.
            let dir_order = b.is_dir_like().cmp(&a.is_dir_like());
            if dir_order != std::cmp::Ordering::Equal {
                return dir_order;
            }
            let ord = match key {
                SortKey::Name => natural_cmp(&a.name, &b.name),
                SortKey::Size => a.size.unwrap_or(0).cmp(&b.size.unwrap_or(0)),
                SortKey::Modified => a.modified.cmp(&b.modified),
            };
            if ord == std::cmp::Ordering::Equal {
                natural_cmp(&a.name, &b.name)
            } else {
                ord
            }
        });
        if self.sort_reverse {
            // Keep directories grouped first; reverse within each group.
            let split = self
                .all
                .iter()
                .position(|e| !e.is_dir_like())
                .unwrap_or(self.all.len());
            self.all[..split].reverse();
            self.all[split..].reverse();
        }
    }

    /// Recompute the visible index list from `filter` and `show_hidden`.
    pub fn refilter(&mut self) {
        let needle = self.filter.to_lowercase();
        self.visible = self
            .all
            .iter()
            .enumerate()
            .filter(|(_, e)| self.show_hidden || !e.name.starts_with('.'))
            .filter(|(_, e)| needle.is_empty() || e.name.to_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect();
    }

    pub fn set_sort(&mut self, key: SortKey) {
        self.sort_key = key;
        self.sort();
        self.refilter();
    }

    pub fn toggle_reverse(&mut self) {
        self.sort_reverse = !self.sort_reverse;
        self.sort();
        self.refilter();
    }

    pub fn set_hidden(&mut self, show: bool) {
        self.show_hidden = show;
        self.refilter();
    }

    pub fn len(&self) -> usize {
        self.visible.len()
    }

    pub fn is_empty(&self) -> bool {
        self.visible.is_empty()
    }

    /// Total number of entries on disk, before filtering.
    pub fn total(&self) -> usize {
        self.all.len()
    }

    pub fn get(&self, index: usize) -> Option<&Entry> {
        self.visible.get(index).and_then(|&i| self.all.get(i))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.visible.iter().filter_map(move |&i| self.all.get(i))
    }

    /// Position of `name` in the visible list, used to restore the cursor after
    /// a reload or when stepping back up into a parent directory.
    pub fn index_of_name(&self, name: &str) -> Option<usize> {
        self.iter().position(|e| e.name == name)
    }

    pub fn counts(&self) -> (usize, usize) {
        let dirs = self.iter().filter(|e| e.is_dir_like()).count();
        (dirs, self.len() - dirs)
    }
}

fn build_entry(entry: &fs::DirEntry) -> Entry {
    let path = entry.path();
    let name = entry.file_name().to_string_lossy().into_owned();
    // symlink_metadata so a link is reported as a link, not as its target.
    let meta = fs::symlink_metadata(&path);
    let (kind, size, modified, mode) = match &meta {
        Ok(m) => {
            let ft = m.file_type();
            let kind = if ft.is_symlink() {
                Kind::Symlink
            } else if ft.is_dir() {
                Kind::Dir
            } else if ft.is_file() {
                Kind::File
            } else {
                Kind::Other
            };
            let size = if matches!(kind, Kind::Dir) {
                None
            } else {
                Some(m.len())
            };
            (kind, size, m.modified().ok(), Some(mode_bits(m)))
        }
        Err(_) => (Kind::Other, None, None, None),
    };
    let link_target = if kind == Kind::Symlink {
        fs::read_link(&path).ok()
    } else {
        None
    };
    Entry {
        name,
        path,
        kind,
        size,
        modified,
        mode,
        link_target,
    }
}

#[cfg(unix)]
fn mode_bits(meta: &fs::Metadata) -> u32 {
    use std::os::unix::fs::MetadataExt;
    meta.mode()
}

#[cfg(not(unix))]
fn mode_bits(meta: &fs::Metadata) -> u32 {
    if meta.permissions().readonly() {
        0o444
    } else {
        0o644
    }
}

/// Case-insensitive comparison that orders embedded digit runs numerically, so
/// `file2.txt` sorts before `file10.txt`.
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let mut ai = a.chars().peekable();
    let mut bi = b.chars().peekable();
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) => {
                if x.is_ascii_digit() && y.is_ascii_digit() {
                    let xn = take_digits(&mut ai);
                    let yn = take_digits(&mut bi);
                    // Compare by length first to handle numbers wider than u64.
                    let ord = xn.len().cmp(&yn.len()).then_with(|| xn.cmp(&yn));
                    if ord != Ordering::Equal {
                        return ord;
                    }
                } else {
                    let ord = x.to_lowercase().cmp(y.to_lowercase());
                    if ord != Ordering::Equal {
                        return ord;
                    }
                    ai.next();
                    bi.next();
                }
            }
        }
    }
}

fn take_digits(iter: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut out = String::new();
    while let Some(&c) = iter.peek() {
        if c.is_ascii_digit() {
            out.push(c);
            iter.next();
        } else {
            break;
        }
    }
    // Drop leading zeros so "007" == "7" numerically.
    let trimmed = out.trim_start_matches('0');
    if trimmed.is_empty() {
        "0".into()
    } else {
        trimmed.to_string()
    }
}

/// Format a timestamp as `YYYY-MM-DD HH:MM` in UTC without pulling in a date crate.
pub fn format_time(time: Option<SystemTime>) -> String {
    let Some(time) = time else {
        return "     -           ".into();
    };
    let secs = match time.duration_since(SystemTime::UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };
    let (y, mo, d, h, mi) = civil_from_unix(secs);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}")
}

/// Days-from-civil algorithm (Howard Hinnant), inverted: seconds -> Y/M/D h:m.
fn civil_from_unix(secs: i64) -> (i64, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d, (rem / 3600) as u32, ((rem % 3600) / 60) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File};
    use std::io::Write;

    fn scratch(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("toolc-listing-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn lists_dirs_before_files_and_hides_dotfiles() {
        let root = scratch("basic");
        fs::create_dir(root.join("zdir")).unwrap();
        File::create(root.join("a.txt"))
            .unwrap()
            .write_all(b"a")
            .unwrap();
        File::create(root.join(".hidden"))
            .unwrap()
            .write_all(b"h")
            .unwrap();

        let mut listing = Listing::new(root.clone(), false);
        let names: Vec<_> = listing.iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, vec!["zdir", "a.txt"]);

        listing.set_hidden(true);
        let names: Vec<_> = listing.iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, vec!["zdir", ".hidden", "a.txt"]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn filter_is_case_insensitive_substring() {
        let root = scratch("filter");
        for name in ["Report.log", "notes.txt", "report_old.log"] {
            File::create(root.join(name)).unwrap();
        }
        let mut listing = Listing::new(root.clone(), false);
        listing.filter = "REPORT".into();
        listing.refilter();
        let names: Vec<_> = listing.iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, vec!["Report.log", "report_old.log"]);
        assert_eq!(listing.total(), 3);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn size_sort_orders_files() {
        let root = scratch("sort");
        File::create(root.join("small"))
            .unwrap()
            .write_all(b"x")
            .unwrap();
        File::create(root.join("big"))
            .unwrap()
            .write_all(&[0u8; 512])
            .unwrap();
        let mut listing = Listing::new(root.clone(), false);
        listing.set_sort(SortKey::Size);
        let names: Vec<_> = listing.iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, vec!["small", "big"]);
        listing.toggle_reverse();
        let names: Vec<_> = listing.iter().map(|e| e.name.clone()).collect();
        assert_eq!(names, vec!["big", "small"]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn natural_order_sorts_numbers_numerically() {
        let mut v = vec!["f10.txt", "f2.txt", "f1.txt"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["f1.txt", "f2.txt", "f10.txt"]);
        assert_eq!(natural_cmp("a", "B"), std::cmp::Ordering::Less);
    }

    #[test]
    fn unreadable_directory_records_error() {
        let listing = Listing::new(PathBuf::from("/definitely/not/here/toolc"), false);
        assert!(listing.error.is_some());
        assert!(listing.is_empty());
    }

    #[test]
    fn time_formatting_matches_known_epoch() {
        let t = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
        assert_eq!(format_time(Some(t)), "2023-11-14 22:13");
        assert_eq!(
            format_time(Some(SystemTime::UNIX_EPOCH)),
            "1970-01-01 00:00"
        );
    }
}
