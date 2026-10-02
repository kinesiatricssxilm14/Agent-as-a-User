//! ratatui rendering.

use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::{App, Mode};
use crate::highlighter::{self, TokenKind};
use crate::help;

const CURSOR_STYLE: Style = Style::new().bg(Color::White).fg(Color::Black);
const BORDER_STYLE: Style = Style::new().fg(Color::DarkGray);

fn kind_style(kind: TokenKind) -> Style {
    match kind {
        TokenKind::Command => Style::default().fg(Color::LightGreen),
        TokenKind::Flag => Style::default().fg(Color::Yellow),
        TokenKind::String => Style::default().fg(Color::LightMagenta),
        TokenKind::Operator => Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD),
        TokenKind::Arg => Style::default().fg(Color::Gray),
        TokenKind::Number => Style::default().fg(Color::LightCyan),
        TokenKind::Variable => Style::default().fg(Color::LightCyan),
    }
}

/// Convert a highlighted byte-range list into styled spans.
fn highlight_spans(text: &str) -> Vec<Span<'static>> {
    let tokens = highlighter::highlight(text);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut last = 0usize;
    for t in tokens {
        if t.start > last {
            spans.push(Span::raw(text[last..t.start].to_string()));
        }
        spans.push(Span::styled(text[t.start..t.end].to_string(), kind_style(t.kind)));
        last = t.end;
    }
    if last < text.len() {
        spans.push(Span::raw(text[last..].to_string()));
    }
    spans
}

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    if app.help_open {
        draw_help(frame, area, app);
        return;
    }

    let seg_count = app.preview_segments().len().max(1);
    let preview_h = ((seg_count + 3) as u16).min(9);

    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(preview_h),
        Constraint::Length(1),
        Constraint::Length(2),
    ])
    .split(area);

    draw_title(frame, chunks[0], app);
    draw_output(frame, chunks[1], app);
    draw_preview(frame, chunks[2], app);
    draw_input(frame, chunks[3], app);
    draw_status(frame, chunks[4], app);
}

fn draw_title(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let left = " toolf — shell pipeline debugger ";
    let right = format!(" file: {} ", app.file.display());
    let lw = UnicodeWidthStr::width(left) as u16;
    let rw = UnicodeWidthStr::width(right.as_str()) as u16;
    let fill = area.width.saturating_sub(lw + rw) as usize;
    let line = Line::from(vec![
        Span::styled(left.to_string(), Style::default().bg(Color::DarkGray).fg(Color::Black).add_modifier(Modifier::BOLD)),
        Span::styled(" ".repeat(fill), Style::default().bg(Color::DarkGray)),
        Span::styled(right, Style::default().bg(Color::DarkGray).fg(Color::Black)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_output(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let block = Block::bordered().title(" Output ").border_style(BORDER_STYLE);
    let inner = block.inner(area);
    let lines: Vec<Line> = app.outputs.iter().map(|o| o.render()).collect();
    let max_scroll = lines.len().saturating_sub(inner.height as usize);
    let scroll = if app.follow {
        max_scroll
    } else {
        app.scroll.min(max_scroll)
    }
    .min(u16::MAX as usize);
    let p = Paragraph::new(lines).block(block).scroll((scroll as u16, 0));
    frame.render_widget(p, area);
}

fn draw_preview(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let segs = app.preview_segments();
    let cur = app.cursor_segment_index();
    let mut lines: Vec<Line> = vec![Line::from(Span::styled(
        "Alt+\\ runs the pipeline up to the ▶ segment",
        Style::default().fg(Color::DarkGray),
    ))];

    if segs.is_empty() {
        lines.push(Line::from(Span::styled(
            "(no pipeline segments yet)",
            Style::default().fg(Color::Gray),
        )));
    }

    for (i, seg) in segs.iter().enumerate() {
        let is_cur = Some(i) == cur;
        let mut spans = vec![
            Span::styled(
                if is_cur { "▶" } else { " " }.to_string(),
                if is_cur {
                    Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::DarkGray)
                },
            ),
            Span::styled(format!(" {}. ", i + 1), Style::default().fg(Color::DarkGray)),
        ];
        if seg.text.is_empty() {
            spans.push(Span::styled("(empty)", Style::default().fg(Color::Gray)));
        } else {
            spans.extend(highlight_spans(&seg.text));
        }
        lines.push(Line::from(spans));
    }

    let block = Block::bordered().title(" Pipeline preview ").border_style(BORDER_STYLE);
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn draw_input(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let (editor, label, title, highlight) = match app.mode {
        Mode::Normal => (&app.editor, "❯ ", " Command ", true),
        Mode::SavePrompt => (&app.save_editor, "save ❯ ", " Save output to file ", false),
    };

    let text = editor.to_string();
    let spans = if highlight {
        highlight_spans(&text)
    } else {
        vec![Span::raw(text.clone())]
    };

    // Expand spans into per-character cells (text, style, display width).
    let mut cells: Vec<(String, Style, u16)> = Vec::new();
    for span in &spans {
        for ch in span.content.chars() {
            let w = UnicodeWidthChar::width(ch).unwrap_or(0).max(1) as u16;
            cells.push((ch.to_string(), span.style, w));
        }
    }

    let cursor = editor.cursor();
    let cursor_col: u16 = cells[..cursor].iter().map(|c| c.2).sum();
    if cursor >= cells.len() {
        cells.push((" ".to_string(), CURSOR_STYLE, 1));
    } else {
        cells[cursor].1 = CURSOR_STYLE;
    }

    let label_w = UnicodeWidthStr::width(label) as u16;
    let avail = area.width.saturating_sub(2 + label_w).max(1);
    let scroll = cursor_col.saturating_sub(avail.saturating_sub(1));

    let mut out: Vec<Span> = vec![Span::styled(
        label.to_string(),
        Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD),
    )];
    let mut col: u16 = 0;
    for (s, st, w) in cells {
        if col + w > scroll && col < scroll + avail {
            out.push(Span::styled(s, st));
        }
        col += w;
        if col >= scroll + avail {
            break;
        }
    }

    let line = Line::from(out);
    let block = Block::bordered().title(title).border_style(BORDER_STYLE);
    frame.render_widget(Paragraph::new(line).block(block), area);
}

fn draw_status(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let keys = match app.mode {
        Mode::Normal => {
            "Enter Run   Alt+\\ Partial   Tab Complete   Ctrl+S Save   F1 Help   ↑/↓ History   PgUp/PgDn Scroll   Ctrl+C Quit"
        }
        Mode::SavePrompt => "Enter Save   Esc Cancel   Tab Complete path   Ctrl+C Quit",
    };
    let key_line = Line::from(Span::styled(
        keys.to_string(),
        Style::default().fg(Color::Cyan),
    ));
    let msg_line = Line::from(Span::styled(
        app.status.clone(),
        Style::default().fg(Color::White),
    ));
    frame.render_widget(Paragraph::new(vec![key_line, msg_line]), area);
}

fn draw_help(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let block = Block::bordered().title(" Help ").border_style(Style::default().fg(Color::Cyan));
    let lines: Vec<Line> = help::HELP_TEXT
        .lines()
        .map(|l| Line::from(Span::raw(l.to_string())))
        .collect();
    let p = Paragraph::new(lines)
        .block(block)
        .scroll((app.help_scroll as u16, 0));
    frame.render_widget(p, area);
}
