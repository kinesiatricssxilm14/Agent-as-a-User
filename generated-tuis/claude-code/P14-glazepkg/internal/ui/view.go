package ui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
)

// Vertical budget. The layout is fixed-height by design: every pane's size is
// computed once per resize so the whole interface is on one screen and nothing
// important is hidden behind a mode change.
const (
	headerHeight = 2 // title line + tab bar
	statusHeight = 2 // status line + short help
	promptHeight = 1 // input line, when a text mode is active
	minListRows  = 3
)

// layout recomputes every pane size for the current terminal dimensions.
func (m *Model) layout() {
	if !m.ready {
		return
	}
	bodyH := m.bodyHeight()
	_, detailW := m.paneWidths()

	// A viewport sits inside a bordered, padded panel below its title row, so it
	// gets the panel height minus the two border rows and the title, and the
	// panel width minus two border and two padding columns.
	innerH := max(1, bodyH-3)
	innerW := max(10, detailW-4)
	for _, t := range m.tabs {
		t.vp.Width = innerW
		t.vp.Height = innerH
	}
	m.helpVP.Width = max(10, m.width-4)
	m.helpVP.Height = max(1, m.height-headerHeight-statusHeight-3)
	m.logVP.Width = max(10, m.width-4)
	m.logVP.Height = max(1, m.logHeight()-3)

	m.input.Width = max(10, m.width-24)

	if t := m.cur(); t != nil {
		t.ensureVisible(m.listHeight())
	}
	m.refreshDetailViewport()
	if m.showLog {
		m.renderLog()
	}
	if m.showHelp {
		m.renderHelp()
	}
}

// paneWidths splits the body between the package list and the details pane.
func (m *Model) paneWidths() (list, detail int) {
	if m.width < 84 {
		// On a narrow terminal the details pane would be unreadable; give the
		// list the room and let the details pane keep a usable minimum.
		list = max(28, m.width*11/20)
	} else {
		list = m.width * 6 / 10
	}
	detail = m.width - list
	if detail < 30 {
		detail = min(30, m.width/2)
		list = m.width - detail
	}
	return list, detail
}

// bodyHeight is the height available to the list and details panes.
func (m *Model) bodyHeight() int {
	h := m.height - headerHeight - statusHeight
	if m.mode == modeFilter || m.mode == modeSearch || m.mode == modeInstall {
		h -= promptHeight
	}
	if m.showLog {
		h -= m.logHeight()
	}
	return max(minListRows+3, h)
}

// logHeight is the space the command log takes when visible.
func (m *Model) logHeight() int {
	if !m.showLog {
		return 0
	}
	// A third of the screen, clamped so neither pane becomes useless.
	h := m.height / 3
	return clamp(h, 6, max(6, m.height-headerHeight-statusHeight-minListRows-4))
}

// listHeight is the number of package rows the list panel can show. It must
// agree with renderList's arithmetic, since it is what the cursor and the
// scroll window are clamped against: the panel height, minus two border rows,
// the panel title, the column header, and the footer.
func (m *Model) listHeight() int {
	return max(1, m.bodyHeight()-5)
}

// ------------------------------------------------------------------- view ----

func (m *Model) View() string {
	if !m.ready {
		return "Starting tooln…"
	}
	if m.width < 40 || m.height < 12 {
		return fmt.Sprintf("tooln needs a terminal of at least 40x12; this one is %dx%d.",
			m.width, m.height)
	}

	var sections []string
	sections = append(sections, m.renderHeader())

	// Confirmations and the maintenance menu are drawn into the details region
	// rather than as a floating window, so the package list and the commands
	// about to run are readable in the same screen.
	if m.showHelp {
		sections = append(sections, m.renderHelpPanel())
	} else {
		sections = append(sections, m.renderBody())
		if m.showLog {
			sections = append(sections, m.renderLogPanel())
		}
	}
	if m.mode == modeFilter || m.mode == modeSearch || m.mode == modeInstall {
		sections = append(sections, m.renderPrompt())
	}
	sections = append(sections, m.renderStatus(), m.renderShortHelp())

	// Clip to the terminal height. The status line and the key reminder are the
	// last rows, so anything trimmed comes off a pane rather than off them —
	// but a stray extra line would scroll the header away, so enforce the total.
	return clipHeight(strings.Join(sections, "\n"), m.height)
}

func (m *Model) renderHeader() string {
	title := m.styles.title.Render("tooln")
	sub := m.styles.dim.Render(" · Python & system package manager")

	var tabs []string
	for i, t := range m.tabs {
		label := t.mgr.Label()
		switch {
		case !t.available() && t.probeErr != "":
			label += " (unavailable)"
		case t.loaded:
			label += fmt.Sprintf(" %d", len(t.all))
		}
		label = fmt.Sprintf(" %d %s ", i+1, label)
		if i == m.active {
			tabs = append(tabs, m.styles.tabActive.Render(label))
			continue
		}
		tabs = append(tabs, m.styles.tabIdle.Render(label))
	}
	tabBar := lipgloss.JoinHorizontal(lipgloss.Top, tabs...)

	right := m.styles.dim.Render("tab switches · ? help")
	gap := m.width - lipgloss.Width(tabBar) - lipgloss.Width(right)
	if gap < 1 {
		gap = 1
		right = ""
	}
	line2 := tabBar + strings.Repeat(" ", gap) + right

	return truncate(title+sub, m.width) + "\n" + truncate(line2, m.width)
}

// renderBody joins the package list and the right-hand pane side by side.
func (m *Model) renderBody() string {
	listW, detailW := m.paneWidths()
	bodyH := m.bodyHeight()

	left := m.renderList(listW, bodyH)
	right := m.renderRightPane(detailW, bodyH)
	return lipgloss.JoinHorizontal(lipgloss.Top, left, right)
}

// renderRightPane shows a confirmation or the maintenance menu when one is
// pending, and the package details the rest of the time.
func (m *Model) renderRightPane(width, height int) string {
	switch m.mode {
	case modeConfirm:
		return m.renderConfirmPanel(width, height)
	case modeMenu:
		return m.renderMenuPanel(width, height)
	default:
		return m.renderDetails(width, height)
	}
}

func (m *Model) renderPrompt() string {
	var label string
	switch m.mode {
	case modeFilter:
		label = "Filter:"
	case modeSearch:
		if t := m.cur(); t != nil {
			label = "Search " + t.mgr.ID() + ":"
		} else {
			label = "Search:"
		}
	case modeInstall:
		if t := m.cur(); t != nil {
			label = "Install with " + t.mgr.ID() + ":"
		} else {
			label = "Install:"
		}
	}
	hint := m.styles.dim.Render("  enter = confirm · esc = cancel")
	line := m.styles.prompt.Render(label) + " " + m.input.View()
	if lipgloss.Width(line)+lipgloss.Width(hint) <= m.width {
		line += hint
	}
	return truncate(line, m.width)
}

func (m *Model) renderStatus() string {
	style := m.styles.statusPlain
	prefix := ""
	switch m.statusLevel {
	case statusGood:
		style, prefix = m.styles.statusOK, "✓ "
	case statusWarning:
		style, prefix = m.styles.statusWarn, "! "
	case statusBad:
		style, prefix = m.styles.statusErr, "✗ "
	case statusBusy:
		style = m.styles.statusBusy
		prefix = m.spin.View() + " "
	}

	text := prefix + m.status
	if m.busy > 0 && m.statusLevel != statusBusy {
		text = m.spin.View() + " " + m.busyLabel + " · " + m.status
	}

	left := style.Render(text)
	// A marks counter on the right makes multi-select state impossible to miss.
	right := ""
	if t := m.cur(); t != nil && len(t.marks) > 0 {
		right = m.styles.marked.Render(fmt.Sprintf(" %d marked ", len(t.marks)))
	}
	gap := m.width - lipgloss.Width(left) - lipgloss.Width(right)
	if gap < 1 {
		return truncate(left, m.width)
	}
	return left + strings.Repeat(" ", gap) + right
}

// renderShortHelp is the always-present key reminder, so the basic operations
// are discoverable without opening the help screen.
func (m *Model) renderShortHelp() string {
	// Each entry carries a priority: 0 must always survive, higher numbers are
	// dropped first when the line will not fit. Truncating from the end would
	// lose "? help" and "q quit" — exactly the two a lost user needs most.
	type entry struct {
		key, desc string
		prio      int
	}
	var entries []entry
	add := func(prio int, k, d string) { entries = append(entries, entry{k, d, prio}) }

	switch m.mode {
	case modeFilter, modeSearch, modeInstall:
		add(0, "enter", "confirm")
		add(0, "esc", "cancel")
		add(1, "←/→", "edit")
		add(1, "ctrl+c", "quit")
	case modeConfirm:
		add(0, "enter/y", "yes, do it")
		add(0, "esc/n", "no, cancel")
		add(1, "L", "command log")
		add(2, "ctrl+c", "quit")
	case modeMenu:
		add(0, "↑/↓", "choose")
		add(0, "enter", "run")
		add(0, "esc", "close")
	default:
		if m.showHelp {
			add(0, "↑/↓", "scroll")
			add(0, "?/esc", "close help")
			add(0, "q", "quit")
			break
		}
		add(0, "↑/↓", "move")
		add(0, "tab", "pip/apt")
		add(1, "f", "filter")
		add(1, "s", "search")
		add(1, "i", "install")
		add(1, "d", "uninstall")
		add(2, "U", "upgrade")
		add(3, "space", "mark")
		add(3, "o", "updates")
		add(2, "r", "refresh")
		add(3, "L", "log")
		add(0, "?", "help")
		add(0, "q", "quit")
	}

	render := func(es []entry) string {
		parts := make([]string, 0, len(es))
		for _, e := range es {
			parts = append(parts, m.styles.helpKey.Render(e.key)+" "+m.styles.helpDesc.Render(e.desc))
		}
		return strings.Join(parts, m.styles.dim.Render(" · "))
	}

	// Drop the lowest-priority entries until the line fits, keeping the original
	// order of whatever survives.
	kept := entries
	for prio := 3; prio >= 1; prio-- {
		if lipgloss.Width(render(kept)) <= m.width {
			break
		}
		var next []entry
		for _, e := range kept {
			if e.prio < prio {
				next = append(next, e)
			}
		}
		kept = next
	}
	return truncate(render(kept), m.width)
}

// ------------------------------------------------------------------ helpers ----

func truncate(s string, width int) string {
	if width <= 0 {
		return ""
	}
	if lipgloss.Width(s) <= width {
		return s
	}
	return lipgloss.NewStyle().MaxWidth(width).Render(s)
}

// pad extends s with spaces to exactly width visible columns.
func pad(s string, width int) string {
	w := lipgloss.Width(s)
	if w >= width {
		return truncate(s, width)
	}
	return s + strings.Repeat(" ", width-w)
}

func clamp(v, lo, hi int) int {
	if hi < lo {
		hi = lo
	}
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}

// ellipsize shortens text to width columns, marking the cut with a character so
// the reader knows something was removed.
func ellipsize(s string, width int) string {
	if width <= 0 {
		return ""
	}
	if lipgloss.Width(s) <= width {
		return s
	}
	if width == 1 {
		return "…"
	}
	return lipgloss.NewStyle().MaxWidth(width-1).Render(s) + "…"
}

// clipHeight forces a rendered block to exactly rows lines, padding short output
// and truncating long output.
//
// lipgloss's Height() is a *minimum*: a block whose content exceeds it grows
// instead of being cut. Without this the whole interface slides off the top of
// the terminal whenever a panel's content is a line or two too tall, so every
// panel's final render goes through here.
func clipHeight(block string, rows int) string {
	if rows <= 0 {
		return ""
	}
	lines := strings.Split(block, "\n")
	if len(lines) > rows {
		lines = lines[:rows]
	}
	for len(lines) < rows {
		lines = append(lines, "")
	}
	return strings.Join(lines, "\n")
}

// clipBody limits the lines of a panel's *content* so the panel's border and
// title are never the thing that gets cut. inner is the number of content lines
// the panel has room for.
func clipBody(body string, inner int) string {
	if inner <= 0 {
		return ""
	}
	lines := strings.Split(body, "\n")
	if len(lines) > inner {
		lines = lines[:inner]
	}
	return strings.Join(lines, "\n")
}
