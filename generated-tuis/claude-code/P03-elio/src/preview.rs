//! File preview: loads real file bytes and renders them as text or a hex dump.
//!
//! The whole file is loaded (up to a generous cap) and the viewport scrolls over
//! it, so the complete content is reachable in the same view without paging into
//! a separate screen.

use std::fs;
use std::path::{Path, PathBuf};

use unicode_width::UnicodeWidthStr;

use crate::fs_ops::human_size;
use crate::listing::{Entry, Kind};

/// Files larger than this are loaded only up to the cap; the preview says so.
pub const MAX_PREVIEW_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewKind {
    Text,
    Binary,
    Directory,
    Empty,
    Error,
    Nothing,
}

#[derive(Debug)]
pub struct Preview {
    /// The path this preview was built from; used to avoid redundant reloads.
    pub path: Option<PathBuf>,
    /// Bare file name, shown in the preview title.
    pub name: String,
    pub kind: PreviewKind,
    /// Rendered lines, ready to display.
    pub lines: Vec<String>,
    /// One-line summary: size, line count, encoding notes.
    pub info: String,
    /// True when the file was truncated at `MAX_PREVIEW_BYTES`.
    pub truncated: bool,
    /// Byte size on disk.
    pub size: u64,
    /// First scrolled-to line.
    pub scroll: u16,
    /// Horizontal scroll offset, for long lines.
    pub hscroll: u16,
    /// Soft-wrap long lines instead of scrolling horizontally.
    pub wrap: bool,
    /// Cache of `lines` broken to the viewport width. The preview wraps its own
    /// text rather than delegating to `Wrap`, so the scroll offset, the scroll
    /// limit and the scrollbar all agree with exactly what is drawn — with
    /// `Wrap` the row count can only be guessed, and guessing low makes the end
    /// of a long-lined file unreachable.
    rows: Vec<String>,
    /// The `(width, wrap)` the cache was built for.
    rows_key: Option<(u16, bool)>,
}

impl Default for Preview {
    fn default() -> Self {
        Self {
            path: None,
            name: String::new(),
            kind: PreviewKind::Nothing,
            lines: Vec::new(),
            info: String::new(),
            truncated: false,
            size: 0,
            scroll: 0,
            hscroll: 0,
            wrap: true,
            rows: Vec::new(),
            rows_key: None,
        }
    }
}

impl Preview {
    /// Placeholder shown when the directory is empty or nothing is selected.
    pub fn empty_selection(reason: &str) -> Self {
        Self {
            kind: PreviewKind::Nothing,
            lines: vec![reason.to_string()],
            info: String::new(),
            ..Default::default()
        }
    }

    /// Build a preview for `entry`, preserving the caller's wrap preference.
    pub fn load(entry: &Entry, wrap: bool) -> Self {
        let mut preview = Self {
            path: Some(entry.path.clone()),
            name: entry.name.clone(),
            wrap,
            ..Default::default()
        };

        if entry.kind == Kind::Symlink {
            if let Some(target) = &entry.link_target {
                preview
                    .lines
                    .push(format!("symlink -> {}", target.display()));
                preview.lines.push(String::new());
            }
        }

        if entry.is_dir_like() {
            preview.fill_directory(&entry.path);
            return preview;
        }

        let meta = match fs::metadata(&entry.path) {
            Ok(m) => m,
            Err(err) => {
                preview.kind = PreviewKind::Error;
                preview.info = format!("cannot stat: {err}");
                preview
                    .lines
                    .push(format!("unable to read {}: {err}", entry.path.display()));
                return preview;
            }
        };
        preview.size = meta.len();

        if !meta.is_file() {
            preview.kind = PreviewKind::Error;
            preview.info = "not a regular file".into();
            preview
                .lines
                .push("special file (socket, device, or fifo) — no content preview".into());
            return preview;
        }

        if meta.len() == 0 {
            preview.kind = PreviewKind::Empty;
            preview.info = "0 B · empty file".into();
            preview.lines.push("(empty file)".into());
            return preview;
        }

        let read_len = meta.len().min(MAX_PREVIEW_BYTES);
        preview.truncated = meta.len() > read_len;
        let bytes = match read_prefix(&entry.path, read_len as usize) {
            Ok(b) => b,
            Err(err) => {
                preview.kind = PreviewKind::Error;
                preview.info = format!("cannot read: {err}");
                preview
                    .lines
                    .push(format!("unable to read {}: {err}", entry.path.display()));
                return preview;
            }
        };

        if looks_binary(&bytes) {
            preview.kind = PreviewKind::Binary;
            preview.lines.extend(hex_dump(&bytes));
            preview.info = format!(
                "{} · binary · hex dump of {} byte(s)",
                human_size(meta.len()),
                bytes.len()
            );
        } else {
            preview.kind = PreviewKind::Text;
            let text = String::from_utf8_lossy(&bytes);
            let lossy = matches!(text, std::borrow::Cow::Owned(_));
            let mut count = 0usize;
            for line in text.split('\n') {
                preview.lines.push(sanitize(line));
                count += 1;
            }
            // A trailing newline yields a final empty element; don't count it as
            // a line, but keep it so the user can see the file ends with \n.
            if preview.lines.last().is_some_and(|l| l.is_empty()) && count > 1 {
                preview.lines.pop();
                count -= 1;
            }
            preview.info = format!(
                "{} · {} line{}{}{}",
                human_size(meta.len()),
                count,
                if count == 1 { "" } else { "s" },
                if lossy {
                    " · invalid UTF-8 replaced"
                } else {
                    ""
                },
                if preview.truncated {
                    format!(" · truncated at {}", human_size(MAX_PREVIEW_BYTES))
                } else {
                    String::new()
                }
            );
        }
        preview
    }

    fn fill_directory(&mut self, path: &Path) {
        self.kind = PreviewKind::Directory;
        match fs::read_dir(path) {
            Ok(iter) => {
                let mut names: Vec<(bool, String)> = iter
                    .flatten()
                    .map(|e| {
                        let is_dir = e.path().is_dir();
                        (is_dir, e.file_name().to_string_lossy().into_owned())
                    })
                    .collect();
                names.sort_by(|a, b| {
                    b.0.cmp(&a.0)
                        .then_with(|| crate::listing::natural_cmp(&a.1, &b.1))
                });
                let dirs = names.iter().filter(|(d, _)| *d).count();
                self.info = format!(
                    "directory · {} dir(s), {} file(s)",
                    dirs,
                    names.len() - dirs
                );
                self.lines.push(format!("{}", path.display()));
                self.lines.push(String::new());
                if names.is_empty() {
                    self.lines.push("(empty directory)".into());
                }
                for (is_dir, name) in names {
                    self.lines
                        .push(format!("  {} {}", if is_dir { "/" } else { " " }, name));
                }
                self.lines.push(String::new());
                self.lines
                    .push("Press Enter or Right to open this directory.".into());
            }
            Err(err) => {
                self.kind = PreviewKind::Error;
                self.info = format!("cannot list: {err}");
                self.lines
                    .push(format!("unable to list {}: {err}", path.display()));
            }
        }
    }

    /// Total lines available, used by the scroll clamp.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn longest_line(&self) -> usize {
        self.lines
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(0)
    }

    /// Scroll down by `delta`, never past the last screenful.
    pub fn scroll_down(&mut self, delta: u16, viewport: u16) {
        let max = self.max_scroll(viewport);
        self.scroll = self.scroll.saturating_add(delta).min(max);
    }

    pub fn scroll_up(&mut self, delta: u16) {
        self.scroll = self.scroll.saturating_sub(delta);
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll = 0;
        self.hscroll = 0;
    }

    pub fn scroll_to_bottom(&mut self, viewport: u16) {
        self.scroll = self.max_scroll(viewport);
    }

    /// Lay the content out for a viewport `width` columns wide, caching the
    /// result. Returns the rows that will be drawn, in order.
    pub fn layout(&mut self, width: u16) -> &[String] {
        let key = (width.max(1), self.wrap);
        if self.rows_key != Some(key) {
            self.rows = if self.wrap {
                self.lines
                    .iter()
                    .flat_map(|line| wrap_line(line, key.0 as usize))
                    .collect()
            } else {
                self.lines.clone()
            };
            self.rows_key = Some(key);
        }
        &self.rows
    }

    /// Total rows the content occupies at the last laid-out width.
    pub fn display_rows(&self) -> usize {
        match self.rows_key {
            Some(_) => self.rows.len(),
            // Not laid out yet: logical lines are the best available answer.
            None => self.line_count(),
        }
    }

    /// The laid-out rows visible in a viewport of `height` rows starting at
    /// `first`. Call `layout` first; before that the cache is empty.
    pub fn rows_slice(&self, first: usize, height: usize) -> &[String] {
        let start = first.min(self.rows.len());
        let end = start.saturating_add(height).min(self.rows.len());
        &self.rows[start..end]
    }

    pub fn max_scroll(&self, viewport: u16) -> u16 {
        let total = self.display_rows().min(u16::MAX as usize) as u16;
        total.saturating_sub(viewport.max(1))
    }

    pub fn scroll_right(&mut self, delta: u16) {
        if !self.wrap {
            let max = self.longest_line().min(u16::MAX as usize) as u16;
            self.hscroll = self.hscroll.saturating_add(delta).min(max);
        }
    }

    pub fn scroll_left(&mut self, delta: u16) {
        self.hscroll = self.hscroll.saturating_sub(delta);
    }

    pub fn toggle_wrap(&mut self) {
        self.wrap = !self.wrap;
        // The row cache is width- and wrap-specific; force a rebuild.
        self.rows_key = None;
        if self.wrap {
            self.hscroll = 0;
        }
    }
}

/// Break one logical line into rows of at most `width` display columns,
/// preferring to break at spaces and falling back to a hard cut for runs that
/// have none. An empty line still yields one row, so blank lines are preserved.
fn wrap_line(line: &str, width: usize) -> Vec<String> {
    if line.is_empty() {
        return vec![String::new()];
    }
    if UnicodeWidthStr::width(line) <= width {
        return vec![line.to_string()];
    }

    let mut rows = Vec::new();
    let mut row = String::new();
    let mut row_width = 0usize;
    // A pending word plus the whitespace that preceded it.
    let mut word = String::new();
    let mut word_width = 0usize;
    let mut gap = String::new();
    let mut gap_width = 0usize;

    let flush_word = |rows: &mut Vec<String>,
                      row: &mut String,
                      row_width: &mut usize,
                      word: &mut String,
                      word_width: &mut usize,
                      gap: &mut String,
                      gap_width: &mut usize| {
        if word.is_empty() {
            return;
        }
        if *row_width + *gap_width + *word_width <= width {
            row.push_str(gap);
            row.push_str(word);
            *row_width += *gap_width + *word_width;
        } else {
            if !row.is_empty() {
                rows.push(std::mem::take(row));
                *row_width = 0;
            }
            // A word longer than the viewport has to be cut hard.
            if *word_width > width {
                for ch in word.chars() {
                    let w = UnicodeWidthStr::width(ch.to_string().as_str());
                    if *row_width + w > width {
                        rows.push(std::mem::take(row));
                        *row_width = 0;
                    }
                    row.push(ch);
                    *row_width += w;
                }
            } else {
                row.push_str(word);
                *row_width = *word_width;
            }
        }
        word.clear();
        *word_width = 0;
        gap.clear();
        *gap_width = 0;
    };

    for ch in line.chars() {
        let w = UnicodeWidthStr::width(ch.to_string().as_str());
        if ch == ' ' {
            flush_word(
                &mut rows,
                &mut row,
                &mut row_width,
                &mut word,
                &mut word_width,
                &mut gap,
                &mut gap_width,
            );
            gap.push(ch);
            gap_width += w;
        } else {
            word.push(ch);
            word_width += w;
        }
    }
    flush_word(
        &mut rows,
        &mut row,
        &mut row_width,
        &mut word,
        &mut word_width,
        &mut gap,
        &mut gap_width,
    );
    if !row.is_empty() {
        rows.push(row);
    }
    if rows.is_empty() {
        rows.push(String::new());
    }
    rows
}

/// Read at most `len` bytes from the head of a file.
fn read_prefix(path: &Path, len: usize) -> std::io::Result<Vec<u8>> {
    use std::io::Read;
    let file = fs::File::open(path)?;
    let mut buf = Vec::with_capacity(len.min(1 << 20));
    file.take(len as u64).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Heuristic used by `grep`/`less`: a NUL byte, or a high share of bytes that
/// are neither printable nor common whitespace, means "binary".
fn looks_binary(bytes: &[u8]) -> bool {
    let sample = &bytes[..bytes.len().min(8192)];
    if sample.contains(&0) {
        return true;
    }
    // Count control bytes that are not tab/newline/carriage-return/form-feed/esc.
    let suspicious = sample
        .iter()
        .filter(|&&b| b < 0x09 || (0x0e..0x20).contains(&b) && b != 0x1b || b == 0x7f)
        .count();
    suspicious * 100 > sample.len() * 5
}

/// Replace characters that would corrupt the terminal, and expand tabs.
fn sanitize(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    for ch in line.chars() {
        match ch {
            '\t' => {
                // Align to the next 4-column stop.
                let pad = 4 - (out.chars().count() % 4);
                for _ in 0..pad {
                    out.push(' ');
                }
            }
            '\r' => {}
            c if c.is_control() => out.push('·'),
            c => out.push(c),
        }
    }
    out
}

/// Classic 16-bytes-per-row hex dump with an ASCII gutter.
fn hex_dump(bytes: &[u8]) -> Vec<String> {
    let mut out = Vec::with_capacity(bytes.len() / 16 + 1);
    for (row, chunk) in bytes.chunks(16).enumerate() {
        let mut hex = String::with_capacity(48);
        let mut ascii = String::with_capacity(16);
        for (i, byte) in chunk.iter().enumerate() {
            if i == 8 {
                hex.push(' ');
            }
            hex.push_str(&format!("{byte:02x} "));
            ascii.push(if (0x20..0x7f).contains(byte) {
                *byte as char
            } else {
                '.'
            });
        }
        out.push(format!("{:08x}  {:<49} |{}|", row * 16, hex, ascii));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listing::Listing;
    use std::io::Write;

    fn scratch(tag: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        dir.push(format!("toolc-preview-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn preview_of(root: &Path, name: &str) -> Preview {
        let listing = Listing::new(root.to_path_buf(), true);
        let entry = listing
            .iter()
            .find(|e| e.name == name)
            .expect("entry present")
            .clone();
        Preview::load(&entry, true)
    }

    #[test]
    fn text_preview_keeps_every_line() {
        let root = scratch("text");
        let body = "alpha\nbeta\ngamma\n";
        fs::File::create(root.join("f.txt"))
            .unwrap()
            .write_all(body.as_bytes())
            .unwrap();
        let p = preview_of(&root, "f.txt");
        assert_eq!(p.kind, PreviewKind::Text);
        assert_eq!(p.lines, vec!["alpha", "beta", "gamma"]);
        assert_eq!(p.name, "f.txt");
        assert!(p.info.contains("3 lines"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn long_file_is_fully_loaded_for_scrolling() {
        let root = scratch("long");
        let body: String = (1..=500).map(|i| format!("line {i}\n")).collect();
        fs::File::create(root.join("big.txt"))
            .unwrap()
            .write_all(body.as_bytes())
            .unwrap();
        let mut p = preview_of(&root, "big.txt");
        assert_eq!(p.line_count(), 500);
        assert_eq!(p.lines[499], "line 500");
        p.scroll_to_bottom(20);
        assert_eq!(p.scroll, 480);
        p.scroll_down(50, 20);
        assert_eq!(p.scroll, 480, "must not scroll past the end");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn binary_file_renders_hex() {
        let root = scratch("bin");
        fs::File::create(root.join("blob.bin"))
            .unwrap()
            .write_all(&[0u8, 1, 2, 255, 65])
            .unwrap();
        let p = preview_of(&root, "blob.bin");
        assert_eq!(p.kind, PreviewKind::Binary);
        assert!(p.lines[0].starts_with("00000000  00 01 02 ff 41"));
        assert!(p.lines[0].contains("|....A|"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn wrapped_long_lines_extend_the_scroll_range() {
        let root = scratch("wrapscroll");
        // Ten lines of exactly 200 columns, plus a short marker at the end.
        let body: String = (1..=10)
            .map(|i| format!("{i:02}{}\n", "w".repeat(198)))
            .chain(std::iter::once("LAST\n".to_string()))
            .collect();
        fs::File::create(root.join("wide.txt"))
            .unwrap()
            .write_all(body.as_bytes())
            .unwrap();
        let mut p = preview_of(&root, "wide.txt");
        assert_eq!(p.line_count(), 11);

        // At 50 columns each 200-column line takes 4 rows: 10*4 + 1 = 41.
        p.wrap = true;
        assert_eq!(p.layout(50).len(), 41);
        assert_eq!(p.display_rows(), 41);
        // The scroll limit must be based on wrapped rows, or the tail of the
        // file becomes unreachable.
        assert_eq!(p.max_scroll(10), 31);
        p.scroll_to_bottom(10);
        assert_eq!(p.scroll, 31);
        // The last row really is the end of the file.
        assert_eq!(p.rows_slice(40, 1), ["LAST".to_string()]);

        // With wrap off, rows equal logical lines again.
        p.toggle_wrap();
        assert_eq!(p.layout(50).len(), 11);
        assert_eq!(p.max_scroll(10), 1);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn wrapping_breaks_at_spaces_and_hard_cuts_long_words() {
        // Short enough to pass through untouched.
        assert_eq!(wrap_line("hello", 10), vec!["hello"]);
        // Blank lines survive as one row, so spacing is preserved.
        assert_eq!(wrap_line("", 10), vec![""]);
        // Breaks at the space rather than mid-word.
        assert_eq!(wrap_line("hello world", 7), vec!["hello", "world"]);
        // A word with no break opportunity is cut to fit.
        assert_eq!(wrap_line("aaaaaaaa", 3), vec!["aaa", "aaa", "aa"]);
        // A short prefix followed by an unbreakable run.
        assert_eq!(wrap_line("id abcdef", 4), vec!["id", "abcd", "ef"]);

        // Every produced row fits, and nothing is silently dropped.
        for width in [1usize, 3, 7, 20] {
            let line = "the quick brown fox jumps over the extraordinarily lazy dog";
            let rows = wrap_line(line, width);
            for row in &rows {
                assert!(
                    UnicodeWidthStr::width(row.as_str()) <= width,
                    "row {row:?} exceeds width {width}"
                );
            }
            let rejoined: String = rows.concat();
            let expected: String = line.chars().filter(|c| *c != ' ').collect();
            let got: String = rejoined.chars().filter(|c| *c != ' ').collect();
            assert_eq!(got, expected, "content lost at width {width}");
        }
    }

    #[test]
    fn wrapping_handles_wide_characters() {
        // Each CJK glyph is two columns, so four fit in a width of 8.
        let rows = wrap_line("English-only textテキスト", 8);
        for row in &rows {
            assert!(UnicodeWidthStr::width(row.as_str()) <= 8);
        }
        assert_eq!(rows.concat(), "English-only textテキスト");
    }

    #[test]
    fn empty_file_is_labelled() {
        let root = scratch("empty");
        fs::File::create(root.join("zero")).unwrap();
        let p = preview_of(&root, "zero");
        assert_eq!(p.kind, PreviewKind::Empty);
        assert_eq!(p.lines, vec!["(empty file)"]);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn directory_preview_lists_children() {
        let root = scratch("dir");
        fs::create_dir(root.join("sub")).unwrap();
        fs::create_dir(root.join("sub/inner")).unwrap();
        fs::File::create(root.join("sub/a.txt")).unwrap();
        let p = preview_of(&root, "sub");
        assert_eq!(p.kind, PreviewKind::Directory);
        let joined = p.lines.join("\n");
        assert!(joined.contains("inner"));
        assert!(joined.contains("a.txt"));
        assert!(p.info.contains("1 dir(s), 1 file(s)"));
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn tabs_expand_and_controls_are_visible() {
        assert_eq!(sanitize("a\tb"), "a   b");
        assert_eq!(sanitize("\tx"), "    x");
        assert_eq!(sanitize("a\u{1}b"), "a·b");
        assert_eq!(sanitize("crlf\r"), "crlf");
    }

    #[test]
    fn binary_heuristic_distinguishes_text() {
        assert!(!looks_binary(b"plain ascii text\nwith newlines\n"));
        assert!(!looks_binary("utf-8 ünïcödé — ok\n".as_bytes()));
        assert!(looks_binary(&[0x7f, 0x45, 0x4c, 0x46, 0x00, 0x01]));
    }
}
