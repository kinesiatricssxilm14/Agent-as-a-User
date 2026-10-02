//! Regex compilation, matching and replacement.
//!
//! Two backends are available:
//!
//! * [`regex`] — the default linear-time engine.
//! * [`fancy_regex`] — a backtracking engine used automatically when the
//!   pattern needs look-around or back-references, which `regex` rejects.
//!
//! Both are wrapped by [`Engine`] so the rest of the program only deals with
//! [`RawMatch`] values carrying byte offsets, which [`crate::doc::Document`]
//! then converts to the character offsets shown in the UI.

use std::fmt;

/// Hard ceiling on the number of matches collected for one pattern.  Protects
/// the UI from patterns like `(?s).*?` on a huge file.
pub const MATCH_LIMIT: usize = 100_000;

/// Inline-flag toggles that the UI can switch without editing the pattern.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    /// `(?i)` — case-insensitive matching.
    pub ignore_case: bool,
    /// `(?m)` — `^`/`$` match at line boundaries.
    pub multi_line: bool,
    /// `(?s)` — `.` matches newline.
    pub dot_all: bool,
    /// `(?x)` — whitespace in the pattern is ignored.
    pub extended: bool,
    /// The pattern is a literal string, not a regex.
    pub literal: bool,
}

impl Flags {
    /// The inline-flag group that this flag set corresponds to, e.g. `(?im)`.
    /// Empty when no flag is set.
    pub fn inline_prefix(&self) -> String {
        let mut s = String::new();
        if self.ignore_case {
            s.push('i');
        }
        if self.multi_line {
            s.push('m');
        }
        if self.dot_all {
            s.push('s');
        }
        if self.extended {
            s.push('x');
        }
        if s.is_empty() {
            String::new()
        } else {
            format!("(?{s})")
        }
    }

    /// Compact indicator used in the status bar, e.g. `i-s-` .
    pub fn chips(&self) -> [(char, bool); 5] {
        [
            ('i', self.ignore_case),
            ('m', self.multi_line),
            ('s', self.dot_all),
            ('x', self.extended),
            ('F', self.literal),
        ]
    }
}

/// Which backend compiled the current pattern.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// The `regex` crate: linear time, no look-around or back-references.
    Fast,
    /// The `fancy-regex` crate: backtracking, supports look-around and
    /// back-references.
    Fancy,
}

impl Backend {
    /// Short human-readable name.
    pub fn label(self) -> &'static str {
        match self {
            Backend::Fast => "regex",
            Backend::Fancy => "fancy",
        }
    }
}

/// A compiled pattern.
pub enum Engine {
    /// Linear-time backend.
    Fast(Box<regex::Regex>),
    /// Backtracking backend.
    Fancy(Box<fancy_regex::Regex>),
}

impl fmt::Debug for Engine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Engine::Fast(r) => write!(f, "Engine::Fast({})", r.as_str()),
            Engine::Fancy(r) => write!(f, "Engine::Fancy({})", r.as_str()),
        }
    }
}

/// One capture group of a match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawGroup {
    /// Group number (0 is the whole match).
    pub index: usize,
    /// Group name, when the group is named.
    pub name: Option<String>,
    /// Byte range within the subject text, `None` when the group did not
    /// participate in the match.
    pub range: Option<(usize, usize)>,
    /// Captured text, `None` when the group did not participate.
    pub text: Option<String>,
}

/// A single match, in byte offsets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawMatch {
    /// Byte offset of the first byte of the match.
    pub start: usize,
    /// Byte offset one past the last byte of the match.
    pub end: usize,
    /// The matched text.
    pub text: String,
    /// Capture groups, index 0 being the whole match.  Empty when the pattern
    /// has no capture groups.
    pub groups: Vec<RawGroup>,
}

impl RawMatch {
    /// True for a zero-width match such as `\b` or `x*` against `y`.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// Why compilation or matching failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineError {
    /// Message shown to the user.
    pub message: String,
}

impl EngineError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for EngineError {}

/// Result of a successful compilation.
#[derive(Debug)]
pub struct Compiled {
    /// The compiled pattern.
    pub engine: Engine,
    /// Which backend was used.
    pub backend: Backend,
    /// The exact pattern string handed to the backend, including any inline
    /// flag prefix synthesised from [`Flags`].
    pub effective_pattern: String,
    /// Set when the `regex` backend rejected the pattern and `fancy-regex`
    /// accepted it; explains the automatic switch in the UI.
    pub fallback_reason: Option<String>,
}

/// Compile `pattern` under `flags`.
///
/// When `prefer_fancy` is false the linear-time engine is tried first and the
/// backtracking engine is used only as a fallback, so that the common case
/// keeps its performance guarantees.
pub fn compile(pattern: &str, flags: Flags, prefer_fancy: bool) -> Result<Compiled, EngineError> {
    if pattern.is_empty() {
        return Err(EngineError::new("pattern is empty"));
    }

    let body = if flags.literal {
        regex::escape(pattern)
    } else {
        pattern.to_string()
    };
    // A literal pattern must not be re-interpreted in extended mode, where
    // escaped whitespace would still be dropped by the parser.
    let mut eff_flags = flags;
    if flags.literal {
        eff_flags.extended = false;
    }
    let effective_pattern = format!("{}{}", eff_flags.inline_prefix(), body);

    if !prefer_fancy {
        match regex::Regex::new(&effective_pattern) {
            Ok(r) => {
                return Ok(Compiled {
                    engine: Engine::Fast(Box::new(r)),
                    backend: Backend::Fast,
                    effective_pattern,
                    fallback_reason: None,
                })
            }
            Err(fast_err) => {
                // The linear engine deliberately rejects look-around and
                // back-references.  Retry with the backtracking engine before
                // reporting a syntax error.
                match fancy_regex::Regex::new(&effective_pattern) {
                    Ok(r) => {
                        return Ok(Compiled {
                            engine: Engine::Fancy(Box::new(r)),
                            backend: Backend::Fancy,
                            effective_pattern,
                            fallback_reason: Some(first_line(&fast_err.to_string())),
                        })
                    }
                    Err(_) => return Err(EngineError::new(clean_error(&fast_err.to_string()))),
                }
            }
        }
    }

    match fancy_regex::Regex::new(&effective_pattern) {
        Ok(r) => Ok(Compiled {
            engine: Engine::Fancy(Box::new(r)),
            backend: Backend::Fancy,
            effective_pattern,
            fallback_reason: None,
        }),
        Err(e) => Err(EngineError::new(clean_error(&e.to_string()))),
    }
}

/// Outcome of scanning the subject text.
#[derive(Debug, Default)]
pub struct Scan {
    /// The matches found, in order of position.
    pub matches: Vec<RawMatch>,
    /// True when [`MATCH_LIMIT`] cut the scan short.
    pub truncated: bool,
}

impl Engine {
    /// Names of every capture group; index 0 is always `None`.
    pub fn group_names(&self) -> Vec<Option<String>> {
        match self {
            Engine::Fast(r) => r.capture_names().map(|n| n.map(str::to_string)).collect(),
            Engine::Fancy(r) => r.capture_names().map(|n| n.map(str::to_string)).collect(),
        }
    }

    /// Find every match in `text`.
    pub fn scan(&self, text: &str) -> Result<Scan, EngineError> {
        let names = self.group_names();
        let mut out = Scan::default();
        match self {
            Engine::Fast(re) => {
                for caps in re.captures_iter(text) {
                    if out.matches.len() >= MATCH_LIMIT {
                        out.truncated = true;
                        break;
                    }
                    let whole = caps.get(0).expect("group 0 always participates");
                    out.matches.push(RawMatch {
                        start: whole.start(),
                        end: whole.end(),
                        text: whole.as_str().to_string(),
                        groups: collect_groups(&names, |i| {
                            caps.get(i).map(|m| (m.start(), m.end(), m.as_str()))
                        }),
                    });
                }
            }
            Engine::Fancy(re) => {
                for caps in re.captures_iter(text) {
                    if out.matches.len() >= MATCH_LIMIT {
                        out.truncated = true;
                        break;
                    }
                    let caps = caps.map_err(|e| EngineError::new(clean_error(&e.to_string())))?;
                    let whole = caps.get(0).expect("group 0 always participates");
                    out.matches.push(RawMatch {
                        start: whole.start(),
                        end: whole.end(),
                        text: whole.as_str().to_string(),
                        groups: collect_groups(&names, |i| {
                            caps.get(i).map(|m| (m.start(), m.end(), m.as_str()))
                        }),
                    });
                }
            }
        }
        Ok(out)
    }
}

/// Build the group table for one match.  Groups are omitted entirely for
/// patterns without capture groups, since group 0 duplicates the match text.
fn collect_groups<'t, F>(names: &[Option<String>], get: F) -> Vec<RawGroup>
where
    F: Fn(usize) -> Option<(usize, usize, &'t str)>,
{
    if names.len() <= 1 {
        return Vec::new();
    }
    (0..names.len())
        .map(|i| {
            let hit = get(i);
            RawGroup {
                index: i,
                name: names[i].clone(),
                range: hit.map(|(s, e, _)| (s, e)),
                text: hit.map(|(_, _, t)| t.to_string()),
            }
        })
        .collect()
}

/// A byte range in the *rewritten* text that came from the replacement
/// template, together with the index of the match that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReplSpan {
    /// Index into the match list.
    pub match_index: usize,
    /// Byte offset of the inserted text in the rewritten output.
    pub start: usize,
    /// Byte offset one past the inserted text.
    pub end: usize,
}

/// The complete rewritten document plus the provenance of each inserted span.
#[derive(Debug, Default)]
pub struct Replacement {
    /// Full replaced content, including every unmodified line.
    pub text: String,
    /// Where the template output landed in [`Replacement::text`].
    pub spans: Vec<ReplSpan>,
    /// How many matches were actually rewritten.
    pub applied: usize,
}

/// Apply `template` to every match (or only the first when `first_only`).
///
/// The whole subject text is copied through, so the result is the complete file
/// content with the matched fragments rewritten — that is what the preview pane
/// displays.
pub fn replace_all(
    text: &str,
    matches: &[RawMatch],
    template: &str,
    first_only: bool,
) -> Replacement {
    let tokens = parse_template(template);
    let mut out = Replacement {
        text: String::with_capacity(text.len() + 16),
        spans: Vec::new(),
        applied: 0,
    };
    let mut cursor = 0usize;
    for (idx, m) in matches.iter().enumerate() {
        if first_only && out.applied == 1 {
            break;
        }
        // Overlapping or out-of-order matches cannot come from our scanners,
        // but guard anyway so a bad input can never panic on slicing.
        if m.start < cursor || m.end > text.len() {
            continue;
        }
        out.text.push_str(&text[cursor..m.start]);
        let ins_start = out.text.len();
        expand_into(&tokens, m, &mut out.text);
        out.spans.push(ReplSpan {
            match_index: idx,
            start: ins_start,
            end: out.text.len(),
        });
        cursor = m.end;
        out.applied += 1;
    }
    out.text.push_str(&text[cursor.min(text.len())..]);
    out
}

/// One piece of a parsed replacement template.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    /// Literal text copied verbatim.
    Literal(String),
    /// `$1`, `${2}` — group by number.
    Number(usize),
    /// `$name`, `${name}` — group by name.
    Name(String),
}

/// Parse a replacement template.
///
/// Supported syntax:
///
/// * `$1` / `${1}`     — numbered capture group (`$0` is the whole match)
/// * `$name` / `${name}` — named capture group
/// * `$$`              — a literal `$`
/// * `\n` `\r` `\t` `\0` `\\` `\$` — the usual escapes
///
/// A reference to a group that does not exist, or that did not participate in
/// the match, expands to the empty string, matching `sed`/`regex` behaviour.
fn parse_template(template: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut lit = String::new();
    let bytes = template.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => {
                let c = bytes[i + 1];
                i += 2;
                match c {
                    b'n' => lit.push('\n'),
                    b't' => lit.push('\t'),
                    b'r' => lit.push('\r'),
                    b'0' => lit.push('\0'),
                    b'\\' => lit.push('\\'),
                    b'$' => lit.push('$'),
                    // Unknown escape: keep both characters so nothing is lost.
                    _ => {
                        lit.push('\\');
                        // `c` is the first byte of the next character; copy the
                        // whole character to stay UTF-8 correct.
                        let rest = &template[i - 1..];
                        let ch = rest.chars().next().unwrap_or('\\');
                        lit.push(ch);
                        i = i - 1 + ch.len_utf8();
                    }
                }
            }
            b'$' if i + 1 < bytes.len() && bytes[i + 1] == b'$' => {
                lit.push('$');
                i += 2;
            }
            b'$' if i + 1 < bytes.len() && bytes[i + 1] == b'{' => {
                match template[i + 2..].find('}') {
                    Some(rel) => {
                        let inner = &template[i + 2..i + 2 + rel];
                        push_ref(&mut tokens, &mut lit, inner);
                        i = i + 2 + rel + 1;
                    }
                    None => {
                        // Unterminated `${`: treat as literal text.
                        lit.push_str(&template[i..]);
                        i = bytes.len();
                    }
                }
            }
            b'$' => {
                let rest = &template[i + 1..];
                let len = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(rest.len());
                if len == 0 {
                    // A trailing `$` with nothing referable after it.
                    lit.push('$');
                    i += 1;
                } else {
                    push_ref(&mut tokens, &mut lit, &rest[..len]);
                    i += 1 + len;
                }
            }
            _ => {
                let ch = template[i..]
                    .chars()
                    .next()
                    .expect("index is on a boundary");
                lit.push(ch);
                i += ch.len_utf8();
            }
        }
    }

    if !lit.is_empty() {
        tokens.push(Token::Literal(lit));
    }
    tokens
}

/// Flush pending literal text and push a group reference token.
fn push_ref(tokens: &mut Vec<Token>, lit: &mut String, name: &str) {
    if !lit.is_empty() {
        tokens.push(Token::Literal(std::mem::take(lit)));
    }
    match name.parse::<usize>() {
        Ok(n) => tokens.push(Token::Number(n)),
        Err(_) => tokens.push(Token::Name(name.to_string())),
    }
}

/// Expand parsed tokens for one match, appending to `out`.
fn expand_into(tokens: &[Token], m: &RawMatch, out: &mut String) {
    for t in tokens {
        match t {
            Token::Literal(s) => out.push_str(s),
            Token::Number(0) => out.push_str(&m.text),
            Token::Number(n) => {
                if let Some(g) = m.groups.get(*n) {
                    if let Some(s) = &g.text {
                        out.push_str(s);
                    }
                }
            }
            Token::Name(name) => {
                if let Some(g) = m
                    .groups
                    .iter()
                    .find(|g| g.name.as_deref() == Some(name.as_str()))
                {
                    if let Some(s) = &g.text {
                        out.push_str(s);
                    }
                }
            }
        }
    }
}

/// Which capture-group references a template uses, for the hint line.
pub fn template_refs(template: &str) -> Vec<String> {
    let mut refs = Vec::new();
    for t in parse_template(template) {
        let label = match t {
            Token::Number(n) => format!("${n}"),
            Token::Name(n) => format!("${n}"),
            Token::Literal(_) => continue,
        };
        if !refs.contains(&label) {
            refs.push(label);
        }
    }
    refs
}

/// First line of a multi-line error message.
fn first_line(msg: &str) -> String {
    msg.lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or(msg)
        .to_string()
}

/// Compress a backend's multi-line error into one readable line.
fn clean_error(msg: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for line in msg.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('^') || line.chars().all(|c| c == '~') {
            continue;
        }
        let line = line
            .strip_prefix("regex parse error:")
            .unwrap_or(line)
            .trim();
        let line = line.strip_prefix("error:").unwrap_or(line).trim();
        if line.is_empty() || parts.contains(&line) {
            continue;
        }
        parts.push(line);
    }
    if parts.is_empty() {
        return first_line(msg);
    }
    // Drop the echoed pattern line, which is noise in a one-line banner.
    if parts.len() > 1 {
        parts.remove(0);
    }
    parts.join(" — ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(pattern: &str, flags: Flags, text: &str) -> Vec<RawMatch> {
        let c = compile(pattern, flags, false).expect("pattern should compile");
        c.engine.scan(text).expect("scan should succeed").matches
    }

    #[test]
    fn plain_matching() {
        let ms = scan(r"\d+", Flags::default(), "a1 bb 234");
        assert_eq!(ms.len(), 2);
        assert_eq!((ms[0].start, ms[0].end), (1, 2));
        assert_eq!(ms[1].text, "234");
    }

    #[test]
    fn inline_case_insensitive_flag() {
        let ms = scan("(?i)error", Flags::default(), "Error ERROR error eRrOr");
        assert_eq!(ms.len(), 4);
    }

    #[test]
    fn flag_toggle_matches_inline_flag() {
        let f = Flags {
            ignore_case: true,
            ..Flags::default()
        };
        let ms = scan("error", f, "Error ERROR");
        assert_eq!(ms.len(), 2);
        assert_eq!(f.inline_prefix(), "(?i)");
    }

    #[test]
    fn case_insensitive_match_inside_a_token() {
        // Requirement: matched substrings may appear mid-token.
        let ms = scan("(?i)err", Flags::default(), "xxERRORxx snafu");
        assert_eq!(ms.len(), 1);
        assert_eq!((ms[0].start, ms[0].end), (2, 5));
        assert_eq!(ms[0].text, "ERR");
    }

    #[test]
    fn literal_flag_escapes_metacharacters() {
        let f = Flags {
            literal: true,
            ..Flags::default()
        };
        let ms = scan("a.c", f, "abc a.c");
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].start, 4);
    }

    #[test]
    fn china_mobile_number_pattern() {
        let text = "call 13812345678 or 8613912345678, not 12345678901 nor 1381234567";
        let ms = scan(r"1[3-9]\d{9}", Flags::default(), text);
        let found: Vec<&str> = ms.iter().map(|m| m.text.as_str()).collect();
        assert_eq!(found, vec!["13812345678", "13912345678"]);
    }

    #[test]
    fn china_landline_pattern() {
        let ms = scan(r"0\d{2,3}-\d{7,8}", Flags::default(), "tel 010-88886666 x");
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].text, "010-88886666");
    }

    #[test]
    fn capture_groups_are_reported() {
        let ms = scan(r"(\w+)@(\w+)", Flags::default(), "bob@corp");
        assert_eq!(ms[0].groups.len(), 3);
        assert_eq!(ms[0].groups[1].text.as_deref(), Some("bob"));
        assert_eq!(ms[0].groups[2].range, Some((4, 8)));
    }

    #[test]
    fn named_groups_carry_their_name() {
        let ms = scan(r"(?P<user>\w+)@", Flags::default(), "bob@corp");
        assert_eq!(ms[0].groups[1].name.as_deref(), Some("user"));
    }

    #[test]
    fn non_participating_group_is_none() {
        let ms = scan(r"(a)|(b)", Flags::default(), "b");
        assert_eq!(ms[0].groups[1].text, None);
        assert_eq!(ms[0].groups[2].text.as_deref(), Some("b"));
    }

    #[test]
    fn fancy_fallback_for_lookahead() {
        let c = compile(r"foo(?=bar)", Flags::default(), false).expect("should fall back");
        assert_eq!(c.backend, Backend::Fancy);
        assert!(c.fallback_reason.is_some());
        let ms = c.engine.scan("foobar foobaz").expect("scan").matches;
        assert_eq!(ms.len(), 1);
        assert_eq!(ms[0].start, 0);
    }

    #[test]
    fn fancy_fallback_for_backreference() {
        let c = compile(r"(\w)\1", Flags::default(), false).expect("should fall back");
        assert_eq!(c.backend, Backend::Fancy);
        let ms = c.engine.scan("aa ab cc").expect("scan").matches;
        assert_eq!(ms.len(), 2);
    }

    #[test]
    fn genuine_syntax_error_is_reported() {
        let e = compile("a(", Flags::default(), false).expect_err("must fail");
        assert!(!e.message.is_empty());
        assert!(!e.message.contains('\n'));
    }

    #[test]
    fn empty_pattern_is_rejected() {
        assert!(compile("", Flags::default(), false).is_err());
    }

    #[test]
    fn zero_width_matches_are_kept_and_flagged() {
        let ms = scan(r"\b", Flags::default(), "hi yo");
        assert!(ms.iter().all(|m| m.is_empty()));
        assert_eq!(ms.len(), 4);
    }

    #[test]
    fn replacement_keeps_untouched_content() {
        let text = "keep\nfoo bar\nkeep2\n";
        let ms = scan("foo", Flags::default(), text);
        let r = replace_all(text, &ms, "QUX", false);
        assert_eq!(r.text, "keep\nQUX bar\nkeep2\n");
        assert_eq!(r.applied, 1);
        assert_eq!(r.spans.len(), 1);
        let s = r.spans[0];
        assert_eq!(&r.text[s.start..s.end], "QUX");
    }

    #[test]
    fn replacement_expands_numbered_groups() {
        let text = "2024-05-06";
        let ms = scan(r"(\d{4})-(\d{2})-(\d{2})", Flags::default(), text);
        let r = replace_all(text, &ms, "$3/$2/$1", false);
        assert_eq!(r.text, "06/05/2024");
    }

    #[test]
    fn replacement_expands_named_and_braced_groups() {
        let text = "bob@corp";
        let ms = scan(r"(?P<user>\w+)@(?P<host>\w+)", Flags::default(), text);
        let r = replace_all(text, &ms, "${host}:${user}", false);
        assert_eq!(r.text, "corp:bob");
        let r2 = replace_all(text, &ms, "$host/$user", false);
        assert_eq!(r2.text, "corp/bob");
    }

    #[test]
    fn replacement_dollar_zero_is_whole_match() {
        let ms = scan(r"\d+", Flags::default(), "id 42");
        let r = replace_all("id 42", &ms, "[$0]", false);
        assert_eq!(r.text, "id [42]");
    }

    #[test]
    fn replacement_escapes() {
        let ms = scan("X", Flags::default(), "aXb");
        assert_eq!(replace_all("aXb", &ms, r"$$", false).text, "a$b");
        assert_eq!(replace_all("aXb", &ms, r"\$1", false).text, "a$1b");
        assert_eq!(replace_all("aXb", &ms, r"\t", false).text, "a\tb");
        assert_eq!(replace_all("aXb", &ms, r"\n", false).text, "a\nb");
        assert_eq!(replace_all("aXb", &ms, r"\\", false).text, "a\\b");
    }

    #[test]
    fn replacement_unknown_group_is_empty() {
        let ms = scan(r"\d+", Flags::default(), "n 7");
        assert_eq!(replace_all("n 7", &ms, "<$9>", false).text, "n <>");
        assert_eq!(replace_all("n 7", &ms, "<$nope>", false).text, "n <>");
    }

    #[test]
    fn replacement_first_only() {
        let text = "a a a";
        let ms = scan("a", Flags::default(), text);
        let r = replace_all(text, &ms, "b", true);
        assert_eq!(r.text, "b a a");
        assert_eq!(r.applied, 1);
    }

    #[test]
    fn replacement_with_empty_template_deletes() {
        let text = "rm-me keep";
        let ms = scan("rm-me ", Flags::default(), text);
        assert_eq!(replace_all(text, &ms, "", false).text, "keep");
    }

    #[test]
    fn replacement_spans_track_every_match() {
        let text = "x1 x2 x3";
        let ms = scan(r"x\d", Flags::default(), text);
        let r = replace_all(text, &ms, "[$0]", false);
        assert_eq!(r.text, "[x1] [x2] [x3]");
        assert_eq!(r.spans.len(), 3);
        for (i, s) in r.spans.iter().enumerate() {
            assert_eq!(s.match_index, i);
            assert_eq!(&r.text[s.start..s.end], format!("[x{}]", i + 1));
        }
    }

    #[test]
    fn replacement_on_multibyte_text() {
        let text = "English-only text 100 English-only text";
        let ms = scan(r"\d+", Flags::default(), text);
        let r = replace_all(text, &ms, "N", false);
        assert_eq!(r.text, "English-only text N English-only text");
    }

    #[test]
    fn replacement_of_zero_width_matches_inserts() {
        let text = "ab";
        let ms = scan(r"\b", Flags::default(), text);
        let r = replace_all(text, &ms, "|", false);
        assert_eq!(r.text, "|ab|");
    }

    #[test]
    fn template_refs_are_listed() {
        assert_eq!(template_refs("$1-${name}-$$"), vec!["$1", "$name"]);
        assert!(template_refs("plain").is_empty());
    }

    #[test]
    fn multiline_flag_affects_anchors() {
        let f = Flags {
            multi_line: true,
            ..Flags::default()
        };
        assert_eq!(scan("^b", f, "a\nb\nc").len(), 1);
        assert_eq!(scan("^b", Flags::default(), "a\nb\nc").len(), 0);
    }

    #[test]
    fn dot_all_flag_crosses_lines() {
        let f = Flags {
            dot_all: true,
            ..Flags::default()
        };
        assert_eq!(scan("a.b", f, "a\nb").len(), 1);
        assert_eq!(scan("a.b", Flags::default(), "a\nb").len(), 0);
    }

    #[test]
    fn extended_flag_ignores_whitespace() {
        let f = Flags {
            extended: true,
            ..Flags::default()
        };
        assert_eq!(scan(r"\d{3} - \d{4}", f, "555-1234").len(), 1);
    }

    #[test]
    fn group_names_include_slot_zero() {
        let c = compile(r"(?P<a>x)(y)", Flags::default(), false).unwrap();
        let names = c.engine.group_names();
        assert_eq!(names.len(), 3);
        assert_eq!(names[0], None);
        assert_eq!(names[1].as_deref(), Some("a"));
        assert_eq!(names[2], None);
    }
}
