package ui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"

	"tooln/internal/pkgmgr"
)

// renderList draws the package list panel: a column header, then one row per
// visible package, then a footer showing the scroll position.
func (m *Model) renderList(width, height int) string {
	t := m.cur()
	inner := max(10, width-4) // 2 border columns + 2 padding columns

	// Content rows available: total height, minus the two border rows, minus the
	// panel title, minus the footer.
	content := max(1, height-2-1-1)

	title := m.listTitle(t, inner)
	body := strings.Join(m.listRows(t, inner, content), "\n") +
		"\n" + m.listFooter(t, inner)

	style := m.styles.panel
	if !m.focusDetails && m.mode != modeConfirm && m.mode != modeMenu {
		style = m.styles.panelFocus
	}
	return renderPanel(style, title, body, width, height)
}

// listTitle names the panel and says what the rows are.
func (m *Model) listTitle(t *tabState, width int) string {
	if t == nil {
		return m.styles.panelTitle.Render("Packages")
	}
	var head string
	if t.source == sourceSearch {
		head = fmt.Sprintf("%s search: %s", t.mgr.ID(), t.query)
	} else {
		head = fmt.Sprintf("%s installed", t.mgr.ID())
	}
	var flags []string
	if t.filter != "" {
		flags = append(flags, "filter="+t.filter)
	}
	if t.installedOnly {
		flags = append(flags, "installed only")
	}
	line := m.styles.panelTitle.Render(head)
	if len(flags) > 0 {
		line += m.styles.dim.Render("  [" + strings.Join(flags, ", ") + "]")
	}
	count := m.styles.dim.Render(fmt.Sprintf("%d/%d", len(t.rows), len(t.all)))
	gap := width - lipgloss.Width(line) - lipgloss.Width(count)
	if gap > 0 {
		line += strings.Repeat(" ", gap) + count
	}
	return truncate(line, width)
}

// Column widths for a list row: the marker, the version and the "newer version"
// column are fixed so names and summaries line up down the list.
const (
	markerWidth  = 2
	versionWidth = 14
	latestWidth  = 12
)

// listRows renders the column header followed by the visible package rows,
// returning exactly `content` lines so the panel keeps a constant height.
func (m *Model) listRows(t *tabState, width, content int) []string {
	// One line goes to the column header; the rest are package rows.
	rows := max(1, content-1)

	fill := func(lines []string) []string {
		return wrapLines(lines, width, content)
	}

	if t == nil {
		return fill([]string{m.styles.dim.Render("No package manager configured.")})
	}
	if !t.available() {
		reason := t.probeErr
		if t.probe == probePending {
			reason = "still checking whether this manager can be used here…"
		}
		return fill([]string{
			m.styles.statusErr.Render(t.mgr.ID() + " cannot be used here."),
			"",
			reason,
			"",
			"Switch managers with tab, or press q to quit.",
		})
	}
	if !t.loaded {
		return fill([]string{m.spin.View() + " Reading the " + t.mgr.ID() + " environment…"})
	}
	if t.loadErr != "" {
		return fill([]string{
			m.styles.statusErr.Render("Could not read the package list:"),
			t.loadErr,
			"",
			"Press r to try again.",
		})
	}
	if len(t.rows) == 0 {
		msg := []string{"Nothing to show."}
		switch {
		case t.filter != "":
			msg = []string{
				fmt.Sprintf("No package matches the filter %q.", t.filter),
				"", "Press esc to clear the filter.",
			}
		case t.installedOnly:
			msg = []string{
				"None of these results is installed.",
				"", "Press a to show every result again.",
			}
		case t.source == sourceSearch:
			msg = []string{fmt.Sprintf("The search for %q returned nothing.", t.query)}
		}
		return fill(prependStyle(m.styles.dim, msg))
	}

	// The name column takes what is left after the fixed columns; the summary
	// gets the remainder, and is dropped entirely on a narrow terminal.
	nameW := clamp(width/3, 12, 34)
	summaryW := width - markerWidth - nameW - versionWidth - latestWidth - 3
	if summaryW < 8 {
		summaryW = 0
		nameW = width - markerWidth - versionWidth - latestWidth - 2
	}

	out := make([]string, 0, content)
	out = append(out, m.columnHeader(width, nameW, summaryW))

	t.ensureVisible(rows)
	end := min(t.top+rows, len(t.rows))
	for i := t.top; i < end; i++ {
		out = append(out, m.renderRow(t, i, width, nameW, summaryW))
	}
	// Pad to a constant height so the panel border does not move as the list
	// shortens.
	for len(out) < content {
		out = append(out, "")
	}
	return out[:content]
}

func (m *Model) columnHeader(width, nameW, summaryW int) string {
	parts := []string{
		pad("", markerWidth),
		pad("PACKAGE", nameW),
		pad("VERSION", versionWidth),
		pad("NEWER", latestWidth),
	}
	if summaryW > 0 {
		parts = append(parts, pad("DESCRIPTION", summaryW))
	}
	return m.styles.listHeader.Render(pad(strings.Join(parts, " "), width))
}

func (m *Model) renderRow(t *tabState, idx, width, nameW, summaryW int) string {
	p := t.all[t.rows[idx]]
	selected := idx == t.cursor

	marker := " "
	switch {
	case t.marks[p.Name]:
		marker = "✓"
	case selected:
		marker = "›"
	}

	name := ellipsize(p.Name, nameW)
	version := ellipsize(orDash(p.Version), versionWidth)
	latest := ellipsize(p.Latest, latestWidth)

	// A search result that is not installed says so where the version would be,
	// so installed and available packages are never confused.
	if !p.Installed && p.Version != "" {
		version = ellipsize(p.Version+" ↓", versionWidth)
	} else if !p.Installed {
		version = ellipsize("(available)", versionWidth)
	}

	cells := []string{
		pad(marker, markerWidth),
		pad(m.rowNameStyle(p, selected).Render(name), nameW),
		pad(m.rowVersionStyle(p).Render(version), versionWidth),
		pad(m.styles.latest.Render(latest), latestWidth),
	}
	if summaryW > 0 {
		summary := p.Summary
		if summary == "" && p.Note != "" {
			summary = p.Note
		} else if p.Note != "" && p.Note != summary {
			summary = p.Note + " · " + summary
		}
		cells = append(cells, pad(m.styles.summary.Render(ellipsize(summary, summaryW)), summaryW))
	}

	line := pad(strings.Join(cells, " "), width)
	if t.marks[p.Name] {
		line = strings.Replace(line, "✓", m.styles.marked.Render("✓"), 1)
	}
	if selected {
		return m.styles.rowSel.Render(line)
	}
	return line
}

func (m *Model) rowNameStyle(p pkgmgr.Package, selected bool) lipgloss.Style {
	switch {
	case selected:
		return m.styles.rowCursor
	case !p.Installed:
		return m.styles.dim
	default:
		return m.styles.name
	}
}

func (m *Model) rowVersionStyle(p pkgmgr.Package) lipgloss.Style {
	if !p.Installed {
		return m.styles.dim
	}
	return m.styles.version
}

// listFooter shows the cursor position and where the window sits in the list.
func (m *Model) listFooter(t *tabState, width int) string {
	if t == nil || len(t.rows) == 0 {
		return m.styles.dim.Render(pad("", width))
	}
	pos := fmt.Sprintf("%d of %d", t.cursor+1, len(t.rows))
	var scroll string
	switch {
	case len(t.rows) <= m.listHeight():
		scroll = "all shown"
	case t.top == 0:
		scroll = "top ↓"
	case t.top+m.listHeight() >= len(t.rows):
		scroll = "↑ bottom"
	default:
		scroll = fmt.Sprintf("↕ %d%%", 100*t.top/max(1, len(t.rows)-m.listHeight()))
	}
	left := m.styles.dim.Render(pos)
	right := m.styles.dim.Render(scroll)
	gap := width - lipgloss.Width(left) - lipgloss.Width(right)
	if gap < 1 {
		return truncate(left, width)
	}
	return left + strings.Repeat(" ", gap) + right
}

// prependStyle applies a style to each line.
func prependStyle(s lipgloss.Style, lines []string) []string {
	out := make([]string, 0, len(lines))
	for _, l := range lines {
		if l == "" {
			out = append(out, "")
			continue
		}
		out = append(out, s.Render(l))
	}
	return out
}

// wrapLines hard-wraps text to width and pads the result to exactly rows lines.
func wrapLines(lines []string, width, rows int) []string {
	var out []string
	for _, l := range lines {
		if l == "" {
			out = append(out, "")
			continue
		}
		out = append(out, strings.Split(wrapText(l, width), "\n")...)
	}
	for len(out) < rows {
		out = append(out, "")
	}
	if len(out) > rows {
		out = out[:rows]
	}
	return out
}
