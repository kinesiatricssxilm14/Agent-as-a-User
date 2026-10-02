//! The board pane: every column side by side, each listing its cards.
//!
//! Columns shrink to a minimum width so that as many as possible fit on screen at once. Only
//! when even that is not enough does the pane scroll horizontally, one column at a time, with a
//! scrollbar and an explicit count of what lies outside the window. The sidebar always lists
//! every column regardless, so no column is ever completely invisible.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::Frame;

use crate::app::{App, Focus};
use crate::ui::text::{truncate, width as display_width};
use crate::ui::{selected_style, title_style};

/// Narrowest a column may become before the pane scrolls instead.
const MIN_COLUMN_WIDTH: u16 = 16;
/// Widest a column grows when there is space to spare.
const MAX_COLUMN_WIDTH: u16 = 34;

/// How many columns fit, and how wide each should be.
///
/// Returns `(visible_count, column_width)`.
fn layout_columns(available: u16, column_count: usize) -> (usize, u16) {
    if column_count == 0 || available == 0 {
        return (0, MIN_COLUMN_WIDTH);
    }
    let ideal = available / column_count as u16;
    if ideal >= MIN_COLUMN_WIDTH {
        // Everything fits; share the space out, capped so a lone column is not absurdly wide.
        return (column_count, ideal.clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH));
    }
    // Too many columns for the width: show as many as fit at the minimum width.
    let fits = (available / MIN_COLUMN_WIDTH).max(1) as usize;
    (fits.min(column_count), MIN_COLUMN_WIDTH)
}

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Board;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if focused {
            Style::default().fg(Color::Cyan)
        } else {
            Style::default().fg(Color::DarkGray)
        })
        .title(Span::styled(" BOARD ", title_style(focused)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    if app.board.columns.is_empty() {
        let hint = if app.store.is_initialised() {
            "This board has no columns yet.\n\nPress 'c' to create one, or '?' for help."
        } else {
            "No board.txt in this directory.\n\nPress 'I' to create a board with the default\ncolumns, 'c' to add a single column,\nor '?' for help."
        };
        frame.render_widget(
            Paragraph::new(hint).style(Style::default().fg(Color::Yellow)),
            inner,
        );
        return;
    }

    let (visible, column_width) = layout_columns(inner.width, app.board.columns.len());
    if visible == 0 {
        return;
    }

    // Keep the selected column inside the visible window.
    if app.selected_column < app.column_offset {
        app.column_offset = app.selected_column;
    } else if app.selected_column >= app.column_offset + visible {
        app.column_offset = app.selected_column + 1 - visible;
    }
    let max_offset = app.board.columns.len().saturating_sub(visible);
    app.column_offset = app.column_offset.min(max_offset);

    let scrolls_horizontally = app.board.columns.len() > visible;
    // Reserve the bottom row for the horizontal scrollbar and the "more columns" note.
    let (columns_area, note_area) = if scrolls_horizontally && inner.height > 2 {
        let [top, bottom] =
            Layout::vertical([Constraint::Min(2), Constraint::Length(1)]).areas(inner);
        (top, Some(bottom))
    } else {
        (inner, None)
    };

    let constraints: Vec<Constraint> =
        (0..visible).map(|_| Constraint::Length(column_width)).collect();
    let slots = Layout::horizontal(constraints).split(columns_area);

    for (slot_index, slot) in slots.iter().enumerate() {
        let column_index = app.column_offset + slot_index;
        if column_index >= app.board.columns.len() {
            break;
        }
        render_column(frame, app, column_index, *slot);
    }

    if let Some(note_area) = note_area {
        render_overflow_note(frame, app, visible, note_area);
    }
}

/// One column: a heading, then its cards.
fn render_column(frame: &mut Frame, app: &mut App, column_index: usize, area: Rect) {
    let is_selected_column = column_index == app.selected_column;
    let column = &app.board.columns[column_index];
    let ambiguous = app.board.is_display_name_ambiguous(&column.display_name);

    let indices = app.filtered_indices(column_index);
    let inner_width = area.width.saturating_sub(2) as usize;

    // Heading: display name, id, and how many cards are shown.
    let count_label = if app.search.is_empty() {
        format!("{}", column.cards.len())
    } else {
        format!("{}/{}", indices.len(), column.cards.len())
    };
    let heading = format!("{} ({})", column.label(ambiguous), column.id);
    let heading_style = if is_selected_column {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray).add_modifier(Modifier::BOLD)
    };

    let mut lines = vec![
        Line::from(vec![Span::styled(truncate(&heading, inner_width), heading_style)]),
        Line::from(vec![Span::styled(
            truncate(&format!("{count_label} card(s)"), inner_width),
            Style::default().fg(Color::DarkGray),
        )]),
    ];

    // Rows available for cards, after the two heading lines.
    let card_rows = area.height.saturating_sub(2) as usize;

    // Scroll the card list so the selection stays visible.
    let scroll = {
        let offset = app.card_scroll.get(column_index).copied().unwrap_or(0);
        let mut offset = offset.min(indices.len().saturating_sub(1));
        if is_selected_column && card_rows > 0 {
            if app.selected_card < offset {
                offset = app.selected_card;
            } else if app.selected_card >= offset + card_rows {
                offset = app.selected_card + 1 - card_rows;
            }
        }
        let max = indices.len().saturating_sub(card_rows);
        offset = offset.min(max);
        if let Some(slot) = app.card_scroll.get_mut(column_index) {
            *slot = offset;
        }
        offset
    };

    if indices.is_empty() {
        let note = if column.cards.is_empty() { "(empty)" } else { "(no matches)" };
        lines.push(Line::from(Span::styled(note, Style::default().fg(Color::DarkGray))));
    }

    for (position, &real_index) in indices.iter().enumerate().skip(scroll).take(card_rows) {
        let card = &column.cards[real_index];
        let is_selected = is_selected_column && position == app.selected_card;

        // Both the title and the id are shown, because the id is the card's file name and is what
        // every operation refers to. The id is reserved space and the title absorbs the
        // truncation, so a narrow column never leaves a card unidentifiable.
        let id_suffix = format!(" [{}]", card.id);
        let id_width = display_width(&id_suffix);
        let text = if id_width + 4 <= inner_width {
            let title = truncate(card.display_title(), inner_width - id_width);
            format!("{title}{id_suffix}")
        } else {
            // Too narrow for both: the id alone is more useful than a clipped title.
            truncate(card.id.as_str(), inner_width)
        };

        let style = if is_selected {
            selected_style()
        } else if card.title.trim().is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(Span::styled(text, style)));
    }

    let hidden_below = indices.len().saturating_sub(scroll + card_rows);
    if hidden_below > 0 && card_rows > 0 {
        // Replace the last rendered row with the indicator, so nothing is silently cut off.
        lines.pop();
        lines.push(Line::from(Span::styled(
            truncate(&format!("↓ {hidden_below} more"), inner_width),
            Style::default().fg(Color::Yellow),
        )));
    }
    if scroll > 0 {
        lines[2] = Line::from(Span::styled(
            truncate(&format!("↑ {scroll} above"), inner_width),
            Style::default().fg(Color::Yellow),
        ));
    }

    let border_style = if is_selected_column {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default().borders(Borders::LEFT).border_style(border_style);
    frame.render_widget(Paragraph::new(lines).block(block), area);

    // A vertical scrollbar when the column has more cards than rows.
    if indices.len() > card_rows && card_rows > 0 && area.height > 2 {
        let track = Rect {
            x: area.x + area.width.saturating_sub(1),
            y: area.y + 2,
            width: 1,
            height: area.height - 2,
        };
        let mut state = ScrollbarState::new(indices.len()).position(scroll);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight).begin_symbol(None).end_symbol(None),
            track,
            &mut state,
        );
    }
}

/// Report columns outside the window, and show where the window sits.
fn render_overflow_note(frame: &mut Frame, app: &App, visible: usize, area: Rect) {
    let total = app.board.columns.len();
    let before = app.column_offset;
    let after = total.saturating_sub(app.column_offset + visible);

    let mut parts = Vec::new();
    if before > 0 {
        parts.push(format!("← {before} column(s)"));
    }
    if after > 0 {
        parts.push(format!("{after} column(s) →"));
    }
    let note = format!(
        "{}  (all {total} listed on the right)",
        parts.join("  ")
    );

    let [text_area, bar_area] =
        Layout::horizontal([Constraint::Min(10), Constraint::Length(8)]).areas(area);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            truncate(&note, text_area.width as usize),
            Style::default().fg(Color::Yellow),
        ))),
        text_area,
    );

    let mut state = ScrollbarState::new(total).position(app.column_offset);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::HorizontalBottom),
        bar_area,
        &mut state,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_columns_fit_when_there_is_room() {
        let (visible, width) = layout_columns(120, 3);
        assert_eq!(visible, 3);
        assert!(width >= MIN_COLUMN_WIDTH);
        assert!(width * 3 <= 120 + MIN_COLUMN_WIDTH);
    }

    #[test]
    fn columns_shrink_to_the_floor_before_scrolling() {
        // Six columns in 100 cells: 16 each fits all six rather than paginating.
        let (visible, width) = layout_columns(100, 6);
        assert_eq!(visible, 6, "shrinking must be preferred over hiding");
        assert_eq!(width, MIN_COLUMN_WIDTH);
    }

    #[test]
    fn very_many_columns_scroll_only_as_a_last_resort() {
        let (visible, width) = layout_columns(60, 12);
        assert_eq!(width, MIN_COLUMN_WIDTH);
        assert_eq!(visible, 3, "60 / 16 = 3 columns at the floor");
        assert!(visible < 12);
    }

    #[test]
    fn a_single_column_does_not_stretch_absurdly() {
        let (visible, width) = layout_columns(200, 1);
        assert_eq!(visible, 1);
        assert_eq!(width, MAX_COLUMN_WIDTH);
    }

    #[test]
    fn degenerate_inputs_do_not_panic_or_divide_by_zero() {
        assert_eq!(layout_columns(0, 3).0, 0);
        assert_eq!(layout_columns(100, 0).0, 0);
        // Narrower than one column still yields one, rather than zero.
        assert_eq!(layout_columns(8, 4).0, 1);
    }
}
