//! Tab completion for command names (from `$PATH`) and file paths.
//!
//! Completion is computed against the current token at the cursor. A token is
//! in "command position" when everything between the previous separator and the
//! token start is whitespace; otherwise it is treated as a path.

use std::collections::BTreeSet;

use crate::editor::Editor;

/// State kept between consecutive Tab presses so we can cycle candidates.
pub struct CompState {
    pub token_start: usize,
    pub candidates: Vec<String>,
    pub next: usize,
}

pub struct CompResult {
    pub token_start: usize,
    pub candidates: Vec<String>,
    pub lcp: String,
}

fn is_delim(c: char) -> bool {
    c.is_whitespace() || matches!(c, '|' | '&' | ';' | '<' | '>' | '(' | ')' | '\'' | '"')
}

struct Token {
    start: usize,
    text: String,
    is_command: bool,
}

fn current_token(chars: &[char], cursor: usize) -> Option<Token> {
    let cursor = cursor.min(chars.len());
    let mut start = cursor;
    while start > 0 && !is_delim(chars[start - 1]) {
        start -= 1;
    }
    let text: String = chars[start..cursor].iter().collect();

    let mut j = start;
    while j > 0 && chars[j - 1].is_whitespace() {
        j -= 1;
    }
    let is_command = j == 0 || matches!(chars[j - 1], '|' | '&' | ';' | '(');

    Some(Token { start, text, is_command })
}

#[cfg(unix)]
fn is_executable(e: &std::fs::DirEntry) -> bool {
    use std::os::unix::fs::PermissionsExt;
    e.metadata()
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(_e: &std::fs::DirEntry) -> bool {
    true
}

const BUILTINS: &[&str] = &[
    "alias", "bg", "break", "builtin", "cd", "command", "continue", "declare", "echo", "eval",
    "exec", "exit", "export", "false", "fg", "getopts", "hash", "help", "history", "jobs", "kill",
    "let", "local", "printf", "pwd", "read", "readonly", "return", "set", "shift", "shopt",
    "source", "test", "times", "trap", "true", "type", "typeset", "ulimit", "umask", "unalias",
    "unset", "wait", "[",
];

fn command_candidates(prefix: &str) -> Vec<String> {
    let mut set = BTreeSet::new();
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            if let Ok(rd) = std::fs::read_dir(dir) {
                for e in rd.flatten() {
                    if !is_executable(&e) {
                        continue;
                    }
                    if let Ok(name) = e.file_name().into_string() {
                        set.insert(name);
                    }
                }
            }
        }
    }
    for b in BUILTINS {
        set.insert((*b).to_string());
    }
    set.into_iter().filter(|n| n.starts_with(prefix)).collect()
}

fn expand_tilde(t: &str) -> String {
    if let Some(rest) = t.strip_prefix('~') {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}{rest}");
        }
    }
    t.to_string()
}

fn split_path(token: &str) -> (&str, &str) {
    match token.rfind('/') {
        Some(i) => (&token[..=i], &token[i + 1..]),
        None => ("", token),
    }
}

fn path_candidates(token: &str) -> Vec<String> {
    let token = expand_tilde(token);
    let (dir, base) = split_path(&token);
    let dir_path = if dir.is_empty() {
        std::path::Path::new(".")
    } else {
        std::path::Path::new(dir)
    };
    let mut v = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir_path) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with(base) {
                continue;
            }
            let is_dir = e.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let mut cand = if dir.is_empty() {
                name
            } else {
                format!("{dir}{name}")
            };
            if is_dir {
                cand.push('/');
            }
            v.push(cand);
        }
    }
    v.sort();
    v
}

fn common_prefix(strings: &[String]) -> String {
    let Some(first) = strings.first() else {
        return String::new();
    };
    let mut len = first.chars().count();
    for s in &strings[1..] {
        let mut i = 0;
        let mut a = first.chars();
        let mut b = s.chars();
        while let (Some(x), Some(y)) = (a.next(), b.next()) {
            if x == y {
                i += 1;
            } else {
                break;
            }
        }
        len = len.min(i);
    }
    first.chars().take(len).collect()
}

/// Compute completion for the command line: commands in command position,
/// file paths everywhere else.
pub fn compute(chars: &[char], cursor: usize) -> Option<CompResult> {
    let tok = current_token(chars, cursor)?;
    let prefix = tok.text.clone();
    let candidates = if tok.is_command {
        command_candidates(&prefix)
    } else {
        path_candidates(&prefix)
    };
    let lcp = common_prefix(&candidates);
    Some(CompResult { token_start: tok.start, candidates, lcp })
}

/// Compute completion for a plain path field (used by the save prompt).
pub fn compute_path(chars: &[char], cursor: usize) -> Option<CompResult> {
    let tok = current_token(chars, cursor)?;
    let candidates = path_candidates(&tok.text);
    let lcp = common_prefix(&candidates);
    Some(CompResult { token_start: tok.start, candidates, lcp })
}

/// Apply a completion result to an editor, supporting candidate cycling across
/// consecutive Tab presses. Returns a human-readable status message.
pub fn apply(
    editor: &mut Editor,
    state: &mut Option<CompState>,
    res: Option<CompResult>,
) -> String {
    let Some(res) = res else {
        *state = None;
        return "no completion available".to_string();
    };
    if res.candidates.is_empty() {
        *state = None;
        return "no completion available".to_string();
    }
    let cursor = editor.cursor();

    if let Some(st) = state {
        if st.token_start == res.token_start && st.candidates == res.candidates {
            let i = st.next % st.candidates.len();
            let cand = st.candidates[i].clone();
            editor.replace_range(res.token_start, cursor, &cand);
            st.next += 1;
            return format!("completion {}/{}: {}", i + 1, st.candidates.len(), cand);
        }
    }

    if res.candidates.len() == 1 {
        let c = res.candidates[0].clone();
        editor.replace_range(res.token_start, cursor, &c);
        *state = None;
        return format!("completed: {c}");
    }

    *state = Some(CompState { token_start: res.token_start, candidates: res.candidates.clone(), next: 0 });
    if res.lcp.chars().count() > cursor - res.token_start {
        editor.replace_range(res.token_start, cursor, &res.lcp);
    }
    let preview = res.candidates.iter().take(6).cloned().collect::<Vec<_>>().join("  ");
    let more = if res.candidates.len() > 6 {
        format!("  … +{}", res.candidates.len() - 6)
    } else {
        String::new()
    };
    format!("{} matches: {}{}  (Tab cycles)", res.candidates.len(), preview, more)
}
