//! The todo.txt data model: parsing, formatting and date helpers.

use std::collections::BTreeMap;

/// A single todo.txt task.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub completed: bool,
    pub completion_date: Option<String>,
    pub priority: Option<char>,
    pub creation_date: Option<String>,
    pub description: String,
    pub projects: Vec<String>,
    pub contexts: Vec<String>,
    /// Additional `key:value` metadata such as `due:2026-03-15`.
    pub metadata: BTreeMap<String, String>,
}

impl Default for Task {
    fn default() -> Self {
        Task {
            completed: false,
            completion_date: None,
            priority: None,
            creation_date: None,
            description: String::new(),
            projects: Vec::new(),
            contexts: Vec::new(),
            metadata: BTreeMap::new(),
        }
    }
}

impl Task {
    pub fn new(description: &str) -> Self {
        let mut t = Task::default();
        t.description = description.trim().to_string();
        t
    }

    pub fn due_date(&self) -> Option<&str> {
        self.metadata.get("due").map(|s| s.as_str())
    }

    pub fn set_due_date(&mut self, d: Option<&str>) {
        match d {
            Some(d) if !d.is_empty() => {
                self.metadata.insert("due".to_string(), d.to_string());
            }
            _ => {
                self.metadata.remove("due");
            }
        }
    }

    /// True when the task has a due date in the past and is still open.
    pub fn is_overdue(&self) -> bool {
        if self.completed {
            return false;
        }
        let Some(due) = self.due_date() else {
            return false;
        };
        let Some(days) = date_to_days(due) else {
            return false;
        };
        days < today_days()
    }

    /// Parse one todo.txt line. Returns `None` only for empty/blank lines.
    pub fn parse(line: &str) -> Option<Task> {
        let line = line.trim();
        if line.is_empty() {
            return None;
        }
        let mut tokens: Vec<&str> = line.split_whitespace().collect();
        let mut t = Task::default();

        if tokens[0] == "x" {
            t.completed = true;
            tokens.remove(0);
            if let Some(&d) = tokens.first() {
                if is_date(d) {
                    t.completion_date = Some(d.to_string());
                    tokens.remove(0);
                }
            }
        }

        if let Some(&first) = tokens.first() {
            if let Some(p) = parse_priority(first) {
                t.priority = Some(p);
                tokens.remove(0);
                if let Some(&d) = tokens.first() {
                    if is_date(d) {
                        t.creation_date = Some(d.to_string());
                        tokens.remove(0);
                    }
                }
            }
        }

        let mut desc: Vec<String> = Vec::new();
        for tok in tokens {
            if let Some(p) = tok.strip_prefix('+') {
                if !p.is_empty() && !t.projects.iter().any(|x| x == p) {
                    t.projects.push(p.to_string());
                }
                continue;
            }
            if let Some(c) = tok.strip_prefix('@') {
                if !c.is_empty() && !t.contexts.iter().any(|x| x == c) {
                    t.contexts.push(c.to_string());
                }
                continue;
            }
            if let Some((k, v)) = split_key_value(tok) {
                t.metadata.insert(k, v);
                continue;
            }
            desc.push(tok.to_string());
        }
        t.description = desc.join(" ");
        Some(t)
    }

    /// Serialize back to canonical todo.txt form (one line).
    pub fn format(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.completed {
            parts.push("x".to_string());
            if let Some(d) = &self.completion_date {
                parts.push(d.clone());
            }
        }
        if let Some(p) = self.priority {
            parts.push(format!("({p})"));
        }
        if let Some(d) = &self.creation_date {
            parts.push(d.clone());
        }
        if !self.description.is_empty() {
            parts.push(self.description.clone());
        }
        for p in &self.projects {
            parts.push(format!("+{p}"));
        }
        for c in &self.contexts {
            parts.push(format!("@{c}"));
        }
        for (k, v) in &self.metadata {
            parts.push(format!("{k}:{v}"));
        }
        parts.join(" ")
    }
}

fn parse_priority(s: &str) -> Option<char> {
    let b = s.as_bytes();
    if b.len() == 3 && b[0] == b'(' && b[2] == b')' && b[1].is_ascii_uppercase() {
        Some(b[1] as char)
    } else {
        None
    }
}

/// Split a `key:value` token. Values that look like URLs (`http://…`) are
/// left as description text rather than being treated as metadata.
fn split_key_value(tok: &str) -> Option<(String, String)> {
    let i = tok.find(':')?;
    if i == 0 {
        return None;
    }
    let key = &tok[..i];
    let val = &tok[i + 1..];
    if val.is_empty() || val.starts_with("//") {
        return None;
    }
    if key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        Some((key.to_string(), val.to_string()))
    } else {
        None
    }
}

/// Format check for `YYYY-MM-DD` (does not validate the calendar).
pub fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[0..4].iter().all(u8::is_ascii_digit)
        && b[5..7].iter().all(u8::is_ascii_digit)
        && b[8..10].iter().all(u8::is_ascii_digit)
}

/// Strict calendar validation for `YYYY-MM-DD`.
pub fn is_valid_date(s: &str) -> bool {
    if !is_date(s) {
        return false;
    }
    let Some(days) = date_to_days(s) else {
        return false;
    };
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}") == s
}

pub fn date_to_days(s: &str) -> Option<i64> {
    if !is_date(s) {
        return None;
    }
    let y: i64 = s[0..4].parse().ok()?;
    let m: u32 = s[5..7].parse().ok()?;
    let d: u32 = s[8..10].parse().ok()?;
    Some(days_from_civil(y, m, d))
}

pub fn today_days() -> i64 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    secs.div_euclid(86_400)
}

pub fn today() -> String {
    let (y, m, d) = civil_from_days(today_days());
    format!("{y:04}-{m:02}-{d:02}")
}

// Days-since-epoch conversion (Howard Hinnant's civil-from-days algorithm).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_incomplete_task() {
        let t = Task::parse("(B) Buy groceries +errands @home due:2026-03-15").unwrap();
        assert!(!t.completed);
        assert_eq!(t.priority, Some('B'));
        assert_eq!(t.description, "Buy groceries");
        assert_eq!(t.projects, vec!["errands"]);
        assert_eq!(t.contexts, vec!["home"]);
        assert_eq!(t.due_date(), Some("2026-03-15"));
    }

    #[test]
    fn parse_complete_task() {
        let t = Task::parse("x (A) Pay rent +finance @computer due:2026-01-01").unwrap();
        assert!(t.completed);
        assert_eq!(t.priority, Some('A'));
        assert_eq!(t.description, "Pay rent");
        assert_eq!(t.projects, vec!["finance"]);
        assert_eq!(t.contexts, vec!["computer"]);
        assert_eq!(t.due_date(), Some("2026-01-01"));
    }

    #[test]
    fn parse_complete_with_date() {
        let t = Task::parse("x 2026-01-02 (A) 2026-01-01 Pay rent").unwrap();
        assert!(t.completed);
        assert_eq!(t.completion_date.as_deref(), Some("2026-01-02"));
        assert_eq!(t.creation_date.as_deref(), Some("2026-01-01"));
        assert_eq!(t.priority, Some('A'));
        assert_eq!(t.description, "Pay rent");
    }

    #[test]
    fn round_trips() {
        for line in [
            "(B) Buy groceries +errands @home due:2026-03-15",
            "x (A) Pay rent +finance @computer due:2026-01-01",
            "plain task with no metadata",
            "(A) 2026-01-01 Write report +work @computer due:2026-02-01",
            "x 2026-01-05 (C) Call mom @phone",
        ] {
            let t = Task::parse(line).unwrap();
            let rt = Task::parse(&t.format()).unwrap();
            assert_eq!(t, rt, "round trip failed for: {line}");
        }
    }

    #[test]
    fn url_is_description_not_metadata() {
        let t = Task::parse("Read https://example.com today").unwrap();
        assert!(t.metadata.is_empty());
        assert_eq!(t.description, "Read https://example.com today");
    }

    #[test]
    fn generic_key_value_metadata() {
        let t = Task::parse("Ship it +work due:2026-03-01 rec:1w").unwrap();
        assert_eq!(t.metadata.get("due").map(|s| s.as_str()), Some("2026-03-01"));
        assert_eq!(t.metadata.get("rec").map(|s| s.as_str()), Some("1w"));
    }

    #[test]
    fn date_validation() {
        assert!(is_valid_date("2026-03-15"));
        assert!(is_valid_date("2000-02-29"));
        assert!(!is_valid_date("2026-02-30"));
        assert!(!is_valid_date("2026-13-01"));
        assert!(!is_valid_date("2026-00-10"));
        assert!(!is_valid_date("26-03-15"));
    }

    #[test]
    fn days_round_trip() {
        assert_eq!(date_to_days("1970-01-01"), Some(0));
        let today = today();
        let d = date_to_days(&today).unwrap();
        let (y, m, dd) = civil_from_days(d);
        assert_eq!(format!("{y:04}-{m:02}-{dd:02}"), today);
    }
}
