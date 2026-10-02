//! Rendering. Layout is deliberately flat: a header, a resource bar, one or
//! two content panes, and a footer. Prompts and confirmations are rendered as
//! extra footer lines rather than floating windows, so nothing on screen is
//! ever hidden behind an overlay.

use std::time::Duration;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::{App, Cursor, Focus, Level, View};
use crate::db::{KeyData, KeyDetail, KeyKind, MEMBER_LIMIT, STREAM_LIMIT};
use crate::help::{footer_hints, help_lines, HelpLine};
use crate::util::{fmt_bytes, fmt_score, fmt_ttl, one_line, pad, pad_left, truncate, width, wrap};

const C_BG: Color = Color::Reset;
const C_ACCENT: Color = Color::Cyan;
const C_KEY: Color = Color::Yellow;
const C_DIM: Color = Color::DarkGray;
const C_SEL: Color = Color::Rgb(38, 58, 74);
const C_SEL_DIM: Color = Color::Rgb(30, 34, 40);

/// One display line, tagged with the list item it belongs to (`None` for
/// decoration such as headers or blank separators).
type Tagged = (Option<usize>, Line<'static>);

/// The page size the key handler should use, derived from the last frame.
pub struct Frame1 {
    pub page: isize,
}

pub fn draw(f: &mut ratatui::Frame, app: &mut App) -> Frame1 {
    let area = f.area();
    f.render_widget(Block::default().style(Style::default().bg(C_BG)), area);

    let footer_h = footer_height(app);
    let bar_h = if app.view == View::Keys { 1 } else { 0 };
    let chunks = Layout::vertical([
        Constraint::Length(1),      // header
        Constraint::Length(1),      // resource bar
        Constraint::Length(bar_h),  // type filter bar (keys only)
        Constraint::Min(3),         // content
        Constraint::Length(footer_h),
    ])
    .split(area);

    draw_header(f, chunks[0], app);
    draw_resource_bar(f, chunks[1], app);
    if bar_h == 1 {
        draw_type_bar(f, chunks[2], app);
    }
    let page = draw_content(f, chunks[3], app);
    draw_footer(f, chunks[4], app);
    Frame1 { page }
}

// ------------------------------------------------------------------- header

fn draw_header(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let mut spans = vec![
        Span::styled(
            " toolj ",
            Style::default()
                .fg(Color::Black)
                .bg(C_ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
    ];
    match app.db.as_ref() {
        Some(db) => {
            spans.push(Span::styled(
                db.name.clone(),
                Style::default().fg(C_KEY).add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                format!("  {}", db.uri),
                Style::default().fg(Color::White),
            ));
            spans.push(Span::styled(
                format!("  db{}", db.db_index),
                Style::default().fg(C_ACCENT),
            ));
            spans.push(Span::styled(
                format!("  redis {} ({})", db.server_version, db.server_mode),
                Style::default().fg(C_DIM),
            ));
            spans.push(Span::styled(
                format!("  keys {}", app.total_keys),
                Style::default().fg(Color::Green),
            ));
            if app.sub.len() > 0 {
                spans.push(Span::styled(
                    format!("  subs {}", app.sub.len()),
                    Style::default().fg(Color::Magenta),
                ));
            }
        }
        None => spans.push(Span::styled(
            "not connected — press 5 for Servers",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_resource_bar(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let mut spans = vec![Span::styled(" ", Style::default())];
    for (i, v) in View::TABS.iter().enumerate() {
        let active = *v == app.view;
        let style = if active {
            Style::default()
                .fg(Color::Black)
                .bg(C_KEY)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Gray)
        };
        spans.push(Span::styled(format!(" {}:{} ", i + 1, v.slug()), style));
        spans.push(Span::raw(" "));
    }
    spans.push(Span::styled(
        format!(
            "│ Tab switches · : selector · ? help{}",
            if app.view == View::Help { " (open)" } else { "" }
        ),
        Style::default().fg(C_DIM),
    ));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_type_bar(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let counts = app.type_counts();
    let total: usize = counts.iter().map(|(_, n)| n).sum();
    let mut spans = vec![Span::styled(" type ", Style::default().fg(C_DIM))];
    let all_active = app.type_filter.is_none();
    spans.push(Span::styled(
        format!(" all({total}) "),
        badge_style(all_active),
    ));
    for (kind, n) in counts {
        if n == 0 && app.type_filter != Some(kind) {
            continue;
        }
        let active = app.type_filter == Some(kind);
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!(" {}({}) ", kind.label(), n),
            badge_style(active),
        ));
    }
    spans.push(Span::styled(
        format!("  t/T cycle · SCAN {}", app.scan_pattern),
        Style::default().fg(C_DIM),
    ));
    if !app.filter.is_empty() {
        spans.push(Span::styled(
            format!(" · /{}", app.filter),
            Style::default().fg(Color::Magenta),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn badge_style(active: bool) -> Style {
    if active {
        Style::default()
            .fg(Color::Black)
            .bg(C_ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    }
}

// ------------------------------------------------------------------ content

fn draw_content(f: &mut ratatui::Frame, area: Rect, app: &mut App) -> isize {
    // Without a connection only the Servers and Help views have anything real
    // to show; anywhere else, say so instead of drawing empty panes.
    if !app.connected() && !matches!(app.view, View::Servers | View::Help) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red))
            .title(Span::styled(
                format!(" {} · not connected ", app.view.title()),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ));
        let inner = block.inner(area);
        f.render_widget(block, area);
        f.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "No Redis connection.",
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Press 5 to open the Servers view, then Enter to connect to a saved",
                    Style::default().fg(Color::Gray),
                )),
                Line::from(Span::styled(
                    "server or a to add one (e.g. New_user redis://localhost:6379/0).",
                    Style::default().fg(Color::Gray),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Press ? for the full key reference.",
                    Style::default().fg(C_DIM),
                )),
            ])
            .wrap(Wrap { trim: true }),
            inner,
        );
        return 10;
    }
    match app.view {
        View::Keys => draw_keys(f, area, app),
        View::Streams => draw_streams(f, area, app),
        View::PubSub => draw_pubsub(f, area, app),
        View::Acl => draw_acl(f, area, app),
        View::Servers => draw_servers(f, area, app),
        View::Info => draw_info(f, area, app),
        View::Help => draw_help(f, area, app),
    }
}

/// Split into a narrow list pane and a wide detail pane.
fn split_panes(area: Rect) -> (Rect, Rect) {
    let list_w = (area.width as u32 * 38 / 100).clamp(20, 60) as u16;
    let cols = Layout::horizontal([Constraint::Length(list_w.min(area.width)), Constraint::Min(10)])
        .split(area);
    (cols[0], cols[1])
}

fn draw_keys(f: &mut ratatui::Frame, area: Rect, app: &mut App) -> isize {
    let (left, right) = split_panes(area);
    let visible = app.visible_keys();
    let rows: Vec<Tagged> = visible
        .iter()
        .enumerate()
        .map(|(row, ki)| {
            let k = &app.keys[*ki];
            (
                Some(row),
                Line::from(vec![
                    Span::styled(format!("{:>4} ", k.kind.short()), kind_style(k.kind)),
                    Span::styled(k.name.clone(), Style::default().fg(Color::White)),
                ]),
            )
        })
        .collect();

    let mut title = format!(
        " keys {}/{} ",
        visible.len(),
        app.total_keys.max(app.keys.len())
    );
    if app.keys_truncated {
        title.push_str("(scan capped) ");
    }
    let focused = app.focus == Focus::List;
    let sel = app.key_cur.sel;
    let page = render_tagged(f, left, &title, rows, sel, &mut app.key_cur, focused, "No keys match — press s to change the SCAN pattern, / to filter, n to create a key");

    let detail_rows = key_detail_rows(app, inner_width(right));
    let dtitle = match app.detail.as_ref() {
        Some(d) => detail_title(d),
        None => " value ".to_string(),
    };
    let dsel = app.detail_cur.sel;
    let dfocused = app.focus == Focus::Detail;
    render_tagged(
        f,
        right,
        &dtitle,
        detail_rows,
        dsel,
        &mut app.detail_cur,
        dfocused,
        "Select a key on the left (Enter loads it)",
    );
    page
}

fn kind_style(kind: KeyKind) -> Style {
    let c = match kind {
        KeyKind::String => Color::Green,
        KeyKind::Hash => Color::Cyan,
        KeyKind::List => Color::Blue,
        KeyKind::Set => Color::Magenta,
        KeyKind::ZSet => Color::LightMagenta,
        KeyKind::Stream => Color::Yellow,
        KeyKind::Other => Color::DarkGray,
    };
    Style::default().fg(c)
}

fn detail_title(d: &KeyDetail) -> String {
    let kind = d.kind.label();
    let mut t = format!(" {} · {} ", d.name, kind);
    let count = d.data.item_count();
    match d.kind {
        KeyKind::String => {}
        _ => {
            if d.total > count {
                t.push_str(&format!("· {count} of {} shown ", d.total));
            } else {
                t.push_str(&format!("· {count} item(s) "));
            }
        }
    }
    t
}

/// Build the detail pane for the loaded key: a metadata header (untagged) plus
/// one tagged row group per field / element / member / entry.
fn key_detail_rows(app: &App, w: usize) -> Vec<Tagged> {
    let Some(d) = app.detail.as_ref() else {
        return Vec::new();
    };
    let mut out: Vec<Tagged> = Vec::new();
    // Metadata is flowed across as many lines as the pane needs, so nothing is
    // clipped off the right edge.
    let mut meta: Vec<(String, String, Style)> = vec![
        (
            "key".into(),
            d.name.clone(),
            Style::default().fg(C_KEY).add_modifier(Modifier::BOLD),
        ),
        ("type".into(), d.kind.label().into(), kind_style(d.kind)),
        (
            "ttl".into(),
            fmt_ttl(d.ttl),
            Style::default().fg(Color::White),
        ),
        (
            "encoding".into(),
            d.encoding.clone(),
            Style::default().fg(Color::White),
        ),
    ];
    if let Some(m) = d.memory {
        meta.push((
            "memory".into(),
            fmt_bytes(m),
            Style::default().fg(Color::White),
        ));
    }
    for (k, v) in &d.extra {
        meta.push((k.clone(), v.clone(), Style::default().fg(Color::White)));
    }
    out.extend(flow_meta(&meta, w));

    if d.total > d.loaded {
        out.push((
            None,
            Line::from(Span::styled(
                format!(
                    "showing the first {} of {} items (cap {})",
                    d.loaded,
                    d.total,
                    if d.kind == KeyKind::Stream {
                        STREAM_LIMIT as isize
                    } else {
                        MEMBER_LIMIT
                    }
                ),
                Style::default().fg(Color::Yellow),
            )),
        ));
    }
    out.push((None, Line::from(Span::styled("─".repeat(w), Style::default().fg(C_DIM)))));

    match &d.data {
        KeyData::Str(v) => {
            if v.is_empty() {
                out.push((Some(0), Line::from(Span::styled("(empty string)", Style::default().fg(C_DIM)))));
            } else {
                for l in wrap(v, w, 4000) {
                    out.push((Some(0), Line::from(Span::styled(l, Style::default().fg(Color::White)))));
                }
            }
        }
        KeyData::Hash(pairs) => {
            let fw = pairs
                .iter()
                .map(|(f, _)| width(f))
                .max()
                .unwrap_or(4)
                .clamp(4, (w / 3).max(6));
            for (i, (fname, val)) in pairs.iter().enumerate() {
                push_pair(&mut out, i, fname, val, fw, w, C_ACCENT);
            }
            if pairs.is_empty() {
                out.push((None, Line::from(Span::styled("(no fields — press a to HSET)", Style::default().fg(C_DIM)))));
            }
        }
        KeyData::List(items) => {
            let fw = items.len().to_string().len().max(1) + 2;
            for (i, val) in items.iter().enumerate() {
                push_pair(&mut out, i, &format!("[{i}]"), val, fw, w, Color::Blue);
            }
            if items.is_empty() {
                out.push((None, Line::from(Span::styled("(empty list — press a to RPUSH)", Style::default().fg(C_DIM)))));
            }
        }
        KeyData::Set(members) => {
            let fw = members.len().to_string().len().max(1) + 2;
            for (i, m) in members.iter().enumerate() {
                push_pair(&mut out, i, &format!("{}.", i + 1), m, fw, w, Color::Magenta);
            }
            if members.is_empty() {
                out.push((None, Line::from(Span::styled("(empty set — press a to SADD)", Style::default().fg(C_DIM)))));
            }
        }
        KeyData::ZSet(members) => {
            let fw = members
                .iter()
                .map(|(_, s)| width(&fmt_score(*s)))
                .max()
                .unwrap_or(4)
                .clamp(4, 18);
            for (i, (m, s)) in members.iter().enumerate() {
                let rank = format!("#{:<3}", i + 1);
                let head = format!("{rank}{}", pad_left(&fmt_score(*s), fw));
                push_pair(&mut out, i, &head, m, width(&head), w, Color::LightMagenta);
            }
            if members.is_empty() {
                out.push((None, Line::from(Span::styled("(empty sorted set — press a to ZADD)", Style::default().fg(C_DIM)))));
            }
        }
        KeyData::Stream(entries) => {
            for (i, e) in entries.iter().enumerate() {
                out.push((
                    Some(i),
                    Line::from(vec![
                        Span::styled(
                            format!("{} ", e.id),
                            Style::default().fg(C_KEY).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("({} field(s))", e.fields.len()),
                            Style::default().fg(C_DIM),
                        ),
                    ]),
                ));
                let fw = e
                    .fields
                    .iter()
                    .map(|(f, _)| width(f))
                    .max()
                    .unwrap_or(4)
                    .clamp(4, (w / 3).max(6));
                for (fname, val) in &e.fields {
                    let head = format!("  {}", pad(fname, fw));
                    for (j, l) in wrap(val, w.saturating_sub(width(&head) + 3), 40).into_iter().enumerate() {
                        let label = if j == 0 {
                            head.clone()
                        } else {
                            " ".repeat(width(&head))
                        };
                        out.push((
                            Some(i),
                            Line::from(vec![
                                Span::styled(label, Style::default().fg(Color::Cyan)),
                                Span::styled(" │ ", Style::default().fg(C_DIM)),
                                Span::styled(l, Style::default().fg(Color::White)),
                            ]),
                        ));
                    }
                }
            }
            if entries.is_empty() {
                out.push((None, Line::from(Span::styled("(no entries — press a to XADD)", Style::default().fg(C_DIM)))));
            }
        }
        KeyData::Missing => out.push((
            None,
            Line::from(Span::styled(
                "key does not exist (it may have expired) — press r to reload",
                Style::default().fg(Color::Red),
            )),
        )),
        KeyData::Other(msg) => out.push((
            None,
            Line::from(Span::styled(msg.clone(), Style::default().fg(Color::Red))),
        )),
    }
    out
}

/// Lay out `label value` metadata pairs across as many lines as needed to fit
/// `w` columns. Long values get a line to themselves and are wrapped.
fn flow_meta(meta: &[(String, String, Style)], w: usize) -> Vec<Tagged> {
    let mut out: Vec<Tagged> = Vec::new();
    let mut cur: Vec<Span<'static>> = Vec::new();
    let mut cur_w = 0usize;
    for (label, value, style) in meta {
        let head = format!("{label} ");
        let cell_w = width(&head) + width(value) + 2;
        // A value too wide for any line gets its own wrapped block.
        if width(&head) + width(value) > w {
            if !cur.is_empty() {
                out.push((None, Line::from(std::mem::take(&mut cur))));
                cur_w = 0;
            }
            let avail = w.saturating_sub(width(&head)).max(8);
            for (i, l) in wrap(value, avail, 20).into_iter().enumerate() {
                out.push((
                    None,
                    Line::from(vec![
                        Span::styled(
                            if i == 0 {
                                head.clone()
                            } else {
                                " ".repeat(width(&head))
                            },
                            Style::default().fg(C_DIM),
                        ),
                        Span::styled(l, *style),
                    ]),
                ));
            }
            continue;
        }
        if cur_w + cell_w > w && !cur.is_empty() {
            out.push((None, Line::from(std::mem::take(&mut cur))));
            cur_w = 0;
        }
        if !cur.is_empty() {
            cur.push(Span::styled("  ", Style::default()));
            cur_w += 2;
        }
        cur.push(Span::styled(head.clone(), Style::default().fg(C_DIM)));
        cur.push(Span::styled(value.clone(), *style));
        cur_w += width(&head) + width(value);
    }
    if !cur.is_empty() {
        out.push((None, Line::from(cur)));
    }
    out
}

/// Append a `label │ value` row, wrapping the value under a hanging indent.
fn push_pair(
    out: &mut Vec<Tagged>,
    item: usize,
    label: &str,
    value: &str,
    label_w: usize,
    total_w: usize,
    label_color: Color,
) {
    let head = pad(label, label_w);
    let avail = total_w.saturating_sub(width(&head) + 3).max(8);
    let lines = wrap(value, avail, 200);
    for (i, l) in lines.into_iter().enumerate() {
        let lbl = if i == 0 {
            head.clone()
        } else {
            " ".repeat(width(&head))
        };
        out.push((
            Some(item),
            Line::from(vec![
                Span::styled(lbl, Style::default().fg(label_color)),
                Span::styled(" │ ", Style::default().fg(C_DIM)),
                Span::styled(l, Style::default().fg(Color::White)),
            ]),
        ));
    }
}

fn draw_streams(f: &mut ratatui::Frame, area: Rect, app: &mut App) -> isize {
    let (left, right) = split_panes(area);
    let visible = app.visible_streams();
    let rows: Vec<Tagged> = visible
        .iter()
        .enumerate()
        .map(|(row, si)| {
            let s = &app.streams[*si];
            (
                Some(row),
                Line::from(vec![
                    Span::styled(
                        format!("{:>7} ", s.length),
                        Style::default().fg(Color::Green),
                    ),
                    Span::styled(s.name.clone(), Style::default().fg(Color::White)),
                    Span::styled(
                        if s.groups > 0 {
                            format!("  {}g", s.groups)
                        } else {
                            String::new()
                        },
                        Style::default().fg(C_DIM),
                    ),
                ]),
            )
        })
        .collect();
    let title = format!(" streams {} ", visible.len());
    let focused = app.focus == Focus::List;
    let sel = app.stream_cur.sel;
    let page = render_tagged(
        f,
        left,
        &title,
        rows,
        sel,
        &mut app.stream_cur,
        focused,
        "No streams in this database — create one from :keys with n (stream <name> * f=v)",
    );

    let w = inner_width(right);
    let mut rows = key_detail_rows(app, w);
    if !app.stream_groups.is_empty() {
        let mut extra: Vec<Tagged> = Vec::new();
        extra.push((
            None,
            Line::from(Span::styled(
                "consumer groups",
                Style::default().fg(C_ACCENT).add_modifier(Modifier::BOLD),
            )),
        ));
        for g in &app.stream_groups {
            let text = g
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("  ");
            for l in wrap(&text, w.saturating_sub(2), 6) {
                extra.push((
                    None,
                    Line::from(Span::styled(format!("  {l}"), Style::default().fg(Color::Gray))),
                ));
            }
        }
        extra.push((None, Line::from(Span::styled("─".repeat(w), Style::default().fg(C_DIM)))));
        // Insert the group summary right after the metadata header block.
        let at = rows
            .iter()
            .position(|(item, _)| item.is_some())
            .unwrap_or(rows.len());
        for (i, e) in extra.into_iter().enumerate() {
            rows.insert(at + i, e);
        }
    }
    let title = match app.selected_stream() {
        Some(s) => format!(" {} · newest first ", s.name),
        None => " messages ".to_string(),
    };
    let dsel = app.stream_detail_cur.sel;
    let dfocused = app.focus == Focus::Detail;
    render_tagged(
        f,
        right,
        &title,
        rows,
        dsel,
        &mut app.stream_detail_cur,
        dfocused,
        "Select a stream on the left",
    );
    page
}

fn draw_pubsub(f: &mut ratatui::Frame, area: Rect, app: &mut App) -> isize {
    let (left, right) = split_panes(area);
    let visible = app.visible_channels();
    let rows: Vec<Tagged> = visible
        .iter()
        .enumerate()
        .map(|(row, ci)| {
            let c = &app.channels[*ci];
            let mark = if c.local { "●" } else { "○" };
            (
                Some(row),
                Line::from(vec![
                    Span::styled(
                        format!(" {mark} "),
                        Style::default().fg(if c.local { Color::Green } else { C_DIM }),
                    ),
                    Span::styled(
                        c.name.clone(),
                        Style::default().fg(if c.pattern { Color::Magenta } else { Color::White }),
                    ),
                    Span::styled(
                        format!("  {} sub", c.subscribers),
                        Style::default().fg(C_DIM),
                    ),
                    Span::styled(
                        if c.pattern { "  pattern" } else { "" },
                        Style::default().fg(Color::Magenta),
                    ),
                ]),
            )
        })
        .collect();
    let title = format!(
        " channels {} · patterns {} ",
        visible.len(),
        app.numpat
    );
    let focused = app.focus == Focus::List;
    let sel = app.channel_cur.sel;
    let page = render_tagged(
        f,
        left,
        &title,
        rows,
        sel,
        &mut app.channel_cur,
        focused,
        "No active channels. Enter subscribes to the selected one; P subscribes to a pattern; p publishes.",
    );

    let w = inner_width(right);
    let msgs = app.sub.messages();
    let mut rows: Vec<Tagged> = Vec::new();
    let subs = app.sub.list();
    let sub_line = if subs.is_empty() {
        Line::from(Span::styled(
            "not subscribed to anything — Enter on a channel, or P for a pattern",
            Style::default().fg(C_DIM),
        ))
    } else {
        Line::from(vec![
            Span::styled("subscribed ", Style::default().fg(C_DIM)),
            Span::styled(
                subs.iter()
                    .map(|(t, p)| if *p { format!("{t}(pattern)") } else { t.clone() })
                    .collect::<Vec<_>>()
                    .join(", "),
                Style::default().fg(Color::Green),
            ),
        ])
    };
    rows.push((None, sub_line));
    rows.push((None, Line::from(Span::styled("─".repeat(w), Style::default().fg(C_DIM)))));
    for (i, m) in msgs.iter().enumerate() {
        let head = format!("#{} {}", m.seq, m.channel);
        let via = m
            .via
            .as_ref()
            .map(|p| format!(" via {p}"))
            .unwrap_or_default();
        rows.push((
            Some(i),
            Line::from(vec![
                Span::styled(head, Style::default().fg(C_KEY)),
                Span::styled(via, Style::default().fg(Color::Magenta)),
            ]),
        ));
        for l in wrap(&m.payload, w.saturating_sub(2), 40) {
            rows.push((
                Some(i),
                Line::from(Span::styled(format!("  {l}"), Style::default().fg(Color::White))),
            ));
        }
    }
    if msgs.is_empty() {
        rows.push((
            None,
            Line::from(Span::styled(
                "no messages received yet",
                Style::default().fg(C_DIM),
            )),
        ));
    }
    let title = format!(" received messages {} ", msgs.len());
    // Keep the newest message in view unless the user scrolled up deliberately.
    let dsel = app.msg_cur.sel;
    let dfocused = app.focus == Focus::Detail;
    render_tagged(
        f,
        right,
        &title,
        rows,
        dsel,
        &mut app.msg_cur,
        dfocused,
        "no messages received yet",
    );
    page
}

fn draw_acl(f: &mut ratatui::Frame, area: Rect, app: &mut App) -> isize {
    let (left, right) = split_panes(area);
    let visible = app.visible_acl();
    let rows: Vec<Tagged> = visible
        .iter()
        .enumerate()
        .map(|(row, ui)| {
            let u = &app.acl[*ui];
            let me = u.name == app.whoami;
            (
                Some(row),
                Line::from(vec![
                    Span::styled(
                        format!(" {} ", if me { "→" } else { " " }),
                        Style::default().fg(C_ACCENT),
                    ),
                    Span::styled(
                        u.name.clone(),
                        Style::default()
                            .fg(Color::White)
                            .add_modifier(if me { Modifier::BOLD } else { Modifier::empty() }),
                    ),
                    Span::styled(
                        if u.rule.contains(" on ") || u.rule.ends_with(" on") {
                            "  on"
                        } else if u.rule.contains(" off ") || u.rule.contains("off") {
                            "  off"
                        } else {
                            ""
                        }
                        .to_string(),
                        Style::default().fg(C_DIM),
                    ),
                ]),
            )
        })
        .collect();
    let title = format!(" acl users {} ", visible.len());
    let focused = app.focus == Focus::List;
    let sel = app.acl_cur.sel;
    let page = render_tagged(
        f,
        left,
        &title,
        rows,
        sel,
        &mut app.acl_cur,
        focused,
        "No ACL users returned by the server",
    );

    let w = inner_width(right);
    let mut rows: Vec<Tagged> = Vec::new();
    let title = match app.selected_acl() {
        Some(u) => {
            rows.push((
                None,
                Line::from(vec![
                    Span::styled("user ", Style::default().fg(C_DIM)),
                    Span::styled(
                        u.name.clone(),
                        Style::default().fg(C_KEY).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        if u.name == app.whoami {
                            "  (current connection)"
                        } else {
                            ""
                        },
                        Style::default().fg(Color::Green),
                    ),
                ]),
            ));
            if !u.rule.is_empty() {
                // The `rules ` prefix is 6 columns wide; leave room for it.
                for (i, l) in wrap(&u.rule, w.saturating_sub(6), 12)
                    .into_iter()
                    .enumerate()
                {
                    rows.push((
                        None,
                        Line::from(vec![
                            Span::styled(
                                if i == 0 { "rules " } else { "      " },
                                Style::default().fg(C_DIM),
                            ),
                            Span::styled(l, Style::default().fg(Color::Gray)),
                        ]),
                    ));
                }
            }
            rows.push((None, Line::from(Span::styled("─".repeat(w), Style::default().fg(C_DIM)))));
            let fw = u
                .attrs
                .iter()
                .map(|(k, _)| width(k))
                .max()
                .unwrap_or(8)
                .clamp(6, 20);
            for (i, (k, v)) in u.attrs.iter().enumerate() {
                push_pair(&mut rows, i, k, v, fw, w, C_ACCENT);
            }
            format!(" {} · ACL GETUSER ", u.name)
        }
        None => " acl detail ".to_string(),
    };
    let dsel = app.acl_detail_cur.sel;
    let dfocused = app.focus == Focus::Detail;
    render_tagged(
        f,
        right,
        &title,
        rows,
        dsel,
        &mut app.acl_detail_cur,
        dfocused,
        "Select a user on the left",
    );
    page
}

fn draw_servers(f: &mut ratatui::Frame, area: Rect, app: &mut App) -> isize {
    let current = app.db.as_ref().map(|d| d.uri.clone());
    let rows: Vec<Tagged> = app
        .cfg
        .servers
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let active = current.as_deref() == Some(s.uri.as_str());
            (
                Some(i),
                Line::from(vec![
                    Span::styled(
                        format!(" {} ", if active { "●" } else { "○" }),
                        Style::default().fg(if active { Color::Green } else { C_DIM }),
                    ),
                    Span::styled(
                        pad(&truncate(&s.name, 20), 20),
                        Style::default().fg(C_KEY).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(s.uri.clone(), Style::default().fg(Color::White)),
                    Span::styled(
                        if active { "   connected" } else { "" },
                        Style::default().fg(Color::Green),
                    ),
                ]),
            )
        })
        .collect();
    let mut all: Vec<Tagged> = vec![
        (
            None,
            Line::from(Span::styled(
                format!("config file: {}", crate::config::Config::path().display()),
                Style::default().fg(C_DIM),
            )),
        ),
        (
            None,
            Line::from(Span::styled(
                "a add · e edit · d remove · Enter connect · w save · s SELECT db",
                Style::default().fg(C_DIM),
            )),
        ),
        (
            None,
            Line::from(Span::styled(
                "─".repeat(inner_width(area)),
                Style::default().fg(C_DIM),
            )),
        ),
    ];
    all.extend(rows);
    let title = format!(" servers {} ", app.cfg.servers.len());
    let sel = app.server_cur.sel;
    render_tagged(
        f,
        area,
        &title,
        all,
        sel,
        &mut app.server_cur,
        true,
        "No servers configured — press a to add one (e.g. New_user redis://localhost:6379/0)",
    )
}

fn draw_info(f: &mut ratatui::Frame, area: Rect, app: &mut App) -> isize {
    let w = inner_width(area);
    let entries = app.info_rows();
    let fw = entries
        .iter()
        .map(|(k, _)| width(k))
        .max()
        .unwrap_or(20)
        .clamp(10, 36);
    let mut rows: Vec<Tagged> = Vec::new();
    for (i, (k, v)) in entries.iter().enumerate() {
        push_pair(&mut rows, i, k, v, fw, w, C_ACCENT);
    }
    if rows.is_empty() {
        rows.push((
            None,
            Line::from(Span::styled(
                "no INFO fields match the current filter",
                Style::default().fg(C_DIM),
            )),
        ));
    }
    let title = format!(" server info {} field(s) ", entries.len());
    let sel = app.info_cur.sel;
    render_tagged(
        f,
        area,
        &title,
        rows,
        sel,
        &mut app.info_cur,
        true,
        "INFO returned nothing",
    )
}

fn draw_help(f: &mut ratatui::Frame, area: Rect, app: &mut App) -> isize {
    let rows: Vec<Tagged> = help_lines()
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let line = match l {
                HelpLine::Section(s) => Line::from(Span::styled(
                    s.to_string(),
                    Style::default()
                        .fg(C_ACCENT)
                        .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
                )),
                HelpLine::Key(k, d) => Line::from(vec![
                    Span::styled(
                        format!("  {}", pad(k, crate::help::KEY_COL)),
                        Style::default().fg(C_KEY).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(d.to_string(), Style::default().fg(Color::White)),
                ]),
                HelpLine::Blank => Line::from(""),
            };
            (Some(i), line)
        })
        .collect();
    let sel = app.help_cur.sel;
    render_tagged(
        f,
        area,
        " help · every binding in toolj ",
        rows,
        sel,
        &mut app.help_cur,
        true,
        "",
    )
}

// -------------------------------------------------------------------- footer

fn footer_height(app: &App) -> u16 {
    let mut h = 2; // status + hints
    if app.prompt.is_some() || app.confirm.is_some() {
        h += 1;
    }
    h
}

fn draw_footer(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let mut constraints = Vec::new();
    if app.prompt.is_some() || app.confirm.is_some() {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Length(1));
    constraints.push(Constraint::Length(1));
    let rows = Layout::vertical(constraints).split(area);
    let mut i = 0;

    if let Some(c) = app.confirm.as_ref() {
        let line = Line::from(vec![
            Span::styled(
                " confirm ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                c.question.clone(),
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            Span::styled("   y = yes   n / Esc = no", Style::default().fg(C_DIM)),
        ]);
        f.render_widget(Paragraph::new(line), rows[i]);
        i += 1;
    } else if let Some(p) = app.prompt.as_ref() {
        let label = format!(" {} ", p.label);
        let mut spans = vec![
            Span::styled(
                label.clone(),
                Style::default()
                    .fg(Color::Black)
                    .bg(C_ACCENT)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
        ];
        let shown = one_line(&p.input);
        spans.push(Span::styled(
            shown.clone(),
            Style::default().fg(Color::White),
        ));
        if !p.hint.is_empty() {
            spans.push(Span::styled(
                format!("   {}", p.hint),
                Style::default().fg(C_DIM),
            ));
        }
        f.render_widget(Paragraph::new(Line::from(spans)), rows[i]);
        // Place the real terminal cursor after the typed text.
        let caret = width(&label) + 1 + width(&one_line(&sub_str(&p.input, p.cursor)));
        let x = rows[i].x + (caret as u16).min(rows[i].width.saturating_sub(1));
        f.set_cursor_position((x, rows[i].y));
        i += 1;
    }

    let (badge, badge_style_) = match app.status.level {
        Level::Ok => (" ok ", Style::default().fg(Color::Black).bg(Color::Green)),
        Level::Info => (" info ", Style::default().fg(Color::Black).bg(C_ACCENT)),
        Level::Warn => (" warn ", Style::default().fg(Color::Black).bg(Color::Yellow)),
        Level::Error => (" error ", Style::default().fg(Color::White).bg(Color::Red)),
    };
    // Fade messages the user has probably already read.
    let text_color = if app.status_age() > Duration::from_secs(12) {
        Color::Gray
    } else {
        Color::White
    };
    let status = Line::from(vec![
        Span::styled(badge, badge_style_.add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(
            truncate(
                &one_line(&app.status.text),
                (area.width as usize).saturating_sub(10),
            ),
            Style::default().fg(text_color),
        ),
    ]);
    f.render_widget(Paragraph::new(status), rows[i]);
    i += 1;

    let hints = Line::from(vec![
        Span::styled(" keys ", Style::default().fg(Color::Black).bg(C_DIM)),
        Span::raw(" "),
        Span::styled(
            truncate(
                footer_hints(app.view, app.focus),
                (area.width as usize).saturating_sub(8),
            ),
            Style::default().fg(Color::Gray),
        ),
    ]);
    f.render_widget(Paragraph::new(hints), rows[i]);
}

fn sub_str(s: &str, chars: usize) -> String {
    s.chars().take(chars).collect()
}

// ------------------------------------------------------------------ plumbing

/// Usable content width inside a bordered pane.
fn inner_width(area: Rect) -> usize {
    area.width.saturating_sub(2) as usize
}

/// Render tagged lines inside a bordered block, scrolling so that the selected
/// item stays visible, and return the page size for the pane.
#[allow(clippy::too_many_arguments)]
fn render_tagged(
    f: &mut ratatui::Frame,
    area: Rect,
    title: &str,
    rows: Vec<Tagged>,
    sel: usize,
    cur: &mut Cursor,
    focused: bool,
    empty_hint: &str,
) -> isize {
    let border_style = if focused {
        Style::default().fg(C_ACCENT)
    } else {
        Style::default().fg(C_DIM)
    };
    let title_style = if focused {
        Style::default()
            .fg(Color::Black)
            .bg(C_ACCENT)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Gray)
    };

    let item_rows: Vec<usize> = rows.iter().filter_map(|(i, _)| *i).collect();
    let items = item_rows.iter().copied().max().map(|m| m + 1).unwrap_or(0);
    let first_line = rows.iter().position(|(i, _)| *i == Some(sel));
    let last_line = rows.iter().rposition(|(i, _)| *i == Some(sel));

    let inner_h = area.height.saturating_sub(2) as usize;
    if inner_h == 0 {
        return 1;
    }
    let total = rows.len();

    // Scroll just enough to bring the selected item fully into view.
    if let (Some(fl), Some(ll)) = (first_line, last_line) {
        if fl < cur.scroll {
            cur.scroll = fl;
        }
        let visible_end = cur.scroll + inner_h;
        if ll >= visible_end {
            // Prefer showing the start of a tall item over its tail.
            let need = ll + 1 - inner_h;
            cur.scroll = need.min(fl);
        }
    }
    if total <= inner_h {
        cur.scroll = 0;
    } else if cur.scroll + inner_h > total {
        cur.scroll = total - inner_h;
    }

    let mut title_full = title.to_string();
    if total > inner_h {
        let from = cur.scroll + 1;
        let to = (cur.scroll + inner_h).min(total);
        title_full.push_str(&format!("· lines {from}-{to}/{total} "));
    }
    if items > 0 {
        title_full.push_str(&format!("· {}/{} ", sel + 1, items));
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Span::styled(
            truncate(&title_full, area.width.saturating_sub(2) as usize),
            title_style,
        ));
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let sel_bg = if focused { C_SEL } else { C_SEL_DIM };
    let visible: Vec<Line> = rows
        .into_iter()
        .skip(cur.scroll)
        .take(inner_h)
        .map(|(item, line)| {
            if item == Some(sel) && items > 0 {
                let styled: Vec<Span> = line
                    .spans
                    .into_iter()
                    .map(|s| {
                        let st = s.style.bg(sel_bg);
                        Span::styled(s.content, st)
                    })
                    .collect();
                Line::from(styled).style(Style::default().bg(sel_bg))
            } else {
                line
            }
        })
        .collect();

    if visible.is_empty() && !empty_hint.is_empty() {
        f.render_widget(
            Paragraph::new(empty_hint.to_string())
                .style(Style::default().fg(C_DIM))
                .wrap(Wrap { trim: true }),
            inner,
        );
    } else {
        f.render_widget(Paragraph::new(visible), inner);
    }
    inner_h as isize
}
