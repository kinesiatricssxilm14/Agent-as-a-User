package main

import (
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/bubbles/spinner"
	tea "github.com/charmbracelet/bubbletea"
)

func (m *model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.ready = true
		m.applyLayout()
		return m, nil

	case spinner.TickMsg:
		var cmd tea.Cmd
		m.spinner, cmd = m.spinner.Update(msg)
		return m, cmd

	case loadResultMsg:
		return m.handleLoadResult(msg)

	case detailsDebounceMsg:
		if msg.reqID != m.detailsReqID {
			return m, nil
		}
		p, ok := m.pkgByName[m.detailsName]
		if !ok {
			return m, nil
		}
		return m, fetchDetailsCmd(p, msg.reqID)

	case detailsResultMsg:
		if msg.reqID != m.detailsReqID {
			return m, nil
		}
		m.details.SetContent(renderDetails(msg.details))
		m.details.GotoTop()
		return m, nil

	case startedMsg:
		m.proc = msg.proc
		m.output = nil
		m.outputView.SetContent("")
		m.outputView.GotoTop()
		return m, msg.proc.nextCmd()

	case lineMsg:
		line := strings.TrimRight(string(msg), "\r")
		m.output = append(m.output, line)
		m.outputView.SetContent(strings.Join(m.output, "\n"))
		m.outputView.GotoBottom()
		if m.proc != nil {
			return m, m.proc.nextCmd()
		}
		return m, nil

	case doneMsg:
		return m.handleDone(msg)

	case tea.KeyMsg:
		return m.handleKey(msg)
	}
	return m, nil
}

func (m *model) handleLoadResult(msg loadResultMsg) (tea.Model, tea.Cmd) {
	m.loading = false
	m.installedMap = msg.installed
	m.availableMap = msg.available
	m.upgradable = msg.upgradable
	m.allPackages, m.pkgByName = mergePackages(msg.installed, msg.available, msg.upgradable)
	cmd := m.rebuildList()

	if m.justOperated {
		m.justOperated = false
		return m, cmd
	}

	switch {
	case msg.err != nil && len(m.allPackages) == 0:
		m.statusMsg = fmt.Sprintf("✗ failed to load package data: %v", msg.err)
	case len(m.availableMap) == 0:
		m.statusMsg = "no package lists found — press R to run apt-get update"
	default:
		m.statusMsg = fmt.Sprintf("loaded %d packages · installed %d · available %d · upgradable %d",
			len(m.allPackages), len(m.installedMap), len(m.availableMap), len(m.upgradable))
	}
	return m, cmd
}

func (m *model) handleDone(msg doneMsg) (tea.Model, tea.Cmd) {
	m.proc = nil
	m.state = StateBrowse
	m.focus = FocusList
	if msg.err != nil {
		m.statusMsg = fmt.Sprintf("✗ %s failed: %v", m.lastActionDesc, msg.err)
	} else {
		m.statusMsg = fmt.Sprintf("✓ %s completed", m.lastActionDesc)
	}
	m.justOperated = true

	if msg.err != nil {
		m.output = append(m.output, fmt.Sprintf("exit error: %v", msg.err))
	} else {
		m.output = append(m.output, "done.")
	}
	m.outputView.SetContent(strings.Join(m.output, "\n"))
	m.outputView.GotoBottom()

	if m.lastActionDesc == "apt-get update" {
		return m, loadPackagesCmd()
	}
	return m, refreshPackagesCmd(m.availableMap)
}

func (m *model) handleKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	key := msg.String()

	switch m.state {
	case StateConfirm:
		switch key {
		case "y", "Y", "enter":
			return m, m.startAction()
		case "n", "N", "esc", "q", "ctrl+c":
			m.state = StateBrowse
			m.confirmMsg = ""
			m.pendingAction = Action{}
			m.statusMsg = "cancelled"
			return m, nil
		}
		return m, nil

	case StateRunning:
		if key == "ctrl+c" {
			return m, tea.Quit
		}
		var cmd tea.Cmd
		m.outputView, cmd = m.outputView.Update(msg)
		return m, cmd
	}

	// Browse state.
	if m.help {
		m.help = false
		return m, nil
	}

	if key == "ctrl+c" {
		return m, tea.Quit
	}

	// While typing a search query, keys belong to the text input so that a
	// filter character such as "q" or "?" is not mistaken for a command.
	if m.focus == FocusSearch {
		return m.handleSearchKey(msg, key)
	}

	switch key {
	case "q":
		return m, tea.Quit
	case "?":
		m.help = true
		return m, nil
	case "tab":
		m.focus = m.focus.Next()
		m.statusMsg = focusHint(m.focus)
		return m, nil
	case "/":
		m.focus = FocusSearch
		return m, m.searchInput.Focus()
	case "esc":
		if m.focus == FocusDetails {
			m.focus = FocusList
		}
		return m, nil
	}

	switch m.focus {
	case FocusDetails:
		var cmd tea.Cmd
		m.details, cmd = m.details.Update(msg)
		return m, cmd
	default:
		return m.handleListKey(msg, key)
	}
}

func (m *model) handleSearchKey(msg tea.KeyMsg, key string) (tea.Model, tea.Cmd) {
	switch key {
	case "enter":
		m.searchInput.Blur()
		m.focus = FocusList
		return m, nil
	case "esc":
		m.searchInput.SetValue("")
		m.searchInput.Blur()
		m.focus = FocusList
		return m, m.rebuildList()
	case "tab":
		m.searchInput.Blur()
		m.focus = FocusList
		return m, nil
	}
	var cmd tea.Cmd
	m.searchInput, cmd = m.searchInput.Update(msg)
	cmd = tea.Batch(cmd, m.rebuildList())
	return m, cmd
}

func (m *model) handleListKey(msg tea.KeyMsg, key string) (tea.Model, tea.Cmd) {
	switch key {
	case "1":
		m.mode = ModeInstalled
		return m, m.rebuildList()
	case "2":
		m.mode = ModeAvailable
		return m, m.rebuildList()
	case "3":
		m.mode = ModeUpgradable
		return m, m.rebuildList()
	case "4":
		m.mode = ModeAll
		return m, m.rebuildList()
	case "i":
		return m, m.prepareAction(Action{Kind: "install"})
	case "r", "d":
		return m, m.prepareAction(Action{Kind: "remove"})
	case "u":
		return m, m.prepareAction(Action{Kind: "upgrade"})
	case "U":
		return m, m.prepareAction(Action{Kind: "upgrade-all"})
	case "R":
		return m, m.prepareAction(Action{Kind: "update"})
	case "enter":
		return m, m.defaultAction()
	}

	before := selectedName(m.list)
	var cmd tea.Cmd
	m.list, cmd = m.list.Update(msg)
	if selectedName(m.list) != before {
		cmd = tea.Batch(cmd, m.onSelectionChanged())
	}
	return m, cmd
}

func (m *model) defaultAction() tea.Cmd {
	item := m.list.SelectedItem()
	p, ok := item.(Package)
	if !ok {
		m.statusMsg = "no package selected"
		return nil
	}
	switch p.Status {
	case StatusUpgradable:
		return m.prepareAction(Action{Kind: "upgrade"})
	case StatusInstalled:
		return m.prepareAction(Action{Kind: "remove"})
	default:
		return m.prepareAction(Action{Kind: "install"})
	}
}

func (m *model) prepareAction(a Action) tea.Cmd {
	if a.Kind == "upgrade-all" {
		if len(m.upgradable) == 0 {
			m.statusMsg = "no packages to upgrade"
			return nil
		}
		m.pendingAction = a
		m.state = StateConfirm
		m.confirmMsg = confirmText(a)
		return nil
	}
	if a.Kind == "update" {
		m.pendingAction = a
		m.state = StateConfirm
		m.confirmMsg = confirmText(a)
		return nil
	}

	item := m.list.SelectedItem()
	p, ok := item.(Package)
	if !ok {
		m.statusMsg = "no package selected"
		return nil
	}

	switch a.Kind {
	case "install":
		if p.Status == StatusInstalled || p.Status == StatusUpgradable {
			m.statusMsg = fmt.Sprintf("%s is already installed", p.Name)
			return nil
		}
	case "remove":
		if p.Status == StatusAvailable {
			m.statusMsg = fmt.Sprintf("%s is not installed", p.Name)
			return nil
		}
	case "upgrade":
		if p.Status != StatusUpgradable {
			m.statusMsg = fmt.Sprintf("%s is not upgradable", p.Name)
			return nil
		}
	}

	a.Target = p.Name
	m.pendingAction = a
	m.state = StateConfirm
	m.confirmMsg = confirmText(a)
	return nil
}

func confirmText(a Action) string {
	switch a.Kind {
	case "install":
		return fmt.Sprintf("Install package %q?  [y]es / [n]o", a.Target)
	case "remove":
		return fmt.Sprintf("Remove package %q?  [y]es / [n]o", a.Target)
	case "upgrade":
		return fmt.Sprintf("Upgrade package %q?  [y]es / [n]o", a.Target)
	case "upgrade-all":
		return "Upgrade ALL upgradable packages?  [y]es / [n]o"
	case "update":
		return "Run apt-get update to refresh package lists?  [y]es / [n]o"
	}
	return ""
}

func (m *model) startAction() tea.Cmd {
	a := m.pendingAction
	m.state = StateRunning
	m.pendingAction = Action{}
	m.confirmMsg = ""
	m.lastActionDesc = a.describe()
	m.statusMsg = fmt.Sprintf("running %s…", m.lastActionDesc)
	return startAptProcess("apt-get", a.command()...)
}

// rebuildList recomputes the filtered list, restores the selection and
// refreshes the details panel when the selection changed.
func (m *model) rebuildList() tea.Cmd {
	m.filtered = filterPackages(m.allPackages, m.mode, m.searchInput.Value())

	items := make([]list.Item, len(m.filtered))
	for i := range m.filtered {
		items[i] = m.filtered[i]
	}

	selName := selectedName(m.list)
	m.list.SetItems(items)

	if len(m.filtered) > 0 {
		idx := -1
		if selName != "" {
			for i := range m.filtered {
				if m.filtered[i].Name == selName {
					idx = i
					break
				}
			}
		}
		if idx >= 0 {
			m.list.Select(idx)
		} else {
			m.list.Select(0)
		}
	}

	return m.onSelectionChanged()
}

func (m *model) onSelectionChanged() tea.Cmd {
	name := selectedName(m.list)
	if name == "" {
		m.detailsName = ""
		m.details.SetContent("")
		return nil
	}
	if name == m.detailsName {
		return nil
	}
	m.detailsName = name
	m.detailsReqID++
	m.details.SetContent(renderDetailsPlaceholder(name))
	m.details.GotoTop()
	return debounceCmd(m.detailsReqID, 120*time.Millisecond)
}
