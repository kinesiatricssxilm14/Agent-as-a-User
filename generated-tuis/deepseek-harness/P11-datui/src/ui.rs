//! Terminal rendering for the toolk TUI.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState},
    Frame,
};

use crate::app::{describe_sort, App, Focus, QueryMode};

/// Maximum number of lines reserved for the selected-row detail panel.
const DETAIL_MAX: usize = 8;
/// Width of the row-number column.
const ROW_NUM_W: usize = 7;

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();

    if app.show_help {
        draw_help(f, area);
        return;
    }

    // Compute the detail panel height up front (needs the body width).
    // The detail panel is shown in every data-browsing context (table, query,
    // export) so the selected row's full column set stays visible.
    let is_data_view = !matches!(app.focus, Focus::Sort | Focus::Analysis);
    let detail_h = if is_data_view {
        let width = area.width.saturating_sub(4).max(20) as usize;
        app.build_detail(width);
        app.detail_lines.len().clamp(1, DETAIL_MAX)
    } else {
        0
    };

    let mut cons: Vec<Constraint> = vec![
        Constraint::Length(1), // header
        Constraint::Length(1), // query input
        Constraint::Min(3),    // body (table / sort / analysis)
    ];
    if detail_h > 0 {
        cons.push(Constraint::Length(detail_h as u16));
    }
    cons.push(Constraint::Length(1)); // status
    cons.push(Constraint::Length(2)); // footer

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(cons)
        .split(area);

    let header = chunks[0];
    let input = chunks[1];
    let body = chunks[2];
    let (detail, status, footer) = if detail_h > 0 {
        (Some(chunks[3]), chunks[4], chunks[5])
    } else {
        (None, chunks[3], chunks[4])
    };

    app.body_rows = body.height.saturating_sub(1) as usize;
    app.body_cols = body.width as usize;

    draw_header(f, header, app);
    draw_input(f, input, app);

    match app.focus {
        Focus::Analysis => draw_analysis(f, body, app),
        Focus::Sort => draw_sort(f, body, app),
        _ => {
            app.ensure_selected_visible(app.body_rows.max(1));
            draw_table(f, body, app);
            if let Some(d) = detail {
                draw_detail(f, d, app);
            }
        }
    }

    draw_status(f, status, app);
    draw_footer(f, footer, app);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let mode_color = match app.mode {
        QueryMode::Fuzzy => Color::Cyan,
        QueryMode::SqlLike => Color::Yellow,
        QueryMode::Sql => Color::Magenta,
    };
    let line = Line::from(vec![
        Span::styled(
            " toolk ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(" {} ", app.path)),
        Span::styled(
            format!("[{}]", app.mode.label()),
            Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
        ),
        Span::raw(format!(
            "  rows: {}  matches: {}  sort: {}",
            app.df.height(),
            app.table.row_count(),
            describe_sort(&app.sort_keys)
        )),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn draw_input(f: &mut Frame, area: Rect, app: &App) {
    let (text, cursor_col, focused, title, prompt) = if app.focus == Focus::Export {
        (
            app.export_editor.content.as_str(),
            app.export_editor.cursor_col(),
            true,
            "Export CSV".to_string(),
            "path> ".to_string(),
        )
    } else {
        (
            app.query_editor.content.as_str(),
            app.query_editor.cursor_col(),
            app.focus == Focus::Query,
            app.mode.label().to_string(),
            format!("{}> ", app.mode.prompt()),
        )
    };

    let border_color = if focused { Color::Yellow } else { Color::DarkGray };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(border_color))
        .title(format!(" {title} "));

    let before: String = text.chars().take(cursor_col).collect();
    let at: Option<char> = text.chars().nth(cursor_col);
    let after: String = text.chars().skip(cursor_col + 1).collect();

    let mut spans = vec![Span::styled(prompt, Style::default().fg(Color::DarkGray))];
    spans.push(Span::raw(before));
    if focused {
        let cursor_char = at.unwrap_or(' ');
        spans.push(Span::styled(
            cursor_char.to_string(),
            Style::default().bg(Color::White).fg(Color::Black),
        ));
        spans.push(Span::raw(after));
    } else {
        if let Some(c) = at {
            spans.push(Span::raw(c.to_string()));
        }
        spans.push(Span::raw(after));
    }

    let p = Paragraph::new(Line::from(spans)).block(block);
    f.render_widget(p, area);
}

fn draw_table(f: &mut Frame, area: Rect, app: &mut App) {
    if app.table.col_count() == 0 {
        let p = Paragraph::new("No columns / no data")
            .block(Block::default().borders(Borders::ALL).title(" Results "));
        f.render_widget(p, area);
        return;
    }

    let widths = &app.col_widths;
    let avail = area.width.saturating_sub(2).saturating_sub(ROW_NUM_W as u16) as usize;

    // Select the visible columns starting at the horizontal scroll offset.
    let mut vis: Vec<usize> = Vec::new();
    let mut used = 0usize;
    let mut c = app.scroll_col.min(widths.len().saturating_sub(1));
    while c < widths.len() {
        let w = widths[c];
        if vis.is_empty() || used + w <= avail {
            vis.push(c);
            used += w;
            c += 1;
        } else {
            break;
        }
    }
    if vis.is_empty() {
        let last = widths.len() - 1;
        vis.push(last);
        app.scroll_col = last;
    }

    let mut header_cells = vec![Cell::from(Span::styled(
        "#",
        Style::default().add_modifier(Modifier::BOLD),
    ))];
    for &i in &vis {
        header_cells.push(Cell::from(Span::styled(
            app.table.headers[i].clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )));
    }
    let header_row = Row::new(header_cells)
        .style(Style::default().bg(Color::DarkGray).fg(Color::White));

    let mut constraints = vec![Constraint::Length(ROW_NUM_W as u16)];
    for &i in &vis {
        constraints.push(Constraint::Length(widths[i] as u16));
    }

    let viewport = app.body_rows.max(1);
    let start = app.scroll_row.min(app.table.row_count().saturating_sub(1));
    let end = app.table.row_count().min(start + viewport);

    let mut rows: Vec<Row> = Vec::new();
    for r in start..end {
        let mut cells = vec![Cell::from((r + 1).to_string())];
        for &i in &vis {
            cells.push(Cell::from(truncate_cell(&app.table.rows[r][i], widths[i])));
        }
        rows.push(Row::new(cells));
    }

    let rel_selected = if app.table.row_count() == 0 {
        None
    } else {
        Some(app.selected.saturating_sub(start))
    };
    let mut state = TableState::default().with_selected(rel_selected);

    let title = format!(
        " Results ({} rows × {} cols, showing cols {}-{}/{}) ",
        app.table.row_count(),
        app.table.col_count(),
        vis.first().map(|i| i + 1).unwrap_or(1),
        vis.last().map(|i| i + 1).unwrap_or(1),
        app.table.col_count()
    );
    let table = Table::new(rows, constraints)
        .header(header_row)
        .block(Block::default().borders(Borders::ALL).title(title))
        .row_highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ")
        .column_spacing(1);

    f.render_stateful_widget(table, area, &mut state);
}

fn draw_detail(f: &mut Frame, area: Rect, app: &App) {
    let text = app.detail_lines.join("\n");
    let p = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Selected row (all columns) "));
    f.render_widget(p, area);
}

fn draw_status(f: &mut Frame, area: Rect, app: &App) {
    let style = if app.status_error {
        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::Green)
    };
    let p = Paragraph::new(Line::from(Span::styled(app.status.clone(), style)));
    f.render_widget(p, area);
}

fn draw_footer(f: &mut Frame, area: Rect, app: &App) {
    let (line1, line2) = match app.focus {
        Focus::Query => (
            "[Enter] run   [Esc] clear/back   [←→/Home/End] edit   [Ctrl+C] quit",
            "type your query — [Tab]/[m] switch mode from the table view",
        ),
        Focus::Export => (
            "[Enter] export CSV   [Esc] cancel   [Ctrl+C] quit",
            "path is resolved from the current working directory",
        ),
        Focus::Sort => (
            "[↑↓] column   [Enter] none→↑→↓   [x] clear all   [Esc] done",
            "columns sort in the order shown (multi-column sort)",
        ),
        Focus::Analysis => (
            "[↑↓/PgUp/PgDn] scroll   [←→] switch histogram column   [Esc] back",
            "statistics · correlation · distribution on one screen",
        ),
        Focus::Table => (
            "[↑↓/jk] row   [←→/hl] col   [Enter] run   [/ or i] query   [Tab/m] mode   [q] quit",
            "[s] sort   [a] analyze   [e] export   [r] reload   [?] help   [Ctrl+C] quit",
        ),
    };
    let p = Paragraph::new(vec![
        Line::from(Span::styled(line1, Style::default().fg(Color::DarkGray))),
        Line::from(Span::styled(line2, Style::default().fg(Color::DarkGray))),
    ]);
    f.render_widget(p, area);
}

fn draw_sort(f: &mut Frame, area: Rect, app: &App) {
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(Span::styled(
        "Sort columns — Enter cycles: none → ↑ ascending → ↓ descending → none",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    for (i, h) in app.table.headers.iter().enumerate() {
        let (marker, is_sorted) = if let Some(pos) = app.sort_keys.iter().position(|k| k.column == *h) {
            let k = &app.sort_keys[pos];
            let arrow = if k.ascending { "↑" } else { "↓" };
            (format!("{}.{}", pos + 1, arrow), true)
        } else {
            ("   ".to_string(), false)
        };
        let prefix = if i == app.sort_selected { "▶ " } else { "  " };
        let mut style = if i == app.sort_selected {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default()
        };
        if is_sorted {
            style = style.fg(Color::Cyan);
        }
        lines.push(Line::from(Span::styled(
            format!("{prefix}{marker}  {h}"),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("Current sort: {}", describe_sort(&app.sort_keys)),
        Style::default().fg(Color::Cyan),
    )));

    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Sort "));
    f.render_widget(p, area);
}

fn draw_analysis(f: &mut Frame, area: Rect, app: &App) {
    let lines: Vec<Line> = match &app.report {
        Some(r) => r.lines.iter().map(|l| Line::from(l.clone())).collect(),
        None => vec![Line::from("no analysis available")],
    };
    let p = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title(" Analysis "))
        .scroll((app.analysis_scroll as u16, 0));
    f.render_widget(p, area);
}

fn draw_help(f: &mut Frame, area: Rect) {
    let text = [
        "toolk — help",
        "",
        "Navigation",
        "  ↑ / k, ↓ / j        move selected row",
        "  ← / h, → / l        scroll columns left / right",
        "  PageUp / PageDown    page through rows",
        "  Home / End           first / last row",
        "",
        "Data operations",
        "  /  or  i             focus the query input",
        "  Enter                run the current query",
        "  Esc                  clear input / go back",
        "  Tab  or  m           switch query mode (Fuzzy / SQL-Like / SQL)",
        "  s                    sort columns (Enter cycles ↑ / ↓ / clear)",
        "  a                    analysis: statistics + correlation + distribution",
        "  e                    export current results to a CSV path",
        "  r                    reload the CSV file",
        "  x                    (in the sort view) clear all sorts",
        "",
        "Query modes",
        "  Fuzzy      keyword matches any column (case-insensitive substring)",
        "  SQL-Like   e.g.  select where age > 40",
        "             e.g.  select where department = 'Engineering' and salary > 10000",
        "  SQL        e.g.  select * from df where country = 'US' and score > 85",
        "             e.g.  select * from df where age = 35 and salary > 5000 and salary < 15000",
        "",
        "Display",
        "  ?                    this help",
        "  q                    quit",
        "  Ctrl+C               quit",
    ]
    .join("\n");

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Help — press any key to close ");
    let p = Paragraph::new(text).block(block);
    f.render_widget(Clear, area);
    f.render_widget(p, area);
}

/// Truncate a cell string to fit its column (content width = column width - 2).
fn truncate_cell(s: &str, width: usize) -> String {
    let w = width.saturating_sub(2);
    let count = s.chars().count();
    if count <= w {
        s.to_string()
    } else if w <= 1 {
        "…".to_string()
    } else {
        let mut out: String = s.chars().take(w - 1).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    const CSV: &str = "\
id,name,department,age,salary,country,score\n\
1,Alice,Engineering,45,12000.50,US,92.3\n\
2,Bob,Sales,38,8500.00,UK,78.0\n\
3,Carol,Engineering,52,15000.00,US,88.9\n";

    fn make_app(tag: &str) -> crate::app::App {
        let path = std::env::temp_dir().join(format!("toolk_ui_{}_{}.csv", std::process::id(), tag));
        std::fs::write(&path, CSV).unwrap();
        let app = crate::app::App::new(path.to_str().unwrap()).unwrap();
        let _ = std::fs::remove_file(&path);
        app
    }

    fn buffer_string(buf: &ratatui::buffer::Buffer) -> String {
        let area = *buf.area();
        let content = buf.content();
        let mut s = String::new();
        for y in 0..area.height {
            for x in 0..area.width {
                s.push_str(content[buf.index_of(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    #[test]
    fn table_view_renders() {
        let mut app = make_app("table");
        let backend = TestBackend::new(140, 44);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let text = buffer_string(terminal.backend().buffer());
        assert!(text.contains("toolk"), "header should show the app name");
        assert!(text.contains("department"), "header row should be visible");
        assert!(text.contains("Alice"), "data row should be visible");
    }

    #[test]
    fn help_and_analysis_render_without_panic() {
        let mut app = make_app("analysis");
        app.show_help = true;
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();

        app.show_help = false;
        app.open_analysis();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
        let text = buffer_string(terminal.backend().buffer());
        assert!(text.contains("Correlation") || text.contains("correlation"));
    }

    #[test]
    fn empty_result_renders_without_panic() {
        let mut app = make_app("empty");
        app.mode = crate::app::QueryMode::Fuzzy;
        app.query_editor.set_content("no-such-value-anywhere");
        app.run_query();
        assert_eq!(app.table.row_count(), 0);
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, &mut app)).unwrap();
    }
}
