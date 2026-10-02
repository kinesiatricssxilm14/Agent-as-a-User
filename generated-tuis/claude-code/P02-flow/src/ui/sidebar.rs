//! The sidebar: every column, and the live configuration.
//!
//! This pane is the guarantee that no column is ever invisible. However narrow the terminal, and
//! however many columns the board has, they are all listed here with their display name, id and
//! card count. The configuration block below it shows the board root, where that value came
//! from, the configuration file path, and every key the file defines.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};
use ratatui::Frame;

use crate::app::{App, Mode, PromptKind};
use crate::ui::text::{clamp_scroll, truncate, truncate_start};
use crate::ui::title_style;

/// Height this pane would like: enough for every column plus the configuration block, but never
/// more than half the available space, so the card pane keeps room.
pub fn preferred_height(app: &App, available: u16) -> u16 {
    let columns = app.board.columns.len() as u16;
    let config_rows = 4 + app.config.values.len().min(6) as u16;
    let wanted = columns + config_rows + 3;
    wanted.min(available.saturating_sub(6)).max(6).min(available)
}

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    // While choosing a move target, this pane doubles as the candidate list: the prompt itself
    // stays a single line, and the full set of choices is visible here rather than in a popup.
    let choosing = matches!(
        &app.mode,
        Mode::Prompt { kind: PromptKind::MoveCard | PromptKind::GotoColumn, .. }
    );
    let typed = match &app.mode {
        Mode::Prompt { kind: PromptKind::MoveCard | PromptKind::GotoColumn, input, .. } => {
            input.text().trim().to_lowercase()
        }
        _ => String::new(),
    };

    let title = if choosing { " COLUMNS (pick one) " } else { " COLUMNS / CONFIG " };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(if choosing {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        })
        .title(Span::styled(title, title_style(choosing)));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let width = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();

    for (index, column) in app.board.columns.iter().enumerate() {
        let is_current = index == app.selected_column;
        let matches_typed =
            !typed.is_empty() && column.display_name.to_lowercase().starts_with(&typed);

        let shown = app.filtered_indices(index).len();
        let count = if app.search.is_empty() {
            format!("{}", column.cards.len())
        } else {
            format!("{shown}/{}", column.cards.len())
        };

        // Name, id and count together: three fields on one line, all visible at once.
        let marker = if is_current { "▶ " } else { "  " };
        let text = format!("{marker}{}  [{}]  {}", column.display_name, column.id, count);
        let style = if choosing && matches_typed {
            Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else if is_current {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        lines.push(Line::from(Span::styled(truncate(&text, width), style)));
    }

    if app.board.columns.is_empty() {
        lines.push(Line::from(Span::styled(
            "no columns - press 'c'",
            Style::default().fg(Color::DarkGray),
        )));
    }

    // Configuration block.
    lines.push(Line::from(Span::styled("─".repeat(width), Style::default().fg(Color::DarkGray))));
    lines.push(config_line("root", &app.store.root.display().to_string(), width));
    lines.push(config_line("source", app.root_source.label(), width));
    lines.push(config_line(
        "config",
        &format!(
            "{}{}",
            app.config.path.display(),
            if app.config.exists { "" } else { " (not created yet)" }
        ),
        width,
    ));

    if app.config.values.is_empty() {
        lines.push(Line::from(Span::styled(
            truncate("  (no keys set - press 'C')", width),
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        for (key, value) in &app.config.values {
            lines.push(Line::from(vec![Span::styled(
                truncate(&format!("  {key} = {value}"), width),
                Style::default().fg(Color::Gray),
            )]));
        }
    }

    // Warnings from the last load, so problems are not silent.
    for warning in app.board.warnings.iter().take(3) {
        lines.push(Line::from(Span::styled(
            truncate(&format!("! {warning}"), width),
            Style::default().fg(Color::Yellow),
        )));
    }

    // Keep the selected column visible when the list is taller than the pane.
    let total = lines.len();
    let height = inner.height;
    let offset = if total > height as usize && app.selected_column >= height as usize {
        clamp_scroll((app.selected_column as u16 + 1).saturating_sub(height), total, height)
    } else {
        0
    };

    let visible: Vec<Line> = lines.into_iter().skip(offset as usize).collect();
    frame.render_widget(Paragraph::new(visible), inner);

    if total > height as usize {
        let track = Rect {
            x: area.x + area.width.saturating_sub(1),
            y: inner.y,
            width: 1,
            height: inner.height,
        };
        let mut state = ScrollbarState::new(total).position(offset as usize);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight).begin_symbol(None).end_symbol(None),
            track,
            &mut state,
        );
    }
}

/// A `label value` row. Values are truncated from the left, so the end of a long path - the part
/// that identifies it - stays readable.
fn config_line<'a>(label: &'a str, value: &str, width: usize) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{label} "), Style::default().fg(Color::DarkGray)),
        Span::styled(
            truncate_start(value, width.saturating_sub(label.len() + 1)),
            Style::default().fg(Color::White),
        ),
    ])
}
