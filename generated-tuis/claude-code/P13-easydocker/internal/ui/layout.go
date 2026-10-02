package ui

import (
	"strings"

	"github.com/charmbracelet/lipgloss"
	"github.com/mattn/go-runewidth"
)

// dispWidth returns the terminal cell width of s.
func dispWidth(s string) int { return runewidth.StringWidth(s) }

// truncate shortens s to at most w display cells, appending an ellipsis when
// characters had to be dropped.
func truncate(s string, w int) string {
	if w <= 0 {
		return ""
	}
	if dispWidth(s) <= w {
		return s
	}
	if w == 1 {
		return "…"
	}
	return runewidth.Truncate(s, w, "…")
}

// pad right-pads s with spaces to exactly w display cells, truncating when it
// is too long. The result always occupies w cells, which keeps columns aligned.
func pad(s string, w int) string {
	if w <= 0 {
		return ""
	}
	s = truncate(s, w)
	if diff := w - dispWidth(s); diff > 0 {
		return s + strings.Repeat(" ", diff)
	}
	return s
}

// wrap breaks s into lines of at most w display cells, preferring to break at
// spaces. Words longer than the limit are split hard so nothing is lost.
func wrap(s string, w int) []string {
	if w <= 0 {
		return []string{s}
	}
	if s == "" {
		return []string{""}
	}
	var out []string
	for _, paragraph := range strings.Split(s, "\n") {
		if paragraph == "" {
			out = append(out, "")
			continue
		}
		line := ""
		for _, word := range strings.Fields(paragraph) {
			switch {
			case line == "":
				line = word
			case dispWidth(line)+1+dispWidth(word) <= w:
				line += " " + word
			default:
				out = append(out, line)
				line = word
			}
			// Hard-split an over-long token.
			for dispWidth(line) > w {
				cut := runewidth.Truncate(line, w, "")
				out = append(out, cut)
				line = strings.TrimPrefix(line, cut)
			}
		}
		if line != "" {
			out = append(out, line)
		}
	}
	if len(out) == 0 {
		out = append(out, "")
	}
	return out
}

// column describes one column of a list table.
type column struct {
	title string
	// width is the resolved width in cells; computed by layoutColumns.
	width int
	// weight controls how leftover space is distributed. Columns with weight
	// 0 keep their natural width.
	weight int
	// min is the smallest acceptable width for the column.
	min int
	// right right-aligns the cell content (used for sizes).
	right bool
}

// layoutColumns resolves column widths so the row fits exactly in total cells.
// natural holds the widest cell content per column (including the header).
func layoutColumns(cols []column, natural []int, total, gap int) []column {
	out := make([]column, len(cols))
	copy(out, cols)
	if len(out) == 0 {
		return out
	}

	gaps := gap * (len(out) - 1)
	avail := total - gaps
	if avail < len(out) {
		avail = len(out)
	}

	sum := 0
	for i := range out {
		w := natural[i]
		if w < out[i].min {
			w = out[i].min
		}
		if hw := dispWidth(out[i].title); w < hw {
			w = hw
		}
		out[i].width = w
		sum += w
	}

	switch {
	case sum < avail:
		// Grow weighted columns to consume the remaining space.
		extra := avail - sum
		totalWeight := 0
		for _, c := range out {
			totalWeight += c.weight
		}
		if totalWeight == 0 {
			// Nothing wants to grow: give the slack to the last column so the
			// row still spans the full width.
			out[len(out)-1].width += extra
			break
		}
		given := 0
		last := -1
		for i := range out {
			if out[i].weight == 0 {
				continue
			}
			share := extra * out[i].weight / totalWeight
			out[i].width += share
			given += share
			last = i
		}
		if last >= 0 && given < extra {
			out[last].width += extra - given
		}
	case sum > avail:
		// Shrink the widest shrinkable columns until the row fits.
		over := sum - avail
		for over > 0 {
			victim, victimWidth := -1, 0
			for i := range out {
				slack := out[i].width - out[i].min
				if slack > 0 && out[i].width > victimWidth {
					victim, victimWidth = i, out[i].width
				}
			}
			if victim < 0 {
				break
			}
			cut := out[victim].width - out[victim].min
			if cut > over {
				cut = over
			}
			out[victim].width -= cut
			over -= cut
		}
	}
	return out
}

// renderRow joins cells according to the resolved column widths.
func renderRow(cols []column, cells []string, gap int) string {
	var b strings.Builder
	sep := strings.Repeat(" ", gap)
	for i, c := range cols {
		if i > 0 {
			b.WriteString(sep)
		}
		cell := ""
		if i < len(cells) {
			cell = cells[i]
		}
		if c.right {
			cell = truncate(cell, c.width)
			if diff := c.width - dispWidth(cell); diff > 0 {
				cell = strings.Repeat(" ", diff) + cell
			}
			b.WriteString(cell)
			continue
		}
		b.WriteString(pad(cell, c.width))
	}
	return b.String()
}

// naturalWidths measures the widest cell per column across all rows.
func naturalWidths(cols []column, rows [][]string) []int {
	widths := make([]int, len(cols))
	for i, c := range cols {
		widths[i] = dispWidth(c.title)
	}
	for _, r := range rows {
		for i := range cols {
			if i >= len(r) {
				continue
			}
			if w := dispWidth(r[i]); w > widths[i] {
				widths[i] = w
			}
		}
	}
	return widths
}

// scrollbar renders a vertical indicator of height rows describing a viewport
// of size visible starting at offset within total items.
func scrollbar(total, visible, offset, height int) []string {
	if height <= 0 {
		return nil
	}
	out := make([]string, height)
	if total <= visible || total == 0 {
		for i := range out {
			out[i] = "│"
		}
		return out
	}
	thumb := height * visible / total
	if thumb < 1 {
		thumb = 1
	}
	maxOffset := total - visible
	pos := 0
	if maxOffset > 0 {
		pos = (height - thumb) * offset / maxOffset
	}
	for i := range out {
		if i >= pos && i < pos+thumb {
			out[i] = "█"
		} else {
			out[i] = "│"
		}
	}
	return out
}

// clamp constrains v to [lo, hi].
func clamp(v, lo, hi int) int {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}

// fitStyled pads or truncates a string that already contains ANSI styling to
// exactly w visible cells. Plain pad/truncate cannot be used on styled text
// because they would count escape sequences as printable width.
func fitStyled(s string, w int) string {
	if w <= 0 {
		return ""
	}
	if lipgloss.Width(s) > w {
		s = lipgloss.NewStyle().MaxWidth(w).Render(s)
	}
	if diff := w - lipgloss.Width(s); diff > 0 {
		return s + strings.Repeat(" ", diff)
	}
	return s
}

// fitLines pads or trims lines to exactly n entries so panels keep a stable
// height regardless of content.
func fitLines(lines []string, n int) []string {
	if n <= 0 {
		return nil
	}
	if len(lines) > n {
		return lines[:n]
	}
	for len(lines) < n {
		lines = append(lines, "")
	}
	return lines
}

// joinHorizontalTop is a small wrapper for readability at call sites.
func joinHorizontalTop(parts ...string) string {
	return lipgloss.JoinHorizontal(lipgloss.Top, parts...)
}

// highlightMatches wraps every case-insensitive occurrence of query inside s
// with the given style. It operates on runes so multi-byte text is safe.
func highlightMatches(s, query string, style lipgloss.Style) string {
	if query == "" || s == "" {
		return s
	}
	lowerS := strings.ToLower(s)
	lowerQ := strings.ToLower(query)
	if !strings.Contains(lowerS, lowerQ) {
		return s
	}
	var b strings.Builder
	for {
		i := strings.Index(lowerS, lowerQ)
		if i < 0 {
			b.WriteString(s)
			break
		}
		b.WriteString(s[:i])
		b.WriteString(style.Render(s[i : i+len(query)]))
		s = s[i+len(query):]
		lowerS = lowerS[i+len(query):]
	}
	return b.String()
}
