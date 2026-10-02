//! ratatui rendering: header, entries list, treemap, top-N and footer are all
//! drawn on the same screen so the required information is visible at once.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::app::{App, Focus, InputMode};
use crate::size::{format_gb_mb, format_size};
use crate::treemap::layout_treemap;

/// Render the whole single-screen layout.
pub fn draw(frame: &mut Frame, app: &App) {
    let size = frame.area();

    // Guard against absurdly small terminals that cannot fit the layout.
    if size.width < 40 || size.height < 12 {
        let msg = Paragraph::new("Terminal too small (resize to at least 40x12)")
            .style(Style::default().fg(Color::Yellow))
            .alignment(ratatui::layout::Alignment::Center);
        frame.render_widget(msg, size);
        return;
    }

    let footer_height = if app.help_visible { 5 } else { 2 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(5),    // main
            Constraint::Length(footer_height),
        ])
        .split(size);

    draw_header(frame, chunks[0], app);

    let main = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),
            Constraint::Length(app.top_n as u16 + 2),
        ])
        .split(main[1]);

    draw_entries(frame, main[0], app);
    draw_treemap(frame, right[0], app);
    draw_top(frame, right[1], app);
    draw_status(frame, chunks[2], app);
}

fn header_line(app: &App, width: u16) -> Vec<Line<'static>> {
    let cur = app.current();
    let children = &cur.children;

    let files = children
        .iter()
        .filter(|&&c| !app.model.nodes[c].is_dir)
        .count();
    let dirs = children.len() - files;

    let largest = children
        .iter()
        .max_by_key(|&&c| app.model.nodes[c].size)
        .map(|&c| &app.model.nodes[c]);

    let width = width as usize;
    let path_max = width.saturating_sub(30).max(16);
    let name_max = width.saturating_sub(44).max(8);

    let largest_str = match largest {
        Some(n) => format!(
            "{} ({})",
            truncate(&n.name, name_max),
            format_gb_mb(n.size)
        ),
        None => "\u{2014} (empty)".to_string(),
    };

    let sort_str = app.sort_mode.label();
    let filter_str = if app.filter_bytes == 0 {
        "none".to_string()
    } else {
        format!("\u{2265} {}", format_size(app.filter_bytes))
    };

    vec![
        Line::from(vec![
            Span::styled("Path: ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::styled(truncate_left(&cur.path.display().to_string(), path_max), Style::default()),
            Span::raw("   "),
            Span::styled("Total: ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::styled(
                format_gb_mb(cur.size),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("Largest: ", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD)),
            Span::raw(largest_str),
            Span::raw("   "),
            Span::styled(format!("Files: {} ", files), Style::default().fg(Color::Yellow)),
            Span::styled(format!("Dirs: {}", dirs), Style::default().fg(Color::Yellow)),
        ]),
        Line::from(vec![
            Span::styled("Sort: ", Style::default().fg(Color::Blue)),
            Span::raw(sort_str),
            Span::raw("   "),
            Span::styled("Filter: ", Style::default().fg(Color::Blue)),
            Span::raw(filter_str),
            Span::raw("   "),
            Span::styled("Top-N: ", Style::default().fg(Color::Blue)),
            Span::raw(app.top_n.to_string()),
        ]),
    ]
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let lines = header_line(app, area.width);
    let para = Paragraph::new(lines);
    frame.render_widget(para, area);
}

fn draw_entries(frame: &mut Frame, area: Rect, app: &App) {
    let border_color = if app.focus == Focus::List {
        Color::Yellow
    } else {
        Color::DarkGray
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(format!(" Entries ({}) ", app.visible.len()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.visible.is_empty() {
        let msg = if app.filter_bytes > 0 {
            "No entries match the current filter"
        } else {
            "(empty directory)"
        };
        let p = Paragraph::new(msg).style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, inner);
        return;
    }

    let height = inner.height as usize;
    if height == 0 {
        return;
    }

    let sel = app.selected.min(app.visible.len() - 1);
    let mut offset = 0usize;
    if sel >= offset + height {
        offset = sel + 1 - height;
    }

    let width = inner.width as usize;

    let lines: Vec<Line> = app
        .visible
        .iter()
        .enumerate()
        .skip(offset)
        .take(height)
        .map(|(idx, &cid)| {
            let node = &app.model.nodes[cid];
            let selected = idx == sel;

            let prefix = if node.is_dir { "\u{25B8} " } else { "  " };
            let display_name = if node.is_dir {
                format!("{}/", node.name)
            } else {
                node.name.clone()
            };
            let size_str = format_size(node.size);

            let base_fg = if node.is_dir { Color::Cyan } else { Color::Reset };
            let style = if selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(base_fg)
            };

            let left = format!("{}{}", prefix, display_name);
            let left_len = left.chars().count();
            let size_len = size_str.chars().count();
            let padding = width.saturating_sub(left_len + size_len);

            Line::from(vec![
                Span::styled(left, style),
                Span::styled(" ".repeat(padding), style),
                Span::styled(size_str, style),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

fn palette() -> Vec<Color> {
    vec![
        Color::Rgb(70, 130, 180),   // steel blue
        Color::Rgb(60, 179, 113),   // medium sea green
        Color::Rgb(205, 92, 92),    // indian red
        Color::Rgb(255, 165, 0),    // orange
        Color::Rgb(147, 112, 219),  // medium purple
        Color::Rgb(255, 215, 0),    // gold
        Color::Rgb(0, 206, 209),    // dark turquoise
        Color::Rgb(255, 105, 180),  // hot pink
        Color::Rgb(154, 205, 50),   // yellow green
        Color::Rgb(210, 105, 30),   // chocolate
    ]
}

fn draw_treemap(frame: &mut Frame, area: Rect, app: &App) {
    let border_color = if app.focus == Focus::Treemap {
        Color::Yellow
    } else {
        Color::DarkGray
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(" Treemap ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let children = app.visible.clone();
    if children.is_empty() {
        let p = Paragraph::new("(nothing to map)")
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, inner);
        return;
    }

    let weights: Vec<f64> = children
        .iter()
        .map(|&c| app.model.nodes[c].size as f64)
        .collect();
    let rects = layout_treemap(&weights, inner.width, inner.height);
    let colors = palette();

    let buf = frame.buffer_mut();
    for (i, &cid) in children.iter().enumerate() {
        let Some(&(x, y, w, h)) = rects.get(i) else {
            continue;
        };
        let color = colors[i % colors.len()];
        let node = &app.model.nodes[cid];

        // Fill the rectangle.
        for dy in 0..h {
            for dx in 0..w {
                let cx = inner.x.saturating_add(x).saturating_add(dx);
                let cy = inner.y.saturating_add(y).saturating_add(dy);
                if cx < area.right() && cy < area.bottom() {
                    buf[(cx, cy)]
                        .set_char('\u{2588}')
                        .set_fg(color)
                        .set_bg(Color::Reset);
                }
            }
        }

        // White outline for the selected item when the treemap has focus.
        if app.focus == Focus::Treemap && i == app.selected && w >= 2 && h >= 2 {
            for dx in 0..w {
                for dy in [0u16, h - 1] {
                    let cx = inner.x.saturating_add(x).saturating_add(dx);
                    let cy = inner.y.saturating_add(y).saturating_add(dy);
                    if cx < area.right() && cy < area.bottom() {
                        buf[(cx, cy)].set_char('\u{2588}').set_fg(Color::White);
                    }
                }
            }
            for dy in 0..h {
                for dx in [0u16, w - 1] {
                    let cx = inner.x.saturating_add(x).saturating_add(dx);
                    let cy = inner.y.saturating_add(y).saturating_add(dy);
                    if cx < area.right() && cy < area.bottom() {
                        buf[(cx, cy)].set_char('\u{2588}').set_fg(Color::White);
                    }
                }
            }
        }

        // Overlay a label when the rectangle is large enough.
        let label = format!("{} {}", node.name, format_size(node.size));
        let chars: Vec<char> = label.chars().collect();
        if w >= 3 && h >= 1 {
            for (j, ch) in chars.iter().enumerate() {
                let lx = inner.x.saturating_add(x).saturating_add(1).saturating_add(j as u16);
                if lx < area.right() && lx < inner.x.saturating_add(x).saturating_add(w) {
                    let cell = &mut buf[(lx, inner.y.saturating_add(y))];
                    cell.set_char(*ch).set_fg(Color::White).set_bg(color);
                }
            }
        }
    }
}

fn draw_top(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" Top {} largest ", app.top_n));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut top: Vec<usize> = app.current().children.clone();
    top.sort_by(|&a, &b| app.model.nodes[b].size.cmp(&app.model.nodes[a].size));
    top.truncate(app.top_n);

    if top.is_empty() {
        let p = Paragraph::new("(empty)").style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, inner);
        return;
    }

    let width = inner.width as usize;
    let lines: Vec<Line> = top
        .iter()
        .enumerate()
        .map(|(i, &cid)| {
            let node = &app.model.nodes[cid];
            let rank = format!("{:>2}.", i + 1);
            let name = truncate(&node.name, width.saturating_sub(rank.chars().count() + 1 + 10));
            Line::from(vec![
                Span::styled(rank, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::raw(name),
                Span::raw(" "),
                Span::styled(format_size(node.size), Style::default().fg(Color::Green)),
            ])
        })
        .collect();

    frame.render_widget(Paragraph::new(lines), inner);
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    let lines: Vec<Line> = match app.input_mode {
        InputMode::Filter => vec![
            Line::from(vec![
                Span::styled(
                    "Filter: show entries >= size (e.g. 100M, 2G, 500KB; empty clears)  ",
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(&app.input, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::raw("_"),
            ]),
            Line::from(vec![Span::styled(
                "Enter apply   Esc cancel   Backspace edit",
                Style::default().fg(Color::DarkGray),
            )]),
        ],
        InputMode::ConfirmDelete => {
            let (name, size) = app
                .visible
                .get(app.selected)
                .map(|&c| {
                    let n = &app.model.nodes[c];
                    (n.name.clone(), format_size(n.size))
                })
                .unwrap_or_default();
            let what = if app
                .visible
                .get(app.selected)
                .map(|&c| app.model.nodes[c].is_dir)
                .unwrap_or(false)
            {
                "directory"
            } else {
                "file"
            };
            vec![
                Line::from(vec![
                    Span::styled("Delete ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
                    Span::styled(format!("{} '{}'", what, name), Style::default().fg(Color::White)),
                    Span::styled(format!(" ({})?", size), Style::default().fg(Color::White)),
                ]),
                Line::from(vec![Span::styled(
                    "y delete   n / Esc cancel",
                    Style::default().fg(Color::DarkGray),
                )]),
            ]
        }
        InputMode::Normal => {
            let mut lines = Vec::new();

            if let Some(msg) = &app.message {
                lines.push(Line::from(vec![Span::styled(
                    msg.clone(),
                    Style::default().fg(Color::Yellow),
                )]));
            } else {
                lines.push(Line::from(vec![Span::styled(
                    format!(
                        "Panel: {} (Tab to switch)",
                        if app.focus == Focus::List {
                            "Entries"
                        } else {
                            "Treemap"
                        }
                    ),
                    Style::default().fg(Color::DarkGray),
                )]));
            }

            lines.push(Line::from(vec![
                Span::styled("\u{2191}\u{2193}/jk move", key_style()),
                Span::raw("  "),
                Span::styled("Enter open", key_style()),
                Span::raw("  "),
                Span::styled("\u{2190}/Backspace up", key_style()),
                Span::raw("  "),
                Span::styled("Tab panel", key_style()),
                Span::raw("  "),
                Span::styled("s sort", key_style()),
                Span::raw("  "),
                Span::styled("f filter", key_style()),
                Span::raw("  "),
                Span::styled("d delete", key_style()),
                Span::raw("  "),
                Span::styled("r rescan", key_style()),
                Span::raw("  "),
                Span::styled("h/? help", key_style()),
                Span::raw("  "),
                Span::styled("q quit", key_style()),
            ]));

            if app.help_visible {
                lines.push(Line::from(vec![Span::styled(
                    "Delete removes the selected file/directory permanently (y to confirm).",
                    Style::default().fg(Color::DarkGray),
                )]));
                lines.push(Line::from(vec![Span::styled(
                    "Filter accepts suffixes: B, K/KB, M/MB, G/GB, T/TB (also KiB/MiB/GiB). Empty input clears it.",
                    Style::default().fg(Color::DarkGray),
                )]));
                lines.push(Line::from(vec![Span::styled(
                    "s cycles sort: size \u{2193} \u{2192} size \u{2191} \u{2192} name \u{2191}. Ctrl+N grows the Top-N list.",
                    Style::default().fg(Color::DarkGray),
                )]));
            }

            lines
        }
    };

    frame.render_widget(Paragraph::new(lines), area);
}

fn key_style() -> Style {
    Style::default().fg(Color::Cyan)
}

/// Truncate a string from the right, appending an ellipsis.
fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if max == 0 {
        return String::new();
    }
    if chars.len() <= max {
        return s.to_string();
    }
    let mut out: String = chars[..max.saturating_sub(1)].iter().collect();
    out.push('\u{2026}');
    out
}

/// Truncate a long path from the left, keeping the tail (more informative).
fn truncate_left(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if max == 0 {
        return String::new();
    }
    if chars.len() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(1);
    let mut out = String::from("\u{2026}");
    out.extend(chars[chars.len() - keep..].iter());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render_string(app: &App, width: u16, height: u16) -> String {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        format!("{}", terminal.backend())
    }

    fn sample_app() -> (App, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "tooli_ui_test_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("one.bin"), vec![0u8; 1_000_000]).unwrap();
        std::fs::write(dir.join("sub/two.bin"), vec![0u8; 500_000]).unwrap();
        let app = App::new(dir.clone());
        (app, dir)
    }

    #[test]
    fn renders_required_panels_on_one_screen() {
        let (app, _dir) = sample_app();
        let s = render_string(&app, 120, 40);
        for needle in [
            "Path:", "Total:", "Largest:", "Files:", "Entries", "Treemap", "Top",
        ] {
            assert!(s.contains(needle), "missing {needle:?} in rendered output:\n{s}");
        }
        // Directory total is 1,500,000 bytes -> "1.50 MB" (two decimals).
        assert!(s.contains("1.50 MB"), "missing total size:\n{s}");
        // Largest entry is 1,000,000 bytes -> "1.00 MB".
        assert!(s.contains("1.00 MB"), "missing largest size:\n{s}");
    }

    #[test]
    fn renders_on_small_terminal_without_panicking() {
        let (app, _dir) = sample_app();
        // Below the minimum size: the guard message path must not panic.
        let _ = render_string(&app, 20, 5);
    }
}
