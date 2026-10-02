package ui

import (
	"context"
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/key"
	tea "github.com/charmbracelet/bubbletea"

	"tooln/internal/pkgmgr"
)

// handleKey dispatches a key press to whichever mode is active. Text entry
// modes get first refusal on the key so that typing "q" into a search box does
// not quit the program.
func (m *Model) handleKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch m.mode {
	case modeFilter, modeSearch, modeInstall:
		return m.handleTextKey(msg)
	case modeConfirm:
		return m.handleConfirmKey(msg)
	case modeMenu:
		return m.handleMenuKey(msg)
	}
	return m.handleBrowseKey(msg)
}

// ------------------------------------------------------------- text entry ----

func (m *Model) handleTextKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "esc":
		switch m.mode {
		case modeFilter:
			// Abandoning a filter restores the list to how it looked before.
			if t := m.cur(); t != nil {
				t.filter = ""
				t.applyFilter()
				t.ensureVisible(m.listHeight())
			}
			m.setStatus(statusInfo, "Filter cleared")
		default:
			m.setStatus(statusInfo, "Cancelled")
		}
		m.mode = modeBrowse
		m.input.Blur()
		m.layout()
		return m, m.ensureDetails()

	case "enter":
		return m.commitText()

	case "ctrl+c":
		return m, tea.Quit
	}

	prev := m.input.Value()
	var cmd tea.Cmd
	m.input, cmd = m.input.Update(msg)

	// The filter is live: every keystroke narrows the list immediately.
	if m.mode == modeFilter && m.input.Value() != prev {
		if t := m.cur(); t != nil {
			t.filter = m.input.Value()
			t.applyFilter()
			t.ensureVisible(m.listHeight())
			m.describeFilter(t)
		}
		return m, tea.Batch(cmd, m.ensureDetails())
	}
	return m, cmd
}

// commitText acts on the text the user typed.
func (m *Model) commitText() (tea.Model, tea.Cmd) {
	value := strings.TrimSpace(m.input.Value())
	mode := m.mode
	t := m.cur()

	m.mode = modeBrowse
	m.input.Blur()
	m.layout()

	switch mode {
	case modeFilter:
		if t == nil {
			return m, nil
		}
		t.filter = value
		t.applyFilter()
		t.ensureVisible(m.listHeight())
		m.describeFilter(t)
		return m, m.ensureDetails()

	case modeSearch:
		if t == nil || value == "" {
			m.setStatus(statusInfo, "Search cancelled")
			return m, nil
		}
		m.setStatus(statusBusy, "Searching the %s index for %q…", t.mgr.ID(), value)
		return m, m.startSearch(t, value)

	case modeInstall:
		if t == nil || value == "" {
			m.setStatus(statusInfo, "Install cancelled")
			return m, nil
		}
		return m, m.prepareInstall(t, value)
	}
	return m, nil
}

func (m *Model) describeFilter(t *tabState) {
	switch {
	case t.filter == "":
		m.setStatus(statusInfo, "Showing all %d package(s)", len(t.rows))
	case len(t.rows) == 0:
		m.setStatus(statusWarning, "No package matches %q — press esc to clear the filter", t.filter)
	default:
		m.setStatus(statusGood, "%d package(s) match %q", len(t.rows), t.filter)
	}
}

// prepareInstall builds the install plan and asks for confirmation, showing the
// exact command that will run.
func (m *Model) prepareInstall(t *tabState, spec string) tea.Cmd {
	plan, err := t.mgr.InstallPlan(context.Background(), spec)
	if err != nil {
		m.setStatus(statusBad, "Cannot install %q: %v", spec, err)
		return nil
	}
	lines := []string{
		fmt.Sprintf("Install %s with %s.", quote(spec), t.mgr.ID()),
		"",
		"Commands that will run:",
	}
	lines = append(lines, planLines(plan)...)
	lines = append(lines,
		"",
		"Its direct dependencies will be installed too, and will be listed in the",
		"details pane once the rescan finishes.")

	m.confirm = &pendingConfirm{
		kind:    confirmInstall,
		title:   "Install package",
		lines:   lines,
		targets: []string{spec},
		plan:    plan,
		mgr:     t.mgr,
		op:      opInstall,
	}
	m.mode = modeConfirm
	m.setStatus(statusInfo, "Press enter to install %s, or esc to cancel", spec)
	m.layout()
	return nil
}

// -------------------------------------------------------------- confirming ----

func (m *Model) handleConfirmKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "enter", "y", "Y":
		c := m.confirm
		m.confirm = nil
		m.mode = modeBrowse
		m.layout()
		if c == nil {
			return m, nil
		}
		if c.kind == confirmQuit {
			return m, tea.Quit
		}
		return m, m.runPlan(c)

	case "esc", "n", "N", "q":
		m.confirm = nil
		m.mode = modeBrowse
		m.layout()
		m.setStatus(statusInfo, "Cancelled — nothing was changed")
		return m, nil

	case "ctrl+c":
		return m, tea.Quit

	case "L":
		m.showLog = !m.showLog
		m.layout()
		return m, nil
	}
	return m, nil
}

// runPlan starts a confirmed plan.
func (m *Model) runPlan(c *pendingConfirm) tea.Cmd {
	m.busy++
	m.mutating = c.kind != confirmExtra
	m.busyLabel = c.plan.Title
	m.showLog = true
	m.layout()
	m.setStatus(statusBusy, "Running: %s", c.plan.Title)
	m.appendLog("")
	m.appendLog("# " + c.plan.Title)
	return runPlanCmd(c.mgr, m.Logger(), c.op, c.plan, c.targets)
}

// ------------------------------------------------------------------- menu ----

func (m *Model) handleMenuKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch {
	case msg.String() == "esc" || msg.String() == "q":
		m.mode = modeBrowse
		m.layout()
		m.setStatus(statusInfo, "Menu closed")
		return m, nil

	case msg.String() == "ctrl+c":
		return m, tea.Quit

	case key.Matches(msg, m.keys.Up):
		if m.menuIndex > 0 {
			m.menuIndex--
		}
		return m, nil

	case key.Matches(msg, m.keys.Down):
		if m.menuIndex < len(m.menuItems)-1 {
			m.menuIndex++
		}
		return m, nil

	case msg.String() == "enter":
		return m, m.chooseMenuItem(m.menuIndex)
	}

	// A direct key from the menu listing selects that entry.
	for i, item := range m.menuItems {
		if item.Key == msg.String() {
			return m, m.chooseMenuItem(i)
		}
	}
	return m, nil
}

func (m *Model) chooseMenuItem(idx int) tea.Cmd {
	if idx < 0 || idx >= len(m.menuItems) {
		return nil
	}
	item := m.menuItems[idx]
	t := m.cur()
	m.mode = modeBrowse
	m.layout()
	if t == nil {
		return nil
	}
	lines := []string{item.Title, "", "Commands that will run:"}
	lines = append(lines, planLines(item.Plan)...)
	m.confirm = &pendingConfirm{
		kind:  confirmExtra,
		title: "Maintenance",
		lines: lines,
		plan:  item.Plan,
		mgr:   t.mgr,
		op:    opExtra,
	}
	m.mode = modeConfirm
	m.setStatus(statusInfo, "Press enter to run, or esc to cancel")
	m.layout()
	return nil
}

// ----------------------------------------------------------------- browse ----

func (m *Model) handleBrowseKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	// Help and the log are readable overlays: while help is open its keys win,
	// so the arrow keys scroll the help text rather than the package list.
	if m.showHelp {
		switch {
		case key.Matches(msg, m.keys.Help), msg.String() == "esc", msg.String() == "q":
			m.showHelp = false
			m.layout()
			return m, nil
		case msg.String() == "ctrl+c":
			return m, tea.Quit
		default:
			var cmd tea.Cmd
			m.helpVP, cmd = m.helpVP.Update(msg)
			return m, cmd
		}
	}

	t := m.cur()

	switch {
	case key.Matches(msg, m.keys.Quit):
		return m, tea.Quit

	case key.Matches(msg, m.keys.Help):
		m.showHelp = true
		m.layout()
		m.renderHelp()
		m.setStatus(statusInfo, "Help — scroll with ↑/↓, close with ? or esc")
		return m, nil

	case key.Matches(msg, m.keys.ToggleLog):
		m.showLog = !m.showLog
		m.layout()
		if m.showLog {
			m.renderLog()
			m.logVP.GotoBottom()
			m.setStatus(statusInfo, "Command log shown — every command tooln runs appears here")
		} else {
			m.setStatus(statusInfo, "Command log hidden")
		}
		return m, m.ensureDetails()

	case key.Matches(msg, m.keys.NextTab):
		return m, m.switchTab(m.active + 1)

	case key.Matches(msg, m.keys.PrevTab):
		return m, m.switchTab(m.active - 1)

	case key.Matches(msg, m.keys.PipTab):
		return m, m.switchToID("pip")

	case key.Matches(msg, m.keys.AptTab):
		return m, m.switchToID("apt")

	case key.Matches(msg, m.keys.FocusNext):
		// h/l and the arrow keys move focus; when the details pane has focus the
		// vertical keys scroll it instead of the list.
		switch msg.String() {
		case "right", "l":
			m.focusDetails = true
		case "left", "h":
			m.focusDetails = false
		}
		return m, nil
	}

	if t == nil || !t.available() {
		// With no usable manager there is nothing else to act on.
		return m, nil
	}

	switch {
	case key.Matches(msg, m.keys.Up):
		if m.focusDetails {
			t.vp.LineUp(1)
			return m, nil
		}
		t.moveCursor(-1)
		t.ensureVisible(m.listHeight())
		return m, m.ensureDetails()

	case key.Matches(msg, m.keys.Down):
		if m.focusDetails {
			t.vp.LineDown(1)
			return m, nil
		}
		t.moveCursor(1)
		t.ensureVisible(m.listHeight())
		return m, m.ensureDetails()

	case key.Matches(msg, m.keys.PageUp):
		if m.focusDetails {
			t.vp.ViewUp()
			return m, nil
		}
		t.moveCursor(-max(1, m.listHeight()-1))
		t.ensureVisible(m.listHeight())
		return m, m.ensureDetails()

	case key.Matches(msg, m.keys.PageDown):
		if m.focusDetails {
			t.vp.ViewDown()
			return m, nil
		}
		t.moveCursor(max(1, m.listHeight()-1))
		t.ensureVisible(m.listHeight())
		return m, m.ensureDetails()

	case key.Matches(msg, m.keys.Home):
		if m.focusDetails {
			t.vp.GotoTop()
			return m, nil
		}
		t.cursor, t.top = 0, 0
		return m, m.ensureDetails()

	case key.Matches(msg, m.keys.End):
		if m.focusDetails {
			t.vp.GotoBottom()
			return m, nil
		}
		t.cursor = max(0, len(t.rows)-1)
		t.ensureVisible(m.listHeight())
		return m, m.ensureDetails()

	case key.Matches(msg, m.keys.ScrollDetailsDown):
		t.vp.LineDown(3)
		return m, nil

	case key.Matches(msg, m.keys.ScrollDetailsUp):
		t.vp.LineUp(3)
		return m, nil

	case key.Matches(msg, m.keys.Filter):
		m.mode = modeFilter
		m.input.SetValue(t.filter)
		m.input.CursorEnd()
		m.input.Placeholder = "type to narrow the list"
		m.input.Focus()
		m.layout()
		m.setStatus(statusInfo, "Filtering the list — enter keeps it, esc clears it")
		return m, nil

	case key.Matches(msg, m.keys.Search):
		m.mode = modeSearch
		m.input.SetValue("")
		m.input.Placeholder = "name or keyword to look for in the " + t.mgr.ID() + " index"
		m.input.Focus()
		m.layout()
		m.setStatus(statusInfo, "Searching the %s index — enter runs the search, esc cancels", t.mgr.ID())
		return m, nil

	case key.Matches(msg, m.keys.Clear):
		return m, m.clearView(t)

	case key.Matches(msg, m.keys.Installed):
		t.installedOnly = !t.installedOnly
		t.applyFilter()
		t.ensureVisible(m.listHeight())
		if t.installedOnly {
			m.setStatus(statusInfo, "Showing installed packages only (%d) — press a to show everything", len(t.rows))
		} else {
			m.setStatus(statusInfo, "Showing all %d row(s) — press a for installed only", len(t.rows))
		}
		return m, m.ensureDetails()

	case key.Matches(msg, m.keys.Outdated):
		m.busy++
		m.busyLabel = "checking for " + t.mgr.ID() + " updates"
		m.setStatus(statusBusy, "Asking %s which packages have a newer version…", t.mgr.ID())
		return m, outdatedCmd(t.mgr)

	case key.Matches(msg, m.keys.Refresh):
		t.invalidate()
		if t.source == sourceSearch && t.query != "" {
			m.setStatus(statusBusy, "Re-running the search for %q…", t.query)
			return m, m.startSearch(t, t.query)
		}
		m.setStatus(statusBusy, "Rescanning the %s environment…", t.mgr.ID())
		return m, m.startList(t)

	case key.Matches(msg, m.keys.Mark):
		cur, ok := t.current()
		if !ok {
			return m, nil
		}
		if t.marks[cur.Name] {
			delete(t.marks, cur.Name)
		} else {
			t.marks[cur.Name] = true
		}
		t.moveCursor(1)
		t.ensureVisible(m.listHeight())
		m.setStatus(statusInfo, "%d package(s) marked — d uninstalls every marked package", len(t.marks))
		return m, m.ensureDetails()

	case key.Matches(msg, m.keys.Unmark):
		n := len(t.marks)
		t.marks = map[string]bool{}
		m.setStatus(statusInfo, "Cleared %d mark(s)", n)
		return m, nil

	case key.Matches(msg, m.keys.CopyName):
		cur, ok := t.current()
		if !ok {
			return m, nil
		}
		m.setStatus(statusInfo, "%s %s  (%s)", cur.Name, cur.Version, t.mgr.ID())
		return m, nil

	case key.Matches(msg, m.keys.Extras):
		m.menuItems = t.mgr.Extras()
		if len(m.menuItems) == 0 {
			m.setStatus(statusInfo, "%s has no maintenance actions", t.mgr.ID())
			return m, nil
		}
		m.menuIndex = 0
		m.mode = modeMenu
		m.layout()
		m.setStatus(statusInfo, "Maintenance menu — ↑/↓ then enter, or press the key shown; esc closes")
		return m, nil

	case key.Matches(msg, m.keys.Install):
		if m.refuseWhileBusy() {
			return m, nil
		}
		m.mode = modeInstall
		m.input.SetValue("")
		if cur, ok := t.current(); ok && !cur.Installed {
			// Pre-fill the highlighted search result: installing what you are
			// looking at is the common case.
			m.input.SetValue(cur.Name)
			m.input.CursorEnd()
		}
		m.input.Placeholder = t.mgr.SpecHint()
		m.input.Focus()
		m.layout()
		m.setStatus(statusInfo, "Package to install with %s — enter confirms, esc cancels", t.mgr.ID())
		return m, nil

	case key.Matches(msg, m.keys.Remove):
		if m.refuseWhileBusy() {
			return m, nil
		}
		return m, m.prepareRemove(t)

	case key.Matches(msg, m.keys.Upgrade):
		if m.refuseWhileBusy() {
			return m, nil
		}
		return m, m.prepareUpgrade(t)
	}

	// Anything else scrolls the log when it is the visible extra pane.
	if m.showLog {
		var cmd tea.Cmd
		m.logVP, cmd = m.logVP.Update(msg)
		return m, cmd
	}
	return m, nil
}

// refuseWhileBusy blocks a second environment-changing action while one is
// already running: two package managers writing at once would corrupt state.
func (m *Model) refuseWhileBusy() bool {
	if m.mutating {
		m.setStatus(statusWarning, "Wait for the running operation to finish first (%s)", m.busyLabel)
		return true
	}
	return false
}

// clearView undoes the filter, then the search, then the marks — one step per
// press, so esc always has an obvious effect.
func (m *Model) clearView(t *tabState) tea.Cmd {
	switch {
	case t.filter != "":
		t.filter = ""
		t.applyFilter()
		t.ensureVisible(m.listHeight())
		m.setStatus(statusInfo, "Filter cleared — showing %d package(s)", len(t.rows))
		return m.ensureDetails()

	case t.installedOnly:
		t.installedOnly = false
		t.applyFilter()
		t.ensureVisible(m.listHeight())
		m.setStatus(statusInfo, "Showing all %d row(s)", len(t.rows))
		return m.ensureDetails()

	case t.source == sourceSearch:
		m.setStatus(statusBusy, "Leaving the search results; rereading installed %s packages…", t.mgr.ID())
		t.cursor, t.top = 0, 0
		return m.startList(t)

	case len(t.marks) > 0:
		t.marks = map[string]bool{}
		m.setStatus(statusInfo, "Marks cleared")
		return nil

	case m.showLog:
		m.showLog = false
		m.layout()
		m.setStatus(statusInfo, "Command log hidden")
		return nil
	}
	m.setStatus(statusInfo, "Nothing to clear — press ? for help, q to quit")
	return nil
}

func (m *Model) switchTab(idx int) tea.Cmd {
	if len(m.tabs) == 0 {
		return nil
	}
	idx = ((idx % len(m.tabs)) + len(m.tabs)) % len(m.tabs)
	if idx == m.active {
		return nil
	}
	m.active = idx
	m.focusDetails = false
	t := m.cur()
	m.layout()

	switch {
	case t.probe == probePending:
		m.setStatus(statusBusy, "Still checking whether %s can be used here…", t.mgr.ID())
		return nil
	case t.probe == probeFailed:
		m.setStatus(statusBad, "%s is not usable here: %s", t.mgr.ID(), t.probeErr)
		return nil
	}
	// A list may already be in flight from the initial probe; asking again would
	// run the package manager twice for no reason.
	if !t.loaded && !t.listing {
		m.setStatus(statusBusy, "Reading %s packages…", t.mgr.ID())
		return m.startList(t)
	}
	if !t.loaded {
		m.setStatus(statusBusy, "Reading %s packages…", t.mgr.ID())
		return nil
	}
	m.describeTab(t)
	return m.ensureDetails()
}

func (m *Model) switchToID(id string) tea.Cmd {
	for i, t := range m.tabs {
		if t.mgr.ID() == id {
			return m.switchTab(i)
		}
	}
	return nil
}

func (m *Model) describeTab(t *tabState) {
	switch {
	case t.source == sourceSearch:
		m.setStatus(statusInfo, "%s — %d search result(s) for %q; esc returns to the installed list",
			t.mgr.ID(), len(t.rows), t.query)
	default:
		m.setStatus(statusInfo, "%s — %d installed package(s)", t.mgr.ID(), len(t.rows))
	}
}

// --------------------------------------------------------------- mutations ----

// targets returns the packages an action applies to: every marked package, or
// the one under the cursor when nothing is marked.
func (m *Model) targets(t *tabState) []string {
	if marked := t.markedNames(); len(marked) > 0 {
		return marked
	}
	if cur, ok := t.current(); ok {
		return []string{cur.Name}
	}
	return nil
}

func (m *Model) prepareRemove(t *tabState) tea.Cmd {
	targets := m.targets(t)
	if len(targets) == 0 {
		m.setStatus(statusWarning, "Nothing selected to uninstall")
		return nil
	}
	// Refuse to remove packages that are not installed: the action would be a
	// no-op and the error from the manager would be confusing.
	var installed []string
	for _, name := range targets {
		if p, ok := t.packageByName(name); ok && !p.Installed {
			continue
		}
		installed = append(installed, name)
	}
	if len(installed) == 0 {
		m.setStatus(statusWarning, "%s is not installed, so there is nothing to uninstall",
			strings.Join(targets, ", "))
		return nil
	}
	targets = installed

	plan, err := t.mgr.RemovePlan(context.Background(), targets[0])
	if of, ok := t.mgr.(pkgmgr.OrphanFinder); ok && len(targets) > 1 {
		plan, err = of.RemovePlanMany(context.Background(), targets)
	}
	if err != nil {
		m.setStatus(statusBad, "Cannot uninstall: %v", err)
		return nil
	}

	lines := []string{
		fmt.Sprintf("Uninstall %d %s package(s):", len(targets), t.mgr.ID()),
		"  " + strings.Join(targets, ", "),
		"",
		"Commands that will run:",
	}
	lines = append(lines, planLines(plan)...)
	lines = append(lines, "")

	c := &pendingConfirm{
		kind:    confirmRemove,
		title:   "Uninstall package",
		lines:   lines,
		targets: targets,
		plan:    plan,
		mgr:     t.mgr,
		op:      opRemove,
	}

	var cmd tea.Cmd
	if _, ok := t.mgr.(*pkgmgr.Pip); ok {
		c.awaiting = true
		c.lines = append(c.lines, "Working out which dependencies become unused…")
		cmd = predictOrphansCmd(t.mgr, targets)
	} else {
		c.lines = append(c.lines,
			"apt-get --auto-remove will also drop dependencies that were pulled in",
			"automatically and are no longer needed.")
	}

	m.confirm = c
	m.mode = modeConfirm
	m.layout()
	m.setStatus(statusInfo, "Press enter to uninstall, or esc to cancel")
	return cmd
}

func (m *Model) prepareUpgrade(t *tabState) tea.Cmd {
	cur, ok := t.current()
	if !ok {
		m.setStatus(statusWarning, "Nothing selected to upgrade")
		return nil
	}
	if !cur.Installed {
		m.setStatus(statusWarning, "%s is not installed — press i to install it", cur.Name)
		return nil
	}
	plan, err := t.mgr.UpgradePlan(context.Background(), cur.Name)
	if err != nil {
		m.setStatus(statusBad, "Cannot upgrade %s: %v", cur.Name, err)
		return nil
	}
	lines := []string{
		fmt.Sprintf("Upgrade %s to the newest version available.", cur.Name),
		fmt.Sprintf("Installed now: %s", orDash(cur.Version)),
	}
	if cur.Latest != "" {
		lines = append(lines, fmt.Sprintf("Newer version found: %s", cur.Latest))
	} else if !t.checked {
		lines = append(lines, "Press o first if you want to know the target version beforehand.")
	}
	lines = append(lines, "", "Commands that will run:")
	lines = append(lines, planLines(plan)...)

	m.confirm = &pendingConfirm{
		kind:    confirmUpgrade,
		title:   "Upgrade package",
		lines:   lines,
		targets: []string{cur.Name},
		plan:    plan,
		mgr:     t.mgr,
		op:      opUpgrade,
	}
	m.mode = modeConfirm
	m.layout()
	m.setStatus(statusInfo, "Press enter to upgrade %s, or esc to cancel", cur.Name)
	return nil
}

// planLines renders a plan's commands for a confirmation dialog.
func planLines(p pkgmgr.Plan) []string {
	out := make([]string, 0, len(p.Steps))
	for _, s := range p.Steps {
		out = append(out, "  $ "+s.Display())
	}
	return out
}

func quote(s string) string { return "\"" + s + "\"" }

func orDash(s string) string {
	if strings.TrimSpace(s) == "" {
		return "—"
	}
	return s
}
