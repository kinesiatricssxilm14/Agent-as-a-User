//! todo.txt parsing, mutation and serialisation.
//!
//! A [`Task`] keeps its description as the raw text that followed the metadata
//! prefix, plus the order in which that prefix appeared on disk. That way a task
//! that is loaded and written back without being edited is reproduced
//! byte-for-byte, which matters because `tooll` rewrites the whole file on every
//! change.

use crate::date::{self, Ymd};

/// Which piece of leading metadata occupied a given prefix slot.
///
/// todo.txt puts priority, completion date and creation date before the
/// description, and real-world files disagree about the order (the spec drops
/// priority on completion, most tools keep it). Recording the observed order
/// lets us round-trip either dialect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Priority,
    CompletionDate,
    CreationDate,
}

/// A single todo.txt entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// Stable identifier, handed out by the store; not part of the file format.
    pub id: u32,
    pub completed: bool,
    priority: Option<char>,
    completion_date: Option<Ymd>,
    creation_date: Option<Ymd>,
    /// Everything after the metadata prefix, verbatim.
    description: String,
    /// Order of the metadata prefix as parsed (or as canonically constructed).
    slots: Vec<Slot>,
}

/// Classification of a whitespace-separated description token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Project,
    Context,
    /// A `key:value` pair whose key is `due`.
    Due,
    /// Any other `key:value` pair.
    KeyValue,
    Word,
}

/// Classify a single description token.
pub fn classify_token(tok: &str) -> TokenKind {
    if tok.len() > 1 && tok.starts_with('+') {
        return TokenKind::Project;
    }
    if tok.len() > 1 && tok.starts_with('@') {
        return TokenKind::Context;
    }
    if let Some((k, v)) = split_key_value(tok) {
        if k.eq_ignore_ascii_case("due") {
            return TokenKind::Due;
        }
        let _ = v;
        return TokenKind::KeyValue;
    }
    TokenKind::Word
}

/// Split a `key:value` token. Both halves must be non-empty and colon-free.
fn split_key_value(tok: &str) -> Option<(&str, &str)> {
    let idx = tok.find(':')?;
    let (k, rest) = tok.split_at(idx);
    let v = &rest[1..];
    if k.is_empty() || v.is_empty() || v.contains(':') {
        return None;
    }
    Some((k, v))
}

/// Strip the leading `+`/`@` sigil from a tag token.
fn bare_tag(tok: &str) -> &str {
    &tok[1..]
}

/// Normalise a user-typed tag name: drop a leading sigil and surrounding
/// whitespace, and collapse internal whitespace to `-` so the token stays a
/// single todo.txt word.
pub fn normalize_tag_name(raw: &str, sigil: char) -> Option<String> {
    let mut s = raw.trim();
    while let Some(rest) = s.strip_prefix(sigil) {
        s = rest.trim_start();
    }
    if s.is_empty() {
        return None;
    }
    let joined = s.split_whitespace().collect::<Vec<_>>().join("-");
    if joined.is_empty() {
        None
    } else {
        Some(joined)
    }
}

/// Split a user-typed tag list on commas and/or whitespace.
pub fn parse_tag_list(raw: &str, sigil: char) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for piece in raw.split(|c: char| c == ',' || c.is_whitespace()) {
        if let Some(name) = normalize_tag_name(piece, sigil) {
            if !out.iter().any(|e| e == &name) {
                out.push(name);
            }
        }
    }
    out
}

impl Task {
    /// Parse one line of a todo.txt file.
    ///
    /// Never fails: text that does not match any metadata shape simply becomes
    /// part of the description, which is what todo.txt implementations do.
    pub fn parse(line: &str, id: u32) -> Task {
        let mut rest = line.trim_start();
        let mut completed = false;
        let mut priority = None;
        let mut dates: Vec<Ymd> = Vec::new();
        let mut slots: Vec<Slot> = Vec::new();

        // The completion marker is exactly `x` followed by whitespace.
        if let Some(after) = strip_marker(rest) {
            completed = true;
            rest = after;
        }

        // Priority and up to two dates may follow in either order.
        loop {
            if priority.is_none() {
                if let Some((p, after)) = strip_priority(rest) {
                    priority = Some(p);
                    slots.push(Slot::Priority);
                    rest = after;
                    continue;
                }
            }
            if dates.len() < 2 {
                if let Some((d, after)) = strip_date(rest) {
                    // On a completed task the first date is the completion date.
                    let slot = if completed && dates.is_empty() {
                        Slot::CompletionDate
                    } else {
                        Slot::CreationDate
                    };
                    // An incomplete task cannot carry a completion date, and no
                    // task carries two creation dates.
                    if slots.contains(&slot) {
                        break;
                    }
                    dates.push(d);
                    slots.push(slot);
                    rest = after;
                    continue;
                }
            }
            break;
        }

        let mut completion_date = None;
        let mut creation_date = None;
        let mut it = dates.into_iter();
        for slot in &slots {
            match slot {
                Slot::CompletionDate => completion_date = it.next(),
                Slot::CreationDate => creation_date = it.next(),
                Slot::Priority => {}
            }
        }

        Task {
            id,
            completed,
            priority,
            completion_date,
            creation_date,
            description: rest.trim_end().to_string(),
            slots,
        }
    }

    /// A brand new task built from free-form todo.txt text.
    pub fn from_input(text: &str, id: u32) -> Task {
        Task::parse(text, id)
    }

    /// Serialise back to a single todo.txt line.
    pub fn render(&self) -> String {
        let mut out = String::new();
        if self.completed {
            out.push_str("x ");
        }
        for slot in &self.slots {
            match slot {
                Slot::Priority => {
                    if let Some(p) = self.priority {
                        out.push('(');
                        out.push(p);
                        out.push_str(") ");
                    }
                }
                Slot::CompletionDate => {
                    if let Some(d) = self.completion_date {
                        out.push_str(&date::format_ymd(d));
                        out.push(' ');
                    }
                }
                Slot::CreationDate => {
                    if let Some(d) = self.creation_date {
                        out.push_str(&date::format_ymd(d));
                        out.push(' ');
                    }
                }
            }
        }
        out.push_str(&self.description);
        // A description-less task must not leave a dangling separator.
        while out.ends_with(' ') {
            out.pop();
        }
        out
    }

    pub fn priority(&self) -> Option<char> {
        self.priority
    }

    pub fn creation_date(&self) -> Option<Ymd> {
        self.creation_date
    }

    pub fn completion_date(&self) -> Option<Ymd> {
        self.completion_date
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    /// The description with tags and `key:value` pairs removed, i.e. the part a
    /// human actually wrote.
    pub fn summary(&self) -> String {
        self.tokens()
            .iter()
            .filter(|t| classify_token(t) == TokenKind::Word)
            .cloned()
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Whitespace-separated description tokens.
    pub fn tokens(&self) -> Vec<String> {
        self.description
            .split_whitespace()
            .map(str::to_string)
            .collect()
    }

    pub fn projects(&self) -> Vec<String> {
        self.tags('+')
    }

    pub fn contexts(&self) -> Vec<String> {
        self.tags('@')
    }

    fn tags(&self, sigil: char) -> Vec<String> {
        let want = if sigil == '+' {
            TokenKind::Project
        } else {
            TokenKind::Context
        };
        let mut out: Vec<String> = Vec::new();
        for tok in self.description.split_whitespace() {
            if classify_token(tok) == want {
                let name = bare_tag(tok).to_string();
                if !out.iter().any(|e| e == &name) {
                    out.push(name);
                }
            }
        }
        out
    }

    /// The `due:` value, if it parses as a real calendar date.
    pub fn due_date(&self) -> Option<Ymd> {
        self.due_raw().and_then(date::parse_ymd)
    }

    /// The raw `due:` value, even when it is not a valid date.
    pub fn due_raw(&self) -> Option<&str> {
        for tok in self.description.split_whitespace() {
            if let Some((k, v)) = split_key_value(tok) {
                if k.eq_ignore_ascii_case("due") {
                    return Some(v);
                }
            }
        }
        None
    }

    /// Set the priority letter, or clear it with `None`.
    ///
    /// Returns `false` when nothing changed.
    pub fn set_priority(&mut self, p: Option<char>) -> bool {
        let p = p.map(|c| c.to_ascii_uppercase());
        if let Some(c) = p {
            if !c.is_ascii_uppercase() {
                return false;
            }
        }
        if self.priority == p {
            return false;
        }
        self.priority = p;
        match p {
            None => self.slots.retain(|s| *s != Slot::Priority),
            Some(_) => {
                if !self.slots.contains(&Slot::Priority) {
                    // Canonical position: immediately after the `x ` marker.
                    self.slots.insert(0, Slot::Priority);
                }
            }
        }
        true
    }

    /// Move one step up the `A > B > ... > Z > (none)` scale.
    pub fn raise_priority(&mut self) -> bool {
        let next = match self.priority {
            None => Some('A'),
            Some('A') => return false,
            Some(c) => Some((c as u8 - 1) as char),
        };
        self.set_priority(next)
    }

    /// Move one step down the `A > B > ... > Z > (none)` scale.
    pub fn lower_priority(&mut self) -> bool {
        let next = match self.priority {
            None => return false,
            Some('Z') => None,
            Some(c) => Some((c as u8 + 1) as char),
        };
        self.set_priority(next)
    }

    /// Mark complete, stamping `today` as the completion date.
    ///
    /// A completion date is only meaningful next to a creation date per the
    /// todo.txt spec, so one is synthesised when absent.
    pub fn complete(&mut self, today: Ymd) -> bool {
        if self.completed {
            return false;
        }
        self.completed = true;
        self.completion_date = Some(today);
        if self.creation_date.is_none() {
            self.creation_date = Some(today);
            self.slots.push(Slot::CreationDate);
        }
        // Completion date must precede the creation date.
        self.slots.retain(|s| *s != Slot::CompletionDate);
        let at = self
            .slots
            .iter()
            .position(|s| *s == Slot::CreationDate)
            .unwrap_or(self.slots.len());
        self.slots.insert(at, Slot::CompletionDate);
        true
    }

    /// Reopen a completed task, dropping its completion date.
    pub fn uncomplete(&mut self) -> bool {
        if !self.completed {
            return false;
        }
        self.completed = false;
        self.completion_date = None;
        self.slots.retain(|s| *s != Slot::CompletionDate);
        true
    }

    pub fn toggle_complete(&mut self, today: Ymd) -> bool {
        if self.completed {
            self.uncomplete()
        } else {
            self.complete(today)
        }
    }

    /// Replace the whole description (tags and all).
    pub fn set_description(&mut self, text: &str) {
        self.description = text.trim().to_string();
    }

    /// Replace the set of `+project` tags.
    ///
    /// Surviving tags keep their position and a replaced tag hands its slot to a
    /// new one, so repeated edits do not shuffle the rest of the line.
    pub fn set_projects(&mut self, names: &[String]) -> bool {
        self.set_tags('+', names)
    }

    /// Replace the set of `@context` tags. See [`set_projects`](Self::set_projects)
    /// for how positions are preserved.
    pub fn set_contexts(&mut self, names: &[String]) -> bool {
        self.set_tags('@', names)
    }

    /// Add a single tag if it is not already present.
    pub fn add_tag(&mut self, sigil: char, name: &str) -> bool {
        let name = match normalize_tag_name(name, sigil) {
            Some(n) => n,
            None => return false,
        };
        let mut current = if sigil == '+' {
            self.projects()
        } else {
            self.contexts()
        };
        if current.iter().any(|c| c == &name) {
            return false;
        }
        current.push(name);
        self.set_tags(sigil, &current)
    }

    fn set_tags(&mut self, sigil: char, names: &[String]) -> bool {
        let want = if sigil == '+' {
            TokenKind::Project
        } else {
            TokenKind::Context
        };
        // Normalise and de-duplicate the requested set.
        let mut wanted: Vec<String> = Vec::new();
        for n in names {
            if let Some(n) = normalize_tag_name(n, sigil) {
                if !wanted.iter().any(|e| e == &n) {
                    wanted.push(n);
                }
            }
        }

        // Tags that are new to this task, consumed in order as existing tag
        // slots are vacated. Reusing a slot keeps `a @home b` -> `a @town b`
        // rather than pushing the replacement to the end of the line.
        let existing: Vec<String> = self.tags(sigil);
        let mut incoming: Vec<&String> = wanted
            .iter()
            .filter(|w| !existing.iter().any(|e| e == *w))
            .collect();
        incoming.reverse(); // so `pop` yields them in the requested order.

        let mut emitted: Vec<String> = Vec::new();
        let mut out: Vec<String> = Vec::new();
        for tok in self.description.split_whitespace() {
            if classify_token(tok) == want {
                let name = bare_tag(tok);
                if wanted.iter().any(|w| w == name) {
                    // Keep it, unless this is a duplicate of one already emitted.
                    if !emitted.iter().any(|e| e == name) {
                        emitted.push(name.to_string());
                        out.push(format!("{sigil}{name}"));
                    }
                } else if let Some(replacement) = incoming.pop() {
                    // This tag is going away; give its slot to a new tag.
                    emitted.push(replacement.clone());
                    out.push(format!("{sigil}{replacement}"));
                }
                // Otherwise the tag is dropped with nothing to take its place.
            } else {
                out.push(tok.to_string());
            }
        }
        // Anything with no slot to reuse goes at the end, in requested order.
        for name in &wanted {
            if !emitted.iter().any(|e| e == name) {
                out.push(format!("{sigil}{name}"));
            }
        }

        let next = out.join(" ");
        if next == self.description {
            return false;
        }
        self.description = next;
        true
    }

    /// Set or clear the `due:` field. `None` removes it.
    pub fn set_due(&mut self, due: Option<Ymd>) -> bool {
        self.set_due_raw(due.map(date::format_ymd).as_deref())
    }

    /// Set or clear the `due:` field from raw text (already validated upstream).
    pub fn set_due_raw(&mut self, value: Option<&str>) -> bool {
        let mut out: Vec<String> = Vec::new();
        let mut replaced = false;
        for tok in self.description.split_whitespace() {
            let is_due = matches!(split_key_value(tok), Some((k, _)) if k.eq_ignore_ascii_case("due"));
            if is_due {
                if let Some(v) = value {
                    if !replaced {
                        out.push(format!("due:{v}"));
                        replaced = true;
                    }
                }
                // Extra or removed due fields are dropped.
            } else {
                out.push(tok.to_string());
            }
        }
        if let Some(v) = value {
            if !replaced {
                out.push(format!("due:{v}"));
            }
        }
        let next = out.join(" ");
        if next == self.description {
            return false;
        }
        self.description = next;
        true
    }

    /// Sort key for priority: `A` is 0, missing priority sorts last.
    pub fn priority_rank(&self) -> u32 {
        match self.priority {
            Some(c) => (c as u32) - ('A' as u32),
            None => 100,
        }
    }

    /// Sort key for due dates: missing dates sort last.
    pub fn due_rank(&self) -> i64 {
        match self.due_date() {
            Some((y, m, d)) => date::days_from_civil(y, m, d),
            None => i64::MAX,
        }
    }

    /// Case-insensitive substring match over the rendered line, so a search hits
    /// priority, tags and dates as well as prose.
    pub fn matches_query(&self, needle_lower: &str) -> bool {
        if needle_lower.is_empty() {
            return true;
        }
        self.render().to_lowercase().contains(needle_lower)
    }

    pub fn has_project(&self, name: &str) -> bool {
        self.projects().iter().any(|p| p.eq_ignore_ascii_case(name))
    }

    pub fn has_context(&self, name: &str) -> bool {
        self.contexts().iter().any(|c| c.eq_ignore_ascii_case(name))
    }
}

/// `x` followed by whitespace marks a completed task.
fn strip_marker(s: &str) -> Option<&str> {
    let rest = s.strip_prefix('x').or_else(|| s.strip_prefix('X'))?;
    if rest.starts_with(char::is_whitespace) {
        Some(rest.trim_start())
    } else {
        None
    }
}

/// `(A) ` — a single uppercase letter in parentheses.
fn strip_priority(s: &str) -> Option<(char, &str)> {
    let b = s.as_bytes();
    if b.len() < 3 || b[0] != b'(' || b[2] != b')' {
        return None;
    }
    let c = b[1] as char;
    if !c.is_ascii_uppercase() {
        return None;
    }
    let rest = &s[3..];
    if rest.is_empty() {
        Some((c, ""))
    } else if rest.starts_with(char::is_whitespace) {
        Some((c, rest.trim_start()))
    } else {
        None
    }
}

/// A leading `YYYY-MM-DD` token.
fn strip_date(s: &str) -> Option<(Ymd, &str)> {
    if s.len() < 10 {
        return None;
    }
    let (head, rest) = s.split_at(10);
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let ymd = date::parse_ymd(head)?;
    Some((ymd, rest.trim_start()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(line: &str) -> Task {
        Task::parse(line, 0)
    }

    #[test]
    fn parses_the_prompt_examples() {
        let t = p("(B) Buy groceries +errands @home due:2026-03-15");
        assert!(!t.completed);
        assert_eq!(t.priority(), Some('B'));
        assert_eq!(t.summary(), "Buy groceries");
        assert_eq!(t.projects(), vec!["errands"]);
        assert_eq!(t.contexts(), vec!["home"]);
        assert_eq!(t.due_date(), Some((2026, 3, 15)));

        let t = p("x (A) Pay rent +finance @computer due:2026-01-01");
        assert!(t.completed);
        assert_eq!(t.priority(), Some('A'));
        assert_eq!(t.projects(), vec!["finance"]);
        assert_eq!(t.contexts(), vec!["computer"]);
        assert_eq!(t.due_date(), Some((2026, 1, 1)));
    }

    #[test]
    fn round_trips_untouched_lines() {
        for line in [
            "(B) Buy groceries +errands @home due:2026-03-15",
            "x (A) Pay rent +finance @computer due:2026-01-01",
            "x 2026-01-02 2025-12-30 Pay rent +finance",
            "2026-01-05 (C) Call plumber @phone",
            "plain task with no metadata",
            "x done thing",
            "(Z) lowest +p @c",
            "task with due:2029-12-12 and key:value pairs",
            "x",
            "(a) not a priority",
            "(AB) not a priority either",
        ] {
            assert_eq!(p(line).render(), line, "round-trip failed for {line:?}");
        }
    }

    #[test]
    fn lone_x_is_a_description_not_a_marker() {
        let t = p("x");
        assert!(!t.completed);
        assert_eq!(t.description(), "x");
    }

    #[test]
    fn parses_dates_in_either_order_relative_to_priority() {
        let t = p("x 2026-02-02 2026-01-01 (A) Ship it");
        assert!(t.completed);
        assert_eq!(t.priority(), Some('A'));
        assert_eq!(t.completion_date(), Some((2026, 2, 2)));
        assert_eq!(t.creation_date(), Some((2026, 1, 1)));
        assert_eq!(t.summary(), "Ship it");
        assert_eq!(t.render(), "x 2026-02-02 2026-01-01 (A) Ship it");
    }

    #[test]
    fn incomplete_single_date_is_a_creation_date() {
        let t = p("(A) 2026-01-01 Write report");
        assert_eq!(t.creation_date(), Some((2026, 1, 1)));
        assert_eq!(t.completion_date(), None);
    }

    #[test]
    fn invalid_dates_stay_in_the_description() {
        let t = p("(A) 2026-02-30 not a date");
        assert_eq!(t.creation_date(), None);
        assert!(t.description().starts_with("2026-02-30"));
    }

    #[test]
    fn upgrades_and_downgrades_priority() {
        let mut t = p("(B) Buy groceries +errands @home");
        assert!(t.raise_priority());
        assert_eq!(t.render(), "(A) Buy groceries +errands @home");
        assert!(!t.raise_priority(), "A is the ceiling");

        assert!(t.lower_priority());
        assert_eq!(t.priority(), Some('B'));

        let mut t = p("no priority here");
        assert!(!t.lower_priority());
        assert!(t.raise_priority());
        assert_eq!(t.render(), "(A) no priority here");

        let mut t = p("(Z) bottom");
        assert!(t.lower_priority());
        assert_eq!(t.render(), "bottom");
    }

    #[test]
    fn set_priority_rejects_non_letters_and_clears() {
        let mut t = p("(C) thing");
        assert!(!t.set_priority(Some('1')));
        assert!(t.set_priority(Some('d')), "lowercase input is upcased");
        assert_eq!(t.priority(), Some('D'));
        assert!(t.set_priority(None));
        assert_eq!(t.render(), "thing");
    }

    #[test]
    fn priority_is_inserted_after_the_completion_marker() {
        let mut t = p("x 2026-02-02 2026-01-01 Ship it");
        assert!(t.set_priority(Some('A')));
        assert_eq!(t.render(), "x (A) 2026-02-02 2026-01-01 Ship it");
    }

    #[test]
    fn completing_stamps_dates_and_reopening_removes_them() {
        let mut t = p("(A) Pay rent +finance");
        assert!(t.complete((2026, 5, 6)));
        assert_eq!(t.render(), "x (A) 2026-05-06 2026-05-06 Pay rent +finance");
        assert!(!t.complete((2026, 5, 7)), "already complete");

        assert!(t.uncomplete());
        assert_eq!(t.render(), "(A) 2026-05-06 Pay rent +finance");
    }

    #[test]
    fn completion_date_precedes_an_existing_creation_date() {
        // The source line puts the creation date before the priority, which is
        // unusual but legal; that layout is preserved, and the new completion
        // date is inserted immediately before the creation date.
        let mut t = p("2025-12-30 (B) Wrap gifts");
        t.complete((2026, 1, 2));
        assert_eq!(t.render(), "x 2026-01-02 2025-12-30 (B) Wrap gifts");

        // With the canonical layout the priority stays in front.
        let mut t = p("(B) 2025-12-30 Wrap gifts");
        t.complete((2026, 1, 2));
        assert_eq!(t.render(), "x (B) 2026-01-02 2025-12-30 Wrap gifts");
    }

    #[test]
    fn toggles_completion() {
        let mut t = p("Buy milk");
        t.toggle_complete((2026, 3, 15));
        assert!(t.completed);
        t.toggle_complete((2026, 3, 15));
        assert!(!t.completed);
        assert_eq!(t.render(), "2026-03-15 Buy milk");
    }

    #[test]
    fn edits_contexts_in_place_and_appends_new_ones() {
        let mut t = p("(B) Buy groceries @home +errands @phone");
        assert!(t.set_contexts(&["phone".into(), "town".into()]));
        // `@town` takes the vacated `@home` slot; `@phone` keeps its own.
        assert_eq!(t.render(), "(B) Buy groceries @town +errands @phone");

        assert!(t.set_contexts(&[]));
        assert_eq!(t.render(), "(B) Buy groceries +errands");
        assert!(!t.set_contexts(&[]), "no-op returns false");
    }

    #[test]
    fn a_replaced_tag_keeps_its_position_in_the_line() {
        // Without slot reuse the new tag would land after `due:`, reshuffling a
        // line the user never asked to reorder.
        let mut t = p("(B) Buy groceries +errands @home due:2026-03-15");
        assert!(t.set_contexts(&["town".into()]));
        assert_eq!(t.render(), "(B) Buy groceries +errands @town due:2026-03-15");
    }

    #[test]
    fn more_new_tags_than_slots_append_the_remainder() {
        let mut t = p("Task @home end");
        assert!(t.set_contexts(&["a".into(), "b".into(), "c".into()]));
        assert_eq!(t.render(), "Task @a end @b @c");
    }

    #[test]
    fn edits_projects_without_touching_contexts_or_due() {
        let mut t = p("Buy groceries +errands @home due:2026-03-15");
        assert!(t.set_projects(&["shopping".into(), "home".into()]));
        // `+shopping` reuses the `+errands` slot; `+home` has none, so it appends.
        assert_eq!(
            t.render(),
            "Buy groceries +shopping @home due:2026-03-15 +home"
        );
        assert_eq!(t.contexts(), vec!["home"]);
        assert_eq!(t.due_date(), Some((2026, 3, 15)));
    }

    #[test]
    fn tag_input_is_normalised() {
        let mut t = p("Task");
        assert!(t.add_tag('@', "  @Deep Work "));
        assert_eq!(t.contexts(), vec!["Deep-Work"]);
        assert!(!t.add_tag('@', "Deep-Work"), "duplicate is a no-op");
        assert!(!t.add_tag('@', "   "), "empty is a no-op");
        assert_eq!(
            parse_tag_list("+a, b  @c,,", '+'),
            vec!["a".to_string(), "b".to_string(), "@c".to_string()]
        );
    }

    #[test]
    fn sets_and_clears_due_dates() {
        let mut t = p("Buy milk @home");
        assert!(t.set_due(Some((2029, 12, 12))));
        assert_eq!(t.render(), "Buy milk @home due:2029-12-12");
        assert!(t.set_due(Some((2030, 1, 1))));
        assert_eq!(t.render(), "Buy milk @home due:2030-01-01");
        assert!(t.set_due(None));
        assert_eq!(t.render(), "Buy milk @home");
    }

    #[test]
    fn duplicate_due_fields_collapse_on_edit() {
        let mut t = p("Task due:2026-01-01 middle due:2026-02-02");
        assert!(t.set_due(Some((2026, 3, 3))));
        assert_eq!(t.render(), "Task due:2026-03-03 middle");
    }

    #[test]
    fn classifies_tokens() {
        assert_eq!(classify_token("+work"), TokenKind::Project);
        assert_eq!(classify_token("@home"), TokenKind::Context);
        assert_eq!(classify_token("due:2026-01-01"), TokenKind::Due);
        assert_eq!(classify_token("pri:A"), TokenKind::KeyValue);
        assert_eq!(classify_token("+"), TokenKind::Word);
        assert_eq!(classify_token("@"), TokenKind::Word);
        assert_eq!(classify_token("plain"), TokenKind::Word);
        assert_eq!(classify_token("a:b:c"), TokenKind::Word);
        assert_eq!(classify_token(":v"), TokenKind::Word);
    }

    #[test]
    fn search_covers_metadata() {
        let t = p("(A) Buy groceries +errands @home due:2026-03-15");
        assert!(t.matches_query("groc"));
        assert!(t.matches_query("+errands"));
        assert!(t.matches_query("(a)"));
        assert!(t.matches_query("2026-03"));
        assert!(!t.matches_query("zzz"));
    }

    #[test]
    fn ranks_for_sorting() {
        assert_eq!(p("(A) x").priority_rank(), 0);
        assert_eq!(p("(B) x").priority_rank(), 1);
        assert!(p("no pri").priority_rank() > p("(Z) x").priority_rank());
        assert!(p("due:2026-01-01 a").due_rank() < p("due:2026-01-02 a").due_rank());
        assert_eq!(p("no due").due_rank(), i64::MAX);
    }

    #[test]
    fn tags_are_deduplicated_and_case_matching_is_loose() {
        let t = p("Task +work +work @home");
        assert_eq!(t.projects(), vec!["work"]);
        assert!(t.has_project("WORK"));
        assert!(t.has_context("Home"));
        assert!(!t.has_project("home"));
    }

    #[test]
    fn extra_whitespace_is_tolerated() {
        let t = p("   (A)   Buy   milk   +shop  ");
        assert_eq!(t.priority(), Some('A'));
        assert_eq!(t.projects(), vec!["shop"]);
        assert_eq!(t.summary(), "Buy milk");
    }
}
