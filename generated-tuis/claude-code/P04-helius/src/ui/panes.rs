//! Form, confirmation and help panes.
//!
//! Like the detail pane these replace the list in the main content area rather
//! than floating over it, so the form and its hints are fully visible.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, Confirm, FormTarget};
use crate::form::{FieldKind, Form};
use crate::keys;
use crate::ui::theme;

// ---------------------------------------------------------------------------
// Forms
// ---------------------------------------------------------------------------

pub fn render_form(frame: &mut Frame, app: &App, area: Rect, target: FormTarget, form: &Form) {
    let [fields_area, hint_area] =
        Layout::vertical([Constraint::Min(9), Constraint::Length(7)]).areas(area);

    render_fields(frame, app, fields_area, target, form);
    render_form_hints(frame, app, hint_area, form);
}

fn render_fields(
    frame: &mut Frame,
    app: &App,
    area: Rect,
    target: FormTarget,
    form: &Form,
) {
    let label_width = form
        .fields
        .iter()
        .map(|f| f.label.chars().count())
        .max()
        .unwrap_or(8)
        .max(8);

    let mut lines: Vec<Line> = vec![Line::from("")];
    // Row of the focused field within the paragraph, used to place the caret.
    let mut caret: Option<(usize, u16)> = None;

    for (idx, field) in form.fields.iter().enumerate() {
        let focused = idx == form.focus;
        let marker = if focused { "▶ " } else { "  " };
        let label_style = if focused {
            Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD)
        } else {
            theme::label()
        };

        // A choice field shows ‹ value › to advertise the ←/→ keys.
        let value_text = if field.is_choice() {
            let pos = if field.options.is_empty() {
                String::new()
            } else {
                format!("  [{}/{}]", field.selected + 1, field.options.len())
            };
            format!("‹ {} ›{}", field.display_value(), pos)
        } else {
            field.display_value()
        };

        let value_style = if field.is_choice() {
            Style::default().fg(theme::ACCENT)
        } else if focused {
            Style::default()
                .fg(ratatui::style::Color::White)
                .add_modifier(Modifier::UNDERLINED)
        } else {
            theme::value()
        };

        // Required fields are flagged so the user knows before submitting.
        let required = matches!(
            field.kind,
            FieldKind::Text { required: true }
                | FieldKind::Amount
                | FieldKind::Choice { required: true }
        );
        let flag = if required { "*" } else { " " };

        lines.push(Line::from(vec![
            Span::styled(marker, Style::default().fg(theme::ACCENT)),
            Span::styled(format!("{:<label_width$}{flag} ", field.label), label_style),
            Span::styled(value_text, value_style),
        ]));

        if focused && !field.is_choice() {
            // 2 (marker) + label + flag + space, then the cursor offset.
            let prefix = 2 + label_width + 2;
            caret = Some((lines.len() - 1, prefix as u16 + field.input.cursor_display_width()));
        }

        // Live feedback under an amount field: echo the parsed value so the
        // two-decimal result is visible before saving.
        if field.kind == FieldKind::Amount && !field.input.is_empty() {
            let echo = match crate::money::parse_positive_cents(field.input.value()) {
                Ok(cents) => Span::styled(
                    format!("{:>indent$}= {} {}", "", app.currency, crate::money::format_cents(cents), indent = 2 + label_width + 2),
                    theme::muted(),
                ),
                Err(e) => Span::styled(
                    format!("{:>indent$}! {e}", "", indent = 2 + label_width + 2),
                    Style::default().fg(theme::EXPENSE),
                ),
            };
            lines.push(Line::from(echo));
        }

        // Same for a date field, showing the canonical form it will be saved as.
        if field.kind == FieldKind::Date {
            let echo = match crate::date::parse_date(field.input.value()) {
                Ok(d) => Span::styled(
                    format!(
                        "{:>indent$}= {d} ({})",
                        "",
                        crate::date::month_label(&crate::date::month_of(&d)),
                        indent = 2 + label_width + 2
                    ),
                    theme::muted(),
                ),
                Err(e) => Span::styled(
                    format!("{:>indent$}! {e}", "", indent = 2 + label_width + 2),
                    Style::default().fg(theme::EXPENSE),
                ),
            };
            lines.push(Line::from(echo));
        }

        if field.kind == FieldKind::Month {
            let echo = match crate::date::parse_month(field.input.value()) {
                Ok(m) => Span::styled(
                    format!(
                        "{:>indent$}= {m} ({})",
                        "",
                        crate::date::month_label(&m),
                        indent = 2 + label_width + 2
                    ),
                    theme::muted(),
                ),
                Err(e) => Span::styled(
                    format!("{:>indent$}! {e}", "", indent = 2 + label_width + 2),
                    Style::default().fg(theme::EXPENSE),
                ),
            };
            lines.push(Line::from(echo));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  * required · values are saved to the SQLite ledger on Enter",
        theme::muted(),
    )));

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(format!(" {} ", target.title()), theme::title()))
        .title_bottom(Span::styled(
            format!(" field {}/{} ", form.focus + 1, form.fields.len()),
            theme::muted(),
        ));

    frame.render_widget(Paragraph::new(lines).block(block), area);

    // Place the real terminal cursor in the focused text field so typing feels
    // like typing, not like driving a state machine.
    if let Some((row, col)) = caret {
        let x = area.x + 1 + col;
        let y = area.y + 1 + row as u16;
        if x < area.x + area.width.saturating_sub(1) && y < area.y + area.height.saturating_sub(1) {
            frame.set_cursor_position((x, y));
        }
    }
}

/// The hint pane under a form: what the focused field expects, plus the keys.
fn render_form_hints(frame: &mut Frame, app: &App, area: Rect, form: &Form) {
    let field = form.focused();
    let mut lines = vec![Line::from(vec![
        Span::styled("  This field: ", theme::label()),
        Span::styled(field.label.clone(), theme::title()),
    ])];

    if !field.hint.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", field.hint),
            theme::value(),
        )));
    }

    // Field-kind specific guidance, so the accepted syntax is discoverable.
    let extra = match field.kind {
        FieldKind::Amount => Some(format!(
            "  Enter yuan with up to two decimals; stored as cents. Example: 1234.56 ({} 1234.56)",
            app.currency
        )),
        FieldKind::Date => Some(
            "  Accepts 2026-08-13, 2026/8/3, today, yesterday. Empty = today. PgUp/PgDn steps a day."
                .to_string(),
        ),
        FieldKind::Month => Some(
            "  Accepts 2026-08 or 2026/8. Empty = the current month. PgUp/PgDn steps a month."
                .to_string(),
        ),
        FieldKind::Choice { .. } => Some(
            "  Press ←/→ to change, or type the first letter to jump to a matching option."
                .to_string(),
        ),
        FieldKind::Text { required } => Some(if required {
            "  Free text; this field cannot be left empty.".to_string()
        } else {
            "  Free text; may be left empty.".to_string()
        }),
    };
    if let Some(extra) = extra {
        lines.push(Line::from(Span::styled(extra, theme::muted())));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        format!("  {}", keys::modal_hint(keys::ModalHint::Form)),
        Style::default().fg(theme::ACCENT),
    )));

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .title(Span::styled(" Guidance ", theme::title())),
        ),
        area,
    );
}

// ---------------------------------------------------------------------------
// Confirmation
// ---------------------------------------------------------------------------

pub fn render_confirm(frame: &mut Frame, area: Rect, confirm: &Confirm) {
    let lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("  {}", confirm.prompt),
            Style::default()
                .fg(theme::WARN)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(Span::styled(format!("  {}", confirm.detail), theme::value())),
        Line::from(""),
        Line::from(Span::styled(
            "  This writes to the SQLite ledger immediately and cannot be undone.",
            theme::muted(),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("  y", Style::default().fg(theme::EXPENSE).add_modifier(Modifier::BOLD)),
            Span::styled(" or ", theme::muted()),
            Span::styled("Enter", Style::default().fg(theme::EXPENSE).add_modifier(Modifier::BOLD)),
            Span::styled("  delete it", theme::value()),
        ]),
        Line::from(vec![
            Span::styled("  n", Style::default().fg(theme::INCOME).add_modifier(Modifier::BOLD)),
            Span::styled(" or ", theme::muted()),
            Span::styled("Esc", Style::default().fg(theme::INCOME).add_modifier(Modifier::BOLD)),
            Span::styled("    keep it", theme::value()),
        ]),
    ];

    frame.render_widget(
        Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme::WARN))
                .title(Span::styled(
                    " CONFIRM DELETE ",
                    Style::default().fg(theme::WARN).add_modifier(Modifier::BOLD),
                )),
        ),
        area,
    );
}

// ---------------------------------------------------------------------------
// Help
// ---------------------------------------------------------------------------

pub fn render_help(frame: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    lines.push(Line::from(vec![
        Span::styled("  toold ", theme::title()),
        Span::styled(
            format!("v{} — local-first personal finance ledger", env!("CARGO_PKG_VERSION")),
            theme::value(),
        ),
    ]));
    lines.push(Line::from(Span::styled(
        format!(
            "  All amounts are in {} and shown with two decimal places. Data is stored in SQLite.",
            app.currency
        ),
        theme::muted(),
    )));
    lines.push(Line::from(""));

    // Where the data actually lives, and how to point the tool elsewhere.
    // Kept above the key tables so it is visible without scrolling.
    lines.push(Line::from(Span::styled(
        "  Ledger storage",
        Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(vec![
        Span::styled(format!("    {:<22}", "Database"), theme::strong()),
        Span::styled(app.store.path().display().to_string(), theme::value()),
    ]));
    lines.push(Line::from(vec![
        Span::styled(format!("    {:<22}", "Chosen via"), theme::strong()),
        Span::styled(app.config.db_source.describe(), theme::value()),
    ]));
    if let Some(cfg) = &app.config.config_path {
        lines.push(Line::from(vec![
            Span::styled(format!("    {:<22}", "Config file"), theme::strong()),
            Span::styled(cfg.display().to_string(), theme::muted()),
        ]));
    }
    lines.push(Line::from(vec![
        Span::styled(format!("    {:<22}", "Override with"), theme::strong()),
        Span::styled(
            "TOOLD_DB=/path/ledger.db, TOOLD_DATA_DIR=/dir, or toold --db /path/ledger.db",
            theme::muted(),
        ),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "  Getting started: create an account (3, then n), add categories (4, then n),",
        theme::muted(),
    )));
    lines.push(Line::from(Span::styled(
        "  record income with i and expenses with e, then set budgets with b.",
        theme::muted(),
    )));
    lines.push(Line::from(""));

    for section in keys::ALL_SECTIONS {
        lines.push(Line::from(Span::styled(
            format!("  {}", section.title),
            Style::default().fg(theme::ACCENT).add_modifier(Modifier::BOLD),
        )));
        for binding in section.bindings {
            lines.push(Line::from(vec![
                Span::styled(format!("    {:<22}", binding.keys), theme::strong()),
                Span::styled(binding.action, theme::value()),
            ]));
        }
        lines.push(Line::from(""));
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .title(Span::styled(" HELP — key bindings ", theme::title()))
        .title_bottom(Span::styled(
            format!(" {} ", keys::modal_hint(keys::ModalHint::Help)),
            theme::muted(),
        ));

    frame.render_widget(
        Paragraph::new(lines).scroll((app.scroll, 0)).block(block),
        area,
    );
}
