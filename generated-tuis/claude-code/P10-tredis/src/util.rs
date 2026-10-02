//! Small text helpers shared by the UI and the command parsers.

use unicode_width::UnicodeWidthChar;

/// Display width of a string, ignoring control characters.
pub fn width(s: &str) -> usize {
    s.chars().map(|c| c.width().unwrap_or(0)).sum()
}

/// Shorten `s` so that it fits into `max` display columns, appending `…`.
pub fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if width(s) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > max.saturating_sub(1) {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

/// Pad `s` on the right so it occupies `w` display columns. Unlike `{:<w$}`,
/// which counts characters, this is correct for wide (CJK) and zero-width
/// characters, keeping table columns aligned.
pub fn pad(s: &str, w: usize) -> String {
    let cur = width(s);
    if cur >= w {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + (w - cur));
    out.push_str(s);
    out.extend(std::iter::repeat_n(' ', w - cur));
    out
}

/// Pad `s` on the left to `w` display columns.
pub fn pad_left(s: &str, w: usize) -> String {
    let cur = width(s);
    if cur >= w {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + (w - cur));
    out.extend(std::iter::repeat_n(' ', w - cur));
    out.push_str(s);
    out
}

/// Replace control characters so a value can be shown on a single line.
pub fn one_line(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0' => out.push_str("\\0"),
            c if is_control(c) => out.push('·'),
            c => out.push(c),
        }
    }
    out
}

/// Characters that must never reach the terminal verbatim: C0/C1 controls plus
/// the isolated surrogate/BOM range that some emulators mis-handle. Redis
/// values are arbitrary bytes, so this is a real concern, not a theoretical one.
fn is_control(c: char) -> bool {
    let u = c as u32;
    u < 0x20 || u == 0x7f || (0x80..=0x9f).contains(&u) || u == 0x200b || u == 0xfeff
}

/// Make a raw Redis value safe to draw: control bytes become a visible `·` so
/// they can neither move the cursor nor start an escape sequence.
pub fn sanitize(s: &str) -> String {
    if !s.chars().any(is_control) {
        return s.to_string();
    }
    s.chars()
        .map(|c| if is_control(c) { '·' } else { c })
        .collect()
}

/// Wrap `s` into lines of at most `w` columns, preferring word boundaries.
/// Embedded newlines start a new line. At most `max_lines` lines are produced;
/// the last one is suffixed with `…` when content had to be dropped.
pub fn wrap(s: &str, w: usize, max_lines: usize) -> Vec<String> {
    if w == 0 || max_lines == 0 {
        return Vec::new();
    }
    let mut lines: Vec<String> = Vec::new();
    let mut truncated = false;

    'outer: for raw in s.split('\n') {
        let raw = raw.replace('\t', "    ").replace('\r', "");
        if raw.is_empty() {
            if lines.len() == max_lines {
                truncated = true;
                break 'outer;
            }
            lines.push(String::new());
            continue;
        }
        let mut cur = String::new();
        let mut cur_w = 0usize;
        for word in raw.split_inclusive(' ') {
            let mut word = word.to_string();
            // A single word longer than the line is hard-split.
            while width(&word) > w {
                let mut head = String::new();
                let mut hw = 0usize;
                let mut rest = String::new();
                for c in word.chars() {
                    let cw = c.width().unwrap_or(0);
                    if hw + cw <= w && rest.is_empty() {
                        head.push(c);
                        hw += cw;
                    } else {
                        rest.push(c);
                    }
                }
                if !cur.is_empty() {
                    if lines.len() == max_lines {
                        truncated = true;
                        break 'outer;
                    }
                    lines.push(std::mem::take(&mut cur));
                    cur_w = 0;
                }
                if lines.len() == max_lines {
                    truncated = true;
                    break 'outer;
                }
                lines.push(head);
                word = rest;
            }
            let ww = width(&word);
            if cur_w + ww > w && !cur.is_empty() {
                if lines.len() == max_lines {
                    truncated = true;
                    break 'outer;
                }
                lines.push(std::mem::take(&mut cur));
                cur_w = 0;
            }
            cur.push_str(&word);
            cur_w += ww;
        }
        if !cur.is_empty() || raw.is_empty() {
            if lines.len() == max_lines {
                truncated = true;
                break 'outer;
            }
            lines.push(cur);
        }
    }

    if truncated {
        if let Some(last) = lines.last_mut() {
            let keep = w.saturating_sub(1);
            *last = format!("{}…", truncate_hard(last, keep));
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn truncate_hard(s: &str, max: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for c in s.chars() {
        let cw = c.width().unwrap_or(0);
        if used + cw > max {
            break;
        }
        out.push(c);
        used += cw;
    }
    out
}

/// Split a command line into tokens, honouring double/single quotes and
/// backslash escapes so that values with spaces can be entered.
pub fn tokenize(input: &str) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut have = false;
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => {
                if have {
                    out.push(std::mem::take(&mut cur));
                    have = false;
                }
            }
            '"' | '\'' => {
                let quote = c;
                have = true;
                let mut closed = false;
                while let Some(c2) = chars.next() {
                    if c2 == '\\' && quote == '"' {
                        match chars.next() {
                            Some('n') => cur.push('\n'),
                            Some('t') => cur.push('\t'),
                            Some('r') => cur.push('\r'),
                            Some(other) => cur.push(other),
                            None => return Err("trailing backslash".into()),
                        }
                    } else if c2 == quote {
                        closed = true;
                        break;
                    } else {
                        cur.push(c2);
                    }
                }
                if !closed {
                    return Err(format!("unbalanced {quote} quote"));
                }
            }
            '\\' => {
                have = true;
                match chars.next() {
                    Some('n') => cur.push('\n'),
                    Some('t') => cur.push('\t'),
                    Some(other) => cur.push(other),
                    None => return Err("trailing backslash".into()),
                }
            }
            c => {
                have = true;
                cur.push(c);
            }
        }
    }
    if have {
        out.push(cur);
    }
    Ok(out)
}

/// Parse `a=1 b=2` or `a 1 b 2` into ordered pairs.
pub fn parse_pairs(input: &str) -> Result<Vec<(String, String)>, String> {
    let toks = tokenize(input)?;
    if toks.is_empty() {
        return Err("no field/value pairs given".into());
    }
    if toks.iter().all(|t| t.contains('=')) {
        let mut out = Vec::new();
        for t in toks {
            let (k, v) = t.split_once('=').unwrap();
            if k.is_empty() {
                return Err("empty field name".into());
            }
            out.push((k.to_string(), v.to_string()));
        }
        return Ok(out);
    }
    if toks.len() % 2 != 0 {
        return Err("expected pairs: `field=value ...` or `field value ...`".into());
    }
    Ok(toks
        .chunks(2)
        .map(|c| (c[0].clone(), c[1].clone()))
        .collect())
}

/// Parse `member=score` / `member score` / `score member` pairs for sorted sets.
pub fn parse_scored(input: &str) -> Result<Vec<(String, f64)>, String> {
    let pairs = parse_pairs(input)?;
    let mut out = Vec::new();
    for (a, b) in pairs {
        if let Ok(score) = b.trim().parse::<f64>() {
            out.push((a, score));
        } else if let Ok(score) = a.trim().parse::<f64>() {
            out.push((b, score));
        } else {
            return Err(format!("`{a}`/`{b}`: no numeric score found"));
        }
    }
    Ok(out)
}

pub fn fmt_ttl(ttl: i64) -> String {
    match ttl {
        -1 => "-".to_string(),
        -2 => "gone".to_string(),
        s if s < 0 => s.to_string(),
        s => {
            let (d, h, m, sec) = (s / 86400, (s % 86400) / 3600, (s % 3600) / 60, s % 60);
            if d > 0 {
                format!("{d}d{h}h")
            } else if h > 0 {
                format!("{h}h{m}m")
            } else if m > 0 {
                format!("{m}m{sec}s")
            } else {
                format!("{sec}s")
            }
        }
    }
}

pub fn fmt_bytes(n: usize) -> String {
    const K: usize = 1024;
    if n < K {
        format!("{n} B")
    } else if n < K * K {
        format!("{:.1} KiB", n as f64 / K as f64)
    } else {
        format!("{:.1} MiB", n as f64 / (K * K) as f64)
    }
}

/// Render a f64 score without a trailing `.0` for whole numbers.
pub fn fmt_score(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v}");
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_quotes() {
        assert_eq!(
            tokenize(r#"a "b c" d\ e"#).unwrap(),
            vec!["a", "b c", "d e"]
        );
        assert!(tokenize("\"oops").is_err());
    }

    #[test]
    fn pairs_both_forms() {
        assert_eq!(
            parse_pairs("f=1 g=2").unwrap(),
            vec![("f".into(), "1".into()), ("g".into(), "2".into())]
        );
        assert_eq!(
            parse_pairs("f 1 g 2").unwrap(),
            vec![("f".into(), "1".into()), ("g".into(), "2".into())]
        );
        assert!(parse_pairs("f 1 g").is_err());
    }

    #[test]
    fn scored_pairs() {
        assert_eq!(parse_scored("alice=10").unwrap(), vec![("alice".into(), 10.0)]);
        assert_eq!(parse_scored("10 alice").unwrap(), vec![("alice".into(), 10.0)]);
    }

    #[test]
    fn wrap_respects_width() {
        let lines = wrap("hello wonderful world", 8, 10);
        assert!(lines.iter().all(|l| width(l) <= 8), "{lines:?}");
        assert_eq!(lines.join("").replace(' ', ""), "hellowonderfulworld");
    }

    #[test]
    fn wrap_caps_lines() {
        let lines = wrap("a b c d e f g h", 2, 2);
        assert_eq!(lines.len(), 2);
        assert!(lines[1].ends_with('…'));
    }

    #[test]
    fn sanitize_strips_control_bytes() {
        // Redis values are arbitrary bytes; an escape sequence reaching the
        // terminal would corrupt the whole screen.
        let evil = "a\u{1b}[31mred\u{0}\u{7}b";
        let clean = sanitize(evil);
        assert!(!clean.contains('\u{1b}'), "{clean:?}");
        assert!(!clean.contains('\u{0}'));
        assert_eq!(clean, "a·[31mred··b");
        // Ordinary text is returned untouched.
        assert_eq!(sanitize("plain ünïcode English-only text"), "plain ünïcode English-only text");
    }

    #[test]
    fn pads_by_display_width_not_char_count() {
        // Each CJK char is two columns wide, so `{:<6}` would be wrong here.
        assert_eq!(width(&pad("English-only text", 6)), 6);
        assert_eq!(width(&pad("ab", 6)), 6);
        assert_eq!(pad("abcdef", 3), "abcdef");
        assert_eq!(width(&pad_left("English-only text", 6)), 6);
        assert!(pad_left("x", 4).starts_with("   "));
    }

    #[test]
    fn truncate_adds_ellipsis() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abc", 4), "abc");
    }
}
