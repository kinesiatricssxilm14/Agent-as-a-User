//! Rendering for the TUI.

use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap};
use ratatui::Frame;
use regex::Regex;
use unicode_width::UnicodeWidthChar;

use crate::app::{col_starts, total_width_of};
use crate::db::QueryOutcome;
use crate::model::{display_width, FilterMode, Focus, Prompt, PromptKind, View};

const ROW_HL_BG: Color = Color::Rgb(40, 42, 46);
const CURSOR_BG: Color = Color::Rgb(0, 110, 160);
const GUTTER: usize = 6;

fn header_style() -> Style {
    Style::default().fg(Color::Black).bg(Color::Gray).add_modifier(Modifier::BOLD)
}
fn row_style() -> Style {
    Style::default().bg(ROW_HL_BG)
}
fn cursor_style() -> Style {
    Style::default().bg(CURSOR_BG).fg(Color::White).add_modifier(Modifier::BOLD)
}
fn sep_style() -> Style {
    Style::default().fg(Color::DarkGray)
}
fn border_style(focused: bool) -> Style {
    if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

pub fn draw(frame: &mut Frame, app: &mut crate::app::App) {
    let area = frame.area();

    let mut constraints: Vec<Constraint> = vec![Constraint::Length(1)];
    if app.banner.is_some() {
        constraints.push(Constraint::Length(1));
    }
    constraints.push(Constraint::Min(4));
    constraints.push(Constraint::Length(1));

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(area);

    let mut i = 0usize;
    let title_area = chunks[i];
    i += 1;
    let banner_area = if app.banner.is_some() {
        let a = chunks[i];
        i += 1;
        Some(a)
    } else {
        None
    };
    let body = chunks[i];
    i += 1;
    let status_area = chunks[i];

    render_title(frame, app, title_area);

    if let (Some(barea), Some(banner)) = (banner_area, &app.banner) {
        let line = Line::from(Span::styled(
            format!("  {}", banner),
            Style::default().fg(Color::White).bg(Color::Red),
        ));
        frame.render_widget(Paragraph::new(line), barea);
    }

    let table_width = (area.width / 4).clamp(18, 32);
    let bs = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(table_width), Constraint::Min(0)])
        .split(body);
    let tables_area = bs[0];
    let main_area = bs[1];

    render_tables(frame, app, tables_area);
    match app.view {
        View::Data => render_data(frame, app, main_area),
        View::Schema => render_schema(frame, app, main_area),
        View::Query => render_query(frame, app, main_area),
    }

    if let Some(p) = &app.prompt {
        let line = prompt_line(p);
        let pstyle = Style::default().fg(Color::Black).bg(Color::Yellow);
        frame.render_widget(Paragraph::new(line).style(pstyle), status_area);
    } else {
        render_status(frame, app, status_area);
    }

    if app.help_open {
        render_help(frame, area);
    }
}

fn render_title(frame: &mut Frame, app: &crate::app::App, area: Rect) {
    let mut spans: Vec<Span> = vec![
        Span::styled(" toolo ", Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw(" "),
        Span::styled(app.db_path.clone(), Style::default().add_modifier(Modifier::BOLD)),
    ];
    if let Some(idx) = app.selected_table {
        if let Some(t) = app.tables.get(idx) {
            spans.push(Span::raw("  ·  "));
            spans.push(Span::styled(t.name.clone(), Style::default().fg(Color::Cyan)));
        }
    }
    spans.push(Span::raw(format!("  ({} tables)", app.tables.len())));
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_status(frame: &mut Frame, app: &crate::app::App, area: Rect) {
    let left = if app.status.is_empty() {
        "? help · Tab focus · 1/2/3 view · o open · q quit".to_string()
    } else {
        app.status.clone()
    };
    let hint = match app.view {
        View::Data => "j/k move · e edit · f filter · s/S sort · x clear",
        View::Schema => "j/k scroll · Tab back to tables",
        View::Query => "type SQL · Enter run · Tab to results",
    };
    let avail = area.width as usize;
    let left_w = display_width(&left);
    let hint_w = display_width(hint);
    let mut line = left;
    if left_w + hint_w + 2 <= avail {
        let pad = avail - left_w - hint_w;
        line.push_str(&" ".repeat(pad));
        line.push_str(hint);
    }
    frame.render_widget(
        Paragraph::new(line).style(Style::default().fg(Color::Black).bg(Color::Gray)),
        area,
    );
}

fn render_tables(frame: &mut Frame, app: &mut crate::app::App, area: Rect) {
    let idxs = app.filtered_table_indices();
    let items: Vec<ListItem> = idxs
        .iter()
        .map(|&i| {
            let mark = if Some(i) == app.selected_table { "*" } else { " " };
            ListItem::new(format!(" {} {}", mark, app.tables[i].name))
        })
        .collect();

    let focused = app.focus == Focus::Tables;
    let mut title = format!(" Tables ({}) ", app.tables.len());
    if !app.table_search.is_empty() {
        title = format!(" Tables ({}) /'{}' ", app.tables.len(), app.table_search);
    }
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style(focused))
        .title(title);
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White))
        .highlight_symbol("");
    frame.render_stateful_widget(list, area, &mut app.table_state);
}

fn render_data(frame: &mut Frame, app: &mut crate::app::App, area: Rect) {
    let focused = matches!(app.focus, Focus::Main);
    let table_name = app
        .selected_table
        .and_then(|i| app.tables.get(i))
        .map(|t| t.name.clone())
        .unwrap_or_default();
    let sort = match app.sort_col {
        Some(c) => {
            let name = app.columns.get(c).map(|c| c.name.clone()).unwrap_or_default();
            format!("{} {}", name, if app.sort_asc { "ASC" } else { "DESC" })
        }
        None => "—".to_string(),
    };
    let title = format!(
        " Data · {} · {} rows ({} shown) · sort {} · {} filter(s) ",
        table_name,
        app.all_rows.len(),
        app.sorted_order.len(),
        sort,
        app.filters.len()
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style(focused))
        .title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    app.data_view_width = inner.width as usize;
    app.data_view_height = inner.height.saturating_sub(2) as usize;

    if app.columns.is_empty() {
        let msg = if app.tables.is_empty() {
            "No tables in database. Press o to open a database file."
        } else {
            "No data."
        };
        frame.render_widget(
            Paragraph::new(msg).style(Style::default().fg(Color::DarkGray)),
            inner,
        );
        return;
    }

    let headers: Vec<String> = app.columns.iter().map(|c| c.name.clone()).collect();
    let row_count = app.sorted_order.len();
    let vscroll = app.vscroll;
    let hscroll = app.hscroll;
    let row_cursor = app.row_cursor;
    let col_cursor = app.col_cursor;
    render_grid(
        frame.buffer_mut(),
        inner,
        &headers,
        &app.col_widths,
        row_count,
        vscroll,
        hscroll,
        row_cursor,
        col_cursor,
        &|r, c| {
            let idx = app.sorted_order[r];
            app.all_rows[idx].cells[c].display()
        },
    );
}

fn render_schema(frame: &mut Frame, app: &crate::app::App, area: Rect) {
    let focused = matches!(app.focus, Focus::Main);
    let table_name = app
        .selected_table
        .and_then(|i| app.tables.get(i))
        .map(|t| t.name.clone())
        .unwrap_or_default();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style(focused))
        .title(format!(" Schema · {} ", table_name));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let sql = app
        .selected_table
        .and_then(|i| app.tables.get(i))
        .and_then(|t| t.sql.clone())
        .unwrap_or_else(|| "(no CREATE statement available)".to_string());

    let mut text = String::new();
    text.push_str("CREATE statement:\n");
    text.push_str(&sql);
    text.push_str("\n\n");
    text.push_str(&format!("Columns ({}):\n", app.columns.len()));
    text.push_str("  name | type | notnull | pk | default\n");
    for c in &app.columns {
        text.push_str(&format!(
            "  {} | {} | {} | {} | {}\n",
            c.name,
            c.declared_type,
            if c.notnull { "yes" } else { "no" },
            c.pk,
            c.default.clone().unwrap_or_default()
        ));
    }
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((app.schema_scroll as u16, 0)),
        inner,
    );
}

fn render_query(frame: &mut Frame, app: &mut crate::app::App, area: Rect) {
    let qs = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(3)])
        .split(area);
    let results_area = qs[0];
    let input_area = qs[1];

    // SQL input box.
    let input_focused = app.focus == Focus::QueryInput;
    let input_block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style(input_focused))
        .title(" SQL ");
    let input_inner = input_block.inner(input_area);
    frame.render_widget(input_block, input_area);
    let line = text_line_with_cursor(&app.query_buf.text, app.query_buf.cursor);
    frame.render_widget(Paragraph::new(line), input_inner);

    // Results panel.
    let result_focused = app.focus == Focus::QueryResult;
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style(result_focused))
        .title(format!(" Result · {} ", app.query_status));
    let inner = block.inner(results_area);
    frame.render_widget(block, results_area);

    app.query_view_width = inner.width as usize;
    app.query_view_height = inner.height.saturating_sub(2) as usize;

    match &app.query_outcome {
        Some(QueryOutcome::Rows { columns, rows }) => {
            let headers = columns.clone();
            let row_count = rows.len();
            let qscroll = app.query_scroll;
            let qhscroll = app.query_hscroll;
            let qrow = app.query_row_cursor;
            let qcol = app.query_col_cursor;
            render_grid(
                frame.buffer_mut(),
                inner,
                &headers,
                &app.query_col_widths,
                row_count,
                qscroll,
                qhscroll,
                qrow,
                qcol,
                &|r, c| app.query_cell(r, c),
            );
        }
        Some(QueryOutcome::Affected { n }) => {
            frame.render_widget(
                Paragraph::new(format!("{} row(s) affected", n)),
                inner,
            );
        }
        None => {
            frame.render_widget(
                Paragraph::new("Enter SQL below and press Enter to run.").style(Style::default().fg(Color::DarkGray)),
                inner,
            );
        }
    }
}

/// Render a tabular grid with a fixed header, a row-number gutter, and
/// two-dimensional scrolling. `cell(r, c)` returns the display text for row
/// `r` (0-based, in display order) and column `c`.
#[allow(clippy::too_many_arguments)]
fn render_grid(
    buf: &mut Buffer,
    area: Rect,
    headers: &[String],
    widths: &[usize],
    row_count: usize,
    vscroll: usize,
    hscroll: usize,
    row_cursor: usize,
    col_cursor: usize,
    cell: &dyn Fn(usize, usize) -> String,
) {
    let starts = col_starts(widths);
    let total = total_width_of(widths);
    let view_w = area.width.saturating_sub(GUTTER as u16) as usize;
    let hscroll = hscroll.min(total.saturating_sub(view_w));
    let left_limit = area.x + GUTTER as u16;
    let right_limit = area.right();
    let ncols = headers.len();

    // Header row.
    buf.set_string(area.x, area.y, " row #", header_style());
    for c in 0..ncols {
        let start = starts.get(c).copied().unwrap_or(0);
        let w = widths.get(c).copied().unwrap_or(8);
        let x = area.x as i64 + GUTTER as i64 + start as i64 - hscroll as i64;
        let text = pad_display(&headers[c], w + 1);
        draw_clipped(buf, x, area.y, &text, header_style(), left_limit, right_limit);
    }

    // Separator.
    let sep_y = area.y + 1;
    let sep = "─".repeat(area.width as usize);
    buf.set_string(area.x, sep_y, &sep, sep_style());

    // Data rows.
    let body_h = area.height.saturating_sub(2) as usize;
    for d in 0..body_h {
        let r = vscroll + d;
        if r >= row_count {
            break;
        }
        let y = area.y + 2 + d as u16;
        let rn = format!("{:>5} ", r + 1);
        let gstyle = if r == row_cursor {
            Style::default().fg(Color::White).bg(ROW_HL_BG)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        buf.set_string(area.x, y, &rn, gstyle);

        for c in 0..ncols {
            let start = starts.get(c).copied().unwrap_or(0);
            let w = widths.get(c).copied().unwrap_or(8);
            let x = area.x as i64 + GUTTER as i64 + start as i64 - hscroll as i64;
            let text = cell(r, c);
            let highlight = r == row_cursor;
            let is_cursor = highlight && c == col_cursor;
            let style = if is_cursor {
                cursor_style()
            } else if highlight {
                row_style()
            } else {
                Style::default()
            };
            let rendered = if highlight {
                pad_display(&text, w + 1)
            } else {
                text
            };
            draw_clipped(buf, x, y, &rendered, style, left_limit, right_limit);
        }
    }
}

fn draw_clipped(buf: &mut Buffer, x: i64, y: u16, text: &str, style: Style, left_limit: u16, right_limit: u16) {
    if x >= right_limit as i64 {
        return;
    }
    let left = (left_limit as i64).max(x).max(0) as u16;
    if left >= right_limit {
        return;
    }
    if y >= buf.area.height {
        return;
    }
    let skip = (left as i64 - x).max(0) as usize;
    let s = clip_skip(text, skip);
    let maxw = (right_limit - left) as usize;
    let s = clip_display(s, maxw);
    buf.set_string(left, y, s, style);
}

fn clip_skip(s: &str, mut skip: usize) -> &str {
    if skip == 0 {
        return s;
    }
    for (i, c) in s.char_indices() {
        let w = UnicodeWidthChar::width(c).unwrap_or(0);
        if skip == 0 {
            return &s[i..];
        }
        skip = skip.saturating_sub(w);
    }
    &s[s.len()..]
}

fn clip_display(s: &str, maxw: usize) -> &str {
    let mut w = 0usize;
    for (i, c) in s.char_indices() {
        let cw = UnicodeWidthChar::width(c).unwrap_or(0);
        if w + cw > maxw {
            return &s[..i];
        }
        w += cw;
    }
    s
}

fn pad_display(s: &str, w: usize) -> String {
    let cur = display_width(s);
    if cur >= w {
        clip_display(s, w).to_string()
    } else {
        let mut out = s.to_string();
        out.push_str(&" ".repeat(w - cur));
        out
    }
}

fn byte_index(s: &str, char_idx: usize) -> usize {
    s.char_indices().nth(char_idx).map(|(i, _)| i).unwrap_or(s.len())
}

fn text_line_with_cursor(text: &str, cursor: usize) -> Line<'static> {
    let byte = byte_index(text, cursor);
    let before = &text[..byte];
    let cur_char = text[byte..].chars().next();
    let (cur_disp, after) = match cur_char {
        Some(c) => (c.to_string(), &text[byte + c.len_utf8()..]),
        None => (" ".to_string(), &text[byte..]),
    };
    Line::from(vec![
        Span::raw(before.to_string()),
        Span::styled(cur_disp, Style::default().bg(Color::White).fg(Color::Black)),
        Span::raw(after.to_string()),
    ])
}

fn prompt_line(p: &Prompt) -> Line<'static> {
    let mut spans: Vec<Span> = vec![Span::styled(
        format!(" {} ", p.title),
        Style::default().fg(Color::Black).bg(Color::Yellow).add_modifier(Modifier::BOLD),
    )];

    if let PromptKind::Filter { .. } = p.kind {
        spans.push(Span::styled(
            format!(" [{}] ", p.mode.label()),
            Style::default().fg(Color::Black).bg(Color::Yellow),
        ));
        if p.mode == FilterMode::Regex {
            let ok = Regex::new(&p.buffer.text).is_ok();
            spans.push(Span::styled(
                if ok { " ok " } else { " invalid! " },
                Style::default()
                    .fg(Color::Black)
                    .bg(if ok { Color::Green } else { Color::Red }),
            ));
        }
    }

    let byte = p.buffer.cursor_byte();
    let t = &p.buffer.text;
    let before = &t[..byte];
    let cur_char = t[byte..].chars().next();
    let (cur_disp, after) = match cur_char {
        Some(c) => (c.to_string(), &t[byte + c.len_utf8()..]),
        None => (" ".to_string(), &t[byte..]),
    };
    spans.push(Span::raw(before.to_string()));
    spans.push(Span::styled(cur_disp, Style::default().bg(Color::White).fg(Color::Black)));
    spans.push(Span::raw(after.to_string()));
    Line::from(spans)
}

fn render_help(frame: &mut Frame, area: Rect) {
    let w = (area.width * 3 / 4).min(area.width).max(40);
    let h = (area.height * 3 / 4).min(area.height).max(20);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    let box_rect = Rect::new(x, y, w, h);
    frame.render_widget(Clear, box_rect);

    let text = r#" toolo — SQLite Database Browser

 GLOBAL
   q / Ctrl+C      quit            o / Ctrl+O   open database
   F1 or ?         this help       Tab          move focus
   1 / 2 / 3       Data / Schema / Query view
   Esc             back to Tables

 TABLE LIST (left panel)
   j/k or ↑/↓      move           Enter        load table
   /               filter table list (live)

 DATA VIEW (right panel)
   j/k or ↑/↓      move row       h/l or ←/→   move column
   PgUp/PgDn       page           g/G          first / last row
   e or Enter      edit cell      f            filter column
                                   (Tab cycles contains → regex → exact)
   s               sort column ascending
   S               sort column descending
   u               clear sort     x            clear filters
   r               reload data

 QUERY VIEW
   type SQL, Enter to run. ↑/↓ recall history.
   Tab moves focus between input and results.

 FILTER MATCHES
   contains = substring, exact = whole value,
   regex = Rust regex syntax (e.g. ^foo.*bar$).
   Matching is case-sensitive unless you write (?i).
"#;

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Help ");
    frame.render_widget(
        Paragraph::new(text).block(block).wrap(Wrap { trim: true }),
        box_rect,
    );
}

#[cfg(test)]
mod render_tests {
    use super::*;
    use crate::app::App;
    use crate::db::Database;
    use crate::model::{CellValue, Focus, View};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn unique_db_path() -> String {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        format!(
            "{}/toolo_render_{}_{}.db",
            std::env::temp_dir().display(),
            std::process::id(),
            n
        )
    }

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let backend = TestBackend::new(w, h);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        let buf = terminal.backend().buffer();
        let mut out = String::new();
        for y in 0..h {
            for x in 0..w {
                out.push_str(buf.cell((x, y)).unwrap().symbol());
            }
            out.push('\n');
        }
        out
    }

    fn setup_app() -> (App, String) {
        let path = unique_db_path();
        let _ = std::fs::remove_file(&path);
        let db = Database::open(&path).unwrap();
        db.run_query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER)").unwrap();
        db.run_query("INSERT INTO users (name, age) VALUES ('alice', 30), ('bob', 25)").unwrap();
        db.run_query("CREATE TABLE orders (id INTEGER PRIMARY KEY, total REAL)").unwrap();
        db.run_query("INSERT INTO orders (total) VALUES (9.5)").unwrap();
        drop(db);

        let mut app = App::new(path.clone());
        app.init();
        (app, path)
    }

    #[test]
    fn renders_tables_and_data() {
        let (mut app, path) = setup_app();
        let users_idx = app.tables.iter().position(|t| t.name == "users").unwrap();
        app.select_table(users_idx);
        app.focus = Focus::Main;
        app.view = View::Data;
        let out = render(&mut app, 100, 30);
        assert!(out.contains("users"), "table list should show users");
        assert!(out.contains("orders"), "table list should show orders");
        assert!(out.contains("name"), "data grid should show column name");
        assert!(out.contains("alice"), "data grid should show row value");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn renders_schema() {
        let (mut app, path) = setup_app();
        app.view = View::Schema;
        let out = render(&mut app, 100, 30);
        assert!(out.contains("CREATE"), "schema should show CREATE statement");
        assert!(out.contains("name"), "schema should list columns");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn renders_query_results() {
        let (mut app, path) = setup_app();
        app.view = View::Query;
        app.query_outcome = Some(QueryOutcome::Rows {
            columns: vec!["x".to_string(), "y".to_string()],
            rows: vec![
                vec![CellValue::Integer(1), CellValue::Text("a".to_string())],
                vec![CellValue::Integer(2), CellValue::Text("b".to_string())],
            ],
        });
        app.query_col_widths = vec![1, 1];
        let out = render(&mut app, 100, 30);
        assert!(out.contains("x"), "query grid should show column x");
        assert!(out.contains("b"), "query grid should show value b");
        let _ = std::fs::remove_file(&path);
    }
}
