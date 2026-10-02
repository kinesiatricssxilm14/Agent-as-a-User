//! A small single-line text editor built on top of a `Vec<char>`.
//!
//! The cursor is a `char` index in `0..=len`, which keeps insertion/deletion and
//! cursor movement simple while still being easy to map to byte offsets (see
//! [`Editor::cursor_byte`]) for the pipeline splitter.

#[derive(Clone, Debug)]
pub struct Editor {
    chars: Vec<char>,
    cursor: usize,
}

impl Editor {
    pub fn new() -> Self {
        Self {
            chars: Vec::new(),
            cursor: 0,
        }
    }

    pub fn to_string(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn chars(&self) -> &[char] {
        &self.chars
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Byte offset of the cursor into the UTF-8 representation of the buffer.
    pub fn cursor_byte(&self) -> usize {
        self.chars[..self.cursor].iter().collect::<String>().len()
    }

    pub fn insert_char(&mut self, c: char) {
        self.chars.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.chars.len() {
            self.cursor += 1;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.chars.len();
    }

    pub fn clear(&mut self) {
        self.chars.clear();
        self.cursor = 0;
    }

    pub fn set_text(&mut self, s: &str) {
        self.chars = s.chars().collect();
        self.cursor = self.chars.len();
    }

    /// Delete the word before the cursor (whitespace and the preceding token).
    pub fn delete_word_before(&mut self) {
        while self.cursor > 0 && self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
        while self.cursor > 0 && !self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
    }

    /// Delete from the cursor to the end of the line.
    pub fn delete_to_end(&mut self) {
        self.chars.truncate(self.cursor);
    }

    /// Replace the `char` range `[start, end)` with `text` and move the cursor
    /// to the end of the inserted text. Used by Tab completion.
    pub fn replace_range(&mut self, start: usize, end: usize, text: &str) {
        let start = start.min(self.chars.len());
        let end = end.min(self.chars.len()).max(start);
        self.chars.splice(start..end, text.chars());
        self.cursor = start + text.chars().count();
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}
