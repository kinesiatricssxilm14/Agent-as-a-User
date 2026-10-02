//! Drawing the interface.
//!
//! Layout, top to bottom:
//!
//! ```text
//! ┌─ header: file, size, engine, flag chips, focus ─────────────────┐
//! │ Pattern: …                                                     │
//! │ Replace: …                                                     │
//! ├──────────────────────────────┬─────────────────────────────────┤
//! │ Source text, matches         │ Matches (index, offsets, text)   │
//! │ highlighted in place         ├─────────────────────────────────┤
//! ├──────────────────────────────┤ Capture groups / presets         │
//! │ Replacement preview          │                                  │
//! ├──────────────────────────────┴─────────────────────────────────┤
//! │ selected-match detail · status message · key bar                │
//! └────────────────────────────────────────────────────────────────┘
//! ```
//!
//! Everything is on one screen: no tabs, no modal overlays over the results.
//! The help pane and the prompt line take their own space in the layout rather
//! than covering the panes, so the match list and its offsets stay readable
//! while they are open.

use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::app::{App, Focus, Level, RightPane, PRESETS};
use crate::engine;
use crate::help::{self, Entry};
use crate::textlayout::{self, ViewParams};
use crate::theme;

/// Draw one frame.
pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    f.render_widget(Clear, area);

    // A very small terminal cannot show the panes meaningfully; say so instead
    // of drawing something unreadable.
    if area.width < 40 || area.height < 12 {
        let msg = Paragraph::new(vec![
            Line::from(Span::styled(
                "Terminal too small",
                Style::default().fg(theme::ERR).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                format!("{}x{} — need at least 40x12", area.width, area.height),
                theme::dim(),
            )),
        ])
        .alignment(Alignment::Center);
        f.render_widget(msg, area);
        return;
    }

    let has_preview = app.show_preview && app.replacement.is_some();
    let prompt_rows = if app.prompt.is_some() { 1 } else { 0 };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),           // header
            Constraint::Length(3),           // pattern field
            Constraint::Length(3),           // replace field
            Constraint::Min(6),              // panes
            Constraint::Length(2),           // detail + status
            Constraint::Length(prompt_rows), // prompt
            Constraint::Length(1),           // key bar
        ])
        .split(area);

    draw_header(f, app, chunks[0]);
    draw_field(f, app, chunks[1], Focus::Pattern);
    draw_field(f, app, chunks[2], Focus::Replace);
    draw_body(f, app, chunks[3], has_preview);
    draw_status(f, app, chunks[4]);
    if prompt_rows == 1 {
        draw_prompt(f, app, chunks[5]);
    }
    draw_keybar(f, app, chunks[6]);
}

/// The top information bar.
fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let mut spans = vec![
        Span::styled(
            " toole ",
            Style::default()
                .fg(theme::MATCH_FG)
                .bg(theme::TITLE)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::styled(
            app.doc.path().display().to_string(),
            Style::default()
                .fg(theme::TEXT)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(
                "  {} lines · {} chars · {} B",
                app.doc.line_count(),
                app.doc.char_count(),
                app.doc.byte_count()
            ),
            theme::dim(),
        ),
    ];

    // Flag chips: always visible, so the current mode is never a mystery.
    spans.push(Span::styled("  flags ", theme::dim()));
    for (c, on) in app.flags.chips() {
        spans.push(Span::styled(
            format!("{c}"),
            Style::default()
                .fg(if on { theme::FLAG_ON } else { theme::FLAG_OFF })
                .add_modifier(if on {
                    Modifier::BOLD | Modifier::UNDERLINED
                } else {
                    Modifier::DIM
                }),
        ));
        spans.push(Span::raw(" "));
    }

    if let Some(b) = app.matches.backend {
        spans.push(Span::styled(
            format!(" engine:{} ", b.label()),
            theme::dim(),
        ));
    }
    spans.push(Span::styled(
        format!(" focus:{} ", app.focus.label()),
        Style::default().fg(theme::BORDER_FOCUS),
    ));

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::HEADER_BG)),
        area,
    );
}

/// One of the two text input fields.
fn draw_field(f: &mut Frame, app: &App, area: Rect, which: Focus) {
    let focused = app.focus == which && app.prompt.is_none();
    let (input, title, hint) = match which {
        Focus::Pattern => (
            &app.pattern,
            " Pattern (regex) ",
            "type a regular expression — e.g. 1[3-9]\\d{9} for a mainland China mobile number",
        ),
        _ => (
            &app.replace,
            " Replace (template) ",
            "type a replacement — $1 for a capture group, $0 for the whole match",
        ),
    };

    // The error banner replaces the pattern field's border colour so an invalid
    // pattern is unmistakable while staying in place.
    let bad = which == Focus::Pattern && app.matches.error.is_some();
    let border = if bad {
        theme::ERR
    } else if focused {
        theme::BORDER_FOCUS
    } else {
        theme::BORDER
    };

    let mut title_spans = vec![Span::styled(
        title,
        Style::default()
            .fg(if bad { theme::ERR } else { theme::TITLE })
            .add_modifier(Modifier::BOLD),
    )];
    match which {
        Focus::Pattern => {
            let count = app.matches.len();
            if let Some(e) = &app.matches.error {
                title_spans.push(Span::styled(
                    format!("✗ {e} "),
                    Style::default().fg(theme::ERR).add_modifier(Modifier::BOLD),
                ));
            } else if !app.pattern.is_empty() {
                title_spans.push(Span::styled(
                    format!("{} match{} ", count, if count == 1 { "" } else { "es" }),
                    Style::default()
                        .fg(if count == 0 { theme::WARN } else { theme::OK })
                        .add_modifier(Modifier::BOLD),
                ));
                if app.matches.truncated {
                    title_spans.push(Span::styled(
                        format!("(stopped at {}) ", engine::MATCH_LIMIT),
                        Style::default().fg(theme::WARN),
                    ));
                }
                let groups = app.matches.group_names.len().saturating_sub(1);
                if groups > 0 {
                    title_spans.push(Span::styled(format!("{groups} group(s) "), theme::dim()));
                }
            }
        }
        _ => {
            if let Some(r) = &app.replacement {
                title_spans.push(Span::styled(
                    format!(
                        "{} applied{} ",
                        r.applied,
                        if app.first_only { " (first only)" } else { "" }
                    ),
                    Style::default().fg(theme::OK),
                ));
                let refs = engine::template_refs(app.replace.value());
                if !refs.is_empty() {
                    title_spans.push(Span::styled(
                        format!("refs {} ", refs.join(" ")),
                        theme::dim(),
                    ));
                }
            }
        }
    }

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border))
        .title(Line::from(title_spans));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let bg = if focused {
        theme::INPUT_BG_FOCUS
    } else {
        theme::INPUT_BG
    };

    // Scroll the field horizontally so the cursor stays visible.
    let width = inner.width as usize;
    let cursor_col = input.cursor_display_col();
    let scroll = cursor_col.saturating_sub(width.saturating_sub(1));

    // An empty field shows its hint whether or not it has focus, so a new user
    // can see what belongs in each one.
    let body: Line = if input.is_empty() {
        Line::from(Span::styled(hint, theme::dim()))
    } else {
        Line::from(Span::styled(
            input.value().to_string(),
            Style::default().fg(theme::TEXT),
        ))
    };

    f.render_widget(
        Paragraph::new(body)
            .style(Style::default().bg(bg))
            .scroll((0, scroll as u16)),
        inner,
    );

    if focused {
        let x = inner.x + (cursor_col - scroll).min(width.saturating_sub(1)) as u16;
        f.set_cursor_position((x, inner.y));
    }
}

/// The main area: text panes on the left, results on the right.
fn draw_body(f: &mut Frame, app: &mut App, area: Rect, has_preview: bool) {
    // Help takes a column of its own rather than covering the results, so the
    // match list and offsets stay visible while reading it.
    let (main, help_area) = if app.show_help {
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(30), Constraint::Length(46)])
            .split(area);
        (cols[0], Some(cols[1]))
    } else {
        (area, None)
    };

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(58), Constraint::Percentage(42)])
        .split(main);

    let left = if has_preview {
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
            .split(cols[0]);
        draw_preview(f, app, rows[1]);
        rows[0]
    } else {
        cols[0]
    };
    draw_source(f, app, left);

    let right = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(5), Constraint::Length(8)])
        .split(cols[1]);
    draw_match_list(f, app, right[0]);
    draw_right_panel(f, app, right[1]);

    if let Some(a) = help_area {
        draw_help(f, app, a);
    }
}

/// The source text with every match highlighted in place.
fn draw_source(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Source && app.prompt.is_none();
    let sel = app
        .selected
        .map(|i| format!(" · match {} selected", i + 1))
        .unwrap_or_default();
    let block = pane_block(
        format!(
            " Source · lines {}-{} of {}{} ",
            app.source_top + 1,
            (app.source_top + area.height.saturating_sub(2) as usize).min(app.doc.line_count()),
            app.doc.line_count(),
            sel
        ),
        focused,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Remember the geometry so paging and "scroll to selection" are accurate.
    app.source_view_rows = inner.height as usize;
    app.source_cols = inner.width as usize;

    let params = ViewParams {
        width: inner.width,
        height: inner.height,
        top_line: app.source_top,
        hscroll: app.source_hscroll,
        tab_width: app.tab_width,
        wrap: app.wrap,
        gutter: app.gutter,
    };
    let rendered = textlayout::render(&app.doc, &app.matches.highlights, &params);
    f.render_widget(Paragraph::new(rendered.lines), inner);
}

/// The complete replaced content.
fn draw_preview(f: &mut Frame, app: &mut App, area: Rect) {
    let Some(pd) = &app.preview_doc else { return };
    let applied = app.replacement.as_ref().map(|r| r.applied).unwrap_or(0);
    let block = pane_block(
        format!(
            " Replacement preview · full file · {} applied · {} lines · K/J scroll ",
            applied,
            pd.line_count()
        ),
        false,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    app.preview_view_rows = inner.height as usize;

    let params = ViewParams {
        width: inner.width,
        height: inner.height,
        top_line: app.preview_top,
        hscroll: 0,
        tab_width: app.tab_width,
        wrap: app.wrap,
        gutter: app.gutter,
    };
    let rendered = textlayout::render(pd, &app.preview_highlights, &params);
    f.render_widget(Paragraph::new(rendered.lines), inner);
}

/// The match list: index, offsets, position and text — all on one screen.
fn draw_match_list(f: &mut Frame, app: &mut App, area: Rect) {
    let focused = app.focus == Focus::Matches && app.prompt.is_none();
    let total = app.matches.len();
    let block = pane_block(
        format!(" Matches · {total} · offsets are 0-indexed "),
        focused,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }

    // One row is the column header; the rest list matches.
    let rows = inner.height.saturating_sub(1) as usize;
    app.list_view_rows = rows;

    let mut lines: Vec<Line> = Vec::with_capacity(inner.height as usize);
    lines.push(Line::from(Span::styled(
        format!(
            "{:>4} {:>7} {:>7}  {:<9} {}",
            "#", "start", "end", "line:col", "text"
        ),
        Style::default()
            .fg(theme::TITLE)
            .bg(theme::HEAD_BG)
            .add_modifier(Modifier::BOLD),
    )));

    if total == 0 {
        let msg = if app.pattern.is_empty() {
            "type a pattern in the Pattern field to see matches"
        } else if app.matches.error.is_some() {
            "the pattern is not valid — see the message above"
        } else {
            "no matches in this file"
        };
        lines.push(Line::from(Span::styled(msg, theme::dim())));
        f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: true }), inner);
        return;
    }

    let top = app.list_top.min(total.saturating_sub(1));
    for (i, m) in app.matches.matches.iter().enumerate().skip(top).take(rows) {
        let selected = app.selected == Some(i);
        let (cs, ce) = app.char_offsets(m);
        let (line, col) = app.line_col(m);
        let base = if selected {
            Style::default()
                .fg(theme::TEXT)
                .bg(theme::ROW_SEL_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            theme::text()
        };

        let prefix = format!(
            "{:>4} {:>7} {:>7}  {:<9} ",
            i + 1,
            cs,
            ce,
            format!("{line}:{col}")
        );
        let used = prefix.width();
        let room = (inner.width as usize).saturating_sub(used);
        // A zero-width match has nothing to show, so name it instead of
        // leaving the row looking truncated.
        let (shown, style) = if m.is_empty() {
            (
                display_snippet("(zero-width)", room),
                Style::default()
                    .fg(theme::REPL_FG)
                    .bg(theme::EMPTY_BG)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            (display_snippet(&m.text, room), theme::match_style(selected))
        };

        lines.push(Line::from(vec![
            Span::styled(prefix, base.patch(Style::default().fg(theme::DIM))),
            Span::styled(shown, style),
        ]));
    }

    f.render_widget(Paragraph::new(lines), inner);
}

/// Capture groups of the selected match, or the preset list.
fn draw_right_panel(f: &mut Frame, app: &App, area: Rect) {
    match app.right_pane {
        RightPane::Groups => draw_groups(f, app, area),
        RightPane::Presets => draw_presets(f, app, area),
    }
}

/// Capture groups of the selected match.
fn draw_groups(f: &mut Frame, app: &App, area: Rect) {
    let block = pane_block(" Capture groups · Ctrl+G for presets ".to_string(), false);
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines: Vec<Line> = Vec::new();
    match app.selected_match() {
        None => lines.push(Line::from(Span::styled("no match selected", theme::dim()))),
        Some(m) if m.groups.is_empty() => {
            lines.push(Line::from(Span::styled(
                "this pattern has no capture groups",
                theme::dim(),
            )));
            lines.push(Line::from(Span::styled(
                "add parentheses, e.g. (\\d+)-(\\d+), to capture parts",
                theme::dim(),
            )));
        }
        Some(m) => {
            for g in &m.groups {
                let label = match &g.name {
                    Some(n) => format!("${} ({n})", g.index),
                    None => format!("${}", g.index),
                };
                let (range, text, style) = match (&g.range, &g.text) {
                    (Some((s, e)), Some(t)) => (
                        format!("{}..{}", app.doc.byte_to_char(*s), app.doc.byte_to_char(*e)),
                        display_snippet(t, area.width as usize),
                        theme::match_style(false),
                    ),
                    _ => (
                        "—".to_string(),
                        "(did not participate)".to_string(),
                        theme::dim(),
                    ),
                };
                lines.push(Line::from(vec![
                    Span::styled(format!("{label:<12} "), Style::default().fg(theme::TITLE)),
                    Span::styled(format!("{range:<12} "), theme::dim()),
                    Span::styled(text, style),
                ]));
            }
        }
    }
    f.render_widget(Paragraph::new(lines), inner);
}

/// The built-in pattern presets.
fn draw_presets(f: &mut Frame, app: &App, area: Rect) {
    let block = pane_block(
        " Presets · [ ] to apply · Ctrl+G for groups ".to_string(),
        false,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);
    if inner.height == 0 {
        return;
    }

    // Keep the highlighted preset in view.
    let rows = inner.height as usize;
    let top = app
        .preset_idx
        .saturating_sub(rows / 2)
        .min(PRESETS.len().saturating_sub(rows));

    let lines: Vec<Line> = PRESETS
        .iter()
        .enumerate()
        .skip(top)
        .take(rows)
        .map(|(i, (name, pat))| {
            let sel = i == app.preset_idx;
            let base = if sel {
                Style::default()
                    .fg(theme::TEXT)
                    .bg(theme::ROW_SEL_BG)
                    .add_modifier(Modifier::BOLD)
            } else {
                theme::text()
            };
            Line::from(vec![
                Span::styled(if sel { "▸ " } else { "  " }, base),
                Span::styled(format!("{name:<22} "), base),
                Span::styled(
                    display_snippet(pat, (inner.width as usize).saturating_sub(26)),
                    base.patch(Style::default().fg(theme::DIM)),
                ),
            ])
        })
        .collect();
    f.render_widget(Paragraph::new(lines), inner);
}

/// The key reference.
fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let block = pane_block(
        " Help · ↑↓ PageUp/PageDown scroll · F1 or Esc closes ".to_string(),
        false,
    );
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines: Vec<Line> = help::HELP
        .iter()
        .map(|e| match e {
            Entry::Section(t) => Line::from(Span::styled(
                *t,
                Style::default()
                    .fg(theme::TITLE)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            )),
            Entry::Key(k, d) => Line::from(vec![
                Span::styled(format!("{k:<19} "), theme::key()),
                Span::styled(*d, theme::text()),
            ]),
            Entry::Note(t) => Line::from(Span::styled(*t, theme::dim())),
            Entry::Blank => Line::from(""),
        })
        .collect();

    let max_scroll = help::line_count().saturating_sub(1) as u16;
    f.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .scroll((app.help_scroll.min(max_scroll), 0)),
        inner,
    );
}

/// The selected-match detail line plus the status message.
fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    // Detail line: the selected match spelled out, so the offsets are readable
    // without counting columns in the list.
    let detail: Line = match app.selected_match() {
        Some(m) => {
            let (cs, ce) = app.char_offsets(m);
            let (line, col) = app.line_col(m);
            let idx = app.selected.unwrap_or(0) + 1;
            let mut spans = vec![
                Span::styled(
                    format!(" match {idx}/{} ", app.matches.len()),
                    Style::default()
                        .fg(theme::MATCH_FG)
                        .bg(theme::SEL_MATCH_BG)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" start ", theme::dim()),
                Span::styled(cs.to_string(), theme::text()),
                Span::styled("  end ", theme::dim()),
                Span::styled(ce.to_string(), theme::text()),
                Span::styled("  length ", theme::dim()),
                Span::styled((ce - cs).to_string(), theme::text()),
                Span::styled("  line:col ", theme::dim()),
                Span::styled(format!("{line}:{col}"), theme::text()),
                Span::styled("  bytes ", theme::dim()),
                Span::styled(format!("{}..{}", m.start, m.end), theme::text()),
                Span::styled("  text ", theme::dim()),
            ];
            let used: usize = spans.iter().map(|s| s.content.width()).sum();
            spans.push(Span::styled(
                display_snippet(&m.text, (area.width as usize).saturating_sub(used + 1)),
                theme::match_style(true),
            ));
            Line::from(spans)
        }
        None => Line::from(Span::styled(
            if app.pattern.is_empty() {
                " no pattern yet — type one in the Pattern field above (F1 for help)"
            } else {
                " no match selected"
            },
            theme::dim(),
        )),
    };
    f.render_widget(
        Paragraph::new(detail).style(Style::default().bg(theme::DETAIL_BG)),
        rows[0],
    );

    // Status message, or the effective pattern when there is nothing to say.
    let status: Line = match &app.message {
        Some(m) => {
            let (fg, bg, mark) = match m.level {
                Level::Info => (theme::TEXT, theme::DETAIL_BG, "·"),
                Level::Success => (theme::OK, theme::DETAIL_BG, "✓"),
                Level::Warn => (theme::WARN, theme::DETAIL_BG, "!"),
                Level::Error => (theme::TEXT, theme::ERR_BG, "✗"),
            };
            Line::from(vec![
                Span::styled(
                    format!(" {mark} "),
                    Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD),
                ),
                Span::styled(m.text.clone(), Style::default().fg(fg).bg(bg)),
            ])
        }
        None => {
            let mut spans = vec![Span::styled(" pattern sent to engine: ", theme::dim())];
            if app.matches.effective_pattern.is_empty() {
                spans.push(Span::styled("(none)", theme::dim()));
            } else {
                spans.push(Span::styled(
                    app.matches.effective_pattern.clone(),
                    Style::default().fg(theme::TEXT),
                ));
            }
            if let Some(reason) = &app.matches.fallback {
                spans.push(Span::styled(
                    format!("  · switched to the backtracking engine: {reason}"),
                    Style::default().fg(theme::WARN),
                ));
            }
            Line::from(spans)
        }
    };
    f.render_widget(Paragraph::new(status), rows[1]);
}

/// The footer prompt line.
fn draw_prompt(f: &mut Frame, app: &App, area: Rect) {
    let Some(p) = &app.prompt else { return };
    let label = p.label();
    let mut spans = vec![Span::styled(
        format!(" {label} "),
        Style::default()
            .fg(theme::MATCH_FG)
            .bg(theme::WARN)
            .add_modifier(Modifier::BOLD),
    )];
    if p.is_text() {
        spans.push(Span::styled(
            format!(" {}", app.prompt_input.value()),
            Style::default().fg(theme::TEXT).bg(theme::INPUT_BG_FOCUS),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::INPUT_BG_FOCUS)),
        area,
    );

    if p.is_text() {
        let x = area.x + label.width() as u16 + 3 + app.prompt_input.cursor_display_col() as u16;
        f.set_cursor_position((x.min(area.x + area.width.saturating_sub(1)), area.y));
    }
}

/// The bottom key bar: context-sensitive so the keys shown are the ones that
/// apply to the current focus.
fn draw_keybar(f: &mut Frame, app: &App, area: Rect) {
    let pairs: Vec<(&str, &str)> = if app.prompt.is_some() {
        vec![("Enter", "confirm"), ("Esc", "cancel"), ("y/n", "answer")]
    } else if app.show_help {
        vec![
            ("↑↓/PgUp/PgDn", "scroll help"),
            ("F1/Esc", "close"),
            ("Tab", "focus"),
        ]
    } else {
        match app.focus {
            Focus::Pattern | Focus::Replace => vec![
                ("Tab", "next field"),
                ("Enter", "results"),
                ("Ctrl+N/P", "match"),
                ("F2", "(?i)"),
                ("F7", "preview"),
                ("Ctrl+S", "write"),
                ("F1", "help"),
                ("Ctrl+Q", "quit"),
            ],
            Focus::Matches => vec![
                ("↑↓/jk", "match"),
                ("Enter", "show in text"),
                ("i", "pattern"),
                ("r", "replace"),
                ("[ ]", "presets"),
                ("Ctrl+G", "panel"),
                ("F1/?", "help"),
                ("Ctrl+Q", "quit"),
            ],
            Focus::Source => vec![
                ("↑↓/jk", "scroll"),
                ("PgUp/PgDn", "page"),
                ("←→", "columns"),
                ("n/p", "match"),
                ("K/J", "preview"),
                ("F9", "wrap"),
                ("F1/?", "help"),
                ("Ctrl+Q", "quit"),
            ],
        }
    };

    let mut spans = Vec::with_capacity(pairs.len() * 3);
    for (k, d) in pairs {
        spans.push(Span::styled(format!(" {k}"), theme::key()));
        spans.push(Span::styled(format!(" {d}"), theme::dim()));
        spans.push(Span::styled(" ·", Style::default().fg(theme::BORDER)));
    }
    spans.pop();

    f.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(theme::FOOTER_BG)),
        area,
    );
}

/// A bordered pane, highlighted when focused.
fn pane_block(title: String, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused {
            theme::BORDER_FOCUS
        } else {
            theme::BORDER
        }))
        .title(Span::styled(
            title,
            Style::default()
                .fg(if focused {
                    theme::BORDER_FOCUS
                } else {
                    theme::TITLE
                })
                .add_modifier(Modifier::BOLD),
        ))
}

/// Render `text` on one line, escaping newlines and tabs and truncating to
/// `width` display cells.
pub fn display_snippet(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let (piece, w): (String, usize) = match ch {
            '\n' => ("⏎".to_string(), 1),
            '\r' => ("␍".to_string(), 1),
            '\t' => ("⇥".to_string(), 1),
            c if (c as u32) < 0x20 => ("·".to_string(), 1),
            c => {
                let w = unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                (c.to_string(), w)
            }
        };
        if used + w > width {
            // Leave room for the ellipsis when something is being cut off.
            if width >= 1 {
                while used + 1 > width {
                    let last = out.chars().next_back();
                    match last {
                        Some(c) => {
                            used -= unicode_width::UnicodeWidthChar::width(c).unwrap_or(1);
                            out.pop();
                        }
                        None => break,
                    }
                }
                out.push('…');
            }
            return out;
        }
        out.push_str(&piece);
        used += w;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snippet_passes_short_text_through() {
        assert_eq!(display_snippet("abc", 10), "abc");
    }

    #[test]
    fn snippet_truncates_with_an_ellipsis() {
        assert_eq!(display_snippet("abcdefgh", 4), "abc…");
        assert_eq!(display_snippet("abcdefgh", 1), "…");
    }

    #[test]
    fn snippet_escapes_control_characters() {
        assert_eq!(display_snippet("a\nb\tc", 10), "a⏎b⇥c");
        assert_eq!(display_snippet("a\u{1}b", 10), "a·b");
    }

    #[test]
    fn snippet_respects_wide_characters() {
        // Two CJK characters are four cells wide.
        assert_eq!(display_snippet("English-only text", 4), "English-only text");
        assert_eq!(display_snippet("English-only text", 3), "English-only text…");
    }

    #[test]
    fn snippet_of_empty_text_is_empty() {
        assert_eq!(display_snippet("", 20), "");
    }

    #[test]
    fn snippet_with_no_room() {
        assert_eq!(display_snippet("abc", 0), "");
    }
}
