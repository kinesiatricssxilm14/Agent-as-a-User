//! Form (modal input) primitives. A form is a vertical list of labeled
//! fields, all visible at once, with a single focused field.

use chrono::NaiveDate;

/// What a submitted form should do.
#[derive(Debug, Clone)]
pub enum FormAction {
    AddAccount,
    AddCategory,
    AddIncome,
    AddExpense,
    AddBudget,
    EditBudget(i64),
}

#[derive(Debug, Clone)]
pub enum Field {
    Text {
        label: String,
        value: String,
        cursor: usize,
    },
    Money {
        label: String,
        value: String,
        cursor: usize,
    },
    Date {
        label: String,
        value: String,
        cursor: usize,
    },
    Choice {
        label: String,
        options: Vec<String>,
        selected: usize,
    },
}

impl Field {
    pub fn text(label: &str, value: &str) -> Self {
        Field::Text {
            label: label.to_string(),
            value: value.to_string(),
            cursor: value.len(),
        }
    }

    pub fn money(label: &str, value: &str) -> Self {
        Field::Money {
            label: label.to_string(),
            value: value.to_string(),
            cursor: value.len(),
        }
    }

    pub fn date(label: &str, value: &str) -> Self {
        Field::Date {
            label: label.to_string(),
            value: value.to_string(),
            cursor: value.len(),
        }
    }

    pub fn choice(label: &str, options: Vec<String>, selected: usize) -> Self {
        Field::Choice {
            label: label.to_string(),
            options,
            selected,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Field::Text { label, .. }
            | Field::Money { label, .. }
            | Field::Date { label, .. }
            | Field::Choice { label, .. } => label,
        }
    }

    pub fn is_choice(&self) -> bool {
        matches!(self, Field::Choice { .. })
    }

    pub fn is_date(&self) -> bool {
        matches!(self, Field::Date { .. })
    }

    pub fn is_money(&self) -> bool {
        matches!(self, Field::Money { .. })
    }

    /// Current user-visible value as a string (choice -> selected option).
    pub fn value(&self) -> String {
        match self {
            Field::Choice {
                options, selected, ..
            } => options.get(*selected).cloned().unwrap_or_default(),
            Field::Text { value, .. } | Field::Money { value, .. } | Field::Date { value, .. } => {
                value.clone()
            }
        }
    }

    /// Cursor byte position (only meaningful for text-like fields).
    pub fn cursor(&self) -> usize {
        match self {
            Field::Text { cursor, .. }
            | Field::Money { cursor, .. }
            | Field::Date { cursor, .. } => *cursor,
            Field::Choice { .. } => 0,
        }
    }

    pub fn placeholder(&self) -> String {
        match self {
            Field::Money { .. } => "0.00".to_string(),
            Field::Date { .. } => "YYYY-MM-DD".to_string(),
            _ => String::new(),
        }
    }

    fn text_parts(&mut self) -> Option<(&mut String, &mut usize)> {
        match self {
            Field::Text { value, cursor, .. }
            | Field::Money { value, cursor, .. }
            | Field::Date { value, cursor, .. } => Some((value, cursor)),
            Field::Choice { .. } => None,
        }
    }

    pub fn insert_char(&mut self, c: char) {
        if !self.allowed_char(c) {
            return;
        }
        let is_money = self.is_money();
        if let Some((value, cursor)) = self.text_parts() {
            if is_money && c == '.' && value.contains('.') {
                return;
            }
            value.insert(*cursor, c);
            *cursor += c.len_utf8();
        }
    }

    fn allowed_char(&self, c: char) -> bool {
        match self {
            Field::Money { .. } => c.is_ascii_digit() || c == '.',
            Field::Date { .. } => c.is_ascii_digit() || c == '-',
            Field::Text { .. } => !c.is_control(),
            Field::Choice { .. } => false,
        }
    }

    pub fn backspace(&mut self) {
        if let Some((value, cursor)) = self.text_parts() {
            if *cursor == 0 {
                return;
            }
            let prev = prev_char_boundary(value, *cursor);
            value.remove(prev);
            *cursor = prev;
        }
    }

    pub fn cursor_left(&mut self) {
        if let Some((value, cursor)) = self.text_parts() {
            *cursor = prev_char_boundary(value, *cursor);
        }
    }

    pub fn cursor_right(&mut self) {
        if let Some((value, cursor)) = self.text_parts() {
            *cursor = next_char_boundary(value, *cursor);
        }
    }

    pub fn choice_prev(&mut self) {
        if let Field::Choice {
            options, selected, ..
        } = self
        {
            if options.is_empty() {
                return;
            }
            *selected = if *selected == 0 {
                options.len() - 1
            } else {
                *selected - 1
            };
        }
    }

    pub fn choice_next(&mut self) {
        if let Field::Choice {
            options, selected, ..
        } = self
        {
            if options.is_empty() {
                return;
            }
            *selected = (*selected + 1) % options.len();
        }
    }

    /// Shift a Date field by +/- days (for quick date selection).
    pub fn date_shift(&mut self, delta: i64) {
        if let Field::Date { value, cursor, .. } = self {
            if let Ok(d) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
                if let Some(d2) = d.checked_add_signed(chrono::Duration::days(delta)) {
                    let s = d2.format("%Y-%m-%d").to_string();
                    *cursor = s.len();
                    *value = s;
                }
            }
        }
    }
}

fn prev_char_boundary(s: &str, mut i: usize) -> usize {
    if i == 0 {
        return 0;
    }
    i -= 1;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    i += 1;
    while i < s.len() && !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

#[derive(Debug, Clone)]
pub struct Form {
    pub title: String,
    pub fields: Vec<Field>,
    pub focus: usize,
    pub error: Option<String>,
    pub action: FormAction,
}

impl Form {
    pub fn new(title: &str, action: FormAction) -> Self {
        Form {
            title: title.to_string(),
            fields: Vec::new(),
            focus: 0,
            error: None,
            action,
        }
    }
}
