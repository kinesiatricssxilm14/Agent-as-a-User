package main

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
)

func (m *model) applyLayout() {
	w, h := m.width, m.height
	if w <= 0 || h <= 0 {
		return
	}
	const headerH, searchH, statusH, helpH = 1, 1, 1, 2
	outputH := 0
	if m.state == StateRunning {
		outputH = clamp(h/4, 6, 14)
	}
	mainH := h - headerH - searchH - statusH - helpH - outputH
	if mainH < 6 {
		mainH = 6
	}

	listW := clamp(w*2/5, 1, w-1)
	detailsW := w - listW

	m.listW, m.listH = listW, mainH
	m.detailsW, m.detailsH = detailsW, mainH
	m.outputW, m.outputH = w, outputH

	m.list.SetSize(listW, mainH)
	m.details.Width = detailsW
	m.details.Height = mainH
	m.outputView.Width = w
	m.outputView.Height = outputH
	m.searchInput.Width = clamp(w-28, 10, 200)
}

func (m *model) View() string {
	if !m.ready || m.width <= 0 || m.height <= 0 {
		return "starting toola…"
	}
	m.applyLayout()

	if m.help {
		return m.renderHelp()
	}

	header := m.renderHeader(m.width)
	search := m.renderSearch(m.width)

	listView := m.list.View()
	detailsView := m.details.View()
	sep := separatorStyle.Render(strings.Repeat("│\n", m.listH-1) + "│")
	mainRow := lipgloss.JoinHorizontal(lipgloss.Top, listView, sep, detailsView)

	status := m.renderStatus(m.width)
	bottom := m.renderBottom(m.width)

	return lipgloss.JoinVertical(lipgloss.Left, header, search, mainRow, status, bottom)
}

func (m *model) renderHeader(w int) string {
	title := titleStyle.Render(" toola ")
	right := headerRightStyle.Render(fmt.Sprintf(" %s · %d pkgs ", m.mode.String(), len(m.filtered)))
	spacerW := max(w-lipgloss.Width(title)-lipgloss.Width(right), 1)
	spacer := lipgloss.NewStyle().Width(spacerW).Render("")
	return headerBarStyle.Render(lipgloss.JoinHorizontal(lipgloss.Top, title, spacer, right))
}

func (m *model) renderSearch(w int) string {
	style := searchStyle
	if m.focus == FocusSearch {
		style = searchFocusStyle
	}
	input := style.Render(m.searchInput.View())
	hint := hintStyle.Render(" [esc] clear · [enter] finish")
	pad := max(w-lipgloss.Width(input)-lipgloss.Width(hint), 0)
	return lipgloss.JoinHorizontal(lipgloss.Left, input, hint, strings.Repeat(" ", pad))
}

func (m *model) renderStatus(w int) string {
	switch m.state {
	case StateConfirm:
		return confirmBarStyle.Width(w).Render(" " + m.confirmMsg + " ")
	case StateRunning:
		return runningStyle.Width(w).Render(" " + m.spinner.View() + " " + m.statusMsg + " ")
	default:
		msg := m.statusMsg
		if m.loading {
			msg = m.spinner.View() + " " + msg
		}
		style := statusStyle
		if strings.HasPrefix(m.statusMsg, "✗") {
			style = statusError
		} else if strings.HasPrefix(m.statusMsg, "✓") {
			style = statusOk
		}
		return style.Width(w).Render(msg)
	}
}

func (m *model) renderBottom(w int) string {
	if m.state == StateRunning {
		return m.outputView.View()
	}
	return lipgloss.JoinVertical(lipgloss.Left, m.renderHelpBarLine1(w), m.renderHelpBarLine2(w))
}

func (m *model) renderHelpBarLine1(w int) string {
	line := joinKeys([][2]string{
		{"↑/↓", "move"}, {"j/k", "move"}, {"/", "search"}, {"Tab", "focus"}, {"Enter", "action"},
	})
	return helpBarStyle.Width(w).Render(line)
}

func (m *model) renderHelpBarLine2(w int) string {
	line := joinKeys([][2]string{
		{"i", "install"}, {"r", "remove"}, {"u", "upgrade"}, {"U", "upgrade all"},
		{"R", "update"}, {"1-4", "view"}, {"?", "help"}, {"q", "quit"},
	})
	return helpBarStyle.Width(w).Render(line)
}

func joinKeys(pairs [][2]string) string {
	parts := make([]string, 0, len(pairs))
	for _, p := range pairs {
		parts = append(parts, keyStyle.Render(p[0])+dimStyle.Render(" "+p[1]))
	}
	return " " + strings.Join(parts, "  ")
}

func (m *model) renderHelp() string {
	rows := [][2]string{
		{"↑/↓  j/k", "move selection"},
		{"PgUp/PgDn", "page through list"},
		{"Home/End  g/G", "jump to first / last"},
		{"/", "search / filter packages"},
		{"Enter", "default action (install / remove / upgrade)"},
		{"Tab", "cycle focus: list → details → search"},
		{"Esc", "cancel / clear search / back to list"},
		{"i", "install selected package"},
		{"r", "remove (uninstall) selected package"},
		{"u", "upgrade selected package"},
		{"U", "upgrade all upgradable packages"},
		{"R", "apt-get update (refresh lists)"},
		{"1 2 3 4", "view: installed / available / upgradable / all"},
		{"?", "toggle this help"},
		{"q  Ctrl+C", "quit"},
	}
	var b strings.Builder
	b.WriteString(titleStyle.Render("toola") + " — key bindings\n\n")
	for _, r := range rows {
		b.WriteString("  " + keyStyle.Width(22).Render(r[0]) + " " + dimStyle.Render(r[1]) + "\n")
	}
	b.WriteString("\npress any key to close")

	w := clamp(m.width-8, 24, 84)
	box := helpBoxStyle.Width(w).Render(b.String())
	return lipgloss.Place(m.width, m.height, lipgloss.Center, lipgloss.Center, box)
}

func renderDetailsPlaceholder(name string) string {
	return fmt.Sprintf("Loading details for %s…", name)
}

// renderDetails renders the full description and every dependency category,
// one dependency per line, for the fixed details panel.
func renderDetails(d PackageDetails) string {
	var b strings.Builder
	label := func(s string) string { return detailsLabelStyle.Render(s) }
	write := func(name, value string) {
		if value == "" {
			return
		}
		fmt.Fprintf(&b, "%s %s\n", label(name+":"), value)
	}

	statusText := d.Status.String()
	if d.Status == StatusUpgradable {
		statusText = "installed (upgradable)"
	}

	write("Name", d.Name)
	write("Status", statusText)
	write("Installed", d.InstalledVer)
	write("Candidate", d.CandidateVer)
	write("Architecture", d.Arch)
	write("Maintainer", d.Maintainer)
	if d.InstalledSize != "" {
		write("Installed-Size", d.InstalledSize+" kB")
	}
	write("Homepage", d.Homepage)

	if d.Description != "" {
		b.WriteString("\n")
		b.WriteString(label("Description:") + "\n")
		b.WriteString(d.Description + "\n")
	}

	groups := []struct {
		name  string
		items []string
	}{
		{"Depends", d.Depends},
		{"Pre-Depends", d.PreDepends},
		{"Recommends", d.Recommends},
		{"Suggests", d.Suggests},
		{"Conflicts", d.Conflicts},
		{"Breaks", d.Breaks},
		{"Replaces", d.Replaces},
		{"Provides", d.Provides},
	}
	for _, g := range groups {
		if len(g.items) == 0 {
			continue
		}
		b.WriteString("\n")
		b.WriteString(label(g.name+":") + "\n")
		for _, item := range g.items {
			fmt.Fprintf(&b, "  %s %s\n", depBulletStyle.Render("•"), depStyle.Render(item))
		}
	}
	return b.String()
}
