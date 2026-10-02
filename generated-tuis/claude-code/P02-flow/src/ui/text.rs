//! Text wrapping and truncation measured in terminal display columns.
//!
//! Ratatui's `Wrap` is not used for the card body: knowing the exact wrapped line count is what
//! makes the scroll bound exact, and `Paragraph::line_count` sits behind an unstable feature.
//! Wrapping here also keeps CJK and emoji correct, since a wide glyph occupies two columns while
//! counting as one char.

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

/// Display width of a string in terminal columns.
pub fn width(text: &str) -> usize {
    text.width()
}

/// Wrap `text` to `max_width` columns, preferring to break at spaces.
///
/// Explicit newlines always start a new line, and a blank line stays blank so the body's visual
/// structure survives. A word longer than the whole width is split rather than allowed to
/// overflow. Returns at least one line so an empty body still occupies a row.
pub fn wrap(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![String::new()];
    }
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            out.push(String::new());
            continue;
        }
        out.extend(wrap_paragraph(paragraph, max_width));
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// Wrap one newline-free run of text.
fn wrap_paragraph(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    // Break a line, dropping whitespace that would otherwise dangle at the end of it.
    // Leading indentation is preserved because it is only ever trimmed from the right.
    let flush = |current: &mut String, current_width: &mut usize, lines: &mut Vec<String>| {
        let trimmed = current.trim_end();
        if !trimmed.is_empty() || !current.is_empty() {
            lines.push(trimmed.to_string());
        }
        current.clear();
        *current_width = 0;
    };

    // `split_word_bounds` keeps whitespace as its own token, so indentation is preserved.
    for word in text.split_word_bounds() {
        let word_width = word.width();

        if current_width + word_width <= max_width {
            current.push_str(word);
            current_width += word_width;
            continue;
        }

        // Whitespace at a break point is dropped rather than pushed onto the next line.
        if word.chars().all(char::is_whitespace) {
            if !current.is_empty() {
                flush(&mut current, &mut current_width, &mut lines);
            }
            continue;
        }

        if !current.is_empty() {
            flush(&mut current, &mut current_width, &mut lines);
        }

        if word_width <= max_width {
            current.push_str(word);
            current_width = word_width;
        } else {
            // A single word wider than the line: hard-split it on grapheme boundaries.
            for cluster in word.graphemes(true) {
                let cluster_width = cluster.width();
                if current_width + cluster_width > max_width && !current.is_empty() {
                    flush(&mut current, &mut current_width, &mut lines);
                }
                current.push_str(cluster);
                current_width += cluster_width;
            }
        }
    }

    if !current.is_empty() {
        let trimmed = current.trim_end();
        lines.push(trimmed.to_string());
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

/// Truncate to `max_width` columns, appending `…` when characters are dropped.
///
/// Used for one-line contexts such as card rows, where wrapping is not an option.
pub fn truncate(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let budget = max_width - 1;
    let mut out = String::new();
    let mut used = 0usize;
    for cluster in text.graphemes(true) {
        let cluster_width = cluster.width();
        if used + cluster_width > budget {
            break;
        }
        out.push_str(cluster);
        used += cluster_width;
    }
    out.push('…');
    out
}

/// Truncate from the *left*, keeping the end of the string.
///
/// For file system paths the tail (the directory and file name) identifies the thing; the leading
/// mount points are the redundant part, so a clipped path should lose its head, not its tail.
pub fn truncate_start(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if text.width() <= max_width {
        return text.to_string();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let budget = max_width - 1;
    // Walk clusters from the end until the budget is spent, then re-assemble in order.
    let mut kept: Vec<&str> = Vec::new();
    let mut used = 0usize;
    for cluster in text.graphemes(true).rev() {
        let cluster_width = cluster.width();
        if used + cluster_width > budget {
            break;
        }
        kept.push(cluster);
        used += cluster_width;
    }
    kept.reverse();
    format!("…{}", kept.concat())
}

/// Clamp a scroll offset so the last line can reach the top of the viewport but no further.
///
/// Over-scrolling a ratatui `Paragraph` renders a completely blank pane, which reads as data
/// loss; every scrollable pane routes its offset through here.
pub fn clamp_scroll(offset: u16, total_lines: usize, viewport_height: u16) -> u16 {
    let max = total_lines.saturating_sub(viewport_height as usize);
    offset.min(max as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_at_word_boundaries() {
        let lines = wrap("the quick brown fox", 10);
        assert_eq!(lines, vec!["the quick", "brown fox"]);
        for line in &lines {
            assert!(width(line) <= 10, "{line:?} is too wide");
        }
    }

    #[test]
    fn keeps_explicit_newlines_and_blank_lines() {
        assert_eq!(wrap("a\nb", 10), vec!["a", "b"]);
        // A blank line between paragraphs is structure, not filler.
        assert_eq!(wrap("a\n\nb", 10), vec!["a", "", "b"]);
        assert_eq!(wrap("\n", 10), vec!["", ""]);
    }

    #[test]
    fn empty_text_still_occupies_one_line() {
        assert_eq!(wrap("", 10), vec![""]);
    }

    #[test]
    fn splits_words_longer_than_the_line() {
        let lines = wrap("abcdefghij", 4);
        assert_eq!(lines, vec!["abcd", "efgh", "ij"]);
    }

    #[test]
    fn never_exceeds_the_width_with_wide_glyphs() {
        // Each CJK glyph is two columns wide, so only two fit in five columns.
        let lines = wrap("English-only text", 5);
        for line in &lines {
            assert!(width(line) <= 5, "{line:?} has width {}", width(line));
        }
        assert_eq!(lines.concat(), "English-only text", "no characters lost");
    }

    #[test]
    fn does_not_split_grapheme_clusters() {
        let family = "👨‍👩‍👧";
        let lines = wrap(family, 2);
        // The cluster is wider than the line but must survive intact.
        assert_eq!(lines.concat(), family);
    }

    #[test]
    fn zero_width_is_handled_without_panicking() {
        assert_eq!(wrap("anything", 0), vec![""]);
        assert_eq!(truncate("anything", 0), "");
    }

    #[test]
    fn preserves_leading_indentation() {
        let lines = wrap("    indented text here", 12);
        assert!(lines[0].starts_with("    "), "got {:?}", lines[0]);
    }

    #[test]
    fn truncate_marks_dropped_text() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("exactly-10", 10), "exactly-10");
        assert_eq!(truncate("truncate me", 8), "truncat…");
        assert_eq!(truncate("abc", 1), "…");
    }

    #[test]
    fn truncate_respects_wide_glyph_widths() {
        let result = truncate("English-only text", 5);
        assert!(width(&result) <= 5, "width {} for {result:?}", width(&result));
        assert!(result.ends_with('…'));
    }

    #[test]
    fn truncate_start_keeps_the_end_of_a_path() {
        assert_eq!(truncate_start("/short", 10), "/short");
        // The file name survives; the leading directories are what gets dropped.
        let result = truncate_start("/very/long/path/to/cols/todo/item-1.md", 20);
        assert!(result.ends_with("item-1.md"), "got {result:?}");
        assert!(result.starts_with('…'), "got {result:?}");
        assert!(width(&result) <= 20, "width {} for {result:?}", width(&result));
        assert_eq!(truncate_start("abc", 1), "…");
        assert_eq!(truncate_start("abc", 0), "");
    }

    #[test]
    fn truncate_start_respects_wide_glyphs() {
        let result = truncate_start("English-only text", 5);
        assert!(width(&result) <= 5, "width {} for {result:?}", width(&result));
    }

    #[test]
    fn clamp_scroll_prevents_blank_panes() {
        // 10 lines in a 4-row viewport: the furthest useful offset is 6.
        assert_eq!(clamp_scroll(0, 10, 4), 0);
        assert_eq!(clamp_scroll(6, 10, 4), 6);
        assert_eq!(clamp_scroll(99, 10, 4), 6, "over-scroll must be pulled back");
        // Content shorter than the viewport can never scroll.
        assert_eq!(clamp_scroll(5, 2, 4), 0);
        assert_eq!(clamp_scroll(1, 0, 4), 0);
        assert_eq!(clamp_scroll(3, 10, 0), 3);
    }
}
