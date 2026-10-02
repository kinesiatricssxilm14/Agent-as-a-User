//! Pipeline splitting utilities.
//!
//! The splitter respects single quotes, double quotes and backslash escapes so
//! that a `|` inside a quoted string does not split the pipeline. The result is
//! used both for the live "pipeline preview" and for partial execution
//! (`Alt+\`).

#[derive(Clone, Debug)]
pub struct Segment {
    /// Byte offset of the character just after the last non-separator char,
    /// i.e. the position of the separator that follows this segment. For the
    /// final segment this equals `input.len()`.
    pub end: usize,
    /// Trimmed segment text, suitable for display.
    pub text: String,
}

/// Return the length (in bytes) of a command separator starting at `i`, or 0.
fn separator_len(input: &str, i: usize) -> usize {
    let b = input.as_bytes();
    let c = input[i..].chars().next().unwrap();
    let cl = c.len_utf8();
    match c {
        '|' => {
            if i + cl < b.len() {
                let n = input[i + cl..].chars().next().unwrap();
                if n == '|' || n == '&' {
                    return cl + n.len_utf8();
                }
            }
            cl
        }
        '&' => {
            if i + cl < b.len() {
                let n = input[i + cl..].chars().next().unwrap();
                if n == '&' {
                    return cl + n.len_utf8();
                }
            }
            // A lone `&` is not treated as a separator so that `2>&1` keeps
            // working as a single segment.
            0
        }
        ';' => cl,
        _ => 0,
    }
}

/// Split `input` into pipeline segments on unquoted separators
/// (`|`, `|&`, `||`, `&&`, `;`).
pub fn split_pipeline(input: &str) -> Vec<Segment> {
    let mut segs = Vec::new();
    let b = input.as_bytes();
    let mut i = 0;
    let mut seg_start = 0;
    let mut in_single = false;
    let mut in_double = false;
    let mut escaped = false;

    while i < b.len() {
        let c = input[i..].chars().next().unwrap();
        let cl = c.len_utf8();

        if escaped {
            escaped = false;
            i += cl;
            continue;
        }
        if in_single {
            if c == '\'' {
                in_single = false;
            }
            i += cl;
            continue;
        }
        if in_double {
            if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_double = false;
            }
            i += cl;
            continue;
        }

        if c == '\'' {
            in_single = true;
            i += cl;
            continue;
        }
        if c == '"' {
            in_double = true;
            i += cl;
            continue;
        }
        if c == '\\' {
            escaped = true;
            i += cl;
            continue;
        }

        let sep = separator_len(input, i);
        if sep > 0 {
            let text = input[seg_start..i].trim().to_string();
            segs.push(Segment { end: i, text });
            i += sep;
            seg_start = i;
            continue;
        }

        i += cl;
    }

    let text = input[seg_start..].trim().to_string();
    segs.push(Segment { end: input.len(), text });
    segs
}

/// Index of the segment that contains the given cursor byte offset.
pub fn segment_at(input: &str, cursor: usize) -> Option<usize> {
    let segs = split_pipeline(input);
    for (i, seg) in segs.iter().enumerate() {
        if cursor <= seg.end {
            return Some(i);
        }
    }
    segs.len().checked_sub(1)
}

/// The pipeline prefix that ends at the segment containing the cursor.
/// This is what `Alt+\` executes.
pub fn partial_prefix(input: &str, cursor: usize) -> String {
    let segs = split_pipeline(input);
    for seg in &segs {
        if cursor <= seg.end {
            return input[..seg.end].to_string();
        }
    }
    input.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_simple_pipeline() {
        let segs = split_pipeline("cat a | grep x | tail -5");
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].text, "cat a");
        assert_eq!(segs[1].text, "grep x");
        assert_eq!(segs[2].text, "tail -5");
    }

    #[test]
    fn respects_quotes() {
        let segs = split_pipeline("grep 'a|b' | wc -l");
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "grep 'a|b'");
    }

    #[test]
    fn partial_prefix_mid() {
        let s = "cat a | grep x | tail -5";
        let cursor = s.find("grep").unwrap() + 2;
        // The runner trims the trailing whitespace left before the pipe.
        assert_eq!(partial_prefix(s, cursor).trim(), "cat a | grep x");
    }

    #[test]
    fn partial_prefix_full() {
        let s = "cat a | grep x";
        assert_eq!(partial_prefix(s, s.len()), s);
    }
}
