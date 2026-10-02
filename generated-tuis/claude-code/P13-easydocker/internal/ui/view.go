package ui

import (
	"fmt"
	"strings"
	"time"

	"toolm/internal/docker"

	"github.com/charmbracelet/lipgloss"
)

// Layout constants. The chrome is one header line, one tab line, one status
// line and one key-hint line, leaving the rest of the terminal for the body.
const (
	headerLines = 2 // title + tab bar
	footerLines = 2 // status message + key hints
	minBodyRows = 3
	colGap      = 2
)

// bodyHeight is the number of rows available to the main content area.
func (m Model) bodyHeight() int {
	h := m.height - headerLines - footerLines
	if h < minBodyRows {
		return minBodyRows
	}
	return h
}

// contentWidth is the usable width, leaving a column for the scrollbar.
func (m Model) contentWidth() int {
	w := m.width
	if w < 20 {
		w = 20
	}
	return w
}

// listPageSize is the number of data rows visible in a list. The body holds
// the column header, a separator-free table and the counter line.
func (m Model) listPageSize() int {
	// body = header row + rows
	n := m.bodyHeight() - 1
	if n < 1 {
		return 1
	}
	return n
}

// detailPageSize is the number of detail rows visible at once.
func (m Model) detailPageSize() int {
	n := m.bodyHeight() - 1 // one line for the object title
	if n < 1 {
		return 1
	}
	return n
}

// logPageSize is the number of log lines visible at once.
func (m Model) logPageSize() int {
	n := m.bodyHeight() - 1 // one line for the log header
	if n < 1 {
		return 1
	}
	return n
}

// View implements tea.Model.
func (m Model) View() string {
	if m.quitting {
		return ""
	}
	if !m.ready {
		return "Starting toolm…"
	}

	var body string
	switch m.mode {
	case modeHelp:
		body = m.renderHelp()
	case modeLogs:
		body = m.renderLogs()
	case modeDetail:
		body = m.renderDetail()
	default:
		body = m.renderList()
	}

	return strings.Join([]string{
		m.renderHeader(),
		m.renderTabs(),
		body,
		m.renderStatus(),
		m.renderKeyHints(),
	}, "\n")
}

// spinnerFrame returns the current spinner glyph, or a blank when idle.
func (m Model) spinnerFrame() string {
	if m.loading <= 0 {
		return ""
	}
	frames := []rune{'⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'}
	return string(frames[m.spinnerStep%len(frames)])
}

// renderHeader draws the application title and connection information.
func (m Model) renderHeader() string {
	w := m.contentWidth()
	left := m.styles.Title.Render("toolm") + m.styles.TitleDim.Render(" · Docker management TUI")

	var right string
	info := []string{}
	if m.version != "" {
		info = append(info, "engine "+m.version)
	}
	info = append(info, m.endpoint)
	if sp := m.spinnerFrame(); sp != "" {
		info = append(info, m.styles.Spinner.Render(sp)+" working")
	} else if !m.lastRefresh.IsZero() {
		info = append(info, "updated "+m.lastRefresh.Format("15:04:05"))
	}
	right = m.styles.Muted.Render(strings.Join(info, "  ·  "))

	gapWidth := w - lipgloss.Width(left) - lipgloss.Width(right)
	if gapWidth < 1 {
		// Not enough room for the connection details; keep the title only.
		return truncate(left, w)
	}
	return left + strings.Repeat(" ", gapWidth) + right
}

// renderTabs draws the view switcher with its numeric shortcuts.
func (m Model) renderTabs() string {
	var parts []string
	for v := view(0); v < viewCount; v++ {
		label := fmt.Sprintf("%d %s (%d)", int(v)+1, v.title(), m.countFor(v))
		if _, bad := m.data.errs[v]; bad {
			label = fmt.Sprintf("%d %s (!)", int(v)+1, v.title())
		}
		if v == m.view && m.mode != modeHelp {
			parts = append(parts, m.styles.TabActive.Render(label))
		} else {
			parts = append(parts, m.styles.TabInactive.Render(label))
		}
	}
	if m.mode == modeHelp {
		parts = append(parts, m.styles.TabActive.Render("? Help"))
	}
	bar := joinHorizontalTop(parts...)
	return truncate(bar, m.contentWidth())
}

// countFor returns the number of items loaded for a view.
func (m Model) countFor(v view) int {
	switch v {
	case viewContainers:
		return len(m.data.containers)
	case viewImages:
		return len(m.data.images)
	case viewNetworks:
		return len(m.data.networks)
	case viewVolumes:
		return len(m.data.volumes)
	}
	return 0
}

// renderStatus draws the status message line, including the filter prompt.
func (m Model) renderStatus() string {
	w := m.contentWidth()
	ls := m.listConst()

	if m.mode == modeList && ls.filtering {
		prompt := m.styles.Accent.Render("/") + ls.filter + m.styles.Accent.Render("▌")
		hint := m.styles.Muted.Render(fmt.Sprintf("  %d of %d %ss match",
			m.filteredCount(), m.totalCount(), m.view.noun()))
		return truncate(prompt+hint, w)
	}

	icon, style := "•", m.styles.Muted
	switch m.statusLevel {
	case levelOK:
		icon, style = "✓", m.styles.OK
	case levelWarn:
		icon, style = "!", m.styles.Warn
	case levelError:
		icon, style = "✗", m.styles.Err
	}

	left := style.Render(icon+" ") + style.Render(m.statusText)

	// The right side carries the position indicator so it is always visible.
	right := m.styles.Faint.Render(m.positionLabel())
	gapWidth := w - lipgloss.Width(left) - lipgloss.Width(right)
	if gapWidth < 1 {
		return truncate(left, w)
	}
	return left + strings.Repeat(" ", gapWidth) + right
}

// positionLabel describes the current scroll position for the status bar.
func (m Model) positionLabel() string {
	switch m.mode {
	case modeLogs:
		total := len(m.renderedLogLines())
		if total == 0 {
			return "no log output"
		}
		last := clamp(m.logs.offset+m.logPageSize(), 0, total)
		return fmt.Sprintf("log lines %d-%d of %d", m.logs.offset+1, last, total)
	case modeDetail:
		total := len(m.detailLines())
		if total == 0 {
			return ""
		}
		last := clamp(m.detail.offset+m.detailPageSize(), 0, total)
		return fmt.Sprintf("fields %d-%d of %d", m.detail.offset+1, last, total)
	case modeHelp:
		return "press esc to return"
	default:
		n := m.filteredCount()
		if n == 0 {
			return "0 items"
		}
		label := fmt.Sprintf("%d of %d", m.listConst().cursor+1, n)
		if total := m.totalCount(); total != n {
			label += fmt.Sprintf(" (filtered from %d)", total)
		}
		opts := sortOptions[m.view]
		arrow := "↑"
		if m.listConst().sortDesc {
			arrow = "↓"
		}
		label += fmt.Sprintf(" · sort %s%s", opts[m.listConst().sortIdx].name, arrow)
		return label
	}
}

// renderKeyHints draws the compact binding reference at the bottom.
func (m Model) renderKeyHints() string {
	hints := shortHelpFor(m.mode, m.view, m.mode == modeList && m.listConst().filtering)
	var parts []string
	for _, h := range hints {
		parts = append(parts, m.styles.KeyCap.Render(h.keys)+m.styles.KeyDesc.Render(" "+h.desc))
	}
	line := strings.Join(parts, m.styles.Faint.Render("  ·  "))
	return truncate(line, m.contentWidth())
}

// --- list rendering ---

// renderList draws the table for the active view.
func (m Model) renderList() string {
	if err, ok := m.data.errs[m.view]; ok {
		return m.renderPlaceholder(fmt.Sprintf("Could not load %s from %s:", strings.ToLower(m.view.title()), m.endpoint),
			err.Error(),
			"Press r to retry, or ? for help.")
	}

	cols, rows := m.tableFor(m.view)
	if len(rows) == 0 {
		if m.totalCount() == 0 {
			if m.loading > 0 {
				return m.renderPlaceholder("Loading " + strings.ToLower(m.view.title()) + "…")
			}
			return m.renderPlaceholder(fmt.Sprintf("No %ss reported by the Docker endpoint.", m.view.noun()),
				m.endpoint,
				"Press r to reload.")
		}
		return m.renderPlaceholder(fmt.Sprintf("No %s matches %q.", m.view.noun(), m.listConst().filter),
			fmt.Sprintf("%d %ss are loaded.", m.totalCount(), m.view.noun()),
			"Press esc to clear the filter, / to edit it.")
	}

	// Reserve one cell for the selection marker gutter and one for the
	// scrollbar, so neither can push table content off the screen.
	width := m.contentWidth() - 2
	resolved := layoutColumns(cols, naturalWidths(cols, rows), width, colGap)

	page := m.listPageSize()
	ls := m.listConst()
	offset := clamp(ls.offset, 0, maxOffset(len(rows), page))

	var out []string
	header := renderRow(resolved, columnTitles(resolved), colGap)
	out = append(out, " "+m.styles.TableHeader.Render(pad(header, width))+" ")

	bars := scrollbar(len(rows), page, offset, page)
	for i := 0; i < page; i++ {
		idx := offset + i
		bar := ""
		if i < len(bars) {
			bar = m.styles.Scroll.Render(bars[i])
		}
		if idx >= len(rows) {
			out = append(out, pad("", width+1)+bar)
			continue
		}
		line := pad(renderRow(resolved, rows[idx], colGap), width)
		if idx == ls.cursor {
			// The selected row carries both a marker and a highlight, so the
			// selection is obvious even without colour support.
			out = append(out, m.styles.RowCursorMark.Render("▸")+
				m.styles.RowSelected.Render(line)+bar)
			continue
		}
		out = append(out, " "+m.styles.Row.Render(line)+bar)
	}
	return strings.Join(out, "\n")
}

// columnTitles extracts the header cells of resolved columns.
func columnTitles(cols []column) []string {
	titles := make([]string, len(cols))
	for i, c := range cols {
		titles[i] = c.title
	}
	return titles
}

// sortMarker appends an arrow to the header of the active sort column.
func (m Model) sortMarker(cols []column, colToSortIdx map[int]int) []column {
	ls := m.listConst()
	out := make([]column, len(cols))
	copy(out, cols)
	for colIdx, sortIdx := range colToSortIdx {
		if sortIdx != ls.sortIdx || colIdx >= len(out) {
			continue
		}
		if ls.sortDesc {
			out[colIdx].title += " ↓"
		} else {
			out[colIdx].title += " ↑"
		}
	}
	return out
}

// tableFor builds the columns and cell contents of a view. Every field the
// specification requires is part of the row, so a single screenshot of the
// list shows all of it at once.
func (m Model) tableFor(v view) ([]column, [][]string) {
	switch v {
	case viewContainers:
		cols := []column{
			{title: "NAME", weight: 3, min: 8},
			{title: "IMAGE", weight: 3, min: 8},
			{title: "STATE", weight: 0, min: 7},
			{title: "STATUS", weight: 2, min: 8},
			{title: "PORTS", weight: 2, min: 5},
			{title: "CONTAINER ID", weight: 0, min: 12},
		}
		cols = m.sortMarker(cols, map[int]int{0: 0, 1: 1, 2: 2})
		var rows [][]string
		for _, c := range m.filteredContainers() {
			state := c.StateLabel()
			rows = append(rows, []string{
				c.Name(),
				c.Image,
				state,
				dash(c.Status),
				dashEmpty(c.PortsString()),
				docker.ShortID(c.ID),
			})
		}
		return cols, rows

	case viewImages:
		cols := []column{
			{title: "REPOSITORY", weight: 4, min: 10},
			{title: "TAG", weight: 1, min: 6},
			{title: "SIZE", weight: 0, min: 10, right: true},
			{title: "IMAGE ID", weight: 0, min: 12},
			{title: "CREATED", weight: 1, min: 12},
		}
		cols = m.sortMarker(cols, map[int]int{0: 0, 1: 1, 2: 2, 4: 3})
		var rows [][]string
		for _, img := range m.filteredImages() {
			repo, tag := imageRepoTag(img)
			rows = append(rows, []string{
				repo,
				tag,
				docker.FormatSizeMB(img.SizeBytes()),
				docker.ShortID(img.ID),
				relativeUnix(img.Created),
			})
		}
		return cols, rows

	case viewNetworks:
		cols := []column{
			{title: "NAME", weight: 3, min: 8},
			{title: "DRIVER", weight: 1, min: 7},
			{title: "SCOPE", weight: 1, min: 6},
			{title: "NETWORK ID", weight: 0, min: 12},
			{title: "SUBNET", weight: 2, min: 8},
		}
		cols = m.sortMarker(cols, map[int]int{0: 0, 1: 1, 2: 2})
		var rows [][]string
		for _, n := range m.filteredNetworks() {
			rows = append(rows, []string{
				n.Name,
				n.DriverName(),
				dash(n.Scope),
				docker.ShortID(n.ID),
				dashEmpty(strings.Join(n.Subnets(), ", ")),
			})
		}
		return cols, rows

	default:
		cols := []column{
			{title: "NAME", weight: 3, min: 8},
			{title: "DRIVER", weight: 1, min: 7},
			{title: "MOUNTPOINT", weight: 4, min: 12},
			{title: "SCOPE", weight: 0, min: 6},
		}
		cols = m.sortMarker(cols, map[int]int{0: 0, 1: 1, 2: 2})
		var rows [][]string
		for _, vol := range m.filteredVolumes() {
			rows = append(rows, []string{
				vol.Name,
				vol.DriverName(),
				dash(vol.Mountpoint),
				dash(vol.Scope),
			})
		}
		return cols, rows
	}
}

// dashEmpty renders an em dash for empty optional columns.
func dashEmpty(s string) string {
	if strings.TrimSpace(s) == "" {
		return "—"
	}
	return s
}

// relativeUnix renders a creation timestamp compactly for list columns.
func relativeUnix(sec int64) string {
	if sec <= 0 {
		return "—"
	}
	return humanizeAge(time.Since(time.Unix(sec, 0)))
}

// renderPlaceholder centres an informational message in the body area.
func (m Model) renderPlaceholder(lines ...string) string {
	h := m.bodyHeight()
	w := m.contentWidth()
	var content []string
	for i, l := range lines {
		style := m.styles.FieldValue
		if i > 0 {
			style = m.styles.Muted
		}
		for _, wrapped := range wrap(l, w-4) {
			content = append(content, style.Render("  "+wrapped))
		}
	}
	top := (h - len(content)) / 2
	if top < 0 {
		top = 0
	}
	out := make([]string, 0, h)
	for i := 0; i < top; i++ {
		out = append(out, "")
	}
	out = append(out, content...)
	return strings.Join(fitLines(out, h), "\n")
}

// --- detail rendering ---

// detailLines renders the detail rows into display lines, wrapping long values
// so nothing is cut off. The result is what the scroll offset indexes into.
func (m Model) detailLines() []string {
	if m.detail.rows == nil && m.detail.err == nil {
		return nil
	}
	w := m.contentWidth() - 1

	keyWidth := 0
	for _, r := range m.detail.rows {
		if r.section {
			continue
		}
		if l := dispWidth(r.key); l > keyWidth {
			keyWidth = l
		}
	}
	keyWidth = clamp(keyWidth, 6, w/3)
	valWidth := w - keyWidth - 2
	if valWidth < 10 {
		valWidth = 10
	}

	var out []string
	if m.detail.err != nil {
		for _, l := range wrap("Inspect failed: "+m.detail.err.Error(), w) {
			out = append(out, m.styles.Err.Render(l))
		}
		if len(m.detail.rows) > 0 {
			out = append(out, "", m.styles.Muted.Render("Showing the fields already available from the list:"))
		}
	}

	for _, r := range m.detail.rows {
		switch {
		case r.section:
			out = append(out, m.styles.SectionHdr.Render(truncate(r.key, w)))
		case r.key == "" && r.value == "":
			out = append(out, "")
		default:
			wrapped := wrap(r.value, valWidth)
			for i, line := range wrapped {
				key := ""
				if i == 0 {
					key = r.key
				}
				out = append(out, "  "+m.styles.FieldKey.Render(pad(key, keyWidth))+" "+
					m.styles.FieldValue.Render(line))
			}
		}
	}
	return out
}

// renderDetail draws the detail pane with its own scrollbar.
func (m Model) renderDetail() string {
	lines := m.detailLines()
	page := m.detailPageSize()
	w := m.contentWidth() - 1
	offset := clamp(m.detail.offset, 0, maxOffset(len(lines), page))

	title := fmt.Sprintf("%s details · %s", m.detail.view.noun(), m.detail.title)
	out := []string{m.styles.PanelTitleActive.Render(truncate(strings.ToUpper(title[:1])+title[1:], w)) + " "}

	bars := scrollbar(len(lines), page, offset, page)
	for i := 0; i < page; i++ {
		idx := offset + i
		bar := ""
		if i < len(bars) {
			bar = m.styles.Scroll.Render(bars[i])
		}
		if idx >= len(lines) {
			out = append(out, pad("", w)+bar)
			continue
		}
		out = append(out, fitStyled(lines[idx], w)+bar)
	}
	return strings.Join(out, "\n")
}

// --- log rendering ---

// renderedLogLines returns the log lines after applying the wrap setting, so
// scroll positions match what is on screen.
func (m Model) renderedLogLines() []string {
	if !m.logs.wrap {
		return m.logs.lines
	}
	w := m.logContentWidth()
	var out []string
	for _, l := range m.logs.lines {
		if l == "" {
			out = append(out, "")
			continue
		}
		out = append(out, wrap(l, w)...)
	}
	return out
}

// logGutterWidth is the width of the line-number gutter.
func (m Model) logGutterWidth() int {
	n := len(m.logs.lines)
	digits := 1
	for n >= 10 {
		n /= 10
		digits++
	}
	if digits < 3 {
		digits = 3
	}
	return digits
}

// logContentWidth is the width available for log text.
func (m Model) logContentWidth() int {
	w := m.contentWidth() - 1 - m.logGutterWidth() - 1
	if w < 10 {
		w = 10
	}
	return w
}

// renderLogs draws the scrollable log viewer.
func (m Model) renderLogs() string {
	w := m.contentWidth() - 1
	page := m.logPageSize()

	head := fmt.Sprintf("Logs · %s", m.logs.containerName)
	if m.logs.containerID != "" {
		head += fmt.Sprintf(" (%s)", docker.ShortID(m.logs.containerID))
	}
	wrapState := "off"
	if m.logs.wrap {
		wrapState = "on"
	}
	head += fmt.Sprintf(" · %d lines · wrap %s", len(m.logs.lines), wrapState)
	out := []string{m.styles.PanelTitleActive.Render(truncate(head, w)) + " "}

	if m.logs.err != nil {
		body := m.renderPlaceholder("Could not read the logs of "+m.logs.containerName+":",
			m.logs.err.Error(), "Press r to retry or esc to go back.")
		return out[0] + "\n" + strings.Join(fitLines(strings.Split(body, "\n"), page), "\n")
	}
	if len(m.logs.lines) == 0 {
		body := m.renderPlaceholder("Container "+m.logs.containerName+" produced no log output.",
			"The Docker endpoint returned an empty log stream.",
			"Press r to re-read or esc to go back.")
		return out[0] + "\n" + strings.Join(fitLines(strings.Split(body, "\n"), page), "\n")
	}

	lines := m.renderedLogLines()
	offset := clamp(m.logs.offset, 0, maxOffset(len(lines), page))
	gutter := m.logGutterWidth()
	textWidth := m.logContentWidth()

	bars := scrollbar(len(lines), page, offset, page)
	for i := 0; i < page; i++ {
		idx := offset + i
		bar := ""
		if i < len(bars) {
			bar = m.styles.Scroll.Render(bars[i])
		}
		if idx >= len(lines) {
			out = append(out, pad("", w)+bar)
			continue
		}
		text := lines[idx]
		if !m.logs.wrap && m.logs.xOffset > 0 {
			text = shiftLeft(text, m.logs.xOffset)
		}
		num := m.styles.LogGutter.Render(padLeft(fmt.Sprintf("%d", idx+1), gutter))
		out = append(out, num+" "+m.styles.LogLine.Render(pad(text, textWidth))+bar)
	}
	return strings.Join(out, "\n")
}

// padLeft left-pads s to w cells.
func padLeft(s string, w int) string {
	if diff := w - dispWidth(s); diff > 0 {
		return strings.Repeat(" ", diff) + s
	}
	return truncate(s, w)
}

// shiftLeft drops the first n display cells of s, for horizontal scrolling.
func shiftLeft(s string, n int) string {
	if n <= 0 {
		return s
	}
	runes := []rune(s)
	consumed := 0
	for i, r := range runes {
		consumed += dispWidth(string(r))
		if consumed > n {
			return string(runes[i:])
		}
	}
	return ""
}

// --- help rendering ---

// helpLines renders the full key reference.
func (m Model) helpLines() []string {
	w := m.contentWidth() - 1
	keyWidth := 0
	for _, s := range helpSections {
		for _, k := range s.keys {
			if l := dispWidth(k.keys); l > keyWidth {
				keyWidth = l
			}
		}
	}
	keyWidth = clamp(keyWidth, 6, w/3)

	var out []string
	out = append(out,
		m.styles.SectionHdr.Render("toolm — keyboard reference"),
		m.styles.Muted.Render("Every action is reachable from the keyboard; no mouse required."),
		"",
	)
	for _, s := range helpSections {
		out = append(out, m.styles.PanelTitleActive.Render(s.title))
		for _, k := range s.keys {
			desc := wrap(k.desc, w-keyWidth-4)
			for i, d := range desc {
				key := ""
				if i == 0 {
					key = k.keys
				}
				out = append(out, "  "+m.styles.KeyCap.Render(padLeft(key, keyWidth))+"  "+
					m.styles.KeyDesc.Render(d))
			}
		}
		out = append(out, "")
	}
	out = append(out,
		m.styles.PanelTitleActive.Render("Views"),
		"  "+m.styles.KeyDesc.Render("Containers  name, image, state, status, ports and ID on one screen"),
		"  "+m.styles.KeyDesc.Render("Images      repository, tag, size in MB, image ID and age"),
		"  "+m.styles.KeyDesc.Render("Networks    name, driver, scope, ID and subnet"),
		"  "+m.styles.KeyDesc.Render("Volumes     name, driver, mountpoint and scope"),
		"",
		m.styles.Muted.Render("Connected to "+m.endpoint),
	)
	return out
}

// renderHelp draws the scrollable help screen.
func (m Model) renderHelp() string {
	lines := m.helpLines()
	page := m.bodyHeight()
	w := m.contentWidth() - 1
	offset := clamp(m.helpOffset, 0, maxOffset(len(lines), page))

	bars := scrollbar(len(lines), page, offset, page)
	var out []string
	for i := 0; i < page; i++ {
		idx := offset + i
		bar := ""
		if i < len(bars) {
			bar = m.styles.Scroll.Render(bars[i])
		}
		if idx >= len(lines) {
			out = append(out, pad("", w)+bar)
			continue
		}
		out = append(out, fitStyled(lines[idx], w)+bar)
	}
	return strings.Join(out, "\n")
}
