package ui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"
)

type col struct {
	title  string
	weight float64
	fixed  int
}

func (m Model) View() string {
	if !m.ready {
		return "Loading…"
	}
	sections := []string{
		m.renderTitle(),
		m.renderTabs(),
		m.renderContent(),
	}
	if m.confirm != nil {
		sections = append(sections, m.renderConfirm())
	}
	sections = append(sections, m.renderHelpBar(), m.renderStatus())
	return lipgloss.JoinVertical(lipgloss.Left, sections...)
}

func (m Model) renderTitle() string {
	left := " toolm "
	right := " Docker Container Management "
	padWidth := maxInt(0, m.width-lipgloss.Width(left)-lipgloss.Width(right))
	return titleBarStyle.Render(left + strings.Repeat(" ", padWidth) + right)
}

func (m Model) renderTabs() string {
	var cells []string
	for i, title := range tabTitles {
		label := fmt.Sprintf(" %d:%s ", i+1, title)
		if c := m.tabCount(tabKind(i)); c >= 0 {
			label = fmt.Sprintf(" %d:%s(%d) ", i+1, title, c)
		}
		if int(m.tab) == i {
			cells = append(cells, activeTabStyle.Render(label))
		} else {
			cells = append(cells, tabStyle.Render(label))
		}
	}
	return strings.Join(cells, " ")
}

func (m Model) tabCount(t tabKind) int {
	if !m.loaded[t] {
		return -1
	}
	switch t {
	case tabContainers:
		return len(m.containers)
	case tabImages:
		return len(m.images)
	case tabNetworks:
		return len(m.networks)
	case tabVolumes:
		return len(m.volumes)
	}
	return -1
}

func (m Model) renderContent() string {
	switch m.view {
	case viewDetail:
		return m.renderDetail()
	case viewLogs:
		return m.renderLogs()
	case viewHelp:
		return m.renderHelp()
	default:
		return m.renderList()
	}
}

func (m Model) renderList() string {
	contentH := m.contentHeight()
	if !m.loaded[m.tab] {
		return m.fillContent(contentH, "Loading…")
	}
	if m.tabErr[m.tab] != "" {
		return m.fillContent(contentH, "Error: "+m.tabErr[m.tab])
	}
	idx := m.filteredIndices()
	if len(idx) == 0 {
		msg := "No items found"
		if m.filter != "" {
			msg = fmt.Sprintf("No items match filter %q", m.filter)
		}
		return m.fillContent(contentH, msg)
	}
	rows := m.buildRows(idx)
	cols := m.tabCols()
	bodyH := contentH - 1
	return renderTable(cols, rows, m.width, bodyH, m.cursor, m.offset)
}

func (m Model) buildRows(idx []int) [][]string {
	rows := make([][]string, 0, len(idx))
	for _, i := range idx {
		switch m.tab {
		case tabContainers:
			c := m.containers[i]
			rows = append(rows, []string{c.Name(), c.Image, c.Status, formatContainerPorts(c.Ports), shortID(c.ID)})
		case tabImages:
			img := m.images[i]
			repo, tag := firstRepoTag(img.RepoTags)
			rows = append(rows, []string{repo, tag, formatSizeMB(img.Size), shortID(img.ID)})
		case tabNetworks:
			n := m.networks[i]
			rows = append(rows, []string{n.Name, n.Driver, n.Scope})
		case tabVolumes:
			v := m.volumes[i]
			rows = append(rows, []string{v.Name, v.Driver, v.Mountpoint})
		}
	}
	return rows
}

func (m Model) tabCols() []col {
	switch m.tab {
	case tabContainers:
		return []col{
			{title: "NAME", weight: 3},
			{title: "IMAGE", weight: 3},
			{title: "STATUS", weight: 3},
			{title: "PORTS", weight: 3},
			{title: "ID", fixed: 12},
		}
	case tabImages:
		return []col{
			{title: "REPOSITORY", weight: 4},
			{title: "TAG", weight: 2},
			{title: "SIZE", fixed: 12},
			{title: "IMAGE ID", fixed: 12},
		}
	case tabNetworks:
		return []col{
			{title: "NAME", weight: 3},
			{title: "DRIVER", weight: 2},
			{title: "SCOPE", weight: 1},
		}
	case tabVolumes:
		return []col{
			{title: "NAME", weight: 3},
			{title: "DRIVER", weight: 2},
			{title: "MOUNTPOINT", weight: 4},
		}
	}
	return nil
}

func renderTable(cols []col, rows [][]string, width, height, selected, offset int) string {
	widths := computeWidths(width, cols)
	total := 0
	for i, w := range widths {
		total += w
		if i > 0 {
			total += 2
		}
	}
	var b strings.Builder

	hc := make([]string, len(cols))
	for i, c := range cols {
		hc[i] = pad(truncate(c.title, widths[i]), widths[i])
	}
	b.WriteString(listHeaderStyle.Render(strings.Join(hc, "  ")))
	b.WriteString("\n")

	end := offset + height
	if end > len(rows) {
		end = len(rows)
	}
	for i := offset; i < end; i++ {
		cells := make([]string, len(cols))
		for j := range cols {
			cells[j] = pad(truncate(rows[i][j], widths[j]), widths[j])
		}
		line := strings.Join(cells, "  ")
		line = pad(line, total)
		if i == selected {
			line = selectedStyle.Render(line)
		}
		b.WriteString(line)
		b.WriteString("\n")
	}
	return strings.TrimRight(b.String(), "\n")
}

func computeWidths(total int, cols []col) []int {
	widths := make([]int, len(cols))
	fixed := 0
	weightSum := 0.0
	lastFlex := -1
	for i, c := range cols {
		if c.fixed > 0 {
			widths[i] = c.fixed
			fixed += c.fixed + 2
		} else {
			weightSum += c.weight
			lastFlex = i
		}
	}
	remaining := total - fixed
	if remaining < 0 {
		remaining = 0
	}
	assigned := 0
	for i, c := range cols {
		if c.fixed == 0 && weightSum > 0 {
			w := int(float64(remaining) * c.weight / weightSum)
			if w < 4 {
				w = 4
			}
			widths[i] = w
			assigned += w
		}
	}
	if lastFlex >= 0 {
		diff := remaining - assigned
		widths[lastFlex] += diff
		if widths[lastFlex] < 4 {
			widths[lastFlex] = 4
		}
	}
	return widths
}

func (m Model) renderDetail() string {
	contentH := m.contentHeight()
	if m.detailLoading {
		return m.fillContent(contentH, "Loading…")
	}
	title := detailTitleStyle.Render(m.detailTitle)
	bodyH := contentH - 1
	if bodyH < 0 {
		bodyH = 0
	}
	if len(m.detailLines) == 0 {
		return title + "\n" + m.fillContent(bodyH, "(no details)")
	}
	end := m.detailScroll + bodyH
	if end > len(m.detailLines) {
		end = len(m.detailLines)
	}
	body := strings.Join(m.detailLines[m.detailScroll:end], "\n")
	return title + "\n" + m.padLines(body, bodyH)
}

func (m Model) renderLogs() string {
	contentH := m.contentHeight()
	title := detailTitleStyle.Render("Logs: " + m.logsName + " (" + shortID(m.logsID) + ")")
	bodyH := contentH - 1
	if bodyH < 0 {
		bodyH = 0
	}
	if m.logsLoading {
		return title + "\n" + m.fillContent(bodyH, "Loading logs…")
	}
	if len(m.logs) == 0 {
		return title + "\n" + m.fillContent(bodyH, "(no log output)")
	}
	end := m.logsScroll + bodyH
	if end > len(m.logs) {
		end = len(m.logs)
	}
	body := strings.Join(m.logs[m.logsScroll:end], "\n")
	return title + "\n" + m.padLines(body, bodyH)
}

func (m Model) renderHelp() string {
	contentH := m.contentHeight()
	lines := helpLines()
	title := detailTitleStyle.Render("Help")
	bodyH := contentH - 1
	if bodyH < 0 {
		bodyH = 0
	}
	end := m.helpScroll + bodyH
	if end > len(lines) {
		end = len(lines)
	}
	body := strings.Join(lines[m.helpScroll:end], "\n")
	return title + "\n" + m.padLines(body, bodyH)
}

func (m Model) renderConfirm() string {
	kind := tabTitles[m.confirm.tab]
	text := fmt.Sprintf(" Remove %s %q?  [y]es / [n]o ", kind, m.confirm.name)
	line := confirmStyle.Render(text)
	fill := maxInt(0, m.width-lipgloss.Width(line))
	return line + strings.Repeat(" ", fill)
}

func (m Model) renderHelpBar() string {
	if m.filtering {
		return keyStyle.Render("enter") + descStyle.Render(" apply  ") +
			keyStyle.Render("esc") + descStyle.Render(" cancel")
	}
	var pairs [][2]string
	switch m.view {
	case viewHelp:
		pairs = [][2]string{
			{"esc/?", "back"}, {"↑/↓", "scroll"}, {"q", "quit"},
		}
	case viewLogs:
		pairs = [][2]string{
			{"↑/↓", "scroll"}, {"PgUp/PgDn", "page"}, {"g/G", "top/bottom"},
			{"esc", "back"}, {"ctrl+r", "reload"}, {"?", "help"}, {"q", "quit"},
		}
	case viewDetail:
		pairs = [][2]string{
			{"↑/↓", "scroll"}, {"esc", "back"},
		}
		if m.tab == tabContainers {
			pairs = append(pairs,
				[2]string{"l", "logs"}, [2]string{"s", "start"}, [2]string{"x", "stop"}, [2]string{"r", "restart"})
		}
		pairs = append(pairs,
			[2]string{"d", "remove"}, [2]string{"ctrl+r", "reload"}, [2]string{"?", "help"}, [2]string{"q", "quit"})
	default:
		pairs = [][2]string{
			{"↑/↓", "move"}, {"enter", "details"}, {"←/→", "view"}, {"/", "filter"},
		}
		if m.tab == tabContainers {
			pairs = append(pairs,
				[2]string{"l", "logs"}, [2]string{"s", "start"}, [2]string{"x", "stop"}, [2]string{"r", "restart"})
		}
		pairs = append(pairs,
			[2]string{"d", "remove"}, [2]string{"ctrl+r", "reload"}, [2]string{"?", "help"}, [2]string{"q", "quit"})
	}
	var b strings.Builder
	for _, p := range pairs {
		b.WriteString(keyStyle.Render(p[0]))
		b.WriteString(descStyle.Render(" " + p[1] + " "))
	}
	return helpBarStyle.Render(strings.TrimRight(b.String(), " "))
}

func (m Model) renderStatus() string {
	if m.filtering {
		return statusStyle.Render(" Filter: ") + m.filterInput.View()
	}
	if m.message != "" {
		if m.messageErr {
			return statusErrorStyle.Render(" " + m.message + " ")
		}
		return statusStyle.Render(" " + m.message + " ")
	}
	if m.filter != "" {
		return statusStyle.Render(" Filter: " + m.filter + "  (/ to edit)")
	}
	return statusStyle.Render(" Ready ")
}

func (m Model) fillContent(h int, text string) string {
	lines := make([]string, 0, h)
	lines = append(lines, text)
	for len(lines) < h {
		lines = append(lines, "")
	}
	return strings.Join(lines, "\n")
}

func (m Model) padLines(s string, h int) string {
	lines := strings.Split(s, "\n")
	for len(lines) < h {
		lines = append(lines, "")
	}
	return strings.Join(lines, "\n")
}
