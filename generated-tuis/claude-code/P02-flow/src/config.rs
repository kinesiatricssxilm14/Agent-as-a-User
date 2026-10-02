//! Configuration file handling.
//!
//! The config is a small line-oriented `key = value` file. Parsing is deliberately tolerant:
//! a benchmark task may hand us a file written by hand or by another tool, so we accept quoted
//! or bare values, `#`/`;` comments, and skip `[section]` headers instead of failing on them.
//!
//! Rewriting preserves every line verbatim except the one key being changed, so comments and
//! unrelated settings survive a round-trip.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::store::{atomic_write, LineEnding};

/// The key holding the board root directory.
pub const KEY_BOARD_ROOT: &str = "board_root";

/// Compiled-in default board root, per the specification.
pub const DEFAULT_BOARD_ROOT: &str = "/bench/data/board";

/// Where the effective board root came from. Shown in the UI so the user can tell why a
/// particular directory is open.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootSource {
    CommandLine,
    Environment,
    ConfigFile,
    Default,
}

impl RootSource {
    pub fn label(self) -> &'static str {
        match self {
            RootSource::CommandLine => "command line",
            RootSource::Environment => "$TOOLB_BOARD",
            RootSource::ConfigFile => "config file",
            RootSource::Default => "built-in default",
        }
    }
}

/// A parsed config file, plus the raw lines needed to rewrite it without losing anything.
#[derive(Debug, Clone)]
pub struct Config {
    /// Path we read from and will write to. Present even when the file does not exist yet.
    pub path: PathBuf,
    /// Whether that file existed at load time.
    pub exists: bool,
    /// Parsed key/value pairs, sorted for stable display.
    pub values: BTreeMap<String, String>,
    /// Every physical line of the file, verbatim (no terminators).
    raw_lines: Vec<String>,
    /// Line ending to reuse when rewriting.
    line_ending: LineEnding,
}

impl Config {
    /// An empty config anchored at `path`, used when the file does not exist.
    fn empty(path: PathBuf) -> Self {
        Self {
            path,
            exists: false,
            values: BTreeMap::new(),
            raw_lines: Vec::new(),
            line_ending: LineEnding::Lf,
        }
    }

    /// Read and parse the config at `path`. A missing file is not an error — it yields an empty
    /// config so a first `set` can create it. Unreadable files are reported.
    pub fn load(path: PathBuf) -> Result<Self, String> {
        let text = match std::fs::read(&path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::empty(path)),
            Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
        };

        let line_ending = LineEnding::detect(&text);
        let raw_lines: Vec<String> = split_lines(&text);
        let mut values = BTreeMap::new();
        for line in &raw_lines {
            if let Some((k, v)) = parse_entry(line) {
                values.insert(k, v);
            }
        }

        Ok(Self { path, exists: true, values, raw_lines, line_ending })
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    /// The board root recorded in the config, if any non-empty value is present.
    pub fn board_root(&self) -> Option<PathBuf> {
        self.get(KEY_BOARD_ROOT)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
    }

    /// Set `key` to `value` and persist immediately.
    ///
    /// An existing assignment is edited in place; a new key is appended. Every other line —
    /// comments, section headers, blank lines — is re-emitted byte for byte.
    pub fn set_and_save(&mut self, key: &str, value: &str) -> Result<(), String> {
        let key = key.trim();
        if key.is_empty() {
            return Err("configuration key must not be empty".to_string());
        }
        if key.contains(['\n', '\r', '=', '[', ']']) {
            return Err("configuration key contains an invalid character".to_string());
        }
        if value.contains(['\n', '\r']) {
            return Err("configuration value must be a single line".to_string());
        }

        let rendered = render_entry(key, value);
        let mut replaced = false;
        for line in &mut self.raw_lines {
            if parse_entry(line).map(|(k, _)| k == key).unwrap_or(false) {
                if replaced {
                    // A duplicate assignment further down would win on reload; blank it so the
                    // file agrees with the value we just committed.
                    line.clear();
                } else {
                    *line = rendered.clone();
                    replaced = true;
                }
            }
        }
        if !replaced {
            self.raw_lines.push(rendered);
        }

        if let Some(parent) = self.path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
            }
        }
        let body = self.line_ending.join(&self.raw_lines);
        atomic_write(&self.path, body.as_bytes())?;

        self.values.insert(key.to_string(), value.to_string());
        self.exists = true;
        Ok(())
    }
}

/// Split text into lines, dropping a single trailing empty element so a file ending in a
/// newline does not gain a blank line on every rewrite.
fn split_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text.split('\n').map(|l| l.trim_end_matches('\r').to_string()).collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines
}

/// Parse one line into a key/value pair, or `None` for blanks, comments and section headers.
fn parse_entry(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with(';') {
        return None;
    }
    // `[section]` headers are accepted and ignored: keys stay in one flat namespace.
    if trimmed.starts_with('[') {
        return None;
    }
    let (key, value) = trimmed.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some((key.to_string(), unquote(value.trim())))
}

/// Strip matching quotes, honouring `\"` and `\\`. Unquoted values keep `#`/`;` verbatim,
/// because a bare value may legitimately be a path containing those characters.
fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    let quote = match bytes.first() {
        Some(b'"') => '"',
        Some(b'\'') => '\'',
        _ => return value.to_string(),
    };
    let mut out = String::new();
    let mut chars = value.chars().skip(1);
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some(next) if next == quote || next == '\\' => out.push(next),
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                }
                None => out.push('\\'),
            }
        } else if ch == quote {
            // Closing quote: anything after it is a trailing comment.
            return out;
        } else {
            out.push(ch);
        }
    }
    out
}

/// Render an assignment, quoting only when the value needs it.
fn render_entry(key: &str, value: &str) -> String {
    let needs_quotes = value.trim() != value
        || value.is_empty()
        || value.starts_with(['"', '\''])
        || value.contains('#')
        || value.contains(';');
    if needs_quotes {
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        format!("{key} = \"{escaped}\"")
    } else {
        format!("{key} = {value}")
    }
}

/// Read an environment variable, treating empty as unset.
fn env_var(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// Locate the config file.
///
/// Order: `--config` > `$TOOLB_CONFIG` > `$XDG_CONFIG_HOME/toolb/config.conf` >
/// `$HOME/.config/toolb/config.conf` > `/root/.config/toolb/config.conf`.
///
/// The last fallback matters: `HOME` is frequently unset for root under `docker exec`, and this
/// must never panic or silently lose the user's settings.
pub fn resolve_config_path(explicit: Option<&Path>) -> PathBuf {
    if let Some(p) = explicit {
        return p.to_path_buf();
    }
    if let Some(p) = env_var("TOOLB_CONFIG") {
        return PathBuf::from(p);
    }
    if let Some(dir) = env_var("XDG_CONFIG_HOME") {
        return PathBuf::from(dir).join("toolb").join("config.conf");
    }
    if let Some(home) = env_var("HOME") {
        return PathBuf::from(home).join(".config").join("toolb").join("config.conf");
    }
    PathBuf::from("/root/.config/toolb/config.conf")
}

/// Decide which board root to use, and remember why.
///
/// Precedence: command line > `$TOOLB_BOARD` > config file > built-in default.
pub fn resolve_board_root(cli_root: Option<&Path>, config: &Config) -> (PathBuf, RootSource) {
    if let Some(p) = cli_root {
        return (p.to_path_buf(), RootSource::CommandLine);
    }
    if let Some(p) = env_var("TOOLB_BOARD") {
        return (PathBuf::from(p), RootSource::Environment);
    }
    if let Some(p) = config.board_root() {
        return (p, RootSource::ConfigFile);
    }
    (PathBuf::from(DEFAULT_BOARD_ROOT), RootSource::Default)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_text(text: &str) -> Config {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.conf");
        std::fs::write(&path, text).unwrap();
        Config::load(path).unwrap()
    }

    #[test]
    fn missing_file_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = Config::load(dir.path().join("nope.conf")).unwrap();
        assert!(!cfg.exists);
        assert!(cfg.values.is_empty());
        assert_eq!(cfg.board_root(), None);
    }

    #[test]
    fn parses_bare_quoted_and_commented_entries() {
        let cfg = parse_text(concat!(
            "# toolb settings\n",
            "board_root = /srv/board\n",
            "quoted = \"/tmp/with space\"\n",
            "single = '/tmp/single'\n",
            "; semicolon comment\n",
            "[section]\n",
            "in_section = 7\n",
            "spaced   =    value   \n",
        ));
        assert_eq!(cfg.get("board_root"), Some("/srv/board"));
        assert_eq!(cfg.get("quoted"), Some("/tmp/with space"));
        assert_eq!(cfg.get("single"), Some("/tmp/single"));
        assert_eq!(cfg.get("in_section"), Some("7"), "section headers are skipped, keys kept");
        assert_eq!(cfg.get("spaced"), Some("value"));
        assert_eq!(cfg.board_root(), Some(PathBuf::from("/srv/board")));
    }

    #[test]
    fn quoted_values_support_escapes_and_trailing_comments() {
        let cfg = parse_text("a = \"say \\\"hi\\\"\"\nb = \"/x\" # trailing\n");
        assert_eq!(cfg.get("a"), Some(r#"say "hi""#));
        assert_eq!(cfg.get("b"), Some("/x"));
    }

    #[test]
    fn bare_values_keep_hash_characters() {
        // A bare path containing '#' is a real path, not a comment.
        let cfg = parse_text("board_root = /srv/c#1\n");
        assert_eq!(cfg.get("board_root"), Some("/srv/c#1"));
    }

    #[test]
    fn empty_board_root_falls_through_to_default() {
        let cfg = parse_text("board_root =\n");
        assert_eq!(cfg.board_root(), None);
        let (root, src) = resolve_board_root(None, &cfg);
        // Environment may be set in the ambient test process; only assert the config lost.
        assert_ne!(src, RootSource::ConfigFile);
        if src == RootSource::Default {
            assert_eq!(root, PathBuf::from(DEFAULT_BOARD_ROOT));
        }
    }

    #[test]
    fn set_preserves_comments_and_unrelated_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.conf");
        std::fs::write(&path, "# keep me\nother = 1\nboard_root = /old\n# tail\n").unwrap();
        let mut cfg = Config::load(path.clone()).unwrap();
        cfg.set_and_save(KEY_BOARD_ROOT, "/new/board").unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text, "# keep me\nother = 1\nboard_root = /new/board\n# tail\n");

        let reread = Config::load(path).unwrap();
        assert_eq!(reread.get("board_root"), Some("/new/board"));
        assert_eq!(reread.get("other"), Some("1"));
    }

    #[test]
    fn set_appends_new_key_and_creates_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        // Nested directory does not exist yet.
        let path = dir.path().join("sub").join("config.conf");
        let mut cfg = Config::load(path.clone()).unwrap();
        cfg.set_and_save(KEY_BOARD_ROOT, "/first").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "board_root = /first\n");
        assert!(cfg.exists);
    }

    #[test]
    fn set_quotes_values_that_need_it_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.conf");
        let mut cfg = Config::load(path.clone()).unwrap();
        cfg.set_and_save("board_root", "/tmp/a b").unwrap();
        cfg.set_and_save("hashy", "/tmp/c#1").unwrap();

        let reread = Config::load(path).unwrap();
        assert_eq!(reread.get("board_root"), Some("/tmp/a b"));
        assert_eq!(reread.get("hashy"), Some("/tmp/c#1"));
    }

    #[test]
    fn set_collapses_duplicate_assignments() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.conf");
        std::fs::write(&path, "board_root = /a\nboard_root = /b\n").unwrap();
        let mut cfg = Config::load(path.clone()).unwrap();
        cfg.set_and_save(KEY_BOARD_ROOT, "/c").unwrap();
        // The later duplicate would otherwise win on reload.
        assert_eq!(Config::load(path).unwrap().get("board_root"), Some("/c"));
    }

    #[test]
    fn set_preserves_crlf_line_endings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.conf");
        std::fs::write(&path, "# c\r\nboard_root = /old\r\n").unwrap();
        let mut cfg = Config::load(path.clone()).unwrap();
        cfg.set_and_save(KEY_BOARD_ROOT, "/new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "# c\r\nboard_root = /new\r\n");
    }

    #[test]
    fn set_rejects_malformed_keys_and_values() {
        let dir = tempfile::tempdir().unwrap();
        let mut cfg = Config::load(dir.path().join("c.conf")).unwrap();
        assert!(cfg.set_and_save("", "x").is_err());
        assert!(cfg.set_and_save("a=b", "x").is_err());
        assert!(cfg.set_and_save("ok", "line1\nline2").is_err());
    }

    #[test]
    fn cli_root_outranks_everything() {
        let cfg = parse_text("board_root = /from/config\n");
        let (root, src) = resolve_board_root(Some(Path::new("/from/cli")), &cfg);
        assert_eq!(root, PathBuf::from("/from/cli"));
        assert_eq!(src, RootSource::CommandLine);
    }

    #[test]
    fn explicit_config_path_wins() {
        let p = PathBuf::from("/custom/toolb.conf");
        assert_eq!(resolve_config_path(Some(&p)), p);
    }
}
