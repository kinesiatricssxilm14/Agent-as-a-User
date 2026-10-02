package ui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/lipgloss"
	"github.com/tooln/tooln/internal/pkgs"
)

// View renders the full interface. All panels, the list, details, status and
// the shortcut footer are drawn in the same screen snapshot.
func (m *model) View() string {
	if !m.ready {
		return "loading…"
	}
	if m.mode == modeHelp {
		return m.renderHelp()
	}

	parts := []string{m.renderHeader()}
	if p := m.renderPrompt(); p != "" {
		parts = append(parts, p)
	}
	parts = append(parts, m.renderMain(), m.renderStatus(), m.renderFooter())
	return lipgloss.JoinVertical(lipgloss.Left, parts...)
}

// --- layout ----------------------------------------------------------------

func (m *model) layout() (listW, detailsW, listRows, detailsRows int) {
	headerH := 1
	promptH := 0
	if m.mode == modeFilter || m.mode == modeSearch || m.mode == modeConfirm || m.busyMsg != "" {
		promptH = 1
	}
	const statusH = 1
	const footerH = 1
	mainH := m.height - headerH - promptH - statusH - footerH
	if mainH < 3 {
		mainH = 3
	}

	listW = m.width * 55 / 100
	if listW < 22 {
		listW = 22
	}
	if listW > m.width-21 {
		listW = m.width - 21
	}
	if listW < 10 {
		listW = 10
	}
	detailsW = m.width - listW - 1
	if detailsW < 10 {
		detailsW = 10
	}

	listRows = mainH - 1
	detailsRows = mainH - 1
	if listRows < 0 {
		listRows = 0
	}
	if detailsRows < 0 {
		detailsRows = 0
	}
	return
}

func (m *model) listRows() int {
	_, _, r, _ := m.layout()
	return r
}

func (m *model) detailsRows() int {
	_, _, _, r := m.layout()
	return r
}

func (m *model) helpRows() int {
	return m.height - 1
}

// --- header / prompt / status / footer ------------------------------------

func (m *model) renderHeader() string {
	left := titleStyle.Render(" tooln ") + " "
	var tabs []string
	for i, name := range []string{"pip", "apt"} {
		label := " " + name + " "
		if i == m.currentIndex {
			tabs = append(tabs, activeTabStyle.Render(label))
		} else {
			tabs = append(tabs, inactiveTabStyle.Render(label))
		}
	}
	left += strings.Join(tabs, " ")

	right := headerStyle.Render(" ? help ")
	pad := m.width - lipgloss.Width(left) - lipgloss.Width(right)
	if pad < 1 {
		pad = 1
	}
	return left + strings.Repeat(" ", pad) + right
}

func (m *model) renderPrompt() string {
	switch {
	case m.busyMsg != "":
		return busyStyle.Render(" " + truncate(m.busyMsg, m.width-2))
	case m.mode == modeFilter:
		return m.filterInput.View()
	case m.mode == modeSearch:
		return m.searchInput.View() + helpTextStyle.Render("  enter:install  esc:back")
	case m.mode == modeConfirm:
		return promptStyle.Render(confirmPrompt(m))
	}
	return ""
}

func confirmPrompt(m *model) string {
	switch m.confirmAction {
	case "install":
		return fmt.Sprintf(" Install %q?   [enter/y] confirm   [esc/n] cancel", m.confirmTarget)
	case "uninstall":
		return fmt.Sprintf(" Uninstall %q?   [enter/y] confirm   [esc/n] cancel", m.confirmTarget)
	case "upgrade":
		return fmt.Sprintf(" Upgrade %q?   [enter/y] confirm   [esc/n] cancel", m.confirmTarget)
	}
	return " Confirm?   [enter/y] confirm   [esc/n] cancel"
}

func (m *model) renderStatus() string {
	if m.statusMsg == "" {
		return ""
	}
	s := truncate(m.statusMsg, m.width)
	switch m.statusKind {
	case statusSuccess:
		return successStyle.Render(s)
	case statusError:
		return errorStyle.Render(s)
	default:
		return helpTextStyle.Render(s)
	}
}

func (m *model) renderFooter() string {
	var parts []string
	switch m.mode {
	case modeFilter:
		parts = append(parts, "esc back", "enter done", "type to filter")
	case modeSearch:
		parts = append(parts, "esc back", "enter install", "↑/↓ choose", "type to search")
	case modeConfirm:
		parts = append(parts, "enter/y confirm", "esc/n cancel")
	case modeBrowse:
		if m.focus == focusDetails {
			parts = append(parts, "↑/↓ scroll", "pgup/pgdn page", "enter/esc back", "? help", "q quit")
		} else {
			for _, b := range browseFooter(m) {
				parts = append(parts, b.Help().Key+" "+b.Help().Desc)
			}
		}
	}
	return helpTextStyle.Render(truncate(strings.Join(parts, "   "), m.width))
}

func browseFooter(m *model) []key.Binding {
	return []key.Binding{
		m.keys.NextManager, m.keys.PrevManager,
		m.keys.Up, m.keys.Down,
		m.keys.Filter, m.keys.Search,
		m.keys.Remove, m.keys.Upgrade, m.keys.Refresh,
		m.keys.Details, m.keys.Help, m.keys.Quit,
	}
}

// --- main panels -----------------------------------------------------------

func (m *model) renderMain() string {
	listW, detailsW, listRows, _ := m.layout()

	var (
		listTitle, detTitle string
		listLines, detLines []string
	)

	if m.mode == modeSearch {
		listTitle = fmt.Sprintf(" Search — %s ", m.current().Name())
		listLines = m.searchListLines(listW)
		detTitle = " Result "
		detLines = m.searchDetailLines(detailsW)
	} else {
		listTitle = fmt.Sprintf(" Packages — %s (%d) ", m.current().Name(), len(m.filtered))
		listLines = m.packageListLines(listW)
		detTitle = " Details "
		detLines = detailsLines(m)
	}

	left := renderColumn(colTitleListStyle, listTitle, listLines, listW, listRows+1)
	right := renderColumn(colTitleDetailStyle, detTitle, detLines, detailsW, listRows+1)

	rows := make([]string, len(left))
	for i := range rows {
		rows[i] = left[i] + "│" + right[i]
	}
	return strings.Join(rows, "\n")
}

// renderColumn builds a fixed-height column: a styled title bar followed by
// body lines, each padded/truncated to exactly w visible columns.
func renderColumn(titleStyle lipgloss.Style, title string, lines []string, w, h int) []string {
	out := make([]string, 0, h)
	out = append(out, titleStyle.Render(padRight(" "+truncate(title, w-1), w)))
	for i := 0; i < h-1; i++ {
		var line string
		if i < len(lines) {
			line = padRight(lines[i], w)
		} else {
			line = strings.Repeat(" ", w)
		}
		out = append(out, line)
	}
	return out
}

func (m *model) packageListLines(w int) []string {
	if len(m.filtered) == 0 {
		return []string{"(no packages)"}
	}
	lines := make([]string, 0, m.listRows()+1)
	end := minInt(len(m.filtered), m.offset+m.listRows())
	for i := m.offset; i < end; i++ {
		p := m.filtered[i]
		row := padRight(formatRow(p.Name, p.Version, w), w)
		if i == m.cursor {
			lines = append(lines, selectedRowStyle.Render(row))
		} else {
			lines = append(lines, normalRowStyle.Render(row))
		}
	}
	return lines
}

func (m *model) searchListLines(w int) []string {
	switch {
	case m.searchLoading:
		return []string{"Searching…"}
	case len(m.searchResults) == 0:
		if strings.TrimSpace(m.searchInput.Value()) == "" {
			return []string{"Type a query above."}
		}
		return []string{"No results."}
	}
	lines := make([]string, 0, m.listRows()+1)
	end := minInt(len(m.searchResults), m.searchOffset+m.listRows())
	for i := m.searchOffset; i < end; i++ {
		r := m.searchResults[i]
		row := padRight(formatRow(r.Name, r.Version, w), w)
		if i == m.searchCursor {
			lines = append(lines, selectedRowStyle.Render(row))
		} else {
			lines = append(lines, normalRowStyle.Render(row))
		}
	}
	return lines
}

func (m *model) searchDetailLines(w int) []string {
	switch {
	case m.searchLoading:
		return wrapText("Searching…", w)
	case len(m.searchResults) == 0:
		if strings.TrimSpace(m.searchInput.Value()) == "" {
			return wrapText("Type a package name or keyword to search the repository.", w)
		}
		return wrapText("No matching packages found.", w)
	}
	if m.searchCursor < 0 || m.searchCursor >= len(m.searchResults) {
		m.searchCursor = 0
	}
	res := m.searchResults[m.searchCursor]
	var b strings.Builder
	addKV(&b, "Name", res.Name)
	addKV(&b, "Version", res.Version)
	addKV(&b, "Summary", res.Summary)
	b.WriteString("Press enter to install.")
	return wrapText(strings.TrimRight(b.String(), "\n"), w)
}

// --- details ---------------------------------------------------------------

func detailsLines(m *model) []string {
	_, detailsW, _, _ := m.layout()
	switch {
	case m.detailsLoading:
		return wrapText("Loading…", detailsW)
	case m.detailsErr != nil:
		return wrapText("Error: "+m.detailsErr.Error(), detailsW)
	case m.details == nil:
		return wrapText("Select a package to view details.", detailsW)
	default:
		return wrapText(renderDetailsText(m, m.details), detailsW)
	}
}

func renderDetailsText(m *model, info *pkgs.PackageInfo) string {
	var b strings.Builder
	addKV(&b, "Name", info.Name)
	addKV(&b, "Version", info.Version)
	if info.Summary != "" {
		addKV(&b, "Summary", info.Summary)
	}
	if info.Status != "" {
		addKV(&b, "Status", info.Status)
	}
	if info.InstalledSize != "" {
		addKV(&b, "Size", info.InstalledSize)
	}
	if info.HomePage != "" {
		addKV(&b, "Homepage", info.HomePage)
	}
	if info.Author != "" {
		addKV(&b, "Author", info.Author)
	}
	if info.License != "" {
		addKV(&b, "License", info.License)
	}
	if info.Location != "" {
		addKV(&b, "Location", info.Location)
	}

	if len(info.Depends) > 0 {
		b.WriteString("Dependencies (direct):\n")
		for _, d := range info.Depends {
			v := installedVersion(m, d)
			if v != "" {
				v = "  (" + v + ")"
			}
			fmt.Fprintf(&b, "  • %s%s\n", d, v)
		}
	} else {
		addKV(&b, "Dependencies", "none")
	}

	if len(info.RequiredBy) > 0 {
		addKV(&b, "Required-by", strings.Join(info.RequiredBy, ", "))
	}
	if info.Description != "" {
		b.WriteString("Description:\n")
		b.WriteString(info.Description)
		b.WriteString("\n")
	}
	return strings.TrimRight(b.String(), "\n")
}

func installedVersion(m *model, name string) string {
	for _, p := range m.packages {
		if strings.EqualFold(p.Name, name) {
			return p.Version
		}
	}
	return ""
}

// --- help ------------------------------------------------------------------

func (m *model) renderHelp() string {
	w := m.width
	title := colTitleDetailStyle.Render(padRight(" Help ", w))
	body := helpLines(m)
	rows := m.helpRows()
	out := []string{title}
	for i := 0; i < rows; i++ {
		if i < len(body) {
			out = append(out, padRight(body[i], w))
		} else {
			out = append(out, "")
		}
	}
	return strings.Join(out, "\n")
}

func helpLines(m *model) []string {
	k := m.keys
	var b strings.Builder
	b.WriteString("tooln — Python & system package manager\n\n")

	b.WriteString("Browse:\n")
	b.WriteString(keyLine(k.Up) + "\n")
	b.WriteString(keyLine(k.Down) + "\n")
	b.WriteString(keyLine(k.PageUp) + "\n")
	b.WriteString(keyLine(k.PageDown) + "\n")
	b.WriteString(keyLine(k.Top) + "\n")
	b.WriteString(keyLine(k.Bottom) + "\n")
	b.WriteString(keyLine(k.NextManager) + "\n")
	b.WriteString(keyLine(k.PrevManager) + "\n\n")

	b.WriteString("Actions:\n")
	b.WriteString(keyLine(k.Filter) + "\n")
	b.WriteString(keyLine(k.Search) + "\n")
	b.WriteString(keyLine(k.Remove) + "\n")
	b.WriteString(keyLine(k.Upgrade) + "\n")
	b.WriteString(keyLine(k.Refresh) + "\n")
	b.WriteString(keyLine(k.Details) + "\n\n")

	b.WriteString("Confirm:\n")
	b.WriteString(keyLine(k.Confirm) + "\n")
	b.WriteString(keyLine(k.Cancel) + "\n\n")

	b.WriteString("General:\n")
	b.WriteString(keyLine(k.Help) + "\n")
	b.WriteString(keyLine(k.Quit) + "\n\n")

	b.WriteString("Notes:\n")
	b.WriteString("  • pip operations run through \"python3 -m pip\" (real pip).\n")
	b.WriteString("  • apt operations run through apt-get / dpkg / apt-cache.\n")
	b.WriteString("  • Uninstalling a pip package also removes now-orphaned dependencies.\n")
	b.WriteString("  • All package data is read live from the container environment.\n")

	return wrapText(b.String(), m.width)
}

func keyLine(b key.Binding) string {
	return "  " + padRight(b.Help().Key, 14) + b.Help().Desc
}

// --- small text helpers ----------------------------------------------------

func addKV(b *strings.Builder, k, v string) {
	if v == "" {
		v = "—"
	}
	fmt.Fprintf(b, "%-12s %s\n", k+":", v)
}

func formatRow(name, version string, w int) string {
	v := version
	nameW := w - runeLen(v)
	if nameW < 4 {
		nameW = 4
	}
	n := truncate(name, nameW)
	return n + strings.Repeat(" ", nameW-runeLen(n)) + v
}

func padRight(s string, w int) string {
	pad := w - lipgloss.Width(s)
	if pad <= 0 {
		return s
	}
	return s + strings.Repeat(" ", pad)
}

func wrapText(s string, width int) []string {
	if width < 1 {
		width = 1
	}
	var out []string
	for _, line := range strings.Split(s, "\n") {
		r := []rune(line)
		if len(r) == 0 {
			out = append(out, "")
			continue
		}
		for len(r) > width {
			out = append(out, string(r[:width]))
			r = r[width:]
		}
		out = append(out, string(r))
	}
	return out
}

func runeLen(s string) int {
	return len([]rune(s))
}

func minInt(a, b int) int {
	if a < b {
		return a
	}
	return b
}
