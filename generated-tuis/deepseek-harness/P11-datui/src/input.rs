//! Minimal single-line text editor used for query / path input fields.
//!
//! The cursor is tracked as a byte index into a UTF-8 string, which keeps
//! insert/delete operations correct for multi-byte characters.

#[derive(Debug, Clone, Default)]
pub struct LineEditor {
    pub content: String,
    /// Byte offset of the cursor.
    pub cursor: usize,
}

impl LineEditor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    pub fn set_content(&mut self, content: impl Into<String>) {
        self.content = content.into();
        self.cursor = self.content.len();
    }

    pub fn clear(&mut self) {
        self.content.clear();
        self.cursor = 0;
    }

    /// Number of characters before the cursor (display column).
    pub fn cursor_col(&self) -> usize {
        self.content[..self.cursor].chars().count()
    }

    pub fn insert_char(&mut self, c: char) {
        self.content.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.prev_char_boundary();
        self.content.drain(prev..self.cursor);
        self.cursor = prev;
    }

    pub fn delete(&mut self) {
        if self.cursor >= self.content.len() {
            return;
        }
        let next = self.next_char_boundary();
        self.content.drain(self.cursor..next);
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.prev_char_boundary();
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.content.len() {
            self.cursor = self.next_char_boundary();
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.content.len();
    }

    fn prev_char_boundary(&self) -> usize {
        if self.cursor == 0 {
            return 0;
        }
        let mut idx = self.cursor - 1;
        while idx > 0 && !self.content.is_char_boundary(idx) {
            idx -= 1;
        }
        idx
    }

    fn next_char_boundary(&self) -> usize {
        if self.cursor >= self.content.len() {
            return self.content.len();
        }
        let mut idx = self.cursor + 1;
        while idx < self.content.len() && !self.content.is_char_boundary(idx) {
            idx += 1;
        }
        idx
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_backspace() {
        let mut e = LineEditor::new();
        e.insert_char('a');
        e.insert_char('b');
        assert_eq!(e.content, "ab");
        e.backspace();
        assert_eq!(e.content, "a");
    }

    #[test]
    fn cursor_moves_over_multibyte() {
        let mut e = LineEditor::new();
        e.set_content("héllo");
        e.home();
        e.move_right();
        e.move_right();
        assert_eq!(e.cursor_col(), 2);
        e.insert_char('X');
        assert_eq!(e.content, "héXllo");
    }
}
