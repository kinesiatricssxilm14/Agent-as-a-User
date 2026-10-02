//! A small text input buffer.
//!
//! Editing works on grapheme clusters rather than chars or bytes: a family emoji or an
//! `e` + combining acute is one thing to the user, so Backspace must remove all of it and the
//! caret must never land inside it. Caret *position* on screen is measured in display columns,
//! which is a different question again — wide CJK glyphs occupy two.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// A single- or multi-line editable buffer with a caret.
#[derive(Debug, Clone, Default)]
pub struct TextInput {
    text: String,
    /// Caret position as a byte offset into `text`, always on a grapheme boundary.
    cursor: usize,
}

impl TextInput {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    /// A buffer pre-filled with `text`, caret at the end. Used to prefill prompts with the
    /// current value so an edit is a tweak rather than a retype.
    pub fn with_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let cursor = text.len();
        Self { text, cursor }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Caret byte offset. Tests assert on it to pin down grapheme-boundary behaviour.
    #[cfg(test)]
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    /// Display width of the text before the caret.
    ///
    /// This, not a character count, is where the terminal cursor belongs.
    pub fn cursor_display_width(&self) -> usize {
        self.text[..self.cursor].width()
    }

    /// Display width of the text before the caret on its own line, for the multi-line editor.
    pub fn cursor_line_and_column(&self) -> (usize, usize) {
        let before = &self.text[..self.cursor];
        let line = before.matches('\n').count();
        let column = match before.rfind('\n') {
            Some(pos) => before[pos + 1..].width(),
            None => before.width(),
        };
        (line, column)
    }

    pub fn insert(&mut self, ch: char) {
        self.text.insert(self.cursor, ch);
        self.cursor += ch.len_utf8();
    }

    pub fn insert_str(&mut self, s: &str) {
        self.text.insert_str(self.cursor, s);
        self.cursor += s.len();
    }

    /// Delete the grapheme before the caret.
    pub fn backspace(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.prev_grapheme_boundary();
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    /// Delete the grapheme after the caret.
    pub fn delete(&mut self) {
        if self.cursor >= self.text.len() {
            return;
        }
        let end = self.next_grapheme_boundary();
        self.text.replace_range(self.cursor..end, "");
    }

    /// Delete the whitespace-delimited word before the caret (Ctrl+W).
    pub fn delete_word(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let before = &self.text[..self.cursor];
        let trimmed_len = before.trim_end().len();
        let start = match before[..trimmed_len].rfind(char::is_whitespace) {
            Some(pos) => pos + before[pos..].chars().next().map_or(1, char::len_utf8),
            None => 0,
        };
        self.text.replace_range(start..self.cursor, "");
        self.cursor = start;
    }

    /// Clear the whole buffer (Ctrl+U).
    pub fn delete_line(&mut self) {
        self.clear();
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.prev_grapheme_boundary();
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor = self.next_grapheme_boundary();
        }
    }

    /// Move the caret to the start of the current line.
    ///
    /// Line-local rather than buffer-local, which is what the multi-line body editor needs; for
    /// a single-line prompt the two are the same thing.
    pub fn move_home(&mut self) {
        self.cursor = match self.text[..self.cursor].rfind('\n') {
            Some(pos) => pos + 1,
            None => 0,
        };
    }

    /// Move the caret to the end of the current line.
    pub fn move_end(&mut self) {
        self.cursor = match self.text[self.cursor..].find('\n') {
            Some(offset) => self.cursor + offset,
            None => self.text.len(),
        };
    }

    /// Move the caret up one line, keeping the column where possible (multi-line editor).
    pub fn move_up(&mut self) {
        let (line, column) = self.cursor_line_and_column();
        if line == 0 {
            self.cursor = 0;
            return;
        }
        self.move_to_line_column(line - 1, column);
    }

    /// Move the caret down one line, keeping the column where possible.
    pub fn move_down(&mut self) {
        let (line, column) = self.cursor_line_and_column();
        let total = self.text.matches('\n').count();
        if line >= total {
            self.cursor = self.text.len();
            return;
        }
        self.move_to_line_column(line + 1, column);
    }

    fn move_to_line_column(&mut self, target_line: usize, target_column: usize) {
        let mut offset = 0usize;
        for (index, line) in self.text.split('\n').enumerate() {
            if index == target_line {
                // Walk graphemes until the column is reached, so the caret stays on a boundary.
                let mut width = 0usize;
                let mut within = line.len();
                for (byte_offset, cluster) in line.grapheme_indices(true) {
                    if width >= target_column {
                        within = byte_offset;
                        break;
                    }
                    width += cluster.width();
                }
                if width < target_column {
                    within = line.len();
                }
                self.cursor = offset + within;
                return;
            }
            offset += line.len() + 1;
        }
        self.cursor = self.text.len();
    }

    fn prev_grapheme_boundary(&self) -> usize {
        self.text[..self.cursor]
            .grapheme_indices(true)
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    fn next_grapheme_boundary(&self) -> usize {
        self.text[self.cursor..]
            .grapheme_indices(true)
            .nth(1)
            .map(|(i, _)| self.cursor + i)
            .unwrap_or(self.text.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(s: &str) -> TextInput {
        let mut input = TextInput::new();
        for ch in s.chars() {
            input.insert(ch);
        }
        input
    }

    #[test]
    fn typing_and_backspace_are_inverses() {
        let mut input = typed("abc");
        assert_eq!(input.text(), "abc");
        input.backspace();
        assert_eq!(input.text(), "ab");
        input.backspace();
        input.backspace();
        input.backspace(); // one too many
        assert_eq!(input.text(), "");
        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn prefilled_input_starts_with_caret_at_end() {
        let input = TextInput::with_text("hello");
        assert_eq!(input.cursor(), 5);
        assert_eq!(input.cursor_display_width(), 5);
    }

    #[test]
    fn caret_moves_over_whole_grapheme_clusters() {
        // A ZWJ family emoji is one cluster made of many chars.
        let family = "👨‍👩‍👧";
        let mut input = TextInput::with_text(family);
        input.move_left();
        assert_eq!(input.cursor(), 0, "one press should skip the whole cluster");
        input.move_right();
        assert_eq!(input.cursor(), family.len());
    }

    #[test]
    fn backspace_removes_a_whole_cluster_not_one_char() {
        let mut input = TextInput::with_text("a👨‍👩‍👧");
        input.backspace();
        assert_eq!(input.text(), "a", "the emoji must vanish in one press");
    }

    #[test]
    fn backspace_handles_combining_marks() {
        // 'e' followed by a combining acute accent.
        let mut input = TextInput::with_text("e\u{0301}");
        input.backspace();
        assert_eq!(input.text(), "", "the base and its mark go together");
    }

    #[test]
    fn cursor_width_accounts_for_wide_glyphs() {
        let input = TextInput::with_text("English-only text");
        // Two CJK glyphs occupy four columns even though they are two chars.
        assert_eq!(input.cursor_display_width(), 4);
    }

    #[test]
    fn insertion_happens_at_the_caret() {
        let mut input = TextInput::with_text("ac");
        input.move_left();
        input.insert('b');
        assert_eq!(input.text(), "abc");
        assert_eq!(input.cursor(), 2);
    }

    #[test]
    fn delete_removes_forwards() {
        let mut input = TextInput::with_text("abc");
        input.move_home();
        input.delete();
        assert_eq!(input.text(), "bc");
        input.move_end();
        input.delete(); // nothing after the caret
        assert_eq!(input.text(), "bc");
    }

    #[test]
    fn delete_word_stops_at_word_boundaries() {
        let mut input = TextInput::with_text("hello world again");
        input.delete_word();
        assert_eq!(input.text(), "hello world ");
        input.delete_word();
        assert_eq!(input.text(), "hello ");
        input.delete_word();
        assert_eq!(input.text(), "");
    }

    #[test]
    fn delete_word_handles_trailing_whitespace_and_empty_input() {
        let mut input = TextInput::with_text("word   ");
        input.delete_word();
        assert_eq!(input.text(), "");
        let mut empty = TextInput::new();
        empty.delete_word(); // must not panic
        assert_eq!(empty.text(), "");
    }

    #[test]
    fn home_and_end_jump_to_the_extremes() {
        let mut input = TextInput::with_text("abc");
        input.move_home();
        assert_eq!(input.cursor(), 0);
        input.move_end();
        assert_eq!(input.cursor(), 3);
    }

    #[test]
    fn multiline_position_reports_line_and_column() {
        let mut input = TextInput::with_text("one\ntwo\nthree");
        assert_eq!(input.cursor_line_and_column(), (2, 5));
        input.move_home();
        assert_eq!(input.cursor_line_and_column(), (2, 0), "home is start of the current line");
        input.move_end();
        assert_eq!(input.cursor_line_and_column(), (2, 5));
    }

    #[test]
    fn home_and_end_are_line_local_in_multiline_text() {
        // Pressing Home on line 2 must not jump to the top of the buffer.
        let mut input = TextInput::with_text("one\ntwo\nthree");
        input.move_up();
        input.move_home();
        assert_eq!(input.cursor(), 4, "start of \"two\"");
        input.move_end();
        assert_eq!(input.cursor(), 7, "end of \"two\", before the newline");
    }

    #[test]
    fn vertical_movement_keeps_the_column_where_possible() {
        let mut input = TextInput::with_text("long line\nab\nlong line");
        // Caret at end of line 3, column 9.
        input.move_up(); // line 2 is shorter, so clamp to its end
        assert_eq!(input.cursor_line_and_column(), (1, 2));
        input.move_up();
        assert_eq!(input.cursor_line_and_column().0, 0);
        input.move_up(); // already at the top
        assert_eq!(input.cursor(), 0);
    }

    #[test]
    fn vertical_movement_clamps_at_the_bottom() {
        let mut input = TextInput::with_text("a\nb");
        input.move_home();
        input.move_down();
        input.move_down();
        assert_eq!(input.cursor(), input.text().len());
    }

    #[test]
    fn vertical_movement_lands_on_grapheme_boundaries_in_wide_text() {
        let mut input = TextInput::with_text("English-only text\nx");
        input.move_end();
        input.move_up();
        // Caret must sit on a char boundary, or slicing would panic elsewhere.
        assert!(input.text().is_char_boundary(input.cursor()));
    }

    #[test]
    fn newlines_can_be_inserted_for_the_body_editor() {
        let mut input = typed("a");
        input.insert('\n');
        input.insert('b');
        assert_eq!(input.text(), "a\nb");
        assert_eq!(input.cursor_line_and_column(), (1, 1));
    }

    #[test]
    fn insert_str_advances_the_caret() {
        let mut input = TextInput::with_text("ab");
        input.move_home();
        input.insert_str("XY");
        assert_eq!(input.text(), "XYab");
        assert_eq!(input.cursor(), 2);
    }

    #[test]
    fn clear_and_delete_line_reset_everything() {
        let mut input = TextInput::with_text("something");
        input.delete_line();
        assert!(input.is_empty());
        assert_eq!(input.cursor(), 0);
    }
}
