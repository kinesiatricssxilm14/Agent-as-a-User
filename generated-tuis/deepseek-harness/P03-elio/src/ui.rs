//! Terminal rendering: dual-pane list + preview, status bar, key/input bar,
//! and the help screen.

use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

use crate::app::{Action, App, Focus, Mode, MsgKind};
use crate::ops::Preview;

const HELP: &str = r#"
toolc - File Manager TUI
========================

Navigation
  Up / k             Move selection up
  Down / j           Move selection down
  Enter / Right      Open the selected directory
  Left / Backspace   Go to the parent directory
  Home / g           Jump to the first entry
  End / G            Jump to the last entry
  PageUp / PageDown  Page through the list
  Tab                Switch focus between list and preview

Preview (when the preview panel is focused)
  Up / Down          Scroll the preview by one line
  PageUp / PageDown  Scroll the preview by one page
  Home / End         Jump to the top / bottom of the preview

File operations
  F2                 Rename the selected file or directory
  F5                 Copy the selected file to a destination path
  F6                 Move the selected file to a directory or path
  F7                 Create a new directory at a path
  F8 / Delete        Delete the selected file or directory (asks confirmation)
  /                  Filter the list by name (clear the filter to show all)

Prompts
  Enter              Confirm the typed value
  Esc                Cancel the current prompt
  Ctrl-U             Clear the input line
  Ctrl-W             Delete the previous word

General
  F1 / ?             Show this help
  q / Ctrl-C         Quit toolc

Arguments
  toolc [DIR]        Open DIR on startup (default: /bench/data/src)

All operations use the real filesystem: copy is byte-identical, move and
rename remove the original path, and delete removes directories recursively.
"#;

pub fn render(f: &mut Frame, app: &mut App) {
    match app.mode {
        Mode::Help => render_help(f, app),
        _ => render_main(f, app),
    }
}

fn render_main(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title bar
            Constraint::Min(3),    // main area (list | preview)
            Constraint::Length(1), // status / message
            Constraint::Length(1), // key hints / input prompt
        ])
        .split(f.area());

    render_title(f, app, chunks[0]);

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(chunks[1]);
    render_list(f, app, panes[0]);
    render_preview(f, app, panes[1]);

    render_status(f, app, chunks[2]);
    render_bottom(f, app, chunks[3]);
}

fn render_title(f: &mut Frame, app: &App, area: Rect) {
    let label = format!(" toolc - {} ", app.cwd.display());
    let style = Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
    let filter = if app.filter.is_empty() {
        String::new()
    } else {
        format!("  [filter: {}]", app.filter)
    };
    let mut spans = vec![Span::styled(label, style)];
    spans.push(Span::styled(
        filter,
        Style::default().fg(Color::Yellow).bg(Color::Black),
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_list(f: &mut Frame, app: &mut App, area: Rect) {
    let items: Vec<ListItem> = app
        .entries
        .iter()
        .map(|e| {
            let name = if e.is_dir {
                format!("{}/", e.name)
            } else {
                e.name.clone()
            };
            let name_span = Span::styled(
                name,
                if e.is_dir {
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            );
            if e.is_dir {
                ListItem::new(Line::from(name_span))
            } else {
                ListItem::new(Line::from(vec![
                    name_span,
                    Span::styled(
                        format!("  {}", human_size(e.size)),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]))
            }
        })
        .collect();

    // Keep the selection visible.
    let list_h = area.height.saturating_sub(2) as usize;
    if list_h > 0 {
        if app.selected < app.list_offset {
            app.list_offset = app.selected;
        } else if app.selected >= app.list_offset + list_h {
            app.list_offset = app.selected + 1 - list_h;
        }
    }

    let mut state = ListState::default();
    state.select(if app.entries.is_empty() {
        None
    } else {
        Some(app.selected)
    });
    *state.offset_mut() = app.list_offset;

    let focused = app.focus == Focus::List;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(if focused { " Files [focus] " } else { " Files " })
        .border_style(if focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    f.render_stateful_widget(list, area, &mut state);
}

fn render_preview(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Preview;
    let title = if app.preview_name.is_empty() {
        " Preview ".to_string()
    } else {
        format!(" Preview: {} ", app.preview_name)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(if focused {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        });

    let lines = preview_lines(app);
    let inner_h = area.height.saturating_sub(2) as usize;
    if inner_h > 0 {
        let max = lines.len().saturating_sub(inner_h);
        if app.preview_scroll > max {
            app.preview_scroll = max;
        }
    }
    let start = app.preview_scroll.min(lines.len());
    let slice: Vec<Line> = lines
        .into_iter()
        .skip(start)
        .take(inner_h.max(1))
        .collect();

    f.render_widget(Paragraph::new(Text::from(slice)).block(block), area);
}

fn preview_lines(app: &App) -> Vec<Line<'static>> {
    match &app.preview {
        None => {
            let msg = if app.preview_name.is_empty() {
                "(no selection)"
            } else {
                "(directory)"
            };
            vec![Line::from(Span::styled(
                msg,
                Style::default().fg(Color::DarkGray),
            ))]
        }
        Some(Preview::Empty) => vec![Line::from(Span::styled(
            "(empty file)",
            Style::default().fg(Color::DarkGray),
        ))],
        Some(Preview::Error(e)) => vec![Line::from(Span::styled(
            format!("(error: {})", e),
            Style::default().fg(Color::Red),
        ))],
        Some(Preview::Text(s)) => s
            .split('\n')
            .map(|l| Line::from(Span::raw(l.to_string())))
            .collect(),
        Some(Preview::Binary { size, hex }) => {
            let mut v = vec![Line::from(Span::styled(
                format!("(binary file, {} bytes)", size),
                Style::default().fg(Color::DarkGray),
            ))];
            for l in hex.split('\n') {
                v.push(Line::from(Span::raw(l.to_string())));
            }
            v
        }
    }
}

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let span = match &app.message {
        Some(m) => {
            let style = match m.kind {
                MsgKind::Info => Style::default().fg(Color::Yellow),
                MsgKind::Success => Style::default().fg(Color::Green),
                MsgKind::Error => Style::default().fg(Color::Red),
            };
            Span::styled(m.text.clone(), style)
        }
        None => Span::styled("Ready", Style::default().fg(Color::DarkGray)),
    };
    f.render_widget(Paragraph::new(Line::from(span)), area);
}

fn render_bottom(f: &mut Frame, app: &App, area: Rect) {
    match app.mode {
        Mode::Input => render_input_bar(f, app, area),
        Mode::Confirm => {
            let label = match app.selected_entry() {
                Some(e) if e.is_dir => format!("Delete directory '{}' and all contents?", e.name),
                Some(e) => format!("Delete file '{}'?", e.name),
                None => "Delete?".to_string(),
            };
            let style = Style::default().bg(Color::Red).fg(Color::White);
            let text = format!(" {}  [y]es / [n]o ", label);
            f.render_widget(Paragraph::new(Line::from(Span::styled(text, style))), area);
        }
        _ => {
            let keys =
                "F1 Help  F2 Rename  F5 Copy  F6 Move  F7 Mkdir  F8 Del  / Filter  Tab Focus  Enter Open  <- Up  q Quit";
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    keys,
                    Style::default().fg(Color::DarkGray),
                ))),
                area,
            );
        }
    }
}

fn render_input_bar(f: &mut Frame, app: &App, area: Rect) {
    let prompt = match app.action {
        Some(Action::Rename) => "Rename to: ",
        Some(Action::Copy) => "Copy to: ",
        Some(Action::Move) => "Move to: ",
        Some(Action::Mkdir) => "New directory: ",
        Some(Action::Search) => "Filter: ",
        None => "",
    };
    let mut spans = vec![Span::styled(
        prompt.to_string(),
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )];
    spans.push(Span::raw(app.input.clone()));
    spans.push(Span::styled(" ", Style::default().bg(Color::White).fg(Color::Black)));
    spans.push(Span::styled(
        "  Enter=confirm  Esc=cancel",
        Style::default().fg(Color::DarkGray),
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_help(f: &mut Frame, app: &mut App) {
    let lines: Vec<Line> = HELP
        .lines()
        .map(|l| Line::from(Span::raw(l.to_string())))
        .collect();
    let inner_h = f.area().height.saturating_sub(2) as usize;
    let max = lines.len().saturating_sub(inner_h);
    if app.help_scroll > max {
        app.help_scroll = max;
    }
    let start = app.help_scroll.min(lines.len());
    let slice: Vec<Line> = lines
        .into_iter()
        .skip(start)
        .take(inner_h.max(1))
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help (Esc / q to close) ");
    f.render_widget(Paragraph::new(Text::from(slice)).block(block), f.area());
}

fn human_size(n: u64) -> String {
    if n < 1024 {
        format!("{} B", n)
    } else if n < 1024 * 1024 {
        format!("{:.1} K", n as f64 / 1024.0)
    } else if n < 1024 * 1024 * 1024 {
        format!("{:.1} M", n as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} G", n as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}
