package main

import (
	"context"
	"fmt"
	"sort"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/spinner"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/charmbracelet/lipgloss"
)

type mode int

const (
	browseMode mode = iota
	filterMode
	searchMode
	installMode
	confirmMode
	helpMode
)

type loadMsg struct {
	Manager  manager
	Packages []pkg
	Err      error
}
type detailsMsg struct {
	Manager manager
	Name    string
	Detail  detail
	Err     error
}
type searchMsg struct {
	Query    string
	Packages []pkg
	Err      error
}
type opMsg struct {
	Manager              manager
	Action, Name, Output string
	Err                  error
}

type model struct {
	manager            manager
	packages, filtered []pkg
	cursor, offset     int
	width, height      int
	mode               mode
	input              textinput.Model
	spinner            spinner.Model
	viewport           viewport.Model
	detail             detail
	detailKey          string
	loading            bool
	status             string
	err                error
	remote             bool
	pendingAction      string
	showOutput         string
}

var (
	purple        = lipgloss.Color("63")
	cyan          = lipgloss.Color("44")
	muted         = lipgloss.Color("241")
	red           = lipgloss.Color("196")
	green         = lipgloss.Color("42")
	titleStyle    = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("255")).Background(purple).Padding(0, 1)
	selectedStyle = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("230")).Background(lipgloss.Color("57"))
	borderStyle   = lipgloss.NewStyle().Border(lipgloss.RoundedBorder()).BorderForeground(lipgloss.Color("238"))
)

func newModel() model {
	ti := textinput.New()
	ti.Prompt = "› "
	ti.CharLimit = 200
	sp := spinner.New()
	sp.Spinner = spinner.Dot
	sp.Style = lipgloss.NewStyle().Foreground(cyan)
	vp := viewport.New(20, 10)
	return model{manager: pipManager, input: ti, spinner: sp, viewport: vp, loading: true, status: "Loading installed pip packages…"}
}

func (m model) Init() tea.Cmd { return tea.Batch(loadCmd(m.manager), m.spinner.Tick) }
func loadCmd(mgr manager) tea.Cmd {
	return func() tea.Msg { p, e := listPackages(mgr); return loadMsg{mgr, p, e} }
}
func detailsCmd(mgr manager, p pkg, remote bool) tea.Cmd {
	return func() tea.Msg { d, e := packageDetail(mgr, p.Name, remote); return detailsMsg{mgr, p.Name, d, e} }
}
func searchCmd(q string) tea.Cmd {
	return func() tea.Msg { p, e := searchPyPI(q); return searchMsg{q, p, e} }
}
func opCmd(mgr manager, action, name string) tea.Cmd {
	return func() tea.Msg {
		r := mutatePackage(mgr, action, name)
		return opMsg{mgr, action, name, r.Output, r.Err}
	}
}

func (m model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	var cmd tea.Cmd
	switch x := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = x.Width, x.Height
		m.resize()
	case spinner.TickMsg:
		if m.loading {
			m.spinner, cmd = m.spinner.Update(msg)
			return m, cmd
		}
	case loadMsg:
		if x.Manager != m.manager {
			return m, nil
		}
		m.loading = false
		m.err = x.Err
		if x.Err == nil {
			m.packages = x.Packages
			m.remote = false
			m.applyFilter("")
			m.status = fmt.Sprintf("Loaded %d installed %s packages", len(x.Packages), m.manager)
		}
		return m, m.loadCurrentDetail()
	case detailsMsg:
		if x.Manager == m.manager && x.Name == m.currentName() {
			m.detail = x.Detail
			m.err = x.Err
			m.detailKey = x.Name
			m.viewport.GotoTop()
		}
	case searchMsg:
		m.loading = false
		m.err = x.Err
		if x.Err == nil {
			m.packages = x.Packages
			m.filtered = x.Packages
			m.cursor, m.offset = 0, 0
			m.remote = true
			m.status = fmt.Sprintf("PyPI results for %q — i installs selected package", x.Query)
			return m, m.loadCurrentDetail()
		}
	case opMsg:
		m.loading = false
		m.showOutput = x.Output
		m.err = x.Err
		if x.Err == nil {
			m.status = fmt.Sprintf("%s %s completed; refreshing…", x.Action, x.Name)
			m.loading = true
			return m, loadCmd(m.manager)
		}
		m.status = fmt.Sprintf("%s failed for %s", x.Action, x.Name)
	}

	if k, ok := msg.(tea.KeyMsg); ok {
		if m.mode == helpMode {
			if k.String() == "?" || k.String() == "esc" || k.String() == "q" {
				m.mode = browseMode
			}
			return m, nil
		}
		if m.mode == filterMode || m.mode == searchMode || m.mode == installMode {
			switch k.String() {
			case "esc":
				m.mode = browseMode
				m.input.Blur()
				if !m.remote {
					m.applyFilter("")
				}
				return m, nil
			case "enter":
				value := strings.TrimSpace(m.input.Value())
				old := m.mode
				m.mode = browseMode
				m.input.Blur()
				if value == "" {
					return m, nil
				}
				if old == searchMode {
					m.loading = true
					m.status = "Searching PyPI…"
					return m, searchCmd(value)
				}
				if old == installMode {
					m.pendingAction = "install"
					m.mode = confirmMode
					m.input.SetValue(value)
					return m, nil
				}
			}
			m.input, cmd = m.input.Update(msg)
			if m.mode == filterMode {
				m.applyFilter(m.input.Value())
				return m, m.loadCurrentDetail()
			}
			return m, cmd
		}
		if m.mode == confirmMode {
			switch strings.ToLower(k.String()) {
			case "y", "enter":
				name := m.input.Value()
				action := m.pendingAction
				m.mode = browseMode
				m.loading = true
				m.status = fmt.Sprintf("Running %s %s…", action, name)
				return m, opCmd(m.manager, action, name)
			case "n", "esc":
				m.mode = browseMode
				m.status = "Operation cancelled"
				return m, nil
			}
			return m, nil
		}
		if m.loading {
			if k.String() == "ctrl+c" || k.String() == "q" {
				return m, tea.Quit
			}
			return m, nil
		}
		switch k.String() {
		case "ctrl+c", "q":
			return m, tea.Quit
		case "tab":
			if m.manager == pipManager {
				m.manager = aptManager
			} else {
				m.manager = pipManager
			}
			m.loading = true
			m.remote = false
			m.packages = nil
			m.filtered = nil
			m.cursor, m.offset = 0, 0
			m.detail = detail{}
			m.status = "Loading installed " + m.manager.String() + " packages…"
			return m, loadCmd(m.manager)
		case "r":
			m.loading = true
			m.status = "Refreshing…"
			return m, loadCmd(m.manager)
		case "/":
			m.mode = filterMode
			m.input.SetValue("")
			m.input.Placeholder = "filter installed list"
			m.input.Focus()
			return m, textinput.Blink
		case "s":
			if m.manager == pipManager {
				m.mode = searchMode
				m.input.SetValue("")
				m.input.Placeholder = "search PyPI by name or keyword"
				m.input.Focus()
				return m, textinput.Blink
			}
		case "i":
			m.pendingAction = "install"
			m.mode = installMode
			m.input.SetValue(func() string {
				if m.remote {
					return m.currentName()
				}
				return ""
			}())
			m.input.Placeholder = "package name (version specifiers allowed)"
			m.input.Focus()
			return m, textinput.Blink
		case "u":
			if m.currentName() != "" {
				m.pendingAction = "uninstall"
				m.input.SetValue(m.currentName())
				m.mode = confirmMode
			}
		case "U":
			if m.currentName() != "" {
				m.pendingAction = "upgrade"
				m.input.SetValue(m.currentName())
				m.mode = confirmMode
			}
		case "?":
			m.mode = helpMode
		case "esc":
			if m.remote {
				m.loading = true
				m.remote = false
				m.status = "Returning to installed packages…"
				return m, loadCmd(m.manager)
			}
			m.showOutput = ""
			m.err = nil
		case "up", "k":
			m.move(-1)
			return m, m.loadCurrentDetail()
		case "down", "j":
			m.move(1)
			return m, m.loadCurrentDetail()
		case "pgup":
			m.move(-m.visibleRows())
			return m, m.loadCurrentDetail()
		case "pgdown":
			m.move(m.visibleRows())
			return m, m.loadCurrentDetail()
		case "home", "g":
			m.cursor = 0
			m.offset = 0
			return m, m.loadCurrentDetail()
		case "end", "G":
			if len(m.filtered) > 0 {
				m.cursor = len(m.filtered) - 1
				m.ensureVisible()
			}
			return m, m.loadCurrentDetail()
		case "ctrl+d", "]":
			m.viewport.HalfViewDown()
		case "ctrl+u", "[":
			m.viewport.HalfViewUp()
		}
	}
	return m, cmd
}

func (m *model) applyFilter(q string) {
	m.filtered = nil
	q = strings.ToLower(q)
	for _, p := range m.packages {
		if q == "" || strings.Contains(strings.ToLower(p.Name), q) || strings.Contains(strings.ToLower(p.Version), q) {
			m.filtered = append(m.filtered, p)
		}
	}
	sort.SliceStable(m.filtered, func(i, j int) bool { return strings.ToLower(m.filtered[i].Name) < strings.ToLower(m.filtered[j].Name) })
	m.cursor, m.offset = 0, 0
}
func (m model) currentName() string {
	if m.cursor >= 0 && m.cursor < len(m.filtered) {
		return m.filtered[m.cursor].Name
	}
	return ""
}
func (m model) loadCurrentDetail() tea.Cmd {
	if len(m.filtered) == 0 {
		return nil
	}
	p := m.filtered[m.cursor]
	return detailsCmd(m.manager, p, m.remote)
}
func (m *model) move(n int) {
	if len(m.filtered) == 0 {
		return
	}
	m.cursor += n
	if m.cursor < 0 {
		m.cursor = 0
	}
	if m.cursor >= len(m.filtered) {
		m.cursor = len(m.filtered) - 1
	}
	m.ensureVisible()
}
func (m *model) ensureVisible() {
	rows := m.visibleRows()
	if m.cursor < m.offset {
		m.offset = m.cursor
	}
	if m.cursor >= m.offset+rows {
		m.offset = m.cursor - rows + 1
	}
	if m.offset < 0 {
		m.offset = 0
	}
}
func (m model) visibleRows() int {
	r := m.height - 9
	if r < 3 {
		return 3
	}
	return r
}
func (m *model) resize() {
	left := m.width * 42 / 100
	if left < 28 {
		left = 28
	}
	right := m.width - left - 3
	if right < 20 {
		right = 20
	}
	m.viewport.Width = right - 4
	m.viewport.Height = m.visibleRows() - 2
}

func (m model) View() string {
	if m.width == 0 {
		return "tooln is starting…"
	}
	header := titleStyle.Render(" tooln ") + "  " + tab("pip", m.manager == pipManager) + " " + tab("apt", m.manager == aptManager) + lipgloss.NewStyle().Foreground(muted).Render("   Tab switches manager")
	if m.mode == helpMode {
		return header + "\n" + m.helpView()
	}
	bodyHeight := m.height - 5
	if bodyHeight < 6 {
		bodyHeight = 6
	}
	leftW := m.width * 42 / 100
	if leftW < 28 {
		leftW = 28
	}
	rightW := m.width - leftW - 1
	if rightW < 20 {
		rightW = 20
	}
	left := borderStyle.Width(leftW - 2).Height(bodyHeight).Render(m.listView(leftW-4, bodyHeight))
	right := borderStyle.Width(rightW - 2).Height(bodyHeight).Render(m.detailView(rightW-4, bodyHeight))
	body := lipgloss.JoinHorizontal(lipgloss.Top, left, right)
	prompt := ""
	switch m.mode {
	case filterMode:
		prompt = "Filter: " + m.input.View() + "  (Enter keep, Esc clear)"
	case searchMode:
		prompt = "PyPI search: " + m.input.View() + "  (Enter search, Esc cancel)"
	case installMode:
		prompt = "Install " + m.manager.String() + ": " + m.input.View() + "  (Enter continue, Esc cancel)"
	case confirmMode:
		prompt = lipgloss.NewStyle().Bold(true).Foreground(lipgloss.Color("220")).Render(fmt.Sprintf("Confirm %s %q using %s? [y/Enter] yes  [n/Esc] no", m.pendingAction, m.input.Value(), m.manager))
	default:
		prompt = m.status
	}
	if m.loading {
		prompt = m.spinner.View() + " " + m.status
	}
	if m.err != nil {
		prompt = lipgloss.NewStyle().Foreground(red).Render("Error: " + oneLine(m.err.Error()))
	}
	footer := lipgloss.NewStyle().Foreground(muted).Render("↑/↓ navigate  [/] details  / filter  s search  i install  u uninstall  U upgrade  r refresh  Tab manager  ? help  q quit")
	return header + "\n" + body + "\n" + truncate(prompt, m.width) + "\n" + truncate(footer, m.width)
}
func tab(s string, on bool) string {
	st := lipgloss.NewStyle().Padding(0, 1)
	if on {
		st = st.Bold(true).Foreground(lipgloss.Color("0")).Background(cyan)
	} else {
		st = st.Foreground(muted)
	}
	return st.Render(s)
}
func (m model) listView(w, h int) string {
	title := fmt.Sprintf("%s packages (%d)", strings.ToUpper(m.manager.String()), len(m.filtered))
	if m.remote {
		title = "PyPI search results"
	}
	lines := []string{lipgloss.NewStyle().Bold(true).Foreground(cyan).Render(title), lipgloss.NewStyle().Foreground(muted).Render(pad("PACKAGE", max(8, w-15)) + " VERSION")}
	rows := m.visibleRows()
	end := min(len(m.filtered), m.offset+rows)
	for i := m.offset; i < end; i++ {
		p := m.filtered[i]
		line := pad(p.Name, max(8, w-15)) + " " + truncate(p.Version, 13)
		if i == m.cursor {
			line = selectedStyle.Width(w).Render(truncate(line, w))
		}
		lines = append(lines, line)
	}
	if len(m.filtered) == 0 {
		lines = append(lines, lipgloss.NewStyle().Foreground(muted).Render("No packages to display"))
	}
	return strings.Join(lines, "\n")
}
func (m model) detailView(w, h int) string {
	if m.loading && m.currentName() == "" {
		return "Loading package data…"
	}
	if m.currentName() == "" {
		return "Select a package to view details."
	}
	d := m.detail
	if m.detailKey != m.currentName() {
		return "Loading details for " + m.currentName() + "…"
	}
	lines := []string{lipgloss.NewStyle().Bold(true).Foreground(cyan).Render("Package details"), "Name:       " + d.Name, "Version:    " + d.Version}
	if d.Summary != "" {
		lines = append(lines, "Summary:    "+d.Summary)
	}
	if d.Homepage != "" {
		lines = append(lines, "Homepage:   "+d.Homepage)
	}
	if d.Location != "" {
		lines = append(lines, "Location:   "+d.Location)
	}
	lines = append(lines, "", "Dependencies (direct):")
	if len(d.Dependencies) == 0 {
		lines = append(lines, "  (none declared)")
	} else {
		for _, x := range d.Dependencies {
			lines = append(lines, "  • "+x)
		}
	}
	lines = append(lines, "", "Required by:")
	if len(d.RequiredBy) == 0 {
		lines = append(lines, "  (none)")
	} else {
		for _, x := range d.RequiredBy {
			lines = append(lines, "  • "+x)
		}
	}
	if d.Extra != "" {
		lines = append(lines, "", d.Extra)
	}
	if m.showOutput != "" {
		lines = append(lines, "", "Last operation output:", m.showOutput)
	}
	content := wrapLines(strings.Join(lines, "\n"), w)
	m.viewport.Width = w
	m.viewport.Height = h
	m.viewport.SetContent(content)
	return m.viewport.View()
}
func (m model) helpView() string {
	help := `KEYBOARD HELP

Navigation
  ↑/↓ or j/k     Move selection       PgUp/PgDn  Move one screen
  g/Home         First package         G/End      Last package
  [ / ]          Scroll package details up / down
  Tab            Switch pip / apt      Esc        Leave PyPI results / clear message

Package actions
  /              Real-time local filter
  s              Search PyPI (pip view); exact names always supported
  i              Install a package (name/version specifier prompt)
  u              Uninstall selected package (confirmation required)
  U              Upgrade selected package (confirmation required)
  r              Refresh installed packages and versions

Other
  ?              Toggle this help      q/Ctrl+C   Quit

All changes invoke real python3 -m pip, apt-get, dpkg-query, and apt-cache commands.
Pip uninstall also examines the installed dependency graph and removes dependencies
that become unused (while preserving pip, setuptools, and wheel).

Press ? or Esc to return.`
	return borderStyle.Width(max(30, m.width-4)).Render(help)
}
func pad(s string, n int) string {
	if len([]rune(s)) >= n {
		return truncate(s, n)
	}
	return s + strings.Repeat(" ", n-len([]rune(s)))
}
func truncate(s string, n int) string {
	r := []rune(s)
	if n <= 0 {
		return ""
	}
	if len(r) <= n {
		return s
	}
	if n == 1 {
		return "…"
	}
	return string(r[:n-1]) + "…"
}
func oneLine(s string) string { return strings.Join(strings.Fields(s), " ") }
func wrapLines(s string, w int) string {
	if w < 10 {
		return s
	}
	var out []string
	for _, line := range strings.Split(s, "\n") {
		for len([]rune(line)) > w {
			r := []rune(line)
			cut := w
			for i := w; i > w/2; i-- {
				if r[i-1] == ' ' {
					cut = i
					break
				}
			}
			out = append(out, strings.TrimSpace(string(r[:cut])))
			line = strings.TrimSpace(string(r[cut:]))
		}
		out = append(out, line)
	}
	return strings.Join(out, "\n")
}

// Ensure command contexts have a finite lifetime even if future commands are added.
var _ = context.Background
var _ = time.Second
