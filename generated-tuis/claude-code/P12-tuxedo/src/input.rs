//! A single-line text editor used by every prompt and form field.
//!
//! Cursor positions are tracked as byte offsets on a character boundary so
//! multi-byte input (UTF-8 task descriptions) behaves correctly.

/// Editable line of text with a cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextInput {
    text: String,
    /// Byte offset of the cursor; always on a char boundary, `<= text.len()`.
    cursor: usize,
}

impl TextInput {
    pub fn new() -> TextInput {
        TextInput::default()
    }

    /// An input pre-filled with `text`, cursor at the end.
    pub fn with_text(text: impl Into<String>) -> TextInput {
        let text = text.into();
        let cursor = text.len();
        TextInput { text, cursor }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Cursor position measured in characters, for rendering.
    pub fn cursor_chars(&self) -> usize {
        self.text[..self.cursor].chars().count()
    }

    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.len();
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn insert(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn insert_str(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// Delete the character before the cursor.
    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let prev = self.prev_boundary(self.cursor);
        self.text.replace_range(prev..self.cursor, "");
        self.cursor = prev;
        true
    }

    /// Delete the character under the cursor.
    pub fn delete(&mut self) -> bool {
        if self.cursor >= self.text.len() {
            return false;
        }
        let next = self.next_boundary(self.cursor);
        self.text.replace_range(self.cursor..next, "");
        true
    }

    /// Delete the whitespace-delimited word before the cursor.
    pub fn delete_word_before(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let mut i = self.cursor;
        // Skip trailing whitespace, then the word itself.
        while i > 0 {
            let p = self.prev_boundary(i);
            if self.text[p..i].chars().next().is_some_and(char::is_whitespace) {
                i = p;
            } else {
                break;
            }
        }
        while i > 0 {
            let p = self.prev_boundary(i);
            if self.text[p..i].chars().next().is_some_and(char::is_whitespace) {
                break;
            }
            i = p;
        }
        self.text.replace_range(i..self.cursor, "");
        self.cursor = i;
        true
    }

    /// Delete from the cursor to the start of the line.
    pub fn delete_to_start(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.text.replace_range(0..self.cursor, "");
        self.cursor = 0;
        true
    }

    /// Delete from the cursor to the end of the line.
    pub fn delete_to_end(&mut self) -> bool {
        if self.cursor >= self.text.len() {
            return false;
        }
        self.text.truncate(self.cursor);
        true
    }

    pub fn left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.prev_boundary(self.cursor);
        }
    }

    pub fn right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor = self.next_boundary(self.cursor);
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.text.len();
    }

    fn prev_boundary(&self, from: usize) -> usize {
        let mut i = from - 1;
        while i > 0 && !self.text.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    fn next_boundary(&self, from: usize) -> usize {
        let mut i = from + 1;
        while i < self.text.len() && !self.text.is_char_boundary(i) {
            i += 1;
        }
        i.min(self.text.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(s: &str) -> TextInput {
        let mut i = TextInput::new();
        for c in s.chars() {
            i.insert(c);
        }
        i
    }

    #[test]
    fn types_and_reports_cursor() {
        let i = typed("hello");
        assert_eq!(i.text(), "hello");
        assert_eq!(i.cursor_chars(), 5);
    }

    #[test]
    fn moves_and_edits_mid_string() {
        let mut i = typed("hello");
        i.left();
        i.left();
        assert_eq!(i.cursor_chars(), 3);
        i.insert('X');
        assert_eq!(i.text(), "helXlo");
        assert!(i.backspace());
        assert_eq!(i.text(), "hello");
        assert!(i.delete());
        assert_eq!(i.text(), "helo");
    }

    #[test]
    fn honours_string_boundaries() {
        let mut i = TextInput::new();
        assert!(!i.backspace());
        assert!(!i.delete());
        i.left();
        assert_eq!(i.cursor_chars(), 0);
        i.set_text("ab");
        i.right();
        i.right();
        i.right();
        assert_eq!(i.cursor_chars(), 2);
        assert!(!i.delete());
    }

    #[test]
    fn handles_multibyte_text() {
        let mut i = typed("café ☕");
        assert_eq!(i.cursor_chars(), 6);
        assert!(i.backspace());
        assert_eq!(i.text(), "café ");
        i.left();
        i.left();
        assert_eq!(i.cursor_chars(), 3);
        i.insert('é');
        assert_eq!(i.text(), "caféé ");
    }

    #[test]
    fn deletes_words_and_line_parts() {
        let mut i = typed("buy milk   and eggs");
        assert!(i.delete_word_before());
        assert_eq!(i.text(), "buy milk   and ");
        assert!(i.delete_word_before());
        assert_eq!(i.text(), "buy milk   ");
        assert!(i.delete_word_before());
        assert_eq!(i.text(), "buy ");

        let mut i = typed("one two");
        i.home();
        assert!(!i.delete_word_before());
        assert!(i.delete_to_end());
        assert_eq!(i.text(), "");

        let mut i = typed("one two");
        assert!(i.delete_to_start());
        assert_eq!(i.text(), "");
        assert!(!i.delete_to_start());
    }

    #[test]
    fn home_and_end_jump() {
        let mut i = typed("abc");
        i.home();
        assert_eq!(i.cursor_chars(), 0);
        i.insert_str("xy");
        assert_eq!(i.text(), "xyabc");
        i.end();
        assert_eq!(i.cursor_chars(), 5);
    }

    #[test]
    fn prefilled_starts_at_the_end() {
        let i = TextInput::with_text("(A) task");
        assert_eq!(i.cursor_chars(), 8);
        assert!(!i.is_empty());
    }

    #[test]
    fn clear_resets_cursor() {
        let mut i = typed("abc");
        i.clear();
        assert!(i.is_empty());
        assert_eq!(i.cursor_chars(), 0);
    }
}
