//! The footer: prompts, messages, and the key hint bar.
//!
//! Its height is measured *before* the panes are laid out, and the panes are given the remaining
//! space. That is the mechanism that keeps prompts and messages from ever covering the board or
//! the card: they occupy their own rows instead of floating above content.

use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::app::{App, MessageKind, Mode};
use crate::keymap::hint_line;
use crate::ui::text::wrap;

/// Rows the body editor gets when it is open.
const BODY_EDITOR_ROWS: u16 = 8;

/// Total footer height for the current state.
pub fn height(app: &App, area_width: u16) -> u16 {
    let width = area_width.max(1) as usize;
    let mut rows = 0u16;

    // Message, wrapped so a long error is fully readable rather than cut off.
    if let Some(message) = &app.message {
        rows += wrap(&message.text, width).len().min(3) as u16;
    }

    rows += match &app.mode {
        Mode::Normal => 0,
        // Prompt label and input line.
        Mode::Prompt { .. } => 2,
        Mode::Confirm { .. } => 1,
        Mode::BodyEditor { .. } => BODY_EDITOR_ROWS,
    };

    // The hint bar is always present, so the keys are never a secret.
    rows += hint_rows(app, width);
    rows.max(1)
}

/// How many rows the hint text needs.
fn hint_rows(app: &App, width: usize) -> u16 {
    wrap(&hint_line(app.mode.kind()), width).len().min(2) as u16
}

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 {
        return;
    }
    let width = area.width.max(1) as usize;

    let message_rows = app
        .message
        .as_ref()
        .map(|m| wrap(&m.text, width).len().min(3) as u16)
        .unwrap_or(0);
    let mode_rows = match &app.mode {
        Mode::Normal => 0,
        Mode::Prompt { .. } => 2,
        Mode::Confirm { .. } => 1,
        Mode::BodyEditor { .. } => BODY_EDITOR_ROWS,
    };
    let hint = hint_rows(app, width);

    let [message_area, mode_area, hint_area] = Layout::vertical([
        Constraint::Length(message_rows),
        Constraint::Length(mode_rows),
        Constraint::Length(hint),
    ])
    .areas(area);

    if let Some(message) = &app.message {
        let style = match message.kind {
            MessageKind::Info => Style::default().fg(Color::Cyan),
            MessageKind::Success => Style::default().fg(Color::Green),
            MessageKind::Error => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        };
        let prefix = match message.kind {
            MessageKind::Info => "i ",
            MessageKind::Success => "✓ ",
            MessageKind::Error => "! ",
        };
        let lines: Vec<Line> = wrap(&format!("{prefix}{}", message.text), width)
            .into_iter()
            .take(3)
            .map(|l| Line::from(Span::styled(l, style)))
            .collect();
        frame.render_widget(Paragraph::new(lines), message_area);
    }

    match &app.mode {
        Mode::Normal => {}
        Mode::Prompt { label, input, .. } => {
            let label_line = Line::from(Span::styled(
                format!("{label}:"),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ));
            let input_line = Line::from(vec![
                Span::styled("> ", Style::default().fg(Color::Yellow)),
                Span::styled(input.text().to_string(), Style::default().fg(Color::White)),
            ]);
            frame.render_widget(Paragraph::new(vec![label_line, input_line]), mode_area);
        }
        Mode::Confirm { question, .. } => {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        question.clone(),
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("  [y/N]", Style::default().fg(Color::Yellow)),
                ])),
                mode_area,
            );
        }
        Mode::BodyEditor { card_id, input, .. } => {
            let mut lines = vec![Line::from(Span::styled(
                format!("Editing the body of {card_id}.md - Ctrl+S saves, Esc discards"),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ))];
            // Show the tail of the text when it is longer than the editor area, so the caret
            // stays in view while typing.
            let text_rows = mode_area.height.saturating_sub(1) as usize;
            let all: Vec<String> = input.text().split('\n').map(str::to_string).collect();
            let (cursor_line, _) = input.cursor_line_and_column();
            let start = cursor_line.saturating_sub(text_rows.saturating_sub(1));
            for line in all.iter().skip(start).take(text_rows) {
                lines.push(Line::from(Span::styled(
                    line.clone(),
                    Style::default().fg(Color::White),
                )));
            }
            frame.render_widget(Paragraph::new(lines), mode_area);
        }
    }

    let hint_text = hint_line(app.mode.kind());
    let hint_lines: Vec<Line> = wrap(&hint_text, width)
        .into_iter()
        .take(2)
        .map(|l| Line::from(Span::styled(l, Style::default().fg(Color::DarkGray))))
        .collect();
    frame.render_widget(Paragraph::new(hint_lines), hint_area);
}

/// Where the caret sits in a single-line prompt.
///
/// Measured in display columns, so it stays correct after wide glyphs.
pub fn prompt_cursor(app: &App, area: Rect) -> Option<Position> {
    let Mode::Prompt { input, .. } = &app.mode else { return None };
    let footer_height = height(app, area.width);
    let footer_top = area.y + area.height.saturating_sub(footer_height);

    let width = area.width.max(1) as usize;
    let message_rows = app
        .message
        .as_ref()
        .map(|m| wrap(&m.text, width).len().min(3) as u16)
        .unwrap_or(0);

    // Row layout inside the footer: [message][label][input][hints].
    let input_row = footer_top + message_rows + 1;
    // "> " prefix is two columns wide.
    let x = area.x + 2 + input.cursor_display_width() as u16;
    Some(Position::new(x.min(area.x + area.width.saturating_sub(1)), input_row))
}

/// Where the caret sits in the multi-line body editor.
pub fn body_cursor(app: &App, area: Rect) -> Option<Position> {
    let Mode::BodyEditor { input, .. } = &app.mode else { return None };
    let footer_height = height(app, area.width);
    let footer_top = area.y + area.height.saturating_sub(footer_height);

    let width = area.width.max(1) as usize;
    let message_rows = app
        .message
        .as_ref()
        .map(|m| wrap(&m.text, width).len().min(3) as u16)
        .unwrap_or(0);

    let (cursor_line, cursor_column) = input.cursor_line_and_column();
    let text_rows = BODY_EDITOR_ROWS.saturating_sub(1) as usize;
    let start = cursor_line.saturating_sub(text_rows.saturating_sub(1));
    let visible_row = cursor_line - start;

    // Row layout: [message][editor header][editor text...][hints].
    let y = footer_top + message_rows + 1 + visible_row as u16;
    let x = area.x + cursor_column as u16;
    Some(Position::new(
        x.min(area.x + area.width.saturating_sub(1)),
        y.min(area.y + area.height.saturating_sub(1)),
    ))
}
