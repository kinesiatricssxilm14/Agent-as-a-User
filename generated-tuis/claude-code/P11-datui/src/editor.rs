//! A minimal single-line text editor with cursor movement and history.
//!
//! Used for the query line and for prompts such as the export path. Kept
//! separate from the UI so the editing rules can be tested directly.

/// Editable line of text with a cursor measured in characters.
#[derive(Debug, Clone, Default)]
pub struct Editor {
    chars: Vec<char>,
    cursor: usize,
    history: Vec<String>,
    /// Position while browsing history; `None` means "editing a fresh line".
    hist_pos: Option<usize>,
    /// The in-progress line, stashed while browsing history.
    stash: Option<String>,
}

impl Editor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    /// Cursor position in characters from the start of the line.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_text(&mut self, s: &str) {
        self.chars = s.chars().collect();
        self.cursor = self.chars.len();
    }

    pub fn clear(&mut self) {
        self.chars.clear();
        self.cursor = 0;
        self.hist_pos = None;
        self.stash = None;
    }

    pub fn insert(&mut self, c: char) {
        let at = self.cursor.min(self.chars.len());
        self.chars.insert(at, c);
        self.cursor = at + 1;
    }

    /// Delete the character before the cursor.
    pub fn backspace(&mut self) {
        if self.cursor > 0 && self.cursor <= self.chars.len() {
            self.chars.remove(self.cursor - 1);
            self.cursor -= 1;
        }
    }

    /// Delete the character under the cursor.
    pub fn delete(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
        }
    }

    pub fn left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn right(&mut self) {
        if self.cursor < self.chars.len() {
            self.cursor += 1;
        }
    }

    pub fn home(&mut self) {
        self.cursor = 0;
    }

    pub fn end(&mut self) {
        self.cursor = self.chars.len();
    }

    /// Delete from the cursor back to the start of the previous word.
    pub fn delete_word(&mut self) {
        while self.cursor > 0 && self.chars[self.cursor - 1].is_whitespace() {
            self.chars.remove(self.cursor - 1);
            self.cursor -= 1;
        }
        while self.cursor > 0 && !self.chars[self.cursor - 1].is_whitespace() {
            self.chars.remove(self.cursor - 1);
            self.cursor -= 1;
        }
    }

    /// Delete from the cursor to the end of the line.
    pub fn kill_to_end(&mut self) {
        self.chars.truncate(self.cursor);
    }

    /// Delete from the start of the line to the cursor.
    pub fn kill_to_start(&mut self) {
        self.chars.drain(0..self.cursor);
        self.cursor = 0;
    }

    /// Record the current line in history (most recent last, no duplicates).
    pub fn remember(&mut self) {
        let t = self.text();
        if t.trim().is_empty() {
            return;
        }
        if self.history.last().map(|h| h == &t).unwrap_or(false) {
            return;
        }
        self.history.push(t);
        if self.history.len() > 200 {
            self.history.remove(0);
        }
        self.hist_pos = None;
        self.stash = None;
    }

    /// Step back through history. Returns false when there is nothing older.
    pub fn history_prev(&mut self) -> bool {
        if self.history.is_empty() {
            return false;
        }
        let next = match self.hist_pos {
            None => {
                // Stash the line being typed so it can be restored.
                self.stash = Some(self.text());
                self.history.len() - 1
            }
            Some(0) => return false,
            Some(i) => i - 1,
        };
        self.hist_pos = Some(next);
        let text = self.history[next].clone();
        self.set_text(&text);
        true
    }

    /// Step forward through history, ending on the stashed in-progress line.
    pub fn history_next(&mut self) -> bool {
        match self.hist_pos {
            None => false,
            Some(i) if i + 1 < self.history.len() => {
                self.hist_pos = Some(i + 1);
                let text = self.history[i + 1].clone();
                self.set_text(&text);
                true
            }
            Some(_) => {
                self.hist_pos = None;
                let text = self.stash.take().unwrap_or_default();
                self.set_text(&text);
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ed(s: &str) -> Editor {
        let mut e = Editor::new();
        e.set_text(s);
        e
    }

    #[test]
    fn inserts_at_the_cursor() {
        let mut e = ed("ac");
        e.left();
        e.insert('b');
        assert_eq!(e.text(), "abc");
        assert_eq!(e.cursor(), 2);
    }

    #[test]
    fn deletes_around_the_cursor() {
        let mut e = ed("abc");
        e.backspace();
        assert_eq!(e.text(), "ab");
        e.home();
        e.delete();
        assert_eq!(e.text(), "b");
        // Deleting past either edge is a no-op, not a panic.
        e.home();
        e.backspace();
        e.end();
        e.delete();
        assert_eq!(e.text(), "b");
    }

    #[test]
    fn word_and_line_kills() {
        let mut e = ed("select where age > 40");
        e.delete_word();
        assert_eq!(e.text(), "select where age > ");
        let mut e = ed("abcdef");
        e.home();
        e.right();
        e.right();
        e.kill_to_end();
        assert_eq!(e.text(), "ab");
        let mut e = ed("abcdef");
        e.home();
        e.right();
        e.right();
        e.kill_to_start();
        assert_eq!(e.text(), "cdef");
        assert_eq!(e.cursor(), 0);
    }

    #[test]
    fn history_walks_back_and_forward() {
        let mut e = Editor::new();
        e.set_text("first");
        e.remember();
        e.set_text("second");
        e.remember();
        e.set_text("partial");

        assert!(e.history_prev());
        assert_eq!(e.text(), "second");
        assert!(e.history_prev());
        assert_eq!(e.text(), "first");
        // Nothing older than the first entry.
        assert!(!e.history_prev());
        assert!(e.history_next());
        assert_eq!(e.text(), "second");
        // Walking forward past the newest restores what was being typed.
        assert!(e.history_next());
        assert_eq!(e.text(), "partial");
        assert!(!e.history_next());
    }

    #[test]
    fn history_skips_blanks_and_repeats() {
        let mut e = Editor::new();
        e.set_text("   ");
        e.remember();
        e.set_text("same");
        e.remember();
        e.set_text("same");
        e.remember();
        // Only one entry was kept, so a single step back exhausts it.
        e.set_text("");
        assert!(e.history_prev());
        assert_eq!(e.text(), "same");
        assert!(!e.history_prev());
    }

    #[test]
    fn multibyte_text_is_handled_by_character() {
        let mut e = ed("héllo");
        e.end();
        e.backspace();
        assert_eq!(e.text(), "héll");
        e.home();
        e.right();
        e.delete();
        assert_eq!(e.text(), "hll");
    }
}
