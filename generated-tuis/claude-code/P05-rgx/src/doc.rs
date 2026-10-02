//! The loaded text file: raw content plus the index structures needed to talk
//! about it in terms of **0-indexed character offsets**, which is what the
//! results pane reports.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// How byte offsets map onto character offsets.
///
/// The overwhelmingly common case is an all-ASCII file, where the two indices
/// coincide and no table is needed at all.
#[derive(Debug)]
enum CharIndex {
    /// `byte_offset == char_offset`.
    Ascii,
    /// `char_starts[i]` is the byte offset of the `i`-th character, with a
    /// final sentinel entry equal to `text.len()` so that the end-of-text
    /// offset maps to `char_count` rather than to the last character.
    Table(Vec<usize>),
}

/// A text file loaded into memory.
#[derive(Debug)]
pub struct Document {
    path: PathBuf,
    text: String,
    /// Byte offset of the first byte of every line.
    line_starts: Vec<usize>,
    /// Total number of characters (Unicode scalar values) in the file.
    char_count: usize,
    index: CharIndex,
    /// True when the file was decoded lossily (invalid UTF-8 was replaced).
    lossy: bool,
    /// Size on disk in bytes.
    disk_bytes: u64,
}

impl Document {
    /// Read `path` from the real filesystem.
    ///
    /// Invalid UTF-8 is replaced rather than rejected so that the tool stays
    /// usable on mixed-encoding logs; [`Document::is_lossy`] reports it.
    pub fn load(path: impl AsRef<Path>) -> io::Result<Self> {
        let path = path.as_ref();
        let bytes = fs::read(path)?;
        let disk_bytes = bytes.len() as u64;
        let (text, lossy) = match String::from_utf8(bytes) {
            Ok(s) => (s, false),
            Err(e) => (String::from_utf8_lossy(e.as_bytes()).into_owned(), true),
        };
        Ok(Self::from_parts(
            path.to_path_buf(),
            text,
            lossy,
            disk_bytes,
        ))
    }

    /// Build a document from text that is already in memory (used by tests).
    pub fn from_text(path: impl Into<PathBuf>, text: impl Into<String>) -> Self {
        let text = text.into();
        let n = text.len() as u64;
        Self::from_parts(path.into(), text, false, n)
    }

    fn from_parts(path: PathBuf, text: String, lossy: bool, disk_bytes: u64) -> Self {
        let line_starts = compute_line_starts(&text);
        let ascii = text.is_ascii();
        let (char_count, index) = if ascii {
            (text.len(), CharIndex::Ascii)
        } else {
            let mut starts: Vec<usize> = text.char_indices().map(|(i, _)| i).collect();
            let count = starts.len();
            starts.push(text.len());
            (count, CharIndex::Table(starts))
        };
        Self {
            path,
            text,
            line_starts,
            char_count,
            index,
            lossy,
            disk_bytes,
        }
    }

    /// Path this document was read from.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Full file content.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Number of lines. A trailing newline does not create a final empty line.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Total character count of the file.
    pub fn char_count(&self) -> usize {
        self.char_count
    }

    /// File size in bytes as read from disk.
    pub fn byte_count(&self) -> u64 {
        self.disk_bytes
    }

    /// Whether invalid UTF-8 was replaced while loading.
    pub fn is_lossy(&self) -> bool {
        self.lossy
    }

    /// Content of line `idx` (0-indexed), without the line terminator.
    pub fn line(&self, idx: usize) -> Option<&str> {
        let start = *self.line_starts.get(idx)?;
        let end = self
            .line_starts
            .get(idx + 1)
            .copied()
            .unwrap_or(self.text.len());
        let mut slice = &self.text[start..end];
        if let Some(s) = slice.strip_suffix('\n') {
            slice = s;
        }
        if let Some(s) = slice.strip_suffix('\r') {
            slice = s;
        }
        Some(slice)
    }

    /// Convert a byte offset into a 0-indexed character offset.
    ///
    /// Byte offsets that fall inside a multi-byte character are rounded down to
    /// the character that contains them.
    pub fn byte_to_char(&self, byte: usize) -> usize {
        match &self.index {
            CharIndex::Ascii => byte.min(self.char_count),
            CharIndex::Table(starts) => match starts.binary_search(&byte) {
                Ok(i) => i,
                // `i` is the number of character starts strictly below `byte`,
                // i.e. the index of the character containing it.
                Err(i) => i.saturating_sub(1).min(self.char_count),
            },
        }
    }

    /// 0-indexed `(line, column)` of a byte offset; the column is counted in
    /// characters from the start of the line.
    pub fn byte_to_line_col(&self, byte: usize) -> (usize, usize) {
        let line = match self.line_starts.binary_search(&byte) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let line_start = self.line_starts.get(line).copied().unwrap_or(0);
        let col = self.text[line_start..byte.min(self.text.len())]
            .chars()
            .count();
        (line, col)
    }

    /// Byte offset at which line `idx` starts.
    pub fn line_start_byte(&self, idx: usize) -> usize {
        self.line_starts
            .get(idx)
            .copied()
            .unwrap_or(self.text.len())
    }
}

/// Byte offsets of the beginning of every line in `text`.
fn compute_line_starts(text: &str) -> Vec<usize> {
    let mut starts = vec![0usize];
    for (i, b) in text.bytes().enumerate() {
        if b == b'\n' && i + 1 < text.len() {
            starts.push(i + 1);
        }
    }
    starts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_ignore_trailing_newline() {
        let d = Document::from_text("t", "alpha\nbeta\n");
        assert_eq!(d.line_count(), 2);
        assert_eq!(d.line(0), Some("alpha"));
        assert_eq!(d.line(1), Some("beta"));
        assert_eq!(d.line(2), None);
    }

    #[test]
    fn empty_final_line_is_kept_when_not_terminated() {
        let d = Document::from_text("t", "a\n\nb");
        assert_eq!(d.line_count(), 3);
        assert_eq!(d.line(1), Some(""));
        assert_eq!(d.line(2), Some("b"));
    }

    #[test]
    fn crlf_terminators_are_stripped() {
        let d = Document::from_text("t", "a\r\nb\r\n");
        assert_eq!(d.line_count(), 2);
        assert_eq!(d.line(0), Some("a"));
        assert_eq!(d.line(1), Some("b"));
    }

    #[test]
    fn ascii_char_offsets_equal_byte_offsets() {
        let d = Document::from_text("t", "hello world");
        assert_eq!(d.byte_to_char(0), 0);
        assert_eq!(d.byte_to_char(6), 6);
        assert_eq!(d.char_count(), 11);
    }

    #[test]
    fn multibyte_char_offsets() {
        // "English-only textabc": each CJK char is 3 bytes.
        let d = Document::from_text("t", "English-only textabc");
        assert_eq!(d.char_count(), 5);
        assert_eq!(d.byte_to_char(0), 0);
        assert_eq!(d.byte_to_char(3), 1);
        assert_eq!(d.byte_to_char(6), 2);
        assert_eq!(d.byte_to_char(7), 3);
        assert_eq!(d.byte_to_char(9), 5);
        // A byte in the middle of a character rounds down.
        assert_eq!(d.byte_to_char(4), 1);
    }

    #[test]
    fn line_col_lookup() {
        let d = Document::from_text("t", "abc\ndéf\nghi");
        assert_eq!(d.byte_to_line_col(0), (0, 0));
        assert_eq!(d.byte_to_line_col(4), (1, 0));
        // 'f' is after a 2-byte 'é', so column 2 in characters.
        assert_eq!(d.byte_to_line_col(7), (1, 2));
        assert_eq!(d.byte_to_line_col(9), (2, 0));
    }

    #[test]
    fn empty_document() {
        let d = Document::from_text("t", "");
        assert_eq!(d.line_count(), 1);
        assert_eq!(d.line(0), Some(""));
        assert_eq!(d.char_count(), 0);
        assert_eq!(d.byte_to_char(0), 0);
    }
}
