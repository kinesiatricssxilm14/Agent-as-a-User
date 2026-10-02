package main

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
)

// The package list. It renders a window over the filtered slice rather than
// paginating, so ↑/↓ scroll continuously through all ~63k packages and the whole
// list is reachable in one view without page-switching.

// Column widths for the list rows. Name and version get fixed room; the synopsis
// takes whatever is left, which is what makes the list useful at wide sizes
// without breaking at narrow ones.
const (
	markWidth    = 2
	versionWidth = 18
	sectionWidth = 12
	colGap       = 1
	minNameWidth = 12
	maxNameWidth = 34

	// A synopsis column narrower than this shows nothing but an ellipsis, so it
	// is dropped rather than stealing width from the name.
	minSynopsisWidth = 12
)

// listColumns resolves the column widths for a given content width.
type listColumns struct {
	Mark     int
	Name     int
	Version  int
	Section  int
	Synopsis int
}

func computeListColumns(width int) listColumns {
	var c listColumns

	// A pane this narrow cannot carry the state mark and a name; the name wins,
	// because it is the only column that identifies the row.
	if width < markWidth+colGap+minNameWidth {
		c.Name = max(1, width)
		return c
	}
	c.Mark = markWidth

	// Everything after the mark, minus the gaps between the five columns.
	rest := width - c.Mark - 4*colGap
	if rest < minNameWidth {
		// Room for the mark and a name, but not for the wider layout's gaps.
		c.Name = max(1, width-c.Mark-colGap)
		return c
	}

	// The name column scales with the terminal but stays within bounds: package
	// names are rarely longer than ~34 characters.
	c.Name = clamp(rest*35/100, minNameWidth, maxNameWidth)
	rest -= c.Name

	if rest >= versionWidth+4 {
		c.Version = versionWidth
		rest -= c.Version
	}
	if rest >= sectionWidth+minSynopsisWidth {
		c.Section = sectionWidth
		rest -= c.Section
	}
	if rest >= minSynopsisWidth {
		c.Synopsis = rest
	} else {
		// The leftover is too small to be a useful description column, so give
		// it back to the name instead of wasting it.
		c.Name = min(c.Name+rest, maxNameWidth)
	}
	return c
}

// renderList draws the column header plus one row per visible package, padded to
// exactly height lines so the pane never changes size.
func (m *model) renderList(width, height int) string {
	cols := computeListColumns(width)
	view := m.store.visible()

	lines := make([]string, 0, height)
	lines = append(lines, m.renderListHeader(cols, width))

	rows := height - 1
	if rows < 1 {
		return strings.Join(lines, "\n")
	}

	if len(view) == 0 {
		lines = append(lines, m.emptyListMessage(width, rows)...)
		for len(lines) < height {
			lines = append(lines, "")
		}
		return strings.Join(lines[:height], "\n")
	}

	end := min(m.listOffset+rows, len(view))
	for i := m.listOffset; i < end; i++ {
		lines = append(lines, m.renderListRow(view[i], i == m.cursor, cols, width))
	}

	// Pad so the pane height is constant regardless of how many rows exist.
	for len(lines) < height {
		lines = append(lines, "")
	}
	return strings.Join(lines[:height], "\n")
}

func (m *model) renderListHeader(cols listColumns, width int) string {
	header := joinColumns(
		column{"", cols.Mark},
		column{"PACKAGE", cols.Name},
		column{"VERSION", cols.Version},
		column{"SECTION", cols.Section},
		column{"DESCRIPTION", cols.Synopsis},
	)
	return m.styles.DetailField.Render(truncate(header, width))
}

// column is one cell of a list row: its text and the width it must occupy.
type column struct {
	text  string
	width int
}

// joinColumns pads each column to its width and joins the populated ones with a
// single gap. Columns of zero width are dropped entirely, so a narrow terminal
// does not accumulate stray spaces where a column used to be.
func joinColumns(cols ...column) string {
	var parts []string
	for _, c := range cols {
		if c.width <= 0 {
			continue
		}
		parts = append(parts, pad(c.text, c.width))
	}
	return strings.Join(parts, strings.Repeat(" ", colGap))
}

// emptyListMessage explains why the list is empty, which differs by cause: an
// unmatched search, an empty filter, or an index that has not been downloaded.
// The text wraps rather than being truncated, because it names the key that gets
// the user out of the empty state and that hint must survive.
func (m *model) emptyListMessage(width, rows int) []string {
	var msg string
	switch {
	case m.loadingInstalled || m.loadingAvailable:
		msg = "loading package data…"
	case m.store.query != "":
		msg = fmt.Sprintf("no package matches %q — press esc to clear the search", m.store.query)
	case m.store.filter == filterUpgradable:
		msg = "every installed package is up to date"
	case m.store.filter == filterResidual:
		msg = "no packages have leftover configuration files"
	case !m.aptListsPresent:
		msg = "no package indexes — press R to run apt-get update"
	default:
		msg = "no packages to show"
	}

	var out []string
	for _, line := range wrapText(msg, max(1, width-2)) {
		if len(out) >= rows {
			break
		}
		out = append(out, m.styles.ListEmpty.Render("  "+line))
	}
	return out
}

// renderListRow draws one package. The leading mark encodes state compactly:
//
//	●↑  installed, an upgrade is available
//	●   installed and current
//	○   available, not installed
//	◌   removed but configuration files remain
func (m *model) renderListRow(p *pkg, selected bool, cols listColumns, width int) string {
	mark, markStyle := m.stateMark(p)

	nameStyle := m.styles.ListName
	if p.Installed {
		nameStyle = nameStyle.Bold(true)
	}

	var parts []string
	if cols.Mark > 0 {
		parts = append(parts, markStyle.Render(pad(mark, cols.Mark)))
	}
	parts = append(parts, nameStyle.Render(pad(truncate(p.Name, cols.Name), cols.Name)))

	if cols.Version > 0 {
		version := p.DisplayVersion()
		style := m.styles.ListVersion
		// An upgradable package shows where it is going, since that is the
		// number the user is deciding about.
		if p.Upgradable && p.UpgradeTarget != "" {
			version = p.UpgradeTarget
			style = m.styles.MarkUpgradable
		}
		parts = append(parts, style.Render(pad(truncate(version, cols.Version), cols.Version)))
	}
	if cols.Section > 0 {
		parts = append(parts, m.styles.ListSection.Render(
			pad(truncate(p.Section, cols.Section), cols.Section)))
	}
	if cols.Synopsis > 0 {
		parts = append(parts, m.styles.ListSynopsis.Render(
			pad(truncate(p.Synopsis, cols.Synopsis), cols.Synopsis)))
	}

	row := strings.Join(parts, strings.Repeat(" ", colGap))

	if selected {
		// The selected row is padded to the full width first, so the highlight
		// spans the pane rather than stopping at the text.
		plain := lipgloss.NewStyle().Render(row)
		return m.styles.ListRowSelected.Render(padVisible(plain, width))
	}
	return truncate(row, width)
}

// stateMark returns the row's state glyph and its style.
func (m *model) stateMark(p *pkg) (string, lipgloss.Style) {
	switch {
	case p.Installed && p.Upgradable:
		return "●↑", m.styles.MarkUpgradable
	case p.Installed:
		return "●", m.styles.MarkInstalled
	case p.ResidualConfig():
		return "◌", m.styles.MarkResidual
	default:
		return "○", m.styles.MarkAvailable
	}
}

// listScrollInfo describes the visible window, so the user knows where they are
// in a list far taller than the screen.
func (m *model) listScrollInfo() string {
	n := len(m.store.visible())
	if n == 0 {
		return "0/0"
	}
	return fmt.Sprintf("%d/%d", m.cursor+1, n)
}

// ---------------------------------------------------------------------------
// Text helpers
// ---------------------------------------------------------------------------

// truncate shortens s to at most width display cells, marking the cut with "…".
// Width is measured with lipgloss so wide characters and any styling already
// applied are accounted for.
func truncate(s string, width int) string {
	if width <= 0 {
		return ""
	}
	if lipgloss.Width(s) <= width {
		return s
	}
	if width == 1 {
		return "…"
	}

	// Trim runes until the ellipsis fits, which handles multi-cell runes without
	// assuming one rune is one column.
	runes := []rune(s)
	for len(runes) > 0 {
		runes = runes[:len(runes)-1]
		if lipgloss.Width(string(runes))+1 <= width {
			return string(runes) + "…"
		}
	}
	return "…"
}

// pad right-pads s to exactly width display cells, truncating if it is longer.
func pad(s string, width int) string {
	if width <= 0 {
		return ""
	}
	s = truncate(s, width)
	if gap := width - lipgloss.Width(s); gap > 0 {
		return s + strings.Repeat(" ", gap)
	}
	return s
}

// padVisible pads to width without truncating existing style sequences, for text
// that has already been styled.
func padVisible(s string, width int) string {
	if gap := width - lipgloss.Width(s); gap > 0 {
		return s + strings.Repeat(" ", gap)
	}
	return s
}

// wrapText breaks s into lines of at most width cells, preserving existing line
// breaks and never splitting a word unless the word itself is too long. This is
// used for descriptions, which must be fully readable in the details pane.
func wrapText(s string, width int) []string {
	if width <= 0 {
		return nil
	}

	var out []string
	for _, paragraph := range strings.Split(s, "\n") {
		if strings.TrimSpace(paragraph) == "" {
			out = append(out, "")
			continue
		}

		// Preserve the indentation of pre-formatted lines (package descriptions
		// use it for lists and examples).
		indent := paragraph[:len(paragraph)-len(strings.TrimLeft(paragraph, " \t"))]
		if lipgloss.Width(indent) >= width {
			indent = ""
		}

		var line strings.Builder
		line.WriteString(indent)
		lineEmpty := true

		for _, word := range strings.Fields(paragraph) {
			wordWidth := lipgloss.Width(word)

			switch {
			case lineEmpty && wordWidth <= width-lipgloss.Width(indent):
				line.WriteString(word)
				lineEmpty = false
			case !lineEmpty && lipgloss.Width(line.String())+1+wordWidth <= width:
				line.WriteByte(' ')
				line.WriteString(word)
			case wordWidth > width-lipgloss.Width(indent):
				// A single word longer than the pane: flush, then hard-split it
				// so no text is silently lost.
				if !lineEmpty {
					out = append(out, line.String())
					line.Reset()
					line.WriteString(indent)
				}
				for _, chunk := range hardSplit(word, width-lipgloss.Width(indent)) {
					out = append(out, indent+chunk)
				}
				lineEmpty = true
			default:
				out = append(out, line.String())
				line.Reset()
				line.WriteString(indent)
				line.WriteString(word)
				lineEmpty = false
			}
		}
		if !lineEmpty {
			out = append(out, line.String())
		}
	}
	return out
}

// hardSplit breaks an over-long token into width-sized chunks.
func hardSplit(s string, width int) []string {
	if width <= 0 {
		return []string{s}
	}

	var (
		out   []string
		chunk []rune
	)
	for _, r := range s {
		chunk = append(chunk, r)
		if lipgloss.Width(string(chunk)) >= width {
			out = append(out, string(chunk))
			chunk = chunk[:0]
		}
	}
	if len(chunk) > 0 {
		out = append(out, string(chunk))
	}
	return out
}
