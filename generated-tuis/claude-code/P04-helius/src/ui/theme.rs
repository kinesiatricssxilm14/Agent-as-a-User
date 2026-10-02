//! Colours and small rendering helpers, kept in one place so the views stay
//! consistent.

use ratatui::style::{Color, Modifier, Style};

pub const ACCENT: Color = Color::Cyan;
pub const INCOME: Color = Color::Green;
pub const EXPENSE: Color = Color::Red;
pub const MUTED: Color = Color::DarkGray;
pub const WARN: Color = Color::Yellow;
pub const HEADER_BG: Color = Color::Blue;

pub fn title() -> Style {
    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
}

pub fn label() -> Style {
    Style::default().fg(Color::Gray)
}

pub fn value() -> Style {
    Style::default().fg(Color::White)
}

pub fn strong() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

pub fn muted() -> Style {
    Style::default().fg(MUTED)
}

pub fn table_header() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Gray)
        .add_modifier(Modifier::BOLD)
}

pub fn selected_row() -> Style {
    Style::default()
        .bg(Color::Rgb(38, 60, 84))
        .add_modifier(Modifier::BOLD)
}

/// Colour for an amount, by direction.
pub fn amount(kind: crate::db::Kind) -> Style {
    match kind {
        crate::db::Kind::Income => Style::default().fg(INCOME),
        crate::db::Kind::Expense => Style::default().fg(EXPENSE),
    }
}

/// Colour a net/remaining figure: green when in the black, red in the red.
pub fn signed(cents: i64) -> Style {
    if cents < 0 {
        Style::default().fg(EXPENSE)
    } else if cents > 0 {
        Style::default().fg(INCOME)
    } else {
        Style::default().fg(Color::White)
    }
}

/// Colour a budget by how much of it is used.
pub fn budget_state(spent: i64, total: i64) -> Style {
    if total > 0 && spent > total {
        Style::default().fg(EXPENSE).add_modifier(Modifier::BOLD)
    } else if total > 0 && spent * 10 >= total * 9 {
        Style::default().fg(WARN)
    } else {
        Style::default().fg(INCOME)
    }
}

/// Truncate to `width` display columns, appending `…` when text is cut. Widths
/// are measured in columns so CJK text does not overflow its cell.
pub fn fit(text: &str, width: usize) -> String {
    use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width <= 1 {
        return "…".to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for c in text.chars() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if used + w > width - 1 {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

/// A horizontal bar of `width` cells filled to `ratio`, used for budget gauges.
pub fn bar(ratio: f64, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let filled = ((ratio.clamp(0.0, 1.0)) * width as f64).round() as usize;
    let filled = filled.min(width);
    format!("{}{}", "█".repeat(filled), "░".repeat(width - filled))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_leaves_short_text_alone() {
        assert_eq!(fit("dining", 10), "dining");
        assert_eq!(fit("dining", 6), "dining");
    }

    #[test]
    fn fit_truncates_with_ellipsis() {
        assert_eq!(fit("transportation", 6), "trans…");
        assert_eq!(fit("abc", 1), "…");
    }

    #[test]
    fn fit_respects_display_width_of_wide_glyphs() {
        use unicode_width::UnicodeWidthStr;
        // Each glyph is two columns, so only two fit in five columns plus '…'.
        let out = fit("English-only text", 5);
        assert!(UnicodeWidthStr::width(out.as_str()) <= 5, "got {out:?}");
        assert!(out.ends_with('…'));
    }

    #[test]
    fn bar_fills_proportionally() {
        assert_eq!(bar(0.0, 4), "░░░░");
        assert_eq!(bar(1.0, 4), "████");
        assert_eq!(bar(0.5, 4), "██░░");
        assert_eq!(bar(2.0, 4), "████", "over-full clamps");
        assert_eq!(bar(-1.0, 4), "░░░░", "negative clamps");
        assert_eq!(bar(0.5, 0), "");
    }
}
