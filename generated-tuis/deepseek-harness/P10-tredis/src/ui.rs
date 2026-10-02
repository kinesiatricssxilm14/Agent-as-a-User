//! Rendering for the toolj TUI.

use crate::app::{App, Focus, MessageKind, Mode, Resource};
use crate::redis_client::{KeyKind, StreamEntry, ValueData};
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap};
use ratatui::Frame;

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tabs
            Constraint::Length(1), // context
            Constraint::Min(3),    // main
            Constraint::Length(1), // status / input
        ])
        .split(area);

    draw_tabs(f, app, rows[0]);
    draw_context(f, app, rows[1]);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(rows[2]);
    draw_list(f, app, main[0]);
    draw_detail(f, app, main[1]);

    draw_status(f, app, rows[3]);

    if app.show_help {
        draw_help(f, area);
    }
}

fn draw_tabs(f: &mut Frame, app: &App, area: Rect) {
    let tabs = Tabs::new(vec!["Keys", "Streams", "PubSub", "ACL"])
        .select(app.resource.index())
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .divider(Span::raw("  "));
    f.render_widget(tabs, area);
}

fn draw_context(f: &mut Frame, app: &App, area: Rect) {
    let server = app
        .servers
        .get(app.server_idx)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "none".to_string());

    let conn_style = if app.connected {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Red)
    };
    let conn_text = if app.connected {
        format!("connected (redis {})", app.version)
    } else {
        "disconnected".to_string()
    };

    let mut spans = vec![
        Span::styled(format!(" server: {server} "), Style::default().fg(Color::Yellow)),
        Span::styled(format!(" [{conn_text}] "), conn_style),
        Span::styled(format!(" db:{:<10} ", app.db_size), Style::default().fg(Color::Cyan)),
    ];

    match app.resource {
        Resource::Keys => {
            spans.push(Span::raw(format!("keys: {:<8} ", app.keys.len())));
            spans.push(Span::styled(
                format!("type: {}", app.type_filter.label()),
                Style::default().fg(Color::Magenta),
            ));
            if !app.search.is_empty() {
                spans.push(Span::styled(
                    format!("  filter: \"{}\"", app.search),
                    Style::default().fg(Color::Green),
                ));
            }
        }
        Resource::Streams => spans.push(Span::raw(format!("streams: {}", app.streams.len()))),
        Resource::PubSub => spans.push(Span::raw(format!("channels: {}", app.channels.len()))),
        Resource::Acl => spans.push(Span::raw(format!("users: {}", app.acl_users.len()))),
    }

    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_list(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::List;
    match app.resource {
        Resource::Keys => {
            let items: Vec<ListItem> = app
                .keys
                .iter()
                .map(|k| {
                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{:>6} ", k.kind.label()), kind_style(k.kind)),
                        Span::raw(k.name.clone()),
                    ]))
                })
                .collect();
            let title = format!(" Keys ({} shown / {} total) ", app.keys.len(), app.db_size);
            render_list(f, area, items, title, focused, app.key_cursor, app.keys.len());
        }
        Resource::Streams => {
            let items: Vec<ListItem> = app
                .streams
                .iter()
                .map(|s| {
                    ListItem::new(Line::from(vec![
                        Span::styled(" stream ", kind_style(KeyKind::Stream)),
                        Span::raw(s.name.clone()),
                    ]))
                })
                .collect();
            render_list(
                f,
                area,
                items,
                format!(" Streams ({}) ", app.streams.len()),
                focused,
                app.stream_cursor,
                app.streams.len(),
            );
        }
        Resource::PubSub => {
            let items: Vec<ListItem> = app
                .channels
                .iter()
                .map(|ch| {
                    ListItem::new(Line::from(vec![
                        Span::raw(ch.name.clone()),
                        Span::styled(
                            format!("  ({} subs)", ch.subscribers),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                })
                .collect();
            render_list(
                f,
                area,
                items,
                format!(" PubSub Channels ({}) ", app.channels.len()),
                focused,
                app.channel_cursor,
                app.channels.len(),
            );
        }
        Resource::Acl => {
            let items: Vec<ListItem> = app
                .acl_users
                .iter()
                .map(|u| ListItem::new(Line::from(Span::raw(u.clone()))))
                .collect();
            render_list(
                f,
                area,
                items,
                format!(" ACL Users ({}) ", app.acl_users.len()),
                focused,
                app.acl_cursor,
                app.acl_users.len(),
            );
        }
    }
}

fn render_list<'a>(
    f: &mut Frame,
    area: Rect,
    items: Vec<ListItem<'a>>,
    title: String,
    focused: bool,
    cursor: usize,
    len: usize,
) {
    let border = if focused {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(title).border_style(border))
        .highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");
    let mut state = ListState::default();
    if len > 0 {
        state.select(Some(cursor));
    }
    let height = area.height.saturating_sub(2) as usize;
    *state.offset_mut() = scroll_offset(cursor, height);
    f.render_stateful_widget(list, area, &mut state);
}

fn scroll_offset(cursor: usize, height: usize) -> usize {
    cursor.saturating_sub(height.saturating_sub(1))
}

fn draw_detail(f: &mut Frame, app: &mut App, area: Rect) {
    let (title, lines) = detail_content(app);
    let border = if app.focus == Focus::Detail {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let width = area.width.saturating_sub(2).max(1) as usize;
    let height = area.height.saturating_sub(2) as usize;
    let total = wrapped_count(&lines, width);
    let max_scroll = total.saturating_sub(height);
    app.detail_scroll = app.detail_scroll.min(max_scroll);

    let text = Text::from(lines);
    let para = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(title).border_style(border))
        .wrap(Wrap { trim: false })
        .scroll((app.detail_scroll as u16, 0));
    f.render_widget(para, area);
}

fn wrapped_count(lines: &[Line], width: usize) -> usize {
    let width = width.max(1);
    let mut total = 0usize;
    for l in lines {
        let w: usize = l.spans.iter().map(|s| s.content.chars().count()).sum::<usize>().max(1);
        total += (w + width - 1) / width;
    }
    total
}

fn detail_content(app: &App) -> (String, Vec<Line<'static>>) {
    match app.resource {
        Resource::Keys => key_detail(app),
        Resource::Streams => stream_detail(app),
        Resource::PubSub => channel_detail(app),
        Resource::Acl => acl_detail(app),
    }
}

fn key_detail(app: &App) -> (String, Vec<Line<'static>>) {
    let key = app.selected_key.as_deref().unwrap_or("(none)");
    match &app.detail_data {
        None => (
            format!(" value — {key} "),
            vec![Line::from(Span::raw("select a key to view its value"))],
        ),
        Some(ValueData::None) => (
            format!(" value — {key} "),
            vec![Line::from(Span::styled(
                "(key does not exist)",
                Style::default().fg(Color::Red),
            ))],
        ),
        Some(ValueData::String(v)) => {
            let mut lines = Vec::new();
            for l in v.split('\n') {
                lines.push(Line::from(Span::raw(l.to_string())));
            }
            if lines.is_empty() {
                lines.push(Line::from(Span::raw("")));
            }
            (format!(" string — {key} "), lines)
        }
        Some(ValueData::Hash(pairs)) => {
            let mut pairs = pairs.clone();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            let mut lines = Vec::new();
            for (f, v) in &pairs {
                lines.push(Line::from(vec![
                    Span::styled(f.clone(), Style::default().fg(Color::Yellow)),
                    Span::raw("  "),
                    Span::raw(v.clone()),
                ]));
            }
            if lines.is_empty() {
                lines.push(Line::from(Span::raw("(empty hash)")));
            }
            (format!(" hash — {key} ({} fields) ", pairs.len()), lines)
        }
        Some(ValueData::List(items)) => {
            let mut lines = Vec::new();
            for (i, el) in items.iter().enumerate() {
                lines.push(Line::from(vec![
                    Span::styled(format!("{:>4} ", i), Style::default().fg(Color::DarkGray)),
                    Span::raw(el.clone()),
                ]));
            }
            if lines.is_empty() {
                lines.push(Line::from(Span::raw("(empty list)")));
            }
            (format!(" list — {key} ({} elements) ", items.len()), lines)
        }
        Some(ValueData::Set(items)) => {
            let mut items = items.clone();
            items.sort();
            let mut lines = Vec::new();
            for m in &items {
                lines.push(Line::from(Span::raw(m.clone())));
            }
            if lines.is_empty() {
                lines.push(Line::from(Span::raw("(empty set)")));
            }
            (format!(" set — {key} ({} members) ", items.len()), lines)
        }
        Some(ValueData::ZSet(items)) => {
            let mut lines = Vec::new();
            for (m, s) in items {
                lines.push(Line::from(vec![
                    Span::raw(m.clone()),
                    Span::styled(format!("  {}", s), Style::default().fg(Color::Cyan)),
                ]));
            }
            if lines.is_empty() {
                lines.push(Line::from(Span::raw("(empty sorted set)")));
            }
            (format!(" sorted set — {key} ({} members) ", items.len()), lines)
        }
        Some(ValueData::Stream(entries)) => (
            format!(" stream — {key} ({} entries) ", entries.len()),
            stream_entry_lines(entries),
        ),
    }
}

fn stream_detail(app: &App) -> (String, Vec<Line<'static>>) {
    let name = app
        .streams
        .get(app.stream_cursor)
        .map(|s| s.name.clone())
        .unwrap_or_else(|| "(none)".to_string());
    let lines = stream_entry_lines(&app.stream_entries);
    (format!(" stream — {name} ({} entries) ", app.stream_entries.len()), lines)
}

fn stream_entry_lines(entries: &[StreamEntry]) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    for e in entries {
        let fields: Vec<String> = e.fields.iter().map(|(f, v)| format!("{f}={v}")).collect();
        lines.push(Line::from(vec![
            Span::styled(e.id.clone(), Style::default().fg(Color::Green)),
            Span::raw("  "),
            Span::raw(fields.join("  ")),
        ]));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::raw("(no messages)")));
    }
    lines
}

fn channel_detail(app: &App) -> (String, Vec<Line<'static>>) {
    match app.channels.get(app.channel_cursor) {
        None => (" channel ".to_string(), vec![Line::from(Span::raw("no channels"))]),
        Some(ch) => {
            let lines = vec![
                Line::from(Span::styled(
                    format!("channel: {}", ch.name),
                    Style::default().fg(Color::Yellow),
                )),
                Line::from(Span::raw(format!("subscribers: {}", ch.subscribers))),
                Line::from(Span::raw("")),
                Line::from(Span::raw("A channel is listed while at least one client")),
                Line::from(Span::raw("is SUBSCRIBEd / PSUBSCRIBEd to it (PUBSUB CHANNELS).")),
            ];
            (format!(" channel — {} ", ch.name), lines)
        }
    }
}

fn acl_detail(app: &App) -> (String, Vec<Line<'static>>) {
    let name = app
        .acl_users
        .get(app.acl_cursor)
        .cloned()
        .unwrap_or_else(|| "(none)".to_string());
    let lines: Vec<Line> = app.acl_detail.iter().map(|l| Line::from(Span::raw(l.clone()))).collect();
    (format!(" ACL — {name} "), lines)
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    match app.mode {
        Mode::Normal => {
            let mut spans = Vec::new();
            if let Some((m, kind)) = &app.message {
                let style = if *kind == MessageKind::Info {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                };
                spans.push(Span::styled(format!(" {m} "), style));
            }
            spans.push(Span::styled(
                " q:quit  ?:help  Tab:switch  ←/→:focus  ↑/↓:move  Enter:view  e:edit  a:add  d:del  /:search  r:refresh  ::cmd ",
                Style::default().fg(Color::DarkGray),
            ));
            f.render_widget(Paragraph::new(Line::from(spans)), area);
        }
        _ => {
            let mut spans = Vec::new();
            spans.push(Span::styled(
                app.input_prompt.clone(),
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(app.input.clone()));
            spans.push(Span::styled("▊", Style::default().fg(Color::Yellow)));
            f.render_widget(Paragraph::new(Line::from(spans)), area);
        }
    }
}

fn draw_help(f: &mut Frame, area: Rect) {
    let help = r#"toolj — Redis database management TUI

Navigation
  Tab / Shift+Tab      switch resource (:keys, :streams, :pubsub, :acl)
  ← / →  (h / l)       move focus between list and detail panels
  ↑ / ↓  (k / j)       move selection / scroll detail
  PgUp / PgDn          page scroll
  Home / End  (g / G)  jump to top / bottom
  Enter                view selected item / toggle focus

Keys view
  1-7                  type filter (1 all, 2 string, 3 hash, 4 list, 5 set, 6 zset, 7 stream)
  /                    live search filter (Esc cancels, Enter accepts)
  e                    edit selected key (string = value; others = add item)
  a                    create new key / add item (detail focused)
  d                    delete key / delete item (detail focused)
  r                    refresh

Server
  s                    add named server (name + Redis URI)
  c                    connect / disconnect
  [ ]                  previous / next server

Commands (press :)
  keys streams pubsub acl   switch resource
  connect disconnect refresh help quit add-server

Other
  ? / F1               toggle this help
  q / Ctrl+C           quit
"#;
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" help (?/Esc to close) ")
        .border_style(Style::default().fg(Color::Cyan));
    let para = Paragraph::new(help).block(block).wrap(Wrap { trim: false });

    let width = area.width.min(88);
    let height = area.height.min(30);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let rect = Rect::new(x, y, width, height);
    f.render_widget(Clear, rect);
    f.render_widget(para, rect);
}

fn kind_style(kind: KeyKind) -> Style {
    let color = match kind {
        KeyKind::String => Color::Green,
        KeyKind::Hash => Color::Yellow,
        KeyKind::List => Color::Cyan,
        KeyKind::Set => Color::Magenta,
        KeyKind::ZSet => Color::Blue,
        KeyKind::Stream => Color::LightRed,
        KeyKind::None | KeyKind::Unknown => Color::DarkGray,
    };
    Style::default().fg(color)
}
