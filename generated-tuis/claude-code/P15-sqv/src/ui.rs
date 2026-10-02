//! Terminal rendering. Purely visual: it never talks to SQLite.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Focus, Pane, Prompt, StatusKind};
use crate::grid::Grid;
use crate::input::Input;
use crate::keymap::{footer_hints, HintContext, SECTIONS};

const COL_W: u16 = 16;

pub fn draw(frame: &mut Frame, app: &mut App) {
    let area = frame.area();
    let prompt = app.prompt.is_some();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(6),
            Constraint::Length(if prompt { 2 } else { 1 }),
        ])
        .split(area);

    render_title(frame, app, chunks[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(28), Constraint::Min(24)])
        .split(chunks[1]);

    render_tables(frame, app, body[0]);
    match app.pane {
        Pane::Data | Pane::Sql => render_data(frame, app, body[1]),
        Pane::Schema => render_schema(frame, app, body[1]),
        Pane::Help => render_help(frame, app, body[1]),
    }

    render_footer(frame, app, chunks[2]);
}

fn title_style() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn focused_border(on: bool) -> Style {
    if on {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn render_title(frame: &mut Frame, app: &App, area: Rect) {
    let obj = app
        .current_object()
        .map(|o| format!("{} {}", o.kind.label(), o.name))
        .unwrap_or_else(|| "no table".into());
    let pane = app.pane.title();
    let focus = app.focus.label();
    let n = app.objects.len();
    let line = format!(
        " toolo  {}  ·  {obj}  ·  {pane}  ·  focus:{focus}  ·  {n} objects ",
        app.db.path().display()
    );
    frame.render_widget(Paragraph::new(line).style(title_style()), area);
}

fn render_tables(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Tables;
    let inner_h = area.height.saturating_sub(2) as usize;
    app.table_height = inner_h.max(1);
    if app.table_cursor < app.table_offset {
        app.table_offset = app.table_cursor;
    } else if app.table_cursor >= app.table_offset + app.table_height {
        app.table_offset = app.table_cursor + 1 - app.table_height;
    }
    let max_off = app.object_view.len().saturating_sub(app.table_height);
    app.table_offset = app.table_offset.min(max_off);

    let title = if app.table_filter.is_empty() {
        format!(" Tables ({}) ", app.object_view.len())
    } else {
        format!(
            " Tables ({}/{}) /{} ",
            app.object_view.len(),
            app.objects.len(),
            app.table_filter
        )
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(focused_border(focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    let end = (app.table_offset + app.table_height).min(app.object_view.len());
    for view_i in app.table_offset..end {
        let obj = &app.objects[app.object_view[view_i]];
        let marker = if view_i == app.table_cursor { ">" } else { " " };
        let count = obj
            .row_count
            .map(|n| n.to_string())
            .unwrap_or_else(|| "?".into());
        let text = format!("{marker} {:<14} {:>5} {}", obj.name, count, obj.kind.label());
        let mut style = Style::default();
        if view_i == app.table_cursor {
            style = style.bg(Color::Cyan).fg(Color::Black).add_modifier(Modifier::BOLD);
        }
        lines.push(Line::from(Span::styled(text, style)));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            " (no tables) ",
            Style::default().fg(Color::DarkGray),
        )));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_data(frame: &mut Frame, app: &mut App, area: Rect) {
    let sql = app.pane == Pane::Sql;
    let chunks = if sql {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),
                Constraint::Min(4),
                Constraint::Length(7),
            ])
            .split(area)
    } else {
        Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(6), Constraint::Length(8)])
            .split(area)
    };

    if sql {
        render_sql_editor(frame, app, chunks[0]);
        render_grid(frame, app, chunks[1]);
        render_detail(frame, app, chunks[2]);
    } else {
        render_grid(frame, app, chunks[0]);
        render_detail(frame, app, chunks[1]);
    }
}

fn render_sql_editor(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::SqlEditor;
    let block = Block::default()
        .title(" SQL ")
        .borders(Borders::ALL)
        .border_style(focused_border(focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let text = format!("> {}", app.sql_input.text());
    frame.render_widget(Paragraph::new(text), inner);
}

fn grid_title(g: &Grid) -> String {
    let name = g.source.as_deref().unwrap_or("query");
    let shown = g.visible_rows();
    let total = g.total_rows();
    let mut extra = String::new();
    if let Some(s) = g.sort() {
        if let Some(col) = g.columns.get(s.column) {
            extra.push_str(&format!(" · sort {} {}", col.name, s.dir.arrow()));
        }
    }
    for f in g.filters() {
        extra.push_str(" · ");
        extra.push_str(&f.summary());
    }
    format!(" {name}  {shown}/{total} rows{extra} ")
}

fn n_visible_cols(width: u16) -> usize {
    let inner = width.saturating_sub(2);
    (inner / (COL_W + 1)).max(1) as usize
}

fn render_grid(frame: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Content && matches!(app.pane, Pane::Data | Pane::Sql);
    let n_cols = n_visible_cols(area.width);
    let inner_h = area.height.saturating_sub(3) as usize; // border + header
    app.grid_height = inner_h.max(1);
    let grid_height = app.grid_height;

    if let Some(g) = app.active_grid_mut() {
        g.scroll_into_view(grid_height);
        if g.col_cursor < g.col_offset {
            g.col_offset = g.col_cursor;
        } else if g.col_cursor >= g.col_offset + n_cols {
            g.col_offset = g.col_cursor + 1 - n_cols;
        }
        let max_off = g.columns.len().saturating_sub(n_cols);
        g.col_offset = g.col_offset.min(max_off);
    }

    let title = match app.active_grid() {
        Some(g) => grid_title(g),
        None => " Data ".into(),
    };
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(focused_border(focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(g) = app.active_grid() else {
        frame.render_widget(
            Paragraph::new("no data — pick a table on the left or run SQL with 3"),
            inner,
        );
        return;
    };
    if g.columns.is_empty() {
        frame.render_widget(Paragraph::new("(no columns)"), inner);
        return;
    }

    let col0 = g.col_offset;
    let col1 = (col0 + n_cols).min(g.columns.len());
    let mut header = String::new();
    for ci in col0..col1 {
        let col = &g.columns[ci];
        let mut name = col.name.clone();
        if let Some(s) = g.sort() {
            if s.column == ci {
                name.push(' ');
                name.push_str(s.dir.arrow());
            }
        }
        if g.filter_on(ci).is_some() {
            name.push('*');
        }
        if ci == g.col_cursor {
            name = format!("[{name}]");
        }
        header.push_str(&pad(&name, COL_W as usize));
        header.push(' ');
    }

    let mut lines = vec![Line::from(Span::styled(
        header,
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD),
    ))];

    let row0 = g.row_offset;
    let row1 = (row0 + app.grid_height).min(g.visible_rows());
    for vi in row0..row1 {
        let Some(row) = g.row_at(vi) else { break };
        let mut text = String::new();
        for ci in col0..col1 {
            let cell = row
                .cells
                .get(ci)
                .map(|v| v.display())
                .unwrap_or_default();
            text.push_str(&pad(&cell, COL_W as usize));
            text.push(' ');
        }
        let mut style = Style::default();
        if vi == g.cursor() {
            style = style.bg(Color::Blue).fg(Color::White).add_modifier(Modifier::BOLD);
        } else if vi % 2 == 1 {
            style = style.bg(Color::Rgb(32, 32, 40));
        }
        lines.push(Line::from(Span::styled(text, style)));
    }
    if g.visible_rows() == 0 {
        lines.push(Line::from(Span::styled(
            "(no rows match)",
            Style::default().fg(Color::DarkGray),
        )));
    }
    frame.render_widget(Paragraph::new(lines), inner);
}

fn render_detail(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Record ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(g) = app.active_grid() else {
        return;
    };
    let Some(row) = g.current_row() else {
        frame.render_widget(Paragraph::new("no row selected"), inner);
        return;
    };
    let mut parts: Vec<String> = Vec::new();
    for (i, col) in g.columns.iter().enumerate() {
        let val = row
            .cells
            .get(i)
            .map(|v| v.display())
            .unwrap_or_else(|| "NULL".into());
        parts.push(format!("{}={}", col.name, val));
    }
    let text = parts.join("  ·  ");
    frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), inner);
}

fn render_schema(frame: &mut Frame, app: &App, area: Rect) {
    let focused = app.focus == Focus::Content;
    let block = Block::default()
        .title(" Schema ")
        .borders(Borders::ALL)
        .border_style(focused_border(focused));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(schema) = &app.schema else {
        frame.render_widget(Paragraph::new("no schema loaded"), inner);
        return;
    };
    let mut lines: Vec<Line> = Vec::new();
    lines.push(Line::from(format!(
        "{} {}{}",
        schema.kind.label(),
        schema.object,
        if schema.without_rowid {
            "  WITHOUT ROWID"
        } else {
            ""
        }
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(schema.create_sql.clone()));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "columns",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    for c in &schema.columns {
        let pk = if c.pk_index > 0 {
            format!(" pk{}", c.pk_index)
        } else {
            String::new()
        };
        let nn = if c.not_null { " NOT NULL" } else { "" };
        lines.push(Line::from(format!(
            "  {} {}{}{}",
            c.name, c.decl_type, nn, pk
        )));
    }
    if !schema.indexes.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "indexes",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        for ix in &schema.indexes {
            lines.push(Line::from(format!(
                "  {} ({}) {}",
                ix.name,
                ix.columns.join(", "),
                if ix.unique { "UNIQUE" } else { "" }
            )));
        }
    }
    if !schema.foreign_keys.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            "foreign keys",
            Style::default().add_modifier(Modifier::BOLD),
        )));
        for fk in &schema.foreign_keys {
            lines.push(Line::from(format!(
                "  {} -> {}.{}",
                fk.column, fk.target_table, fk.target_column
            )));
        }
    }
    let skip = app.schema_scroll as usize;
    let shown: Vec<Line> = lines.into_iter().skip(skip).collect();
    frame.render_widget(Paragraph::new(shown).wrap(Wrap { trim: false }), inner);
}

fn render_help(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(" Help ")
        .borders(Borders::ALL)
        .border_style(focused_border(true));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    for section in SECTIONS {
        lines.push(Line::from(Span::styled(
            section.title,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        for b in section.bindings {
            lines.push(Line::from(format!("  {:<22} {}", b.keys, b.description)));
        }
        lines.push(Line::from(""));
    }
    let skip = app.help_scroll as usize;
    let shown: Vec<Line> = lines.into_iter().skip(skip).collect();
    frame.render_widget(Paragraph::new(shown), inner);
}

fn render_footer(frame: &mut Frame, app: &App, area: Rect) {
    if let Some(prompt) = &app.prompt {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Length(1)])
            .split(area);
        let (label, input, extra) = prompt_bits(app, prompt);
        let line = format!("{label} {extra}  {}", input.text());
        frame.render_widget(
            Paragraph::new(line).style(Style::default().fg(Color::Black).bg(Color::Yellow)),
            chunks[0],
        );
        let hints = footer_hints(prompt_hint(prompt));
        frame.render_widget(
            Paragraph::new(hints).style(Style::default().fg(Color::Black).bg(Color::Gray)),
            chunks[1],
        );
        return;
    }

    let (fg, bg) = match app.status.kind {
        StatusKind::Info => (Color::Black, Color::Gray),
        StatusKind::Success => (Color::Black, Color::Green),
        StatusKind::Warn => (Color::Black, Color::Yellow),
        StatusKind::Error => (Color::White, Color::Red),
    };
    let hints = footer_hints(status_hint(app));
    let msg = if app.status.text.is_empty() {
        hints.to_string()
    } else {
        format!("{}   ·   {hints}", app.status.text)
    };
    frame.render_widget(Paragraph::new(msg).style(Style::default().fg(fg).bg(bg)), area);
}

fn prompt_bits<'a>(app: &'a App, prompt: &'a Prompt) -> (String, &'a Input, String) {
    match prompt {
        Prompt::Filter(p) => (
            format!("Filter {} [{}]", p.column_name, p.mode.label()),
            &app.filter_input,
            if p.case_sensitive { "Aa" } else { "" }.into(),
        ),
        Prompt::Edit(p) => (
            format!(
                "Edit {}{}",
                p.column_name,
                if p.set_null { " [NULL]" } else { "" }
            ),
            &app.edit_input,
            String::new(),
        ),
        Prompt::Search => ("Search".into(), &app.search_input, String::new()),
        Prompt::TableFilter => ("Tables /".into(), &app.search_input, String::new()),
    }
}

fn prompt_hint(prompt: &Prompt) -> HintContext {
    match prompt {
        Prompt::Filter(_) => HintContext::FilterPrompt,
        Prompt::Edit(_) => HintContext::EditPrompt,
        Prompt::Search => HintContext::SearchPrompt,
        Prompt::TableFilter => HintContext::TableFilterPrompt,
    }
}

fn status_hint(app: &App) -> HintContext {
    if app.focus == Focus::Tables {
        return HintContext::Tables;
    }
    match app.pane {
        Pane::Schema => HintContext::Schema,
        Pane::Help => HintContext::Help,
        Pane::Sql if app.focus == Focus::SqlEditor => HintContext::SqlEditor,
        Pane::Sql => HintContext::SqlGrid,
        Pane::Data => HintContext::Data,
    }
}

fn pad(s: &str, width: usize) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        if out.chars().count() >= width {
            break;
        }
        out.push(ch);
    }
    while out.chars().count() < width {
        out.push(' ');
    }
    out
}
