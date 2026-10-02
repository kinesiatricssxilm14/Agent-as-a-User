//! Rendering.
//!
//! The layout is computed so that every pane occupies a disjoint rectangle: prompts and messages
//! consume real rows and shrink the panes above them rather than being drawn on top. Nothing is
//! ever hidden behind an overlay, so a single screenshot shows the columns, the selected card's
//! full title and body, the column list, and the active key hints together.

mod board;
mod detail;
mod footer;
mod help;
mod sidebar;
pub mod text;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, Mode};

/// Below this the panes have no useful inner area, so we show a message instead of a broken UI.
const MIN_WIDTH: u16 = 40;
const MIN_HEIGHT: u16 = 10;

/// Width at which the board and card panes stop sitting side by side.
const NARROW_WIDTH: u16 = 76;

/// Draw the whole interface.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let notice = Paragraph::new(vec![
            Line::from("Terminal too small"),
            Line::from(format!("Need at least {MIN_WIDTH}x{MIN_HEIGHT}")),
        ])
        .style(Style::default().fg(Color::Yellow));
        frame.render_widget(notice, area);
        return;
    }

    // The footer's height is measured first, then subtracted: this is what keeps prompts from
    // covering the panes.
    let footer_height = footer::height(app, area.width);
    let [header_area, body_area, footer_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(footer_height),
    ])
    .areas(area);

    render_header(frame, app, header_area);

    // Sidebar and help share the right-hand column; help replaces the sidebar rather than
    // covering the board, so the columns and the selected card stay visible while reading it.
    let show_help = app.show_help;
    let narrow = area.width < NARROW_WIDTH;

    if narrow {
        // Not enough width for three columns: stack the panes vertically. Everything is still
        // on one screen, and each pane scrolls independently.
        let detail_height = (body_area.height / 2).max(3);
        let [top, bottom] =
            Layout::vertical([Constraint::Min(3), Constraint::Length(detail_height)])
                .areas(body_area);
        board::render(frame, app, top);
        if show_help {
            help::render(frame, bottom);
        } else {
            detail::render(frame, app, bottom);
        }
    } else {
        let right_width = right_pane_width(area.width);
        let [left, right] =
            Layout::horizontal([Constraint::Min(20), Constraint::Length(right_width)])
                .areas(body_area);

        board::render(frame, app, left);

        if show_help {
            // Card detail keeps the top half so the selected card is never hidden by help.
            let [detail_area, help_area] =
                Layout::vertical([Constraint::Percentage(50), Constraint::Min(5)]).areas(right);
            detail::render(frame, app, detail_area);
            help::render(frame, help_area);
        } else {
            let sidebar_height = sidebar::preferred_height(app, right.height);
            let [detail_area, sidebar_area] =
                Layout::vertical([Constraint::Min(4), Constraint::Length(sidebar_height)])
                    .areas(right);
            detail::render(frame, app, detail_area);
            sidebar::render(frame, app, sidebar_area);
        }
    }

    footer::render(frame, app, footer_area);

    // The caret belongs in whichever input is active.
    place_cursor(frame, app, area);
}

/// Width of the right-hand column: roughly a third, bounded so neither side is unusable.
fn right_pane_width(total: u16) -> u16 {
    (total / 3).clamp(28, 44).min(total.saturating_sub(20))
}

/// One-line summary of what is open and how big it is.
fn render_header(frame: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled("toolb", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("  "),
        Span::styled(app.store.root.display().to_string(), Style::default().fg(Color::White)),
    ];

    spans.push(Span::styled(
        format!("  columns:{}", app.board.columns.len()),
        Style::default().fg(Color::DarkGray),
    ));
    spans.push(Span::styled(
        format!("  cards:{}", app.board.total_cards()),
        Style::default().fg(Color::DarkGray),
    ));

    if !app.search.is_empty() {
        let matched: usize =
            (0..app.board.columns.len()).map(|i| app.filtered_indices(i).len()).sum();
        spans.push(Span::styled(
            format!("  filter:'{}' ({matched})", app.search),
            Style::default().fg(Color::Yellow),
        ));
    }
    if !app.store.is_initialised() {
        spans.push(Span::styled(
            "  [no board.txt]",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Put the terminal cursor where the user is typing, measured in display columns so it lands
/// correctly after wide glyphs.
fn place_cursor(frame: &mut Frame, app: &App, area: Rect) {
    match &app.mode {
        Mode::Prompt { .. } => {
            if let Some(position) = footer::prompt_cursor(app, area) {
                frame.set_cursor_position(position);
            }
        }
        Mode::BodyEditor { .. } => {
            if let Some(position) = footer::body_cursor(app, area) {
                frame.set_cursor_position(position);
            }
        }
        _ => {}
    }
}

/// Style for a selected row: reverse video so it is visible on any colour scheme, including
/// terminals with a light background.
pub fn selected_style() -> Style {
    Style::default().add_modifier(Modifier::REVERSED).add_modifier(Modifier::BOLD)
}

/// Style for a pane title.
pub fn title_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn right_pane_leaves_room_for_the_board() {
        for total in [MIN_WIDTH, 80, 100, 120, 200, 400] {
            let right = right_pane_width(total);
            assert!(right < total, "right pane {right} must fit in {total}");
            assert!(total - right >= 20 || total < 48, "board pane too narrow at {total}");
        }
    }

    #[test]
    fn right_pane_width_is_bounded() {
        assert_eq!(right_pane_width(120), 40);
        assert_eq!(right_pane_width(300), 44, "capped");
        assert!(right_pane_width(80) >= 28);
    }
}
