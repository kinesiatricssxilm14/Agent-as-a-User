use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Task {
    pub completed: bool,
    pub priority: Option<char>,
    pub description: String,
    pub projects: Vec<String>,
    pub contexts: Vec<String>,
    pub due: Option<String>,
    pub extra: Vec<String>,
}

impl Task {
    pub fn parse(line: &str) -> Self {
        let mut rest = line.trim();
        let completed = rest.starts_with("x ");
        if completed {
            rest = rest[2..].trim_start();
        }

        let mut priority = None;
        if rest.len() >= 4 {
            let bytes = rest.as_bytes();
            if bytes[0] == b'('
                && bytes[2] == b')'
                && bytes[3].is_ascii_whitespace()
                && bytes[1].is_ascii_uppercase()
            {
                priority = Some(bytes[1] as char);
                rest = rest[4..].trim_start();
            }
        } else if rest.len() == 3 {
            let bytes = rest.as_bytes();
            if bytes[0] == b'(' && bytes[2] == b')' && bytes[1].is_ascii_uppercase() {
                priority = Some(bytes[1] as char);
                rest = "";
            }
        }

        let mut description_parts = Vec::new();
        let mut projects = Vec::new();
        let mut contexts = Vec::new();
        let mut due = None;
        let mut extra = Vec::new();

        for token in rest.split_whitespace() {
            if let Some(value) = token.strip_prefix('+').filter(|v| !v.is_empty()) {
                projects.push(value.to_string());
            } else if let Some(value) = token.strip_prefix('@').filter(|v| !v.is_empty()) {
                contexts.push(value.to_string());
            } else if let Some(value) = token.strip_prefix("due:").filter(|v| !v.is_empty()) {
                due = Some(value.to_string());
            } else if token.contains(':') {
                extra.push(token.to_string());
            } else {
                description_parts.push(token);
            }
        }

        Self {
            completed,
            priority,
            description: description_parts.join(" "),
            projects,
            contexts,
            due,
            extra,
        }
    }

    pub fn new(
        description: String,
        priority: Option<char>,
        projects: Vec<String>,
        contexts: Vec<String>,
        due: Option<String>,
    ) -> Self {
        Self {
            completed: false,
            priority,
            description,
            projects,
            contexts,
            due,
            extra: Vec::new(),
        }
    }

    pub fn matches(&self, search: &str, project: &str, context: &str) -> bool {
        let search = search.to_lowercase();
        let text_match = search.is_empty() || self.to_string().to_lowercase().contains(&search);
        let project_match = project.is_empty()
            || self
                .projects
                .iter()
                .any(|tag| tag.eq_ignore_ascii_case(project));
        let context_match = context.is_empty()
            || self
                .contexts
                .iter()
                .any(|tag| tag.eq_ignore_ascii_case(context));
        text_match && project_match && context_match
    }

    pub fn set_priority(&mut self, value: Option<char>) {
        self.priority = value.filter(|c| c.is_ascii_uppercase());
    }
}

impl fmt::Display for Task {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if self.completed {
            parts.push("x".to_string());
        }
        if let Some(priority) = self.priority {
            parts.push(format!("({priority})"));
        }
        if !self.description.is_empty() {
            parts.push(self.description.clone());
        }
        parts.extend(self.projects.iter().map(|tag| format!("+{tag}")));
        parts.extend(self.contexts.iter().map(|tag| format!("@{tag}")));
        if let Some(due) = &self.due {
            parts.push(format!("due:{due}"));
        }
        parts.extend(self.extra.iter().cloned());
        write!(f, "{}", parts.join(" "))
    }
}

pub fn parse_tags(value: &str, prefix: char) -> Vec<String> {
    let mut result = Vec::new();
    for part in value.split(|c: char| c.is_whitespace() || c == ',') {
        let tag = part.trim().trim_start_matches(prefix);
        if !tag.is_empty() && !result.iter().any(|existing| existing == tag) {
            result.push(tag.to_string());
        }
    }
    result
}

pub fn valid_due_date(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    let bytes = value.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return false;
    }
    if !bytes
        .iter()
        .enumerate()
        .all(|(i, b)| i == 4 || i == 7 || b.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<u32>().unwrap_or(0);
    let month = value[5..7].parse::<u32>().unwrap_or(0);
    let day = value[8..10].parse::<u32>().unwrap_or(0);
    year > 0 && (1..=12).contains(&month) && (1..=31).contains(&day)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_task() {
        let task = Task::parse("(B) Buy groceries +errands @home due:2026-03-15");
        assert_eq!(task.priority, Some('B'));
        assert_eq!(task.description, "Buy groceries");
        assert_eq!(task.projects, vec!["errands"]);
        assert_eq!(task.contexts, vec!["home"]);
        assert_eq!(task.due.as_deref(), Some("2026-03-15"));
        assert_eq!(
            task.to_string(),
            "(B) Buy groceries +errands @home due:2026-03-15"
        );
    }

    #[test]
    fn parses_completed_task() {
        let task = Task::parse("x (A) Pay rent +finance @computer due:2026-01-01");
        assert!(task.completed);
        assert_eq!(task.priority, Some('A'));
    }

    #[test]
    fn matching_is_case_insensitive() {
        let task = Task::parse("(A) Write report +Work @Computer");
        assert!(task.matches("REPORT", "work", "computer"));
        assert!(!task.matches("", "personal", ""));
    }
}
