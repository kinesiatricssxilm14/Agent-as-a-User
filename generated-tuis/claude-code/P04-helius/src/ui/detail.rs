//! The transaction detail pane.
//!
//! This is rendered *in place of* the list, filling the main content area
//! rather than floating as an overlay, so that every field of the transaction
//! is present in one screen snapshot with nothing hidden behind it.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::App;
use crate::db::{Kind, Transaction};
use crate::money::{format_cents, format_cents_signed};
use crate::ui::theme;

pub fn render(frame: &mut Frame, app: &App, area: Rect, id: i64) {
    let Some(t) = app.detail_transaction(id) else {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  This transaction no longer exists. Press Esc to go back.",
                    theme::muted(),
                )),
            ])
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Span::styled(" TRANSACTION DETAIL ", theme::title())),
            ),
            area,
        );
        return;
    };

    // The headline amount is given its own band; the field table and context
    // share the remainder.
    let [headline, fields, context] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Min(12),
        Constraint::Length(6),
    ])
    .areas(area);

    render_headline(frame, app, headline, &t);
    render_fields(frame, app, fields, &t);
    render_context(frame, app, context, &t);
}

/// The amount, large and signed, with its direction spelled out.
fn render_headline(frame: &mut Frame, app: &App, area: Rect, t: &Transaction) {
    let signed = match t.kind {
        Kind::Income => t.amount_cents,
        Kind::Expense => -t.amount_cents,
    };

    let lines = vec![
        Line::from(vec![
            Span::styled(
                format!("  {} {}", app.currency, format_cents(t.amount_cents)),
                theme::amount(t.kind).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("   {} ", t.kind.label().to_uppercase()),
                theme::amount(t.kind).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("(effect on account: {} {})", app.currency, format_cents_signed(signed)),
                theme::muted(),
            ),
        ]),
        Line::from(vec![
            Span::styled("  ", theme::muted()),
            Span::styled(
                theme::bar(1.0, 40.min(area.width.saturating_sub(6) as usize)),
                theme::amount(t.kind),
            ),
        ]),
        Line::from(Span::styled(
            format!(
                "  transaction #{} · recorded {} · stored as {} cents",
                t.id, t.created_at, t.amount_cents
            ),
            theme::muted(),
        )),
    ];

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(" TRANSACTION DETAIL ", theme::title())),
        ),
        area,
    );
}

/// Every stored field as a `label: value` row. Written as a paragraph rather
/// than a table so long notes can wrap and stay visible.
fn render_fields(frame: &mut Frame, app: &App, area: Rect, t: &Transaction) {
    let pad = 14;
    let dash = "—".to_string();

    let mut lines: Vec<Line> = Vec::new();
    let mut push = |label: &str, value: String, style: ratatui::style::Style| {
        lines.push(Line::from(vec![
            Span::styled(format!("  {label:<pad$}"), theme::label()),
            Span::styled(value, style),
        ]));
    };

    push(
        "Amount",
        format!("{} {}", app.currency, format_cents(t.amount_cents)),
        theme::amount(t.kind).add_modifier(Modifier::BOLD),
    );
    push("Kind", t.kind.label().to_string(), theme::amount(t.kind));
    push("Account", t.account_name.clone(), theme::value());
    push("Category", t.category_name.clone(), theme::value());
    push("Date", t.date.clone(), theme::value());
    push(
        "Payee",
        if t.payee.is_empty() { dash.clone() } else { t.payee.clone() },
        if t.payee.is_empty() { theme::muted() } else { theme::value() },
    );
    push(
        "Notes",
        if t.notes.is_empty() { dash.clone() } else { t.notes.clone() },
        if t.notes.is_empty() { theme::muted() } else { theme::value() },
    );
    push("Month", crate::date::month_of(&t.date), theme::muted());
    push("Currency", app.currency.clone(), theme::muted());
    push("Recorded at", t.created_at.clone(), theme::muted());
    push("Record ID", t.id.to_string(), theme::muted());

    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((app.scroll, 0))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(Span::styled(" Fields ", theme::title())),
            ),
        area,
    );
}

/// How this record sits within its month and its category's budget.
fn render_context(frame: &mut Frame, app: &App, area: Rect, t: &Transaction) {
    let month = crate::date::month_of(&t.date);
    let cur = &app.currency;
    let mut lines: Vec<Line> = Vec::new();

    // Re-query rather than reuse the loaded month, since the record may belong
    // to a different month than the one being browsed.
    if let Ok(summary) = app.store.monthly_summary(&month) {
        let share = if t.kind == Kind::Income {
            summary.income_cents
        } else {
            summary.expense_cents
        };
        let pct = if share > 0 {
            format!("{:.1}%", t.amount_cents as f64 / share as f64 * 100.0)
        } else {
            "—".to_string()
        };
        lines.push(Line::from(vec![
            Span::styled(format!("  {month} totals: "), theme::label()),
            Span::styled(
                format!("in {cur} {}", format_cents(summary.income_cents)),
                theme::amount(Kind::Income),
            ),
            Span::styled("   ", theme::muted()),
            Span::styled(
                format!("out {cur} {}", format_cents(summary.expense_cents)),
                theme::amount(Kind::Expense),
            ),
            Span::styled("   ", theme::muted()),
            Span::styled(
                format!("net {cur} {}", format_cents(summary.net_cents())),
                theme::signed(summary.net_cents()),
            ),
        ]));
        lines.push(Line::from(Span::styled(
            format!(
                "  This record is {pct} of the month's {} total.",
                t.kind.as_str()
            ),
            theme::muted(),
        )));
    }

    // Budget position for this record's category, if one is set.
    if let (Some(cat_id), Ok(budgets)) = (t.category_id, app.store.budgets(&month)) {
        if let Some(b) = budgets.iter().find(|b| {
            b.category_id == cat_id && (b.account_id.is_none() || b.account_id == Some(t.account_id))
        }) {
            let remaining = b.remaining_cents();
            lines.push(Line::from(vec![
                Span::styled(format!("  Budget '{}': ", b.category_name), theme::label()),
                Span::styled(
                    format!("{cur} {}", format_cents(b.amount_cents)),
                    theme::value(),
                ),
                Span::styled(
                    format!("   spent {cur} {}", format_cents(b.spent_cents)),
                    theme::budget_state(b.spent_cents, b.amount_cents),
                ),
                Span::styled(
                    format!("   remaining {cur} {}", format_cents(remaining)),
                    theme::signed(remaining),
                ),
            ]));
        } else {
            lines.push(Line::from(Span::styled(
                format!("  No {month} budget covers this category — press b to set one."),
                theme::muted(),
            )));
        }
    }

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(" In context ", theme::title())),
        ),
        area,
    );
}
