//! Tab completion for command names and filesystem paths.
//!
//! Both sources are real: command names come from scanning the executable directories in
//! `$PATH`, and path candidates come from reading the actual directory on disk. Nothing is
//! hard coded to a particular command or file.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::pipeline::{split_stages, stage_at, unquote};

/// What the word under the cursor is being completed as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompKind {
    Command,
    Path,
}

/// A completion request resolved against the current line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    /// Character range of the word being replaced.
    pub start: usize,
    pub end: usize,
    /// Candidate replacements, already sorted and de-duplicated.
    pub candidates: Vec<String>,
    pub kind: CompKind,
}

/// Compute completions for the word ending at `cursor`.
///
/// Returns `None` when there is nothing to complete.
pub fn complete(chars: &[char], cursor: usize) -> Option<Completion> {
    let cursor = cursor.min(chars.len());
    let (start, word) = word_before(chars, cursor);
    // The first word of a stage is a command name; anything later is a path.
    let stages = split_stages(chars);
    let idx = stage_at(&stages, cursor);
    let stage_start = stages[idx].trimmed_range(chars).start;
    let is_command_position = start <= stage_start && !word.contains('/');

    let kind = if is_command_position { CompKind::Command } else { CompKind::Path };
    let candidates = match kind {
        CompKind::Command => complete_command(&word),
        CompKind::Path => complete_path(&word),
    };
    if candidates.is_empty() {
        return None;
    }
    Some(Completion { start, end: cursor, candidates, kind })
}

/// Range and unquoted text of the word immediately before `cursor`.
fn word_before(chars: &[char], cursor: usize) -> (usize, String) {
    let mut start = cursor;
    while start > 0 {
        let c = chars[start - 1];
        // Word characters for completion purposes: stop at whitespace and shell separators.
        if c.is_whitespace() || matches!(c, '|' | ';' | '&' | '<' | '>' | '(' | ')') {
            break;
        }
        start -= 1;
    }
    let word = unquote(&chars[start..cursor]);
    (start, word)
}

/// Executable names in `$PATH` that start with `prefix`.
///
/// Shell builtins and keywords are included because they are legitimate pipeline stages that
/// have no file in `$PATH`.
fn complete_command(prefix: &str) -> Vec<String> {
    let mut out = BTreeSet::new();
    for dir in std::env::var_os("PATH").iter().flat_map(std::env::split_paths) {
        let Ok(entries) = fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !name.starts_with(prefix) {
                continue;
            }
            if is_executable(&entry.path()) {
                out.insert(name);
            }
        }
    }
    for builtin in BUILTINS {
        if builtin.starts_with(prefix) {
            out.insert((*builtin).to_string());
        }
    }
    out.into_iter().collect()
}

/// Shell builtins worth completing; they never appear as files in `$PATH`.
const BUILTINS: &[&str] = &[
    "cd", "echo", "export", "read", "set", "unset", "source", "test", "time", "type", "wait",
    "while", "for", "if", "printf", "pwd", "exit", "true", "false",
];

/// Filesystem entries matching `prefix`.
///
/// Directories get a trailing `/` so repeated Tab presses descend naturally.
fn complete_path(prefix: &str) -> Vec<String> {
    let expanded = expand_tilde(prefix);
    // Split the prefix into "directory to list" and "partial file name".
    let (dir, partial) = match expanded.rfind('/') {
        Some(slash) => (expanded[..=slash].to_string(), expanded[slash + 1..].to_string()),
        None => (String::new(), expanded.clone()),
    };
    let list_dir: PathBuf =
        if dir.is_empty() { PathBuf::from(".") } else { PathBuf::from(&dir) };

    let Ok(entries) = fs::read_dir(&list_dir) else { return Vec::new() };
    let mut out = BTreeSet::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(&partial) {
            continue;
        }
        // Hidden files only surface when explicitly asked for.
        if name.starts_with('.') && !partial.starts_with('.') {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
        let mut cand = format!("{dir}{name}");
        if is_dir {
            cand.push('/');
        }
        out.insert(cand);
    }
    out.into_iter().collect()
}

/// Expand a leading `~` using `$HOME`.
fn expand_tilde(s: &str) -> String {
    if let Some(rest) = s.strip_prefix('~')
        && (rest.is_empty() || rest.starts_with('/'))
        && let Some(home) = std::env::var_os("HOME")
    {
        return format!("{}{rest}", home.to_string_lossy());
    }
    s.to_string()
}

/// True when `path` is a regular file with an execute bit set.
#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|m| !m.is_dir() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

/// Longest common prefix of `items`, used to extend the word before showing a list.
pub fn common_prefix(items: &[String]) -> String {
    let Some(first) = items.first() else { return String::new() };
    let mut len = first.chars().count();
    for item in &items[1..] {
        len = first
            .chars()
            .zip(item.chars())
            .take(len)
            .take_while(|(a, b)| a == b)
            .count();
        if len == 0 {
            break;
        }
    }
    first.chars().take(len).collect()
}

/// Quote a completion candidate if it contains characters the shell would split on.
pub fn quote_if_needed(s: &str) -> String {
    let needs = s.chars().any(|c| {
        c.is_whitespace() || matches!(c, '\'' | '"' | '|' | '&' | ';' | '(' | ')' | '*' | '?' | '$')
    });
    if !needs {
        return s.to_string();
    }
    // Single quotes are literal in the shell; embedded ones need the '\'' dance.
    format!("'{}'", s.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cv(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    #[test]
    fn finds_word_before_cursor() {
        let c = cv("cat /bench/ser");
        let (start, word) = word_before(&c, c.len());
        assert_eq!(start, 4);
        assert_eq!(word, "/bench/ser");
        // A pipe terminates the word.
        let c = cv("cat f |gre");
        let (start, word) = word_before(&c, c.len());
        assert_eq!((start, word.as_str()), (7, "gre"));
    }

    #[test]
    fn completes_real_commands_from_path() {
        // `ls` exists in PATH on any POSIX system running these tests.
        let cands = complete_command("l");
        assert!(cands.iter().any(|c| c == "ls"), "expected ls in {cands:?}");
        assert!(complete_command("zzz-no-such-cmd-zzz").is_empty());
    }

    #[test]
    fn completes_builtins() {
        assert!(complete_command("ech").iter().any(|c| c == "echo"));
    }

    #[test]
    fn completes_real_paths_with_dir_slash() {
        let dir = std::env::temp_dir().join("toolf-comp-test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("subdir")).unwrap();
        fs::write(dir.join("alpha.log"), b"x").unwrap();
        fs::write(dir.join(".hidden"), b"x").unwrap();

        let prefix = format!("{}/", dir.display());
        let cands = complete_path(&prefix);
        assert!(cands.contains(&format!("{prefix}alpha.log")), "{cands:?}");
        assert!(cands.contains(&format!("{prefix}subdir/")), "{cands:?}");
        // Hidden entries stay hidden unless requested.
        assert!(!cands.iter().any(|c| c.ends_with(".hidden")));
        assert!(complete_path(&format!("{prefix}.hid")).iter().any(|c| c.ends_with(".hidden")));

        let partial = complete_path(&format!("{prefix}alp"));
        assert_eq!(partial, vec![format!("{prefix}alpha.log")]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn chooses_command_vs_path_position() {
        let c = cv("ca");
        assert_eq!(complete(&c, 2).unwrap().kind, CompKind::Command);
        // After a pipe, we are completing a command again.
        let c = cv("cat f | gre");
        assert_eq!(complete(&c, c.len()).unwrap().kind, CompKind::Command);
        // An argument is a path.
        let c = cv("cat /");
        assert_eq!(complete(&c, c.len()).unwrap().kind, CompKind::Path);
    }

    #[test]
    fn computes_common_prefix() {
        let items = vec!["grep".to_string(), "grepdiff".to_string()];
        assert_eq!(common_prefix(&items), "grep");
        assert_eq!(common_prefix(&["ab".to_string(), "cd".to_string()]), "");
        assert_eq!(common_prefix(&[]), "");
    }

    #[test]
    fn quotes_only_when_necessary() {
        assert_eq!(quote_if_needed("/bench/server.log"), "/bench/server.log");
        assert_eq!(quote_if_needed("my file"), "'my file'");
    }
}
