use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::{
    app::{App, InputMode, VisibleEntry},
    model::{format_gb, format_size, Node},
};

const BG: Color = Color::Rgb(18, 20, 28);
const PANEL: Color = Color::Rgb(27, 30, 41);
const ACCENT: Color = Color::Rgb(255, 177, 77);
const CYAN: Color = Color::Rgb(87, 208, 217);
const TEXT: Color = Color::Rgb(222, 225, 232);
const MUTED: Color = Color::Rgb(130, 137, 153);
const RED: Color = Color::Rgb(245, 104, 104);

pub fn draw(frame: &mut Frame, app: &mut App) {
    frame.render_widget(
        Block::default().style(Style::default().bg(BG)),
        frame.area(),
    );
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(frame.area());

    draw_header(frame, app, outer[0]);
    draw_stats(frame, app, outer[1]);

    if app.show_help {
        draw_help(frame, outer[2]);
    } else {
        draw_workspace(frame, app, outer[2]);
    }
    draw_footer(frame, app, outer[3]);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let current = app.current();
    let title = Line::from(vec![
        Span::styled(" tooli ", Style::default().fg(BG).bg(ACCENT).bold()),
        Span::styled("  DISK SPACE EXPLORER", Style::default().fg(TEXT).bold()),
        Span::styled(
            format!("   {} view", app.view_mode.label()),
            Style::default().fg(CYAN),
        ),
    ]);
    let path = Line::from(vec![
        Span::styled("Path  ", Style::default().fg(MUTED)),
        Span::styled(
            app.current_path.display().to_string(),
            Style::default().fg(TEXT).bold(),
        ),
    ]);
    let total = Line::from(vec![
        Span::styled("Total ", Style::default().fg(MUTED)),
        Span::styled(format_gb(current.size), Style::default().fg(ACCENT).bold()),
        Span::styled(
            format!("  ({})", format_size(current.size)),
            Style::default().fg(MUTED),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(vec![title, path, total])
            .style(Style::default().bg(PANEL))
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(MUTED)),
            ),
        area,
    );
}

fn draw_stats(frame: &mut Frame, app: &App, area: Rect) {
    let current = app.current();
    let (files, dirs) = current.direct_counts();
    let largest = current.largest_child();
    let largest_text = largest
        .map(|node| format!("{} — {}", node.name, format_gb(node.size)))
        .unwrap_or_else(|| "—".to_string());
    let filter = if app.filter_bytes == 0 {
        "OFF".to_string()
    } else {
        format!("≥ {}", format_size(app.filter_bytes))
    };
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(24),
            Constraint::Percentage(46),
            Constraint::Percentage(30),
        ])
        .split(area);
    stat_box(
        frame,
        columns[0],
        "DIRECT ITEMS",
        &format!("{files} files  ·  {dirs} dirs"),
        CYAN,
    );
    stat_box(frame, columns[1], "LARGEST ITEM", &largest_text, ACCENT);
    stat_box(
        frame,
        columns[2],
        "SIZE FILTER",
        &filter,
        if app.filter_bytes == 0 { MUTED } else { CYAN },
    );
}

fn stat_box(frame: &mut Frame, area: Rect, label: &str, value: &str, color: Color) {
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(format!(" {label}  "), Style::default().fg(MUTED)),
            Span::styled(value, Style::default().fg(color).bold()),
        ]))
        .style(Style::default().bg(PANEL))
        .block(
            Block::default()
                .borders(Borders::RIGHT)
                .border_style(Style::default().fg(BG)),
        ),
        area,
    );
}

fn draw_workspace(frame: &mut Frame, app: &mut App, area: Rect) {
    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(area);
    draw_explorer(frame, app, panes[0]);
    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(panes[1]);
    draw_treemap(frame, app, right[0]);
    draw_top_files(frame, app, right[1]);
}

fn draw_explorer(frame: &mut Frame, app: &mut App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    app.ensure_selection_visible(inner_height);
    let entries = app.visible_entries();
    let items: Vec<ListItem> = entries
        .iter()
        .enumerate()
        .skip(app.scroll)
        .take(inner_height)
        .map(|(index, entry)| entry_line(index, entry, app.selected))
        .collect();
    let sort = if app.descending {
        "SIZE ↓"
    } else {
        "SIZE ↑"
    };
    let title = format!(
        " {} · {} · {} entries ",
        app.view_mode.label(),
        sort,
        entries.len()
    );
    frame.render_widget(
        List::new(items).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CYAN)),
        ),
        area,
    );
}

fn entry_line(index: usize, entry: &VisibleEntry, selected: usize) -> ListItem<'static> {
    let is_selected = index == selected;
    let indent = "  ".repeat(entry.depth);
    let marker = if is_selected { "▸ " } else { "  " };
    let icon = if entry.is_dir { "◆" } else { "·" };
    let name = format!("{marker}{indent}{icon} {}", entry.name);
    let size = format_size(entry.size);
    let style = if is_selected {
        Style::default()
            .fg(BG)
            .bg(ACCENT)
            .add_modifier(Modifier::BOLD)
    } else if entry.is_dir {
        Style::default().fg(CYAN)
    } else {
        Style::default().fg(TEXT)
    };
    ListItem::new(Line::from(vec![
        Span::styled(format!("{name:<42}"), style),
        Span::styled(format!("{size:>12}"), style),
    ]))
}

fn draw_treemap(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" TREEMAP · direct children by size ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut children: Vec<&Node> = app
        .current()
        .children
        .iter()
        .filter(|node| node.size >= app.filter_bytes)
        .collect();
    children.sort_by(|a, b| b.size.cmp(&a.size));
    children.truncate(18);
    if children.is_empty() || inner.width < 4 || inner.height < 2 {
        frame.render_widget(
            Paragraph::new("No items match the current filter")
                .alignment(Alignment::Center)
                .style(Style::default().fg(MUTED)),
            inner,
        );
        return;
    }
    let mut tiles = Vec::new();
    layout_tiles(&children, inner, &mut tiles);
    let palette = [
        Color::Rgb(242, 153, 74),
        Color::Rgb(87, 208, 217),
        Color::Rgb(172, 126, 241),
        Color::Rgb(112, 193, 119),
        Color::Rgb(235, 103, 138),
        Color::Rgb(91, 143, 249),
    ];
    for (index, (node, tile)) in tiles.into_iter().enumerate() {
        if tile.width == 0 || tile.height == 0 {
            continue;
        }
        let color = palette[index % palette.len()];
        let title = if tile.width > 8 {
            truncate(&node.name, tile.width.saturating_sub(4) as usize)
        } else {
            String::new()
        };
        let tile_block = Block::default()
            .title(format!(" {title} "))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(color))
            .style(Style::default().bg(PANEL));
        let tile_inner = tile_block.inner(tile);
        frame.render_widget(tile_block, tile);
        if tile_inner.height > 0 && tile_inner.width > 4 {
            frame.render_widget(
                Paragraph::new(format_size(node.size))
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(color).bold()),
                tile_inner,
            );
        }
    }
}

fn layout_tiles<'a>(nodes: &[&'a Node], area: Rect, output: &mut Vec<(&'a Node, Rect)>) {
    if nodes.is_empty() || area.width == 0 || area.height == 0 {
        return;
    }
    if nodes.len() == 1 || area.width < 6 || area.height < 3 {
        output.push((nodes[0], area));
        return;
    }
    let total: u128 = nodes.iter().map(|node| node.size.max(1) as u128).sum();
    let mut first_total = 0_u128;
    let mut split = 0;
    for (index, node) in nodes.iter().enumerate() {
        if index > 0 && first_total * 2 >= total {
            break;
        }
        first_total += node.size.max(1) as u128;
        split = index + 1;
    }
    split = split.clamp(1, nodes.len() - 1);
    let ratio = first_total as f64 / total as f64;
    if area.width >= area.height.saturating_mul(2) {
        let first_width =
            ((area.width as f64 * ratio).round() as u16).clamp(1, area.width.saturating_sub(1));
        let left = Rect::new(area.x, area.y, first_width, area.height);
        let right = Rect::new(
            area.x + first_width,
            area.y,
            area.width - first_width,
            area.height,
        );
        layout_tiles(&nodes[..split], left, output);
        layout_tiles(&nodes[split..], right, output);
    } else {
        let first_height =
            ((area.height as f64 * ratio).round() as u16).clamp(1, area.height.saturating_sub(1));
        let top = Rect::new(area.x, area.y, area.width, first_height);
        let bottom = Rect::new(
            area.x,
            area.y + first_height,
            area.width,
            area.height - first_height,
        );
        layout_tiles(&nodes[..split], top, output);
        layout_tiles(&nodes[split..], bottom, output);
    }
}

fn draw_top_files(frame: &mut Frame, app: &App, area: Rect) {
    let inner_height = area.height.saturating_sub(2) as usize;
    let rows: Vec<ListItem> = app
        .top_files()
        .into_iter()
        .take(inner_height)
        .enumerate()
        .map(|(index, file)| {
            let relative = app.relative_path(&file.path).display().to_string();
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:>2}. ", index + 1), Style::default().fg(MUTED)),
                Span::styled(truncate(&relative, 28), Style::default().fg(TEXT)),
                Span::styled(
                    format!("  {}", format_size(file.size)),
                    Style::default().fg(ACCENT),
                ),
            ]))
        })
        .collect();
    frame.render_widget(
        List::new(rows).block(
            Block::default()
                .title(format!(" TOP {} FILES · [ / ] changes N ", app.top_n))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(CYAN)),
        ),
        area,
    );
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let prompt = match &app.input_mode {
        InputMode::Filter(value) => Line::from(vec![
            Span::styled(" FILTER > ", Style::default().fg(BG).bg(CYAN).bold()),
            Span::styled(
                format!(" {value}█  "),
                Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                "Enter apply · Esc cancel · examples: 10MB, 1.5GB, 0",
                Style::default().fg(MUTED),
            ),
        ]),
        InputMode::ConfirmDelete(_) => Line::from(vec![
            Span::styled(" DELETE? ", Style::default().fg(BG).bg(RED).bold()),
            Span::styled(format!(" {} ", app.status), Style::default().fg(RED).bold()),
        ]),
        InputMode::Normal => Line::from(vec![
            key("↑↓/jk", "move"),
            key(" Enter", "open"),
            key(" ←/h", "back"),
            key(" Tab/v", "view"),
            key(" s", "sort"),
            key(" f", "filter"),
            key(" d", "delete"),
            key(" r", "rescan"),
            key(" ?", "help"),
            key(" q", "quit"),
        ]),
    };
    let status = if matches!(app.input_mode, InputMode::Normal) {
        Line::from(vec![
            Span::styled(" STATUS  ", Style::default().fg(MUTED)),
            Span::styled(&app.status, Style::default().fg(TEXT)),
        ])
    } else {
        Line::default()
    };
    frame.render_widget(
        Paragraph::new(vec![prompt, status])
            .style(Style::default().bg(PANEL))
            .block(
                Block::default()
                    .borders(Borders::TOP)
                    .border_style(Style::default().fg(MUTED)),
            ),
        area,
    );
}

fn key<'a>(binding: &'a str, action: &'a str) -> Span<'a> {
    Span::styled(format!("{binding}:{action}"), Style::default().fg(ACCENT))
}

fn draw_help(frame: &mut Frame, area: Rect) {
    frame.render_widget(Clear, area);
    let help = vec![
        Line::styled("KEYBOARD REFERENCE", Style::default().fg(ACCENT).bold()),
        Line::raw(""),
        Line::from(vec![
            Span::styled("↑/↓, j/k, Home/End  ", Style::default().fg(CYAN)),
            Span::raw("Move through visible entries"),
        ]),
        Line::from(vec![
            Span::styled("Enter, →, l          ", Style::default().fg(CYAN)),
            Span::raw("Open selected directory"),
        ]),
        Line::from(vec![
            Span::styled("Esc, ←, h, Backspace ", Style::default().fg(CYAN)),
            Span::raw("Return to parent directory"),
        ]),
        Line::from(vec![
            Span::styled("Tab or v             ", Style::default().fg(CYAN)),
            Span::raw("Switch recursive tree / direct list"),
        ]),
        Line::from(vec![
            Span::styled("s                    ", Style::default().fg(CYAN)),
            Span::raw("Toggle ascending/descending size sort"),
        ]),
        Line::from(vec![
            Span::styled("f                    ", Style::default().fg(CYAN)),
            Span::raw("Set minimum size filter (0 clears)"),
        ]),
        Line::from(vec![
            Span::styled("d or Delete          ", Style::default().fg(CYAN)),
            Span::raw("Delete selected file after confirmation"),
        ]),
        Line::from(vec![
            Span::styled("r                    ", Style::default().fg(CYAN)),
            Span::raw("Rescan filesystem and update all sizes"),
        ]),
        Line::from(vec![
            Span::styled("[ / ]                ", Style::default().fg(CYAN)),
            Span::raw("Decrease/increase Top-N file count"),
        ]),
        Line::from(vec![
            Span::styled("? / Esc              ", Style::default().fg(CYAN)),
            Span::raw("Close this help"),
        ]),
        Line::from(vec![
            Span::styled("q / Ctrl-C            ", Style::default().fg(CYAN)),
            Span::raw("Quit safely"),
        ]),
        Line::raw(""),
        Line::styled(
            "Symlinks are measured but never followed. Unreadable entries are skipped.",
            Style::default().fg(MUTED),
        ),
    ];
    frame.render_widget(
        Paragraph::new(help)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(TEXT).bg(PANEL))
            .block(
                Block::default()
                    .title(" HELP · all controls are keyboard-only ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(ACCENT)),
            ),
        area,
    );
}

fn truncate(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_string();
    }
    if max_chars <= 1 {
        return "…".chars().take(max_chars).collect();
    }
    let mut result: String = value.chars().take(max_chars - 1).collect();
    result.push('…');
    result
}
