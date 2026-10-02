//! A single-line text field with a cursor, shared by the filter prompt, the cell
//! editor and the SQL editor.
//!
//! Positions are tracked as byte offsets into the string but only ever moved to
//! character boundaries, so multi-byte input is safe.

/// Editable line of text plus a cursor and (optionally) a value history.
#[derive(Debug, Default, Clone)]
pub struct Input {
    text: String,
    /// Byte offset of the cursor; always on a char boundary.
    cursor: usize,
    /// Previously submitted values, oldest first.
    history: Vec<String>,
    /// Position while walking history with Up/Down; `None` when editing fresh text.
    history_pos: Option<usize>,
    /// Text set aside when history browsing started, restored on the way back down.
    stash: Option<String>,
}

impl Input {
    pub fn new() -> Input {
        Input::default()
    }

    /// Replace the contents and put the cursor at the end.
    pub fn set(&mut self, text: impl Into<String>) {
        self.text = text.into();
        self.cursor = self.text.len();
        self.history_pos = None;
        self.stash = None;
    }

    pub fn clear(&mut self) {
        self.set(String::new());
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Cursor position measured in characters, for placing the terminal caret.
    pub fn cursor_chars(&self) -> usize {
        self.text[..self.cursor].chars().count()
    }

    pub fn insert(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        self.history_pos = None;
    }

    pub fn insert_str(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
        self.history_pos = None;
    }

    /// Delete the character before the cursor (Backspace).
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let prev = self.prev_boundary(self.cursor);
        self.text.replace_range(prev..self.cursor, "");
        self.cursor = prev;
    }

    /// Delete the character under the cursor (Delete).
    pub fn delete(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let next = self.next_boundary(self.cursor);
        self.text.replace_range(self.cursor..next, "");
    }

    /// Delete from the cursor back to the start of the previous word (Ctrl+W).
    pub fn delete_word(&mut self) {
        let start = self.word_start();
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    /// Delete from the cursor to the end of the line (Ctrl+K).
    pub fn kill_to_end(&mut self) {
        self.text.truncate(self.cursor);
    }

    /// Delete from the start of the line to the cursor (Ctrl+U).
    pub fn kill_to_start(&mut self) {
        self.text.replace_range(..self.cursor, "");
        self.cursor = 0;
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

    /// Move left to the start of the current or previous word.
    pub fn word_left(&mut self) {
        self.cursor = self.word_start();
    }

    /// Move right past the end of the current or next word.
    pub fn word_right(&mut self) {
        let bytes = self.text.as_bytes();
        let mut i = self.cursor;
        while i < bytes.len() && bytes[i].is_ascii_whitespace() {
            i = self.next_boundary(i);
        }
        while i < bytes.len() && !bytes[i].is_ascii_whitespace() {
            i = self.next_boundary(i);
        }
        self.cursor = i;
    }

    fn word_start(&self) -> usize {
        let bytes = self.text.as_bytes();
        let mut i = self.cursor;
        while i > 0 && bytes[self.prev_boundary(i)].is_ascii_whitespace() {
            i = self.prev_boundary(i);
        }
        while i > 0 && !bytes[self.prev_boundary(i)].is_ascii_whitespace() {
            i = self.prev_boundary(i);
        }
        i
    }

    fn prev_boundary(&self, from: usize) -> usize {
        let mut i = from.saturating_sub(1);
        while i > 0 && !self.text.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    fn next_boundary(&self, from: usize) -> usize {
        let mut i = (from + 1).min(self.text.len());
        while i < self.text.len() && !self.text.is_char_boundary(i) {
            i += 1;
        }
        i
    }

    // ---- history ---------------------------------------------------------

    /// Record the current text in history (most recent last, no duplicates).
    pub fn push_history(&mut self) {
        let t = self.text.trim().to_string();
        if t.is_empty() {
            return;
        }
        self.history.retain(|h| *h != t);
        self.history.push(t);
        const MAX: usize = 100;
        if self.history.len() > MAX {
            self.history.remove(0);
        }
        self.history_pos = None;
        self.stash = None;
    }

    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Step to an older history entry.
    pub fn history_prev(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let next = match self.history_pos {
            None => {
                self.stash = Some(self.text.clone());
                self.history.len() - 1
            }
            Some(0) => 0,
            Some(i) => i - 1,
        };
        self.history_pos = Some(next);
        self.text = self.history[next].clone();
        self.cursor = self.text.len();
    }

    /// Step to a newer history entry, or back to the text being edited.
    pub fn history_next(&mut self) {
        let Some(i) = self.history_pos else { return };
        if i + 1 < self.history.len() {
            self.history_pos = Some(i + 1);
            self.text = self.history[i + 1].clone();
        } else {
            self.history_pos = None;
            self.text = self.stash.take().unwrap_or_default();
        }
        self.cursor = self.text.len();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(s: &str) -> Input {
        let mut i = Input::new();
        for c in s.chars() {
            i.insert(c);
        }
        i
    }

    #[test]
    fn typing_appends_and_tracks_cursor() {
        let i = typed("abc");
        assert_eq!(i.text(), "abc");
        assert_eq!(i.cursor_chars(), 3);
    }

    #[test]
    fn insert_happens_at_the_cursor() {
        let mut i = typed("ac");
        i.left();
        i.insert('b');
        assert_eq!(i.text(), "abc");
        assert_eq!(i.cursor_chars(), 2);
    }

    #[test]
    fn backspace_and_delete_remove_the_right_side() {
        let mut i = typed("abc");
        i.backspace();
        assert_eq!(i.text(), "ab");
        i.home();
        i.delete();
        assert_eq!(i.text(), "b");
    }

    #[test]
    fn edits_at_the_boundaries_are_noops() {
        let mut i = Input::new();
        i.backspace();
        i.delete();
        i.left();
        i.right();
        assert_eq!(i.text(), "");
        assert_eq!(i.cursor_chars(), 0);
    }

    #[test]
    fn multibyte_text_is_handled_by_character() {
        let mut i = typed("naïve→");
        assert_eq!(i.cursor_chars(), 6);
        i.backspace();
        assert_eq!(i.text(), "naïve");
        i.left();
        i.left();
        assert_eq!(i.cursor_chars(), 3);
        i.insert('X');
        assert_eq!(i.text(), "naïXve");
    }

    #[test]
    fn word_deletion_stops_at_word_start() {
        let mut i = typed("SELECT * FROM users");
        i.delete_word();
        assert_eq!(i.text(), "SELECT * FROM ");
        i.delete_word();
        assert_eq!(i.text(), "SELECT * ");
    }

    #[test]
    fn kill_to_end_and_start() {
        let mut i = typed("hello world");
        i.home();
        i.word_right();
        i.kill_to_end();
        assert_eq!(i.text(), "hello");
        i.end();
        i.kill_to_start();
        assert_eq!(i.text(), "");
    }

    #[test]
    fn word_motion_moves_across_words() {
        let mut i = typed("one two three");
        i.home();
        i.word_right();
        assert_eq!(i.cursor_chars(), 3);
        i.word_right();
        assert_eq!(i.cursor_chars(), 7);
        i.word_left();
        assert_eq!(i.cursor_chars(), 4);
    }

    #[test]
    fn history_walks_backwards_and_forwards() {
        let mut i = Input::new();
        i.set("first");
        i.push_history();
        i.set("second");
        i.push_history();
        i.set("draft");

        i.history_prev();
        assert_eq!(i.text(), "second");
        i.history_prev();
        assert_eq!(i.text(), "first");
        i.history_prev();
        assert_eq!(i.text(), "first", "clamps at the oldest entry");
        i.history_next();
        assert_eq!(i.text(), "second");
        i.history_next();
        assert_eq!(i.text(), "draft", "restores the text being edited");
    }

    #[test]
    fn history_deduplicates_and_ignores_blanks() {
        let mut i = Input::new();
        i.set("a");
        i.push_history();
        i.set("   ");
        i.push_history();
        i.set("a");
        i.push_history();
        assert_eq!(i.history(), &["a".to_string()]);
    }

    #[test]
    fn set_resets_history_browsing() {
        let mut i = Input::new();
        i.set("one");
        i.push_history();
        i.history_prev();
        i.set("fresh");
        i.history_next();
        assert_eq!(i.text(), "fresh");
    }
}
