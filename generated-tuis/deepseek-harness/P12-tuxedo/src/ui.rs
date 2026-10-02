//! ratatui rendering.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{App, Mode};
use crate::input::LineBuffer;
use crate::task::Task;

pub fn render(frame: &mut ratatui::Frame<'_>, app: &mut App) {
    let area = frame.area();
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    render_title(frame, app, chunks[0]);
    if app.mode == Mode::Help {
        render_help(frame, chunks[1]);
    } else {
        render_main(frame, app, chunks[1]);
    }
    render_status(frame, app, chunks[2]);
    render_help_bar(frame, chunks[3]);
}

fn render_main(frame: &mut ratatui::Frame<'_>, app: &mut App, area: Rect) {
    let form_height = app
        .input
        .as_ref()
        .map(|inp| inp.fields.len() as u16 + 3)
        .unwrap_or(0);

    let (top, form_area) = if form_height > 0 {
        let chunks = Layout::vertical([Constraint::Min(3), Constraint::Length(form_height)])
            .split(area);
        (chunks[0], Some(chunks[1]))
    } else {
        (area, None)
    };

    let cols = Layout::horizontal([Constraint::Percentage(70), Constraint::Percentage(30)])
        .split(top);

    render_list(frame, app, cols[0]);
    render_detail(frame, app, cols[1]);

    if let Some(form_area) = form_area {
        render_form(frame, app, form_area);
    }
}

fn render_title(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let done = app.tasks.iter().filter(|t| t.completed).count();
    let total = app.tasks.len();
    let title = format!(
        " tooll · {} · {} tasks ({} done, {} open) ",
        app.store.path().display(),
        total,
        done,
        total - done
    );
    let p = Paragraph::new(Line::from(Span::styled(
        title,
        Style::default()
            .fg(Color::White)
            .bg(Color::Blue)
            .add_modifier(Modifier::BOLD),
    )));
    frame.render_widget(p, area);
}

fn render_list(frame: &mut ratatui::Frame<'_>, app: &mut App, area: Rect) {
    app.list_height = area.height.saturating_sub(2);
    let visible = app.visible_indices();

    if visible.is_empty() {
        let hint = if app.tasks.is_empty() {
            "No tasks yet. Press 'a' to add the first task."
        } else {
            "No tasks match the current filter. Press 'r' or Esc to clear it."
        };
        let p = Paragraph::new(hint)
            .block(Block::default().borders(Borders::ALL).title(" Tasks "))
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, area);
        return;
    }

    let items: Vec<ListItem> = visible
        .iter()
        .map(|&i| ListItem::new(task_line(&app.tasks[i])))
        .collect();
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Tasks "))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

    let mut state = ListState::default();
    state.select(Some(app.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn task_line(t: &Task) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    let mark = if t.completed { "[x]" } else { "[ ]" };
    spans.push(Span::styled(
        mark.to_string(),
        Style::default().fg(if t.completed { Color::Green } else { Color::DarkGray }),
    ));
    spans.push(Span::raw(" "));

    if let Some(p) = t.priority {
        spans.push(Span::styled(format!("({p})"), priority_style(p)));
        spans.push(Span::raw(" "));
    }

    spans.push(Span::raw(t.description.clone()));

    for p in &t.projects {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(format!("+{p}"), Style::default().fg(Color::Cyan)));
    }
    for c in &t.contexts {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(format!("@{c}"), Style::default().fg(Color::Magenta)));
    }
    if let Some(d) = t.due_date() {
        spans.push(Span::raw(" "));
        let style = if t.is_overdue() {
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Yellow)
        };
        spans.push(Span::styled(format!("due:{d}"), style));
    }

    let mut line = Line::from(spans);
    if t.completed {
        line = line.patch_style(
            Style::default().add_modifier(Modifier::CROSSED_OUT | Modifier::DIM),
        );
    }
    line
}

fn priority_style(p: char) -> Style {
    match p {
        'A' => Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        'B' => Style::default().fg(Color::Yellow),
        'C' => Style::default().fg(Color::Green),
        _ => Style::default().fg(Color::Blue),
    }
}

fn render_detail(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let text: Vec<Line> = match app.selected_index() {
        Some(i) => detail_lines(&app.tasks[i], i),
        None => vec![Line::from("No selection")],
    };
    let p = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Details "))
        .wrap(Wrap { trim: true });
    frame.render_widget(p, area);
}

fn detail_lines(t: &Task, real: usize) -> Vec<Line<'static>> {
    let label =
        |s: &'static str| Span::styled(s, Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));

    let mut lines = vec![
        Line::from(Span::styled(
            format!("Task #{real}"),
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        Line::from(""),
        Line::from(vec![
            label("Status:     "),
            Span::raw(if t.completed { "Completed" } else { "Open" }),
        ]),
    ];

    let prio = t.priority.map(|p| format!("({p})")).unwrap_or_else(|| "-".into());
    lines.push(Line::from(vec![label("Priority:   "), Span::raw(prio)]));

    let projects = if t.projects.is_empty() {
        "-".to_string()
    } else {
        t.projects.iter().map(|p| format!("+{p}")).collect::<Vec<_>>().join(" ")
    };
    lines.push(Line::from(vec![label("Projects:   "), Span::raw(projects)]));

    let contexts = if t.contexts.is_empty() {
        "-".to_string()
    } else {
        t.contexts.iter().map(|c| format!("@{c}")).collect::<Vec<_>>().join(" ")
    };
    lines.push(Line::from(vec![label("Contexts:   "), Span::raw(contexts)]));

    let due = match t.due_date() {
        Some(d) if t.is_overdue() => format!("{d} (overdue)"),
        Some(d) => d.to_string(),
        None => "-".to_string(),
    };
    lines.push(Line::from(vec![label("Due:        "), Span::raw(due)]));

    if let Some(cd) = &t.completion_date {
        lines.push(Line::from(vec![label("Completed:  "), Span::raw(cd.clone())]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![label("Description:"), Span::raw("")]));
    lines.push(Line::from(Span::raw(t.description.clone())));
    lines.push(Line::from(""));
    lines.push(Line::from(vec![label("Raw line:"), Span::raw("")]));
    lines.push(Line::from(Span::styled(
        t.format(),
        Style::default().fg(Color::DarkGray),
    )));

    lines
}

fn render_status(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let mut spans: Vec<Span> = Vec::new();

    let mut filters: Vec<String> = Vec::new();
    if let Some(p) = &app.filter.project {
        filters.push(format!("+{p}"));
    }
    if let Some(c) = &app.filter.context {
        filters.push(format!("@{c}"));
    }
    if let Some(s) = &app.filter.search {
        filters.push(format!("/{s}/"));
    }
    if !filters.is_empty() {
        spans.push(Span::styled(
            format!(" filter: {} (r/Esc clears) ", filters.join(" ")),
            Style::default().fg(Color::Black).bg(Color::Cyan),
        ));
        spans.push(Span::raw(" "));
    }

    if let Some(s) = &app.status {
        spans.push(Span::raw(s.clone()));
    } else {
        spans.push(Span::raw("Ready"));
    }

    let p = Paragraph::new(Line::from(spans));
    frame.render_widget(p, area);
}

fn render_help_bar(frame: &mut ratatui::Frame<'_>, area: Rect) {
    let bar = Line::from(Span::styled(
        " q quit · a add · e edit · Space done · p/P priority · d delete · / search · + project · @ context · r clear · ? help ",
        Style::default().fg(Color::White).bg(Color::DarkGray),
    ));
    frame.render_widget(Paragraph::new(bar), area);
}

fn render_form(frame: &mut ratatui::Frame<'_>, app: &App, area: Rect) {
    let Some(input) = &app.input else {
        return;
    };

    let mut lines: Vec<Line> = Vec::new();
    for (i, field) in input.fields.iter().enumerate() {
        let active = i == input.active;
        let mut spans: Vec<Span> = vec![
            Span::styled(
                if active { "▸ " } else { "  " },
                Style::default().fg(if active { Color::Yellow } else { Color::DarkGray }),
            ),
            Span::styled(
                format!("{}: ", field.label),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
        ];
        spans.extend(field_value_spans(&field.value, active));
        let mut line = Line::from(spans);
        if active {
            line = line.patch_style(Style::default().bg(Color::DarkGray));
        }
        lines.push(line);
    }

    if let Some(e) = &input.error {
        lines.push(Line::from(Span::styled(
            format!(" ! {e}"),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
    } else {
        lines.push(Line::from(Span::styled(
            " Tab/↑↓ field · ←/→ cursor · Enter save · Esc cancel",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(format!(" {} ", input.title));
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

fn field_value_spans(value: &LineBuffer, active: bool) -> Vec<Span<'static>> {
    let chars: Vec<char> = value.as_string().chars().collect();
    let cursor = value.cursor().min(chars.len());
    let mut spans = Vec::new();
    for (i, ch) in chars.iter().enumerate() {
        if active && i == cursor {
            spans.push(Span::styled(
                ch.to_string(),
                Style::default().fg(Color::Black).bg(Color::White),
            ));
        } else {
            spans.push(Span::raw(ch.to_string()));
        }
    }
    if active && cursor == chars.len() {
        spans.push(Span::styled(
            " ".to_string(),
            Style::default().fg(Color::Black).bg(Color::White),
        ));
    }
    spans
}

fn render_help(frame: &mut ratatui::Frame<'_>, area: Rect) {
    let text = vec![
        Line::from(Span::styled(
            "tooll — key bindings",
            Style::default().add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )),
        Line::from(""),
        Line::from("  Navigation"),
        Line::from("    ↑/↓ or j/k     Move selection"),
        Line::from("    Home/End, g/G   Jump to first / last task"),
        Line::from("    PgUp/PgDn       Page through the list"),
        Line::from(""),
        Line::from("  Tasks"),
        Line::from("    a               Add a new task"),
        Line::from("    e / Enter       Edit the selected task"),
        Line::from("    Space           Toggle complete / reopen"),
        Line::from("    x               Mark task complete"),
        Line::from("    u               Mark task not complete"),
        Line::from("    p / P           Raise / lower priority (e.g. (B) → (A))"),
        Line::from("    d               Delete task (with confirmation)"),
        Line::from(""),
        Line::from("  Filtering & search"),
        Line::from("    /               Live text search"),
        Line::from("    +               Filter by project tag"),
        Line::from("    @               Filter by context tag"),
        Line::from("    r / Esc         Clear all filters"),
        Line::from(""),
        Line::from("  Editing fields"),
        Line::from("    Tab / Shift+Tab Move between fields"),
        Line::from("    ←/→, Home/End   Move cursor"),
        Line::from("    Enter           Save / confirm"),
        Line::from("    Esc             Cancel"),
        Line::from(""),
        Line::from("  General"),
        Line::from("    ? / h           Show this help"),
        Line::from("    q / Ctrl+C      Quit"),
        Line::from(""),
        Line::from("All changes are written back to the todo.txt file immediately."),
    ];
    let p = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Help (? or Esc to close) "))
        .wrap(Wrap { trim: true });
    frame.render_widget(p, area);
}
