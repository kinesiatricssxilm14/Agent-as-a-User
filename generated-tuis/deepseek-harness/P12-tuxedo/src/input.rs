//! A small cursor-aware single-line text buffer used by the input forms.

#[derive(Debug, Clone)]
pub struct LineBuffer {
    chars: Vec<char>,
    cursor: usize,
}

impl LineBuffer {
    pub fn new(s: &str) -> Self {
        let chars: Vec<char> = s.chars().collect();
        let cursor = chars.len();
        Self { chars, cursor }
    }

    pub fn as_string(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_backspace() {
        let mut b = LineBuffer::new("ab");
        b.move_left();
        b.insert_char('X');
        assert_eq!(b.as_string(), "aXb");
        b.backspace();
        assert_eq!(b.as_string(), "ab");
    }

    #[test]
    fn delete_and_cursor() {
        let mut b = LineBuffer::new("abc");
        b.move_home();
        b.delete();
        assert_eq!(b.as_string(), "bc");
        b.move_end();
        assert_eq!(b.cursor(), 2);
    }
}
