package main

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
)

func (m *model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {

	case tea.WindowSizeMsg:
		return m, m.handleResize(msg)

	case tea.KeyMsg:
		return m, m.handleKey(msg)

	case installedLoadedMsg:
		m.loadingInstalled = false
		if msg.err != nil {
			return m, m.setStatus("cannot read the dpkg database: "+msg.err.Error(), statusError)
		}
		m.store.setInstalled(msg.pkgs)
		m.store.rebuild()
		m.clampCursor()
		// Candidate versions for installed packages come from apt-cache policy,
		// which needs the installed list, so it is chained here.
		return m, tea.Batch(m.requestDetails(), loadPolicyCmd(m.store.installedNames()))

	case availableLoadedMsg:
		m.loadingAvailable = false
		m.store.setAvailable(msg.pkgs)
		m.store.rebuild()
		m.clampCursor()
		if msg.err != nil {
			// Partial results are still useful, so they are kept and the error
			// is reported rather than discarding the scan.
			return m, tea.Batch(m.requestDetails(),
				m.setStatus("package index incomplete: "+msg.err.Error(), statusWarn))
		}
		return m, m.requestDetails()

	case upgradableLoadedMsg:
		m.loadingUpgradable = false
		if msg.err != nil {
			return m, m.setStatus("cannot compute the upgrade plan: "+msg.err.Error(), statusError)
		}
		m.store.setUpgradable(msg.cands)
		m.store.rebuild()
		m.clampCursor()
		return m, nil

	case policyLoadedMsg:
		if msg.err == nil {
			m.store.applyPolicy(msg.entries)
			m.store.rebuild()
			m.clampCursor()
		}
		return m, nil

	case aptListsMsg:
		m.aptListsChecked = true
		m.aptListsPresent = msg.present
		return m, nil

	case detailsLoadedMsg:
		// Discard responses for a selection the user has already left.
		if msg.gen != m.detailsGen {
			return m, nil
		}
		m.detailsLoading = false
		m.detailsPkg = msg.details
		m.detailsErr = msg.err
		if msg.details != nil {
			synopsis := msg.details.Synopsis
			m.store.setSynopsis(msg.name, msg.details.Section, synopsis)
		}
		m.details.GotoTop()
		return m, nil

	case opLineMsg:
		m.appendLog(logLine{
			text:   msg.line,
			stderr: msg.stderr,
			cmd:    strings.HasPrefix(msg.line, "$ "),
		})
		// Keep draining until the stream closes.
		return m, m.runner.waitForLine()

	case opDoneMsg:
		return m, m.handleOpDone(msg)

	case statusMsg:
		return m, m.setStatus(msg.text, msg.level)

	case statusExpiredMsg:
		if msg.token == m.statusToken {
			m.status = ""
			m.statusLevel = statusInfo
		}
		return m, nil
	}

	// Anything else (spinner ticks, viewport internals) goes to the components.
	return m, m.updateComponents(msg)
}

// updateComponents forwards a message to the spinner, which animates the
// busy indicator while an operation runs.
func (m *model) updateComponents(msg tea.Msg) tea.Cmd {
	sp, cmd := m.spin.Update(msg)
	m.spin = sp
	return cmd
}

// handleResize recomputes the layout and resizes every component. All pane sizes
// derive from computeLayout, so no pane can overflow the terminal.
func (m *model) handleResize(msg tea.WindowSizeMsg) tea.Cmd {
	m.width, m.height = msg.Width, msg.Height
	m.ready = true
	m.relayout()
	return nil
}

// relayout resolves geometry for the current size and pushes it into the
// components. It is called on resize and whenever something that affects the
// footer height changes (help toggling, an operation starting or finishing).
func (m *model) relayout() {
	if m.width <= 0 || m.height <= 0 {
		return
	}

	footer := m.footerHeight()
	m.layout = computeLayout(m.width, m.height, footer, m.busy)

	m.search.Width = max(4, m.width-len("Search: ")-24)

	m.details.Width = max(1, m.layout.DetailsWidth)
	m.details.Height = max(1, m.layout.BodyHeight-1) // one row is the pane title

	m.logView.Width = max(1, m.width-paneFrameWidth)
	m.logView.Height = max(1, m.layout.LogHeight-1) // one row is the pane title

	m.help.Width = m.width

	// The log content is width-dependent, so a resize invalidates it.
	m.logDirty = true
	m.clampCursor()
}

// handleKey routes a keypress. Modes are checked in priority order: a pending
// confirmation captures y/n first, then the search box captures text, then the
// global bindings apply.
func (m *model) handleKey(msg tea.KeyMsg) tea.Cmd {
	// Quit is always available, even mid-operation, but ctrl+c during an
	// operation only detaches the UI; apt keeps running to completion, which is
	// safer than killing dpkg.
	if key.Matches(msg, m.keys.Quit) {
		if m.busy && msg.String() != "ctrl+c" {
			return m.setStatus("an operation is running; press ctrl+c to quit anyway", statusWarn)
		}
		return tea.Quit
	}

	if m.confirm != nil {
		return m.handleConfirmKey(msg)
	}
	if m.searchFocus {
		return m.handleSearchKey(msg)
	}
	return m.handleGlobalKey(msg)
}

// handleConfirmKey resolves a pending confirmation. Only y and n/esc are
// accepted, so a stray keypress cannot trigger a package operation.
func (m *model) handleConfirmKey(msg tea.KeyMsg) tea.Cmd {
	switch {
	case key.Matches(msg, m.keys.Confirm):
		op := m.confirm.op
		m.confirm = nil
		return m.startOperation(op)

	case key.Matches(msg, m.keys.Cancel):
		verb := m.confirm.op.Describe()
		m.confirm = nil
		return m.setStatus("cancelled: "+verb, statusInfo)
	}
	return nil
}

// handleSearchKey handles keys while the search box has focus. Navigation keys
// still move the list so the user can search and select without leaving the box,
// which is what makes real-time filtering useful.
func (m *model) handleSearchKey(msg tea.KeyMsg) tea.Cmd {
	switch {
	case key.Matches(msg, m.keys.AcceptInput):
		// Accept the query and return to the list, keeping the filter applied.
		m.searchFocus = false
		m.search.Blur()
		return m.setStatus(fmt.Sprintf("filter %q — %d packages match",
			m.store.query, len(m.store.visible())), statusInfo)

	case key.Matches(msg, m.keys.ClearSearch):
		// Esc clears a non-empty query, and leaves the box when already empty,
		// so one key both undoes the search and exits the mode.
		if m.search.Value() != "" {
			m.search.SetValue("")
			return m.applyQuery("")
		}
		m.searchFocus = false
		m.search.Blur()
		return nil

	case key.Matches(msg, m.keys.Up):
		return m.moveCursor(-1)
	case key.Matches(msg, m.keys.Down):
		return m.moveCursor(1)
	case key.Matches(msg, m.keys.PageUp):
		return m.moveCursor(-m.listHeight())
	case key.Matches(msg, m.keys.PageDown):
		return m.moveCursor(m.listHeight())
	}

	// Everything else is text. textinput's default keymap claims up/down, which
	// is why those are intercepted above.
	var cmd tea.Cmd
	m.search, cmd = m.search.Update(msg)

	if m.search.Value() != m.store.query {
		return tea.Batch(cmd, m.applyQuery(m.search.Value()))
	}
	return cmd
}

// applyQuery re-filters the list for a new search string. The cursor returns to
// the top because the previous selection is usually not in the new result set.
func (m *model) applyQuery(query string) tea.Cmd {
	m.store.query = query
	m.store.rebuild()
	m.cursor, m.listOffset = 0, 0
	m.clampCursor()
	return m.requestDetails()
}

// handleGlobalKey handles keys when the list has focus.
func (m *model) handleGlobalKey(msg tea.KeyMsg) tea.Cmd {
	switch {
	case key.Matches(msg, m.keys.Help):
		m.showHelp = !m.showHelp
		m.help.ShowAll = m.showHelp
		// The help view expands the footer in place, so the layout is recomputed
		// and the list shrinks; nothing is covered up.
		m.relayout()
		return nil

	case key.Matches(msg, m.keys.Search):
		m.searchFocus = true
		m.search.Focus()
		return textinput.Blink

	case key.Matches(msg, m.keys.Tab):
		m.focus = m.focus.next()
		return nil

	case key.Matches(msg, m.keys.Up):
		return m.scrollFocused(-1)
	case key.Matches(msg, m.keys.Down):
		return m.scrollFocused(1)
	case key.Matches(msg, m.keys.PageUp):
		return m.scrollFocused(-m.pageSize())
	case key.Matches(msg, m.keys.PageDown):
		return m.scrollFocused(m.pageSize())

	case key.Matches(msg, m.keys.Home):
		return m.jumpFocused(true)
	case key.Matches(msg, m.keys.End):
		return m.jumpFocused(false)

	case key.Matches(msg, m.keys.ClearSearch):
		// Esc from the list clears an active search without entering the box.
		if m.store.query != "" {
			m.search.SetValue("")
			return m.applyQuery("")
		}
		return nil

	case key.Matches(msg, m.keys.AcceptInput):
		// Enter re-reads the selected package from apt, which is how the user
		// refreshes one row after an external change.
		return m.refreshDetails()

	case key.Matches(msg, m.keys.Filter):
		m.store.filter = (m.store.filter + 1) % 5
		return m.applyFilter()
	case key.Matches(msg, m.keys.FilterAll):
		m.store.filter = filterAll
		return m.applyFilter()
	case key.Matches(msg, m.keys.FilterInstalled):
		m.store.filter = filterInstalled
		return m.applyFilter()
	case key.Matches(msg, m.keys.FilterAvailable):
		m.store.filter = filterAvailable
		return m.applyFilter()
	case key.Matches(msg, m.keys.FilterUpgradable):
		m.store.filter = filterUpgradable
		return m.applyFilter()
	case key.Matches(msg, m.keys.FilterResidual):
		m.store.filter = filterResidual
		return m.applyFilter()

	case key.Matches(msg, m.keys.Install):
		return m.confirmInstall()
	case key.Matches(msg, m.keys.Remove):
		return m.confirmRemove(false)
	case key.Matches(msg, m.keys.Purge):
		return m.confirmRemove(true)
	case key.Matches(msg, m.keys.UpgradeOne):
		return m.confirmUpgradeOne()
	case key.Matches(msg, m.keys.UpgradeAll):
		return m.confirmUpgradeAll()

	case key.Matches(msg, m.keys.Reload):
		return m.rescan("rescanning the system…")
	case key.Matches(msg, m.keys.Update):
		return m.requestOperation(operation{Kind: opUpdate},
			"Download the latest package lists from all configured repositories?", "")
	}
	return nil
}

// pageSize is the scroll distance for the page keys in the focused pane.
func (m *model) pageSize() int {
	switch m.focus {
	case focusDetails:
		return max(1, m.details.Height-1)
	case focusLog:
		return max(1, m.logView.Height-1)
	default:
		return max(1, m.listHeight())
	}
}

// scrollFocused moves the cursor or scrolls the focused pane. Only one pane
// responds, but all remain visible.
func (m *model) scrollFocused(delta int) tea.Cmd {
	switch m.focus {
	case focusDetails:
		m.scrollViewport(&m.details, delta)
		return nil
	case focusLog:
		m.scrollViewport(&m.logView, delta)
		return nil
	default:
		return m.moveCursor(delta)
	}
}

// scrollViewport scrolls a viewport by delta lines, clamped to its content.
func (m *model) scrollViewport(vp *viewport.Model, delta int) {
	if delta < 0 {
		vp.LineUp(-delta)
		return
	}
	vp.LineDown(delta)
}

// jumpFocused moves to the start or end of the focused pane.
func (m *model) jumpFocused(top bool) tea.Cmd {
	switch m.focus {
	case focusDetails:
		if top {
			m.details.GotoTop()
		} else {
			m.details.GotoBottom()
		}
		return nil
	case focusLog:
		if top {
			m.logView.GotoTop()
		} else {
			m.logView.GotoBottom()
		}
		return nil
	default:
		if top {
			m.cursor = 0
		} else {
			m.cursor = len(m.store.visible()) - 1
		}
		m.clampCursor()
		return m.requestDetails()
	}
}

// applyFilter re-filters after a filter-mode change.
func (m *model) applyFilter() tea.Cmd {
	m.store.rebuild()
	m.cursor, m.listOffset = 0, 0
	m.clampCursor()

	n := len(m.store.visible())
	return tea.Batch(
		m.requestDetails(),
		m.setStatus(fmt.Sprintf("filter: %s — %d packages", m.store.filter, n), statusInfo),
	)
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

// requestOperation stages an operation behind a confirmation prompt. Nothing
// touches the system until the user answers y.
func (m *model) requestOperation(op operation, prompt, warning string) tea.Cmd {
	if m.busy {
		return m.setStatus("an operation is already running", statusWarn)
	}
	m.confirm = &pendingConfirm{op: op, prompt: prompt, warning: warning}
	return nil
}

func (m *model) confirmInstall() tea.Cmd {
	p := m.selected()
	if p == nil {
		return m.setStatus("no package selected", statusWarn)
	}
	if p.Installed {
		if p.Upgradable {
			return m.setStatus(fmt.Sprintf("%s is already installed; press u to upgrade it to %s",
				p.Name, p.UpgradeTarget), statusWarn)
		}
		return m.setStatus(p.Name+" is already installed", statusWarn)
	}

	version := p.AvailableVersion
	if version == "" {
		version = "the candidate version"
	}
	return m.requestOperation(
		operation{Kind: opInstall, Target: p.Name},
		fmt.Sprintf("Install %s (%s)?", p.Name, version),
		"",
	)
}

func (m *model) confirmRemove(purge bool) tea.Cmd {
	p := m.selected()
	if p == nil {
		return m.setStatus("no package selected", statusWarn)
	}

	kind := opRemove
	verb := "Remove"
	if purge {
		kind = opPurge
		verb = "Purge"
	}
	if !p.Installed && !p.ResidualConfig() {
		return m.setStatus(p.Name+" is not installed", statusWarn)
	}

	// Ask apt what else would go, so the user is warned about collateral
	// removals before confirming rather than after the fact.
	warning := ""
	if others, err := previewRemoveCmd(p.Name, purge); err == nil {
		var extra []string
		for _, name := range others {
			if name != p.Name {
				extra = append(extra, name)
			}
		}
		if len(extra) > 0 {
			warning = fmt.Sprintf("this also removes %d other package(s): %s",
				len(extra), strings.Join(extra, " "))
		}
	} else {
		// apt refuses outright for essential packages; surface its reason.
		warning = "apt reports a problem with this removal: " + err.Error()
	}

	prompt := fmt.Sprintf("%s %s (%s)?", verb, p.Name, p.InstalledVersion)
	if purge {
		prompt = fmt.Sprintf("%s %s (%s), deleting its configuration files?",
			verb, p.Name, p.InstalledVersion)
	}
	return m.requestOperation(operation{Kind: kind, Target: p.Name}, prompt, warning)
}

func (m *model) confirmUpgradeOne() tea.Cmd {
	p := m.selected()
	if p == nil {
		return m.setStatus("no package selected", statusWarn)
	}
	if !p.Installed {
		return m.setStatus(p.Name+" is not installed; press i to install it", statusWarn)
	}
	if !p.Upgradable {
		return m.setStatus(p.Name+" is already at the newest version", statusInfo)
	}

	return m.requestOperation(
		operation{Kind: opUpgradeOne, Target: p.Name},
		fmt.Sprintf("Upgrade %s from %s to %s?", p.Name, p.InstalledVersion, p.UpgradeTarget),
		"",
	)
}

func (m *model) confirmUpgradeAll() tea.Cmd {
	n := m.store.countUpgradable
	if n == 0 {
		return m.setStatus("no upgradable packages", statusInfo)
	}
	return m.requestOperation(
		operation{Kind: opUpgradeAll},
		fmt.Sprintf("Upgrade all %d upgradable package(s)?", n),
		"",
	)
}

// startOperation launches a confirmed operation and begins streaming its output.
func (m *model) startOperation(op operation) tea.Cmd {
	if m.busy {
		return m.setStatus("an operation is already running", statusWarn)
	}

	m.busy = true
	m.current = op
	// The log pane grows while an operation runs, so recompute the layout.
	m.relayout()

	m.appendLog(logLine{text: "", cmd: false})
	return tea.Batch(
		m.runner.start(op),
		m.spin.Tick,
		m.setStatus("running: "+op.CommandLine(), statusInfo),
	)
}

// handleOpDone finishes an operation: report the outcome, then re-read system
// state from dpkg and apt so the interface matches reality.
func (m *model) handleOpDone(msg opDoneMsg) tea.Cmd {
	m.busy = false
	m.relayout()

	var cmds []tea.Cmd

	if msg.err != nil {
		m.appendLog(logLine{text: "✗ " + msg.op.Verb() + " failed: " + msg.err.Error(), stderr: true})
		cmds = append(cmds, m.setStatus(
			fmt.Sprintf("%s failed — see the apt output pane", msg.op.Describe()), statusError))
	} else {
		m.appendLog(logLine{text: "✓ " + msg.op.Describe() + " completed"})
		cmds = append(cmds, m.setStatus(msg.op.Describe()+" completed", statusSuccess))
	}

	// Rescan unconditionally: a failed operation can still have changed state
	// (a partial install, or a removal that succeeded before a later error).
	cmds = append(cmds, m.rescanCmds(msg.op)...)
	return tea.Batch(cmds...)
}

// rescan re-reads everything from the system.
func (m *model) rescan(message string) tea.Cmd {
	cmds := m.rescanCmds(operation{})
	cmds = append(cmds, m.setStatus(message, statusInfo))
	return tea.Batch(cmds...)
}

// rescanCmds returns the commands that refresh state from the system. The
// available index is only re-read after apt-get update, because it is the
// expensive scan and only that operation can change it.
func (m *model) rescanCmds(op operation) []tea.Cmd {
	m.loadingInstalled = true
	m.loadingUpgradable = true

	cmds := []tea.Cmd{
		loadInstalledCmd(),
		loadUpgradableCmd(),
		m.refreshDetails(),
	}

	if op.Kind == opUpdate {
		m.loadingAvailable = true
		cmds = append(cmds, loadAvailableCmd(), probeAptListsCmd())
	}
	return cmds
}
