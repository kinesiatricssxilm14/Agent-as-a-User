//! The tabular views: TRANSACTIONS, ACCOUNTS, CATEGORIES and BUDGETS.
//!
//! Each renders a header row plus one row per record, with a footer line of
//! column totals where a total is meaningful. Selection state lives in
//! [`crate::app::App`] and is copied into a fresh `TableState` each frame.

use ratatui::layout::{Constraint, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;

use crate::app::App;
use crate::db::Kind;
use crate::money::format_cents;
use crate::ui::theme;

/// Render a table with the app's selection applied, plus a scrollbar-ish
/// position indicator in the block title.
fn render_table(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    title: String,
    header: Vec<String>,
    widths: Vec<Constraint>,
    rows: Vec<Row<'_>>,
    footer: Option<Row<'_>>,
) {
    let count = rows.len();
    let position = if count == 0 {
        " (empty) ".to_string()
    } else {
        format!(" {}/{} ", app.selected() + 1, count)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(title, theme::title()))
        .title_bottom(Span::styled(position, theme::muted()));

    let mut table = Table::new(rows, widths)
        .header(Row::new(header).style(theme::table_header()))
        .column_spacing(1)
        .row_highlight_style(theme::selected_row())
        .highlight_symbol("▌")
        .block(block);

    if let Some(footer) = footer {
        table = table.footer(footer);
    }

    let mut state = TableState::default();
    if count > 0 {
        state.select(Some(app.selected().min(count - 1)));
    }
    frame.render_stateful_widget(table, area, &mut state);
}

/// An empty-state panel that tells the user which key creates the first record.
fn render_empty(frame: &mut Frame, area: Rect, title: String, lines: &[&str]) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(title, theme::title()));
    let mut text = vec![Line::from("")];
    for line in lines {
        text.push(Line::from(Span::styled(format!("  {line}"), theme::muted())));
    }
    frame.render_widget(Paragraph::new(text).block(block), area);
}

// ---------------------------------------------------------------------------
// Transactions
// ---------------------------------------------------------------------------

pub fn render_transactions(frame: &mut Frame, app: &App, area: Rect) {
    let txns = app.visible_transactions();
    let cur = &app.currency;

    // The title states every active filter, so the screen is self-describing.
    let mut title = format!(" TRANSACTIONS — {} record(s)", txns.len());
    if app.filter_by_month {
        title.push_str(&format!(" · month {}", app.month));
    } else {
        title.push_str(" · all months");
    }
    if let Some(q) = &app.search {
        title.push_str(&format!(" · search '{q}'"));
    }
    title.push(' ');

    if txns.is_empty() {
        let hint: Vec<&str> = if app.search.is_some() {
            vec![
                "No transaction matches the current search.",
                "Press Esc or Ctrl+L to clear the filter, / to search again.",
            ]
        } else if app.filter_by_month {
            vec![
                "No transactions in this month.",
                "Press i for income, e for an expense, m to show all months.",
            ]
        } else {
            vec![
                "No transactions recorded yet.",
                "Press i to record income, e to record an expense.",
            ]
        };
        render_empty(frame, area, title, &hint);
        return;
    }

    let rows: Vec<Row> = txns
        .iter()
        .map(|t| {
            let sign = match t.kind {
                Kind::Income => '+',
                Kind::Expense => '-',
            };
            Row::new(vec![
                Cell::from(Span::styled(t.date.clone(), theme::value())),
                Cell::from(Span::styled(
                    match t.kind {
                        Kind::Income => "income",
                        Kind::Expense => "expense",
                    },
                    theme::amount(t.kind),
                )),
                Cell::from(Span::styled(
                    format!("{sign}{:>12}", format_cents(t.amount_cents)),
                    theme::amount(t.kind),
                )),
                Cell::from(Span::styled(theme::fit(&t.account_name, 14), theme::value())),
                Cell::from(Span::styled(theme::fit(&t.category_name, 16), theme::value())),
                Cell::from(Span::styled(
                    theme::fit(if t.payee.is_empty() { "—" } else { &t.payee }, 18),
                    theme::value(),
                )),
                Cell::from(Span::styled(
                    theme::fit(if t.notes.is_empty() { "—" } else { &t.notes }, 24),
                    theme::muted(),
                )),
            ])
        })
        .collect();

    // Totals over exactly the rows on screen, so a filtered list still adds up.
    let income: i64 = txns
        .iter()
        .filter(|t| t.kind == Kind::Income)
        .map(|t| t.amount_cents)
        .sum();
    let expense: i64 = txns
        .iter()
        .filter(|t| t.kind == Kind::Expense)
        .map(|t| t.amount_cents)
        .sum();
    let net = income - expense;

    let footer = Row::new(vec![
        Cell::from(Span::styled("TOTAL", theme::strong())),
        Cell::from(Span::styled(format!("{} shown", txns.len()), theme::muted())),
        Cell::from(Span::styled(format!("{:>13}", format_cents(net)), theme::signed(net))),
        Cell::from(Span::styled(
            format!("in {}", format_cents(income)),
            theme::amount(Kind::Income),
        )),
        Cell::from(Span::styled(
            format!("out {}", format_cents(expense)),
            theme::amount(Kind::Expense),
        )),
        Cell::from(Span::styled("Enter = full detail", theme::muted())),
        Cell::from(""),
    ])
    .style(theme::strong());

    render_table(
        frame,
        app,
        area,
        title,
        vec![
            "DATE".to_string(),
            "KIND".to_string(),
            format!("AMOUNT/{cur}"),
            "ACCOUNT".to_string(),
            "CATEGORY".to_string(),
            "PAYEE".to_string(),
            "NOTES".to_string(),
        ],
        vec![
            Constraint::Length(10),
            Constraint::Length(7),
            Constraint::Length(13),
            Constraint::Length(14),
            Constraint::Length(16),
            Constraint::Length(18),
            Constraint::Min(10),
        ],
        rows,
        Some(footer),
    );
}

// ---------------------------------------------------------------------------
// Accounts
// ---------------------------------------------------------------------------

pub fn render_accounts(frame: &mut Frame, app: &App, area: Rect) {
    let accounts = &app.data.accounts;
    let cur = &app.currency;
    let title = format!(" ACCOUNTS — {} account(s) ", accounts.len());

    if accounts.is_empty() {
        render_empty(
            frame,
            area,
            title,
            &[
                "No accounts yet. An account is where money moves in and out of.",
                "Press n to create one, e.g. 'personal' of type 'checking'.",
            ],
        );
        return;
    }

    let rows: Vec<Row> = accounts
        .iter()
        .map(|a| {
            Row::new(vec![
                Cell::from(Span::styled(theme::fit(&a.name, 20), theme::value())),
                Cell::from(Span::styled(theme::fit(&a.account_type, 12), theme::value())),
                Cell::from(Span::styled(a.currency.clone(), theme::muted())),
                Cell::from(Span::styled(
                    format!("{:>14}", format_cents(a.balance_cents)),
                    theme::signed(a.balance_cents),
                )),
                Cell::from(Span::styled(format!("{:>6}", a.txn_count), theme::muted())),
                Cell::from(Span::styled(
                    theme::fit(if a.notes.is_empty() { "—" } else { &a.notes }, 28),
                    theme::muted(),
                )),
            ])
        })
        .collect();

    let total: i64 = accounts.iter().map(|a| a.balance_cents).sum();
    let txns: i64 = accounts.iter().map(|a| a.txn_count).sum();
    let footer = Row::new(vec![
        Cell::from(Span::styled("TOTAL", theme::strong())),
        Cell::from(""),
        Cell::from(""),
        Cell::from(Span::styled(
            format!("{:>14}", format_cents(total)),
            theme::signed(total),
        )),
        Cell::from(Span::styled(format!("{txns:>6}"), theme::muted())),
        Cell::from(""),
    ])
    .style(theme::strong());

    render_table(
        frame,
        app,
        area,
        title,
        vec![
            "NAME".to_string(),
            "TYPE".to_string(),
            "CCY".to_string(),
            format!("BALANCE/{cur}"),
            "TXNS".to_string(),
            "NOTES".to_string(),
        ],
        vec![
            Constraint::Length(20),
            Constraint::Length(12),
            Constraint::Length(4),
            Constraint::Length(14),
            Constraint::Length(6),
            Constraint::Min(10),
        ],
        rows,
        Some(footer),
    );
}

// ---------------------------------------------------------------------------
// Categories
// ---------------------------------------------------------------------------

pub fn render_categories(frame: &mut Frame, app: &App, area: Rect) {
    let categories = &app.data.categories;
    let cur = &app.currency;
    let title = format!(" CATEGORIES — {} category(ies) ", categories.len());

    if categories.is_empty() {
        render_empty(
            frame,
            area,
            title,
            &[
                "No categories yet. Categories group transactions for reporting.",
                "Press n to create one, e.g. 'dining' (expense) or 'salary' (income).",
            ],
        );
        return;
    }

    // Budgeted amount for the shown month, so this view answers "is this
    // category budgeted?" without a switch to BUDGETS.
    let rows: Vec<Row> = categories
        .iter()
        .map(|c| {
            let budget: i64 = app
                .data
                .budgets
                .iter()
                .filter(|b| b.category_id == c.id)
                .map(|b| b.amount_cents)
                .sum();
            Row::new(vec![
                Cell::from(Span::styled(theme::fit(&c.name, 22), theme::value())),
                Cell::from(Span::styled(c.kind.as_str(), theme::amount(c.kind))),
                Cell::from(Span::styled(
                    format!("{:>14}", format_cents(c.total_cents)),
                    theme::amount(c.kind),
                )),
                Cell::from(Span::styled(format!("{:>6}", c.txn_count), theme::muted())),
                Cell::from(if budget > 0 {
                    Span::styled(format!("{:>12}", format_cents(budget)), theme::value())
                } else {
                    Span::styled(format!("{:>12}", "—"), theme::muted())
                }),
                Cell::from(Span::styled(
                    theme::fit(if c.notes.is_empty() { "—" } else { &c.notes }, 24),
                    theme::muted(),
                )),
            ])
        })
        .collect();

    let income: i64 = categories
        .iter()
        .filter(|c| c.kind == Kind::Income)
        .map(|c| c.total_cents)
        .sum();
    let expense: i64 = categories
        .iter()
        .filter(|c| c.kind == Kind::Expense)
        .map(|c| c.total_cents)
        .sum();

    let footer = Row::new(vec![
        Cell::from(Span::styled("TOTAL", theme::strong())),
        Cell::from(Span::styled(
            format!("{} in / {} out", income / 100, expense / 100),
            theme::muted(),
        )),
        Cell::from(Span::styled(
            format!("{:>14}", format_cents(income + expense)),
            theme::value(),
        )),
        Cell::from(""),
        Cell::from(Span::styled("b = set budget", theme::muted())),
        Cell::from(""),
    ])
    .style(theme::strong());

    render_table(
        frame,
        app,
        area,
        title,
        vec![
            "NAME".to_string(),
            "KIND".to_string(),
            format!("ALL-TIME/{cur}"),
            "TXNS".to_string(),
            format!("BUDGET {}", app.month),
            "NOTES".to_string(),
        ],
        vec![
            Constraint::Length(22),
            Constraint::Length(7),
            Constraint::Length(14),
            Constraint::Length(6),
            Constraint::Length(18),
            Constraint::Min(10),
        ],
        rows,
        Some(footer),
    );
}

// ---------------------------------------------------------------------------
// Budgets
// ---------------------------------------------------------------------------

pub fn render_budgets(frame: &mut Frame, app: &App, area: Rect) {
    let budgets = &app.data.budgets;
    let cur = &app.currency;
    let title = format!(
        " BUDGETS — {} · {} budget(s) ",
        crate::date::month_label(&app.month),
        budgets.len()
    );

    if budgets.is_empty() {
        render_empty(
            frame,
            area,
            title,
            &[
                &format!("No budgets set for {}.", app.month),
                "Press n to set a monthly budget for a category.",
                "Use ←/→ to look at another month, T to jump back to this one.",
            ],
        );
        return;
    }

    // BUDGET, SPENT and REMAINING all appear on each row, per the spec.
    let rows: Vec<Row> = budgets
        .iter()
        .map(|b| {
            let remaining = b.remaining_cents();
            let state = theme::budget_state(b.spent_cents, b.amount_cents);
            Row::new(vec![
                Cell::from(Span::styled(theme::fit(&b.category_name, 18), theme::value())),
                Cell::from(Span::styled(b.category_kind.as_str(), theme::amount(b.category_kind))),
                Cell::from(Span::styled(theme::fit(&b.account_name, 14), theme::muted())),
                Cell::from(Span::styled(
                    format!("{:>12}", format_cents(b.amount_cents)),
                    theme::value(),
                )),
                Cell::from(Span::styled(format!("{:>12}", format_cents(b.spent_cents)), state)),
                Cell::from(Span::styled(
                    format!("{:>12}", format_cents(remaining)),
                    theme::signed(remaining),
                )),
                Cell::from(Span::styled(
                    format!("{} {:>4.0}%", theme::bar(b.used_ratio(), 10), b.used_ratio() * 100.0),
                    state,
                )),
                Cell::from(if b.is_over() {
                    Span::styled("OVER", theme::amount(Kind::Expense).add_modifier(
                        ratatui::style::Modifier::BOLD,
                    ))
                } else {
                    Span::styled("ok", theme::muted())
                }),
            ])
        })
        .collect();

    let total: i64 = budgets.iter().map(|b| b.amount_cents).sum();
    let spent: i64 = budgets.iter().map(|b| b.spent_cents).sum();
    let remaining = total - spent;

    let footer = Row::new(vec![
        Cell::from(Span::styled("TOTAL", theme::strong())),
        Cell::from(""),
        Cell::from(""),
        Cell::from(Span::styled(format!("{:>12}", format_cents(total)), theme::value())),
        Cell::from(Span::styled(
            format!("{:>12}", format_cents(spent)),
            theme::budget_state(spent, total),
        )),
        Cell::from(Span::styled(
            format!("{:>12}", format_cents(remaining)),
            theme::signed(remaining),
        )),
        Cell::from(""),
        Cell::from(""),
    ])
    .style(theme::strong());

    render_table(
        frame,
        app,
        area,
        title,
        vec![
            "CATEGORY".to_string(),
            "KIND".to_string(),
            "ACCOUNT".to_string(),
            format!("BUDGET/{cur}"),
            format!("SPENT/{cur}"),
            format!("REMAINING/{cur}"),
            "USED".to_string(),
            "".to_string(),
        ],
        vec![
            Constraint::Length(18),
            Constraint::Length(7),
            Constraint::Length(14),
            Constraint::Length(12),
            Constraint::Length(12),
            Constraint::Length(14),
            Constraint::Length(16),
            Constraint::Length(5),
        ],
        rows,
        Some(footer),
    );
}
