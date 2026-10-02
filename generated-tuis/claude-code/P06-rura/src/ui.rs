//! Rendering.
//!
//! The layout is a single screen: header, command line, output, and side panels are all
//! visible at once. Prompts and completion lists are rendered as *panels*, never as modal
//! overlays, so nothing on screen is ever hidden behind something else.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Clear, List, ListItem, ListState, Paragraph, Scrollbar,
    ScrollbarOrientation, ScrollbarState, Wrap,
};
use unicode_width::UnicodeWidthStr;

use crate::app::{App, HELP_LINES, Level, Prompt};
use crate::exec::RunKind;
use crate::highlight::{TokenClass, highlight};
use crate::pipeline;

// A restrained palette that stays legible on both dark and light terminals.
const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;

/// Colour for a syntax class.
fn class_style(class: TokenClass) -> Style {
    match class {
        TokenClass::Command => Style::default().fg(Color::LightGreen).bold(),
        TokenClass::Flag => Style::default().fg(Color::LightYellow),
        TokenClass::String => Style::default().fg(Color::LightMagenta),
        TokenClass::Variable => Style::default().fg(Color::LightBlue),
        TokenClass::Pipe => Style::default().fg(Color::LightRed).bold(),
        TokenClass::Operator => Style::default().fg(Color::LightRed),
        TokenClass::Path => Style::default().fg(Color::LightCyan),
        TokenClass::Number => Style::default().fg(Color::Yellow),
        TokenClass::Plain => Style::default().fg(Color::White),
    }
}

/// Draw the whole interface.
pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    // A very small terminal cannot show the full layout; say so rather than panicking.
    if area.width < 40 || area.height < 12 {
        let msg = Paragraph::new("Terminal too small — please enlarge to at least 40x12")
            .style(Style::default().fg(Color::LightRed))
            .wrap(Wrap { trim: true });
        f.render_widget(msg, area);
        return;
    }

    // The prompt row only exists while a prompt is open, so no space is wasted otherwise.
    let prompt_h = if app.prompt == Prompt::None { 0 } else { 3 };
    let [header, cmd, body, prompt_row, status, keys] = Layout::vertical([
        Constraint::Length(3),
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(prompt_h),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    draw_header(f, app, header);
    let cursor = draw_command(f, app, cmd);
    draw_body(f, app, body);
    if prompt_h > 0 {
        let pcur = draw_prompt(f, app, prompt_row);
        // The prompt owns the cursor while it is open.
        f.set_cursor_position(pcur);
    } else if let Some(pos) = cursor {
        f.set_cursor_position(pos);
    }
    draw_status(f, app, status);
    draw_keybar(f, app, keys);
}

/// Header: which file the session targets, plus its real size on disk.
fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let exists = app.file_exists();
    let (marker, marker_style) = if exists {
        ("●", Style::default().fg(Color::LightGreen))
    } else {
        ("✗", Style::default().fg(Color::LightRed))
    };
    let mut spans = vec![
        Span::styled(" toolf ", Style::default().fg(Color::Black).bg(ACCENT).bold()),
        Span::raw("  "),
        Span::styled(marker, marker_style),
        Span::raw(" "),
        Span::styled("log file: ", Style::default().fg(DIM)),
        Span::styled(
            app.file.display().to_string(),
            Style::default().fg(Color::LightCyan).bold(),
        ),
        Span::styled("   size: ", Style::default().fg(DIM)),
        Span::styled(app.file_info(), Style::default().fg(Color::White)),
    ];
    if !exists {
        spans.push(Span::styled("   (file not found)", Style::default().fg(Color::LightRed)));
    }
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM))
        .title(Span::styled(" session ", Style::default().fg(ACCENT)));
    f.render_widget(Paragraph::new(Line::from(spans)).block(block), area);
}

/// The editable, syntax-highlighted command line. Returns where the terminal cursor goes.
fn draw_command(f: &mut Frame, app: &App, area: Rect) -> Option<Position> {
    let (stage_n, stage_total) = app.stage_position();
    let validation = app.validation();
    let border_style = if validation.is_some() {
        Style::default().fg(Color::LightRed)
    } else {
        Style::default().fg(ACCENT)
    };

    let mut title = vec![
        Span::styled(" pipeline ", Style::default().fg(ACCENT).bold()),
        Span::styled(
            format!("[stage {stage_n}/{stage_total}] "),
            Style::default().fg(Color::LightYellow),
        ),
    ];
    if let Some(v) = &validation {
        title.push(Span::styled(format!("⚠ {v} "), Style::default().fg(Color::LightRed).bold()));
    }

    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border_style)
        .title(Line::from(title))
        .title_bottom(Line::from(Span::styled(
            " Enter=run  Alt+\\=run to cursor  Tab=complete ",
            Style::default().fg(DIM),
        )));
    let inner = block.inner(area);
    f.render_widget(block, area);

    // Highlight the stage the cursor is in so partial execution is predictable.
    let stages = pipeline::split_stages(&app.input);
    let active = pipeline::stage_at(&stages, app.cursor);
    let active_range = stages[active].range.clone();

    let tokens = highlight(&app.input);
    let mut spans = vec![Span::styled("$ ", Style::default().fg(Color::LightGreen).bold())];
    for t in &tokens {
        let text: String = app.input[t.start..t.end].iter().collect();
        let mut style = class_style(t.class);
        // Dim everything outside the active stage.
        if t.end <= active_range.start || t.start >= active_range.end {
            style = style.add_modifier(Modifier::DIM);
        }
        spans.push(Span::styled(text, style));
    }
    if app.input.is_empty() {
        spans.push(Span::styled(
            "type a pipeline, e.g. cat /bench/server.log | grep ERROR | tail -n 20",
            Style::default().fg(DIM).italic(),
        ));
    }

    // Scroll horizontally so the cursor stays visible on long lines.
    let prefix_w: usize = app.input[..app.cursor.min(app.input.len())]
        .iter()
        .collect::<String>()
        .width();
    let avail = inner.width.saturating_sub(2) as usize; // "$ " takes two columns
    let scroll = prefix_w.saturating_sub(avail.saturating_sub(1)) as u16;

    f.render_widget(Paragraph::new(Line::from(spans)).scroll((0, scroll)), inner);

    if app.prompt != Prompt::None {
        return None;
    }
    let x = inner.x + 2 + (prefix_w as u16).saturating_sub(scroll);
    Some(Position { x: x.min(inner.right().saturating_sub(1)), y: inner.y })
}

/// Output pane plus the right-hand panels.
fn draw_body(f: &mut Frame, app: &mut App, area: Rect) {
    // Reserve a side column for help / completion / stages when there is room.
    let side_w = if area.width >= 96 { 38 } else if area.width >= 72 { 30 } else { 0 };
    let (out_area, side_area) = if side_w > 0 {
        let [a, b] =
            Layout::horizontal([Constraint::Min(20), Constraint::Length(side_w)]).areas(area);
        (a, Some(b))
    } else {
        (area, None)
    };

    draw_output(f, app, out_area);

    if let Some(side) = side_area {
        // Stage list is always useful; help and completions share the remaining space.
        let comp_h = app
            .completion
            .as_ref()
            .map(|c| (c.candidates.len().min(8) + 2) as u16)
            .unwrap_or(0);
        let stage_h = {
            let n = pipeline::split_stages(&app.input).len().min(6) as u16;
            n + 2
        };
        let constraints = if app.show_help {
            vec![
                Constraint::Length(comp_h),
                Constraint::Length(stage_h),
                Constraint::Min(6),
            ]
        } else {
            vec![
                Constraint::Length(comp_h),
                Constraint::Length(stage_h),
                Constraint::Length(0),
            ]
        };
        let chunks = Layout::vertical(constraints).split(side);
        if comp_h > 0 {
            draw_completion(f, app, chunks[0]);
        }
        draw_stages(f, app, chunks[1]);
        if app.show_help && chunks[2].height > 2 {
            draw_help(f, app, chunks[2]);
        }
    }
}

/// The output pane: stdout, then any stderr, with search matches highlighted.
fn draw_output(f: &mut Frame, app: &mut App, area: Rect) {
    let mut title = vec![Span::styled(" output ", Style::default().fg(ACCENT).bold())];
    let mut bottom = Vec::new();
    if let Some(r) = &app.result {
        let kind = match r.kind {
            RunKind::Full => "full".to_string(),
            RunKind::Partial(n, total) => format!("partial {n}/{total}"),
        };
        let code_style = if r.succeeded() {
            Style::default().fg(Color::LightGreen)
        } else {
            Style::default().fg(Color::LightRed)
        };
        title.push(Span::styled(format!("[{kind}] "), Style::default().fg(Color::LightYellow)));
        title.push(Span::styled(
            format!(
                "exit {} ",
                r.exit_code.map(|c| c.to_string()).unwrap_or_else(|| "-".into())
            ),
            code_style,
        ));
        title.push(Span::styled(
            format!("{} line(s) {} B ", r.line_count(), r.byte_count()),
            Style::default().fg(DIM),
        ));
        if r.truncated {
            title.push(Span::styled("[truncated] ", Style::default().fg(Color::LightRed)));
        }
        bottom.push(Span::styled(
            format!(" $ {} ", ellipsize(&r.command, area.width.saturating_sub(4) as usize)),
            Style::default().fg(DIM).italic(),
        ));
    } else {
        title.push(Span::styled("[no run yet] ", Style::default().fg(DIM)));
    }
    if app.running {
        // A simple animated marker so long pipelines look alive.
        let frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let i = app
            .run_elapsed()
            .map(|d| (d.as_millis() / 100) as usize % frames.len())
            .unwrap_or(0);
        title.push(Span::styled(
            format!("{} running… ", frames[i]),
            Style::default().fg(Color::LightYellow).bold(),
        ));
    }

    let mut block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM))
        .title(Line::from(title));
    if !bottom.is_empty() {
        block = block.title_bottom(Line::from(bottom));
    }
    let inner = block.inner(area);
    f.render_widget(block, area);

    app.output_height = inner.height as usize;

    let needle = app.search_term.to_lowercase();
    let mut lines: Vec<Line> = Vec::new();
    let total = app.output_lines().len();
    let start = app.output_scroll.min(total.saturating_sub(1));
    let width = inner.width as usize;
    let gutter = number_width(total);

    for (i, text) in app
        .output_lines()
        .iter()
        .enumerate()
        .skip(start)
        .take(inner.height as usize)
    {
        let is_match = !needle.is_empty() && text.to_lowercase().contains(&needle);
        let is_current = app.matches.get(app.match_pos).copied() == Some(i);
        let num_style = if is_current {
            Style::default().fg(Color::Black).bg(Color::LightYellow)
        } else if is_match {
            Style::default().fg(Color::LightYellow)
        } else {
            Style::default().fg(DIM)
        };
        let mut spans = vec![
            Span::styled(format!("{:>w$} ", i + 1, w = gutter), num_style),
            Span::styled("│ ", Style::default().fg(DIM)),
        ];
        let body_width = width.saturating_sub(gutter + 3);
        let shown = truncate(text, body_width);
        if is_match {
            spans.extend(highlight_matches(&shown, &needle, is_current));
        } else {
            spans.push(Span::styled(shown, Style::default().fg(Color::White)));
        }
        lines.push(Line::from(spans));
    }

    if lines.is_empty() {
        let hint = if app.result.is_some() {
            "(no output)"
        } else {
            "press Enter to run the pipeline above"
        };
        lines.push(Line::from(Span::styled(hint, Style::default().fg(DIM).italic())));
    }

    // stderr is appended in the same pane so errors are never hidden from the snapshot.
    if let Some(r) = &app.result
        && !r.stderr.is_empty()
    {
        lines.push(Line::from(Span::styled(
            "── stderr ──",
            Style::default().fg(Color::LightRed).bold(),
        )));
        for e in r.stderr.iter().take(6) {
            lines.push(Line::from(Span::styled(
                truncate(e, width),
                Style::default().fg(Color::LightRed),
            )));
        }
        if r.stderr.len() > 6 {
            lines.push(Line::from(Span::styled(
                format!("… {} more stderr line(s)", r.stderr.len() - 6),
                Style::default().fg(Color::LightRed).italic(),
            )));
        }
    }
    if let Some(err) = app.result.as_ref().and_then(|r| r.error.as_ref()) {
        lines.push(Line::from(Span::styled(
            format!("⚠ {err}"),
            Style::default().fg(Color::LightRed).bold(),
        )));
    }

    f.render_widget(Paragraph::new(Text::from(lines)), inner);

    // Scrollbar makes it obvious that more output exists below the fold.
    if total > inner.height as usize {
        let mut state = ScrollbarState::new(total).position(app.output_scroll);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(DIM))
                .begin_symbol(None)
                .end_symbol(None),
            area,
            &mut state,
        );
    }
}

/// Split `text` so occurrences of `needle` are visually marked.
fn highlight_matches(text: &str, needle: &str, current: bool) -> Vec<Span<'static>> {
    let base = Style::default().fg(Color::White);
    let hit = if current {
        Style::default().fg(Color::Black).bg(Color::LightYellow).bold()
    } else {
        Style::default().fg(Color::Black).bg(Color::Yellow)
    };
    if needle.is_empty() {
        return vec![Span::styled(text.to_string(), base)];
    }
    let lower = text.to_lowercase();
    let mut spans = Vec::new();
    let mut pos = 0usize;
    while let Some(rel) = lower[pos..].find(needle) {
        let at = pos + rel;
        if at > pos {
            spans.push(Span::styled(text[pos..at].to_string(), base));
        }
        let end = at + needle.len();
        spans.push(Span::styled(text[at..end].to_string(), hit));
        pos = end;
    }
    if pos < text.len() {
        spans.push(Span::styled(text[pos..].to_string(), base));
    }
    spans
}

/// Per-stage breakdown of the current pipeline, marking the cursor's stage.
fn draw_stages(f: &mut Frame, app: &App, area: Rect) {
    if area.height < 3 {
        return;
    }
    let stages = pipeline::split_stages(&app.input);
    let active = pipeline::stage_at(&stages, app.cursor);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM))
        .title(Span::styled(" stages ", Style::default().fg(ACCENT).bold()));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let mut lines = Vec::new();
    for (i, s) in stages.iter().enumerate().take(inner.height as usize) {
        let word = s.command_word(&app.input);
        let label = if word.is_empty() { "(empty)".to_string() } else { word };
        let marker = if i == active { "▸" } else { " " };
        let style = if i == active {
            Style::default().fg(Color::LightYellow).bold()
        } else {
            Style::default().fg(Color::White)
        };
        lines.push(Line::from(vec![
            Span::styled(format!("{marker} {}. ", i + 1), style),
            Span::styled(
                truncate(&label, inner.width.saturating_sub(6) as usize),
                style,
            ),
        ]));
    }
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Completion candidates, rendered as a bordered side panel (never over the output).
fn draw_completion(f: &mut Frame, app: &App, area: Rect) {
    let Some(c) = &app.completion else { return };
    if area.height < 3 {
        return;
    }
    let kind = match c.kind {
        crate::complete::CompKind::Command => "commands",
        crate::complete::CompKind::Path => "paths",
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::LightMagenta))
        .title(Span::styled(
            format!(" {kind}: {} ", c.candidates.len()),
            Style::default().fg(Color::LightMagenta).bold(),
        ));
    let inner = block.inner(area);
    // Clear only the panel's own rectangle so stale text cannot bleed through.
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let width = inner.width as usize;
    let items: Vec<ListItem> = c
        .candidates
        .iter()
        .map(|cand| {
            // Show the leaf name for long paths; the full value still goes into the line.
            let shown = if cand.len() > width {
                let leaf = cand.trim_end_matches('/').rsplit('/').next().unwrap_or(cand);
                let suffix = if cand.ends_with('/') { "/" } else { "" };
                format!("…/{leaf}{suffix}")
            } else {
                cand.clone()
            };
            ListItem::new(truncate(&shown, width))
        })
        .collect();
    let mut state = ListState::default();
    state.select(Some(c.selected));
    let list = List::new(items)
        .highlight_style(Style::default().fg(Color::Black).bg(Color::LightMagenta).bold())
        .highlight_symbol("▸ ");
    f.render_stateful_widget(list, inner, &mut state);
}

/// Scrollable key reference, always available in-app.
fn draw_help(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(DIM))
        .title(Span::styled(" keys (F1) ", Style::default().fg(ACCENT).bold()))
        .title_bottom(Span::styled(" Alt+↑↓ scroll ", Style::default().fg(DIM)));
    let inner = block.inner(area);
    f.render_widget(block, area);

    let lines: Vec<Line> = HELP_LINES
        .iter()
        .skip(app.help_scroll)
        .take(inner.height as usize)
        .map(|(key, desc)| {
            if key.is_empty() {
                Line::from(Span::styled(
                    *desc,
                    Style::default().fg(Color::LightYellow).bold(),
                ))
            } else {
                Line::from(vec![
                    Span::styled(format!("{key:<11}"), Style::default().fg(ACCENT)),
                    Span::styled(*desc, Style::default().fg(Color::White)),
                ])
            }
        })
        .collect();
    f.render_widget(Paragraph::new(Text::from(lines)), inner);
}

/// Inline prompt row for save / search. Returns the cursor position inside it.
fn draw_prompt(f: &mut Frame, app: &App, area: Rect) -> Position {
    let (label, hint, color) = match app.prompt {
        Prompt::Save => (
            " save output to ",
            " Enter=write  Tab=complete path  Esc=cancel ",
            Color::LightGreen,
        ),
        Prompt::Search => (
            " search output ",
            " Enter=find  Esc=cancel ",
            Color::LightYellow,
        ),
        Prompt::None => (" ", " ", DIM),
    };
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(color))
        .title(Span::styled(label, Style::default().fg(color).bold()))
        .title_bottom(Span::styled(hint, Style::default().fg(DIM)));
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);

    let prefix_w: usize = app
        .prompt_input
        .chars()
        .take(app.prompt_cursor)
        .collect::<String>()
        .width();
    let avail = inner.width.saturating_sub(2) as usize;
    let scroll = prefix_w.saturating_sub(avail.saturating_sub(1)) as u16;

    let line = Line::from(vec![
        Span::styled("> ", Style::default().fg(color).bold()),
        Span::styled(app.prompt_input.clone(), Style::default().fg(Color::White)),
    ]);
    f.render_widget(Paragraph::new(line).scroll((0, scroll)), inner);

    let x = inner.x + 2 + (prefix_w as u16).saturating_sub(scroll);
    Position { x: x.min(inner.right().saturating_sub(1)), y: inner.y }
}

/// Status line: last operation's outcome.
fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let (icon, style) = match app.status_level {
        Level::Info => ("•", Style::default().fg(ACCENT)),
        Level::Success => ("✓", Style::default().fg(Color::LightGreen)),
        Level::Error => ("✗", Style::default().fg(Color::LightRed)),
    };
    let mut spans = vec![
        Span::styled(format!(" {icon} "), style),
        Span::styled(
            truncate(&app.status, area.width.saturating_sub(4) as usize),
            style,
        ),
    ];
    if !app.search_term.is_empty() {
        spans.push(Span::styled(
            format!("   [search: {} · {} hit(s)]", app.search_term, app.matches.len()),
            Style::default().fg(DIM),
        ));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Always-visible shortcut bar, so keys are discoverable without opening help.
fn draw_keybar(f: &mut Frame, app: &App, area: Rect) {
    let keys: &[(&str, &str)] = if app.prompt != Prompt::None {
        &[("Enter", "confirm"), ("Tab", "complete"), ("Esc", "cancel")]
    } else {
        &[
            ("Enter", "run"),
            ("Alt+\\", "run→cursor"),
            ("Alt+←→", "stage"),
            ("Tab", "complete"),
            ("Ctrl+S", "save"),
            ("Ctrl+F", "find"),
            ("F3", "next"),
            ("↑↓", "history"),
            ("PgUp/Dn", "scroll"),
            ("F1", "help"),
            ("F10", "quit"),
        ]
    };
    let mut spans = Vec::new();
    // Fit as many hints as the width allows, but never drop the last few (help/quit): a user
    // who cannot see how to quit or open help is stuck.
    const ESSENTIAL: usize = 2;
    let budget = area.width as usize;
    let mut used = 0usize;
    let tail: usize = keys
        .iter()
        .skip(keys.len().saturating_sub(ESSENTIAL))
        .map(|(k, d)| k.width() + d.width() + 4)
        .sum();
    let optional = keys.len().saturating_sub(ESSENTIAL);
    for (i, (k, d)) in keys.iter().enumerate() {
        let w = k.width() + d.width() + 4;
        // Reserve room for the essential trailing hints while emitting the optional ones.
        let reserve = if i < optional { tail } else { 0 };
        if used + w + reserve > budget {
            continue;
        }
        used += w;
        spans.push(Span::styled(
            format!(" {k} "),
            Style::default().fg(Color::Black).bg(ACCENT).bold(),
        ));
        spans.push(Span::styled(format!(" {d}  "), Style::default().fg(Color::Gray)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// Number of columns needed to print `n`.
fn number_width(n: usize) -> usize {
    let mut w = 1;
    let mut v = n;
    while v >= 10 {
        v /= 10;
        w += 1;
    }
    w.max(3)
}

/// Cut `s` to `width` display columns.
fn truncate(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if s.width() <= width {
        return s.to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for c in s.chars() {
        let cw = c.to_string().width();
        if used + cw > width {
            break;
        }
        out.push(c);
        used += cw;
    }
    out
}

/// Shorten a long command for a title, keeping both ends readable.
fn ellipsize(s: &str, width: usize) -> String {
    if width < 8 || s.width() <= width {
        return truncate(s, width.max(1));
    }
    let keep = width - 1;
    let head = keep / 2;
    let tail = keep - head;
    let chars: Vec<char> = s.chars().collect();
    let h: String = chars.iter().take(head).collect();
    let t: String = chars[chars.len().saturating_sub(tail)..].iter().collect();
    format!("{h}…{t}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    fn app_with(cmd: &str) -> App {
        let mut a = App::new(PathBuf::from("/tmp/toolf-ui.log"));
        a.clear_line();
        a.insert_str(cmd);
        a
    }

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| draw(f, app)).unwrap();
        let buf = term.backend().buffer().clone();
        (0..buf.area.height)
            .map(|y| {
                (0..buf.area.width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn run_and_wait(a: &mut App) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while a.running && Instant::now() < deadline {
            if !a.poll() {
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        a.poll();
    }

    #[test]
    fn renders_without_panicking_at_many_sizes() {
        for (w, h) in [(40, 12), (60, 20), (80, 24), (100, 30), (200, 60), (39, 11), (20, 8)] {
            let mut a = app_with("cat /tmp/x.log | grep ERROR | tail -n 5");
            let _ = render(&mut a, w, h);
        }
    }

    #[test]
    fn command_line_and_output_are_on_the_same_screen() {
        let mut a = app_with("printf 'alpha\\nbeta\\n'");
        a.run_full();
        run_and_wait(&mut a);
        let screen = render(&mut a, 100, 30);
        // The command being edited is visible...
        assert!(screen.contains("printf"), "{screen}");
        // ...at the same time as its output.
        assert!(screen.contains("alpha"), "{screen}");
        assert!(screen.contains("beta"), "{screen}");
        // And so is the stage/exit metadata.
        assert!(screen.contains("exit 0"), "{screen}");
    }

    #[test]
    fn partial_run_output_and_command_visible_together() {
        let mut a = app_with("printf '1\\n2\\n3\\n' | tail -n 1 | wc -l");
        a.move_home();
        a.move_next_boundary();
        a.move_next_boundary();
        a.run_partial();
        run_and_wait(&mut a);
        let screen = render(&mut a, 100, 30);
        assert!(screen.contains("partial 2/3"), "{screen}");
        assert!(screen.contains("tail"), "{screen}");
    }

    #[test]
    fn keybar_and_help_document_shortcuts() {
        // The essential keys must survive at every supported width, including narrow ones.
        for w in [40u16, 60, 80, 100, 140, 200] {
            let mut a = app_with("cat x");
            let screen = render(&mut a, w, 30);
            let bar = screen.lines().last().unwrap().to_string();
            assert!(bar.contains("F1"), "width {w} lost help hint: {bar:?}");
            assert!(bar.contains("F10"), "width {w} lost quit hint: {bar:?}");
            // The bar must never wrap or overflow its single row.
            assert!(bar.width() <= w as usize, "width {w} overflowed: {bar:?}");
        }
        let mut a = app_with("cat x");
        let screen = render(&mut a, 100, 30);
        assert!(screen.contains("Enter"), "{screen}");
        assert!(screen.contains("run→cursor") || screen.contains("Alt+\\"), "{screen}");
        // Side help panel lists the full reference.
        assert!(screen.contains("keys (F1)"), "{screen}");
    }

    #[test]
    fn prompt_row_appears_and_keeps_output_visible() {
        let mut a = app_with("printf 'visible-line\\n'");
        a.run_full();
        run_and_wait(&mut a);
        a.begin_save();
        let screen = render(&mut a, 100, 30);
        assert!(screen.contains("save output to"), "{screen}");
        // Critically, the prompt does not hide the output it will write.
        assert!(screen.contains("visible-line"), "{screen}");
    }

    #[test]
    fn completion_panel_lists_candidates_without_hiding_output() {
        let dir = std::env::temp_dir().join("toolf-ui-comp");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("shared-a.log"), b"x").unwrap();
        std::fs::write(dir.join("shared-b.log"), b"x").unwrap();

        let mut a = app_with("printf 'kept-line\\n'");
        a.run_full();
        run_and_wait(&mut a);
        a.clear_line();
        a.insert_str(&format!("cat {}/sh", dir.display()));
        a.complete();
        let screen = render(&mut a, 120, 30);
        assert!(screen.contains("paths"), "{screen}");
        assert!(screen.contains("kept-line"), "{screen}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stage_panel_shows_each_command_word() {
        let mut a = app_with("cat f | grep ERROR | wc -l");
        let screen = render(&mut a, 110, 30);
        assert!(screen.contains("stages"), "{screen}");
        assert!(screen.contains("1. cat"), "{screen}");
        assert!(screen.contains("2. grep"), "{screen}");
        assert!(screen.contains("3. wc"), "{screen}");
    }

    #[test]
    fn validation_warning_is_rendered() {
        let mut a = app_with("grep 'unterminated");
        let screen = render(&mut a, 100, 30);
        assert!(screen.contains("unterminated"), "{screen}");
    }

    #[test]
    fn search_matches_are_marked_and_counted() {
        let mut a = app_with("printf 'alpha\\nbeta\\n'");
        a.run_full();
        run_and_wait(&mut a);
        a.begin_search();
        a.prompt_input = "alpha".into();
        a.prompt_confirm();
        let screen = render(&mut a, 100, 30);
        assert!(screen.contains("search: alpha"), "{screen}");
    }

    #[test]
    fn stderr_is_shown_in_the_output_pane() {
        let mut a = app_with("echo boom >&2; exit 1");
        a.run_full();
        run_and_wait(&mut a);
        let screen = render(&mut a, 100, 30);
        assert!(screen.contains("stderr"), "{screen}");
        assert!(screen.contains("boom"), "{screen}");
        assert!(screen.contains("exit 1"), "{screen}");
    }

    #[test]
    fn tiny_terminal_shows_a_hint_instead_of_panicking() {
        let mut a = app_with("cat x");
        let screen = render(&mut a, 30, 8);
        assert!(screen.contains("too small"), "{screen}");
    }

    #[test]
    fn truncate_and_ellipsize_respect_width() {
        assert_eq!(truncate("hello", 3), "hel");
        assert_eq!(truncate("hello", 0), "");
        assert_eq!(truncate("hi", 10), "hi");
        assert!(ellipsize("abcdefghijklmnop", 10).width() <= 10);
        assert_eq!(number_width(5), 3);
        assert_eq!(number_width(12345), 5);
    }
}
