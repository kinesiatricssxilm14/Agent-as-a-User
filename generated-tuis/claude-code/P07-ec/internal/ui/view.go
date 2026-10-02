package ui

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/lipgloss"
	"github.com/charmbracelet/x/ansi"
	"github.com/mattn/go-runewidth"

	"github.com/toolg/toolg/internal/gitx"
)

// View renders the whole screen. Every view is composed as header, body and
// footer of exactly the window height, so the terminal never scrolls on its own
// and the layout stays stable.
func (m *Model) View() string {
	var body string
	switch m.view {
	case viewMerge:
		body = m.viewMerge()
	case viewHistory:
		body = m.viewHistory()
	case viewFiles:
		body = m.viewFiles()
	case viewHelp:
		body = m.viewHelp()
	}

	parts := []string{m.viewHeader(), body}
	if out := m.renderGitOutput(); out != "" {
		parts = append(parts, out)
	}
	if mod := m.renderModal(); mod != "" {
		parts = append(parts, mod)
	}
	parts = append(parts, m.viewFooter())
	return strings.Join(parts, "\n")
}

// --- header -----------------------------------------------------------------

// viewHeader shows the repository context: branch, merge state, open file and
// resolution progress. All of it stays visible in every view.
func (m *Model) viewHeader() string {
	t := m.theme
	left := t.title.Render("toolg")

	var bits []string
	if m.status != nil {
		bits = append(bits, t.crumb.Render("branch ")+t.accent.Render(m.status.Branch))
		if m.status.Merging {
			head := m.status.MergeHead
			if head == "" {
				head = "MERGE_HEAD"
			}
			bits = append(bits, t.badgeWarn.Render("MERGING ← "+head))
		} else {
			bits = append(bits, t.badgeInfo.Render("no merge in progress"))
		}
	}
	bits = append(bits, t.crumb.Render("file ")+t.bold.Render(m.relPath))

	// Progress is the number the user cares about most while working.
	if m.file != nil {
		total := len(m.file.Blocks)
		left0 := m.file.UnresolvedCount()
		done := total - left0
		switch {
		case total == 0:
			bits = append(bits, t.badgeOk.Render("no conflict markers"))
		case left0 == 0:
			bits = append(bits, t.badgeOk.Render(fmt.Sprintf("resolved %d/%d", done, total)))
		default:
			bits = append(bits, t.badgeErr.Render(fmt.Sprintf("resolved %d/%d", done, total)))
		}
	}
	if m.dirty {
		bits = append(bits, t.badgeWarn.Render("UNSAVED"))
	}

	line1 := left + " " + strings.Join(bits, t.faint.Render(" │ "))
	line1 = truncate(line1, m.width)

	// A second line names the sides so the panel colours have a legend, and
	// lists the other conflicted files for context.
	var legend []string
	legend = append(legend,
		lipgloss.NewStyle().Foreground(sideColor(sideOurs)).Bold(true).Render("■ ours = HEAD"),
		lipgloss.NewStyle().Foreground(sideColor(sideTheirs)).Bold(true).Render("■ theirs = incoming"),
	)
	if m.status != nil && len(m.status.Conflicted) > 1 {
		legend = append(legend, t.dim.Render(fmt.Sprintf("%d conflicted files (f)", len(m.status.Conflicted))))
	}
	line2 := t.faint.Render("  ") + strings.Join(legend, t.faint.Render("  "))
	return line1 + "\n" + truncate(line2, m.width)
}

// --- merge view -------------------------------------------------------------

// viewMerge renders the ours / [base] / result / theirs panels side by side.
//
// All panels are drawn from the same row list and share one vertical scroll
// offset, so a conflict's competing texts always appear on the same screen
// line. Nothing is hidden behind a tab or a page.
func (m *Model) viewMerge() string {
	t := m.theme
	h := m.mergeViewportHeight()

	if m.loadErr != nil {
		return m.renderNotice(h,
			t.err.Render("Cannot open "+m.relPath),
			"",
			t.dim.Render(m.loadErr.Error()),
			"",
			t.dim.Render("Press ")+t.keyName.Render("f")+t.dim.Render(" to list conflicted files, ")+
				t.keyName.Render("r")+t.dim.Render(" to retry, ")+
				t.keyName.Render("?")+t.dim.Render(" for help."))
	}
	if m.file == nil {
		return m.renderNotice(h, t.dim.Render("No file loaded."))
	}
	if len(m.file.Blocks) == 0 {
		// A file with no markers is a valid, common state: either it was never
		// conflicted or the merge is already resolved.
		lines := []string{
			t.ok.Render("No conflict markers in " + m.relPath),
			"",
			t.dim.Render("The file contains no <<<<<<< / ======= / >>>>>>> markers."),
		}
		if m.status != nil && m.status.Merging {
			lines = append(lines, "",
				t.dim.Render("A merge is in progress — press ")+t.keyName.Render("c")+
					t.dim.Render(" to complete the merge commit, or ")+
					t.keyName.Render("f")+t.dim.Render(" to pick another conflicted file."))
		} else {
			lines = append(lines, "",
				t.dim.Render("Press ")+t.keyName.Render("L")+t.dim.Render(" to view git history, ")+
					t.keyName.Render("?")+t.dim.Render(" for help."))
		}
		return m.renderNotice(h, lines...)
	}

	sides := m.visibleSides()
	widths := m.panelWidths(len(sides))

	cols := make([]string, len(sides))
	for i, s := range sides {
		cols[i] = m.renderPanel(s, widths[i], h)
	}
	return lipgloss.JoinHorizontal(lipgloss.Top, cols...)
}

// panelWidths splits the terminal width between panels, giving any remainder to
// the leftmost panels so the total exactly fills the window.
func (m *Model) panelWidths(n int) []int {
	// Each panel spends 2 columns on its border.
	avail := m.width
	if avail < n*8 {
		avail = n * 8
	}
	base := avail / n
	rem := avail - base*n
	w := make([]int, n)
	for i := range w {
		w[i] = base
		if i < rem {
			w[i]++
		}
	}
	return w
}

// renderPanel draws one side's column, including its title and border.
func (m *Model) renderPanel(s side, width, height int) string {
	t := m.theme
	inner := maxInt(4, width-2)

	// Title: name, git label and, for the result panel, the pending choice.
	title := m.panelTitle(s, inner)

	rows := make([]string, 0, height)
	for i := 0; i < height; i++ {
		idx := m.offset + i
		if m.lay == nil || idx >= len(m.lay.rows) {
			rows = append(rows, strings.Repeat(" ", inner))
			continue
		}
		rows = append(rows, m.renderRow(m.lay.rows[idx], s, inner, idx == m.cursor))
	}

	content := title + "\n" + strings.Join(rows, "\n")
	style := t.panel
	if s == m.focus {
		style = t.panelActive
	}
	return style.Width(inner).Render(content)
}

// panelTitle builds the heading for a panel, naming the git side it shows.
func (m *Model) panelTitle(s side, inner int) string {
	t := m.theme
	name := s.String()
	var detail string

	if m.file != nil && len(m.file.Blocks) > 0 {
		b := m.file.Blocks[0]
		switch s {
		case sideOurs:
			detail = b.OursLabel
		case sideTheirs:
			detail = b.TheirsLabel
		case sideBase:
			detail = b.BaseLabel
			if detail == "" {
				detail = "common ancestor"
			}
		}
	}
	switch s {
	case sideOurs:
		if detail == "" {
			detail = "HEAD"
		}
	case sideTheirs:
		if detail == "" && m.status != nil {
			detail = m.status.MergeHead
		}
	case sideResult:
		detail = "written to " + m.relPath
	}

	head := lipgloss.NewStyle().Foreground(sideColor(s)).Bold(true).Render(name)
	if detail != "" {
		head += t.dim.Render("  " + detail)
	}
	if s == m.focus {
		head += t.accent.Render("  ◀ focus")
	}
	line := truncate(head, inner)
	// A rule under the title separates it from content without spending a
	// second border.
	return line + "\n" + t.faint.Render(strings.Repeat("─", inner))
}

// renderRow renders one row of one panel, applying the conflict background
// highlight that distinguishes each side's text.
func (m *Model) renderRow(r row, s side, inner int, isCursor bool) string {
	t := m.theme

	// The gutter shows either the line number or a marker for filler rows.
	const gutterW = 5
	textW := maxInt(1, inner-gutterW-1)

	// Conflict header rows carry a label spanning the panel instead of content.
	if r.header {
		return m.renderHeaderRow(r, s, inner, isCursor)
	}

	c := r.cellFor(s)

	var gutter string
	if c.lineNo > 0 {
		g := fmt.Sprintf("%*d", gutterW, c.lineNo)
		if isCursor {
			gutter = t.gutterCur.Render(g)
		} else {
			gutter = t.gutter.Render(g)
		}
	} else {
		gutter = t.gutter.Render(strings.Repeat(" ", gutterW))
	}

	// Choose the body style. Conflict rows get a background per side; the
	// result panel is coloured by whether the block is resolved.
	var bodyStyle lipgloss.Style
	switch {
	case !r.conflict:
		bodyStyle = t.context
		if isCursor {
			bodyStyle = bodyStyle.Bold(true)
		}
	case s == sideResult:
		bodyStyle = t.resultStyle(r.resolved, isCursor)
	default:
		bodyStyle = t.sideStyle(s, isCursor)
	}

	// Filler rows keep panels aligned; a faint dash pattern shows they are not
	// real content rather than leaving a confusing blank.
	text := c.text
	if c.filler {
		if r.conflict {
			// Inside a conflict, a missing line on this side is meaningful:
			// this side simply has nothing here.
			body := strings.Repeat("·", textW)
			return gutter + " " + bodyStyle.Faint(true).Render(body)
		}
		text = ""
	}

	body := m.fitText(text, textW)
	return gutter + " " + bodyStyle.Render(body)
}

// renderHeaderRow draws the banner row that opens a conflict block.
func (m *Model) renderHeaderRow(r row, s side, inner int, isCursor bool) string {
	t := m.theme
	const gutterW = 5
	textW := maxInt(1, inner-gutterW-1)

	marker := "◆"
	if isCursor {
		marker = "▶"
	}
	var label string
	switch s {
	case sideOurs:
		label = fmt.Sprintf("%s conflict %d/%d — ours", marker, r.blockIndex+1, len(m.file.Blocks))
	case sideTheirs:
		label = fmt.Sprintf("%s conflict %d/%d — theirs", marker, r.blockIndex+1, len(m.file.Blocks))
	case sideBase:
		label = fmt.Sprintf("%s conflict %d/%d — base", marker, r.blockIndex+1, len(m.file.Blocks))
	case sideResult:
		label = fmt.Sprintf("%s conflict %d/%d — %s", marker, r.blockIndex+1, len(m.file.Blocks), r.choice)
	}

	var st lipgloss.Style
	if s == sideResult {
		st = t.resultStyle(r.resolved, isCursor).Bold(true)
	} else {
		st = t.sideStyle(s, isCursor).Bold(true)
	}
	gutter := t.gutter.Render(strings.Repeat(" ", gutterW))
	return gutter + " " + st.Render(m.fitText(label, textW))
}

// fitText applies horizontal scrolling and pads to an exact display width so
// background highlights form solid, aligned blocks.
func (m *Model) fitText(text string, w int) string {
	// Tabs would break column alignment, so render them as spaces.
	text = strings.ReplaceAll(text, "\t", "    ")
	// Strip control characters that would corrupt the display.
	text = sanitize(text)

	if m.hoff > 0 {
		text = runewidth.TruncateLeft(text, m.hoff, "")
	}
	return runewidth.FillRight(runewidth.Truncate(text, w, "›"), w)
}

// --- notices ----------------------------------------------------------------

// renderNotice centres a short message in the body area, framed like a panel so
// the screen keeps its shape.
func (m *Model) renderNotice(height int, lines ...string) string {
	box := lipgloss.JoinVertical(lipgloss.Left, lines...)
	inner := maxInt(4, m.width-2)
	return m.theme.panel.Width(inner).Height(height).Render(
		lipgloss.Place(inner, height, lipgloss.Center, lipgloss.Center, box),
	)
}

// --- history view -----------------------------------------------------------

// viewHistory lists real `git log` output. Every column of every visible entry
// is on screen at once; only vertical scrolling is used.
func (m *Model) viewHistory() string {
	t := m.theme
	h := m.listViewportHeight()
	inner := maxInt(4, m.width-2)

	if len(m.log) == 0 {
		return m.renderNotice(h,
			t.dim.Render("No commits yet in this repository."),
			"",
			t.dim.Render("Press ")+t.keyName.Render("m")+t.dim.Render(" to return to the merge view."))
	}

	head := t.panelTitle.Render("GIT HISTORY") +
		t.dim.Render(fmt.Sprintf("  %d commit(s), newest first", len(m.log)))
	rule := t.faint.Render(strings.Repeat("─", inner))

	rows := make([]string, 0, h)
	// Reserve the title and rule.
	avail := maxInt(1, h-2)
	for i := 0; i < avail; i++ {
		idx := m.logOffset + i
		if idx >= len(m.log) {
			rows = append(rows, strings.Repeat(" ", inner))
			continue
		}
		rows = append(rows, m.renderLogRow(m.log[idx], inner, idx == m.logCursor))
	}
	content := head + "\n" + rule + "\n" + strings.Join(rows, "\n")
	return t.panel.Width(inner).Render(content)
}

// renderLogRow renders one commit. Merge commits are flagged, since spotting
// the merge that was just created is the usual reason to open this view.
func (m *Model) renderLogRow(e gitx.LogEntry, inner int, selected bool) string {
	t := m.theme

	cursor := "  "
	if selected {
		cursor = t.accent.Render("▶ ")
	}
	kind := t.faint.Render("·")
	if e.Merge {
		kind = t.warn.Render("⑃")
	}

	hash := t.accent.Render(e.Short)
	when := t.dim.Render(e.When)
	author := t.faint.Render(truncate(e.Author, 14))

	subject := e.Subject
	// The fixed-width columns plus their separating spaces.
	used := 2 + 1 + 1 + len(e.Short) + 1 + len(e.When) + 1 + runewidth.StringWidth(truncate(e.Author, 14)) + 4
	subjW := maxInt(8, inner-used)

	subjStyle := t.context
	if selected {
		subjStyle = t.bold
	}
	line := cursor + kind + " " + hash + " " + when + " " + author + "  " +
		subjStyle.Render(runewidth.Truncate(sanitize(subject), subjW, "…"))

	// Ref decorations (branch and tag names) matter for orientation.
	if e.Refs != "" {
		line += " " + t.warn.Render("("+truncate(e.Refs, 30)+")")
	}
	return truncate(line, inner)
}

// --- conflicted files view --------------------------------------------------

// viewFiles lists every unmerged path git reports, so a multi-file conflict can
// be worked through without leaving the tool.
func (m *Model) viewFiles() string {
	t := m.theme
	h := m.listViewportHeight()
	inner := maxInt(4, m.width-2)

	if len(m.conflictFiles) == 0 {
		lines := []string{t.ok.Render("No conflicted files.")}
		if m.status != nil && m.status.Merging {
			lines = append(lines, "",
				t.dim.Render("All conflicts are resolved in the index — press ")+
					t.keyName.Render("c")+t.dim.Render(" to commit the merge."))
		} else {
			lines = append(lines, "", t.dim.Render("No merge is in progress."))
		}
		lines = append(lines, "",
			t.dim.Render("Press ")+t.keyName.Render("m")+t.dim.Render(" for the merge view, ")+
				t.keyName.Render("r")+t.dim.Render(" to refresh."))
		return m.renderNotice(h, lines...)
	}

	head := t.panelTitle.Render("CONFLICTED FILES") +
		t.dim.Render(fmt.Sprintf("  %d unmerged path(s) — enter to open", len(m.conflictFiles)))
	rule := t.faint.Render(strings.Repeat("─", inner))

	rows := make([]string, 0, h)
	avail := maxInt(1, h-2)
	for i := 0; i < avail; i++ {
		if i >= len(m.conflictFiles) {
			rows = append(rows, strings.Repeat(" ", inner))
			continue
		}
		p := m.conflictFiles[i]
		cursor := "  "
		if i == m.filesCursor {
			cursor = t.accent.Render("▶ ")
		}
		name := t.context.Render(p)
		if i == m.filesCursor {
			name = t.bold.Render(p)
		}
		mark := ""
		if p == m.relPath {
			mark = t.ok.Render("  ← open")
		}
		rows = append(rows, truncate(cursor+t.badgeErr.Render("UU")+" "+name+mark, inner))
	}
	return t.panel.Width(inner).Render(head + "\n" + rule + "\n" + strings.Join(rows, "\n"))
}

// --- help view --------------------------------------------------------------

// viewHelp documents every binding, grouped by purpose, so no shortcut has to
// be guessed or looked up outside the tool.
func (m *Model) viewHelp() string {
	t := m.theme
	h := m.listViewportHeight()
	inner := maxInt(4, m.width-2)

	var lines []string
	add := func(s string) { lines = append(lines, truncate(s, inner)) }

	add(t.panelTitle.Render("KEYBOARD REFERENCE") +
		t.dim.Render("  ↑/↓ scroll · esc or ? to close"))
	add(t.faint.Render(strings.Repeat("─", inner)))

	// Two-column key list per section keeps the whole reference compact enough
	// to read without much scrolling.
	for _, sec := range m.keys.sections() {
		add(t.accent.Render(sec.Title))
		for _, b := range sec.Keys {
			hk := b.Help()
			add("   " + t.keyName.Render(padRight(hk.Key, 12)) + t.keyDesc.Render(hk.Desc))
		}
		add("")
	}

	add(t.accent.Render("Resolution strategies"))
	add(t.keyDesc.Render("   ours   keep the HEAD side only (the branch you are merging into)"))
	add(t.keyDesc.Render("   theirs keep the incoming side only (the branch being merged)"))
	add(t.keyDesc.Render("   both   keep ours first, then theirs"))
	add(t.keyDesc.Render("   none   drop both sides, removing the block entirely"))
	add(t.keyDesc.Render("   edit   type the resolved text by hand (ctrl+s applies, esc cancels)"))
	add("")
	add(t.accent.Render("Workflow"))
	add(t.keyDesc.Render("   1. n / p to move between conflicts"))
	add(t.keyDesc.Render("   2. o / t / b / d to choose a strategy for the current one"))
	add(t.keyDesc.Render("   3. s to write the file back (conflict markers are removed)"))
	add(t.keyDesc.Render("   4. c to stage the file and create the merge commit"))
	add(t.keyDesc.Render("   5. L to review the resulting git history"))
	add("")
	add(t.accent.Render("Colour key"))
	add("   " + m.theme.sideStyle(sideOurs, true).Render(" ours text ") + t.keyDesc.Render("  conflict content from HEAD"))
	add("   " + m.theme.sideStyle(sideTheirs, true).Render(" theirs text ") + t.keyDesc.Render("  conflict content from the incoming branch"))
	add("   " + m.theme.resultStyle(true, true).Render(" resolved ") + t.keyDesc.Render("  a decision has been made for this block"))
	add("   " + m.theme.resultStyle(false, true).Render(" unresolved ") + t.keyDesc.Render("  still needs a choice; markers stay on save"))

	// Clamp scrolling so the last page cannot be scrolled past.
	avail := maxInt(1, h)
	maxOff := maxInt(0, len(lines)-avail)
	if m.helpOffset > maxOff {
		m.helpOffset = maxOff
	}
	end := minInt(len(lines), m.helpOffset+avail)
	page := lines[m.helpOffset:end]
	for len(page) < avail {
		page = append(page, "")
	}
	return t.panel.Width(inner).Render(strings.Join(page, "\n"))
}

// --- git output -------------------------------------------------------------

// renderGitOutput echoes the last git command's own output, so the user sees
// git's real words rather than a paraphrase.
func (m *Model) renderGitOutput() string {
	if m.gitOutput == "" {
		return ""
	}
	t := m.theme
	all := strings.Split(strings.TrimSpace(m.gitOutput), "\n")
	if len(all) > 3 {
		all = all[:3]
	}
	out := make([]string, 0, len(all))
	for _, l := range all {
		out = append(out, truncate(t.faint.Render("git │ ")+t.dim.Render(sanitize(l)), m.width))
	}
	return strings.Join(out, "\n")
}

// --- modals -----------------------------------------------------------------

// renderModal draws the active input overlay as a bar beneath the panels. It is
// deliberately small: the merge content above stays visible while typing.
func (m *Model) renderModal() string {
	t := m.theme
	inner := maxInt(4, m.width-2)

	switch m.modal {
	case modalCommit:
		label := t.prompt.Render("commit message")
		hint := t.keyDesc.Render("enter commit · esc cancel")
		return t.panelActive.Width(inner).Render(
			label + " " + hint + "\n" + m.commitInput.View())

	case modalConfirmAbort:
		warn := t.badgeErr.Render("ABORT MERGE")
		text := t.context.Render(" Discard the merge and restore the pre-merge working tree?")
		hint := t.keyDesc.Render(" y/enter confirm · n/esc cancel")
		return t.panelActive.Width(inner).Render(warn + text + "\n" + hint)

	case modalEditBlock:
		label := t.prompt.Render(fmt.Sprintf("edit conflict %d", m.editingBlock+1))
		hint := t.keyDesc.Render("ctrl+s apply · esc cancel · enter newline")
		return t.panelActive.Width(inner).Render(
			label + " " + hint + "\n" + m.blockEdit.View())
	}
	return ""
}

// --- footer -----------------------------------------------------------------

// viewFooter shows the status message and the key hints for the current view,
// which is what makes the bindings discoverable without opening help.
func (m *Model) viewFooter() string {
	t := m.theme

	// Line 1: the most recent operation's outcome, or an orientation hint.
	var msg string
	switch {
	case m.statusMsg != "":
		switch m.statusKind {
		case statusSuccess:
			msg = t.statusOk.Render("✓ " + m.statusMsg)
		case statusWarning:
			msg = t.statusWarn.Render("! " + m.statusMsg)
		case statusError:
			msg = t.statusErr.Render("✗ " + m.statusMsg)
		default:
			msg = t.dim.Render("• " + m.statusMsg)
		}
	default:
		msg = t.dim.Render("• " + m.defaultHint())
	}

	// Line 2: compact key hints.
	return truncate(msg, m.width) + "\n" + m.renderHints()
}

// renderHints lays out the footer key hints so they fit the window width.
//
// If the full list is too wide, hints are dropped from the end while the help
// and quit pair is kept: losing a hint is acceptable, but losing the pointer to
// the full reference would make the rest undiscoverable.
func (m *Model) renderHints() string {
	t := m.theme
	hints := m.keys.footerHints(m.view)
	if len(hints) == 0 {
		return ""
	}

	render := func(bs []key.Binding) string {
		parts := make([]string, 0, len(bs))
		for _, b := range bs {
			hk := b.Help()
			parts = append(parts, t.keyName.Render(hk.Key)+t.keyDesc.Render(" "+hk.Desc))
		}
		return strings.Join(parts, t.faint.Render(" · "))
	}

	if line := render(hints); lipgloss.Width(line) <= m.width {
		return line
	}

	// footerHints always ends with help and quit; keep that pair and trim the
	// body in front of it.
	tail := hints[maxInt(0, len(hints)-2):]
	body := hints[:maxInt(0, len(hints)-2)]
	for n := len(body); n > 0; n-- {
		trial := append(append([]key.Binding{}, body[:n]...), tail...)
		if line := render(trial); lipgloss.Width(line) <= m.width {
			return line
		}
	}
	return truncate(render(tail), m.width)
}

// defaultHint suggests the next useful action based on real current state.
func (m *Model) defaultHint() string {
	switch m.view {
	case viewHistory:
		return "git history — press m to go back to the merge view"
	case viewFiles:
		return "conflicted files — enter opens the highlighted file"
	case viewHelp:
		return "keyboard reference — press esc or ? to close"
	}
	if m.file == nil {
		return "no file open — press f to list conflicted files"
	}
	switch {
	case len(m.file.Blocks) == 0:
		return "no conflict markers in this file"
	case m.file.UnresolvedCount() > 0:
		return fmt.Sprintf("%d conflict(s) to resolve — o ours · t theirs · b both · d discard",
			m.file.UnresolvedCount())
	case m.dirty:
		return "all conflicts resolved — press s to write the file back"
	default:
		return "file saved and fully resolved — press c to create the merge commit"
	}
}

// --- text helpers -----------------------------------------------------------

// truncate cuts an already-styled string to a display width.
//
// runewidth would count the ANSI escape bytes lipgloss inserts as visible
// columns and cut far too early, so this uses the escape-aware truncation and
// keeps styling intact.
func truncate(s string, w int) string {
	if w <= 0 {
		return ""
	}
	if lipgloss.Width(s) <= w {
		return s
	}
	return ansi.Truncate(s, w, "")
}

func padRight(s string, w int) string {
	if runewidth.StringWidth(s) >= w {
		return s + " "
	}
	return s + strings.Repeat(" ", w-runewidth.StringWidth(s))
}

// sanitize removes control characters that would corrupt the terminal, while
// keeping printable text (including wide and combining runes) intact.
func sanitize(s string) string {
	if !strings.ContainsFunc(s, isControl) {
		return s
	}
	var b strings.Builder
	b.Grow(len(s))
	for _, r := range s {
		if isControl(r) {
			b.WriteRune('·')
			continue
		}
		b.WriteRune(r)
	}
	return b.String()
}

func isControl(r rune) bool {
	return r < 0x20 || r == 0x7f
}
