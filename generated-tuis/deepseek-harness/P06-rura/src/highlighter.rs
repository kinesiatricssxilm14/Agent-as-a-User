//! Pure (UI-independent) syntax highlighting for shell pipeline commands.
//!
//! The highlighter scans a command line once and returns byte ranges annotated
//! with a [`TokenKind`]. The UI layer maps each kind to a color.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    /// First word of a pipeline segment (a command or executable).
    Command,
    /// A short or long option such as `-n` or `--color=auto`.
    Flag,
    /// A single- or double-quoted string.
    String,
    /// Pipe/redirection/separator characters: `|`, `||`, `&&`, `;`, `<`, `>`...
    Operator,
    /// A plain argument.
    Arg,
    /// A numeric literal.
    Number,
    /// A variable reference such as `$HOME` or `${PATH}`.
    Variable,
}

#[derive(Clone, Copy, Debug)]
pub struct Hl {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
}

fn is_op(c: char) -> bool {
    matches!(c, '|' | '&' | ';' | '<' | '>')
}

fn scan_op(line: &str, i: usize) -> usize {
    let b = line.as_bytes();
    let mut j = i;
    while j < b.len() {
        let c = line[j..].chars().next().unwrap();
        if is_op(c) {
            j += c.len_utf8();
        } else {
            break;
        }
    }
    j
}

fn scan_quote(line: &str, i: usize, q: char) -> usize {
    let b = line.as_bytes();
    let mut j = i + q.len_utf8();
    while j < b.len() {
        let c = line[j..].chars().next().unwrap();
        if c == '\\' {
            j += 1;
            if j < b.len() {
                j += line[j..].chars().next().unwrap().len_utf8();
            }
        } else if c == q {
            j += c.len_utf8();
            break;
        } else {
            j += c.len_utf8();
        }
    }
    j
}

fn scan_word(line: &str, i: usize) -> usize {
    let b = line.as_bytes();
    let mut j = i;
    while j < b.len() {
        let c = line[j..].chars().next().unwrap();
        if c.is_whitespace() || is_op(c) || c == '\'' || c == '"' {
            break;
        }
        j += c.len_utf8();
    }
    j
}

/// Highlight `line`, returning non-overlapping byte ranges.
pub fn highlight(line: &str) -> Vec<Hl> {
    let mut out = Vec::new();
    let b = line.as_bytes();
    let mut i = 0;
    let mut first_word = true;

    while i < b.len() {
        let c = line[i..].chars().next().unwrap();

        if c.is_whitespace() {
            i += c.len_utf8();
            continue;
        }

        if is_op(c) {
            let end = scan_op(line, i);
            out.push(Hl { start: i, end, kind: TokenKind::Operator });
            i = end;
            first_word = true;
            continue;
        }

        if c == '\'' || c == '"' {
            let end = scan_quote(line, i, c);
            out.push(Hl { start: i, end, kind: TokenKind::String });
            i = end;
            first_word = false;
            continue;
        }

        let end = scan_word(line, i);
        let word = &line[i..end];
        let kind = if first_word {
            first_word = false;
            TokenKind::Command
        } else if word == "-" {
            TokenKind::Arg
        } else if word.starts_with("--") {
            TokenKind::Flag
        } else if word.starts_with('-')
            && word.len() > 1
            && word[1..].chars().next().map_or(false, |d| d.is_ascii_digit())
        {
            TokenKind::Number
        } else if word.starts_with('-') {
            TokenKind::Flag
        } else if word.starts_with('$') {
            TokenKind::Variable
        } else if word.chars().all(|c| c.is_ascii_digit() || c == '.') {
            TokenKind::Number
        } else {
            TokenKind::Arg
        };
        out.push(Hl { start: i, end, kind });
        i = end;
    }

    out
}
