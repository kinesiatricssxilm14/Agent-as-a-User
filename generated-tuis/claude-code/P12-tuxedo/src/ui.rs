//! Rendering. A pure function of [`App`] — no state is kept here.
//!
//! Layout (everything on one screen, no tabs and no information-hiding
//! overlays):
//!
//! ```text
//! ┌ header: file path, counts, active filters, sort ──────────────┐
//! ├ tasks ───────────────────────┬ projects ─────┬ contexts ──────┤
//! │ ● (A) Pay rent +finance ...  │ all (12)      │ all (12)       │
//! │   (B) Buy groceries ...      │ finance (3)   │ computer (4)   │
//! ├ detail: the selected task's fields, one per column ───────────┤
//! ├ prompt / form (only when active, list stays visible) ─────────┤
//! ├ status message ──────────────────────────────────────────────┤
//! └ key hints ───────────────────────────────────────────────────┘
//! ```

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
    ScrollbarOrientation, ScrollbarState, Wrap,
};
use ratatui::Frame;

use crate::app::{App, DoneFilter, Focus, FormField, Message, MessageKind, Mode, Prompt};
use crate::date::{self, Ymd};
use crate::keys;
use crate::task::{classify_token, Task, TokenKind};

// A restrained palette that stays legible on both dark and light terminals.
const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const PRIORITY_A: Color = Color::LightRed;
const PRIORITY_B: Color = Color::LightYellow;
const PRIORITY_C: Color = Color::LightGreen;
const PRIORITY_OTHER: Color = Color::LightBlue;
const PROJECT: Color = Color::LightMagenta;
const CONTEXT: Color = Color::LightCyan;
const DUE: Color = Color::Yellow;
const OVERDUE: Color = Color::LightRed;
const DONE: Color = Color::DarkGray;

/// Draw one frame.
pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Height budget for the optional prompt/form pane.
    let editor_height = match &app.mode {
        Mode::Form => 10,
        Mode::Prompt(_) => 3,
        _ => 0,
    };

    // The panes below are sized generously on a normal terminal but give their
    // rows back on a short one, so the task list never collapses to nothing.
    // 4 = header, 1 = message, 2 = hints, 4 = the minimum usable list.
    let spare = area
        .height
        .saturating_sub(4 + 1 + 2 + 4 + editor_height);
    let detail_height = spare.min(6);
    let hints_height = if area.height >= 12 { 2 } else { 1 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),             // header (title + counts/filters)
            Constraint::Min(4),                // task list + side panels
            Constraint::Length(detail_height), // selected-task detail
            Constraint::Length(editor_height), // prompt or form
            Constraint::Length(1),             // status message
            Constraint::Length(hints_height),  // key hints
        ])
        .split(area);

    render_header(f, chunks[0], app);
    render_body(f, chunks[1], app);
    if detail_height > 0 {
        render_detail(f, chunks[2], app);
    }
    if editor_height > 0 {
        match &app.mode {
            Mode::Form => render_form(f, chunks[3], app),
            Mode::Prompt(p) => render_prompt(f, chunks[3], app, &p.clone()),
            _ => {}
        }
    }
    render_message(f, chunks[4], app);
    render_hints(f, chunks[5], app);

    // Help is a full-screen view rather than a partial overlay, so no list
    // content is half-covered; Esc/?/q returns to the list.
    if app.mode == Mode::Help {
        render_help(f, area, app);
    }
}

// --------------------------------------------------------------------- header

fn render_header(f: &mut Frame, area: Rect, app: &App) {
    let total = app.store.len();
    let done = app.store.completed_count();
    let open = total - done;
    let overdue = app.store.overdue_count(app.today);
    let shown = app.visible_count();
    let width = area.width.saturating_sub(2) as usize;

    // The path matters most, so on a narrow terminal keep its tail (the file
    // name) rather than its head.
    let path = app.store.path().display().to_string();
    let source = format!(" [{}]", app.file_source.label());
    let room = width.saturating_sub(7 + source.chars().count());
    let path = if path.chars().count() > room && room > 1 {
        format!("…{}", path.chars().skip(path.chars().count() - room + 1).collect::<String>())
    } else {
        path
    };

    let mut top = vec![
        Span::styled(
            "tooll",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(path, Style::default().add_modifier(Modifier::BOLD)),
        Span::styled(source, Style::default().fg(DIM)),
    ];
    if let Some(cfg) = &app.config_path {
        let extra = format!("  config: {}", cfg.display());
        if spans_width(&top) + extra.chars().count() <= width {
            top.push(Span::styled(extra, Style::default().fg(DIM)));
        }
    }

    // The status line is assembled from most to least important and truncated by
    // dropping whole items, so nothing is ever cut mid-word.
    let mut second = vec![
        Span::raw("showing "),
        Span::styled(
            format!("{shown}/{total}"),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
    ];
    let mut optional: Vec<Vec<Span>> = vec![vec![Span::raw(format!("   open {open}"))]];
    if overdue > 0 {
        optional.push(vec![Span::styled(
            format!("   overdue {overdue}"),
            Style::default().fg(OVERDUE).add_modifier(Modifier::BOLD),
        )]);
    }
    optional.push(vec![
        Span::raw("   filter: "),
        Span::styled(
            app.filter_summary(),
            if app.is_filtered() {
                Style::default().fg(DUE).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(DIM)
            },
        ),
    ]);
    optional.push(vec![Span::raw(format!("   done {done}"))]);
    optional.push(vec![
        Span::raw("   sort: "),
        Span::styled(app.sort_mode.label(), Style::default().fg(DIM)),
    ]);
    optional.push(vec![
        Span::raw("   today "),
        Span::styled(date::format_ymd(app.today), Style::default().fg(DIM)),
    ]);

    for group in optional {
        if spans_width(&second) + spans_width(&group) <= width {
            second.extend(group);
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM));
    f.render_widget(
        Paragraph::new(vec![Line::from(top), Line::from(second)]).block(block),
        area,
    );
}

// ----------------------------------------------------------------------- body

fn render_body(f: &mut Frame, area: Rect, app: &mut App) {
    // The side panels are useful but must not starve the task text. They shrink
    // on a narrow terminal and step aside entirely when there is no room, in
    // which case `f`/`@` still filter and the header still names the filter.
    let panel = match area.width {
        0..=59 => 0,
        60..=79 => 14,
        80..=109 => 18,
        _ => 22,
    };

    if panel == 0 {
        render_task_list(f, area, app);
        return;
    }

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(30),
            Constraint::Length(panel),
            Constraint::Length(panel),
        ])
        .split(area);

    render_task_list(f, cols[0], app);
    render_tag_panel(f, cols[1], app, true);
    render_tag_panel(f, cols[2], app, false);
}

fn render_task_list(f: &mut Frame, area: Rect, app: &mut App) {
    let focused = app.focus == Focus::Tasks && app.mode != Mode::Help;
    let title = format!(
        " Tasks ({}/{}) ",
        if app.visible_count() == 0 {
            0
        } else {
            app.selected + 1
        },
        app.visible_count()
    );
    let block = panel_block(&title, focused);

    // Report the viewport height so PageUp/PageDown match the screen.
    app.viewport_rows = area.height.saturating_sub(2) as usize;

    if app.rows().is_empty() {
        let hint = if app.store.is_empty() {
            vec![
                Line::from(Span::styled(
                    "This task file is empty.",
                    Style::default().add_modifier(Modifier::BOLD),
                )),
                Line::raw(""),
                Line::from("Press `a` to add your first task."),
                Line::from("Press `?` to see every key binding."),
            ]
        } else {
            vec![
                Line::from(Span::styled(
                    "No task matches the current filter.",
                    Style::default().fg(DUE).add_modifier(Modifier::BOLD),
                )),
                Line::raw(""),
                Line::from(format!("Active filter: {}", app.filter_summary())),
                Line::from("Press `F` to clear every filter, or `v` to change visibility."),
            ]
        };
        f.render_widget(
            Paragraph::new(hint).block(block).wrap(Wrap { trim: false }),
            area,
        );
        return;
    }

    // Width available for task text: the panel interior, less the 1-column
    // selection gutter the List widget reserves, less the pinned row gutter.
    const GUTTER: usize = 8; // "nnn " + "[x] "
    let text_width = (area.width as usize)
        .saturating_sub(2 + 1 + GUTTER);

    let mut widest = 0usize;
    let items: Vec<ListItem> = app
        .rows()
        .iter()
        .map(|row| {
            let task = app.store.get(row.task_id).expect("row points at a task");
            let spans = task_spans(task, app.today);
            widest = widest.max(spans_width(&spans));
            let mut line = task_gutter(task, row.file_index);
            line.extend(scroll_spans(spans, app.h_scroll, text_width));
            ListItem::new(Line::from(line))
        })
        .collect();

    // Remember how far right the user may usefully scroll. The `‹`/`›` markers
    // each eat a column, so the limit must exceed the naive difference or the
    // last couple of characters would be unreachable.
    app.max_h_scroll = if widest > text_width {
        widest - text_width.saturating_sub(2)
    } else {
        0
    };

    let mut state = ListState::default();
    state.select(Some(app.selected));

    let list = List::new(items).block(block).highlight_style(
        Style::default()
            .bg(if focused { ACCENT } else { DIM })
            .fg(Color::Black)
            .add_modifier(Modifier::BOLD),
    );
    f.render_stateful_widget(list, area, &mut state);

    // Remember the scroll offset so the detail pane and tests can reason about
    // what is on screen.
    app.list_scroll = state.offset();

    // A scrollbar makes it obvious that content continues past the fold.
    if app.rows().len() > app.viewport_rows && app.viewport_rows > 0 {
        let mut sb_state = ScrollbarState::new(app.rows().len()).position(app.selected);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .style(Style::default().fg(DIM)),
            area.inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb_state,
        );
    }
}

/// The fixed left gutter of a task row: file line number and completion box.
///
/// These stay pinned while the text scrolls horizontally, because the line
/// number is what ties a row to a line in the file and the box is the answer to
/// "is this done?".
fn task_gutter(task: &Task, file_index: usize) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            format!("{:>3} ", file_index + 1),
            Style::default().fg(DIM),
        ),
        Span::styled(
            if task.completed { "[x] " } else { "[ ] " },
            if task.completed {
                Style::default().fg(DONE)
            } else {
                Style::default().fg(ACCENT)
            },
        ),
    ]
}

/// Colourised spans for the scrollable part of a task row: priority and text.
fn task_spans(task: &Task, today: Ymd) -> Vec<Span<'static>> {
    let mut spans = Vec::new();

    let base = if task.completed {
        Style::default().fg(DONE).add_modifier(Modifier::CROSSED_OUT)
    } else {
        Style::default()
    };

    match task.priority() {
        Some(c) => spans.push(Span::styled(
            format!("({c}) "),
            if task.completed {
                base
            } else {
                Style::default()
                    .fg(priority_color(c))
                    .add_modifier(Modifier::BOLD)
            },
        )),
        None => spans.push(Span::styled("    ", base)),
    }

    for (i, tok) in task.tokens().iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" ", base));
        }
        let style = if task.completed {
            base
        } else {
            match classify_token(tok) {
                TokenKind::Project => Style::default().fg(PROJECT),
                TokenKind::Context => Style::default().fg(CONTEXT),
                TokenKind::Due => {
                    let overdue = task
                        .due_date()
                        .is_some_and(|d| date::days_until(d, today) < 0);
                    Style::default()
                        .fg(if overdue { OVERDUE } else { DUE })
                        .add_modifier(if overdue {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        })
                }
                TokenKind::KeyValue => Style::default().fg(DIM),
                TokenKind::Word => base,
            }
        };
        spans.push(Span::styled(tok.clone(), style));
    }

    spans
}

/// Total character width of a span run.
fn spans_width(spans: &[Span]) -> usize {
    spans.iter().map(|s| s.content.chars().count()).sum()
}

/// Drop the first `offset` characters of a span run and clip it to `width`,
/// marking either end that had content removed.
///
/// The `List` widget would silently truncate on the right, which is exactly the
/// failure mode "same-screen visibility" is about: a user cannot tell whether a
/// line ends there or continues. The `‹`/`›` markers say so, and `<`/`>` scroll.
fn scroll_spans(spans: Vec<Span<'static>>, offset: usize, width: usize) -> Vec<Span<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let total = spans_width(&spans);
    let offset = offset.min(total);

    let mut out: Vec<Span<'static>> = Vec::new();
    let mut budget = width;

    if offset > 0 {
        out.push(Span::styled("‹", Style::default().fg(ACCENT)));
        budget -= 1;
    }

    let mut skipped = 0usize;
    let mut clipped_right = false;
    for span in spans {
        let len = span.content.chars().count();
        if skipped + len <= offset {
            skipped += len;
            continue;
        }
        // Partially consumed span: take the tail that survives the offset.
        let take_from = offset.saturating_sub(skipped);
        skipped += len;
        let text: String = span.content.chars().skip(take_from).collect();
        let text_len = text.chars().count();

        if budget == 0 {
            clipped_right = true;
            break;
        }
        if text_len > budget {
            let head: String = text.chars().take(budget.saturating_sub(1)).collect();
            out.push(Span::styled(head, span.style));
            clipped_right = true;
            break;
        }
        budget -= text_len;
        out.push(Span::styled(text, span.style));
    }

    if clipped_right {
        out.push(Span::styled("›", Style::default().fg(ACCENT)));
    }
    out
}

fn priority_color(c: char) -> Color {
    match c {
        'A' => PRIORITY_A,
        'B' => PRIORITY_B,
        'C' => PRIORITY_C,
        _ => PRIORITY_OTHER,
    }
}

/// The projects panel (`projects == true`) or the contexts panel.
fn render_tag_panel(f: &mut Frame, area: Rect, app: &App, projects: bool) {
    let (entries, cursor, active, focus, sigil, name) = if projects {
        (
            app.project_entries(),
            app.project_cursor,
            app.project_filter.clone(),
            Focus::Projects,
            '+',
            "Projects",
        )
    } else {
        (
            app.context_entries(),
            app.context_cursor,
            app.context_filter.clone(),
            Focus::Contexts,
            '@',
            "Contexts",
        )
    };
    let focused = app.focus == focus && app.mode != Mode::Help;
    let key = if projects { "2" } else { "3" };
    let block = panel_block(&format!(" {name} ({key}) "), focused);

    let mut items: Vec<ListItem> = Vec::new();
    let all_selected = active.is_none();
    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            if all_selected { "● " } else { "  " },
            Style::default().fg(ACCENT),
        ),
        Span::styled(
            "all",
            if all_selected {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            },
        ),
        Span::styled(
            format!(" ({})", app.store.len()),
            Style::default().fg(DIM),
        ),
    ])));

    for (tag, count) in &entries {
        let is_active = active
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case(tag));
        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                if is_active { "● " } else { "  " },
                Style::default().fg(ACCENT),
            ),
            Span::styled(
                format!("{sigil}{tag}"),
                if is_active {
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(if projects { PROJECT } else { CONTEXT })
                },
            ),
            Span::styled(format!(" ({count})"), Style::default().fg(DIM)),
        ])));
    }

    if entries.is_empty() {
        items.push(ListItem::new(Line::from(Span::styled(
            format!("  no {sigil}tags yet"),
            Style::default().fg(DIM),
        ))));
    }

    let mut state = ListState::default();
    state.select(Some(cursor.min(items.len().saturating_sub(1))));

    f.render_stateful_widget(
        List::new(items).block(block).highlight_style(
            Style::default()
                .bg(if focused { ACCENT } else { DIM })
                .fg(Color::Black),
        ),
        area,
        &mut state,
    );
}

fn panel_block(title: &str, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Rounded
        })
        .border_style(Style::default().fg(if focused { ACCENT } else { DIM }))
        .title(Span::styled(
            title.to_string(),
            if focused {
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(DIM)
            },
        ))
}

// --------------------------------------------------------------------- detail

/// The selected task, broken into its todo.txt fields.
///
/// This is what makes every field of the current task visible at once without
/// opening anything.
fn render_detail(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM))
        .title(Span::styled(" Selected task ", Style::default().fg(DIM)));

    let Some(task) = app.selected_task() else {
        f.render_widget(
            Paragraph::new(vec![Line::from(Span::styled(
                "Nothing selected.",
                Style::default().fg(DIM),
            ))])
            .block(block),
            area,
        );
        return;
    };

    let inner = block.inner(area);
    f.render_widget(block, area);

    // The raw line is the ground truth against the file, so it gets the full
    // width; the parsed fields share the two columns above it.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50),
            Constraint::Length(2), // a gutter, so wrapped text cannot collide
            Constraint::Min(20),
        ])
        .split(rows[0]);
    let (left_area, right_area) = (cols[0], cols[2]);

    let projects = task.projects();
    let contexts = task.contexts();
    let left = vec![
        Line::from(vec![
            label_span("Line"),
            Span::styled(
                format!(
                    "{} of {}",
                    app.store.index_of(task.id).map(|i| i + 1).unwrap_or(0),
                    app.store.len()
                ),
                Style::default().fg(DIM),
            ),
        ]),
        field_line("Text", &task.summary(), Style::default()),
        field_line(
            "Projects",
            &if projects.is_empty() {
                "none".to_string()
            } else {
                projects
                    .iter()
                    .map(|p| format!("+{p}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            },
            Style::default().fg(if projects.is_empty() { DIM } else { PROJECT }),
        ),
    ];

    let due_style = match task.due_date() {
        Some(d) if date::days_until(d, app.today) < 0 => Style::default().fg(OVERDUE),
        Some(_) => Style::default().fg(DUE),
        None => Style::default().fg(DIM),
    };
    let due_text = match (task.due_raw(), task.due_date()) {
        (Some(raw), Some(d)) => format!("{raw}  ({})", date::relative_wording(d, app.today)),
        (Some(raw), None) => format!("{raw}  (not a valid date)"),
        (None, _) => "none".to_string(),
    };

    let right = vec![
        Line::from(vec![
            label_span("Status"),
            Span::styled(
                if task.completed { "complete" } else { "open" },
                if task.completed {
                    Style::default().fg(DONE)
                } else {
                    Style::default().fg(PRIORITY_C)
                },
            ),
            Span::raw("   "),
            label_span("Priority"),
            match task.priority() {
                Some(c) => Span::styled(
                    format!("({c})"),
                    Style::default()
                        .fg(priority_color(c))
                        .add_modifier(Modifier::BOLD),
                ),
                None => Span::styled("none", Style::default().fg(DIM)),
            },
        ]),
        field_line("Due", &due_text, due_style),
        field_line(
            "Contexts",
            &if contexts.is_empty() {
                "none".to_string()
            } else {
                contexts
                    .iter()
                    .map(|c| format!("@{c}"))
                    .collect::<Vec<_>>()
                    .join(" ")
            },
            Style::default().fg(if contexts.is_empty() { DIM } else { CONTEXT }),
        ),
    ];

    f.render_widget(Paragraph::new(left).wrap(Wrap { trim: false }), left_area);
    f.render_widget(Paragraph::new(right).wrap(Wrap { trim: false }), right_area);
    f.render_widget(
        Paragraph::new(field_line("Raw", &task.render(), Style::default().fg(DIM))),
        rows[1],
    );
}

fn label_span(name: &str) -> Span<'static> {
    Span::styled(format!("{name}: "), Style::default().fg(DIM))
}

fn field_line(name: &str, value: &str, style: Style) -> Line<'static> {
    Line::from(vec![
        label_span(name),
        Span::styled(value.to_string(), style),
    ])
}

// ------------------------------------------------------------- prompt & form

fn render_prompt(f: &mut Frame, area: Rect, app: &App, prompt: &Prompt) {
    let is_confirm = prompt.is_confirm();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(if is_confirm { OVERDUE } else { ACCENT }));

    let mut spans = vec![
        Span::styled(
            format!("{}: ", prompt.label()),
            Style::default()
                .fg(if is_confirm { OVERDUE } else { ACCENT })
                .add_modifier(Modifier::BOLD),
        ),
    ];

    if is_confirm {
        if let Prompt::ConfirmDelete(id) = prompt {
            if let Some(t) = app.store.get(*id) {
                spans.push(Span::raw(t.render()));
            }
        }
        if let Prompt::ConfirmArchive = prompt {
            spans.push(Span::raw(format!(
                "{} completed task(s) → {}",
                app.store.completed_count(),
                app.store.archive_path().display()
            )));
        }
    } else {
        spans.push(Span::raw(app.prompt_input.text().to_string()));
    }

    let inner = block.inner(area);
    f.render_widget(Paragraph::new(Line::from(spans)).block(block), area);

    if !is_confirm {
        // Place the real terminal cursor so typing feels native.
        let prefix = prompt.label().chars().count() + 2;
        let x = inner.x + (prefix + app.prompt_input.cursor_chars()) as u16;
        if x < inner.x + inner.width {
            f.set_cursor_position((x, inner.y));
        }
    }
}

fn render_form(f: &mut Frame, area: Rect, app: &App) {
    let Some(form) = &app.form else { return };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Thick)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" {} ", form.title()),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut rows = vec![Constraint::Length(1); FormField::ALL.len()];
    rows.push(Constraint::Length(1)); // preview
    rows.push(Constraint::Min(1)); // error / hint
    let lines = Layout::default()
        .direction(Direction::Vertical)
        .constraints(rows)
        .split(inner);

    // Wide enough for the longest label ("Description") plus the focus marker
    // and a gap, so a label never abuts its value.
    const LABEL_WIDTH: usize = 15;
    for (i, field) in FormField::ALL.iter().enumerate() {
        let focused = form.field == *field;
        let input = form.input(*field);
        let marker = if focused { "▸" } else { " " };
        let mut spans = vec![
            Span::styled(
                format!("{marker} {:<width$}", field.label(), width = LABEL_WIDTH - 2),
                if focused {
                    Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(DIM)
                },
            ),
            Span::styled(
                input.text().to_string(),
                if focused {
                    Style::default().add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                },
            ),
        ];
        if input.is_empty() {
            spans.push(Span::styled(
                field.hint().to_string(),
                Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
            ));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), lines[i]);

        if focused {
            let x = lines[i].x + (LABEL_WIDTH + input.cursor_chars()) as u16;
            if x < lines[i].x + lines[i].width {
                f.set_cursor_position((x, lines[i].y));
            }
        }
    }

    // Live preview of the exact line that will be written to the file.
    let preview = form_preview(form);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {:<width$}", "Will write", width = LABEL_WIDTH - 2),
                Style::default().fg(DIM),
            ),
            Span::styled(preview, Style::default().fg(PRIORITY_C)),
        ])),
        lines[FormField::ALL.len()],
    );

    let footer = match &form.error {
        Some(e) => Line::from(Span::styled(
            format!("  {e}"),
            Style::default().fg(OVERDUE).add_modifier(Modifier::BOLD),
        )),
        None => Line::from(Span::styled(
            format!("  {}", form.field.hint()),
            Style::default().fg(DIM),
        )),
    };
    f.render_widget(Paragraph::new(footer), lines[FormField::ALL.len() + 1]);
}

/// Best-effort render of the line the form would produce, shown live.
fn form_preview(form: &crate::app::TaskForm) -> String {
    let mut out = String::new();
    let pri = form
        .priority
        .text()
        .trim()
        .trim_start_matches('(')
        .trim_end_matches(')')
        .to_string();
    if let Some(c) = pri.chars().next() {
        if pri.chars().count() == 1 && c.is_ascii_alphabetic() {
            out.push_str(&format!("({}) ", c.to_ascii_uppercase()));
        }
    }
    out.push_str(form.description.text().trim());
    for p in crate::task::parse_tag_list(form.projects.text(), '+') {
        out.push_str(&format!(" +{p}"));
    }
    for c in crate::task::parse_tag_list(form.contexts.text(), '@') {
        out.push_str(&format!(" @{c}"));
    }
    let due = form.due.text().trim();
    if !due.is_empty() {
        out.push_str(&format!(" due:{due}"));
    }
    if out.trim().is_empty() {
        "(nothing yet)".to_string()
    } else {
        out
    }
}

// -------------------------------------------------------------------- footers

fn render_message(f: &mut Frame, area: Rect, app: &App) {
    let line = match &app.message {
        Some(Message { text, kind }) => {
            let (fg, tag) = match kind {
                MessageKind::Info => (ACCENT, "i"),
                MessageKind::Success => (PRIORITY_C, "✓"),
                MessageKind::Warning => (DUE, "!"),
                MessageKind::Error => (OVERDUE, "✗"),
            };
            Line::from(vec![
                Span::styled(
                    format!(" {tag} "),
                    Style::default().fg(Color::Black).bg(fg),
                ),
                Span::raw(" "),
                Span::styled(text.clone(), Style::default().fg(fg)),
            ])
        }
        None => Line::from(Span::styled(
            " Press ? for help".to_string(),
            Style::default().fg(DIM),
        )),
    };
    f.render_widget(Paragraph::new(line), area);
}

/// The key-hint bar.
///
/// Hints are packed onto as many lines as the bar has, because a footer that
/// silently truncates would hide exactly the keys a new user needs (`?` and `q`
/// sit at the end of the list).
fn render_hints(f: &mut Frame, area: Rect, app: &App) {
    if area.height == 0 {
        return;
    }
    let set = match &app.mode {
        Mode::Normal => keys::FOOTER_NORMAL,
        Mode::Help => keys::FOOTER_HELP,
        Mode::Form => keys::FOOTER_FORM,
        Mode::Prompt(p) if p.is_confirm() => keys::FOOTER_CONFIRM,
        Mode::Prompt(_) => keys::FOOTER_PROMPT,
    };

    let width = area.width as usize;
    let capacity = area.height as usize;
    let mut lines: Vec<Line> = Vec::new();
    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    let mut used = 1usize;

    for (key, what) in set {
        // `key what` plus the ` · ` separator that precedes it.
        let sep = if spans.len() > 1 { 3 } else { 0 };
        let cost = sep + key.chars().count() + 1 + what.chars().count();
        if used + cost > width && spans.len() > 1 {
            if lines.len() + 1 == capacity {
                // No room for another line: say so rather than truncate silently.
                spans.push(Span::styled(" …?=help", Style::default().fg(DIM)));
                break;
            }
            lines.push(Line::from(std::mem::take(&mut spans)));
            spans.push(Span::raw(" "));
            used = 1;
        }
        if spans.len() > 1 {
            spans.push(Span::styled(" · ", Style::default().fg(DIM)));
        }
        spans.push(Span::styled(
            (*key).to_string(),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled((*what).to_string(), Style::default().fg(DIM)));
        used += cost;
    }
    if spans.len() > 1 {
        lines.push(Line::from(spans));
    }

    f.render_widget(Paragraph::new(lines), area);
}

// ----------------------------------------------------------------------- help

fn render_help(f: &mut Frame, area: Rect, app: &App) {
    f.render_widget(Clear, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(area);

    let mut text: Vec<Line> = Vec::new();
    for l in keys::HELP_INTRO {
        text.push(Line::from(Span::styled(
            (*l).to_string(),
            Style::default().fg(if l.starts_with("  ") { DIM } else { Color::Reset }),
        )));
    }

    for section in keys::SECTIONS {
        text.push(Line::raw(""));
        text.push(Line::from(Span::styled(
            section.title.to_string(),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )));
        for b in section.bindings {
            text.push(Line::from(vec![
                Span::styled(
                    format!("  {:<16}", b.keys),
                    Style::default()
                        .fg(PRIORITY_B)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(b.what.to_string()),
            ]));
        }
    }

    text.push(Line::raw(""));
    text.push(Line::from(Span::styled(
        format!(
            "Task file: {}   ({} from the {})",
            app.store.path().display(),
            app.store.len(),
            app.file_source.label()
        ),
        Style::default().fg(DIM),
    )));
    text.push(Line::from(Span::styled(
        format!(
            "Visibility is currently `{}`; sort is `{}`.",
            match app.done_filter {
                DoneFilter::All => "all",
                DoneFilter::OpenOnly => "open only",
                DoneFilter::DoneOnly => "done only",
            },
            app.sort_mode.label()
        ),
        Style::default().fg(DIM),
    )));

    let total = text.len();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" tooll {} - keys and format ", crate::config::VERSION),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));

    let viewport = rows[0].height.saturating_sub(2) as usize;
    let max_scroll = total.saturating_sub(viewport) as u16;
    let scroll = app.help_scroll.min(max_scroll);

    f.render_widget(
        Paragraph::new(Text::from(text))
            .block(block)
            .scroll((scroll, 0)),
        rows[0],
    );

    if total > viewport {
        let mut sb = ScrollbarState::new(total).position(scroll as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight).style(Style::default().fg(DIM)),
            rows[0].inner(ratatui::layout::Margin {
                vertical: 1,
                horizontal: 0,
            }),
            &mut sb,
        );
    }

    let hint = if total > viewport {
        format!(
            " line {}/{}  ·  ↑↓ PgUp PgDn scroll  ·  ? / Esc / q back to the list",
            scroll as usize + 1,
            total
        )
    } else {
        " ? / Esc / q back to the list".to_string()
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(hint, Style::default().fg(DIM))))
            .alignment(Alignment::Left),
        rows[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain(s: &str) -> Vec<Span<'static>> {
        vec![Span::raw(s.to_string())]
    }

    fn text(spans: &[Span]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn short_lines_are_untouched() {
        let out = scroll_spans(plain("hello"), 0, 20);
        assert_eq!(text(&out), "hello");
    }

    #[test]
    fn long_lines_are_clipped_with_a_marker() {
        let out = scroll_spans(plain("abcdefghij"), 0, 5);
        // 4 characters plus the `›` marker fills exactly 5 columns.
        assert_eq!(text(&out), "abcd›");
        assert_eq!(spans_width(&out), 5);
    }

    #[test]
    fn scrolling_marks_both_ends() {
        // Both markers plus three characters fill the 5 available columns.
        let out = scroll_spans(plain("abcdefghij"), 3, 5);
        assert_eq!(text(&out), "‹def›");
        assert_eq!(spans_width(&out), 5);
    }

    #[test]
    fn scrolling_to_the_end_drops_the_right_marker() {
        let out = scroll_spans(plain("abcdefghij"), 6, 5);
        assert_eq!(text(&out), "‹ghij");
    }

    #[test]
    fn scroll_offsets_span_boundaries() {
        let spans = vec![
            Span::raw("(A) ".to_string()),
            Span::raw("task".to_string()),
            Span::raw(" +tag".to_string()),
        ];
        assert_eq!(text(&scroll_spans(spans.clone(), 0, 40)), "(A) task +tag");
        // The offset falls inside the second span.
        assert_eq!(text(&scroll_spans(spans.clone(), 6, 40)), "‹sk +tag");
        // Styles survive slicing.
        let styled = vec![
            Span::styled("aaa".to_string(), Style::default().fg(PROJECT)),
            Span::styled("bbb".to_string(), Style::default().fg(CONTEXT)),
        ];
        let out = scroll_spans(styled, 2, 10);
        assert_eq!(text(&out), "‹abbb");
        assert_eq!(out[1].style.fg, Some(PROJECT));
        assert_eq!(out[2].style.fg, Some(CONTEXT));
    }

    #[test]
    fn degenerate_widths_do_not_panic() {
        assert!(scroll_spans(plain("abc"), 0, 0).is_empty());
        assert_eq!(text(&scroll_spans(plain("abc"), 99, 5)), "‹");
        assert_eq!(text(&scroll_spans(Vec::new(), 0, 5)), "");
    }

    #[test]
    fn multibyte_text_is_sliced_on_char_boundaries() {
        let out = scroll_spans(plain("café ☕ done"), 3, 6);
        // Slicing by characters, not bytes; no panic and no mojibake.
        assert!(text(&out).starts_with('‹'));
        assert!(spans_width(&out) <= 6);
    }
}
