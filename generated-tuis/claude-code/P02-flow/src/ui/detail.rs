//! The card pane: the selected card's id, full title, file path and complete body.
//!
//! This pane updates as soon as the selection moves, without needing Enter, because the
//! specification requires that a selected card's title and full body be visible on the same
//! screen. Enter only hands ↑/↓ to this pane for scrolling long bodies.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::Frame;

use crate::app::{App, Focus};
use crate::ui::text::{clamp_scroll, wrap};
use crate::ui::title_style;

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Detail;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        })
        .title(Span::styled(
            if focused { " CARD (↑↓ scrolls) " } else { " CARD " },
            title_style(focused),
        ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let Some(card) = app.current_card() else {
        let hint = if app.board.columns.is_empty() {
            "No columns yet.\n\nPress 'c' to create one."
        } else if app.visible_card_count() == 0 && !app.search.is_empty() {
            "No card matches the current filter.\n\nPress Esc to clear it."
        } else {
            "No card selected.\n\nPress 'n' to create one."
        };
        frame.render_widget(Paragraph::new(hint).style(Style::default().fg(Color::DarkGray)), inner);
        return;
    };

    let column = &app.board.columns[app.selected_column];
    let width = inner.width as usize;

    // Header block: id, full title (wrapped, never truncated), column and absolute path.
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("id ", Style::default().fg(Color::DarkGray)),
        Span::styled(card.id.clone(), Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
    ]));

    // The full title, wrapped rather than cut, since it must be readable in full here.
    let title_label = "title ";
    let title_indent = " ".repeat(title_label.len());
    let title_text = if card.title.trim().is_empty() {
        "(untitled)".to_string()
    } else {
        card.title.clone()
    };
    let title_lines = wrap(&title_text, width.saturating_sub(title_label.len()).max(1));
    for (index, line) in title_lines.iter().enumerate() {
        let prefix = if index == 0 { title_label } else { &title_indent };
        lines.push(Line::from(vec![
            Span::styled(prefix.to_string(), Style::default().fg(Color::DarkGray)),
            Span::styled(
                line.clone(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("column ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} ({})", column.display_name, column.id),
            Style::default().fg(Color::Gray),
        ),
    ]));

    // The real path on disk: evidence that this reflects the filesystem, not an internal model.
    let path = card.path(&app.board.root, &column.id);
    let path_label = "file ";
    let path_indent = " ".repeat(path_label.len());
    let path_lines = wrap(
        &path.display().to_string(),
        width.saturating_sub(path_label.len()).max(1),
    );
    for (index, line) in path_lines.iter().enumerate() {
        let prefix = if index == 0 { path_label } else { &path_indent };
        lines.push(Line::from(vec![
            Span::styled(prefix.to_string(), Style::default().fg(Color::DarkGray)),
            Span::styled(line.clone(), Style::default().fg(Color::DarkGray)),
        ]));
    }

    if card.missing_header {
        lines.push(Line::from(Span::styled(
            "note: no '# ' title line in this file",
            Style::default().fg(Color::Yellow),
        )));
    }

    lines.push(Line::from(Span::styled(
        "─".repeat(width),
        Style::default().fg(Color::DarkGray),
    )));

    let header_height = lines.len();

    // The body, wrapped to the pane. Wrapping here (rather than with Paragraph::wrap) is what
    // makes the line count exact, and therefore the scroll bound exact.
    let body_lines: Vec<String> = if card.body.is_empty() {
        vec!["(no description yet - press 'a' to append a line, or 'B' to edit)".to_string()]
    } else {
        wrap(&card.body, width)
    };
    let body_is_placeholder = card.body.is_empty();

    let body_height = inner.height.saturating_sub(header_height as u16);
    // Clamp before rendering: an over-scrolled Paragraph draws nothing, which looks like the
    // body was lost.
    let scroll = clamp_scroll(app.detail_scroll, body_lines.len(), body_height);
    app.detail_scroll = scroll;

    for line in body_lines.iter().skip(scroll as usize).take(body_height.max(1) as usize) {
        let style = if body_is_placeholder {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(Span::styled(line.clone(), style)));
    }

    frame.render_widget(Paragraph::new(lines), inner);

    // Scrollbar for bodies longer than the pane, so it is obvious more text exists.
    if body_lines.len() > body_height as usize && body_height > 0 {
        let track = Rect {
            x: area.x + area.width.saturating_sub(1),
            y: inner.y + header_height as u16,
            width: 1,
            height: body_height,
        };
        let mut state = ScrollbarState::new(body_lines.len()).position(scroll as usize);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            track,
            &mut state,
        );
    }
}
