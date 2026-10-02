package ui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/lipgloss"

	"tooln/internal/pkgmgr"
)

// renderPanel is the single place a bordered panel is produced. It clips the
// content to the rows the panel actually has, then clips the finished block, so
// no panel can ever push the layout off screen.
func renderPanel(style lipgloss.Style, title, body string, width, height int) string {
	inner := max(10, width-4)   // 2 border columns + 2 padding columns
	rows := max(3, height)      // total lines this panel may occupy
	content := max(1, rows-2-1) // minus the two border rows and the title row

	block := title + "\n" + clipBody(body, content)
	return clipHeight(
		style.Width(inner+2).Height(rows-2).MaxWidth(inner+4).Render(block),
		rows,
	)
}

// panelTitle builds a panel title with a right-aligned hint.
func (m *Model) panelHeading(title, hint string, inner int) string {
	head := m.styles.panelTitle.Render(title)
	if hint == "" {
		return truncate(head, inner)
	}
	rendered := m.styles.dim.Render(hint)
	gap := inner - lipgloss.Width(head) - lipgloss.Width(rendered)
	if gap < 1 {
		return truncate(head, inner)
	}
	return head + strings.Repeat(" ", gap) + rendered
}

// renderDetails draws the details panel around the scrollable viewport that
// holds the selected package's fields.
func (m *Model) renderDetails(width, height int) string {
	t := m.cur()
	inner := max(10, width-4)

	name := "Details"
	if t != nil {
		if cur, ok := t.current(); ok {
			name = "Details: " + ellipsize(cur.Name, max(8, inner-14))
		}
	}
	title := m.panelHeading(name, "J/K scroll", inner)

	style := m.styles.panel
	if m.focusDetails {
		style = m.styles.panelFocus
	}
	return renderPanel(style, title, m.vpView(t), width, height)
}

func (m *Model) vpView(t *tabState) string {
	if t == nil {
		return ""
	}
	return t.vp.View()
}

// refreshDetailViewport rebuilds the details text for the selected package. The
// content is regenerated only when the selection or the cached data changed, so
// the scroll position survives cursor movement within the same package.
func (m *Model) refreshDetailViewport() {
	t := m.cur()
	if t == nil {
		return
	}
	cur, ok := t.current()
	if !ok {
		t.vp.SetContent(m.styles.dim.Render("No package selected."))
		t.shownName = ""
		return
	}
	key := cur.Name
	if d, cached := t.details[key]; cached && d != nil {
		key += "\x00" + d.Version
	} else if err, failed := t.detailErrs[cur.Name]; failed {
		key += "\x00err" + err
	} else {
		key += "\x00pending"
	}
	if t.shownName == key {
		return
	}
	t.shownName = key
	t.vp.SetContent(m.detailContent(t, cur))
	t.vp.GotoTop()
}

// detailContent lays out everything known about one package.
func (m *Model) detailContent(t *tabState, cur pkgmgr.Package) string {
	width := max(20, t.vp.Width)
	var b strings.Builder

	// A short value shares the label's line; a long one wraps beneath it under a
	// hanging indent. Keeping short fields on one line matters: it is what lets
	// the whole record fit on screen at once instead of needing to be scrolled.
	writeField := func(key, value string) {
		value = strings.TrimSpace(value)
		if value == "" {
			return
		}
		label := key + ":"
		rendered := m.styles.fieldKey.Render(label)
		if lipgloss.Width(label)+1+lipgloss.Width(value) <= width && !strings.Contains(value, "\n") {
			b.WriteString(rendered + " " + m.styles.fieldVal.Render(value) + "\n")
			return
		}
		const indent = 2
		b.WriteString(rendered + "\n")
		for _, l := range strings.Split(wrapText(value, max(8, width-indent)), "\n") {
			b.WriteString(strings.Repeat(" ", indent) + m.styles.fieldVal.Render(l) + "\n")
		}
	}

	d, haveDetails := t.details[cur.Name]
	errText, failed := t.detailErrs[cur.Name]

	switch {
	case haveDetails && d != nil:
		for _, f := range d.Fields {
			writeField(f.Key, f.Value)
		}
		b.WriteString("\n")
		m.writeDeps(&b, t, "Direct dependencies", d.Requires, width)
		if len(d.RequiredBy) > 0 {
			m.writeDeps(&b, t, "Needed by", d.RequiredBy, width)
		}
		if d.Source != "" {
			b.WriteString(m.styles.dim.Render(wrapText("Source: "+d.Source, width)) + "\n")
		}

	case failed:
		writeField("Package", cur.Name)
		writeField("Version", orDash(cur.Version))
		writeField("Summary", cur.Summary)
		b.WriteString("\n")
		b.WriteString(m.styles.statusErr.Render("Could not read the details:") + "\n")
		b.WriteString(wrapText(errText, width) + "\n\n")
		b.WriteString(m.styles.dim.Render("Press r to rescan and try again.") + "\n")

	default:
		writeField("Package", cur.Name)
		writeField("Version", orDash(cur.Version))
		writeField("Summary", cur.Summary)
		if cur.Note != "" {
			writeField("Note", cur.Note)
		}
		b.WriteString("\n" + m.spin.View() + " " +
			m.styles.dim.Render("Reading the full metadata…") + "\n")
	}

	// Actions available for this specific package, so the next keystroke is
	// always visible next to the thing it acts on.
	b.WriteString("\n" + m.styles.fieldKey.Render("What you can do here:") + "\n")
	for _, line := range m.packageActions(t, cur) {
		b.WriteString("  " + line + "\n")
	}
	return strings.TrimRight(b.String(), "\n")
}

// writeDeps lists dependency names, annotating each with whether it is present
// in the current environment.
func (m *Model) writeDeps(b *strings.Builder, t *tabState, title string, deps []string, width int) {
	b.WriteString(m.styles.fieldKey.Render(fmt.Sprintf("%s (%d):", title, len(deps))) + "\n")
	if len(deps) == 0 {
		b.WriteString("  " + m.styles.dim.Render("none") + "\n\n")
		return
	}
	installed := map[string]bool{}
	for _, p := range t.all {
		if p.Installed {
			installed[loosen(p.Name)] = true
		}
	}
	for _, dep := range deps {
		mark, style := "?", m.styles.dim
		if installed[loosen(dep)] {
			mark, style = "✓", m.styles.depName
		} else if t.source == sourceInstalled && t.loaded {
			mark, style = "·", m.styles.dim
		}
		line := fmt.Sprintf("%s %s", mark, dep)
		b.WriteString("  " + style.Render(ellipsize(line, max(8, width-2))) + "\n")
	}
	b.WriteString("\n")
}

// packageActions describes the keys that apply to the selected package.
func (m *Model) packageActions(t *tabState, cur pkgmgr.Package) []string {
	k := func(key, desc string) string {
		return m.styles.helpKey.Render(key) + " " + m.styles.helpDesc.Render(desc)
	}
	var out []string
	if cur.Installed {
		out = append(out, k("d", "uninstall "+cur.Name))
		if cur.Latest != "" {
			out = append(out, k("U", "upgrade to "+cur.Latest))
		} else {
			out = append(out, k("U", "upgrade to the newest version"))
		}
	} else {
		out = append(out, k("i", "install "+cur.Name))
	}
	out = append(out,
		k("space", "mark it so d can act on several at once"),
		k("o", "check which installed packages have updates"),
		k("r", "rescan so this list matches the environment"),
		k("?", "the full key list"),
	)
	return out
}

// ------------------------------------------------------- confirm and menu ----

// renderConfirmPanel replaces the details pane while a question is pending. It
// deliberately occupies the same region rather than floating over the list, so
// the packages being acted on stay visible alongside the commands to be run.
func (m *Model) renderConfirmPanel(width, height int) string {
	inner := max(10, width-4)
	c := m.confirm
	if c == nil {
		return renderPanel(m.styles.panel, "", "", width, height)
	}

	var b strings.Builder
	for _, line := range c.lines {
		if line == "" {
			b.WriteString("\n")
			continue
		}
		style := m.styles.fieldVal
		if strings.HasPrefix(strings.TrimSpace(line), "$") {
			style = m.styles.cmdLine
		}
		for _, wrapped := range strings.Split(wrapText(line, inner), "\n") {
			b.WriteString(style.Render(wrapped) + "\n")
		}
	}
	if c.awaiting {
		b.WriteString("\n" + m.spin.View() + " " + m.styles.dim.Render("checking…") + "\n")
	}
	b.WriteString("\n" +
		m.styles.helpKey.Render("enter") + m.styles.helpDesc.Render(" or ") +
		m.styles.helpKey.Render("y") + m.styles.helpDesc.Render(" to go ahead") + "\n" +
		m.styles.helpKey.Render("esc") + m.styles.helpDesc.Render(" or ") +
		m.styles.helpKey.Render("n") + m.styles.helpDesc.Render(" to cancel"))

	title := m.panelHeading(c.title, "", inner)
	return renderPanel(m.styles.dialog, title, b.String(), width, height)
}

// renderMenuPanel draws the maintenance menu in the details region.
func (m *Model) renderMenuPanel(width, height int) string {
	inner := max(10, width-4)

	var b strings.Builder
	if t := m.cur(); t != nil {
		b.WriteString(m.styles.dim.Render("actions for "+t.mgr.ID()) + "\n")
	}
	b.WriteString("\n")

	for i, item := range m.menuItems {
		marker := "  "
		style := m.styles.fieldVal
		if i == m.menuIndex {
			marker = "› "
			style = m.styles.rowCursor
		}
		label := fmt.Sprintf("[%s] %s", item.Key, item.Title)
		b.WriteString(style.Render(ellipsize(marker+label, inner)) + "\n")
		for _, step := range item.Plan.Steps {
			b.WriteString(m.styles.dim.Render(ellipsize("      $ "+step.Display(), inner)) + "\n")
		}
	}
	b.WriteString("\n" +
		m.styles.helpKey.Render("↑/↓") + m.styles.helpDesc.Render(" choose · ") +
		m.styles.helpKey.Render("enter") + m.styles.helpDesc.Render(" run · ") +
		m.styles.helpKey.Render("esc") + m.styles.helpDesc.Render(" close"))

	title := m.panelHeading("Maintenance", "", inner)
	return renderPanel(m.styles.dialog, title, b.String(), width, height)
}

// ------------------------------------------------------------ help and log ----

// renderHelp fills the help viewport with the long-form key documentation.
func (m *Model) renderHelp() {
	width := max(20, m.helpVP.Width)
	var b strings.Builder

	b.WriteString(m.styles.title.Render("tooln — keys and what they do") + "\n")
	b.WriteString(m.styles.dim.Render(wrapText(
		"tooln manages the packages on this machine by calling pip and apt directly. "+
			"Every action shows the command it will run before it runs it, and the list is "+
			"rescanned afterwards so what you see is the real state of the environment.",
		width)) + "\n\n")

	for _, section := range m.keys.helpSections() {
		b.WriteString(m.styles.fieldKey.Render(section.Title) + "\n")
		// Align the descriptions in a column.
		keyW := 0
		for _, r := range section.Rows {
			keyW = max(keyW, lipgloss.Width(r.Keys))
		}
		keyW = min(keyW, max(8, width/3))
		for _, r := range section.Rows {
			keys := pad(m.styles.helpKey.Render(ellipsize(r.Keys, keyW)), keyW)
			desc := wrapText(r.Desc, max(10, width-keyW-4))
			lines := strings.Split(desc, "\n")
			b.WriteString("  " + keys + "  " + m.styles.helpDesc.Render(lines[0]) + "\n")
			for _, extra := range lines[1:] {
				b.WriteString("  " + strings.Repeat(" ", keyW) + "  " +
					m.styles.helpDesc.Render(extra) + "\n")
			}
		}
		b.WriteString("\n")
	}

	b.WriteString(m.styles.fieldKey.Render("How the two views differ") + "\n")
	for _, line := range []string{
		"pip manages the Python packages for the interpreter tooln found on PATH. " +
			"Searching looks the name up on PyPI, so you can install something that is " +
			"not on this machine yet.",
		"apt manages the Debian system packages. Searching goes through apt-cache, " +
			"which only knows about the package lists this machine has fetched; press m " +
			"then u to refresh them.",
		"Uninstalling a pip package also removes the dependencies it pulled in, once " +
			"nothing else needs them. Uninstalling an apt package uses --auto-remove for " +
			"the same reason.",
	} {
		b.WriteString("  " + strings.ReplaceAll(wrapText(line, width-2), "\n", "\n  ") + "\n\n")
	}

	m.helpVP.SetContent(strings.TrimRight(b.String(), "\n"))
}

func (m *Model) renderHelpPanel() string {
	height := m.height - headerHeight - statusHeight
	inner := max(10, m.width-4)
	title := m.panelHeading("Help", "↑/↓ scroll · ? or esc closes", inner)
	return renderPanel(m.styles.panelFocus, title, m.helpVP.View(), m.width, height)
}

// renderLog fills the log viewport with the command trace.
func (m *Model) renderLog() {
	lines := m.logSnapshot()
	if len(lines) == 0 {
		m.logVP.SetContent(m.styles.dim.Render(
			"Nothing has run yet. Every command tooln executes appears here, " +
				"with its output, so you can see exactly what happened."))
		return
	}
	width := max(20, m.logVP.Width)
	var b strings.Builder
	for _, l := range lines {
		style := m.styles.logLine
		switch {
		case strings.HasPrefix(l, "$ "):
			style = m.styles.cmdLine
		case strings.HasPrefix(l, "# "), strings.HasPrefix(l, "── "):
			style = m.styles.accent
		case strings.HasPrefix(l, "! "):
			style = m.styles.statusWarn
		case strings.HasPrefix(l, "> "):
			style = m.styles.accent
		case strings.Contains(l, "ERROR"), strings.Contains(l, "error:"):
			style = m.styles.statusErr
		}
		b.WriteString(style.Render(ellipsize(l, width)) + "\n")
	}
	m.logVP.SetContent(strings.TrimRight(b.String(), "\n"))
}

func (m *Model) renderLogPanel() string {
	inner := max(10, m.width-4)
	title := m.panelHeading("Command log", "the real commands being run · L hides this", inner)
	return renderPanel(m.styles.panel, title, m.logVP.View(), m.width, m.logHeight())
}

// wrapText hard-wraps text at width columns on word boundaries, breaking words
// that are longer than the line.
func wrapText(text string, width int) string {
	if width <= 0 {
		return text
	}
	var out []string
	for _, paragraph := range strings.Split(text, "\n") {
		if strings.TrimSpace(paragraph) == "" {
			out = append(out, "")
			continue
		}
		var line strings.Builder
		for _, word := range strings.Fields(paragraph) {
			switch {
			case line.Len() == 0:
				for lipgloss.Width(word) > width {
					cut := lipgloss.NewStyle().MaxWidth(width).Render(word)
					out = append(out, cut)
					word = word[len(cut):]
					if word == "" {
						break
					}
				}
				if word != "" {
					line.WriteString(word)
				}
			case lipgloss.Width(line.String())+1+lipgloss.Width(word) <= width:
				line.WriteString(" " + word)
			default:
				out = append(out, line.String())
				line.Reset()
				line.WriteString(word)
			}
		}
		if line.Len() > 0 {
			out = append(out, line.String())
		}
	}
	return strings.Join(out, "\n")
}
