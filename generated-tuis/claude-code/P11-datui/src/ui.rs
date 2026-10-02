//! Rendering. Every panel is laid out side by side so that the table, the
//! selected row's full field list, and the analysis metrics are all visible in
//! the same view — nothing important is hidden behind an overlay or a tab.

use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Cell, Clear, Paragraph, Row, Scrollbar, ScrollbarOrientation, ScrollbarState,
    Table, TableState, Tabs, Wrap,
};
use ratatui::Frame;

use crate::app::{App, Focus, Mode, MsgKind, Screen};
use crate::data::ColumnKind;

// A single accent palette keeps the panels visually consistent.
const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const KEY: Color = Color::Yellow;

pub fn draw(f: &mut Frame, app: &mut App) {
    if app.screen == Screen::Help {
        draw_help(f, app);
        return;
    }

    let area = f.area();
    // Terminals narrower than this cannot show the side panel legibly, so the
    // layout drops to a single column rather than rendering unreadable slivers.
    let narrow = area.width < 100;

    let prompt_height = if app.prompt.is_some() { 3 } else { 0 };
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),             // mode tabs + file summary
            Constraint::Length(3),             // query line
            Constraint::Length(prompt_height), // optional prompt
            Constraint::Min(5),                // table + panels
            Constraint::Length(2),             // status + key hints
        ])
        .split(area);

    draw_header(f, app, chunks[0]);
    draw_query(f, app, chunks[1]);
    if prompt_height > 0 {
        draw_prompt(f, app, chunks[2]);
    }

    let body = chunks[3];
    if narrow {
        // Stack vertically: the table keeps most of the height, the detail
        // panel keeps enough to stay useful.
        let split = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(4), Constraint::Percentage(45)])
            .split(body);
        draw_table(f, app, split[0]);
        if app.analysis_open {
            draw_analysis(f, app, split[1]);
        } else {
            draw_detail(f, app, split[1]);
        }
    } else {
        let right_width = if app.analysis_open { 46 } else { 34 };
        let split = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(40), Constraint::Length(right_width)])
            .split(body);
        draw_table(f, app, split[0]);
        if app.analysis_open {
            // Analysis and the row detail share the right column so the
            // selected row's values stay visible next to the statistics. The
            // detail panel asks for exactly the height its fields need (plus
            // borders), capped so the analysis panel keeps a usable share.
            let want = app.view_columns().len() as u16 + 2;
            let detail_h = want.clamp(5, (split[1].height / 2).max(5));
            let rsplit = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(8), Constraint::Length(detail_h)])
                .split(split[1]);
            draw_analysis(f, app, rsplit[0]);
            draw_detail(f, app, rsplit[1]);
        } else {
            draw_detail(f, app, split[1]);
        }
    }

    draw_status(f, app, chunks[4]);
}

/// Mode tabs on the left, dataset summary on the right.
fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(46), Constraint::Min(10)])
        .split(area);

    let titles: Vec<Line> = Mode::ALL
        .iter()
        .enumerate()
        .map(|(i, m)| {
            // The F-key is printed beside each mode so the shortcut is
            // discoverable without opening help.
            Line::from(vec![
                Span::styled(format!("F{} ", i + 2), Style::new().fg(KEY)),
                Span::raw(m.title()),
            ])
        })
        .collect();
    let tabs = Tabs::new(titles)
        .select(app.mode.index())
        .divider(Span::styled("│", Style::new().fg(DIM)))
        .highlight_style(
            Style::new()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD),
        );
    f.render_widget(tabs, split[0]);

    let shown = app.row_count();
    let total = app.ds.total_rows();
    let cols = app.view_columns().len();
    let mut spans = vec![
        Span::styled("toolk ", Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled(
            shorten_path(&app.ds.path.to_string_lossy(), 40),
            Style::new().fg(Color::White),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{shown}/{total} rows"),
            Style::new().fg(if shown == total { Color::White } else { ACCENT }),
        ),
        Span::styled(format!("  {cols} cols"), Style::new().fg(DIM)),
        Span::styled(
            format!("  num {}", app.cell_fmt.label()),
            Style::new().fg(DIM),
        ),
    ];
    if !app.ds.sort_keys.is_empty() {
        spans.push(Span::styled(
            format!("  sort {}", app.sort_summary()),
            Style::new().fg(Color::Magenta),
        ));
    }
    let p = Paragraph::new(Line::from(spans)).right_aligned();
    f.render_widget(p, split[1]);
}

/// The query input line, with the active mode's syntax hint when empty.
fn draw_query(f: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Query && app.prompt.is_none();
    let border = if focused { ACCENT } else { DIM };
    let live = app.mode == Mode::Fuzzy;
    let title = Line::from(vec![
        Span::styled(
            format!(" {} query ", app.mode.title()),
            Style::new().fg(border).add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            if live {
                "[live] ".to_string()
            } else {
                "[Enter runs] ".to_string()
            },
            Style::new().fg(DIM),
        ),
    ]);

    let text = app.query_text();
    let inner_width = area.width.saturating_sub(3) as usize;
    let body: Line = if text.is_empty() {
        Line::from(Span::styled(
            app.mode.hint(),
            Style::new().fg(DIM).add_modifier(Modifier::ITALIC),
        ))
    } else {
        // Scroll the text horizontally so the cursor stays visible.
        let chars: Vec<char> = text.chars().collect();
        let cursor = app.editor().cursor();
        let start = cursor.saturating_sub(inner_width.saturating_sub(1));
        let visible: String = chars[start.min(chars.len())..].iter().collect();
        Line::from(Span::styled(visible, Style::new().fg(Color::White)))
    };

    let p = Paragraph::new(body).block(
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(border))
            .title(title),
    );
    f.render_widget(p, area);

    if focused {
        let cursor = app.editor().cursor();
        let start = cursor.saturating_sub(inner_width.saturating_sub(1));
        let x = area.x + 1 + (cursor - start) as u16;
        f.set_cursor_position(Position::new(x.min(area.x + area.width - 2), area.y + 1));
    }
}

/// The path prompt used by export and open.
fn draw_prompt(f: &mut Frame, app: &App, area: Rect) {
    let Some((kind, ed)) = &app.prompt else {
        return;
    };
    let text = ed.text();
    let inner_width = area.width.saturating_sub(3) as usize;
    let chars: Vec<char> = text.chars().collect();
    let start = ed.cursor().saturating_sub(inner_width.saturating_sub(1));
    let visible: String = chars[start.min(chars.len())..].iter().collect();

    let p = Paragraph::new(Line::from(Span::styled(
        visible,
        Style::new().fg(Color::White),
    )))
    .block(
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(KEY))
            .title(Line::from(vec![
                Span::styled(
                    format!(" {} ", kind.label()),
                    Style::new().fg(KEY).add_modifier(Modifier::BOLD),
                ),
                Span::styled("Enter=confirm  Esc=cancel ", Style::new().fg(DIM)),
            ])),
    );
    f.render_widget(p, area);
    let x = area.x + 1 + (ed.cursor() - start) as u16;
    f.set_cursor_position(Position::new(
        x.min(area.x + area.width - 2),
        area.y + 1,
    ));
}

/// The results table. Only the visible window of rows is materialised, so a
/// large file still scrolls smoothly.
fn draw_table(f: &mut Frame, app: &mut App, area: Rect) {
    let columns = app.view_columns();
    let rows_total = app.row_count();

    // 2 border rows + 1 header row.
    let viewport = area.height.saturating_sub(3) as usize;
    app.page_rows = viewport.max(1);

    // Keep the selection inside the visible window.
    if app.selected < app.row_offset {
        app.row_offset = app.selected;
    } else if viewport > 0 && app.selected >= app.row_offset + viewport {
        app.row_offset = app.selected + 1 - viewport;
    }
    if app.row_offset + viewport > rows_total {
        app.row_offset = rows_total.saturating_sub(viewport);
    }

    let focused = app.focus == Focus::Table && app.prompt.is_none();
    let border = if focused { ACCENT } else { DIM };

    if columns.is_empty() || rows_total == 0 {
        let msg = if columns.is_empty() {
            "The query returned no columns."
        } else {
            "No rows matched. Press Tab to edit the query, or c to clear it."
        };
        let p = Paragraph::new(Line::from(Span::styled(msg, Style::new().fg(DIM))))
            .block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(Style::new().fg(border))
                    .title(table_title(app, border)),
            )
            .wrap(Wrap { trim: true });
        f.render_widget(p, area);
        return;
    }

    // Row-number gutter width, plus per-column widths measured from the values
    // actually on screen.
    let gutter = format!("{}", rows_total).len().max(2) + 1;
    let end = (app.row_offset + viewport).min(rows_total);
    let visible_rows: Vec<usize> = (app.row_offset..end).collect();

    // The sort marker is part of the header text, so it has to be measured
    // before the column widths are fixed or ratatui will truncate it away.
    let sort_marks: Vec<String> = columns
        .iter()
        .map(|name| {
            app.ds
                .sort_keys
                .iter()
                .position(|k| &k.column == name)
                .map(|pos| {
                    let k = &app.ds.sort_keys[pos];
                    if app.ds.sort_keys.len() > 1 {
                        format!("{}{}", k.dir.arrow(), pos + 1)
                    } else {
                        k.dir.arrow().to_string()
                    }
                })
                .unwrap_or_default()
        })
        .collect();

    let mut widths: Vec<usize> = columns
        .iter()
        .zip(&sort_marks)
        .map(|(c, mark)| (c.chars().count() + mark.chars().count()).min(28))
        .collect();
    for &r in &visible_rows {
        for (i, name) in columns.iter().enumerate() {
            let len = app.ds.cell(r, name, app.cell_fmt).chars().count();
            if len > widths[i] {
                widths[i] = len.min(28);
            }
        }
    }

    // Horizontally scroll columns so the focused one is always on screen.
    let avail = area.width.saturating_sub(2 + gutter as u16) as usize;
    if app.focused_col < app.col_offset {
        app.col_offset = app.focused_col;
    }
    loop {
        let mut used = 0usize;
        let mut last = app.col_offset;
        for i in app.col_offset..columns.len() {
            let w = widths[i] + 1;
            if used + w > avail && i > app.col_offset {
                break;
            }
            used += w;
            last = i;
        }
        if app.focused_col > last && app.col_offset + 1 < columns.len() {
            app.col_offset += 1;
        } else {
            break;
        }
    }

    let shown: Vec<usize> = {
        let mut used = 0usize;
        let mut v = Vec::new();
        for i in app.col_offset..columns.len() {
            let w = widths[i] + 1;
            if used + w > avail && !v.is_empty() {
                break;
            }
            used += w;
            v.push(i);
        }
        v
    };

    let header_cells: Vec<Cell> = std::iter::once(Cell::from(Span::styled(
        format!("{:>w$}", "#", w = gutter - 1),
        Style::new().fg(DIM),
    )))
    .chain(shown.iter().map(|&i| {
        let name = &columns[i];
        let kind = app.ds.view_column_kind(name);
        let sort = &sort_marks[i];
        let mut style = Style::new().add_modifier(Modifier::BOLD);
        style = if i == app.focused_col {
            style.fg(Color::Black).bg(KEY)
        } else if kind == ColumnKind::Numeric {
            style.fg(Color::LightGreen)
        } else {
            style.fg(Color::White)
        };
        let label = format!("{}{sort}", truncate(name, widths[i].saturating_sub(sort.chars().count())));
        Cell::from(Span::styled(label, style))
    }))
    .collect();

    let body: Vec<Row> = visible_rows
        .iter()
        .map(|&r| {
            let mut cells: Vec<Cell> = Vec::with_capacity(shown.len() + 1);
            cells.push(Cell::from(Span::styled(
                format!("{:>w$}", r + 1, w = gutter - 1),
                Style::new().fg(DIM),
            )));
            for &i in &shown {
                let name = &columns[i];
                let text = app.ds.cell(r, name, app.cell_fmt);
                let kind = app.ds.view_column_kind(name);
                let mut style = Style::new();
                if text.is_empty() {
                    style = style.fg(DIM);
                } else if kind == ColumnKind::Numeric {
                    style = style.fg(Color::LightGreen);
                }
                if i == app.focused_col {
                    style = style.add_modifier(Modifier::BOLD);
                }
                cells.push(Cell::from(Span::styled(truncate(&text, widths[i]), style)));
            }
            Row::new(cells)
        })
        .collect();

    let mut constraints: Vec<Constraint> = vec![Constraint::Length(gutter as u16)];
    constraints.extend(shown.iter().map(|&i| Constraint::Length(widths[i] as u16)));

    let table = Table::new(body, constraints)
        .header(Row::new(header_cells).bottom_margin(0))
        .column_spacing(1)
        .row_highlight_style(
            Style::new()
                .bg(Color::Rgb(38, 58, 74))
                .add_modifier(Modifier::BOLD),
        )
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(border))
                .title(table_title(app, border)),
        );

    let mut state = TableState::default();
    // The table only holds the visible window, so the selection is relative.
    state.select(Some(app.selected.saturating_sub(app.row_offset)));
    f.render_stateful_widget(table, area, &mut state);

    if rows_total > viewport {
        let mut sb = ScrollbarState::default()
            .content_length(rows_total)
            .viewport_content_length(viewport)
            .position(app.row_offset);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None)
                .thumb_style(Style::new().fg(border)),
            scroll_track(area),
            &mut sb,
        );
    }
}

fn table_title(app: &App, border: Color) -> Line<'static> {
    let mut spans = vec![Span::styled(
        " Result ",
        Style::new().fg(border).add_modifier(Modifier::BOLD),
    )];
    match app.match_count {
        Some(n) => spans.push(Span::styled(
            format!("{n} match{} ", if n == 1 { "" } else { "es" }),
            Style::new().fg(Color::LightGreen).add_modifier(Modifier::BOLD),
        )),
        None => spans.push(Span::styled(
            format!("all {} rows ", app.ds.total_rows()),
            Style::new().fg(DIM),
        )),
    }
    if app.row_count() > 0 {
        spans.push(Span::styled(
            format!("· row {}/{} ", app.selected + 1, app.row_count()),
            Style::new().fg(DIM),
        ));
    }
    let cols = app.view_columns();
    if !cols.is_empty() {
        // Show which column analysis and sort will act on.
        spans.push(Span::styled(
            format!("· col {}/{} ", app.focused_col + 1, cols.len()),
            Style::new().fg(DIM),
        ));
    }
    Line::from(spans)
}

/// Every field of the selected row, so no column is off-screen for the
/// selection even when the table is scrolled horizontally.
fn draw_detail(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(DIM))
        .title(Line::from(vec![
            Span::styled(
                " Selected row ",
                Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
            ),
            Span::styled("(all columns · J/K scrolls) ", Style::new().fg(DIM)),
        ]));

    if app.row_count() == 0 {
        let p = Paragraph::new(Line::from(Span::styled(
            "Nothing selected.",
            Style::new().fg(DIM),
        )))
        .block(block);
        f.render_widget(p, area);
        return;
    }

    let cells = app.ds.row_cells(app.selected, app.cell_fmt);
    let label_w = cells
        .iter()
        .map(|(k, _)| k.chars().count())
        .max()
        .unwrap_or(6)
        .min(18);
    let value_w = area.width.saturating_sub(label_w as u16 + 4) as usize;

    let mut lines: Vec<Line> = Vec::new();
    for (i, (k, v)) in cells.iter().enumerate() {
        let focused = i == app.focused_col;
        let label = Span::styled(
            format!("{:>w$} ", truncate(k, label_w), w = label_w),
            if focused {
                Style::new().fg(Color::Black).bg(KEY)
            } else {
                Style::new().fg(ACCENT)
            },
        );
        let shown = if v.is_empty() { "(null)".to_string() } else { v.clone() };
        let vstyle = if v.is_empty() {
            Style::new().fg(DIM).add_modifier(Modifier::ITALIC)
        } else {
            Style::new().fg(Color::White)
        };
        // Long values wrap onto continuation lines rather than being cut off.
        let wrapped = wrap_text(&shown, value_w.max(4));
        for (j, part) in wrapped.iter().enumerate() {
            if j == 0 {
                lines.push(Line::from(vec![label.clone(), Span::styled(part.clone(), vstyle)]));
            } else {
                lines.push(Line::from(vec![
                    Span::raw(" ".repeat(label_w + 1)),
                    Span::styled(part.clone(), vstyle),
                ]));
            }
        }
    }

    let p = Paragraph::new(Text::from(lines))
        .block(block)
        .scroll((app.detail_scroll, 0));
    f.render_widget(p, area);
}

/// Descriptive statistics, distribution and correlations for the focused
/// column — all in one scrollable panel on the same screen as the table.
fn draw_analysis(f: &mut Frame, app: &mut App, area: Rect) {
    let num_label = app.num_fmt.label();
    let column = app.focused_column().unwrap_or_default();
    let inner_w = area.width.saturating_sub(4) as usize;

    let mut lines: Vec<Line> = Vec::new();
    let analysis = app.analysis();

    match analysis {
        None => lines.push(Line::from(Span::styled(
            "No column to analyse.",
            Style::new().fg(DIM),
        ))),
        Some(a) => {
            match &a.stats {
                Ok(s) => {
                    lines.push(section(&format!(
                        "Descriptive statistics — {} [{}]",
                        s.column,
                        s.kind.short()
                    )));
                    let label_w = s
                        .rows
                        .iter()
                        .map(|(k, _)| k.chars().count())
                        .max()
                        .unwrap_or(8);
                    for (k, v) in &s.rows {
                        lines.push(Line::from(vec![
                            Span::styled(
                                format!("  {:<w$} ", k, w = label_w),
                                Style::new().fg(ACCENT),
                            ),
                            Span::styled(
                                v.clone(),
                                Style::new().fg(Color::White).add_modifier(Modifier::BOLD),
                            ),
                        ]));
                    }
                }
                Err(e) => {
                    lines.push(section("Descriptive statistics"));
                    lines.extend(err_lines(e, inner_w));
                }
            }

            lines.push(Line::raw(""));
            match &a.distribution {
                Ok(d) => {
                    // The row total makes the percentages self-explanatory.
                    lines.push(section(&format!(
                        "Distribution — {} ({} rows)",
                        d.column, d.total
                    )));
                    if d.bars.is_empty() {
                        lines.push(Line::from(Span::styled(
                            "  no values",
                            Style::new().fg(DIM),
                        )));
                    } else {
                        if d.binned {
                            lines.push(Line::from(Span::styled(
                                "  (numeric bins)",
                                Style::new().fg(DIM),
                            )));
                        }
                        let label_w = d
                            .bars
                            .iter()
                            .map(|b| b.label.chars().count())
                            .max()
                            .unwrap_or(6)
                            .min(inner_w.saturating_sub(20).max(6));
                        let max = d.bars.iter().map(|b| b.count).max().unwrap_or(1).max(1);
                        let bar_w = inner_w.saturating_sub(label_w + 16).clamp(4, 24);
                        for b in &d.bars {
                            let filled = ((b.count as f64 / max as f64) * bar_w as f64).round()
                                as usize;
                            lines.push(Line::from(vec![
                                Span::styled(
                                    format!("  {:<w$} ", truncate(&b.label, label_w), w = label_w),
                                    Style::new().fg(Color::White),
                                ),
                                Span::styled(
                                    "█".repeat(filled.min(bar_w)),
                                    Style::new().fg(ACCENT),
                                ),
                                Span::styled(
                                    "·".repeat(bar_w.saturating_sub(filled)),
                                    Style::new().fg(DIM),
                                ),
                                Span::styled(
                                    format!(" {:>5} ", b.count),
                                    Style::new().fg(Color::LightGreen),
                                ),
                                Span::styled(
                                    crate::fmtnum::percent_2dp(b.share),
                                    Style::new().fg(DIM),
                                ),
                            ]));
                        }
                    }
                }
                Err(e) => {
                    lines.push(section("Distribution"));
                    lines.extend(err_lines(e, inner_w));
                }
            }

            lines.push(Line::raw(""));
            lines.push(section("Correlation (Pearson r)"));
            match &a.correlations {
                Ok(c) => {
                    // Ranked pairs first: the actionable summary.
                    for (x, y, r, strength) in c.pairs.iter().take(10) {
                        lines.push(Line::from(vec![
                            Span::styled("  ", Style::new()),
                            Span::styled(
                                format!("{x} ~ {y}"),
                                Style::new().fg(Color::White),
                            ),
                            Span::raw("  "),
                            Span::styled(
                                r.clone(),
                                Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(format!("  {strength}"), Style::new().fg(DIM)),
                        ]));
                    }
                    if !c.names.is_empty() {
                        lines.push(Line::raw(""));
                        // Compact matrix, indexed so long names stay readable.
                        let idx_w = 2;
                        let cell_w = c
                            .matrix
                            .iter()
                            .flatten()
                            .map(|v| v.chars().count())
                            .max()
                            .unwrap_or(5)
                            .max(5);
                        let mut head = vec![Span::raw("  ".to_string() + &" ".repeat(idx_w + 1))];
                        for i in 0..c.names.len() {
                            head.push(Span::styled(
                                format!("{:>w$} ", i + 1, w = cell_w),
                                Style::new().fg(DIM),
                            ));
                        }
                        lines.push(Line::from(head));
                        for (i, name) in c.names.iter().enumerate() {
                            let mut row = vec![Span::styled(
                                format!("  {:>w$} ", i + 1, w = idx_w),
                                Style::new().fg(DIM),
                            )];
                            for (j, v) in c.matrix[i].iter().enumerate() {
                                let style = if i == j {
                                    Style::new().fg(DIM)
                                } else if v
                                    .trim_start_matches('-')
                                    .parse::<f64>()
                                    .map(|x| x >= 0.7)
                                    .unwrap_or(false)
                                {
                                    Style::new().fg(ACCENT).add_modifier(Modifier::BOLD)
                                } else {
                                    Style::new().fg(Color::White)
                                };
                                row.push(Span::styled(format!("{:>w$} ", v, w = cell_w), style));
                            }
                            row.push(Span::styled(
                                format!(" {name}"),
                                Style::new().fg(Color::White),
                            ));
                            lines.push(Line::from(row));
                        }
                    }
                }
                Err(e) => lines.extend(err_lines(e, inner_w)),
            }
        }
    }

    let total = lines.len();
    let view_h = area.height.saturating_sub(2) as usize;
    // Clamp the scroll so the panel cannot be scrolled past its content.
    let max_scroll = total.saturating_sub(view_h) as u16;
    if app.analysis_scroll > max_scroll {
        app.analysis_scroll = max_scroll;
    }

    let title = Line::from(vec![
        Span::styled(
            " Analysis ",
            Style::new().fg(Color::Magenta).add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!("{column} "), Style::new().fg(Color::White)),
        Span::styled(format!("[{num_label}] "), Style::new().fg(KEY)),
    ]);

    let p = Paragraph::new(Text::from(lines))
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(Color::Magenta))
                .title(title),
        )
        .scroll((app.analysis_scroll, 0));
    f.render_widget(p, area);

    if total > view_h {
        let mut sb = ScrollbarState::default()
            .content_length(total)
            .viewport_content_length(view_h)
            .position(app.analysis_scroll as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(None)
                .end_symbol(None),
            scroll_track(area),
            &mut sb,
        );
    }
}

fn section(title: &str) -> Line<'static> {
    Line::from(Span::styled(
        title.to_string(),
        Style::new()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    ))
}

fn err_lines(msg: &str, width: usize) -> Vec<Line<'static>> {
    wrap_text(msg, width.saturating_sub(4).max(8))
        .into_iter()
        .map(|part| {
            Line::from(Span::styled(
                format!("  {part}"),
                Style::new().fg(Color::LightRed),
            ))
        })
        .collect()
}

/// Status message plus the context-sensitive key bar.
fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let split = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let color = match app.msg_kind {
        MsgKind::Info => Color::White,
        MsgKind::Good => Color::LightGreen,
        MsgKind::Error => Color::LightRed,
    };
    let prefix = match app.msg_kind {
        MsgKind::Info => "  ",
        MsgKind::Good => "✓ ",
        MsgKind::Error => "✗ ",
    };
    let msg = Paragraph::new(Line::from(vec![
        Span::styled(prefix, Style::new().fg(color)),
        Span::styled(app.message.clone(), Style::new().fg(color)),
    ]));
    f.render_widget(msg, split[0]);

    // The key bar lists the keys that apply right now, so the bindings are
    // always discoverable from inside the TUI.
    let keys: Vec<(&str, &str)> = if app.prompt.is_some() {
        vec![
            ("Enter", "confirm"),
            ("Esc", "cancel"),
            ("^u", "clear"),
            ("^w", "del word"),
        ]
    } else if app.focus == Focus::Query {
        vec![
            ("Enter", "run"),
            ("Tab", "table"),
            ("↑↓", "history"),
            ("^u", "clear line"),
            ("^l", "reset query"),
            ("F2/F3/F4", "mode"),
            ("F1", "help"),
        ]
    } else {
        vec![
            ("↑↓", "row"),
            ("←→", "column"),
            ("Tab", "query"),
            ("s/S", "sort/add"),
            ("R", "unsort"),
            ("a", "analysis"),
            ("e", "export"),
            ("x/F", "num fmt"),
            ("o/r", "open/reload"),
            ("?", "help"),
            ("q", "quit"),
        ]
    };
    let mut spans = vec![Span::raw("  ")];
    for (i, (k, d)) in keys.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" · ", Style::new().fg(DIM)));
        }
        spans.push(Span::styled(
            k.to_string(),
            Style::new().fg(KEY).add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(d.to_string(), Style::new().fg(DIM)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), split[1]);
}

/// Full-screen help, grouped by category as required.
fn draw_help(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Clear, area);

    let groups: [(&str, &[(&str, &str)]); 6] = [
        (
            "Navigation",
            &[
                ("↑ / k", "previous row"),
                ("↓ / j", "next row"),
                ("← / h", "previous column (analysis + sort target)"),
                ("→ / l", "next column"),
                ("PgUp / PgDn", "scroll a page of rows"),
                ("Space", "page down"),
                ("Home / g", "first row"),
                ("End / G", "last row"),
                ("J / K", "scroll the side panel"),
                ("Tab", "move between the query line and the table"),
                ("Esc", "leave the query line, close a panel, or clear the query"),
            ],
        ),
        (
            "Query modes",
            &[
                ("F2", "Fuzzy mode — keyword match across all columns, live"),
                ("F3", "SQL-Like mode — select where <condition>"),
                ("F4", "SQL mode — select * from df where ..."),
                ("m", "cycle to the next mode"),
                ("Shift-Tab", "cycle to the previous mode"),
                ("Ctrl-← / Ctrl-→", "previous / next mode"),
                ("/ or f", "jump to the query line"),
                ("Enter", "run the query (Fuzzy also runs as you type)"),
                ("↑ / ↓", "recall previous queries (in the query line)"),
                ("Ctrl-l or c", "clear the query and show every row"),
            ],
        ),
        (
            "Data operations",
            &[
                ("s", "sort by the focused column (press again to reverse)"),
                ("S", "add the focused column as a secondary sort key"),
                ("R", "remove all sort keys"),
                ("a", "toggle the analysis panel for the focused column"),
                ("e", "export the current result to a CSV path"),
                ("o", "open a different CSV file"),
                ("r", "reload the current file from disk"),
            ],
        ),
        (
            "Display",
            &[
                ("x", "cycle table number format: auto / 2 decimals / 3 sig figs"),
                ("F", "cycle analysis number format: 2 decimals / 3 sig figs"),
                ("a", "show statistics, distribution and correlation together"),
            ],
        ),
        (
            "Query syntax",
            &[
                ("Fuzzy", "Sales — matches any column; several words all must match"),
                ("SQL-Like", "select where age > 40"),
                ("SQL-Like", "select where department = 'Engineering' and salary > 10000"),
                ("SQL-Like", "supports and, or, not, =, !=, >, <, >=, <=,"),
                ("", "between .. and .., in (..), like '%x%', is null"),
                ("SQL", "select * from df where country = 'US' and score > 85"),
                ("SQL", "select department, count(*) from df group by department"),
                ("SQL", "the table is named df (also data, or the file stem)"),
            ],
        ),
        (
            "General",
            &[
                ("? or F1", "open this help page"),
                ("any key", "close this help page"),
                ("q / Ctrl-c", "quit toolk"),
            ],
        ),
    ];

    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled(
            "toolk",
            Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" — CSV data analysis TUI. Keyboard only; every action below has a key."),
    ]));
    lines.push(Line::raw(""));

    let key_w = 17;
    for (title, items) in groups {
        lines.push(section(title));
        for (k, d) in items {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:<w$} ", k, w = key_w),
                    Style::new().fg(KEY).add_modifier(Modifier::BOLD),
                ),
                Span::styled(d.to_string(), Style::new().fg(Color::White)),
            ]));
        }
        lines.push(Line::raw(""));
    }
    lines.push(Line::from(vec![
        Span::styled("Data file: ", Style::new().fg(DIM)),
        Span::styled(
            app.ds.path.to_string_lossy().to_string(),
            Style::new().fg(Color::White),
        ),
        Span::styled(
            format!(
                "  ({} rows, {} columns)",
                app.ds.total_rows(),
                app.ds.columns().len()
            ),
            Style::new().fg(DIM),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        format!("Columns: {}", column_summary(app)),
        Style::new().fg(DIM),
    )));

    let total = lines.len();
    let view_h = area.height.saturating_sub(2) as usize;
    let max_scroll = total.saturating_sub(view_h) as u16;
    if app.help_scroll > max_scroll {
        app.help_scroll = max_scroll;
    }

    let p = Paragraph::new(Text::from(lines))
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(ACCENT))
                .title(Line::from(vec![
                    Span::styled(
                        " Help ",
                        Style::new().fg(ACCENT).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled("↑↓ scroll · any other key closes ", Style::new().fg(DIM)),
                ])),
        )
        .scroll((app.help_scroll, 0));
    f.render_widget(p, area);

    if total > view_h {
        let mut sb = ScrollbarState::default()
            .content_length(total)
            .viewport_content_length(view_h)
            .position(app.help_scroll as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight),
            scroll_track(area),
            &mut sb,
        );
    }
}

/// `name (kind)` list of the loaded columns, for the help footer.
fn column_summary(app: &App) -> String {
    app.ds
        .columns()
        .iter()
        .map(|(n, k)| format!("{n} [{}]", k.short()))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The track a vertical scrollbar should occupy inside a bordered block, so it
/// never paints over the border corners or the title.
fn scroll_track(area: Rect) -> Rect {
    Rect {
        x: area.x,
        y: area.y + 1,
        width: area.width,
        height: area.height.saturating_sub(2),
    }
}

/// Truncate to `max` characters, marking the cut with `…`.
fn truncate(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let keep = max.saturating_sub(1);
    let mut out: String = s.chars().take(keep).collect();
    out.push('…');
    out
}

/// Hard-wrap text at `width` characters, preferring word boundaries.
fn wrap_text(s: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![s.to_string()];
    }
    if s.chars().count() <= width {
        return vec![s.to_string()];
    }
    let mut out = Vec::new();
    let mut line = String::new();
    let mut len = 0usize;
    for word in s.split_whitespace() {
        let wlen = word.chars().count();
        if len > 0 && len + 1 + wlen > width {
            out.push(std::mem::take(&mut line));
            len = 0;
        }
        if wlen > width {
            // A single oversized token is split hard so nothing is lost.
            if len > 0 {
                out.push(std::mem::take(&mut line));
                len = 0;
            }
            let mut chunk = String::new();
            for c in word.chars() {
                if chunk.chars().count() == width {
                    out.push(std::mem::take(&mut chunk));
                }
                chunk.push(c);
            }
            if !chunk.is_empty() {
                line = chunk;
                len = line.chars().count();
            }
            continue;
        }
        if len > 0 {
            line.push(' ');
            len += 1;
        }
        line.push_str(word);
        len += wlen;
    }
    if !line.is_empty() {
        out.push(line);
    }
    if out.is_empty() {
        out.push(String::new());
    }
    out
}

/// Shorten a long path by keeping the tail, which is the informative part.
fn shorten_path(p: &str, max: usize) -> String {
    let count = p.chars().count();
    if count <= max {
        return p.to_string();
    }
    let tail: String = p.chars().skip(count - max.saturating_sub(1)).collect();
    format!("…{tail}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_marks_the_cut() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello", 5), "hello");
        assert_eq!(truncate("hello", 4), "hel…");
        assert_eq!(truncate("héllo", 4), "hél…");
        assert_eq!(truncate("hello", 0), "");
    }

    #[test]
    fn wrapping_prefers_word_boundaries() {
        assert_eq!(wrap_text("a b c", 10), vec!["a b c"]);
        assert_eq!(wrap_text("aaa bbb ccc", 7), vec!["aaa bbb", "ccc"]);
        // An oversized token is split rather than dropped.
        let parts = wrap_text("abcdefghij", 4);
        assert_eq!(parts.join(""), "abcdefghij");
        assert!(parts.iter().all(|p| p.chars().count() <= 4));
    }

    #[test]
    fn long_paths_keep_their_tail() {
        let p = "/bench/data/very/deep/path/employees.csv";
        let s = shorten_path(p, 20);
        assert!(s.starts_with('…'));
        assert!(s.ends_with("employees.csv"));
        assert_eq!(s.chars().count(), 20);
        assert_eq!(shorten_path("/a/b.csv", 20), "/a/b.csv");
    }
}
