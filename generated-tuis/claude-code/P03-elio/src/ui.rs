//! Rendering. Everything is laid out as tiled regions — the file list and the
//! preview are always on screen together, and prompts, confirmations, help and
//! the activity log take their own rows/columns instead of floating over the
//! panes. No information is ever hidden behind a modal or a tab.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Borders, Cell, Clear, List, ListItem, ListState, Padding, Paragraph, Row,
    Scrollbar, ScrollbarOrientation, ScrollbarState, Table, Wrap,
};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Confirm, Focus, Level, PromptKind};
use crate::fs_ops::human_size;
use crate::listing::{format_time, Kind};
use crate::preview::PreviewKind;

// ---- palette -----------------------------------------------------------

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const DIR_COLOR: Color = Color::LightBlue;
const LINK_COLOR: Color = Color::Magenta;
const OK: Color = Color::Green;
const BAD: Color = Color::Red;
const WARN: Color = Color::Yellow;

fn focused_border(focused: bool) -> Style {
    if focused {
        Style::default().fg(ACCENT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(DIM)
    }
}

/// Draw a vertical scrollbar on a bordered block's right edge.
///
/// The track is inset by one row top and bottom so it rides the border line
/// without overwriting the block's corners or its title.
fn draw_scrollbar(frame: &mut Frame, area: Rect, total: usize, position: usize) {
    if area.height < 3 {
        return;
    }
    let track = Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: area.height - 2,
    };
    let mut state = ScrollbarState::new(total).position(position);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None)
            .track_symbol(None)
            .thumb_style(Style::default().fg(ACCENT)),
        track,
        &mut state,
    );
}

/// Top-level draw: header, panes, prompt/confirm lines, key bar.
pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    if area.width < 40 || area.height < 10 {
        let msg = Paragraph::new(vec![
            Line::from("Terminal too small"),
            Line::from(format!(
                "{}x{} — need at least 40x10",
                area.width, area.height
            )),
        ])
        .alignment(Alignment::Center)
        .style(Style::default().fg(WARN));
        frame.render_widget(Clear, area);
        frame.render_widget(msg, area);
        return;
    }

    // Reserve rows bottom-up: key bar, status, and the optional prompt/confirm.
    let mut bottom = vec![
        Constraint::Length(1), // status line
        Constraint::Length(2), // key bar (two rows of shortcuts)
    ];
    let interactive_rows = if app.prompt.is_some() || app.confirm.is_some() {
        bottom.insert(0, Constraint::Length(3));
        1
    } else {
        0
    };

    let mut constraints = vec![Constraint::Length(3), Constraint::Min(6)];
    constraints.extend(bottom);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let header = rows[0];
    let body = rows[1];
    let interactive = if interactive_rows == 1 {
        Some(rows[2])
    } else {
        None
    };
    let status = rows[2 + interactive_rows];
    let keybar = rows[3 + interactive_rows];

    draw_header(frame, app, header);
    draw_body(frame, app, body);
    if let Some(area) = interactive {
        draw_interactive(frame, app, area);
    }
    draw_status(frame, app, status);
    draw_keybar(frame, app, keybar);
}

// ---- header ------------------------------------------------------------

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM))
        .title(Span::styled(
            " toolc — file manager ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));

    let (dirs, files) = app.listing.counts();
    let mut spans = vec![
        Span::styled("cwd ", Style::default().fg(DIM)),
        Span::styled(
            app.cwd().display().to_string(),
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{dirs} dir · {files} file"),
            Style::default().fg(DIM),
        ),
        Span::raw("  "),
        Span::styled(
            format!(
                "sort:{}{}",
                app.listing.sort_key.label(),
                if app.listing.sort_reverse {
                    "↓"
                } else {
                    "↑"
                }
            ),
            Style::default().fg(DIM),
        ),
    ];
    if app.listing.show_hidden {
        spans.push(Span::raw("  "));
        spans.push(Span::styled("hidden:on", Style::default().fg(WARN)));
    }
    if !app.listing.filter.is_empty() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            format!("filter:{}", app.listing.filter),
            Style::default().fg(WARN).add_modifier(Modifier::BOLD),
        ));
    }
    let para = Paragraph::new(Line::from(spans)).block(block);
    frame.render_widget(para, area);
}

// ---- body: list | preview | (help) -------------------------------------

fn draw_body(frame: &mut Frame, app: &mut App, area: Rect) {
    // The help panel is a third column, so the list and preview stay visible.
    // 48 columns fits the widest "key  description" pair without wrapping.
    let help_width = if app.show_help {
        48.min(area.width / 3)
    } else {
        0
    };

    let columns = if help_width > 0 {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(34),
                Constraint::Min(20),
                Constraint::Length(help_width),
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Min(20)])
            .split(area)
    };

    // The activity log shares the left column below the list when enabled.
    let left = if app.show_activity {
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Length(8)])
            .split(columns[0]);
        draw_activity(frame, app, split[1]);
        split[0]
    } else {
        columns[0]
    };

    draw_list(frame, app, left);
    draw_preview(frame, app, columns[1]);
    if help_width > 0 {
        draw_help(frame, app, columns[2]);
    }
}

fn draw_list(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::List;
    let title = Line::from(vec![
        Span::styled(
            " Files ",
            Style::default()
                .fg(if focused { ACCENT } else { Color::White })
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if app.listing.filter.is_empty() {
                format!("[{}] ", app.listing.len())
            } else {
                format!("[{}/{}] ", app.listing.len(), app.listing.total())
            },
            Style::default().fg(DIM),
        ),
    ]);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(focused_border(focused))
        .title(title);

    if let Some(err) = &app.listing.error {
        let para = Paragraph::new(err.as_str())
            .style(Style::default().fg(BAD))
            .wrap(Wrap { trim: true })
            .block(block);
        frame.render_widget(para, area);
        return;
    }

    let inner = block.inner(area);
    app.list_viewport = inner.height.max(1);

    if app.listing.is_empty() {
        let msg = if app.listing.total() == 0 {
            "empty directory"
        } else {
            "no match for the current filter (Esc clears it)"
        };
        let para = Paragraph::new(msg)
            .style(Style::default().fg(DIM).add_modifier(Modifier::ITALIC))
            .block(block);
        frame.render_widget(para, area);
        return;
    }

    // Size column is only worth showing when there is room for it.
    let show_size = inner.width >= 34;
    let name_width = inner.width.saturating_sub(if show_size { 13 } else { 4 }) as usize;

    let items: Vec<ListItem> = app
        .listing
        .iter()
        .map(|entry| {
            let (color, marker) = match entry.kind {
                Kind::Dir => (DIR_COLOR, "/"),
                Kind::Symlink => (LINK_COLOR, "@"),
                Kind::Other => (WARN, "?"),
                Kind::File => (Color::White, ""),
            };
            let label = format!("{}{}", entry.name, marker);
            let mut spans = vec![Span::styled(
                truncate_end(&label, name_width.max(4)),
                Style::default().fg(color),
            )];
            if show_size {
                let pad = name_width
                    .saturating_sub(display_width(&truncate_end(&label, name_width.max(4))));
                spans.push(Span::raw(" ".repeat(pad + 1)));
                let size = match entry.size {
                    Some(bytes) => human_size(bytes),
                    None => "—".into(),
                };
                spans.push(Span::styled(
                    format!("{size:>10}"),
                    Style::default().fg(DIM),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(if focused { ACCENT } else { DIM })
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("");

    let mut state = ListState::default();
    state.select(Some(app.selected));
    *state.offset_mut() = app.list_offset;
    frame.render_stateful_widget(list, area, &mut state);
    app.list_offset = state.offset();

    if app.listing.len() > inner.height as usize {
        draw_scrollbar(frame, area, app.listing.len(), app.selected);
    }
}

fn draw_preview(frame: &mut Frame, app: &mut App, area: Rect) {
    // The preview column stacks: metadata table, then the content viewer.
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(6), Constraint::Min(3)])
        .split(area);
    draw_details(frame, app, rows[0]);
    draw_content(frame, app, rows[1]);
}

fn draw_details(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM))
        .title(Span::styled(
            " Selected ",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));

    let Some(entry) = app.selected_entry() else {
        let para = Paragraph::new("nothing selected")
            .style(Style::default().fg(DIM))
            .block(block);
        frame.render_widget(para, area);
        return;
    };

    let kind = match entry.kind {
        Kind::Dir => "directory",
        Kind::File => "regular file",
        Kind::Symlink => "symlink",
        Kind::Other => "special",
    };
    let size = match entry.size {
        Some(bytes) => format!("{} ({} bytes)", human_size(bytes), bytes),
        None => "—".into(),
    };
    let mut path_line = entry.path.display().to_string();
    if let Some(target) = &entry.link_target {
        path_line.push_str(&format!("  ->  {}", target.display()));
    }

    let label = Style::default().fg(DIM);
    let value = Style::default().fg(Color::White);
    let rows = vec![
        Row::new(vec![
            Cell::from(Span::styled("name", label)),
            Cell::from(Span::styled(
                entry.name.clone(),
                Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
            )),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("path", label)),
            Cell::from(Span::styled(path_line, value)),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("type", label)),
            Cell::from(Span::styled(
                format!("{kind} · {} · {size}", entry.permissions()),
                value,
            )),
        ]),
        Row::new(vec![
            Cell::from(Span::styled("mtime", label)),
            Cell::from(Span::styled(format_time(entry.modified), value)),
        ]),
    ];

    let table = Table::new(rows, [Constraint::Length(6), Constraint::Min(10)])
        .block(block)
        .column_spacing(1);
    frame.render_widget(table, area);
}

fn draw_content(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Preview;
    let preview = &app.preview;

    let kind_label = match preview.kind {
        PreviewKind::Text => "text",
        PreviewKind::Binary => "binary",
        PreviewKind::Directory => "dir",
        PreviewKind::Empty => "empty",
        PreviewKind::Error => "error",
        PreviewKind::Nothing => "—",
    };

    let mut title = vec![Span::styled(
        " Preview ",
        Style::default()
            .fg(if focused { ACCENT } else { Color::White })
            .add_modifier(Modifier::BOLD),
    )];
    if !preview.name.is_empty() {
        title.push(Span::styled(
            format!("{} ", preview.name),
            Style::default().fg(ACCENT),
        ));
    }
    title.push(Span::styled(
        format!("({kind_label}) "),
        Style::default().fg(DIM),
    ));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(focused_border(focused))
        .title(Line::from(title))
        .title_bottom(Line::from(Span::styled(
            if preview.info.is_empty() {
                String::new()
            } else {
                format!(" {} ", preview.info)
            },
            Style::default().fg(DIM),
        )))
        .padding(Padding::horizontal(1));

    let inner = block.inner(area);
    app.preview_viewport = inner.height.max(1);

    let base = match app.preview.kind {
        PreviewKind::Error => Style::default().fg(BAD),
        PreviewKind::Nothing | PreviewKind::Empty => {
            Style::default().fg(DIM).add_modifier(Modifier::ITALIC)
        }
        PreviewKind::Binary => Style::default().fg(Color::Rgb(160, 200, 160)),
        _ => Style::default().fg(Color::White),
    };

    // Wrap the text ourselves so the row count, the scroll offset and the
    // scrollbar all describe the same rows the user is looking at.
    let total = app.preview.layout(inner.width).len();

    // Clamp the stored scroll to what this viewport allows, so resizing the
    // terminal can never leave the view stranded past the last row.
    let max_scroll = app.preview.max_scroll(app.preview_viewport);
    if app.preview.scroll > max_scroll {
        app.preview.scroll = max_scroll;
    }
    let preview = &app.preview;

    // Only the visible slice needs to be built into widget lines.
    let first = preview.scroll as usize;
    let rows = preview.rows_slice(first, inner.height as usize);
    let lines: Vec<Line> = rows
        .iter()
        .map(|row| Line::from(Span::styled(row.clone(), base)))
        .collect();

    let mut para = Paragraph::new(lines).block(block);
    if !preview.wrap {
        // With wrap off, long rows scroll sideways instead.
        para = para.scroll((0, preview.hscroll));
    }
    frame.render_widget(para, area);

    if total > inner.height as usize {
        draw_scrollbar(frame, area, total, preview.scroll as usize);
    }
}

fn draw_activity(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM))
        .title(Span::styled(
            format!(" Activity [{}] ", app.activity.len()),
            Style::default().fg(Color::White),
        ));
    let inner_h = block.inner(area).height as usize;
    let lines: Vec<Line> = if app.activity.is_empty() {
        vec![Line::from(Span::styled(
            "no filesystem changes yet",
            Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
        ))]
    } else {
        app.activity
            .iter()
            .rev()
            .take(inner_h)
            .map(|l| Line::from(Span::styled(l.clone(), Style::default().fg(OK))))
            .collect()
    };
    frame.render_widget(Paragraph::new(lines).block(block), area);
}

// ---- prompt & confirmation --------------------------------------------

fn draw_interactive(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(confirm) = &app.confirm {
        let (question, detail) = match confirm {
            Confirm::Delete { path, is_dir } => (
                format!(
                    "Delete {} {}?",
                    if *is_dir { "directory" } else { "file" },
                    path.display()
                ),
                if *is_dir {
                    "the directory and everything inside it will be removed"
                } else {
                    "this cannot be undone"
                },
            ),
            Confirm::Overwrite { dest, .. } => (
                format!("Overwrite existing {}?", dest.display()),
                "the current contents of that path will be replaced",
            ),
        };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Double)
            .border_style(Style::default().fg(BAD).add_modifier(Modifier::BOLD))
            .title(Span::styled(
                " Confirm ",
                Style::default().fg(BAD).add_modifier(Modifier::BOLD),
            ));
        let text = Line::from(vec![
            Span::styled(
                question,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled("[y]", Style::default().fg(OK).add_modifier(Modifier::BOLD)),
            Span::styled(" yes  ", Style::default().fg(DIM)),
            Span::styled(
                "[n/Esc]",
                Style::default().fg(WARN).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" no  ·  ", Style::default().fg(DIM)),
            Span::styled(detail, Style::default().fg(DIM)),
        ]);
        frame.render_widget(Paragraph::new(text).block(block), area);
        return;
    }

    let Some(prompt) = &app.prompt else { return };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            format!(" {} ", prompt.kind.title()),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Line::from(Span::styled(
            if prompt.kind.is_live() {
                " Enter keep · Esc clear · list updates as you type ".to_string()
            } else {
                " Enter confirm · Esc cancel · Tab complete path · Ctrl-W delete segment "
                    .to_string()
            },
            Style::default().fg(DIM),
        )));

    let subject = prompt
        .subject
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned());

    let mut spans = Vec::new();
    if let Some(name) = subject {
        if !prompt.kind.is_live()
            && prompt.kind != PromptKind::MkDir
            && prompt.kind != PromptKind::Jump
        {
            spans.push(Span::styled(format!("{name} → "), Style::default().fg(DIM)));
        }
    }
    spans.push(Span::styled(
        prompt.input.clone(),
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    ));

    let inner = block.inner(area);
    let prefix_width: usize = spans[..spans.len() - 1]
        .iter()
        .map(|s| display_width(&s.content))
        .sum();
    let cursor_col =
        prefix_width + display_width(&prompt.input.chars().take(prompt.cursor).collect::<String>());

    frame.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
    // A real terminal cursor is friendlier than a drawn block for text entry.
    let x = inner.x + (cursor_col as u16).min(inner.width.saturating_sub(1));
    frame.set_cursor_position((x, inner.y));
}

// ---- status & key bar --------------------------------------------------

fn draw_status(frame: &mut Frame, app: &App, area: Rect) {
    let (fg, tag) = match app.status.level {
        Level::Info => (Color::White, " i "),
        Level::Success => (OK, " ok "),
        Level::Error => (BAD, " ! "),
    };
    let line = Line::from(vec![
        Span::styled(
            tag,
            Style::default()
                .bg(match app.status.level {
                    Level::Info => DIM,
                    Level::Success => OK,
                    Level::Error => BAD,
                })
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(app.status.text.clone(), Style::default().fg(fg)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

/// Two dense rows of shortcuts, always visible so keys are discoverable
/// without opening anything.
fn draw_keybar(frame: &mut Frame, app: &App, area: Rect) {
    let key = Style::default()
        .fg(Color::Black)
        .bg(ACCENT)
        .add_modifier(Modifier::BOLD);
    let danger = Style::default()
        .fg(Color::Black)
        .bg(BAD)
        .add_modifier(Modifier::BOLD);
    let text = Style::default().fg(Color::Gray);

    let pair = |k: &str, label: &str, style: Style| {
        vec![
            Span::styled(format!(" {k} "), style),
            Span::styled(format!(" {label}  "), text),
        ]
    };

    let (row1, row2) = if app.prompt.is_some() {
        (
            [
                pair("Enter", "confirm", key),
                pair("Esc", "cancel", key),
                pair("Tab", "complete path", key),
                pair("←→", "move cursor", key),
            ]
            .concat(),
            [
                pair("Ctrl-W", "delete segment", key),
                pair("Ctrl-U", "clear line", key),
                pair("Home/End", "line start/end", key),
            ]
            .concat(),
        )
    } else if app.confirm.is_some() {
        (
            [
                pair("y", "yes, do it", danger),
                pair("n", "no", key),
                pair("Esc", "cancel", key),
            ]
            .concat(),
            vec![Span::styled(
                "  a confirmation is pending — answer it to continue",
                Style::default().fg(WARN),
            )],
        )
    } else {
        (
            [
                pair("↑↓/jk", "move", key),
                pair("Enter/→", "open dir", key),
                pair("←/u", "up", key),
                pair("Tab", "switch pane", key),
                pair("c", "copy", key),
                pair("m", "move", key),
                pair("r", "rename", key),
                pair("n", "new dir", key),
            ]
            .concat(),
            [
                pair("d", "delete", danger),
                pair("/", "filter", key),
                pair("g", "go to path", key),
                pair("H", "hidden", key),
                pair("s", "sort", key),
                pair("w", "wrap", key),
                pair("l", "activity", key),
                pair("F5", "reload", key),
                pair("?", "help", key),
                pair("q", "quit", key),
            ]
            .concat(),
        )
    };

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);
    frame.render_widget(Paragraph::new(Line::from(row1)), rows[0]);
    frame.render_widget(Paragraph::new(Line::from(row2)), rows[1]);
}

// ---- help panel --------------------------------------------------------

/// Every binding, grouped. Rendered as a side column so the list and preview
/// remain on screen while the user reads it.
const HELP: &[(&str, &[(&str, &str)])] = &[
    (
        "Navigate",
        &[
            ("↑ / k", "previous entry"),
            ("↓ / j", "next entry"),
            ("PgUp / PgDn", "page up / down"),
            ("Home / End", "first / last entry"),
            ("Enter / → / l", "open selected directory"),
            ("← / u / Bksp", "go to parent directory"),
            ("g", "go to any path"),
            ("~", "go to $HOME"),
            ("Tab", "switch focus: list/preview"),
        ],
    ),
    (
        "Preview",
        &[
            ("↑ ↓ (in preview)", "scroll one line"),
            ("PgUp / PgDn", "scroll one screen"),
            ("Home / End", "jump to top / bottom"),
            ("Shift-←/→", "scroll horizontally"),
            ("w", "toggle soft wrap"),
        ],
    ),
    (
        "File operations",
        &[
            ("c", "copy to a destination"),
            ("m", "move into a directory"),
            ("r", "rename the selected entry"),
            ("n", "create a new directory"),
            ("d / Delete", "delete (asks to confirm)"),
        ],
    ),
    (
        "View",
        &[
            ("/", "filter list by substring"),
            ("Esc", "clear the filter"),
            ("H", "show / hide dotfiles"),
            ("s", "cycle sort: name/size/time"),
            ("S", "reverse the sort order"),
            ("l", "toggle the activity log"),
            ("F5 / Ctrl-R", "re-read the directory"),
        ],
    ),
    (
        "General",
        &[
            ("?  / F1", "toggle this help"),
            ("Ctrl-↑ / Ctrl-↓", "scroll this help column"),
            ("q / Ctrl-C", "quit"),
        ],
    ),
];

fn draw_help(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(ACCENT))
        .title(Span::styled(
            " Keys ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
        .title_bottom(Line::from(Span::styled(
            " Ctrl-↑/↓ scroll · ? closes ",
            Style::default().fg(DIM),
        )))
        .padding(Padding::horizontal(1));

    let key_style = Style::default().fg(WARN);
    let desc_style = Style::default().fg(Color::Gray);
    let mut lines: Vec<Line> = Vec::new();
    for (group, entries) in HELP {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(
            *group,
            Style::default()
                .fg(ACCENT)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        )));
        for (k, d) in *entries {
            lines.push(Line::from(vec![
                Span::styled(format!("{k:<18}"), key_style),
                Span::styled(*d, desc_style),
            ]));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Paths may be relative or absolute; ~ expands to $HOME. Tab completes them.",
        Style::default().fg(DIM).add_modifier(Modifier::ITALIC),
    )));

    let inner = block.inner(area);
    // Estimate wrapped height so the scroll limit matches what is drawn.
    let wrapped: usize = lines
        .iter()
        .map(|l| {
            let w = l.width().max(1);
            w.div_ceil(inner.width.max(1) as usize)
        })
        .sum();
    app.help_max_scroll = (wrapped as u16).saturating_sub(inner.height.max(1));
    if app.help_scroll > app.help_max_scroll {
        app.help_scroll = app.help_max_scroll;
    }

    let para = Paragraph::new(lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((app.help_scroll, 0));
    frame.render_widget(para, area);

    if app.help_max_scroll > 0 {
        draw_scrollbar(frame, area, wrapped, app.help_scroll as usize);
    }
}

// ---- text helpers ------------------------------------------------------

fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Truncate to `width` display columns, marking the cut with `…`.
fn truncate_end(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }
    if width <= 1 {
        return "…".into();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let w = UnicodeWidthStr::width(ch.to_string().as_str());
        if used + w > width - 1 {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncation_respects_width() {
        assert_eq!(truncate_end("short", 10), "short");
        assert_eq!(truncate_end("abcdefgh", 4), "abc…");
        assert_eq!(truncate_end("abc", 1), "…");
        assert_eq!(display_width(&truncate_end("abcdefgh", 4)), 4);
    }

    #[test]
    fn wide_characters_are_measured_in_columns() {
        assert_eq!(display_width("English-only text"), 6);
        assert!(display_width(&truncate_end("English-only textテスト", 5)) <= 5);
    }

    #[test]
    fn help_covers_every_advertised_group() {
        let groups: Vec<&str> = HELP.iter().map(|(g, _)| *g).collect();
        assert!(groups.contains(&"File operations"));
        let all: Vec<&str> = HELP
            .iter()
            .flat_map(|(_, e)| e.iter().map(|(k, _)| *k))
            .collect();
        for expected in ["c", "m", "r", "n", "d / Delete", "/", "q / Ctrl-C"] {
            assert!(all.contains(&expected), "missing binding {expected}");
        }
    }
}
