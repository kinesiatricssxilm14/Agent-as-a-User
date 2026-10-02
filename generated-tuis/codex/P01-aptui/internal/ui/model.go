package ui

import (
	"context"
	"fmt"
	"sort"
	"strings"

	"github.com/charmbracelet/bubbles/spinner"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"

	"toola/internal/apt"
)

type packageFilter int

const (
	filterAll packageFilter = iota
	filterInstalled
	filterAvailable
	filterUpgradable
)

type operation int

const (
	opNone operation = iota
	opInstall
	opRemove
	opUpgrade
	opUpgradeAll
	opUpdate
)

type packagesMsg struct {
	packages []apt.Package
	err      error
}

type detailsMsg struct {
	name    string
	details apt.Details
	err     error
}

type operationMsg struct {
	operation operation
	name      string
	output    string
	err       error
}

type Model struct {
	backend apt.Backend

	width    int
	height   int
	ready    bool
	listRows int

	packages []apt.Package
	filtered []apt.Package
	filter   packageFilter
	cursor   int
	offset   int

	search      textinput.Model
	searching   bool
	detailView  viewport.Model
	details     apt.Details
	detailsName string
	outputTitle string
	outputText  string

	spinner  spinner.Model
	busy     bool
	busyText string
	mutating bool

	confirm        operation
	confirmPackage string
	status         string
	statusIsError  bool
}

func New(backend apt.Backend) Model {
	search := textinput.New()
	search.Placeholder = "type name or keyword..."
	search.Prompt = "Search: "
	search.CharLimit = 120
	search.Blur()

	spin := spinner.New()
	spin.Spinner = spinner.Dot
	spin.Style = lipgloss.NewStyle().Foreground(colorAccent)

	return Model{
		backend:  backend,
		filter:   filterAll,
		search:   search,
		spinner:  spin,
		status:   "Loading package catalog…",
		busy:     true,
		busyText: "Loading package catalog",
	}
}

func (m Model) Init() tea.Cmd {
	return tea.Batch(m.spinner.Tick, loadPackages(m.backend))
}

func (m Model) Update(message tea.Msg) (tea.Model, tea.Cmd) {
	var commands []tea.Cmd

	switch msg := message.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		m.resize()
		if m.ready {
			m.refreshDetailContent()
		}
		return m, nil

	case spinner.TickMsg:
		if m.busy {
			var cmd tea.Cmd
			m.spinner, cmd = m.spinner.Update(msg)
			commands = append(commands, cmd)
		}

	case packagesMsg:
		m.busy = false
		if msg.err != nil {
			m.setError(msg.err)
			return m, nil
		}
		selected := m.selectedName()
		m.packages = msg.packages
		m.applyFilter(selected)
		m.status = fmt.Sprintf("Loaded %d packages", len(m.packages))
		m.statusIsError = false
		if pkg, ok := m.selected(); ok {
			commands = append(commands, loadDetails(m.backend, pkg.Name))
		} else {
			m.details = apt.Details{}
			m.detailsName = ""
			m.refreshDetailContent()
		}

	case detailsMsg:
		if msg.name != m.selectedName() {
			return m, nil
		}
		if msg.err != nil {
			m.detailsName = msg.name
			m.details = apt.Details{Name: msg.name, Description: "Unable to load details:\n" + msg.err.Error()}
			m.setError(msg.err)
		} else {
			m.detailsName = msg.name
			m.details = msg.details
		}
		m.refreshDetailContent()

	case operationMsg:
		m.mutating = false
		m.busy = false
		m.confirm = opNone
		if msg.err != nil {
			m.setError(msg.err)
			m.setOutput(operationTitle(msg.operation, msg.name)+" failed", joinedOutput(msg.output, msg.err.Error()))
		} else {
			m.status = operationTitle(msg.operation, msg.name) + " completed; reloading package state"
			m.statusIsError = false
			m.setOutput(operationTitle(msg.operation, msg.name)+" completed", msg.output)
		}
		m.busy = true
		m.busyText = "Refreshing package state"
		commands = append(commands, m.spinner.Tick, loadPackages(m.backend))
	}

	if key, ok := message.(tea.KeyMsg); ok {
		if m.busy {
			if !m.mutating && (key.String() == "ctrl+c" || key.String() == "q") {
				return m, tea.Quit
			}
			return m, tea.Batch(commands...)
		}

		if m.searching {
			switch key.String() {
			case "esc":
				m.searching = false
				m.search.Blur()
				m.search.SetValue("")
				m.applyFilter("")
				m.status = "Search cleared"
				m.statusIsError = false
			case "enter":
				m.searching = false
				m.search.Blur()
				m.status = fmt.Sprintf("%d packages match %q", len(m.filtered), m.search.Value())
				m.statusIsError = false
			default:
				before := m.search.Value()
				var cmd tea.Cmd
				m.search, cmd = m.search.Update(key)
				commands = append(commands, cmd)
				if before != m.search.Value() {
					m.applyFilter("")
					commands = append(commands, m.loadSelectedDetails())
				}
			}
			return m, tea.Batch(commands...)
		}

		if m.confirm != opNone {
			switch strings.ToLower(key.String()) {
			case "y", "enter":
				op, name := m.confirm, m.confirmPackage
				m.confirm = opNone
				m.busy = true
				m.mutating = true
				m.busyText = operationTitle(op, name)
				commands = append(commands, m.spinner.Tick, runOperation(m.backend, op, name))
			case "n", "esc":
				m.confirm = opNone
				m.status = "Operation cancelled"
				m.statusIsError = false
			}
			return m, tea.Batch(commands...)
		}

		switch key.String() {
		case "ctrl+c", "q":
			return m, tea.Quit
		case "/":
			m.searching = true
			m.search.Focus()
			commands = append(commands, textinput.Blink)
		case "esc":
			if m.search.Value() != "" {
				m.search.SetValue("")
				m.applyFilter("")
				commands = append(commands, m.loadSelectedDetails())
			}
		case "1":
			m.changeFilter(filterAll)
			commands = append(commands, m.loadSelectedDetails())
		case "2":
			m.changeFilter(filterInstalled)
			commands = append(commands, m.loadSelectedDetails())
		case "3":
			m.changeFilter(filterAvailable)
			commands = append(commands, m.loadSelectedDetails())
		case "4":
			m.changeFilter(filterUpgradable)
			commands = append(commands, m.loadSelectedDetails())
		case "up", "k":
			if m.moveCursor(-1) {
				commands = append(commands, m.loadSelectedDetails())
			}
		case "down", "j":
			if m.moveCursor(1) {
				commands = append(commands, m.loadSelectedDetails())
			}
		case "pgup":
			if m.moveCursor(-m.listHeight()) {
				commands = append(commands, m.loadSelectedDetails())
			}
		case "pgdown":
			if m.moveCursor(m.listHeight()) {
				commands = append(commands, m.loadSelectedDetails())
			}
		case "home", "g":
			if m.setCursor(0) {
				commands = append(commands, m.loadSelectedDetails())
			}
		case "end", "G":
			if m.setCursor(len(m.filtered) - 1) {
				commands = append(commands, m.loadSelectedDetails())
			}
		case "ctrl+u":
			m.detailView.HalfViewUp()
		case "ctrl+d":
			m.detailView.HalfViewDown()
		case "left", "h":
			m.detailView.LineUp(1)
		case "right", "l":
			m.detailView.LineDown(1)
		case "i":
			if pkg, ok := m.selected(); ok && !pkg.Installed() {
				m.ask(opInstall, pkg.Name)
			}
		case "x":
			if pkg, ok := m.selected(); ok && pkg.Installed() {
				m.ask(opRemove, pkg.Name)
			}
		case "u":
			if pkg, ok := m.selected(); ok && pkg.Upgradable {
				m.ask(opUpgrade, pkg.Name)
			}
		case "U":
			if m.upgradableCount() > 0 {
				m.ask(opUpgradeAll, "")
			}
		case "r":
			m.busy = true
			m.busyText = "Refreshing package state"
			commands = append(commands, m.spinner.Tick, loadPackages(m.backend))
		case "a":
			m.ask(opUpdate, "")
		case "o":
			if m.outputText != "" {
				if m.detailView.AtBottom() {
					m.detailView.GotoTop()
				} else {
					m.detailView.GotoBottom()
				}
			}
		}
	}

	return m, tea.Batch(commands...)
}

func (m Model) View() string {
	if m.width == 0 || m.height == 0 {
		return "Starting toola…"
	}

	header := m.renderHeader()
	status := m.renderStatus()
	help := helpStyle.Width(max(1, m.width-2)).Render(m.renderHelp())
	bodyHeight := max(3, m.height-lipgloss.Height(header)-lipgloss.Height(status)-lipgloss.Height(help)-3)
	leftWidth := max(24, min(52, m.width*2/5))
	rightWidth := max(20, m.width-leftWidth-1)

	left := panelStyle.Width(leftWidth - 2).Height(bodyHeight - 2).Render(m.renderList(leftWidth-4, bodyHeight-2))
	right := panelStyle.Width(rightWidth - 2).Height(bodyHeight - 2).Render(m.detailView.View())
	body := lipgloss.JoinHorizontal(lipgloss.Top, left, right)

	return header + "\n" + body + "\n" + status + "\n" + help
}

func (m *Model) resize() {
	headerHeight := lipgloss.Height(m.renderHeader())
	statusHeight := lipgloss.Height(m.renderStatus())
	helpHeight := lipgloss.Height(m.renderHelp())
	bodyHeight := max(3, m.height-headerHeight-statusHeight-helpHeight-3)
	m.listRows = max(1, bodyHeight-5)
	leftWidth := max(24, min(52, m.width*2/5))
	rightWidth := max(20, m.width-leftWidth-1)
	m.detailView.Width = max(1, rightWidth-4)
	m.detailView.Height = max(1, bodyHeight-2)
	m.ready = true
	m.ensureCursorVisible()
}

func (m *Model) applyFilter(preferred string) {
	query := strings.ToLower(strings.TrimSpace(m.search.Value()))
	m.filtered = m.filtered[:0]
	for _, pkg := range m.packages {
		if query != "" &&
			!strings.Contains(strings.ToLower(pkg.Name), query) &&
			!strings.Contains(strings.ToLower(pkg.Summary), query) {
			continue
		}
		switch m.filter {
		case filterInstalled:
			if !pkg.Installed() {
				continue
			}
		case filterAvailable:
			if pkg.Installed() {
				continue
			}
		case filterUpgradable:
			if !pkg.Upgradable {
				continue
			}
		}
		m.filtered = append(m.filtered, pkg)
	}
	sort.Slice(m.filtered, func(i, j int) bool { return m.filtered[i].Name < m.filtered[j].Name })
	m.cursor = 0
	if preferred != "" {
		for i := range m.filtered {
			if m.filtered[i].Name == preferred {
				m.cursor = i
				break
			}
		}
	}
	m.offset = 0
	m.ensureCursorVisible()
}

func (m *Model) changeFilter(filter packageFilter) {
	m.filter = filter
	m.applyFilter("")
	m.status = fmt.Sprintf("Showing %s packages (%d)", filterName(filter), len(m.filtered))
	m.statusIsError = false
}

func (m *Model) moveCursor(delta int) bool {
	return m.setCursor(m.cursor + delta)
}

func (m *Model) setCursor(cursor int) bool {
	if len(m.filtered) == 0 {
		return false
	}
	cursor = max(0, min(cursor, len(m.filtered)-1))
	if cursor == m.cursor {
		return false
	}
	m.cursor = cursor
	m.ensureCursorVisible()
	return true
}

func (m *Model) ensureCursorVisible() {
	height := m.listHeight()
	if m.cursor < m.offset {
		m.offset = m.cursor
	}
	if m.cursor >= m.offset+height {
		m.offset = m.cursor - height + 1
	}
	maxOffset := max(0, len(m.filtered)-height)
	m.offset = max(0, min(m.offset, maxOffset))
}

func (m Model) listHeight() int {
	return max(1, m.listRows)
}

func (m Model) selected() (apt.Package, bool) {
	if m.cursor < 0 || m.cursor >= len(m.filtered) {
		return apt.Package{}, false
	}
	return m.filtered[m.cursor], true
}

func (m Model) selectedName() string {
	pkg, ok := m.selected()
	if !ok {
		return ""
	}
	return pkg.Name
}

func (m *Model) loadSelectedDetails() tea.Cmd {
	pkg, ok := m.selected()
	if !ok {
		m.details = apt.Details{}
		m.detailsName = ""
		m.refreshDetailContent()
		return nil
	}
	return loadDetails(m.backend, pkg.Name)
}

func (m *Model) ask(op operation, name string) {
	m.confirm = op
	m.confirmPackage = name
	m.status = confirmationText(op, name)
	m.statusIsError = false
}

func (m *Model) setError(err error) {
	m.status = err.Error()
	m.statusIsError = true
}

func (m *Model) refreshDetailContent() {
	content := renderDetails(m.details, m.detailView.Width)
	if m.outputText != "" {
		content += "\n\n" + strings.Repeat("─", max(1, m.detailView.Width)) + "\n"
		content += titleStyle.Render(m.outputTitle) + "\n\n"
		content += lipgloss.NewStyle().Width(max(1, m.detailView.Width)).Render(m.outputText)
	}
	m.detailView.SetContent(content)
	m.detailView.GotoTop()
}

func (m *Model) setOutput(title, output string) {
	if strings.TrimSpace(output) == "" {
		output = "Command completed without output."
	}
	m.outputTitle = title
	m.outputText = output
	m.refreshDetailContent()
	m.detailView.GotoBottom()
}

func (m Model) upgradableCount() int {
	count := 0
	for _, pkg := range m.packages {
		if pkg.Upgradable {
			count++
		}
	}
	return count
}

func loadPackages(backend apt.Backend) tea.Cmd {
	return func() tea.Msg {
		packages, err := backend.Packages(context.Background())
		return packagesMsg{packages: packages, err: err}
	}
}

func loadDetails(backend apt.Backend, name string) tea.Cmd {
	return func() tea.Msg {
		details, err := backend.Details(context.Background(), name)
		return detailsMsg{name: name, details: details, err: err}
	}
}

func runOperation(backend apt.Backend, op operation, name string) tea.Cmd {
	return func() tea.Msg {
		ctx := context.Background()
		var output string
		var err error
		switch op {
		case opInstall:
			output, err = backend.Install(ctx, name)
		case opRemove:
			output, err = backend.Remove(ctx, name)
		case opUpgrade:
			output, err = backend.Upgrade(ctx, name)
		case opUpgradeAll:
			output, err = backend.UpgradeAll(ctx)
		case opUpdate:
			output, err = backend.Update(ctx)
		}
		return operationMsg{operation: op, name: name, output: output, err: err}
	}
}

func joinedOutput(parts ...string) string {
	var nonempty []string
	for _, part := range parts {
		if strings.TrimSpace(part) != "" {
			nonempty = append(nonempty, strings.TrimSpace(part))
		}
	}
	return strings.Join(nonempty, "\n\n")
}

func max(a, b int) int {
	if a > b {
		return a
	}
	return b
}

func min(a, b int) int {
	if a < b {
		return a
	}
	return b
}
