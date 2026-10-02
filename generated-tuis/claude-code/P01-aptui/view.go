package main

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
)

// The single view. Every pane is rendered at the exact size computeLayout
// resolved and stacked vertically, so the whole interface is one screen snapshot:
// the package list, the details for the selected package, the live apt output,
// the status line and the key help are all visible simultaneously. Nothing is
// ever drawn over anything else — the help expands the footer and shrinks the
// list rather than overlaying it, and confirmations appear inline in the status
// line.

func (m *model) View() string {
	// Bubble Tea calls View once before the first WindowSizeMsg arrives.
	if !m.ready || m.width <= 0 || m.height <= 0 {
		return "starting toola…"
	}

	if m.layout.Degraded && (m.width < minWidth || m.height < minHeight) {
		return m.renderTooSmall()
	}

	// Fold any apt output that arrived since the last frame into the log pane.
	// Doing it once per render rather than once per line keeps a noisy apt run
	// from making redraws quadratic in the amount of output.
	m.syncLogView()

	sections := []string{
		m.renderHeader(),
		m.renderSearch(),
		m.renderBody(),
	}
	if m.layout.LogHeight > 0 {
		sections = append(sections, m.renderLogPane())
	}
	sections = append(sections,
		m.renderStatus(),
		m.renderFooter(),
	)

	view := strings.Join(sections, "\n")

	// Guarantee the rendered height never exceeds the terminal: a pane that
	// grew unexpectedly would otherwise scroll the top of the interface away.
	lines := strings.Split(view, "\n")
	if len(lines) > m.height {
		lines = lines[:m.height]
	}
	return strings.Join(lines, "\n")
}

// renderTooSmall explains the problem instead of drawing a broken interface.
func (m *model) renderTooSmall() string {
	msg := fmt.Sprintf("terminal too small\n\ntoola needs at least %d×%d\ncurrent size: %d×%d\n\nq to quit",
		minWidth, minHeight, m.width, m.height)

	rendered := lipgloss.NewStyle().
		Width(m.width).
		Height(m.height).
		Align(lipgloss.Center, lipgloss.Center).
		Render(msg)

	// Height() pads but does not truncate, so a message taller than the
	// terminal would still overflow. Trim it explicitly.
	lines := strings.Split(rendered, "\n")
	if len(lines) > m.height {
		lines = lines[:m.height]
	}
	return strings.Join(lines, "\n")
}

// renderHeader is the title bar: the tool name, the totals for the whole system,
// and the active filter. The counts describe the store rather than the filtered
// view, so they stay meaningful while searching.
func (m *model) renderHeader() string {
	title := m.styles.HeaderTitle.Render("toola")

	counts := fmt.Sprintf("%d installed · %d available · %d upgradable",
		m.store.countInstalled, m.store.countAvailable, m.store.countUpgradable)
	if m.store.countResidual > 0 {
		counts += fmt.Sprintf(" · %d residual", m.store.countResidual)
	}

	right := "filter: " + m.store.filter.String()
	if m.busy {
		right = m.spin.View() + " " + m.current.Verb() + " · " + right
	} else if m.loading() {
		right = m.spin.View() + " loading · " + right
	}

	left := title + m.styles.HeaderCount.Render("  "+counts)

	// A missing package index is the one condition that makes most of the tool
	// useless, so it is called out in the header rather than only the status line.
	if m.aptListsChecked && !m.aptListsPresent {
		left += m.styles.HeaderWarn.Render("  ⚠ no package index (R to update)")
	}

	return m.joinEnds(left, m.styles.HeaderCount.Render(right), m.width)
}

// loading reports whether any initial scan is still running.
func (m *model) loading() bool {
	return m.loadingInstalled || m.loadingAvailable || m.loadingUpgradable
}

// renderSearch is the search line. It always shows the current query, so an
// active filter can never be forgotten about, and reports how many packages match.
func (m *model) renderSearch() string {
	label := "Search: "
	style := m.styles.SearchLabel
	if !m.searchFocus {
		style = m.styles.DetailField
	}

	left := style.Render(label) + m.search.View()

	shown := len(m.store.visible())
	right := m.styles.SearchCount.Render(fmt.Sprintf("%d shown · %s", shown, m.listScrollInfo()))

	return m.joinEnds(left, right, m.width)
}

// renderBody is the middle row: the package list and the details pane side by
// side, both always populated and both the same height.
func (m *model) renderBody() string {
	listContent := m.renderList(m.layout.ListWidth, m.layout.BodyHeight)

	// The details content goes through a viewport so a long dependency list can
	// be scrolled without the pane changing size or hiding the list.
	m.details.SetContent(m.renderDetails(m.layout.DetailsWidth))
	detailsContent := m.detailsPaneContent()

	listPane := m.styles.pane(m.focus == focusList).
		Width(m.layout.ListWidth).
		Height(m.layout.BodyHeight).
		Render(listContent)

	detailsPane := m.styles.pane(m.focus == focusDetails).
		Width(m.layout.DetailsWidth).
		Height(m.layout.BodyHeight).
		Render(detailsContent)

	return lipgloss.JoinHorizontal(lipgloss.Top, listPane, detailsPane)
}

// detailsPaneContent renders the details title row plus the scrolled viewport.
func (m *model) detailsPaneContent() string {
	name := "details"
	if p := m.selected(); p != nil {
		name = p.Name
	}

	title := m.styles.PaneTitle.Render(truncate(name, m.layout.DetailsWidth-8))
	if info := m.detailsScrollInfo(); info != "" {
		title = m.joinEnds(title, m.styles.ScrollInfo.Render("↕ "+info), m.layout.DetailsWidth)
	}

	return title + "\n" + m.details.View()
}

// renderLogPane is the live apt output. It grows while an operation runs so the
// real command output is prominent, and shrinks back when idle.
func (m *model) renderLogPane() string {
	title := "apt output"
	if m.busy {
		title = m.spin.View() + " " + m.current.CommandLine()
	} else if len(m.logs) == 0 {
		title = "apt output — every operation is a real apt-get command; its output appears here"
	}

	header := m.styles.PaneTitle.Render(truncate(title, m.width-12))
	if m.logView.TotalLineCount() > m.logView.Height {
		header = m.joinEnds(header,
			m.styles.ScrollInfo.Render(fmt.Sprintf("↕ %d%%", int(m.logView.ScrollPercent()*100))),
			m.width-paneFrameWidth)
	}

	return m.styles.pane(m.focus == focusLog).
		Width(m.width - paneFrameWidth).
		Height(m.layout.LogHeight).
		Render(header + "\n" + m.logView.View())
}

// logContent renders the retained apt output. The viewport holds this, so old
// output stays scrollable while new lines arrive.
func (m *model) logContent() string {
	if len(m.logs) == 0 {
		return m.styles.ListEmpty.Render("no operations run yet")
	}

	var b strings.Builder
	for i, l := range m.logs {
		if i > 0 {
			b.WriteByte('\n')
		}
		style := m.styles.LogLine
		switch {
		case l.cmd:
			style = m.styles.LogCmd
		case l.stderr:
			style = m.styles.LogStderr
		}
		b.WriteString(style.Render(truncate(l.text, m.width-paneFrameWidth)))
	}
	return b.String()
}

// renderStatus is the status line, which doubles as the confirmation prompt.
// Confirmations are inline here rather than in a modal, so asking the user to
// confirm never hides the package list, the details or the log.
func (m *model) renderStatus() string {
	if m.confirm != nil {
		prompt := m.styles.StatusConfirm.Render(m.confirm.prompt)
		if m.confirm.warning != "" {
			prompt += m.styles.StatusWarn.Render("  ⚠ " + m.confirm.warning)
		}
		answer := m.styles.StatusConfirm.Render("[y/n]")
		return m.joinEnds(truncate(prompt, m.width-8), answer, m.width)
	}

	text, level := m.statusOrDefault()
	return m.styles.statusStyle(level).Render(truncate(text, m.width))
}

// renderFooter is the key help. The short form is always visible so every action
// is discoverable without documentation; ? expands it in place.
func (m *model) renderFooter() string {
	var bindings = m.keys.ShortHelp()
	switch {
	case m.confirm != nil:
		bindings = m.keys.confirmHelp()
	case m.searchFocus:
		bindings = m.keys.searchHelp()
	}

	if m.showHelp {
		return m.help.View(m.keys)
	}

	view := m.help.ShortHelpView(bindings)
	// Name the focused pane so Tab's effect is discoverable by experiment.
	focus := m.styles.HeaderCount.Render("focus: " + m.focus.String())
	return m.joinEnds(view, focus, m.width)
}

// footerHeight measures the rendered footer, which the layout needs because the
// help view's height depends on its contents and the terminal width.
func (m *model) footerHeight() int {
	if !m.showHelp {
		return 1
	}
	if m.width <= 0 {
		return 1
	}

	h := m.help
	h.ShowAll = true
	h.Width = m.width
	return lipgloss.Height(h.View(m.keys))
}

// joinEnds places left and right on one line of exactly the given width, with
// the gap between them filled. If they cannot both fit, the left side wins and
// the right side is dropped, so the primary information is never lost.
func (m *model) joinEnds(left, right string, width int) string {
	lw, rw := lipgloss.Width(left), lipgloss.Width(right)

	if lw+rw+1 > width {
		return truncate(left, width)
	}
	return left + strings.Repeat(" ", width-lw-rw) + right
}
