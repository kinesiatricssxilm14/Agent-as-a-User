//! A single-line text input with a cursor, editable by keyboard only.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use unicode_width::UnicodeWidthStr;

/// An editable line of text.  The cursor is a byte offset that always sits on a
/// character boundary.
#[derive(Debug, Clone, Default)]
pub struct Input {
    value: String,
    cursor: usize,
}

impl Input {
    /// An input pre-filled with `value`, cursor at the end.
    pub fn with_value(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.len();
        Self { value, cursor }
    }

    /// The current text.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Cursor position as a byte offset.  Only the tests need this directly;
    /// rendering uses [`Input::cursor_display_col`].
    #[cfg(test)]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Display width of the text before the cursor, for placing the terminal
    /// cursor.
    pub fn cursor_display_col(&self) -> usize {
        self.value[..self.cursor].width()
    }

    /// True when the field is empty.
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Replace the whole value, putting the cursor at the end.
    pub fn set(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.cursor = self.value.len();
    }

    /// Remove all text.
    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    /// Handle a key press.  Returns `true` when the text changed.
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let alt = key.modifiers.contains(KeyModifiers::ALT);
        match key.code {
            KeyCode::Char(c) if ctrl || alt => match c {
                'a' => {
                    self.cursor = 0;
                    false
                }
                'e' => {
                    self.cursor = self.value.len();
                    false
                }
                'u' => {
                    // Kill to start of line.
                    self.value.drain(..self.cursor);
                    self.cursor = 0;
                    true
                }
                'k' => {
                    self.value.truncate(self.cursor);
                    true
                }
                'w' => self.delete_word_left(),
                'b' => {
                    self.cursor = self.prev_word(self.cursor);
                    false
                }
                'f' => {
                    self.cursor = self.next_word(self.cursor);
                    false
                }
                _ => false,
            },
            KeyCode::Char(c) => {
                self.value.insert(self.cursor, c);
                self.cursor += c.len_utf8();
                true
            }
            KeyCode::Backspace if ctrl || alt => self.delete_word_left(),
            KeyCode::Backspace => {
                if self.cursor == 0 {
                    return false;
                }
                let prev = self.prev_boundary(self.cursor);
                self.value.drain(prev..self.cursor);
                self.cursor = prev;
                true
            }
            KeyCode::Delete => {
                if self.cursor >= self.value.len() {
                    return false;
                }
                let next = self.next_boundary(self.cursor);
                self.value.drain(self.cursor..next);
                true
            }
            KeyCode::Left if ctrl || alt => {
                self.cursor = self.prev_word(self.cursor);
                false
            }
            KeyCode::Right if ctrl || alt => {
                self.cursor = self.next_word(self.cursor);
                false
            }
            KeyCode::Left => {
                self.cursor = self.prev_boundary(self.cursor);
                false
            }
            KeyCode::Right => {
                self.cursor = self.next_boundary(self.cursor);
                false
            }
            KeyCode::Home => {
                self.cursor = 0;
                false
            }
            KeyCode::End => {
                self.cursor = self.value.len();
                false
            }
            _ => false,
        }
    }

    fn delete_word_left(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        let start = self.prev_word(self.cursor);
        self.value.drain(start..self.cursor);
        self.cursor = start;
        true
    }

    fn prev_boundary(&self, at: usize) -> usize {
        if at == 0 {
            return 0;
        }
        self.value[..at]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    fn next_boundary(&self, at: usize) -> usize {
        if at >= self.value.len() {
            return self.value.len();
        }
        at + self.value[at..]
            .chars()
            .next()
            .map(char::len_utf8)
            .unwrap_or(1)
    }

    /// Start of the word before `at`.
    fn prev_word(&self, at: usize) -> usize {
        let mut i = at;
        while i > 0 {
            let p = self.prev_boundary(i);
            if !self.value[p..i].chars().all(char::is_whitespace) {
                break;
            }
            i = p;
        }
        while i > 0 {
            let p = self.prev_boundary(i);
            if self.value[p..i].chars().all(char::is_whitespace) {
                break;
            }
            i = p;
        }
        i
    }

    /// Start of the word after `at`.
    fn next_word(&self, at: usize) -> usize {
        let mut i = at;
        let len = self.value.len();
        while i < len {
            let n = self.next_boundary(i);
            if self.value[i..n].chars().all(char::is_whitespace) {
                break;
            }
            i = n;
        }
        while i < len {
            let n = self.next_boundary(i);
            if !self.value[i..n].chars().all(char::is_whitespace) {
                break;
            }
            i = n;
        }
        i
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    fn code(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::NONE)
    }

    fn typed(s: &str) -> Input {
        let mut i = Input::default();
        for c in s.chars() {
            i.handle_key(key(c));
        }
        i
    }

    #[test]
    fn typing_appends() {
        let i = typed(r"\d+");
        assert_eq!(i.value(), r"\d+");
        assert_eq!(i.cursor(), 3);
    }

    #[test]
    fn backspace_deletes_left() {
        let mut i = typed("abc");
        assert!(i.handle_key(code(KeyCode::Backspace)));
        assert_eq!(i.value(), "ab");
        i.handle_key(code(KeyCode::Home));
        assert!(!i.handle_key(code(KeyCode::Backspace)));
    }

    #[test]
    fn insert_in_the_middle() {
        let mut i = typed("ac");
        i.handle_key(code(KeyCode::Left));
        i.handle_key(key('b'));
        assert_eq!(i.value(), "abc");
        assert_eq!(i.cursor(), 2);
    }

    #[test]
    fn delete_removes_right() {
        let mut i = typed("abc");
        i.handle_key(code(KeyCode::Home));
        assert!(i.handle_key(code(KeyCode::Delete)));
        assert_eq!(i.value(), "bc");
    }

    #[test]
    fn home_and_end() {
        let mut i = typed("hello");
        i.handle_key(code(KeyCode::Home));
        assert_eq!(i.cursor(), 0);
        i.handle_key(code(KeyCode::End));
        assert_eq!(i.cursor(), 5);
    }

    #[test]
    fn ctrl_u_and_k_kill_to_the_edges() {
        let mut i = typed("abcdef");
        i.handle_key(code(KeyCode::Left));
        i.handle_key(code(KeyCode::Left));
        assert!(i.handle_key(ctrl('u')));
        assert_eq!(i.value(), "ef");
        let mut j = typed("abcdef");
        j.handle_key(code(KeyCode::Left));
        assert!(j.handle_key(ctrl('k')));
        assert_eq!(j.value(), "abcde");
    }

    #[test]
    fn ctrl_w_deletes_a_word() {
        let mut i = typed("foo bar baz");
        assert!(i.handle_key(ctrl('w')));
        assert_eq!(i.value(), "foo bar ");
        i.handle_key(ctrl('w'));
        assert_eq!(i.value(), "foo ");
    }

    #[test]
    fn ctrl_a_and_e_move_without_editing() {
        let mut i = typed("xyz");
        assert!(!i.handle_key(ctrl('a')));
        assert_eq!(i.cursor(), 0);
        assert!(!i.handle_key(ctrl('e')));
        assert_eq!(i.cursor(), 3);
    }

    #[test]
    fn word_motions() {
        let mut i = typed("one two three");
        i.handle_key(ctrl('b'));
        assert_eq!(&i.value()[i.cursor()..], "three");
        i.handle_key(ctrl('b'));
        assert_eq!(&i.value()[i.cursor()..], "two three");
        i.handle_key(ctrl('f'));
        assert_eq!(&i.value()[i.cursor()..], "three");
    }

    #[test]
    fn multibyte_editing_stays_on_boundaries() {
        let mut i = typed("English-only text");
        assert_eq!(i.cursor(), 6);
        i.handle_key(code(KeyCode::Left));
        assert_eq!(i.cursor(), 3);
        assert!(i.handle_key(code(KeyCode::Backspace)));
        assert_eq!(i.value(), "English-only text");
        assert_eq!(i.cursor(), 0);
    }

    #[test]
    fn display_column_accounts_for_wide_characters() {
        let i = typed("English-only texta");
        assert_eq!(i.cursor_display_col(), 3);
    }

    #[test]
    fn set_and_clear() {
        let mut i = Input::with_value("abc");
        assert_eq!(i.cursor(), 3);
        i.set("longer value");
        assert_eq!(i.cursor(), 12);
        i.clear();
        assert!(i.is_empty());
        assert_eq!(i.cursor(), 0);
    }

    #[test]
    fn unhandled_keys_report_no_change() {
        let mut i = typed("a");
        assert!(!i.handle_key(code(KeyCode::Enter)));
        assert!(!i.handle_key(code(KeyCode::Esc)));
        assert_eq!(i.value(), "a");
    }
}
