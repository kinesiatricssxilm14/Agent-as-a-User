//! Shell pipeline parsing.
//!
//! `toolf` needs to understand where the `|` separators of a pipeline are so it can
//! (a) highlight each stage, (b) tell which stage the cursor sits in, and (c) build the
//! prefix pipeline used by partial execution.
//!
//! The scanner is deliberately conservative: it only tracks the constructs that change
//! the meaning of a `|` character (quotes, escapes, `$(...)` / `(...)` nesting and the
//! `||` operator). Everything else is left to the real shell at execution time.

use std::ops::Range;

/// Quote state left over at the end of a scan; used for live validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unterminated {
    No,
    Single,
    Double,
    Paren,
}

/// One pipeline stage. `range` covers the stage text (without the surrounding `|`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stage {
    /// Character range of the stage inside the whole command line.
    pub range: Range<usize>,
}

impl Stage {
    /// Character range with surrounding whitespace removed.
    pub fn trimmed_range(&self, chars: &[char]) -> Range<usize> {
        let mut start = self.range.start;
        let mut end = self.range.end.min(chars.len());
        while start < end && chars[start].is_whitespace() {
            start += 1;
        }
        while end > start && chars[end - 1].is_whitespace() {
            end -= 1;
        }
        start..end
    }

    /// Stage text with surrounding whitespace removed.
    pub fn text(&self, chars: &[char]) -> String {
        chars[self.trimmed_range(chars)].iter().collect()
    }

    /// The command word of the stage (first token), with quotes stripped.
    ///
    /// Leading `VAR=value` assignments are skipped the way a shell would, so
    /// `LC_ALL=C sort -u` reports `sort`.
    pub fn command_word(&self, chars: &[char]) -> String {
        let r = self.trimmed_range(chars);
        let mut i = r.start;
        loop {
            let tok = next_token(chars, i, r.end);
            let text: String = unquote(&chars[tok.clone()]);
            if text.is_empty() {
                return String::new();
            }
            // Skip environment assignments: NAME=... where NAME is a valid identifier.
            if is_assignment(&text) && tok.end < r.end {
                i = tok.end;
                while i < r.end && chars[i].is_whitespace() {
                    i += 1;
                }
                if i >= r.end {
                    return String::new();
                }
                continue;
            }
            return text;
        }
    }
}

fn is_assignment(tok: &str) -> bool {
    match tok.find('=') {
        Some(0) | None => false,
        Some(eq) => {
            let name = &tok[..eq];
            name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
    }
}

/// Range of the token starting at or after `from`, bounded by `end`.
fn next_token(chars: &[char], from: usize, end: usize) -> Range<usize> {
    let mut i = from;
    while i < end && chars[i].is_whitespace() {
        i += 1;
    }
    let start = i;
    let mut single = false;
    let mut double = false;
    while i < end {
        let c = chars[i];
        if single {
            if c == '\'' {
                single = false;
            }
            i += 1;
            continue;
        }
        if double {
            if c == '\\' {
                i = (i + 2).min(end);
                continue;
            }
            if c == '"' {
                double = false;
            }
            i += 1;
            continue;
        }
        match c {
            '\\' => {
                i = (i + 2).min(end);
                continue;
            }
            '\'' => single = true,
            '"' => double = true,
            c if c.is_whitespace() => break,
            _ => {}
        }
        i += 1;
    }
    start..i
}

/// Remove one level of shell quoting from a token.
pub fn unquote(chars: &[char]) -> String {
    let mut out = String::new();
    let mut i = 0;
    let mut single = false;
    let mut double = false;
    while i < chars.len() {
        let c = chars[i];
        if single {
            if c == '\'' {
                single = false;
            } else {
                out.push(c);
            }
            i += 1;
            continue;
        }
        if double {
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == '"' {
                double = false;
            } else {
                out.push(c);
            }
            i += 1;
            continue;
        }
        match c {
            '\\' if i + 1 < chars.len() => {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            '\'' => single = true,
            '"' => double = true,
            _ => out.push(c),
        }
        i += 1;
    }
    out
}

/// Split a command line into pipeline stages.
///
/// Always returns at least one stage (possibly empty) so callers can index stage 0.
pub fn split_stages(chars: &[char]) -> Vec<Stage> {
    let mut stages = Vec::new();
    let n = chars.len();
    let mut start = 0usize;
    let mut i = 0usize;
    let mut single = false;
    let mut double = false;
    let mut depth = 0usize;

    while i < n {
        let c = chars[i];
        if single {
            if c == '\'' {
                single = false;
            }
            i += 1;
            continue;
        }
        if double {
            if c == '\\' {
                i = (i + 2).min(n);
                continue;
            }
            if c == '"' {
                double = false;
            }
            i += 1;
            continue;
        }
        match c {
            '\\' => {
                i = (i + 2).min(n);
                continue;
            }
            '\'' => single = true,
            '"' => double = true,
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            '|' if depth == 0 => {
                // `||` is a logical operator, not a pipeline separator.
                if i + 1 < n && chars[i + 1] == '|' {
                    i += 2;
                    continue;
                }
                // A `|` right after another separator would produce an empty stage; that is
                // still reported so the validator can flag it.
                stages.push(Stage { range: start..i });
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    stages.push(Stage { range: start..n });
    stages
}

/// Positions (character indices) of the top level `|` separators.
pub fn pipe_positions(chars: &[char]) -> Vec<usize> {
    let stages = split_stages(chars);
    stages.iter().take(stages.len().saturating_sub(1)).map(|s| s.range.end).collect()
}

/// Quote/paren state at the end of the line.
pub fn unterminated(chars: &[char]) -> Unterminated {
    let n = chars.len();
    let mut i = 0usize;
    let mut single = false;
    let mut double = false;
    let mut depth = 0usize;
    while i < n {
        let c = chars[i];
        if single {
            if c == '\'' {
                single = false;
            }
            i += 1;
            continue;
        }
        if double {
            if c == '\\' {
                i = (i + 2).min(n);
                continue;
            }
            if c == '"' {
                double = false;
            }
            i += 1;
            continue;
        }
        match c {
            '\\' => {
                i = (i + 2).min(n);
                continue;
            }
            '\'' => single = true,
            '"' => double = true,
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ => {}
        }
        i += 1;
    }
    if single {
        Unterminated::Single
    } else if double {
        Unterminated::Double
    } else if depth > 0 {
        Unterminated::Paren
    } else {
        Unterminated::No
    }
}

/// Index of the stage that contains `cursor`.
///
/// A cursor sitting exactly on a `|` belongs to the stage *before* the separator, which is
/// what "execute the segment before the cursor" needs.
pub fn stage_at(stages: &[Stage], cursor: usize) -> usize {
    for (i, s) in stages.iter().enumerate() {
        if cursor <= s.range.end {
            return i;
        }
    }
    stages.len().saturating_sub(1)
}

/// Build the pipeline prefix that ends with the stage under the cursor.
///
/// Returns `None` when that prefix contains no runnable text.
pub fn prefix_through_cursor(chars: &[char], cursor: usize) -> Option<(String, usize, usize)> {
    let stages = split_stages(chars);
    let idx = stage_at(&stages, cursor.min(chars.len()));
    let end = stages[idx].range.end.min(chars.len());
    let text: String = chars[..end].iter().collect();
    let trimmed = text.trim();
    // A trailing separator would make the shell wait for another stage.
    let trimmed = trimmed.trim_end_matches('|').trim_end();
    if trimmed.is_empty() {
        return None;
    }
    Some((trimmed.to_string(), idx + 1, stages.len()))
}

/// Character index of the boundary before `cursor` (start of the current stage, then the
/// previous separator, and so on).
pub fn prev_boundary(chars: &[char], cursor: usize) -> usize {
    let mut targets = vec![0usize];
    for p in pipe_positions(chars) {
        targets.push(p);
        targets.push((p + 1).min(chars.len()));
    }
    targets.push(chars.len());
    targets.sort_unstable();
    targets.dedup();
    targets.iter().rev().copied().find(|&t| t < cursor).unwrap_or(0)
}

/// Character index of the boundary after `cursor`.
pub fn next_boundary(chars: &[char], cursor: usize) -> usize {
    let mut targets = vec![0usize];
    for p in pipe_positions(chars) {
        targets.push(p);
        targets.push((p + 1).min(chars.len()));
    }
    targets.push(chars.len());
    targets.sort_unstable();
    targets.dedup();
    targets.iter().copied().find(|&t| t > cursor).unwrap_or(chars.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cv(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn splits_simple_pipeline() {
        let c = cv("cat a.log | grep ERROR | wc -l");
        let st = split_stages(&c);
        assert_eq!(st.len(), 3);
        assert_eq!(st[0].text(&c), "cat a.log");
        assert_eq!(st[1].text(&c), "grep ERROR");
        assert_eq!(st[2].text(&c), "wc -l");
    }

    #[test]
    fn ignores_pipe_inside_quotes() {
        let c = cv(r#"grep 'a|b' f | wc -l"#);
        let st = split_stages(&c);
        assert_eq!(st.len(), 2);
        assert_eq!(st[0].text(&c), "grep 'a|b' f");
        let c2 = cv(r#"awk -F"|" '{print $1}' f"#);
        assert_eq!(split_stages(&c2).len(), 1);
    }

    #[test]
    fn ignores_escaped_pipe_and_logical_or() {
        assert_eq!(split_stages(&cv(r"grep a\|b f")).len(), 1);
        assert_eq!(split_stages(&cv("false || true")).len(), 1);
    }

    #[test]
    fn ignores_pipe_inside_subshell() {
        let c = cv("echo $(cat f | wc -l) | tr -d ' '");
        let st = split_stages(&c);
        assert_eq!(st.len(), 2);
        assert_eq!(st[0].text(&c), "echo $(cat f | wc -l)");
    }

    #[test]
    fn command_word_skips_assignments_and_quotes() {
        let c = cv("LC_ALL=C sort -u");
        assert_eq!(split_stages(&c)[0].command_word(&c), "sort");
        let c = cv(r#""grep" -i x"#);
        assert_eq!(split_stages(&c)[0].command_word(&c), "grep");
        let c = cv("  tail -n 5 f  ");
        assert_eq!(split_stages(&c)[0].command_word(&c), "tail");
    }

    #[test]
    fn stage_at_maps_cursor_to_stage() {
        let c = cv("cat f | grep E | wc -l");
        let st = split_stages(&c);
        assert_eq!(stage_at(&st, 0), 0);
        assert_eq!(stage_at(&st, 6), 0); // on the first '|'
        assert_eq!(stage_at(&st, 7), 1);
        assert_eq!(stage_at(&st, c.len()), 2);
    }

    #[test]
    fn prefix_through_cursor_builds_partial_pipeline() {
        let c = cv("cat f | grep E | wc -l");
        let (cmd, n, total) = prefix_through_cursor(&c, 9).unwrap();
        assert_eq!(cmd, "cat f | grep E");
        assert_eq!((n, total), (2, 3));
        // Cursor on the separator keeps the stage before it.
        let (cmd, n, _) = prefix_through_cursor(&c, 6).unwrap();
        assert_eq!(cmd, "cat f");
        assert_eq!(n, 1);
        // Trailing separator is dropped rather than left dangling.
        let c2 = cv("cat f | ");
        let (cmd, _, _) = prefix_through_cursor(&c2, c2.len()).unwrap();
        assert_eq!(cmd, "cat f");
        assert!(prefix_through_cursor(&cv("   "), 3).is_none());
    }

    #[test]
    fn detects_unterminated_quotes() {
        assert_eq!(unterminated(&cv("grep 'x")), Unterminated::Single);
        assert_eq!(unterminated(&cv(r#"grep "x"#)), Unterminated::Double);
        assert_eq!(unterminated(&cv("echo $(x")), Unterminated::Paren);
        assert_eq!(unterminated(&cv("grep 'x' f")), Unterminated::No);
    }

    #[test]
    fn boundaries_walk_between_stages() {
        let c = cv("cat f | grep E | wc -l");
        assert_eq!(next_boundary(&c, 0), 6);
        assert_eq!(next_boundary(&c, 6), 7);
        assert_eq!(prev_boundary(&c, c.len()), 16);
        assert_eq!(prev_boundary(&c, 0), 0);
    }
}
