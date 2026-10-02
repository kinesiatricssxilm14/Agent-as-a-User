//! Rendering. Every pane is drawn in the same frame — the summary, the tree,
//! the top-N list, the treemap and the key bar are always simultaneously
//! visible, and nothing is hidden behind a modal.

use std::path::PathBuf;

use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Paragraph, Wrap};

use crate::app::{App, Level, Pane, Prompt};
use crate::format;
use crate::scan::Kind;
use crate::treemap::{self, Item};

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const DIR_COLOR: Color = Color::LightBlue;
const FILE_COLOR: Color = Color::Gray;
const LINK_COLOR: Color = Color::Magenta;

/// Palette used for treemap tiles and the inline share bars; index by rank so a
/// tile keeps its colour as long as the ordering is stable.
const TILE_COLORS: [Color; 8] = [
    Color::Cyan,
    Color::Green,
    Color::Yellow,
    Color::Magenta,
    Color::Blue,
    Color::LightGreen,
    Color::LightRed,
    Color::LightYellow,
];

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    let keys_h = keybar_height(area.width);
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),      // title
            Constraint::Length(6),      // summary facts
            Constraint::Min(8),         // panes
            Constraint::Length(keys_h), // key bar
            Constraint::Length(1),      // status / prompt
        ])
        .split(area);

    draw_title(f, vertical[0], app);
    draw_summary(f, vertical[1], app);

    if app.show_help {
        draw_help(f, vertical[2], app);
    } else {
        draw_panes(f, vertical[2], app);
    }

    draw_keybar(f, vertical[3], app);
    draw_status(f, vertical[4], app);
}

fn draw_title(f: &mut Frame, area: Rect, app: &App) {
    let scope = app.scope_node();
    let line = Line::from(vec![
        Span::styled(
            " tooli ",
            Style::default()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" disk space visualiser  "),
        Span::styled("root ", Style::default().fg(DIM)),
        Span::styled(
            app.root.display().to_string(),
            Style::default().fg(Color::White),
        ),
        Span::styled("   sort ", Style::default().fg(DIM)),
        Span::styled(app.sort.label(), Style::default().fg(Color::Yellow)),
        Span::styled("   focus ", Style::default().fg(DIM)),
        Span::styled(app.focus.title(), Style::default().fg(ACCENT)),
        Span::styled("   scanned ", Style::default().fg(DIM)),
        Span::styled(
            format!("{} entries in {} ms", app.stats.entries, app.last_scan_ms),
            Style::default().fg(Color::White),
        ),
        Span::styled(
            if scope.error.is_some() {
                "   [partial: read errors]"
            } else {
                ""
            },
            Style::default().fg(Color::Red),
        ),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

/// The fact panel: current path, total size, largest item, counts, filters.
/// Everything a task might ask for is on this one screen.
fn draw_summary(f: &mut Frame, area: Rect, app: &App) {
    let scope = app.scope_node();
    let total = scope.size;

    let largest_child = app.largest_child();
    let largest_file = app.largest_file();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM))
        .title(Span::styled(
            " Current directory ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(52), Constraint::Percentage(48)])
        .split(inner);
    // Values are elided rather than clipped by the pane edge, so a long path or
    // file name never hides the size that follows it.
    let lw = columns[0].width as usize;
    let rw = columns[1].width as usize;

    let mut left = Vec::new();
    left.push(Line::from(vec![
        Span::styled(format!("{:<13}", "Path"), Style::default().fg(DIM)),
        Span::styled(
            // A deep path keeps its tail, which is the part that identifies it.
            truncate_front(&scope.path.display().to_string(), lw.saturating_sub(13)),
            Style::default().fg(Color::White),
        ),
    ]));
    left.push(from_kv(
        "Total size",
        format::size_dual(total),
        Color::LightGreen,
        lw,
    ));
    left.push(from_kv(
        "Direct items",
        format!(
            "{} files + {} directories = {} entries",
            scope.direct_files(),
            scope.direct_dirs(),
            scope.direct_entries()
        ),
        Color::White,
        lw,
    ));
    left.push(from_kv(
        "Recursive",
        format!(
            "{} files, {} directories",
            scope.files_deep, scope.dirs_deep
        ),
        Color::White,
        lw,
    ));

    let mut right = Vec::new();
    right.push(match largest_child {
        Some(n) => from_kv(
            "Largest item",
            format!(
                "{}{}  {}  ({} of dir)",
                n.name,
                if n.kind.is_dir() { "/" } else { "" },
                format::size_dual(n.size),
                format::percent(n.size, total)
            ),
            Color::LightRed,
            rw,
        ),
        None => from_kv("Largest item", "—".to_string(), DIM, rw),
    });
    right.push(match largest_file {
        Some(n) => from_kv(
            "Largest file",
            format!(
                "{}  {}  ({} bytes)",
                n.path
                    .strip_prefix(&scope.path)
                    .unwrap_or(&n.path)
                    .display(),
                format::size_dual(n.size),
                n.size
            ),
            Color::LightRed,
            rw,
        ),
        None => from_kv("Largest file", "—".to_string(), DIM, rw),
    });
    right.push(from_kv(
        "Filters",
        if app.filter_active() {
            let mut parts = Vec::new();
            if app.min_size > 0 {
                parts.push(format!("size ≥ {}", format::human(app.min_size)));
            }
            if !app.search.is_empty() {
                parts.push(format!("name contains `{}`", app.search));
            }
            parts.join(", ")
        } else {
            "none (all entries shown)".to_string()
        },
        if app.filter_active() {
            Color::Yellow
        } else {
            DIM
        },
        rw,
    ));
    right.push(from_kv(
        "Selected",
        match app.selected_node() {
            Some(n) => format!(
                "{}{}  {}",
                n.name,
                if n.kind.is_dir() { "/" } else { "" },
                format::human(n.size)
            ),
            None => "—".to_string(),
        },
        ACCENT,
        rw,
    ));

    f.render_widget(Paragraph::new(left), columns[0]);
    f.render_widget(Paragraph::new(right), columns[1]);
}

/// One `key   value` line, with the value elided to fit `width`.
fn from_kv(key: &str, value: String, color: Color, width: usize) -> Line<'static> {
    const KEY_W: usize = 13;
    let room = width.saturating_sub(KEY_W);
    Line::from(vec![
        Span::styled(format!("{:<KEY_W$}", key), Style::default().fg(DIM)),
        Span::styled(truncate(&value, room), Style::default().fg(color)),
    ])
}

fn draw_panes(f: &mut Frame, area: Rect, app: &mut App) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(46), Constraint::Percentage(54)])
        .split(area);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
        .split(columns[1]);

    draw_tree(f, columns[0], app);
    draw_top(f, right[0], app);
    draw_map(f, right[1], app);
}

fn pane_block(title: String, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(if focused {
            BorderType::Thick
        } else {
            BorderType::Rounded
        })
        .border_style(Style::default().fg(if focused { ACCENT } else { DIM }))
        .title(Span::styled(
            title,
            Style::default()
                .fg(if focused { ACCENT } else { Color::White })
                .add_modifier(if focused {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ))
}

fn draw_tree(f: &mut Frame, area: Rect, app: &mut App) {
    let rows = app.rows();
    let focused = app.focus == Pane::Tree;
    let title = format!(
        " Directory tree — {} rows, {}  [{}] ",
        rows.len(),
        format::gb_or_mb(app.scope_size()),
        app.sort.label()
    );
    let block = pane_block(title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let height = inner.height as usize;
    app.tree_rows_visible = height.max(1);

    if rows.is_empty() {
        let msg = if app.filter_active() {
            "No entry matches the active filters — press c to clear them."
        } else {
            "This directory is empty."
        };
        f.render_widget(
            Paragraph::new(msg).style(Style::default().fg(DIM)),
            inner,
        );
        return;
    }

    let selected_index = app
        .selected
        .as_ref()
        .and_then(|p| rows.iter().position(|r| &r.path == p));
    // Keep the cursor inside the window even when the terminal was resized.
    let mut offset = app.tree_offset.min(rows.len().saturating_sub(1));
    if let Some(i) = selected_index {
        if i < offset {
            offset = i;
        } else if height > 0 && i >= offset + height {
            offset = i + 1 - height;
        }
    }
    offset = offset.min(rows.len().saturating_sub(height.max(1)));
    app.tree_offset = offset;

    // On a narrow pane the share bar and item count give way so the name and
    // size — the two things the view exists to show — keep their room.
    let show_items = inner.width >= 52;
    let bar_w: usize = if inner.width >= 44 { 8 } else { 0 };
    let name_width = (inner.width as usize)
        .saturating_sub(12 + bar_w + if show_items { 8 } else { 0 })
        .max(8);
    let mut lines = Vec::new();
    for (i, row) in rows.iter().skip(offset).take(height).enumerate() {
        let is_selected = selected_index == Some(offset + i);
        // An empty directory gets a hollow marker so it reads differently from
        // one that is merely collapsed.
        let marker = if row.kind.is_dir() {
            if row.expanded {
                "▾ "
            } else if row.has_children {
                "▸ "
            } else {
                "· "
            }
        } else {
            "  "
        };
        let indent = "  ".repeat(row.depth);
        let icon = match row.kind {
            Kind::Dir => "📁",
            Kind::Symlink => "🔗",
            Kind::File => "📄",
        };
        let color = match row.kind {
            Kind::Dir => DIR_COLOR,
            Kind::Symlink => LINK_COLOR,
            Kind::File => FILE_COLOR,
        };
        let label = format!("{}{}{} {}", indent, marker, icon, row.name);
        let label = truncate(&label, name_width);

        let mut spans = vec![Span::styled(
            format!("{:<width$}", label, width = name_width),
            Style::default().fg(color).add_modifier(if row.kind.is_dir() {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
        )];
        if bar_w > 0 {
            spans.push(Span::styled(
                share_bar(row.share, bar_w),
                Style::default().fg(share_color(row.share)),
            ));
        }
        spans.push(Span::styled(
            format!(" {:>11}", format::human(row.size)),
            Style::default().fg(Color::LightGreen),
        ));
        if show_items {
            spans.push(Span::styled(
                if row.kind.is_dir() {
                    format!(" {:>6}", format!("{} it", row.items))
                } else {
                    " ".repeat(7)
                },
                Style::default().fg(DIM),
            ));
        }
        if row.error {
            spans.push(Span::styled(" !", Style::default().fg(Color::Red)));
        }

        let style = if is_selected {
            Style::default().bg(if focused { Color::Blue } else { Color::DarkGray })
        } else {
            Style::default()
        };
        lines.push(Line::from(spans).style(style));
    }

    f.render_widget(Paragraph::new(lines), inner);
    draw_scrollbar(f, area, rows.len(), offset, height);
}

fn draw_top(f: &mut Frame, area: Rect, app: &mut App) {
    let rows = app.top_rows();
    let focused = app.focus == Pane::Top;
    let title = format!(
        " Top {} by size — {} ({} listed) ",
        app.top_n,
        app.top_scope.label(),
        rows.len()
    );
    let block = pane_block(title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let height = inner.height as usize;
    app.top_rows_visible = height.max(1);

    if rows.is_empty() {
        f.render_widget(
            Paragraph::new("Nothing to rank here.").style(Style::default().fg(DIM)),
            inner,
        );
        return;
    }

    let mut offset = app.top_offset.min(rows.len().saturating_sub(1));
    if app.top_selected < offset {
        offset = app.top_selected;
    } else if height > 0 && app.top_selected >= offset + height {
        offset = app.top_selected + 1 - height;
    }
    offset = offset.min(rows.len().saturating_sub(height.max(1)));
    app.top_offset = offset;

    // Same degradation order as the tree: percentage, then bar, then width.
    let show_pct = inner.width >= 56;
    let bar_w: usize = if inner.width >= 46 { 8 } else { 0 };
    let name_width = (inner.width as usize)
        .saturating_sub(17 + bar_w + if show_pct { 9 } else { 0 })
        .max(8);
    let mut lines = Vec::new();
    for (i, row) in rows.iter().skip(offset).take(height).enumerate() {
        let index = offset + i;
        let is_selected =
            focused && index == app.top_selected || app.selected.as_ref() == Some(&row.path);
        let color = match row.kind {
            Kind::Dir => DIR_COLOR,
            Kind::Symlink => LINK_COLOR,
            Kind::File => FILE_COLOR,
        };
        let mut spans = vec![
            Span::styled(
                format!("{:>3}. ", row.rank),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                format!(
                    "{:<width$}",
                    // Relative paths here, so keep the file name visible.
                    truncate_front(&row.display, name_width),
                    width = name_width
                ),
                Style::default().fg(color),
            ),
        ];
        if bar_w > 0 {
            spans.push(Span::styled(
                share_bar(row.share, bar_w),
                Style::default().fg(share_color(row.share)),
            ));
        }
        spans.push(Span::styled(
            format!(" {:>11}", format::human(row.size)),
            Style::default().fg(Color::LightGreen),
        ));
        if show_pct {
            spans.push(Span::styled(
                format!(" {:>8}", format::percent_short(row.share)),
                Style::default().fg(DIM),
            ));
        }
        let style = if is_selected {
            Style::default().bg(if focused { Color::Blue } else { Color::DarkGray })
        } else {
            Style::default()
        };
        lines.push(Line::from(spans).style(style));
    }

    f.render_widget(Paragraph::new(lines), inner);
    draw_scrollbar(f, area, rows.len(), offset, height);
}

fn draw_map(f: &mut Frame, area: Rect, app: &mut App) {
    // Copy what the drawing needs out of the tree so the borrow of `app` ends
    // before the tile cache is refreshed.
    let entries: Vec<(PathBuf, String, u64, Kind)> = app
        .map_entries()
        .into_iter()
        .map(|n| (n.path.clone(), n.name.clone(), n.size, n.kind))
        .collect();
    let focused = app.focus == Pane::Map;
    let total: u64 = entries.iter().map(|e| e.2).sum();
    let title = format!(
        " Treemap — {} tiles, area ∝ size, total {} ",
        entries.len(),
        format::size_dual(total)
    );
    let block = pane_block(title, focused);
    let inner = block.inner(area);
    f.render_widget(block, area);

    app.tiles.clear();
    if entries.is_empty() || inner.width < 4 || inner.height < 2 {
        f.render_widget(
            Paragraph::new(if entries.is_empty() {
                "Nothing to plot."
            } else {
                "Terminal too small for the treemap."
            })
            .style(Style::default().fg(DIM)),
            inner,
        );
        return;
    }

    let items: Vec<Item> = entries
        .iter()
        .enumerate()
        .map(|(i, e)| Item {
            label: e.1.clone(),
            weight: e.2,
            index: i,
        })
        .collect();
    let tiles = treemap::squarify(&items, inner);

    for tile in &tiles {
        let (path, _, _, kind) = &entries[tile.index];
        app.tiles.push((path.clone(), tile.rect));

        let selected = app.selected.as_ref() == Some(path);
        let color = TILE_COLORS[tile.index % TILE_COLORS.len()];
        let fill_style = if selected {
            Style::default()
                .bg(color)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(color)
        };

        // Fill the tile so its area really is visible, then border and label it.
        let body = "█".repeat(tile.rect.width as usize);
        let mut fill_lines = Vec::new();
        for _ in 0..tile.rect.height {
            fill_lines.push(Line::from(Span::styled(
                body.clone(),
                Style::default().fg(color),
            )));
        }
        f.render_widget(Paragraph::new(fill_lines), tile.rect);

        if selected {
            f.render_widget(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Thick)
                    .border_style(Style::default().fg(Color::White)),
                tile.rect,
            );
        }

        // Labels go inside the tile whenever they fit at all.
        let label_area = Rect {
            x: tile.rect.x + 1,
            y: tile.rect.y + tile.rect.height / 2,
            width: tile.rect.width.saturating_sub(2),
            height: 1,
        };
        if label_area.width >= 3 && label_area.y < tile.rect.bottom() {
            let width = label_area.width as usize;
            // The tile carries the label and weight the layout was given, so the
            // drawn text always matches the rectangle that was computed for it.
            let mut text = format!("{}{}", tile.label, if kind.is_dir() { "/" } else { "" });
            let size_text = format::human(tile.weight);
            if width >= text.chars().count() + size_text.chars().count() + 2 {
                text = format!("{}  {}", text, size_text);
            } else if width >= size_text.chars().count() && width < text.chars().count() {
                // The name will not fit; the size is the point of the view.
                text = size_text;
            }
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    truncate(&text, width),
                    fill_style,
                )))
                .alignment(Alignment::Left),
                label_area,
            );
        }
    }

    if tiles.len() < entries.len() {
        let dropped = entries.len() - tiles.len();
        let note = Rect {
            x: inner.x,
            y: inner.bottom().saturating_sub(1),
            width: inner.width,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                truncate(
                    &format!("+{} tile(s) too small to draw — press f to filter", dropped),
                    inner.width as usize,
                ),
                Style::default().fg(Color::Black).bg(Color::Yellow),
            )))
            .alignment(Alignment::Right),
            note,
        );
    }
}

fn draw_scrollbar(f: &mut Frame, area: Rect, len: usize, offset: usize, height: usize) {
    if height == 0 || len <= height || area.width < 2 || area.height < 3 {
        return;
    }
    let track_x = area.right() - 1;
    let track_top = area.y + 1;
    let track_h = area.height.saturating_sub(2) as usize;
    if track_h == 0 {
        return;
    }
    let thumb_h = ((height * track_h) / len).max(1);
    let max_offset = len - height;
    let thumb_y = if max_offset == 0 {
        0
    } else {
        (offset * (track_h - thumb_h)) / max_offset
    };
    for i in 0..track_h {
        let filled = i >= thumb_y && i < thumb_y + thumb_h;
        let cell = Rect {
            x: track_x,
            y: track_top + i as u16,
            width: 1,
            height: 1,
        };
        f.render_widget(
            Paragraph::new(if filled { "█" } else { "│" })
                .style(Style::default().fg(if filled { ACCENT } else { DIM })),
            cell,
        );
    }
}

/// Inline proportional bar; the same idea as the treemap, one row tall.
fn share_bar(share: f64, width: usize) -> String {
    let filled = ((share.clamp(0.0, 1.0) * width as f64).round() as usize).min(width);
    let mut s = String::with_capacity(width);
    for i in 0..width {
        s.push(if i < filled { '▓' } else { '░' });
    }
    s
}

fn share_color(share: f64) -> Color {
    if share >= 0.5 {
        Color::LightRed
    } else if share >= 0.2 {
        Color::Yellow
    } else if share >= 0.05 {
        Color::LightGreen
    } else {
        DIM
    }
}

fn truncate(s: &str, width: usize) -> String {
    let count = s.chars().count();
    if count <= width {
        return s.to_string();
    }
    if width <= 1 {
        return s.chars().take(width).collect();
    }
    let mut out: String = s.chars().take(width - 1).collect();
    out.push('…');
    out
}

/// Elide from the front — for paths, where the tail identifies the entry.
fn truncate_front(s: &str, width: usize) -> String {
    let count = s.chars().count();
    if count <= width {
        return s.to_string();
    }
    if width <= 1 {
        return s.chars().rev().take(width).collect();
    }
    let skip = count - (width - 1);
    let mut out = String::from("…");
    out.extend(s.chars().skip(skip));
    out
}

/// Every binding, in the order the key bar lists them.
const KEYS: [(&str, &str); 22] = [
    ("↑↓/jk", "move"),
    ("←→/hl", "expand/collapse"),
    ("Enter", "enter dir"),
    ("Space", "fold"),
    ("Esc/u", "up"),
    ("Tab", "next pane"),
    ("1/2/3", "pick pane"),
    ("g/G", "first/last"),
    ("PgUp/PgDn", "page"),
    ("R", "root"),
    ("s", "sort"),
    ("f", "min size"),
    ("/", "name filter"),
    ("c", "clear filters"),
    ("n", "top N"),
    ("+/-", "top N ±"),
    ("t", "top scope"),
    ("e/E", "expand/collapse all"),
    ("d/Del", "delete"),
    ("r/F5", "rescan"),
    ("?/F1", "help"),
    ("q", "quit"),
];

/// The key bar never grows past this, so it cannot crowd out the data panes on
/// a small terminal.
const KEYBAR_MAX_ROWS: usize = 3;

/// How many rows the key bar needs at this width, so no binding is ever clipped
/// off the screen.
fn keybar_height(width: u16) -> u16 {
    keybar_lines(width).len().max(1) as u16
}

/// Pack every binding into at most [`KEYBAR_MAX_ROWS`] lines. Descriptions are
/// dropped before any binding is, since the key names themselves plus the help
/// page keep the interface discoverable either way.
fn keybar_lines(width: u16) -> Vec<Line<'static>> {
    let full = pack(width, true);
    if full.len() <= KEYBAR_MAX_ROWS {
        return full;
    }
    pack(width, false)
}

fn pack(width: u16, with_desc: bool) -> Vec<Line<'static>> {
    let width = width.max(20) as usize;
    let mut lines = Vec::new();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;

    for (key, desc) in KEYS {
        let cost = key.chars().count()
            + 3
            + if with_desc {
                desc.chars().count() + 3
            } else {
                1
            };
        if used + cost > width && !spans.is_empty() {
            lines.push(Line::from(std::mem::take(&mut spans)));
            used = 0;
        }
        spans.push(Span::styled(
            format!(" {} ", key),
            Style::default()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            if with_desc {
                format!(" {}  ", desc)
            } else {
                " ".to_string()
            },
            Style::default().fg(Color::Gray),
        ));
        used += cost;
    }
    if !spans.is_empty() {
        lines.push(Line::from(spans));
    }
    lines
}

/// Always-visible key documentation.
fn draw_keybar(f: &mut Frame, area: Rect, _app: &App) {
    f.render_widget(Paragraph::new(keybar_lines(area.width)), area);
}

fn draw_status(f: &mut Frame, area: Rect, app: &App) {
    let line = match &app.prompt {
        Prompt::MinSize(buf) => prompt_line("Minimum size (e.g. 10MB, 1.5G, 4096, empty = off)", buf),
        Prompt::Search(buf) => prompt_line("Name contains", buf),
        Prompt::TopN(buf) => prompt_line("Show how many top items (1-999)", buf),
        Prompt::ConfirmDelete { path, is_dir } => Line::from(vec![
            Span::styled(
                " CONFIRM ",
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Red)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(
                format!(
                    "Really delete {} {}? ",
                    if *is_dir { "directory" } else { "file" },
                    path.display()
                ),
                Style::default().fg(Color::White),
            ),
            Span::styled(
                "[y]es",
                Style::default().fg(Color::LightRed).add_modifier(Modifier::BOLD),
            ),
            Span::raw(" / "),
            Span::styled(
                "[n]o",
                Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD),
            ),
        ]),
        Prompt::None => {
            let (tag, color) = match app.level {
                Level::Info => (" INFO ", Color::Blue),
                Level::Success => (" OK ", Color::Green),
                Level::Warn => (" WARN ", Color::Yellow),
                Level::Error => (" ERROR ", Color::Red),
            };
            Line::from(vec![
                Span::styled(
                    tag,
                    Style::default()
                        .fg(Color::Black)
                        .bg(color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(app.status.clone(), Style::default().fg(Color::White)),
            ])
        }
    };
    f.render_widget(Paragraph::new(line), area);
}

fn prompt_line(label: &str, buf: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            " INPUT ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(format!("{}: ", label), Style::default().fg(Color::Gray)),
        Span::styled(
            buf.to_string(),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
        Span::styled("█", Style::default().fg(ACCENT)),
        Span::styled(
            "   Enter = apply, Esc = cancel, Backspace = erase",
            Style::default().fg(DIM),
        ),
    ])
}

/// The help page. It replaces only the pane row, so the path, total size,
/// largest item and counts above it stay on screen.
fn draw_help(f: &mut Frame, area: Rect, app: &App) {
    let block = pane_block(" Key reference — press ? or F1 to return ".to_string(), true);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let head = |s: &str| {
        Line::from(Span::styled(
            s.to_string(),
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))
    };
    let row = |k: &str, d: &str| {
        Line::from(vec![
            Span::styled(
                // Wide enough for the longest binding label below.
                format!("  {:<20}", k),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(d.to_string(), Style::default().fg(Color::Gray)),
        ])
    };

    let lines = vec![
        head("Navigation"),
        row("↑ / k", "move the cursor up"),
        row("↓ / j", "move the cursor down"),
        row("→ / l", "expand the selected directory (or step into the next row)"),
        row("← / h", "collapse the directory, or jump to its parent"),
        row("Enter", "make the selected directory the current directory"),
        row("Space", "expand or collapse the selected directory in place"),
        row("Esc / u / Backspace", "leave the current directory (Esc also cancels a prompt)"),
        row("Home / g", "jump to the first row"),
        row("End / G", "jump to the last row"),
        row("PgUp / PgDn", "scroll a whole page"),
        row("R", "return to the scan root"),
        row("Tab / Shift+Tab", "move focus between tree, top list and treemap"),
        row("1 / 2 / 3", "focus the tree, the top list or the treemap directly"),
        head("Analysis"),
        row("s", "cycle sort: size ↓, size ↑, name, item count, modified"),
        row("f", "filter: show only entries at or above a size you type"),
        row("/", "filter: show only entries whose name contains your text"),
        row("c", "clear both filters"),
        row("n", "type how many items the top list shows"),
        row("+ / -", "grow or shrink the top list by one"),
        row("t", "toggle the top list between all files and direct children"),
        row("e / E", "expand every directory / collapse everything"),
        head("Filesystem"),
        row("d / Delete", "delete the selected file or directory (asks y/n first)"),
        row("y / n", "answer a delete confirmation"),
        row("r / F5", "rescan the root from disk and refresh every size"),
        head("Other"),
        row("? / F1", "toggle this page"),
        row("q / Ctrl+C", "quit"),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Sizes ", Style::default().fg(DIM)),
            Span::styled(
                "always carry two decimals (e.g. 21.56 GB, 149.00 MB) and come from real stat() calls; deletes really unlink.",
                Style::default().fg(Color::Gray),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Now:   ", Style::default().fg(DIM)),
            Span::styled(
                format!(
                    "{} — {}, {} direct files, {} directories",
                    app.scope.display(),
                    format::size_dual(app.scope_size()),
                    app.scope_node().direct_files(),
                    app.scope_node().direct_dirs()
                ),
                Style::default().fg(Color::LightGreen),
            ),
        ]),
    ];

    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
}
