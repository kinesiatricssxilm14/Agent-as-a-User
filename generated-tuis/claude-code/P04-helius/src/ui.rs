//! Screen assembly: the tab bar and context line at the top, the active view
//! or modal in the middle, and the message/hint bar at the bottom.

pub mod detail;
pub mod lists;
pub mod panes;
pub mod summary;
pub mod theme;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Tabs};
use ratatui::Frame;

use crate::app::{App, MessageKind, Modal, View};
use crate::keys;
use crate::money::format_cents;

/// Draw one frame.
pub fn draw(frame: &mut Frame, app: &App) {
    let [tabs_area, context_area, body_area, status_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(8),
        Constraint::Length(2),
    ])
    .areas(frame.area());

    render_tabs(frame, app, tabs_area);
    render_context(frame, app, context_area);
    render_body(frame, app, body_area);
    render_status(frame, app, status_area);
}

/// The view tabs, each labelled with the digit that jumps to it.
fn render_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let titles: Vec<Line> = View::ALL
        .iter()
        .map(|v| {
            Line::from(vec![
                Span::styled(format!("{} ", v.digit()), Style::default().fg(theme::MUTED)),
                Span::raw(v.title()),
            ])
        })
        .collect();

    let tabs = Tabs::new(titles)
        .select(app.view.index())
        .divider(Span::styled("│", Style::default().fg(theme::MUTED)))
        .style(Style::default().fg(ratatui::style::Color::Gray))
        .highlight_style(
            Style::default()
                .fg(ratatui::style::Color::Black)
                .bg(theme::ACCENT)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_widget(tabs, area);
}

/// A single line of always-visible context: app name, month, key totals and
/// where the data is stored.
fn render_context(frame: &mut Frame, app: &App, area: Rect) {
    let s = &app.data.summary;
    let net = s.net_cents();
    let cur = &app.currency;

    let mut spans = vec![
        Span::styled(" toold ", Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD)),
        Span::styled("│ ", Style::default().fg(theme::MUTED)),
        Span::styled(
            format!("{} ", crate::date::month_label(&app.month)),
            Style::default().add_modifier(Modifier::BOLD),
        ),
        Span::styled("│ ", Style::default().fg(theme::MUTED)),
        Span::styled(
            format!("income {cur} {}", format_cents(s.income_cents)),
            theme::amount(crate::db::Kind::Income),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            format!("expense {cur} {}", format_cents(s.expense_cents)),
            theme::amount(crate::db::Kind::Expense),
        ),
        Span::styled("  ", Style::default()),
        Span::styled(format!("net {cur} {}", format_cents(net)), theme::signed(net)),
        Span::styled(" │ ", Style::default().fg(theme::MUTED)),
    ];

    // Active filters are surfaced here so a filtered list is never mistaken for
    // an empty ledger.
    if app.filter_by_month {
        spans.push(Span::styled("[month] ", Style::default().fg(theme::WARN)));
    }
    if let Some(q) = &app.search {
        spans.push(Span::styled(
            format!("[/{}] ", theme::fit(q, 16)),
            Style::default().fg(theme::WARN),
        ));
    }

    spans.push(Span::styled(
        format!("{} accounts · {} categories", app.data.accounts.len(), app.data.categories.len()),
        theme::muted(),
    ));

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The main content area: either the active view, or the modal that replaced it.
fn render_body(frame: &mut Frame, app: &App, area: Rect) {
    match &app.modal {
        Modal::Help => panes::render_help(frame, app, area),
        Modal::Confirm(c) => panes::render_confirm(frame, area, c),
        Modal::Detail { id } => detail::render(frame, app, area, *id),
        Modal::Form { target, form } => panes::render_form(frame, app, area, *target, form),
        Modal::Browse => match app.view {
            View::Summary => summary::render(frame, app, area),
            View::Transactions => lists::render_transactions(frame, app, area),
            View::Accounts => lists::render_accounts(frame, app, area),
            View::Categories => lists::render_categories(frame, app, area),
            View::Budgets => lists::render_budgets(frame, app, area),
        },
    }
}

/// Two lines at the bottom: the last message (or the search prompt), and the
/// key hints for whatever has focus.
fn render_status(frame: &mut Frame, app: &App, area: Rect) {
    let [message_area, hint_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);

    // The search prompt occupies the message line while it is open.
    if let Some(input) = &app.search_input {
        let prompt = "  Search: ";
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(prompt, Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD)),
                Span::styled(input.value().to_string(), theme::value()),
            ])),
            message_area,
        );
        // Put the caret where the user is typing.
        let x = message_area.x + prompt.chars().count() as u16 + input.cursor_display_width();
        if x < message_area.x + message_area.width {
            frame.set_cursor_position((x, message_area.y));
        }
    } else {
        let line = match &app.message {
            Some((text, kind)) => {
                let (marker, style) = match kind {
                    MessageKind::Success => ("✓", Style::default().fg(theme::INCOME)),
                    MessageKind::Error => ("✗", Style::default().fg(theme::EXPENSE)),
                    MessageKind::Info => ("·", Style::default().fg(ratatui::style::Color::Gray)),
                };
                Line::from(vec![
                    Span::styled(format!("  {marker} "), style.add_modifier(Modifier::BOLD)),
                    Span::styled(text.clone(), style),
                ])
            }
            None => Line::from(Span::styled(
                format!("  {} · {}", app.store.path().display(), keys::footer_hint(app.view)),
                theme::muted(),
            )),
        };
        frame.render_widget(Paragraph::new(line), message_area);
    }

    // The hint line reflects the focused layer, not just the view.
    let hint = if app.search_input.is_some() {
        keys::modal_hint(keys::ModalHint::Search)
    } else {
        match &app.modal {
            Modal::Form { .. } => keys::modal_hint(keys::ModalHint::Form),
            Modal::Detail { .. } => keys::modal_hint(keys::ModalHint::Detail),
            Modal::Confirm(_) => keys::modal_hint(keys::ModalHint::Confirm),
            Modal::Help => keys::modal_hint(keys::ModalHint::Help),
            Modal::Browse => keys::footer_hint(app.view),
        }
    };

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {hint}"),
            Style::default().fg(theme::ACCENT),
        ))),
        hint_area,
    );
}
