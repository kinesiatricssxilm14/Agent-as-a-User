//! Terminal rendering with ratatui.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::app::{App, Focus, Mode};

const COL_WIDTH: usize = 22;

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.clamp();

    if area.width < 40 || area.height < 8 {
        let msg = Paragraph::new("Terminal too small.\n\nResize to at least 40×8.")
            .block(Block::default().borders(Borders::ALL).title(" toolb "));
        f.render_widget(msg, area);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(1),    // main (board + detail)
            Constraint::Length(1), // status / input / confirm
            Constraint::Length(2), // help bar
        ])
        .split(area);

    draw_header(f, chunks[0], app);

    match app.mode {
        Mode::Help => draw_help(f, chunks[1]),
        _ => {
            let hsplit = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(20), Constraint::Percentage(35)])
                .split(chunks[1]);
            draw_columns(f, hsplit[0], app);
            draw_detail(f, hsplit[1], app);
        }
    }

    draw_status(f, chunks[2], app);
    draw_help_bar(f, chunks[3], app);
}

fn draw_header(f: &mut Frame, area: Rect, app: &App) {
    let left = format!(" toolb · {}", app.root.display());
    let mut right = match app.mode {
        Mode::Normal => "NORMAL".to_string(),
        Mode::Input => "INPUT".to_string(),
        Mode::Confirm => "CONFIRM".to_string(),
        Mode::Help => "HELP".to_string(),
    };
    if let Some(f) = &app.filter {
        right = format!("{} · filter:\"{}\"", right, f);
    }

    let width = area.width as usize;
    let left = truncate(&left, width);
    let right = truncate(&right, width);
    let fill = width.saturating_sub(left.chars().count() + right.chars().count());

    let line = Line::from(vec![
        Span::styled(
            left,
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" ".repeat(fill), Style::default().bg(Color::Cyan)),
        Span::styled(
            right,
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn draw_columns(f: &mut Frame, area: Rect, app: &mut App) {
    if app.board.columns.is_empty() {
        let p = Paragraph::new(vec![
            Line::from(" No columns yet."),
            Line::from(""),
            Line::from("  c  create a column"),
            Line::from("  ?  show help"),
        ])
        .block(Block::default().borders(Borders::ALL).title(" Board "));
        f.render_widget(p, area);
        return;
    }

    let ncols = (area.width as usize / COL_WIDTH).max(1);
    // Keep the selected column within the horizontal viewport.
    if app.col_scroll > app.col_index {
        app.col_scroll = app.col_index;
    }
    if app.col_index >= app.col_scroll + ncols {
        app.col_scroll = app.col_index + 1 - ncols;
    }

    let start = app.col_scroll;
    let end = (start + ncols).min(app.board.columns.len());
    for i in start..end {
        let x = ((i - start) * COL_WIDTH) as u16;
        let avail = area.width.saturating_sub(x);
        if avail == 0 {
            break;
        }
        let w = avail.min(COL_WIDTH as u16);
        let r = Rect {
            x: area.x + x,
            y: area.y,
            width: w,
            height: area.height,
        };
        render_column(f, r, app, i);
    }
}

fn render_column(f: &mut Frame, r: Rect, app: &mut App, col_idx: usize) {
    let is_selected = col_idx == app.col_index && app.focus == Focus::Board;
    let name = app.board.columns[col_idx].name.clone();
    let count = app.board.columns[col_idx].cards.len();
    let vis = app.visible_indices(col_idx);

    // Vertical auto-scroll so the selected card stays visible.
    let inner_h = (r.height.saturating_sub(2)) as usize;
    if is_selected && inner_h > 0 {
        let cur = app.card_scrolls[col_idx];
        if app.card_index < cur {
            app.card_scrolls[col_idx] = app.card_index;
        } else if app.card_index >= cur + inner_h {
            app.card_scrolls[col_idx] = app.card_index + 1 - inner_h;
        }
    }

    let border_style = if is_selected {
        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(format!(" {} ({}) ", name, count));

    let mut lines: Vec<Line> = Vec::new();
    for (pos, &real) in vis.iter().enumerate() {
        let card = &app.board.columns[col_idx].cards[real];
        let sel = is_selected && pos == app.card_index;
        let marker = if sel { "▶ " } else { "  " };
        let style = if sel {
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", marker, card.title),
            style,
        )));
    }
    if lines.is_empty() {
        lines.push(Line::from(Span::styled(
            " (empty)",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let scroll = app.card_scrolls[col_idx] as u16;
    let p = Paragraph::new(lines).block(block).scroll((scroll, 0));
    f.render_widget(p, r);
}

fn draw_detail(f: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title(" Card Details ");
    let card = app.selected_card();
    let col_name = app.selected_column().map(|c| c.name.clone()).unwrap_or_default();

    match card {
        None => {
            let p = Paragraph::new(
                "No card selected.\n\nMove selection with ↑↓/jk and press Tab to focus this panel.",
            )
            .block(block);
            f.render_widget(p, area);
        }
        Some(card) => {
            let mut lines: Vec<Line> = Vec::new();
            lines.push(Line::from(Span::styled(
                format!("Title: {}", card.title),
                Style::default().add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(Span::styled(
                format!("ID: {}   Column: {}", card.id, col_name),
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "─ Description ─",
                Style::default().fg(Color::Cyan),
            )));
            if card.body.trim().is_empty() {
                lines.push(Line::from(Span::styled(
                    "(empty)",
                    Style::default().fg(Color::DarkGray),
                )));
            } else {
                for l in card.body.lines() {
                    lines.push(Line::from(l.to_string()));
                }
            }
            let p = Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((app.detail_scroll, 0));
            f.render_widget(p, area);
        }
    }
}

fn draw_status(f: &mut Frame, area: Rect, app: &App) {
    match app.mode {
        Mode::Input => {
            let prompt = app.input.action.prompt();
            let buf = with_cursor(&app.input.buffer, app.input.cursor);
            let text = format!(" {}: {}", prompt, buf);
            let style = Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD);
            f.render_widget(Paragraph::new(Span::styled(text, style)), area);
        }
        Mode::Confirm => {
            let text = app.confirm_text();
            let style = Style::default().fg(Color::Red).add_modifier(Modifier::BOLD);
            f.render_widget(Paragraph::new(Span::styled(text, style)), area);
        }
        _ => match &app.message {
            Some(m) => {
                let style = if app.error {
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Green)
                };
                f.render_widget(Paragraph::new(Span::styled(format!(" {}", m), style)), area);
            }
            None => {
                let style = Style::default().fg(Color::DarkGray);
                f.render_widget(Paragraph::new(Span::styled(" Ready", style)), area);
            }
        },
    }
}

fn draw_help_bar(f: &mut Frame, area: Rect, app: &App) {
    let gray = Style::default().fg(Color::DarkGray);
    let lines: Vec<Line> = match app.mode {
        Mode::Input => vec![
            Line::from(Span::styled(
                " Enter confirm   Esc cancel   ←→ move cursor   Backspace delete   Ctrl+U clear   Ctrl+W word",
                gray,
            )),
            Line::from(""),
        ],
        Mode::Confirm => vec![
            Line::from(Span::styled(" y / Enter confirm    n / Esc cancel", gray)),
            Line::from(""),
        ],
        Mode::Help => vec![
            Line::from(Span::styled(" Press any key to return", gray)),
            Line::from(""),
        ],
        Mode::Normal => vec![
            Line::from(Span::styled(
                " q quit   ? help   ↑↓/jk card   ←→/hl column   Tab focus   g/G first·last",
                gray,
            )),
            Line::from(Span::styled(
                " n new   e edit   a append   d delete   c column   D del-column   </> move   / filter   r reload",
                gray,
            )),
        ],
    };
    f.render_widget(Paragraph::new(lines), area);
}

fn draw_help(f: &mut Frame, area: Rect) {
    let text = vec![
        Line::from(Span::styled(
            "toolb — keyboard reference",
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(" Navigation"),
        Line::from("   ↑/↓  or  j/k       select card"),
        Line::from("   ←/→  or  h/l       select column"),
        Line::from("   Tab                switch focus (board ↔ card details)"),
        Line::from("   g / G              jump to first / last card"),
        Line::from(""),
        Line::from(" Cards"),
        Line::from("   n                  new card in the current column"),
        Line::from("   e / Enter          edit the card title"),
        Line::from("   a                  append a line to the card body"),
        Line::from("   d                  delete the card (asks for confirmation)"),
        Line::from("   < / >              move card to previous / next column"),
        Line::from(""),
        Line::from(" Columns"),
        Line::from("   c                  create a new column"),
        Line::from("   D                  delete the current column (asks for confirmation)"),
        Line::from(""),
        Line::from(" Other"),
        Line::from("   /                  filter cards by text (Esc clears)"),
        Line::from("   r                  reload the board from disk"),
        Line::from("   ?                  this help"),
        Line::from("   q / Esc            quit"),
        Line::from(""),
        Line::from(" Press any key to return."),
    ];
    let p = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title(" Help "))
        .wrap(Wrap { trim: false });
    f.render_widget(p, area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

fn with_cursor(s: &str, pos: usize) -> String {
    let mut out = String::new();
    let mut i = 0;
    for ch in s.chars() {
        if i == pos {
            out.push('▏');
        }
        out.push(ch);
        i += 1;
    }
    if pos >= i {
        out.push('▏');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Mode};
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn tmp_board() -> PathBuf {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "toolb-ui-test-{}-{}",
            std::process::id(),
            n
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("cols/todo")).unwrap();
        fs::create_dir_all(root.join("cols/doing")).unwrap();
        fs::write(root.join("board.txt"), "col todo \"TO DO\"\ncol doing \"DOING\"\n").unwrap();
        fs::write(root.join("cols/todo/order.txt"), "item-1\n").unwrap();
        fs::write(
            root.join("cols/todo/item-1.md"),
            "# Fix login bug\nInvestigate timeout on mobile clients.\n",
        )
        .unwrap();
        root
    }

    fn render_text(app: &mut App) -> String {
        let backend = TestBackend::new(120, 40);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect()
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn type_text(app: &mut App, s: &str) {
        for ch in s.chars() {
            app.on_key(key(KeyCode::Char(ch)));
        }
    }

    #[test]
    fn renders_board_and_details() {
        let root = tmp_board();
        let mut app = App::new(root.clone()).unwrap();
        let text = render_text(&mut app);
        assert!(text.contains("TO DO"));
        assert!(text.contains("DOING"));
        assert!(text.contains("Fix login bug"));
        assert!(text.contains("Investigate timeout on mobile clients."));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn full_card_lifecycle_via_keys() {
        let root = tmp_board();
        let mut app = App::new(root.clone()).unwrap();

        // navigate columns
        app.on_key(key(KeyCode::Char('l')));
        assert_eq!(app.col_index, 1);
        app.on_key(key(KeyCode::Char('h')));
        assert_eq!(app.col_index, 0);

        // create a card
        app.on_key(key(KeyCode::Char('n')));
        assert!(matches!(app.mode, Mode::Input));
        type_text(&mut app, "Ship v2");
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.selected_card().unwrap().title, "Ship v2");
        let card_file = fs::read_to_string(root.join("cols/todo/ship-v2.md")).unwrap();
        assert!(card_file.starts_with("# Ship v2\n"));
        let order = fs::read_to_string(root.join("cols/todo/order.txt")).unwrap();
        assert!(order.lines().any(|l| l == "ship-v2"));

        // edit the title
        app.on_key(key(KeyCode::Char('e')));
        type_text(&mut app, "!");
        app.on_key(key(KeyCode::Enter));
        assert_eq!(app.selected_card().unwrap().title, "Ship v2!");

        // move to next column
        app.on_key(key(KeyCode::Char('>')));
        assert_eq!(app.col_index, 1);
        assert!(root.join("cols/doing/ship-v2.md").exists());
        assert!(!root.join("cols/todo/ship-v2.md").exists());

        // delete (confirm with y)
        app.on_key(key(KeyCode::Char('d')));
        assert!(matches!(app.mode, Mode::Confirm));
        app.on_key(key(KeyCode::Char('y')));
        assert!(!root.join("cols/doing/ship-v2.md").exists());
        assert!(app.selected_card().is_none());

        let _ = fs::remove_dir_all(&root);
    }
}
