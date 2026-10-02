//! Rendering for the toole TUI: header, input panel, match list, preview pane,
//! help bar, and the help/preset overlays.

use crate::app::{App, Focus, InputField, SpanKind, PRESETS};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

const ACCENT: Color = Color::Cyan;
const ACCENT2: Color = Color::Magenta;
const ACCENT3: Color = Color::Yellow;

const DIM: Style = Style::new().fg(Color::DarkGray);
const BOLD: Modifier = Modifier::BOLD;
const HIGHLIGHT: Style = Style::new().bg(Color::Yellow).fg(Color::Black);
const REPL_HIGHLIGHT: Style = Style::new().bg(Color::Green).fg(Color::Black);
const CURSOR: Style = Style::new().add_modifier(Modifier::REVERSED);

const HELP_TEXT: &str = "\
 toole — keyboard reference
────────────────────────────────────────────
 Editing
  type               insert characters into the focused input
  ← →                move cursor
  Backspace / Delete delete character
  Home / End         jump to start / end
  Ctrl+A / Ctrl+E    jump to start / end
  Ctrl+U             clear the input
  Ctrl+K             delete to end of line
  Esc                clear the focused input
  Enter              leave input, focus matches
 Navigation & focus
  Tab / Shift+Tab    cycle focus (pattern→replace→matches→preview)
  /                  focus pattern input
  r                  focus replacement input
  i                  toggle case-insensitive matching (?i)
  ↑ ↓                move selection (matches) or scroll (preview)
  PgUp / PgDn        page through matches / preview
  Home / End         jump to first / last
  Enter (matches)    jump preview to selected match
 Extras
  F2                 open preset menu (China phone numbers, email, …)
  F1 / ? / h         show this help
  Ctrl+R             reload the input file
  q / Ctrl+C         quit
────────────────────────────────────────────
 Matches are highlighted with a yellow background;
 their [start..end] character offsets are 0-indexed.
 The preview pane shows the full file content with
 replacements applied (green background).";

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let chunks = Layout::vertical([
        Constraint::Length(1), // header
        Constraint::Length(4), // inputs + status
        Constraint::Min(0),    // matches + preview
        Constraint::Length(2), // help bar
    ])
    .split(area);

    draw_header(f, chunks[0], app);
    draw_input_panel(f, chunks[1], app);

    let main = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[2]);
    draw_matches(f, main[0], app);
    draw_preview(f, main[1], app);

    draw_help_bar(f, chunks[3], app);

    if app.show_help {
        draw_overlay(f, area, " Help ", HELP_TEXT, Color::Cyan);
    }
    if app.show_presets {
        draw_presets_overlay(f, area, app);
    }
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let flags = if app.case_insensitive { "ignore-case" } else { "case-sensitive" };
    let title = format!(
        " toole {}  │  file: {}  │  matches: {}  │  {}  ",
        env!("CARGO_PKG_VERSION"),
        app.file_path,
        app.matches.len(),
        flags
    );
    let line = Line::from(Span::styled(
        title,
        Style::default()
            .fg(Color::Black)
            .bg(ACCENT)
            .add_modifier(BOLD),
    ));
    f.render_widget(Paragraph::new(line), area);
}

fn draw_input_panel(f: &mut Frame, area: Rect, app: &App) {
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    draw_field_line(
        f,
        rows[0],
        "Pattern",
        &app.pattern,
        app.focus == Focus::Pattern,
        ACCENT,
    );
    draw_field_line(
        f,
        rows[1],
        "Replace",
        &app.replacement,
        app.focus == Focus::Replacement,
        ACCENT2,
    );
    draw_flags_line(f, rows[2], app);
    draw_status_line(f, rows[3], app);
}

fn draw_field_line(
    f: &mut Frame,
    area: Rect,
    label: &str,
    field: &InputField,
    focused: bool,
    accent: Color,
) {
    let label_text = format!(" {:<8} ", label);
    let label_style = if focused {
        Style::default().fg(accent).add_modifier(BOLD)
    } else {
        DIM
    };
    let mut spans = vec![Span::styled(label_text, label_style)];

    let label_width = 10usize;
    let avail = (area.width as usize).saturating_sub(label_width);
    spans.extend(render_buffer(field, avail));

    let base = if focused {
        Style::default().bg(Color::DarkGray)
    } else {
        Style::default()
    };
    f.render_widget(Paragraph::new(Line::from(spans)).style(base), area);
}

/// Render the input buffer with horizontal auto-scroll and an in-text cursor.
fn render_buffer(field: &InputField, width: usize) -> Vec<Span<'static>> {
    if width == 0 {
        return vec![Span::raw("")];
    }
    let chars: Vec<char> = field.buf.chars().collect();
    let total = chars.len();
    let cursor_char = field.cursor_char_index();

    // Keep the cursor visible: when it passes the right edge, scroll left.
    let mut start = 0usize;
    if cursor_char >= width {
        start = cursor_char - width + 1;
    }
    let end = (start + width).min(total);

    let mut spans: Vec<Span<'static>> = Vec::with_capacity(end - start + 1);
    for (i, &ch) in chars.iter().enumerate().take(end).skip(start) {
        let style = if i == cursor_char { CURSOR } else { Style::default() };
        spans.push(Span::styled(ch.to_string(), style));
    }

    // Cursor past the last character: draw a visible insertion block.
    if cursor_char == total && cursor_char >= start && cursor_char - start < width {
        spans.push(Span::styled(" ", CURSOR));
    }
    spans
}

fn draw_flags_line(f: &mut Frame, area: Rect, app: &App) {
    let mut spans: Vec<Span> = Vec::new();
    spans.push(Span::styled(
        format!(" [i] {}", if app.case_insensitive { "ignore-case: ON" } else { "case-sensitive" }),
        Style::default().fg(ACCENT3),
    ));
    spans.push(Span::styled(
        "   [/] pattern  [r] replace  [F2] presets  [Ctrl+R] reload",
        DIM,
    ));
    if !app.matches.is_empty() {
        let m = &app.matches[app.selected];
        spans.push(Span::styled(
            format!("   selection {}/{} @[{}..{}]", app.selected + 1, app.matches.len(), m.start, m.end),
            Style::default().fg(ACCENT2),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_status_line(f: &mut Frame, area: Rect, app: &App) {
    let text = if let Some(e) = &app.error {
        format!(" ✖ regex error: {}", e)
    } else if let Some(e) = &app.file_error {
        format!(" ✖ file error: {}", e)
    } else if !app.status_msg.is_empty() {
        format!(" ℹ {}", app.status_msg)
    } else {
        " Type a regex to match in real time — Esc clears, Tab moves focus, arrows browse.".to_string()
    };

    let style = if app.error.is_some() || app.file_error.is_some() {
        Style::default().fg(Color::Red)
    } else if !app.status_msg.is_empty() {
        Style::default().fg(Color::Cyan)
    } else {
        DIM
    };
    f.render_widget(Paragraph::new(Span::styled(text, style)), area);
}

fn draw_matches(f: &mut Frame, area: Rect, app: &mut App) {
    let focused = app.focus == Focus::Results;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Matches ({}) — 0-indexed char offsets ", app.matches.len()))
        .border_style(if focused {
            Style::default().fg(ACCENT)
        } else {
            Style::default()
        });
    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.matches.is_empty() {
        let msg = if app.error.is_some() {
            "invalid regex (see error above)"
        } else if app.pattern.is_empty() {
            "enter a regex pattern to see matches here"
        } else {
            "no matches"
        };
        f.render_widget(
            Paragraph::new(Span::styled(msg, DIM)),
            inner,
        );
        return;
    }

    let height = inner.height as usize;
    app.results_scroll = clamp_scroll(app.results_scroll, app.selected, app.matches.len(), height);
    let start = app.results_scroll;
    let end = (start + height).min(app.matches.len());

    let mut lines: Vec<Line> = Vec::with_capacity(end - start);
    for i in start..end {
        let m = &app.matches[i];
        let marker = if i == app.selected {
            Span::styled("▶ ", Style::default().fg(ACCENT3))
        } else {
            Span::raw("  ")
        };
        let idx = Span::styled(format!("{:>4} ", i + 1), DIM);
        let offset = Span::styled(
            format!("[{:>5}..{:<5}] ", m.start, m.end),
            Style::default().fg(ACCENT2),
        );
        let ln = Span::styled(format!("L{:>3} ", m.line), Style::default().fg(ACCENT));
        let txt = Span::styled(m.text.clone(), HIGHLIGHT);
        lines.push(Line::from(vec![marker, idx, offset, ln, txt]));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_preview(f: &mut Frame, area: Rect, app: &mut App) {
    let focused = app.focus == Focus::Preview;
    let title = if app.replacement.is_empty() {
        " Preview — original text (matches highlighted) "
    } else {
        " Preview — replaced content (green = replaced) "
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(if focused {
            Style::default().fg(ACCENT2)
        } else {
            Style::default()
        });
    let inner = block.inner(area);
    f.render_widget(block, area);

    let height = inner.height as usize;
    let total = app.preview_lines.len();
    let max_scroll = total.saturating_sub(height);
    app.preview_scroll = app.preview_scroll.min(max_scroll);

    let start = app.preview_scroll;
    let end = (start + height).min(total);

    let mut lines: Vec<Line> = Vec::with_capacity(end - start);
    for i in start..end {
        let mut spans: Vec<Span> = Vec::new();
        spans.push(Span::styled(format!("{:>4} ", i + 1), DIM));
        for sp in &app.preview_lines[i] {
            let style = match sp.kind {
                SpanKind::Plain => Style::default(),
                SpanKind::Match => HIGHLIGHT,
                SpanKind::Replacement => REPL_HIGHLIGHT,
            };
            spans.push(Span::styled(sp.text.clone(), style));
        }
        lines.push(Line::from(spans));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled("(empty file)", DIM)));
    }
    f.render_widget(Paragraph::new(lines), inner);
}

fn draw_help_bar(f: &mut Frame, area: Rect, app: &App) {
    let line1 = Line::from(vec![
        Span::styled(" Tab ", Style::default().fg(Color::Black).bg(ACCENT).add_modifier(BOLD)),
        Span::styled("focus ", Style::default().fg(Color::DarkGray)),
        Span::styled("/ ", Style::default().fg(Color::Black).bg(ACCENT).add_modifier(BOLD)),
        Span::styled("pattern ", Style::default().fg(Color::DarkGray)),
        Span::styled("r ", Style::default().fg(Color::Black).bg(ACCENT2).add_modifier(BOLD)),
        Span::styled("replace ", Style::default().fg(Color::DarkGray)),
        Span::styled("i ", Style::default().fg(Color::Black).bg(ACCENT3).add_modifier(BOLD)),
        Span::styled("ignore-case ", Style::default().fg(Color::DarkGray)),
        Span::styled("F2 ", Style::default().fg(Color::Black).bg(ACCENT).add_modifier(BOLD)),
        Span::styled("presets ", Style::default().fg(Color::DarkGray)),
        Span::styled("F1 ", Style::default().fg(Color::Black).bg(ACCENT).add_modifier(BOLD)),
        Span::styled("help ", Style::default().fg(Color::DarkGray)),
        Span::styled("q ", Style::default().fg(Color::Black).bg(Color::Red).add_modifier(BOLD)),
        Span::styled("quit", Style::default().fg(Color::DarkGray)),
    ]);

    let line2 = match app.focus {
        Focus::Pattern | Focus::Replacement => Line::from(Span::styled(
            " typing edits · ←→ cursor · Backspace/Del · Ctrl+A/E home/end · Ctrl+U clear · Ctrl+K kill · Esc clear · Enter → matches",
            DIM,
        )),
        Focus::Results => Line::from(Span::styled(
            " ↑↓ select match · Enter jump preview · PgUp/PgDn page · Home/End first/last · / pattern · r replace · i case",
            DIM,
        )),
        Focus::Preview => Line::from(Span::styled(
            " ↑↓ scroll preview · PgUp/PgDn page · Home/End top/bottom · / pattern · r replace · i case",
            DIM,
        )),
    };

    f.render_widget(Paragraph::new(vec![line1, line2]), area);
}

fn draw_overlay(f: &mut Frame, area: Rect, title: &str, text: &str, color: Color) {
    let width = area.width.min(64);
    let height = area.height.min(24);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let rect = Rect {
        x,
        y,
        width,
        height,
    };
    f.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(color));
    f.render_widget(Paragraph::new(text).block(block), rect);
}

fn draw_presets_overlay(f: &mut Frame, area: Rect, app: &App) {
    let width = area.width.min(62);
    let height = ((PRESETS.len() + 4) as u16).min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let rect = Rect {
        x,
        y,
        width,
        height,
    };
    f.render_widget(Clear, rect);

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        " Presets — Enter applies, Esc cancels ",
        Style::default().fg(ACCENT).add_modifier(BOLD),
    )));
    for (i, (name, pattern)) in PRESETS.iter().enumerate() {
        let selected = i == app.preset_selection;
        let base = if selected {
            Style::default().add_modifier(Modifier::REVERSED)
        } else {
            Style::default()
        };
        let arrow = if selected { "▶ " } else { "  " };
        lines.push(Line::from(vec![
            Span::styled(format!("{}{:<20}", arrow, name), base),
            Span::styled(format!(" {}", pattern), DIM),
        ]));
    }
    lines.push(Line::from(Span::styled(
        " ↑↓ select · Enter apply · Esc cancel ",
        DIM,
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Presets ")
        .border_style(Style::default().fg(ACCENT3));
    f.render_widget(Paragraph::new(lines).block(block), rect);
}

/// Clamp a vertical scroll offset so that `selected` stays within `height` rows.
fn clamp_scroll(scroll: usize, selected: usize, len: usize, height: usize) -> usize {
    let mut s = scroll;
    if s > selected {
        s = selected;
    }
    if selected >= s + height {
        s = selected + 1 - height;
    }
    s.min(len.saturating_sub(height))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::backend::TestBackend;
    use ratatui::style::Color;
    use ratatui::Terminal;

    fn render_rows(app: &mut App, w: u16, h: u16) -> Vec<String> {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        let buf = terminal.backend().buffer();
        let mut rows = Vec::new();
        for y in 0..h {
            let mut row = String::new();
            for x in 0..w {
                row.push_str(buf.cell((x, y)).expect("in bounds").symbol());
            }
            rows.push(row);
        }
        rows
    }

    #[test]
    fn screen_shows_match_text_and_offsets() {
        let mut app = App::from_text("call 13812345678 now\n");
        app.pattern.set(r"1[3-9]\d{9}");
        app.recompute();
        let rows = render_rows(&mut app, 120, 36);
        let screen = rows.join("\n");
        assert!(screen.contains("13812345678"), "match text should be on screen");
        assert!(
            screen.contains("5..16"),
            "0-indexed char offsets [5..16) should be on screen"
        );
        assert!(screen.contains("Matches (1)"), "match count should be shown");
    }

    #[test]
    fn matched_text_gets_yellow_background() {
        let mut app = App::from_text("call 13812345678 now\n");
        app.pattern.set(r"1[3-9]\d{9}");
        app.recompute();
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let buf = terminal.backend().buffer();
        let highlighted = buf.content.iter().any(|c| {
            c.style().bg == Some(Color::Yellow)
                && c.symbol().chars().any(|ch| ch.is_ascii_digit())
        });
        assert!(highlighted, "matched digits must have a yellow background");
    }

    #[test]
    fn replacement_preview_highlighted_green() {
        let mut app = App::from_text("call 13812345678\n");
        app.pattern.set(r"1[3-9]\d{9}");
        app.replacement.set("X");
        app.recompute();
        let backend = TestBackend::new(120, 36);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let buf = terminal.backend().buffer();
        let green = buf.content.iter().any(|c| c.style().bg == Some(Color::Green));
        assert!(green, "replaced content must have a green background");
    }

    #[test]
    fn both_panes_visible_on_same_screen() {
        let mut app = App::from_text("line1\nline2\nline3\n");
        app.pattern.set("line");
        app.recompute();
        let rows = render_rows(&mut app, 120, 36);
        let screen = rows.join("\n");
        assert!(screen.contains("Matches (3)"));
        assert!(screen.contains("Preview"));
    }
}
