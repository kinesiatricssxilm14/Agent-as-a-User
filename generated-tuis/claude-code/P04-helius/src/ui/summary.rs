//! The SUMMARY view: monthly income, expense and net income, plus the
//! per-category breakdown and budget roll-up.
//!
//! Everything is laid out in one scrollable pane so that income, expense and
//! net are visible in a single screen snapshot, as the spec requires.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::App;
use crate::date;
use crate::db::Kind;
use crate::money::{format_cents, format_cents_signed};
use crate::ui::theme;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    // Headline totals get a fixed height; the breakdown takes the rest.
    let [totals_area, lower] =
        Layout::vertical([Constraint::Length(11), Constraint::Min(6)]).areas(area);

    render_totals(frame, app, totals_area);

    let [categories_area, budgets_area] =
        Layout::horizontal([Constraint::Percentage(55), Constraint::Percentage(45)])
            .areas(lower);

    render_category_breakdown(frame, app, categories_area);
    render_budget_rollup(frame, app, budgets_area);
}

/// Income / expense / net, each on its own line and labelled, so a screen
/// snapshot of this view always contains all three figures.
fn render_totals(frame: &mut Frame, app: &App, area: Rect) {
    let s = &app.data.summary;
    let cur = &app.currency;
    let net = s.net_cents();

    // Width of the widest label, so the amounts line up in a column.
    let pad = 16;
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(vec![
        Span::styled(
            format!("{:<pad$}", "Month"),
            theme::label(),
        ),
        Span::styled(
            format!("{}  ({})", date::month_label(&s.month), s.month),
            theme::title(),
        ),
    ]));
    lines.push(Line::from(""));

    lines.push(amount_line(
        "Total income",
        &format_cents(s.income_cents),
        cur,
        theme::amount(Kind::Income),
        pad,
        &format!("{} record(s)", s.income_count),
    ));
    lines.push(amount_line(
        "Total expense",
        &format_cents(s.expense_cents),
        cur,
        theme::amount(Kind::Expense),
        pad,
        &format!("{} record(s)", s.expense_count),
    ));

    // A rule between the components and the total, as on a paper statement.
    lines.push(Line::from(Span::styled(
        format!("{:<pad$}{}", "", "─".repeat(28)),
        theme::muted(),
    )));

    lines.push(amount_line(
        "Net income",
        &format_cents(net),
        cur,
        theme::signed(net).add_modifier(Modifier::BOLD),
        pad,
        &match s.savings_rate() {
            Some(rate) => format!("{:.1}% of income kept", rate * 100.0),
            None => "income - expense".to_string(),
        },
    ));

    lines.push(Line::from(""));

    // Budget roll-up for the same month, so the overview answers "am I within
    // budget?" without switching views.
    if s.budget_count > 0 {
        let remaining = s.budget_remaining_cents();
        lines.push(Line::from(vec![
            Span::styled(format!("{:<pad$}", "Budgeted"), theme::label()),
            Span::styled(
                format!("{cur} {:>12}", format_cents(s.budget_total_cents)),
                theme::value(),
            ),
            Span::styled(
                format!("   spent {cur} {}", format_cents(s.budget_spent_cents)),
                theme::muted(),
            ),
            Span::styled(
                format!("   remaining {cur} {}", format_cents(remaining)),
                theme::signed(remaining),
            ),
            Span::styled(
                format!("   ({} budget(s))", s.budget_count),
                theme::muted(),
            ),
        ]));
    } else {
        lines.push(Line::from(Span::styled(
            format!("{:<pad$}no budgets set for this month — press b to set one", ""),
            theme::muted(),
        )));
    }

    if let Some((payee, cents)) = &s.largest_expense {
        lines.push(Line::from(vec![
            Span::styled(format!("{:<pad$}", "Largest expense"), theme::label()),
            Span::styled(format!("{cur} {:>12}", format_cents(*cents)), theme::value()),
            Span::styled(format!("   {}", theme::fit(payee, 30)), theme::muted()),
        ]));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" Monthly summary ", theme::title()));

    frame.render_widget(
        Paragraph::new(lines).block(block).scroll((app.scroll, 0)),
        area,
    );
}

/// One `label   CNY  amount   note` row.
fn amount_line<'a>(
    label: &'a str,
    amount: &str,
    currency: &str,
    style: ratatui::style::Style,
    pad: usize,
    note: &str,
) -> Line<'a> {
    Line::from(vec![
        Span::styled(format!("{label:<pad$}"), theme::label()),
        Span::styled(format!("{currency} {amount:>12}"), style),
        Span::styled(format!("   {note}"), theme::muted()),
    ])
}

/// Per-category totals for the month, expenses and income together.
fn render_category_breakdown(frame: &mut Frame, app: &App, area: Rect) {
    let totals = &app.data.category_totals;
    let cur = &app.currency;

    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        format!(" Category breakdown — {} ", app.month),
        theme::title(),
    ));

    if totals.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  No transactions in this month.",
                    theme::muted(),
                )),
                Line::from(Span::styled(
                    "  Press i to record income, e to record an expense.",
                    theme::muted(),
                )),
            ])
            .block(block),
            area,
        );
        return;
    }

    // Scale the inline bars against the largest total in each direction.
    let max_expense = totals
        .iter()
        .filter(|t| t.kind == Kind::Expense)
        .map(|t| t.amount_cents)
        .max()
        .unwrap_or(0);
    let max_income = totals
        .iter()
        .filter(|t| t.kind == Kind::Income)
        .map(|t| t.amount_cents)
        .max()
        .unwrap_or(0);

    let rows: Vec<Row> = totals
        .iter()
        .map(|t| {
            let max = if t.kind == Kind::Income { max_income } else { max_expense };
            let ratio = if max > 0 {
                t.amount_cents as f64 / max as f64
            } else {
                0.0
            };
            Row::new(vec![
                Cell::from(Span::styled(
                    match t.kind {
                        Kind::Income => "income ",
                        Kind::Expense => "expense",
                    },
                    theme::amount(t.kind),
                )),
                Cell::from(Span::styled(theme::fit(&t.name, 18), theme::value())),
                Cell::from(Span::styled(
                    format!("{cur} {:>10}", format_cents(t.amount_cents)),
                    theme::amount(t.kind),
                )),
                Cell::from(Span::styled(format!("{:>3}", t.count), theme::muted())),
                Cell::from(Span::styled(theme::bar(ratio, 10), theme::amount(t.kind))),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(7),
            Constraint::Length(18),
            Constraint::Length(18),
            Constraint::Length(4),
            Constraint::Length(10),
        ],
    )
    .header(
        Row::new(vec!["KIND", "CATEGORY", "TOTAL", "N", "SHARE"]).style(theme::table_header()),
    )
    .column_spacing(1)
    .block(block);

    frame.render_widget(table, area);
}

/// Compact budget status for the month, mirroring the BUDGETS view.
fn render_budget_rollup(frame: &mut Frame, app: &App, area: Rect) {
    let budgets = &app.data.budgets;
    let cur = &app.currency;

    let block = Block::default().borders(Borders::ALL).title(Span::styled(
        format!(" Budget status — {} ", app.month),
        theme::title(),
    ));

    if budgets.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled("  No budgets for this month.", theme::muted())),
                Line::from(Span::styled(
                    "  Press b to set a monthly budget.",
                    theme::muted(),
                )),
            ])
            .block(block),
            area,
        );
        return;
    }

    let rows: Vec<Row> = budgets
        .iter()
        .map(|b| {
            let remaining = b.remaining_cents();
            Row::new(vec![
                Cell::from(Span::styled(theme::fit(&b.category_name, 14), theme::value())),
                Cell::from(Span::styled(
                    theme::bar(b.used_ratio(), 8),
                    theme::budget_state(b.spent_cents, b.amount_cents),
                )),
                Cell::from(Span::styled(
                    format!("{:>10}", format_cents(b.spent_cents)),
                    theme::budget_state(b.spent_cents, b.amount_cents),
                )),
                Cell::from(Span::styled(format!("{:>10}", format_cents(b.amount_cents)), theme::value())),
                Cell::from(Span::styled(format!("{:>10}", format_cents(remaining)), theme::signed(remaining))),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(14),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(10),
        ],
    )
    .header(
        Row::new(vec![
            "CATEGORY".to_string(),
            "USED".to_string(),
            format!("SPENT/{cur}"),
            "BUDGET".to_string(),
            "LEFT".to_string(),
        ])
        .style(theme::table_header()),
    )
    .column_spacing(1)
    .block(block);

    frame.render_widget(table, area);
}

/// Suffix showing the net figure, used in the window title bar.
pub fn net_badge(app: &App) -> String {
    format!(
        "net {} {}",
        app.currency,
        format_cents_signed(app.data.summary.net_cents())
    )
}
