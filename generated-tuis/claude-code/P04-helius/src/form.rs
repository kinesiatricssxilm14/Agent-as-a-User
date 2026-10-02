//! Text input and form primitives.
//!
//! All cursor arithmetic is done in character (not byte) units and rendered
//! using display width, so CJK payee names and notes behave correctly.

use unicode_width::UnicodeWidthStr;

/// A single-line editable text field with a cursor.
#[derive(Debug, Clone, Default)]
pub struct TextInput {
    value: String,
    /// Cursor position, in characters, in `0..=char_count`.
    cursor: usize,
}

impl TextInput {
    pub fn new() -> Self {
        Self::default()
    }

    /// A field pre-filled with `value`, cursor at the end (for edit forms).
    pub fn with_value(value: impl Into<String>) -> Self {
        let value = value.into();
        let cursor = value.chars().count();
        Self { value, cursor }
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn trimmed(&self) -> &str {
        self.value.trim()
    }

    pub fn is_empty(&self) -> bool {
        self.value.trim().is_empty()
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.cursor = self.value.chars().count();
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.cursor = 0;
    }

    pub fn insert(&mut self, c: char) {
        let byte = self.byte_at(self.cursor);
        self.value.insert(byte, c);
        self.cursor += 1;
    }

    pub fn insert_str(&mut self, s: &str) {
        for c in s.chars() {
            self.insert(c);
        }
    }

    /// Backspace: delete the character before the cursor.
    pub fn delete_backward(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let start = self.byte_at(self.cursor - 1);
        let end = self.byte_at(self.cursor);
        self.value.replace_range(start..end, "");
        self.cursor -= 1;
    }

    /// Delete: remove the character at the cursor.
    pub fn delete_forward(&mut self) {
        let len = self.value.chars().count();
        if self.cursor >= len {
            return;
        }
        let start = self.byte_at(self.cursor);
        let end = self.byte_at(self.cursor + 1);
        self.value.replace_range(start..end, "");
    }

    /// Ctrl-W: delete the whitespace-delimited word before the cursor.
    pub fn delete_word_backward(&mut self) {
        let chars: Vec<char> = self.value.chars().collect();
        let mut at = self.cursor;
        while at > 0 && chars[at - 1].is_whitespace() {
            at -= 1;
        }
        while at > 0 && !chars[at - 1].is_whitespace() {
            at -= 1;
        }
        let start = self.byte_at(at);
        let end = self.byte_at(self.cursor);
        self.value.replace_range(start..end, "");
        self.cursor = at;
    }

    /// Ctrl-U: clear everything before the cursor.
    pub fn delete_to_start(&mut self) {
        let end = self.byte_at(self.cursor);
        self.value.replace_range(0..end, "");
        self.cursor = 0;
    }

    /// Ctrl-K: clear everything from the cursor to the end.
    pub fn delete_to_end(&mut self) {
        let start = self.byte_at(self.cursor);
        self.value.truncate(start);
    }

    pub fn move_left(&mut self) {
        self.cursor = self.cursor.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.cursor = (self.cursor + 1).min(self.value.chars().count());
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.value.chars().count();
    }

    /// Display width of the text before the cursor, for placing the terminal
    /// caret. Wide (CJK) glyphs count as two columns.
    pub fn cursor_display_width(&self) -> u16 {
        let byte = self.byte_at(self.cursor);
        UnicodeWidthStr::width(&self.value[..byte]) as u16
    }

    /// Byte offset of character index `n`, clamped to the string length.
    fn byte_at(&self, n: usize) -> usize {
        self.value
            .char_indices()
            .nth(n)
            .map(|(i, _)| i)
            .unwrap_or(self.value.len())
    }
}

/// What sort of value a form field holds. Drives validation and which editing
/// affordances the field offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// Free text. `required` rejects an all-whitespace value.
    Text { required: bool },
    /// A positive yuan amount, parsed into cents.
    Amount,
    /// A `YYYY-MM-DD` date; empty means today.
    Date,
    /// A `YYYY-MM` month; empty means the current month.
    Month,
    /// A pick-one list navigated with ←/→ (or h/l). `options` may be empty,
    /// which is a validation error unless the field is optional.
    Choice { required: bool },
}

/// One row of a form.
#[derive(Debug, Clone)]
pub struct Field {
    pub label: String,
    pub kind: FieldKind,
    pub input: TextInput,
    /// Options for a `Choice` field: (id, display label). The id is `None` for
    /// sentinel entries such as "All accounts" or "(uncategorized)".
    pub options: Vec<(Option<i64>, String)>,
    pub selected: usize,
    /// One-line hint shown under the form while this field is focused.
    pub hint: String,
}

impl Field {
    pub fn text(label: &str, required: bool, hint: &str) -> Self {
        Self {
            label: label.to_string(),
            kind: FieldKind::Text { required },
            input: TextInput::new(),
            options: Vec::new(),
            selected: 0,
            hint: hint.to_string(),
        }
    }

    pub fn amount(label: &str, hint: &str) -> Self {
        Self {
            label: label.to_string(),
            kind: FieldKind::Amount,
            input: TextInput::new(),
            options: Vec::new(),
            selected: 0,
            hint: hint.to_string(),
        }
    }

    pub fn date(label: &str, default: &str, hint: &str) -> Self {
        Self {
            label: label.to_string(),
            kind: FieldKind::Date,
            input: TextInput::with_value(default),
            options: Vec::new(),
            selected: 0,
            hint: hint.to_string(),
        }
    }

    pub fn month(label: &str, default: &str, hint: &str) -> Self {
        Self {
            label: label.to_string(),
            kind: FieldKind::Month,
            input: TextInput::with_value(default),
            options: Vec::new(),
            selected: 0,
            hint: hint.to_string(),
        }
    }

    pub fn choice(
        label: &str,
        options: Vec<(Option<i64>, String)>,
        required: bool,
        hint: &str,
    ) -> Self {
        Self {
            label: label.to_string(),
            kind: FieldKind::Choice { required },
            input: TextInput::new(),
            options,
            selected: 0,
            hint: hint.to_string(),
        }
    }

    pub fn with_value(mut self, value: &str) -> Self {
        self.input.set_value(value);
        self
    }

    /// Preselect the option whose id matches `id`; leaves the selection alone
    /// if there is no such option.
    pub fn select_id(mut self, id: Option<i64>) -> Self {
        if let Some(idx) = self.options.iter().position(|(oid, _)| *oid == id) {
            self.selected = idx;
        }
        self
    }

    pub fn is_choice(&self) -> bool {
        matches!(self.kind, FieldKind::Choice { .. })
    }

    /// The id of the selected option, or `None` for a sentinel/empty choice.
    pub fn selected_id(&self) -> Option<i64> {
        self.options.get(self.selected).and_then(|(id, _)| *id)
    }

    /// Text shown in the field's value column.
    pub fn display_value(&self) -> String {
        match self.kind {
            FieldKind::Choice { .. } => match self.options.get(self.selected) {
                Some((_, label)) => label.clone(),
                None => "(none available)".to_string(),
            },
            _ => self.input.value().to_string(),
        }
    }

    pub fn next_option(&mut self) {
        if self.options.is_empty() {
            return;
        }
        self.selected = (self.selected + 1) % self.options.len();
    }

    pub fn prev_option(&mut self) {
        if self.options.is_empty() {
            return;
        }
        self.selected = if self.selected == 0 {
            self.options.len() - 1
        } else {
            self.selected - 1
        };
    }

    /// Validate the field, returning a message suitable for the status bar.
    pub fn validate(&self) -> Result<(), String> {
        match self.kind {
            FieldKind::Text { required } => {
                if required && self.input.is_empty() {
                    Err(format!("{} is required", self.label))
                } else {
                    Ok(())
                }
            }
            FieldKind::Amount => crate::money::parse_positive_cents(self.input.value())
                .map(|_| ())
                .map_err(|e| format!("{}: {e}", self.label)),
            FieldKind::Date => crate::date::parse_date(self.input.value())
                .map(|_| ())
                .map_err(|e| format!("{}: {e}", self.label)),
            FieldKind::Month => crate::date::parse_month(self.input.value())
                .map(|_| ())
                .map_err(|e| format!("{}: {e}", self.label)),
            FieldKind::Choice { required } => {
                if required && self.options.is_empty() {
                    Err(format!("{} has no options to choose from", self.label))
                } else {
                    Ok(())
                }
            }
        }
    }

    /// Parsed amount in cents. Only meaningful for an `Amount` field.
    pub fn amount_cents(&self) -> Result<i64, String> {
        crate::money::parse_positive_cents(self.input.value())
            .map_err(|e| format!("{}: {e}", self.label))
    }

    /// Canonical `YYYY-MM-DD`. Only meaningful for a `Date` field.
    pub fn date_value(&self) -> Result<String, String> {
        crate::date::parse_date(self.input.value()).map_err(|e| format!("{}: {e}", self.label))
    }

    /// Canonical `YYYY-MM`. Only meaningful for a `Month` field.
    pub fn month_value(&self) -> Result<String, String> {
        crate::date::parse_month(self.input.value()).map_err(|e| format!("{}: {e}", self.label))
    }
}

/// A vertical stack of fields with one focused row.
#[derive(Debug, Clone)]
pub struct Form {
    pub fields: Vec<Field>,
    pub focus: usize,
}

impl Form {
    pub fn new(fields: Vec<Field>) -> Self {
        Self { fields, focus: 0 }
    }

    pub fn focused(&self) -> &Field {
        &self.fields[self.focus]
    }

    pub fn focused_mut(&mut self) -> &mut Field {
        &mut self.fields[self.focus]
    }

    pub fn focus_next(&mut self) {
        if !self.fields.is_empty() {
            self.focus = (self.focus + 1) % self.fields.len();
        }
    }

    pub fn focus_prev(&mut self) {
        if self.fields.is_empty() {
            return;
        }
        self.focus = if self.focus == 0 {
            self.fields.len() - 1
        } else {
            self.focus - 1
        };
    }

    /// Validate every field, reporting the first problem and focusing it so the
    /// user lands on the field that needs attention.
    pub fn validate(&mut self) -> Result<(), String> {
        for (idx, field) in self.fields.iter().enumerate() {
            if let Err(e) = field.validate() {
                self.focus = idx;
                return Err(e);
            }
        }
        Ok(())
    }

    pub fn field(&self, index: usize) -> &Field {
        &self.fields[index]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_ascii_text() {
        let mut i = TextInput::new();
        i.insert_str("hello");
        assert_eq!(i.value(), "hello");
        assert_eq!(i.cursor(), 5);

        i.move_left();
        i.insert('X');
        assert_eq!(i.value(), "hellXo");

        i.delete_backward();
        assert_eq!(i.value(), "hello");

        i.move_home();
        i.delete_forward();
        assert_eq!(i.value(), "ello");
    }

    #[test]
    fn edits_multibyte_text_by_character() {
        let mut i = TextInput::with_value("English-only text");
        assert_eq!(i.cursor(), 3);
        // Each CJK glyph is 3 bytes but 1 character and 2 display columns.
        assert_eq!(i.cursor_display_width(), 6);

        i.delete_backward();
        assert_eq!(i.value(), "English-only text");
        assert_eq!(i.cursor(), 2);

        i.move_home();
        i.insert('English-only text');
        assert_eq!(i.value(), "English-only text");
        assert_eq!(i.cursor_display_width(), 2);

        i.move_end();
        i.delete_word_backward();
        assert_eq!(i.value(), "");
    }

    #[test]
    fn cursor_clamps_at_both_ends() {
        let mut i = TextInput::with_value("ab");
        i.move_home();
        i.move_left();
        i.move_left();
        assert_eq!(i.cursor(), 0);
        i.delete_backward(); // no-op at the start
        assert_eq!(i.value(), "ab");

        i.move_end();
        i.move_right();
        assert_eq!(i.cursor(), 2);
        i.delete_forward(); // no-op at the end
        assert_eq!(i.value(), "ab");
    }

    #[test]
    fn word_and_line_deletions() {
        let mut i = TextInput::with_value("one two three");
        i.delete_word_backward();
        assert_eq!(i.value(), "one two ");

        let mut i = TextInput::with_value("one two  ");
        i.delete_word_backward();
        assert_eq!(i.value(), "one ");

        let mut i = TextInput::with_value("abcdef");
        i.move_home();
        i.move_right();
        i.move_right();
        i.delete_to_start();
        assert_eq!(i.value(), "cdef");
        assert_eq!(i.cursor(), 0);

        let mut i = TextInput::with_value("abcdef");
        i.move_home();
        i.move_right();
        i.delete_to_end();
        assert_eq!(i.value(), "a");
    }

    #[test]
    fn choice_field_cycles_both_ways() {
        let mut f = Field::choice(
            "Account",
            vec![(Some(1), "personal".into()), (Some(2), "business".into())],
            true,
            "",
        );
        assert_eq!(f.selected_id(), Some(1));
        f.next_option();
        assert_eq!(f.selected_id(), Some(2));
        assert_eq!(f.display_value(), "business");
        f.next_option();
        assert_eq!(f.selected_id(), Some(1), "wraps forward");
        f.prev_option();
        assert_eq!(f.selected_id(), Some(2), "wraps backward");
    }

    #[test]
    fn empty_choice_is_safe() {
        let mut f = Field::choice("Account", vec![], true, "");
        f.next_option();
        f.prev_option();
        assert_eq!(f.selected_id(), None);
        assert_eq!(f.display_value(), "(none available)");
        assert!(f.validate().is_err());

        let optional = Field::choice("Category", vec![], false, "");
        assert!(optional.validate().is_ok());
    }

    #[test]
    fn preselects_matching_option() {
        let f = Field::choice(
            "Account",
            vec![(None, "All accounts".into()), (Some(7), "personal".into())],
            false,
            "",
        )
        .select_id(Some(7));
        assert_eq!(f.selected_id(), Some(7));

        // An id that isn't present leaves the default selection.
        let f = f.select_id(Some(999));
        assert_eq!(f.selected_id(), Some(7));
    }

    #[test]
    fn validates_field_kinds() {
        assert!(Field::text("Name", true, "").validate().is_err());
        assert!(Field::text("Notes", false, "").validate().is_ok());
        assert!(Field::text("Name", true, "").with_value("x").validate().is_ok());

        assert!(Field::amount("Amount", "").validate().is_err());
        assert!(Field::amount("Amount", "").with_value("12.34").validate().is_ok());
        assert!(Field::amount("Amount", "").with_value("0").validate().is_err());
        assert!(Field::amount("Amount", "").with_value("1.234").validate().is_err());

        // An empty date/month is allowed and means today / this month.
        assert!(Field::date("Date", "", "").validate().is_ok());
        assert!(Field::date("Date", "", "").with_value("2026-02-30").validate().is_err());
        assert!(Field::month("Month", "", "").validate().is_ok());
        assert!(Field::month("Month", "", "").with_value("2026-13").validate().is_err());
    }

    #[test]
    fn form_focus_wraps_and_validation_focuses_first_error() {
        let mut form = Form::new(vec![
            Field::text("Name", false, ""),
            Field::amount("Amount", ""),
            Field::text("Notes", false, ""),
        ]);
        assert_eq!(form.focus, 0);
        form.focus_prev();
        assert_eq!(form.focus, 2, "wraps to the last field");
        form.focus_next();
        assert_eq!(form.focus, 0, "wraps to the first field");

        let err = form.validate().unwrap_err();
        assert!(err.contains("Amount"), "got: {err}");
        assert_eq!(form.focus, 1, "focus moves to the invalid field");

        form.fields[1].input.set_value("10.00");
        assert!(form.validate().is_ok());
    }

    #[test]
    fn parses_typed_field_values() {
        let f = Field::amount("Amount", "").with_value("¥1,234.56");
        assert_eq!(f.amount_cents(), Ok(123_456));

        let f = Field::date("Date", "", "").with_value("2026/8/3");
        assert_eq!(f.date_value().unwrap(), "2026-08-03");

        let f = Field::month("Month", "", "").with_value("2026/8");
        assert_eq!(f.month_value().unwrap(), "2026-08");
    }
}
