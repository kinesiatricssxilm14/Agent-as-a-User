//! The help pane.
//!
//! Rendered as a pane beside the board, not as a full-screen page or a popup: the board and the
//! selected card stay visible while it is open, so reading the key list never hides the data.
//! Content comes from the same binding table that dispatches keys, so it cannot go stale.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::keymap::{bindings_for, ModeKind};
use crate::ui::text::truncate;

pub fn render(frame: &mut Frame, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(Span::styled(
            " HELP (? closes) ",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let width = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    let mut current_group = "";

    for binding in bindings_for(ModeKind::Normal) {
        if binding.group != current_group {
            if !lines.is_empty() {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                binding.group.to_string(),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )));
            current_group = binding.group;
        }

        // Key column then description, padded so the two align and scan easily.
        let keys = format!("{:<11}", binding.keys);
        let help_width = width.saturating_sub(keys.len() + 1);
        let help = truncate(binding.help, help_width);
        lines.push(Line::from(vec![
            Span::styled(keys, Style::default().fg(Color::Cyan)),
            Span::styled(help, Style::default().fg(Color::Gray)),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        truncate(
            "Board files: board.txt lists columns; cols/<id>/order.txt holds card order; cards are cols/<id>/<card-id>.md with '# title' on line 1.",
            width * 3,
        ),
        Style::default().fg(Color::DarkGray),
    )));

    // Scroll to keep the tail reachable in short panes; the pane never hides the board.
    let visible_height = inner.height as usize;
    let start = lines.len().saturating_sub(visible_height).min(lines.len());
    let shown: Vec<Line> = if lines.len() > visible_height {
        // Show from the top and let the user shrink the terminal less; truncation is preferable
        // to hiding the first groups, which are the most used.
        lines.into_iter().take(visible_height).collect()
    } else {
        let _ = start;
        lines
    };

    frame.render_widget(Paragraph::new(shown), inner);
}
