//! Colour palette.  Every highlight in `toole` uses a *background* colour so
//! that matched text stays distinguishable from body text even on terminals
//! with unusual foreground themes.

use ratatui::style::{Color, Modifier, Style};

/// Frame / border colour of an unfocused panel.
pub const BORDER: Color = Color::Rgb(88, 96, 112);
/// Frame colour of the focused panel.
pub const BORDER_FOCUS: Color = Color::Rgb(122, 200, 255);
/// Panel title colour.
pub const TITLE: Color = Color::Rgb(150, 210, 255);
/// Ordinary body text.
pub const TEXT: Color = Color::Rgb(214, 220, 232);
/// Secondary / explanatory text.
pub const DIM: Color = Color::Rgb(132, 142, 160);
/// Line-number gutter.
pub const GUTTER: Color = Color::Rgb(104, 114, 132);

/// Background of a normal match.
pub const MATCH_BG: Color = Color::Rgb(180, 142, 0);
/// Foreground of a normal match.
pub const MATCH_FG: Color = Color::Rgb(20, 20, 24);
/// Background of the currently selected match.
pub const SEL_MATCH_BG: Color = Color::Rgb(255, 208, 64);
/// Background of text produced by the replacement template.
pub const REPL_BG: Color = Color::Rgb(28, 132, 84);
/// Foreground of replaced text.
pub const REPL_FG: Color = Color::Rgb(236, 255, 244);
/// Background of the selected replacement span.
pub const SEL_REPL_BG: Color = Color::Rgb(52, 200, 124);
/// Background used to mark a zero-width match position.
pub const EMPTY_BG: Color = Color::Rgb(150, 100, 190);

/// Selected row in the match list.
pub const ROW_SEL_BG: Color = Color::Rgb(44, 62, 92);
/// Header row of the match list.
pub const HEAD_BG: Color = Color::Rgb(34, 44, 62);

/// Success / affirmative.
pub const OK: Color = Color::Rgb(126, 222, 148);
/// Warning.
pub const WARN: Color = Color::Rgb(240, 190, 90);
/// Error.
pub const ERR: Color = Color::Rgb(255, 118, 118);
/// Background of the error banner.
pub const ERR_BG: Color = Color::Rgb(92, 28, 34);

/// Background of an input field.
pub const INPUT_BG: Color = Color::Rgb(26, 32, 44);
/// Background of a focused input field.
pub const INPUT_BG_FOCUS: Color = Color::Rgb(34, 44, 62);
/// Background of the top header bar.
pub const HEADER_BG: Color = Color::Rgb(30, 40, 58);
/// Background of the bottom key bar.
pub const FOOTER_BG: Color = Color::Rgb(24, 30, 42);
/// Background of the "selected match details" strip.
pub const DETAIL_BG: Color = Color::Rgb(22, 28, 40);
/// Colour of an active flag chip.
pub const FLAG_ON: Color = Color::Rgb(126, 222, 148);
/// Colour of an inactive flag chip.
pub const FLAG_OFF: Color = Color::Rgb(96, 104, 120);

/// Body text style.
pub fn text() -> Style {
    Style::default().fg(TEXT)
}

/// Dim / secondary style.
pub fn dim() -> Style {
    Style::default().fg(DIM)
}

/// Key-name style used in the footer and help pane.
pub fn key() -> Style {
    Style::default()
        .fg(Color::Rgb(255, 224, 130))
        .add_modifier(Modifier::BOLD)
}

/// Style for a matched fragment in the source pane.
pub fn match_style(selected: bool) -> Style {
    Style::default()
        .fg(MATCH_FG)
        .bg(if selected { SEL_MATCH_BG } else { MATCH_BG })
        .add_modifier(Modifier::BOLD)
}

/// Style for a replaced fragment in the preview pane.
pub fn repl_style(selected: bool) -> Style {
    Style::default()
        .fg(REPL_FG)
        .bg(if selected { SEL_REPL_BG } else { REPL_BG })
        .add_modifier(Modifier::BOLD)
}
