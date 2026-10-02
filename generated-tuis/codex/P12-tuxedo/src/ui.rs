use crate::app::{App, Mode};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(6),
            Constraint::Length(3),
            Constraint::Length(2),
        ])
        .split(area);

    draw_header(frame, app, chunks[0]);
    draw_tasks(frame, app, chunks[1]);
    draw_status(frame, app, chunks[2]);
    draw_shortcuts(frame, app, chunks[3]);

    match app.mode {
        Mode::Add => draw_add(frame, app),
        Mode::Priority => draw_priority(frame),
        Mode::EditTags => draw_tag_editor(frame, app),
        Mode::Help => draw_help(frame),
        Mode::ConfirmDelete => draw_delete(frame),
        _ => {}
    }
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let total = app.tasks.len();
    let complete = app.tasks.iter().filter(|task| task.completed).count();
    let visible = app.visible_indices().len();
    let title = Line::from(vec![
        Span::styled(
            " TOOLL ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "  {visible} visible · {} open · {complete} done",
            total.saturating_sub(complete)
        )),
    ]);
    let path = format!(" {} ", app.path.display());
    frame.render_widget(
        Paragraph::new(title).block(Block::default().borders(Borders::ALL).title_bottom(path)),
        area,
    );
}

fn draw_tasks(frame: &mut Frame, app: &App, area: Rect) {
    let indices = app.visible_indices();
    let inner_height = area.height.saturating_sub(2) as usize;
    let offset = if inner_height == 0 || app.selected < inner_height {
        0
    } else {
        app.selected + 1 - inner_height
    };
    let items: Vec<ListItem> = indices
        .iter()
        .map(|index| {
            let task = &app.tasks[*index];
            let marker = if task.completed { "✓" } else { "○" };
            let priority = task
                .priority
                .map(|value| format!("({value})"))
                .unwrap_or_else(|| "   ".to_string());
            let mut spans = vec![
                Span::styled(
                    format!("{marker} "),
                    Style::default().fg(if task.completed {
                        Color::DarkGray
                    } else {
                        Color::Green
                    }),
                ),
                Span::styled(
                    format!("{priority} "),
                    Style::default()
                        .fg(priority_color(task.priority))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    task.description.clone(),
                    Style::default()
                        .fg(if task.completed {
                            Color::DarkGray
                        } else {
                            Color::White
                        })
                        .add_modifier(if task.completed {
                            Modifier::CROSSED_OUT
                        } else {
                            Modifier::empty()
                        }),
                ),
            ];
            for project in &task.projects {
                spans.push(Span::styled(
                    format!(" +{project}"),
                    Style::default().fg(Color::Magenta),
                ));
            }
            for context in &task.contexts {
                spans.push(Span::styled(
                    format!(" @{context}"),
                    Style::default().fg(Color::Cyan),
                ));
            }
            if let Some(due) = &task.due {
                spans.push(Span::styled(
                    format!(" due:{due}"),
                    Style::default().fg(Color::Yellow),
                ));
            }
            for extra in &task.extra {
                spans.push(Span::styled(
                    format!(" {extra}"),
                    Style::default().fg(Color::Blue),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let title = if indices.is_empty() {
        " Tasks — no matching tasks "
    } else {
        " Tasks "
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title))
        .highlight_symbol("› ")
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );
    let mut state = ListState::default();
    *state.offset_mut() = offset;
    if !indices.is_empty() {
        state.select(Some(app.selected));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let content = match app.mode {
        Mode::Search => format!("Search: {}▌  Enter apply · Esc cancel", app.input),
        Mode::ProjectFilter => {
            format!("Project: +{}▌  Enter apply · empty clears", app.input)
        }
        Mode::ContextFilter => {
            format!("Context: @{}▌  Enter apply · empty clears", app.input)
        }
        Mode::Normal => {
            let filters = active_filters(app);
            if filters.is_empty() {
                app.message.clone()
            } else {
                format!("{}  │  {}", filters.join("  "), app.message)
            }
        }
        _ => app.message.clone(),
    };
    frame.render_widget(
        Paragraph::new(content)
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).title(" Status ")),
        area,
    );
}

fn draw_shortcuts(frame: &mut Frame, app: &App, area: Rect) {
    let text = match app.mode {
        Mode::Normal | Mode::Search | Mode::ProjectFilter | Mode::ContextFilter => {
            "↑/↓ navigate  a add  Enter/x complete  p priority  t tags  / search  P project  C context  c clear  ? help  q quit"
        }
        Mode::Add => "Tab/↑/↓ fields  Enter next/save on Due  Ctrl+Enter save  Esc cancel",
        Mode::Priority => "A-Z set priority  - remove  Esc cancel",
        Mode::EditTags => "Format: projects | contexts   Enter save  Esc cancel",
        Mode::Help => "Esc/?/q close help",
        Mode::ConfirmDelete => "y delete permanently  n/Esc cancel",
    };
    frame.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray)),
        area,
    );
}

fn active_filters(app: &App) -> Vec<String> {
    let mut filters = Vec::new();
    if !app.search.is_empty() {
        filters.push(format!("search:\"{}\"", app.search));
    }
    if !app.project_filter.is_empty() {
        filters.push(format!("project:+{}", app.project_filter));
    }
    if !app.context_filter.is_empty() {
        filters.push(format!("context:@{}", app.context_filter));
    }
    filters
}

fn draw_add(frame: &mut Frame, app: &App) {
    let area = centered_rect(76, 17, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Add task ")
        .style(Style::default().bg(Color::Black));
    frame.render_widget(block, area);
    let inner = area.inner(ratatui::layout::Margin {
        horizontal: 2,
        vertical: 1,
    });
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(2),
        ])
        .split(inner);
    let values = [
        (
            "Description",
            &app.add_form.description,
            "What needs doing?",
        ),
        ("Priority", &app.add_form.priority, "A-Z or blank"),
        ("Projects", &app.add_form.projects, "+work, +personal"),
        ("Contexts", &app.add_form.contexts, "@home, @computer"),
        ("Due", &app.add_form.due, "YYYY-MM-DD or blank"),
    ];
    for (index, (label, value, hint)) in values.iter().enumerate() {
        let active = app.add_field.index() == index;
        let displayed = if value.is_empty() && !active {
            Span::styled(*hint, Style::default().fg(Color::DarkGray))
        } else {
            Span::raw(format!("{value}{}", if active { "▌" } else { "" }))
        };
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(
                    format!("{label:12}"),
                    Style::default()
                        .fg(if active { Color::Cyan } else { Color::White })
                        .add_modifier(Modifier::BOLD),
                ),
                displayed,
            ])),
            rows[index],
        );
    }
    frame.render_widget(
        Paragraph::new("Tags may be separated with spaces or commas. Prefixes are optional.")
            .style(Style::default().fg(Color::DarkGray)),
        rows[5],
    );
}

fn draw_priority(frame: &mut Frame) {
    draw_dialog(
        frame,
        52,
        7,
        " Change priority ",
        vec![
            Line::from("Press A–Z to assign a priority."),
            Line::from("Press - or Backspace to remove it."),
            Line::from("Esc cancels."),
        ],
    );
}

fn draw_tag_editor(frame: &mut Frame, app: &App) {
    draw_dialog(
        frame,
        72,
        8,
        " Edit project and context tags ",
        vec![
            Line::from("Projects are left of | and contexts are right."),
            Line::from("Prefixes are optional; spaces or commas separate tags."),
            Line::from(""),
            Line::from(vec![
                Span::styled("> ", Style::default().fg(Color::Cyan)),
                Span::raw(format!("{}▌", app.input)),
            ]),
        ],
    );
}

fn draw_help(frame: &mut Frame) {
    draw_dialog(
        frame,
        82,
        22,
        " Help — all commands ",
        vec![
            Line::from(vec![
                key("↑/↓, j/k"),
                Span::raw("  Move through visible tasks"),
            ]),
            Line::from(vec![
                key("Home/g"),
                Span::raw("  First task; End/G last task"),
            ]),
            Line::from(vec![
                key("a"),
                Span::raw("  Add task with all todo.txt fields"),
            ]),
            Line::from(vec![key("Enter/x"), Span::raw("  Toggle complete/reopen")]),
            Line::from(vec![
                key("p"),
                Span::raw("  Set or remove selected priority"),
            ]),
            Line::from(vec![
                key("t"),
                Span::raw("  Replace project and context tags"),
            ]),
            Line::from(vec![
                key("d"),
                Span::raw("  Delete selected task (asks first)"),
            ]),
            Line::from(vec![key("/"), Span::raw("  Live free-text search")]),
            Line::from(vec![key("P"), Span::raw("  Live exact project filter")]),
            Line::from(vec![key("C"), Span::raw("  Live exact context filter")]),
            Line::from(vec![key("c"), Span::raw("  Clear all active filters")]),
            Line::from(vec![key("r"), Span::raw("  Reload current file from disk")]),
            Line::from(vec![key("q/Ctrl+C"), Span::raw("  Quit")]),
            Line::from(""),
            Line::from("Every mutation is written immediately and atomically."),
            Line::from("Launch with: tooll [PATH] or tooll --file PATH"),
        ],
    );
}

fn draw_delete(frame: &mut Frame) {
    draw_dialog(
        frame,
        52,
        6,
        " Delete task? ",
        vec![
            Line::from("This removes the selected line from disk."),
            Line::from("Press y to delete, n or Esc to cancel."),
        ],
    );
}

fn draw_dialog(frame: &mut Frame, width: u16, height: u16, title: &str, lines: Vec<Line>) {
    let area = centered_rect(width, height, frame.area());
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .style(Style::default().bg(Color::Black)),
        ),
        area,
    );
}

fn key(value: &'static str) -> Span<'static> {
    Span::styled(
        format!("{value:12}"),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
}

fn priority_color(priority: Option<char>) -> Color {
    match priority {
        Some('A') => Color::Red,
        Some('B') => Color::Yellow,
        Some('C') => Color::Green,
        Some(_) => Color::Blue,
        None => Color::DarkGray,
    }
}

fn centered_rect(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect {
        x: area.x + area.width.saturating_sub(width) / 2,
        y: area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    }
}
