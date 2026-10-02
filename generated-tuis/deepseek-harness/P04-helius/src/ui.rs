//! Terminal rendering with ratatui.

use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Tabs},
    Frame,
};

use crate::app::{App, Confirm, View};
use crate::form::Field;
use crate::models::TransactionDetail;
use crate::util;

const CYAN: Color = Color::Cyan;
const GREEN: Color = Color::Green;
const RED: Color = Color::Red;
const YELLOW: Color = Color::Yellow;
const DARK: Color = Color::DarkGray;
const GRAY: Color = Color::Gray;

fn selected_style() -> Style {
    Style::default().add_modifier(Modifier::REVERSED)
}

fn header_style() -> Style {
    Style::default()
        .add_modifier(Modifier::BOLD)
        .add_modifier(Modifier::UNDERLINED)
}

fn cell_right(text: String) -> Cell<'static> {
    Cell::from(Line::from(text).alignment(Alignment::Right))
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();

    if let Some(confirm) = &app.confirm {
        draw_confirm(frame, area, confirm);
    } else if let Some(form) = &app.form {
        draw_form(frame, area, form);
    } else if let Some(detail) = &app.detail {
        draw_detail(frame, area, detail);
    } else {
        draw_main(frame, area, app);
    }
}

fn draw_main(frame: &mut Frame, area: Rect, app: &App) {
    let chunks = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    draw_tabs(frame, chunks[0], app);
    match app.view {
        View::Summary => draw_summary(frame, chunks[1], app),
        View::Accounts => draw_accounts(frame, chunks[1], app),
        View::Categories => draw_categories(frame, chunks[1], app),
        View::Transactions => draw_transactions(frame, chunks[1], app),
        View::Budgets => draw_budgets(frame, chunks[1], app),
        View::Help => draw_help(frame, chunks[1], app),
    }
    draw_status(frame, chunks[2], app);
    draw_helpbar(frame, chunks[3], app);
}

fn draw_tabs(frame: &mut Frame, area: Rect, app: &App) {
    let titles: Vec<String> = View::ALL
        .iter()
        .map(|v| format!(" {} ", v.label()))
        .collect();
    let tabs = Tabs::new(titles)
        .select(app.view.index())
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" toold — personal finance ledger "),
        )
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_widget(tabs, area);
}

fn draw_status(frame: &mut Frame, area: Rect, app: &App) {
    if let Some(s) = &app.status {
        let style = if s.is_error {
            Style::default().fg(RED)
        } else {
            Style::default().fg(GREEN)
        };
        frame.render_widget(Paragraph::new(Span::styled(s.text.clone(), style)), area);
    }
}

fn draw_helpbar(frame: &mut Frame, area: Rect, app: &App) {
    let text = match app.view {
        View::Summary => "←/→ or +/- : month · t : today · Tab : next view · 1-6 : jump · ? : help · q : quit",
        View::Accounts => "↑/↓ : select · a : add account · d : delete · Tab : next view · ? : help · q : quit",
        View::Categories => "↑/↓ : select · a : add category · d : delete · Tab : next view · ? : help · q : quit",
        View::Transactions => {
            "↑/↓ : select · i : income · e : expense · Enter : details · d : delete · f : filter · ? : help · q : quit"
        }
        View::Budgets => "↑/↓ : select · a : add · e : edit · d : delete · ←/→ : month · ? : help · q : quit",
        View::Help => "any key : return · q : quit",
    };
    let p = Paragraph::new(Span::styled(text, Style::default().fg(DARK)));
    frame.render_widget(p, area);
}

// ---- summary ------------------------------------------------------------

fn draw_summary(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title(format!(
        " SUMMARY — {} (currency: CNY ¥) ",
        util::month_str(app.month.0, app.month.1)
    ));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::vertical([Constraint::Length(6), Constraint::Min(0)]).split(inner);
    let cards = Layout::horizontal([
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
        Constraint::Ratio(1, 3),
    ])
    .split(rows[0]);

    draw_stat_card(frame, cards[0], " INCOME ", app.summary.income_cents, GREEN);
    draw_stat_card(frame, cards[1], " EXPENSE ", app.summary.expense_cents, RED);
    let net = app.summary.net_cents();
    let net_color = if net >= 0 { GREEN } else { RED };
    draw_stat_card(frame, cards[2], " NET INCOME ", net, net_color);

    let table_rows: Vec<Row> = app
        .breakdown
        .iter()
        .map(|ct| {
            Row::new(vec![
                Cell::from(ct.name.clone()),
                Cell::from(ct.kind.clone()),
                cell_right(util::fmt_cents(ct.total_cents)),
            ])
        })
        .collect();
    let table = Table::new(
        table_rows,
        [
            Constraint::Percentage(40),
            Constraint::Percentage(20),
            Constraint::Percentage(40),
        ],
    )
    .header(Row::new(vec!["Category", "Type", "Total"]).style(header_style()))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" By category "),
    );
    frame.render_widget(table, rows[1]);
}

fn draw_stat_card(frame: &mut Frame, area: Rect, title: &str, cents: i64, color: Color) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(color));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let amount = util::fmt_cents(cents);
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            amount,
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )),
    ];
    frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

// ---- list views ---------------------------------------------------------

fn draw_accounts(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title(" ACCOUNTS ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.accounts.is_empty() {
        draw_empty_hint(frame, inner, "No accounts yet.\n\nPress 'a' to create one (accounts separate funding sources, e.g. personal / business).");
        return;
    }
    let rows: Vec<Row> = app
        .accounts
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let mut row = Row::new(vec![
                Cell::from(a.name.clone()),
                Cell::from(a.account_type.clone()),
            ]);
            if i == app.selected {
                row = row.style(selected_style());
            }
            row
        })
        .collect();
    let table = Table::new(
        rows,
        [Constraint::Percentage(60), Constraint::Percentage(40)],
    )
    .header(Row::new(vec!["Name", "Type"]).style(header_style()));
    frame.render_widget(table, inner);
}

fn draw_categories(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title(" CATEGORIES ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.categories.is_empty() {
        draw_empty_hint(
            frame,
            inner,
            "No categories yet.\n\nPress 'a' to create one (income or expense).",
        );
        return;
    }
    let rows: Vec<Row> = app
        .categories
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let mut row = Row::new(vec![Cell::from(c.name.clone()), Cell::from(c.kind.clone())]);
            if i == app.selected {
                row = row.style(selected_style());
            }
            row
        })
        .collect();
    let table = Table::new(
        rows,
        [Constraint::Percentage(60), Constraint::Percentage(40)],
    )
    .header(Row::new(vec!["Name", "Kind"]).style(header_style()));
    frame.render_widget(table, inner);
}

fn draw_transactions(frame: &mut Frame, area: Rect, app: &App) {
    let title = format!(" TRANSACTIONS [filter: {}] ", app.tx_filter.label());
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.transactions.is_empty() {
        draw_empty_hint(
            frame,
            inner,
            "No transactions.\n\nPress 'i' to record income or 'e' to record an expense.",
        );
        return;
    }
    let rows: Vec<Row> = app
        .transactions
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let mut row = Row::new(vec![
                Cell::from(t.date.clone()),
                Cell::from(t.kind.clone()),
                cell_right(util::fmt_cents(t.amount_cents)),
                Cell::from(t.account.clone()),
                Cell::from(t.category.clone()),
                Cell::from(t.payee.clone()),
            ]);
            if i == app.selected {
                row = row.style(selected_style());
            }
            row
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Length(12),
            Constraint::Length(9),
            Constraint::Length(12),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ],
    )
    .header(
        Row::new(vec![
            "Date", "Type", "Amount", "Account", "Category", "Payee",
        ])
        .style(header_style()),
    );
    frame.render_widget(table, inner);
}

fn draw_budgets(frame: &mut Frame, area: Rect, app: &App) {
    let title = format!(
        " BUDGETS — {} (currency: CNY ¥) ",
        util::month_str(app.month.0, app.month.1)
    );
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    if app.budgets.is_empty() {
        draw_empty_hint(frame, inner, "No budgets configured for this month.\n\nPress 'a' to set a monthly budget for a category.");
        return;
    }
    let rows: Vec<Row> = app
        .budgets
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let remaining = b.amount_cents - b.spent_cents;
            let mut row = Row::new(vec![
                Cell::from(b.account.clone()),
                Cell::from(b.category.clone()),
                cell_right(util::fmt_cents(b.amount_cents)),
                cell_right(util::fmt_cents(b.spent_cents)),
                cell_right(util::fmt_cents(remaining)),
            ]);
            if i == app.selected {
                row = row.style(selected_style());
            }
            row
        })
        .collect();
    let table = Table::new(
        rows,
        [
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
            Constraint::Percentage(20),
        ],
    )
    .header(
        Row::new(vec!["Account", "Category", "Budget", "Spent", "Remaining"]).style(header_style()),
    );
    frame.render_widget(table, inner);
}

fn draw_empty_hint(frame: &mut Frame, area: Rect, hint: &str) {
    let p = Paragraph::new(hint)
        .alignment(Alignment::Center)
        .style(Style::default().fg(GRAY));
    // Rough vertical centering.
    let vpad = area.height.saturating_sub(3) / 2;
    let centered = Rect {
        x: area.x,
        y: area.y + vpad,
        width: area.width,
        height: 3.min(area.height),
    };
    frame.render_widget(p, centered);
}

// ---- help ---------------------------------------------------------------

fn draw_help(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default().borders(Borders::ALL).title(" HELP ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = vec![
        Line::from(Span::styled("Navigation", Style::default().fg(YELLOW).add_modifier(Modifier::BOLD))),
        Line::from("  Tab / Shift+Tab     next / previous view"),
        Line::from("  1-6                 jump: 1 SUMMARY · 2 ACCOUNTS · 3 CATEGORIES · 4 TRANSACTIONS · 5 BUDGETS · 6 HELP"),
        Line::from("  ↑/↓                 move selection in a list"),
        Line::from("  ?                   this help (any key returns)"),
        Line::from(""),
        Line::from(Span::styled("Transactions", Style::default().fg(YELLOW).add_modifier(Modifier::BOLD))),
        Line::from("  i / e               record income / expense"),
        Line::from("  Enter               open transaction details"),
        Line::from("  d                   delete selected transaction"),
        Line::from("  f                   cycle filter: all / income / expense"),
        Line::from(""),
        Line::from(Span::styled("Accounts & Categories", Style::default().fg(YELLOW).add_modifier(Modifier::BOLD))),
        Line::from("  a                   add new account / category"),
        Line::from("  d                   delete selected (only when unused)"),
        Line::from(""),
        Line::from(Span::styled("Budgets & Summary", Style::default().fg(YELLOW).add_modifier(Modifier::BOLD))),
        Line::from("  a / e / d           add / edit / delete budget"),
        Line::from("  ←/→  or  +/-       change viewed month"),
        Line::from("  t                   jump to current month"),
        Line::from(""),
        Line::from(Span::styled("Forms", Style::default().fg(YELLOW).add_modifier(Modifier::BOLD))),
        Line::from("  Enter               next field / submit"),
        Line::from("  Tab / Shift+Tab     next / previous field"),
        Line::from("  ←/→                move cursor (or cycle a choice)"),
        Line::from("  +/-                 change date by one day"),
        Line::from("  Esc                 cancel"),
        Line::from(""),
        Line::from(Span::styled("Global", Style::default().fg(YELLOW).add_modifier(Modifier::BOLD))),
        Line::from("  q / Ctrl-C          quit"),
        Line::from(""),
        Line::from("All money is stored as integer cents and shown in yuan (CNY/¥) with two decimals."),
        Line::from(""),
        Line::from(Span::styled(
            format!("Database: {}", app.db_path.display()),
            Style::default().fg(DARK),
        )),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}

// ---- detail screen ------------------------------------------------------

fn draw_detail(frame: &mut Frame, area: Rect, d: &TransactionDetail) {
    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);

    let block = Block::default()
        .borders(Borders::ALL)
        .title(" TRANSACTION DETAIL ")
        .border_style(Style::default().fg(CYAN));
    let inner = block.inner(chunks[0]);
    frame.render_widget(block, chunks[0]);

    let kind_display = match d.kind.as_str() {
        "income" => "income",
        _ => "expense",
    };
    let amount = format!("{} CNY (¥)", util::fmt_cents(d.amount_cents));
    let account = format!("{} ({})", d.account, d.account_type);

    let label = |s: &str| {
        Span::styled(
            format!("{:<12}", s),
            Style::default().fg(DARK).add_modifier(Modifier::BOLD),
        )
    };
    let lines = vec![
        Line::from(vec![label("ID:"), Span::raw(d.id.to_string())]),
        Line::from(vec![label("Type:"), Span::raw(kind_display.to_string())]),
        Line::from(vec![label("Amount:"), Span::raw(amount)]),
        Line::from(vec![label("Account:"), Span::raw(account)]),
        Line::from(vec![label("Category:"), Span::raw(d.category.clone())]),
        Line::from(vec![label("Date:"), Span::raw(d.date.clone())]),
        Line::from(vec![label("Payee:"), Span::raw(d.payee.clone())]),
        Line::from(vec![label("Notes:"), Span::raw(d.notes.clone())]),
        Line::from(vec![label("Created:"), Span::raw(d.created_at.clone())]),
    ];
    frame.render_widget(Paragraph::new(lines), inner);

    let hint = Paragraph::new(Span::styled(
        "Esc / Enter / q : back · d : delete this transaction · q (again) : quit",
        Style::default().fg(DARK),
    ));
    frame.render_widget(hint, chunks[1]);
}

// ---- form ---------------------------------------------------------------

fn draw_form(frame: &mut Frame, area: Rect, form: &crate::form::Form) {
    let hint = "Enter : next field / submit · Tab / Shift+Tab : prev / next · ↑/↓ : field · ←/→ : cursor / cycle · +/- : date · Esc : cancel";
    let chunks = Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).split(area);

    let height = (form.fields.len() as u16 + 3).min(chunks[0].height);
    let width = 76.min(chunks[0].width);
    let rect = Rect {
        x: chunks[0].x + (chunks[0].width.saturating_sub(width)) / 2,
        y: chunks[0].y + (chunks[0].height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" {} ", form.title))
        .border_style(Style::default().fg(CYAN));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let mut lines: Vec<Line> = form
        .fields
        .iter()
        .enumerate()
        .map(|(i, f)| render_field_line(f, i == form.focus))
        .collect();
    if let Some(err) = &form.error {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            format!("⚠ {}", err),
            Style::default().fg(RED),
        )));
    }
    frame.render_widget(Paragraph::new(lines), inner);

    frame.render_widget(
        Paragraph::new(Span::styled(hint, Style::default().fg(DARK))),
        chunks[1],
    );
}

fn render_field_line(field: &Field, focused: bool) -> Line<'static> {
    let label_style = if focused {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Gray)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(DARK)
    };
    let mut spans: Vec<Span<'static>> =
        vec![Span::styled(format!("{:>11} ", field.label()), label_style)];

    match field {
        Field::Choice {
            options, selected, ..
        } => {
            let val = options
                .get(*selected)
                .cloned()
                .unwrap_or_else(|| "(none)".to_string());
            spans.push(Span::raw(format!("◀ {} ▶", val)));
        }
        _ => {
            let value = field.value();
            let placeholder = field.placeholder();
            let display: &str = if value.is_empty() {
                &placeholder
            } else {
                &value
            };
            let cursor = field.cursor().min(display.len());

            if focused {
                let before = &display[..cursor];
                let after = &display[cursor..];
                spans.push(Span::raw(before.to_string()));
                if let Some(ch) = after.chars().next() {
                    let clen = ch.len_utf8();
                    spans.push(Span::styled(
                        ch.to_string(),
                        Style::default().bg(Color::White).fg(Color::Black),
                    ));
                    spans.push(Span::raw(after[clen..].to_string()));
                } else {
                    spans.push(Span::styled(
                        " ".to_string(),
                        Style::default().bg(Color::White).fg(Color::Black),
                    ));
                }
            } else {
                let s = if value.is_empty() {
                    placeholder.clone()
                } else {
                    value.clone()
                };
                spans.push(Span::raw(s));
            }
        }
    }
    Line::from(spans)
}

// ---- confirm ------------------------------------------------------------

fn draw_confirm(frame: &mut Frame, area: Rect, confirm: &Confirm) {
    let msg = &confirm.message;
    let width = (msg.chars().count() as u16 + 10).clamp(24, area.width);
    let height = 6;
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .borders(Borders::ALL)
        .title(" Confirm ")
        .border_style(Style::default().fg(YELLOW));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            msg.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "y / Enter : confirm      n / Esc : cancel",
            Style::default().fg(DARK),
        )),
    ];
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{App, Confirm, ConfirmAction, View};
    use crate::db::Db;
    use crate::form::{Field, Form, FormAction};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    fn test_app() -> (App, PathBuf) {
        let n = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("toold_ui_{}_{}", std::process::id(), n));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ui.db");
        let db = Db::open(&path).unwrap();
        db.add_account("personal", "checking").unwrap();
        db.add_category("salary", "income").unwrap();
        db.add_category("dining", "expense").unwrap();
        let acc = db.account_id_by_name("personal").unwrap();
        let sal = db.category_id_by_name("salary").unwrap();
        let din = db.category_id_by_name("dining").unwrap();
        db.add_transaction(acc, sal, "income", 500_000, "2025-01-10", "", "")
            .unwrap();
        db.add_transaction(acc, din, "expense", 1_200, "2025-01-11", "ramen", "")
            .unwrap();
        db.add_budget(acc, din, "2025-01", 3_000).unwrap();
        let mut app = App::new(db, path.clone());
        app.reload();
        (app, dir)
    }

    #[test]
    fn renders_every_screen_without_panic() {
        let (mut app, dir) = test_app();
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();

        // Every normal view, including an empty one (categories emptied not needed;
        // this exercises non-empty paths).
        for view in View::ALL {
            app.set_view(view);
            terminal.draw(|f| draw(f, &app)).unwrap();
        }

        // Transaction detail.
        app.set_view(View::Transactions);
        app.selected = 0;
        app.detail = app
            .transactions
            .first()
            .and_then(|t| app.db.transaction_detail(t.id).ok());
        terminal.draw(|f| draw(f, &app)).unwrap();
        app.detail = None;

        // Form (expense).
        let mut form = Form::new("New Expense", FormAction::AddExpense);
        form.fields
            .push(Field::choice("Account", vec!["personal".into()], 0));
        form.fields
            .push(Field::choice("Category", vec!["dining".into()], 0));
        form.fields.push(Field::money("Amount (¥)", "12.34"));
        form.fields.push(Field::date("Date", "2025-01-11"));
        form.fields.push(Field::text("Payee", "ramen"));
        form.fields.push(Field::text("Notes", ""));
        app.form = Some(form);
        terminal.draw(|f| draw(f, &app)).unwrap();
        app.form = None;

        // Confirm dialog.
        app.confirm = Some(Confirm {
            message: "Delete transaction #1?".to_string(),
            action: ConfirmAction::DeleteTransaction(1),
        });
        terminal.draw(|f| draw(f, &app)).unwrap();

        // Empty-list rendering (fresh database, no data).
        let dir2 = dir.join("empty");
        std::fs::create_dir_all(&dir2).unwrap();
        let db2 = Db::open(&dir2.join("empty.db")).unwrap();
        let mut empty = App::new(db2, dir2.join("empty.db"));
        empty.reload();
        for view in View::ALL {
            empty.set_view(view);
            terminal.draw(|f| draw(f, &empty)).unwrap();
        }

        std::fs::remove_dir_all(&dir).ok();
    }
}
