//! Turning a [`Document`] plus a set of highlight ranges into styled terminal
//! rows.
//!
//! The panes scroll by *logical line*, so only the lines that are actually
//! visible get laid out.  That keeps rendering independent of file size while
//! still supporting soft wrapping, tab expansion and double-width characters.

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use unicode_width::UnicodeWidthChar;

use crate::doc::Document;
use crate::theme;

/// What a highlight represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HlKind {
    /// A regex match in the source text.
    Match,
    /// Text inserted by the replacement template.
    Repl,
}

/// A byte range of the rendered document that must stand out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Highlight {
    /// Byte offset of the first byte.
    pub start: usize,
    /// Byte offset one past the last byte; equal to `start` for a zero-width
    /// match, which is drawn as a one-cell caret.
    pub end: usize,
    /// Index of the match this highlight belongs to.
    pub index: usize,
    /// Whether this is the currently selected match.
    pub selected: bool,
    /// Match or replacement.
    pub kind: HlKind,
}

impl Highlight {
    /// The style to paint this highlight with.
    fn style(&self) -> Style {
        match self.kind {
            HlKind::Match => theme::match_style(self.selected),
            HlKind::Repl => theme::repl_style(self.selected),
        }
    }
}

/// Geometry and options for one render pass.
#[derive(Debug, Clone, Copy)]
pub struct ViewParams {
    /// Total pane width in cells, gutter included.
    pub width: u16,
    /// Pane height in rows.
    pub height: u16,
    /// First logical line to draw.
    pub top_line: usize,
    /// Horizontal scroll offset in cells; ignored when wrapping.
    pub hscroll: usize,
    /// Columns per tab stop.
    pub tab_width: u16,
    /// Soft-wrap long lines instead of scrolling horizontally.
    pub wrap: bool,
    /// Draw the line-number gutter.
    pub gutter: bool,
}

/// The rows produced by [`render`], ready to hand to a ratatui `Paragraph`.
#[derive(Debug, Default)]
pub struct Rendered {
    /// One entry per terminal row.
    pub lines: Vec<Line<'static>>,
    /// Last logical line that was (at least partly) drawn.
    pub last_line: usize,
    /// True when the final logical line did not fit completely.
    pub clipped: bool,
    /// Widest logical line, in cells, among those laid out; used to bound
    /// horizontal scrolling.
    pub max_line_width: usize,
}

/// One display cell of a laid-out logical line.
struct Cell {
    text: String,
    width: usize,
    style: Style,
}

/// Number of decimal digits needed for the largest line number.
pub fn gutter_width(line_count: usize) -> u16 {
    let mut digits = 1u16;
    let mut n = line_count.max(1);
    while n >= 10 {
        n /= 10;
        digits += 1;
    }
    digits + 1
}

/// Lay out the visible window of `doc`, painting `hls` on top of it.
///
/// `hls` must be sorted by `start` and, apart from zero-width entries, must not
/// overlap — which is what both regex backends produce.
pub fn render(doc: &Document, hls: &[Highlight], p: &ViewParams) -> Rendered {
    let mut out = Rendered {
        last_line: p.top_line,
        ..Rendered::default()
    };
    if p.width == 0 || p.height == 0 {
        return out;
    }
    let gw = if p.gutter {
        gutter_width(doc.line_count()).min(p.width.saturating_sub(1))
    } else {
        0
    };
    let text_width = p.width.saturating_sub(gw) as usize;
    if text_width == 0 {
        return out;
    }

    let total_lines = doc.line_count();
    let mut line_idx = p.top_line.min(total_lines.saturating_sub(1));
    let tab = p.tab_width.max(1) as usize;

    while out.lines.len() < p.height as usize && line_idx < total_lines {
        let cells = layout_line(doc, hls, line_idx, tab);
        let line_width: usize = cells.iter().map(|c| c.width).sum();
        out.max_line_width = out.max_line_width.max(line_width);

        let chunks = if p.wrap {
            wrap_cells(&cells, text_width)
        } else {
            vec![slice_cells(&cells, p.hscroll, text_width)]
        };

        let room = p.height as usize - out.lines.len();
        out.clipped = chunks.len() > room;
        for (i, chunk) in chunks.into_iter().take(room).enumerate() {
            let mut spans: Vec<Span<'static>> = Vec::with_capacity(chunk.len() + 1);
            if gw > 0 {
                spans.push(gutter_span(line_idx, i > 0, gw));
            }
            spans.extend(chunk);
            out.lines.push(Line::from(spans));
        }
        out.last_line = line_idx;
        line_idx += 1;
    }

    out
}

/// The line-number cell for a row.
fn gutter_span(line_idx: usize, continuation: bool, gw: u16) -> Span<'static> {
    let w = gw.saturating_sub(1) as usize;
    let body = if continuation {
        // A wrapped continuation row: mark it instead of repeating the number.
        format!("{:>w$} ", "↳", w = w)
    } else {
        format!("{:>w$} ", line_idx + 1, w = w)
    };
    Span::styled(
        body,
        Style::default().fg(if continuation {
            theme::DIM
        } else {
            theme::GUTTER
        }),
    )
}

/// Expand one logical line into styled display cells.
fn layout_line(doc: &Document, hls: &[Highlight], line_idx: usize, tab: usize) -> Vec<Cell> {
    let line = doc.line(line_idx).unwrap_or("");
    let line_start = doc.line_start_byte(line_idx);
    let line_end = line_start + line.len();

    // Highlights that touch this line, plus the zero-width ones keyed by the
    // byte offset at which their caret is drawn.
    let (ranged, carets) = split_highlights(hls, line_start, line_end);

    let mut cells: Vec<Cell> = Vec::with_capacity(line.len() + 1);
    let mut col = 0usize;
    let mut cursor = 0usize;

    for (rel, ch) in line.char_indices() {
        let abs = line_start + rel;
        if let Some(h) = carets.iter().find(|h| h.start == abs) {
            cells.push(caret_cell(*h));
            col += 1;
        }
        while cursor < ranged.len() && ranged[cursor].end <= abs {
            cursor += 1;
        }
        let style = ranged
            .get(cursor)
            .filter(|h| h.start <= abs && abs < h.end)
            .map(|h| h.style())
            .unwrap_or_else(theme::text);

        if ch == '\t' {
            let stop = tab - (col % tab);
            cells.push(Cell {
                text: " ".repeat(stop),
                width: stop,
                style,
            });
            col += stop;
        } else if ch == '\r' {
            // A stray CR inside the line: show it rather than let it move the
            // cursor around.
            cells.push(Cell {
                text: "␍".to_string(),
                width: 1,
                style: style.patch(theme::dim()),
            });
            col += 1;
        } else {
            let w = ch.width().unwrap_or(0);
            if w == 0 {
                // Control or combining character: render a placeholder so the
                // column count stays honest.
                cells.push(Cell {
                    text: "·".to_string(),
                    width: 1,
                    style: style.patch(theme::dim()),
                });
                col += 1;
            } else {
                cells.push(Cell {
                    text: ch.to_string(),
                    width: w,
                    style,
                });
                col += w;
            }
        }
    }

    // A caret sitting exactly at end-of-line.
    if let Some(h) = carets.iter().find(|h| h.start == line_end) {
        cells.push(caret_cell(*h));
    }

    // When a match runs past the end of this line (possible with `(?s)`), show
    // the newline it swallowed so the highlight visibly continues.
    if let Some(h) = ranged
        .iter()
        .find(|h| h.start <= line_end && h.end > line_end)
    {
        cells.push(Cell {
            text: "↵".to_string(),
            width: 1,
            style: h.style().remove_modifier(Modifier::BOLD),
        });
    }

    cells
}

/// The one-cell marker drawn for a zero-width match.
fn caret_cell(h: Highlight) -> Cell {
    Cell {
        text: "▏".to_string(),
        width: 1,
        style: Style::default()
            .fg(theme::REPL_FG)
            .bg(if h.selected {
                theme::SEL_MATCH_BG
            } else {
                theme::EMPTY_BG
            })
            .add_modifier(Modifier::BOLD),
    }
}

/// Partition the highlights touching `[line_start, line_end]` into ranged
/// highlights and zero-width carets.
fn split_highlights(
    hls: &[Highlight],
    line_start: usize,
    line_end: usize,
) -> (Vec<Highlight>, Vec<Highlight>) {
    // First highlight that could reach this line.  Sorted by `start`, so
    // partition on `end < line_start` is not valid in general; scan back from
    // the lower bound on `start` instead, which is cheap because matches do not
    // overlap.
    let from = hls.partition_point(|h| h.start < line_start);
    let mut lo = from;
    while lo > 0 && hls[lo - 1].end >= line_start {
        lo -= 1;
    }

    let mut ranged = Vec::new();
    let mut carets = Vec::new();
    for h in &hls[lo..] {
        if h.start > line_end {
            break;
        }
        if h.end < line_start {
            continue;
        }
        if h.start == h.end {
            if h.start >= line_start && h.start <= line_end {
                carets.push(*h);
            }
        } else if h.end > line_start {
            ranged.push(*h);
        }
    }
    (ranged, carets)
}

/// Break cells into rows no wider than `width`.
fn wrap_cells(cells: &[Cell], width: usize) -> Vec<Vec<Span<'static>>> {
    let mut rows: Vec<Vec<Span<'static>>> = Vec::new();
    let mut row: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;

    for c in cells {
        // A single cell wider than the pane still has to go somewhere; give it
        // its own row rather than looping forever.
        if used + c.width > width && used > 0 {
            rows.push(std::mem::take(&mut row));
            used = 0;
        }
        push_cell(&mut row, c);
        used += c.width;
    }
    rows.push(row);
    rows
}

/// Take the horizontal window `[hscroll, hscroll + width)` of `cells`.
fn slice_cells(cells: &[Cell], hscroll: usize, width: usize) -> Vec<Span<'static>> {
    let mut row: Vec<Span<'static>> = Vec::new();
    let mut col = 0usize;
    let mut used = 0usize;
    for c in cells {
        let cell_end = col + c.width;
        if cell_end <= hscroll {
            col = cell_end;
            continue;
        }
        if used + c.width > width {
            break;
        }
        // A double-width character straddling the left edge is replaced by a
        // filler so the remaining columns stay aligned.
        if col < hscroll {
            let visible = cell_end - hscroll;
            row.push(Span::styled("…".repeat(visible.min(width)), c.style));
            used += visible;
        } else {
            push_cell(&mut row, c);
            used += c.width;
        }
        col = cell_end;
    }
    row
}

/// Append a cell to a row, merging it into the previous span when the style is
/// unchanged so that ratatui has fewer spans to draw.
fn push_cell(row: &mut Vec<Span<'static>>, c: &Cell) {
    match row.last_mut() {
        Some(last) if last.style == c.style => {
            let mut s = std::mem::take(&mut last.content).into_owned();
            s.push_str(&c.text);
            last.content = s.into();
        }
        _ => row.push(Span::styled(c.text.clone(), c.style)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(width: u16, height: u16) -> ViewParams {
        ViewParams {
            width,
            height,
            top_line: 0,
            hscroll: 0,
            tab_width: 4,
            wrap: false,
            gutter: false,
        }
    }

    fn plain(l: &Line<'_>) -> String {
        l.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    fn hl(start: usize, end: usize) -> Highlight {
        Highlight {
            start,
            end,
            index: 0,
            selected: false,
            kind: HlKind::Match,
        }
    }

    #[test]
    fn renders_plain_lines() {
        let d = Document::from_text("t", "one\ntwo\nthree");
        let r = render(&d, &[], &params(40, 10));
        assert_eq!(r.lines.len(), 3);
        assert_eq!(plain(&r.lines[0]), "one");
        assert_eq!(plain(&r.lines[2]), "three");
    }

    #[test]
    fn height_limits_rows_and_reports_top() {
        let d = Document::from_text("t", "a\nb\nc\nd");
        let mut p = params(40, 2);
        p.top_line = 1;
        let r = render(&d, &[], &p);
        assert_eq!(r.lines.len(), 2);
        assert_eq!(plain(&r.lines[0]), "b");
        assert_eq!(r.last_line, 2);
    }

    #[test]
    fn match_gets_a_background_colour() {
        let d = Document::from_text("t", "abcdef");
        let r = render(&d, &[hl(2, 4)], &params(40, 4));
        let styles: Vec<_> = r.lines[0]
            .spans
            .iter()
            .map(|s| (s.content.to_string(), s.style.bg))
            .collect();
        assert_eq!(styles[0], ("ab".to_string(), None));
        assert_eq!(styles[1].0, "cd");
        assert_eq!(styles[1].1, Some(theme::MATCH_BG));
        assert_eq!(styles[2], ("ef".to_string(), None));
    }

    #[test]
    fn selected_match_uses_the_selection_background() {
        let d = Document::from_text("t", "abc");
        let mut h = hl(0, 1);
        h.selected = true;
        let r = render(&d, &[h], &params(40, 2));
        assert_eq!(r.lines[0].spans[0].style.bg, Some(theme::SEL_MATCH_BG));
    }

    #[test]
    fn highlight_spanning_two_lines() {
        let d = Document::from_text("t", "ab\ncd");
        // Covers "b\nc".
        let r = render(&d, &[hl(1, 4)], &params(40, 4));
        assert_eq!(plain(&r.lines[0]), "ab↵");
        assert_eq!(r.lines[0].spans[1].style.bg, Some(theme::MATCH_BG));
        assert_eq!(r.lines[1].spans[0].style.bg, Some(theme::MATCH_BG));
        assert_eq!(r.lines[1].spans[0].content.as_ref(), "c");
    }

    #[test]
    fn zero_width_match_draws_a_caret() {
        let d = Document::from_text("t", "ab");
        let r = render(&d, &[hl(1, 1)], &params(40, 2));
        assert_eq!(plain(&r.lines[0]), "a▏b");
        assert_eq!(r.lines[0].spans[1].style.bg, Some(theme::EMPTY_BG));
    }

    #[test]
    fn caret_at_end_of_line() {
        let d = Document::from_text("t", "ab");
        let r = render(&d, &[hl(2, 2)], &params(40, 2));
        assert_eq!(plain(&r.lines[0]), "ab▏");
    }

    #[test]
    fn tabs_expand_to_the_next_stop() {
        let d = Document::from_text("t", "a\tb");
        let r = render(&d, &[], &params(40, 2));
        assert_eq!(plain(&r.lines[0]), "a   b");
    }

    #[test]
    fn tab_inside_a_match_keeps_the_highlight() {
        let d = Document::from_text("t", "a\tb");
        let r = render(&d, &[hl(1, 2)], &params(40, 2));
        let tab_span = r.lines[0]
            .spans
            .iter()
            .find(|s| s.content.as_ref() == "   ")
            .expect("expanded tab");
        assert_eq!(tab_span.style.bg, Some(theme::MATCH_BG));
    }

    #[test]
    fn wrapping_splits_long_lines() {
        let d = Document::from_text("t", "abcdefgh");
        let mut p = params(3, 10);
        p.wrap = true;
        let r = render(&d, &[], &p);
        assert_eq!(plain(&r.lines[0]), "abc");
        assert_eq!(plain(&r.lines[1]), "def");
        assert_eq!(plain(&r.lines[2]), "gh");
    }

    #[test]
    fn wrapping_does_not_split_a_wide_character() {
        let d = Document::from_text("t", "aEnglish-only textb");
        let mut p = params(2, 10);
        p.wrap = true;
        let r = render(&d, &[], &p);
        assert_eq!(plain(&r.lines[0]), "a");
        assert_eq!(plain(&r.lines[1]), "English-only text");
        assert_eq!(plain(&r.lines[2]), "b");
    }

    #[test]
    fn horizontal_scroll_skips_leading_cells() {
        let d = Document::from_text("t", "0123456789");
        let mut p = params(4, 2);
        p.hscroll = 3;
        let r = render(&d, &[], &p);
        assert_eq!(plain(&r.lines[0]), "3456");
    }

    #[test]
    fn horizontal_scroll_keeps_match_styles() {
        let d = Document::from_text("t", "0123456789");
        let mut p = params(4, 2);
        p.hscroll = 2;
        let r = render(&d, &[hl(4, 6)], &p);
        assert_eq!(plain(&r.lines[0]), "2345");
        let styled: String = r.lines[0]
            .spans
            .iter()
            .filter(|s| s.style.bg == Some(theme::MATCH_BG))
            .map(|s| s.content.as_ref())
            .collect();
        assert_eq!(styled, "45");
    }

    #[test]
    fn gutter_shows_line_numbers_and_continuations() {
        let d = Document::from_text("t", "aaaa\nb");
        let mut p = params(6, 6);
        p.gutter = true;
        p.wrap = true;
        let r = render(&d, &[], &p);
        assert_eq!(plain(&r.lines[0]), "1 aaaa");
        assert_eq!(plain(&r.lines[1]), "2 b");
        assert_eq!(gutter_width(9), 2);
        assert_eq!(gutter_width(10), 3);
        assert_eq!(gutter_width(1000), 5);
    }

    #[test]
    fn gutter_marks_wrapped_rows() {
        let d = Document::from_text("t", "abcdef");
        let mut p = params(5, 6);
        p.gutter = true;
        p.wrap = true;
        let r = render(&d, &[], &p);
        assert_eq!(plain(&r.lines[0]), "1 abc");
        assert_eq!(plain(&r.lines[1]), "↳ def");
    }

    #[test]
    fn multibyte_line_widths_are_measured_in_cells() {
        let d = Document::from_text("t", "English-only text");
        let r = render(&d, &[], &params(40, 2));
        assert_eq!(r.max_line_width, 4);
    }

    #[test]
    fn empty_document_renders_one_row() {
        let d = Document::from_text("t", "");
        let r = render(&d, &[], &params(10, 4));
        assert_eq!(r.lines.len(), 1);
        assert_eq!(plain(&r.lines[0]), "");
    }

    #[test]
    fn zero_size_viewport_is_safe() {
        let d = Document::from_text("t", "x");
        assert!(render(&d, &[], &params(0, 5)).lines.is_empty());
        assert!(render(&d, &[], &params(5, 0)).lines.is_empty());
    }

    #[test]
    fn many_matches_on_one_line_all_render() {
        let d = Document::from_text("t", "a a a a");
        let hls: Vec<Highlight> = (0..4).map(|i| hl(i * 2, i * 2 + 1)).collect();
        let r = render(&d, &hls, &params(40, 2));
        let count = r.lines[0]
            .spans
            .iter()
            .filter(|s| s.style.bg == Some(theme::MATCH_BG))
            .count();
        assert_eq!(count, 4);
    }

    #[test]
    fn control_characters_get_a_placeholder() {
        let d = Document::from_text("t", "a\u{7}b");
        let r = render(&d, &[], &params(10, 2));
        assert_eq!(plain(&r.lines[0]), "a·b");
    }
}
