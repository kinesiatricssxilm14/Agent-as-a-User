//! Syntax highlighting for the command line.
//!
//! The highlighter classifies each character of the command line into a [`TokenClass`], which
//! the UI turns into colours. Classification is span based rather than regex based so it stays
//! consistent with [`crate::pipeline`]'s view of quoting and stage boundaries.

use crate::pipeline::{Stage, split_stages};

/// Semantic class of a run of characters, used to pick a colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenClass {
    /// The command word of a stage (`grep`, `tail`, ...).
    Command,
    /// A `-f` / `--flag` argument.
    Flag,
    /// A single or double quoted string.
    String,
    /// A `$VAR` or `$(...)` expansion.
    Variable,
    /// The `|` pipeline separator.
    Pipe,
    /// Redirections and other shell operators.
    Operator,
    /// Something that looks like a filesystem path.
    Path,
    /// A bare number.
    Number,
    /// Anything else.
    Plain,
}

/// A classified span of the command line, in character indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    pub class: TokenClass,
}

/// Classify the whole command line.
///
/// The returned tokens are ordered, non-overlapping, and cover every character, so the
/// renderer can walk them directly.
pub fn highlight(chars: &[char]) -> Vec<Token> {
    let stages = split_stages(chars);
    let mut tokens = Vec::new();
    for (i, stage) in stages.iter().enumerate() {
        if i > 0 {
            // The separator sits just before this stage's start.
            let p = stage.range.start.saturating_sub(1);
            if p < chars.len() && chars[p] == '|' {
                tokens.push(Token { start: p, end: p + 1, class: TokenClass::Pipe });
            }
        }
        classify_stage(chars, stage, &mut tokens);
    }
    tokens.sort_by_key(|t| t.start);
    fill_gaps(chars.len(), tokens)
}

/// Classify the tokens inside one pipeline stage.
fn classify_stage(chars: &[char], stage: &Stage, out: &mut Vec<Token>) {
    let range = stage.trimmed_range(chars);
    let mut i = range.start;
    let mut seen_command = false;
    while i < range.end {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        let tok = token_span(chars, i, range.end);
        if tok.0 >= tok.1 {
            break;
        }
        let (start, end) = tok;
        let text: String = chars[start..end].iter().collect();
        let class = if !seen_command && !is_assignment_token(&text) {
            seen_command = true;
            TokenClass::Command
        } else if is_assignment_token(&text) {
            TokenClass::Variable
        } else {
            classify_argument(&text)
        };
        // Quoted and expanded arguments get sub-spans so the quotes themselves are coloured.
        if class == TokenClass::Command || !push_sub_tokens(chars, start, end, out) {
            out.push(Token { start, end, class });
        }
        i = end;
    }
}

/// Split an argument into quoted / expanded / plain sub-spans.
///
/// Returns `false` when the argument has no internal structure worth colouring separately.
fn push_sub_tokens(chars: &[char], start: usize, end: usize, out: &mut Vec<Token>) -> bool {
    let has_structure = chars[start..end]
        .iter()
        .any(|&c| c == '\'' || c == '"' || c == '$');
    if !has_structure {
        return false;
    }
    let mut i = start;
    let mut plain_start = None;
    let flush = |plain_start: &mut Option<usize>, at: usize, out: &mut Vec<Token>| {
        if let Some(s) = plain_start.take()
            && s < at
        {
            let text: String = chars[s..at].iter().collect();
            out.push(Token { start: s, end: at, class: classify_argument(&text) });
        }
    };
    while i < end {
        match chars[i] {
            '\'' | '"' => {
                flush(&mut plain_start, i, out);
                let q = chars[i];
                let mut j = i + 1;
                while j < end {
                    if q == '"' && chars[j] == '\\' {
                        j = (j + 2).min(end);
                        continue;
                    }
                    if chars[j] == q {
                        j += 1;
                        break;
                    }
                    j += 1;
                }
                out.push(Token { start: i, end: j, class: TokenClass::String });
                i = j;
            }
            '$' => {
                flush(&mut plain_start, i, out);
                let mut j = i + 1;
                if j < end && chars[j] == '(' {
                    let mut depth = 1usize;
                    j += 1;
                    while j < end && depth > 0 {
                        match chars[j] {
                            '(' => depth += 1,
                            ')' => depth -= 1,
                            _ => {}
                        }
                        j += 1;
                    }
                } else if j < end && chars[j] == '{' {
                    while j < end && chars[j] != '}' {
                        j += 1;
                    }
                    j = (j + 1).min(end);
                } else {
                    while j < end && (chars[j].is_alphanumeric() || chars[j] == '_') {
                        j += 1;
                    }
                }
                out.push(Token { start: i, end: j, class: TokenClass::Variable });
                i = j;
            }
            _ => {
                if plain_start.is_none() {
                    plain_start = Some(i);
                }
                i += 1;
            }
        }
    }
    flush(&mut plain_start, end, out);
    true
}

/// Classify a bare (unquoted) argument by shape.
fn classify_argument(text: &str) -> TokenClass {
    if text.is_empty() {
        return TokenClass::Plain;
    }
    if text.starts_with('-') && text.len() > 1 {
        return TokenClass::Flag;
    }
    if matches!(text, ">" | ">>" | "<" | "&&" | "||" | ";" | "&" | "2>" | "2>&1") {
        return TokenClass::Operator;
    }
    if text.parse::<f64>().is_ok() {
        return TokenClass::Number;
    }
    if text.contains('/') || text.starts_with('~') {
        return TokenClass::Path;
    }
    TokenClass::Plain
}

fn is_assignment_token(text: &str) -> bool {
    match text.find('=') {
        Some(0) | None => false,
        Some(eq) => {
            let name = &text[..eq];
            name.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
    }
}

/// Span of the token starting at `from`, honouring quotes so `'a b'` stays one token.
fn token_span(chars: &[char], from: usize, end: usize) -> (usize, usize) {
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
    (start, i)
}

/// Insert [`TokenClass::Plain`] tokens for characters no rule claimed (whitespace, mostly)
/// so the token list covers the entire line.
fn fill_gaps(len: usize, tokens: Vec<Token>) -> Vec<Token> {
    let mut out = Vec::with_capacity(tokens.len() * 2);
    let mut pos = 0usize;
    for t in tokens {
        if t.start > pos {
            out.push(Token { start: pos, end: t.start, class: TokenClass::Plain });
        }
        pos = t.end.max(pos);
        out.push(t);
    }
    if pos < len {
        out.push(Token { start: pos, end: len, class: TokenClass::Plain });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cv(s: &str) -> Vec<char> {
        s.chars().collect()
    }

    /// Class of the token covering character `at`.
    fn class_at(line: &str, at: usize) -> TokenClass {
        let c = cv(line);
        highlight(&c)
            .into_iter()
            .find(|t| at >= t.start && at < t.end)
            .map(|t| t.class)
            .unwrap()
    }

    #[test]
    fn covers_every_character_without_overlap() {
        for line in [
            "cat /bench/server.log | grep -i 'error' | tail -n 20",
            r#"awk -F"|" '{print $1}' f"#,
            "",
            "   ",
            "echo $HOME $(date) ${X}",
        ] {
            let chars = cv(line);
            let tokens = highlight(&chars);
            let mut pos = 0;
            for t in &tokens {
                assert_eq!(t.start, pos, "gap/overlap in {line:?}");
                assert!(t.end > t.start || chars.is_empty());
                pos = t.end;
            }
            assert_eq!(pos, chars.len(), "coverage short in {line:?}");
        }
    }

    #[test]
    fn classifies_commands_flags_and_pipes() {
        let line = "cat /bench/server.log | grep -i error | tail -n 20";
        assert_eq!(class_at(line, 0), TokenClass::Command); // cat
        assert_eq!(class_at(line, 4), TokenClass::Path); // /bench/...
        assert_eq!(class_at(line, 22), TokenClass::Pipe);
        assert_eq!(class_at(line, 24), TokenClass::Command); // grep
        assert_eq!(class_at(line, 29), TokenClass::Flag); // -i
        assert_eq!(class_at(line, 48), TokenClass::Number); // 20
    }

    #[test]
    fn classifies_strings_and_variables() {
        assert_eq!(class_at("grep 'ERROR' f", 5), TokenClass::String);
        assert_eq!(class_at("echo $HOME", 5), TokenClass::Variable);
        assert_eq!(class_at("echo $(date)", 5), TokenClass::Variable);
        assert_eq!(class_at("echo ${X}", 5), TokenClass::Variable);
        // Assignment prefix is a variable, and the real command still highlights.
        let line = "LC_ALL=C sort";
        assert_eq!(class_at(line, 0), TokenClass::Variable);
        assert_eq!(class_at(line, 9), TokenClass::Command);
    }

    #[test]
    fn pipe_inside_quotes_is_not_a_separator() {
        let line = r#"awk -F"|" '{print $1}' f"#;
        assert_eq!(class_at(line, 6), TokenClass::String);
        let chars = cv(line);
        assert!(!highlight(&chars).iter().any(|t| t.class == TokenClass::Pipe));
    }

    #[test]
    fn empty_line_yields_no_tokens() {
        assert!(highlight(&cv("")).is_empty());
    }
}
