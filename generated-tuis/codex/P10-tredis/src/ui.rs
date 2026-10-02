use crate::app::{App, Focus, FormStage, InputMode, KeyType, Resource};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs, Wrap},
    Frame,
};

const CYAN: Color = Color::Rgb(62, 207, 190);
const BG: Color = Color::Rgb(17, 24, 31);
const PANEL: Color = Color::Rgb(25, 35, 44);
const MUTED: Color = Color::Rgb(130, 151, 164);
pub fn draw(frame: &mut Frame, app: &App) {
    frame.render_widget(
        Block::default().style(Style::default().bg(BG)),
        frame.area(),
    );
    if !app.is_connected() {
        draw_servers(frame, app);
        return;
    }
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(input_height(app)),
            Constraint::Length(2),
        ])
        .split(frame.area());
    draw_header(frame, app, rows[0]);
    draw_main(frame, app, rows[1]);
    draw_input(frame, app, rows[2]);
    draw_footer(frame, app, rows[3]);
    if app.show_help {
        draw_help(frame);
    }
}
fn draw_servers(frame: &mut Frame, app: &App) {
    let area = centered(78, 72, frame.area());
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(8),
            Constraint::Length(input_height(app)),
            Constraint::Length(3),
        ])
        .split(area);
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                " toolj ",
                Style::default()
                    .fg(BG)
                    .bg(CYAN)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " Redis database management",
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray)),
        ),
        rows[0],
    );
    let list = List::new(app.servers.iter().map(|s| {
        ListItem::new(vec![
            Line::styled(
                &s.name,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
            Line::styled(&s.uri, Style::default().fg(MUTED)),
        ])
    }))
    .block(
        Block::default()
            .title(" Servers ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(CYAN)),
    )
    .highlight_style(Style::default().bg(Color::Rgb(38, 57, 68)))
    .highlight_symbol(" ▶ ");
    let mut state = ListState::default().with_selected(Some(app.server_selected));
    frame.render_stateful_widget(list, rows[1], &mut state);
    draw_input(frame, app, rows[2]);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Enter", key()),
                Span::raw(" connect   "),
                Span::styled("n", key()),
                Span::raw(" add server   "),
                Span::styled("↑↓", key()),
                Span::raw(" select   "),
                Span::styled("?", key()),
                Span::raw(" help   "),
                Span::styled("q", key()),
                Span::raw(" quit"),
            ]),
            Line::styled(&app.status, Style::default().fg(MUTED)),
        ]),
        rows[3],
    );
    if app.show_help {
        draw_help(frame)
    }
}
fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let resources = [
        Resource::Keys,
        Resource::Streams,
        Resource::PubSub,
        Resource::Acl,
    ];
    let titles = resources
        .iter()
        .map(|r| Line::from(r.label()))
        .collect::<Vec<_>>();
    let selected = resources
        .iter()
        .position(|r| *r == app.resource)
        .unwrap_or(0);
    let server = app
        .connected
        .and_then(|i| app.servers.get(i))
        .map(|s| s.name.as_str())
        .unwrap_or("");
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(45),
            Constraint::Length((server.len() + 5).min(30) as u16),
        ])
        .split(area);
    frame.render_widget(
        Tabs::new(titles)
            .select(selected)
            .style(Style::default().fg(MUTED))
            .highlight_style(Style::default().fg(CYAN).add_modifier(Modifier::BOLD))
            .divider(" │ ")
            .padding(" ", " ")
            .block(Block::default().borders(Borders::BOTTOM)),
        chunks[0],
    );
    frame.render_widget(
        Paragraph::new(format!("● {server}"))
            .alignment(ratatui::layout::Alignment::Right)
            .style(Style::default().fg(Color::Green))
            .block(Block::default().borders(Borders::BOTTOM)),
        chunks[1],
    );
}
fn draw_main(frame: &mut Frame, app: &App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
        .split(area);
    let title = match app.resource {
        Resource::Keys => format!(
            " Keys {}/{} · {} ",
            app.items.len(),
            app.total_keys(),
            app.type_filter.label()
        ),
        Resource::Streams => format!(" Streams {} ", app.items.len()),
        Resource::PubSub => format!(" Active channels {} ", app.items.len()),
        Resource::Acl => format!(" ACL users {} ", app.items.len()),
    };
    let items = app
        .items
        .iter()
        .map(|name| {
            let suffix = if app.resource == Resource::Keys {
                app.keys
                    .iter()
                    .find(|k| k.name == *name)
                    .map(|k| {
                        let ttl = if k.ttl < 0 {
                            String::new()
                        } else {
                            format!(" · {}ms", k.ttl)
                        };
                        format!("  {}{ttl}", k.kind.label())
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            ListItem::new(Line::from(vec![
                Span::raw(name),
                Span::styled(suffix, Style::default().fg(MUTED)),
            ]))
        })
        .collect::<Vec<_>>();
    let border = if app.focus == Focus::List {
        CYAN
    } else {
        Color::DarkGray
    };
    let list = List::new(items)
        .block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border))
                .style(Style::default().bg(PANEL)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(38, 57, 68))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    let mut state =
        ListState::default().with_selected((!app.items.is_empty()).then_some(app.selected));
    frame.render_stateful_widget(list, columns[0], &mut state);
    let detail_border = if app.focus == Focus::Detail {
        CYAN
    } else {
        Color::DarkGray
    };
    let lines = app
        .detail
        .iter()
        .map(|s| {
            if s.ends_with(':') {
                Line::styled(s, Style::default().fg(CYAN).add_modifier(Modifier::BOLD))
            } else {
                Line::styled(s, Style::default().fg(Color::White))
            }
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines)
            .scroll((app.detail_scroll, 0))
            .wrap(Wrap { trim: false })
            .block(
                Block::default()
                    .title(" Detail · ↑↓ scroll when focused ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(detail_border))
                    .style(Style::default().bg(PANEL)),
            ),
        columns[1],
    );
}
fn input_height(app: &App) -> u16 {
    if matches!(app.mode, InputMode::Normal) {
        0
    } else {
        3
    }
}
fn draw_input(frame: &mut Frame, app: &App, area: Rect) {
    if area.height == 0 {
        return;
    }
    let (label, hint) = match &app.mode {
        InputMode::Normal => ("", ""),
        InputMode::Search { .. } => (" Search ", "live key/name filter"),
        InputMode::Command => (" Resource ", "keys, streams, pubsub, or acl"),
        InputMode::AddServerName => (" Server name ", "a memorable connection name"),
        InputMode::AddServerUri { .. } => (" Redis URI ", "redis://[:password@]host:port/db"),
        InputMode::Create {
            stage: FormStage::Type,
            kind,
            ..
        } => (" Create · type ", kind.label()),
        InputMode::Create {
            stage: FormStage::Key,
            ..
        } => (" Create · key ", "any Redis key name"),
        InputMode::Create {
            stage: FormStage::Data,
            kind,
            ..
        } => (" Create · data ", data_hint(*kind)),
        InputMode::Edit { kind, .. } => (
            " Edit data ",
            if *kind == KeyType::Stream {
                "append one JSON object message"
            } else {
                data_hint(*kind)
            },
        ),
        InputMode::DeleteConfirm { key } => (" Confirm delete ", key.as_str()),
    };
    let content = if matches!(
        app.mode,
        InputMode::Create {
            stage: FormStage::Type,
            ..
        }
    ) {
        format!("◀  {hint}  ▶    (Left/Right choose, Enter continue)")
    } else if matches!(app.mode, InputMode::DeleteConfirm { .. }) {
        format!("Delete {hint}?  y = yes, n/Esc = cancel")
    } else {
        app.input.clone()
    };
    frame.render_widget(
        Paragraph::new(content)
            .style(Style::default().fg(Color::White).bg(PANEL))
            .block(
                Block::default()
                    .title(label)
                    .title_bottom(Line::styled(
                        format!(" {hint} · Enter accept · Esc cancel "),
                        Style::default().fg(MUTED),
                    ))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(CYAN)),
            ),
        area,
    );
    if !matches!(
        app.mode,
        InputMode::Normal
            | InputMode::Create {
                stage: FormStage::Type,
                ..
            }
            | InputMode::DeleteConfirm { .. }
    ) {
        let inner_width = area.width.saturating_sub(2) as usize;
        let before = app.input[..app.cursor].chars().count();
        let x = area.x + 1 + (before.min(inner_width.saturating_sub(1)) as u16);
        frame.set_cursor_position((x, area.y + 1));
    }
}
fn data_hint(k: KeyType) -> &'static str {
    match k {
        KeyType::String => "plain string",
        KeyType::Hash => "JSON object: {\"field\":\"value\"}",
        KeyType::List => "JSON array: [\"first\",\"second\"]",
        KeyType::Set => "JSON array of members",
        KeyType::ZSet => "JSON object: {\"member\":1.5}",
        KeyType::Stream => "JSON object message: {\"field\":\"value\"}",
        _ => "",
    }
}
fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let keys=match app.resource{Resource::Keys=>"↑↓ select  ←→ focus  / search  t type  n new  e edit  d delete  r refresh  1-4 views  : selector  c servers  ? help  q quit",Resource::Streams=>"↑↓ select  ←→ focus  / search  n new stream  e append  d delete  r refresh  1-4 views  : selector  ? help  q quit",_=>"↑↓ select  ←→ focus  / search  r refresh  1-4 views  : selector  c servers  ? help  q quit"};
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);
    frame.render_widget(
        Paragraph::new(keys).style(Style::default().fg(MUTED)),
        rows[0],
    );
    let status_style = if app.status.starts_with("Error:") {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Green)
    };
    frame.render_widget(Paragraph::new(&*app.status).style(status_style), rows[1]);
}
fn draw_help(frame: &mut Frame) {
    let area = centered(82, 84, frame.area());
    frame.render_widget(Clear, area);
    let text = vec![
        Line::styled(
            "toolj keyboard guide",
            Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
        ),
        Line::raw(""),
        Line::raw("Navigation"),
        Line::raw("  ↑/↓ or j/k   select list item; scroll detail when detail focused"),
        Line::raw("  ←/→ or h/l   move focus between resource list and detail"),
        Line::raw("  1 2 3 4      :keys, :streams, :pubsub, :acl"),
        Line::raw("  :             type a resource selector such as :keys"),
        Line::raw(""),
        Line::raw("Actions"),
        Line::raw("  / search      t cycle key type      r refresh"),
        Line::raw("  n create      e edit/append         d delete"),
        Line::raw("  c servers     q quit                ? close help"),
        Line::raw(""),
        Line::raw("Editing formats"),
        Line::raw("  Strings use plain text. Hashes/streams/zsets use JSON objects."),
        Line::raw("  Lists and sets use JSON arrays. Edit rewrites the selected value."),
        Line::raw("  Stream edit appends a new message. TTL is preserved on rewrites."),
        Line::raw(""),
        Line::styled(
            "All views query the connected Redis server directly.",
            Style::default().fg(MUTED),
        ),
        Line::styled("Press ? or Esc to close.", Style::default().fg(CYAN)),
    ];
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }).block(
            Block::default()
                .title(" Help ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CYAN))
                .style(Style::default().bg(PANEL)),
        ),
        area,
    );
}
fn key() -> Style {
    Style::default().fg(CYAN).add_modifier(Modifier::BOLD)
}
fn centered(px: u16, py: u16, r: Rect) -> Rect {
    let v = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - py) / 2),
            Constraint::Percentage(py),
            Constraint::Percentage((100 - py) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - px) / 2),
            Constraint::Percentage(px),
            Constraint::Percentage((100 - px) / 2),
        ])
        .split(v[1])[1]
}
