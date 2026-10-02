// Package ui implements the tooln TUI on top of Bubble Tea, Bubbles and
// Lip Gloss.
package ui

import (
	"context"
	"fmt"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/key"
	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"
	"github.com/tooln/tooln/internal/manager"
	"github.com/tooln/tooln/internal/pkgs"
)

type statusKind int

const (
	statusNone statusKind = iota
	statusInfo
	statusSuccess
	statusError
)

type model struct {
	width  int
	height int
	ready  bool

	pip pkgs.Manager
	apt pkgs.Manager

	currentIndex int // 0 = pip, 1 = apt

	packages []pkgs.Package
	filtered []pkgs.Package
	cursor   int
	offset   int

	details        *pkgs.PackageInfo
	detailsLoading bool
	detailsErr     error
	detailsSeq     int
	detailsScroll  int
	focus          focusArea

	filterInput textinput.Model
	searchInput textinput.Model

	searchResults []pkgs.SearchResult
	searchCursor  int
	searchOffset  int
	searchLoading bool
	searchSeq     int

	mode mode

	confirmAction string
	confirmTarget string

	busyMsg    string
	statusMsg  string
	statusKind statusKind

	helpScroll int

	keys keyMap
}

// New creates the root model with pip and apt backends.
func New() *model {
	filterInput := textinput.New()
	filterInput.Prompt = "/ "
	filterInput.Placeholder = "filter packages…"
	filterInput.CharLimit = 128

	searchInput := textinput.New()
	searchInput.Prompt = "> "
	searchInput.CharLimit = 128
	searchInput.Placeholder = "search package name/keyword…"

	return &model{
		pip:          manager.NewPip(),
		apt:          manager.NewApt(),
		currentIndex: 0,
		filterInput:  filterInput,
		searchInput:  searchInput,
		keys:         newKeyMap(),
		focus:        focusList,
	}
}

// current returns the active manager.
func (m *model) current() pkgs.Manager {
	if m.currentIndex == 0 {
		return m.pip
	}
	return m.apt
}

// Init loads the initial package list.
func (m *model) Init() tea.Cmd {
	return loadPackages(m.pip)
}

// --- asynchronous commands --------------------------------------------------

func loadPackages(mgr pkgs.Manager) tea.Cmd {
	return func() tea.Msg {
		ctx := context.Background()
		list, err := mgr.List(ctx)
		return packagesLoadedMsg{manager: mgr.Name(), packages: list, err: err}
	}
}

func loadDetails(mgr pkgs.Manager, name string, seq int) tea.Cmd {
	return func() tea.Msg {
		info, err := mgr.Info(context.Background(), name)
		return detailsLoadedMsg{seq: seq, info: info, err: err}
	}
}

func runSearch(mgr pkgs.Manager, query string) tea.Cmd {
	return func() tea.Msg {
		results, err := mgr.Search(context.Background(), query)
		return searchDoneMsg{query: query, results: results, err: err}
	}
}

func runOp(mgr pkgs.Manager, action, name string) tea.Cmd {
	return func() tea.Msg {
		var (
			res pkgs.OpResult
			err error
		)
		switch action {
		case "install":
			res, err = mgr.Install(context.Background(), name)
		case "uninstall":
			res, err = mgr.Uninstall(context.Background(), name)
		case "upgrade":
			res, err = mgr.Upgrade(context.Background(), name)
		}
		return opDoneMsg{manager: mgr.Name(), action: action, target: name, result: res, err: err}
	}
}

// --- Update ----------------------------------------------------------------

func (m *model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.ready = true
		return m, nil

	case tea.KeyMsg:
		return m.handleKey(msg)

	case packagesLoadedMsg:
		if msg.manager != m.current().Name() {
			return m, nil
		}
		if msg.err != nil {
			m.setStatus(statusError, msg.err.Error())
		}
		m.packages = msg.packages
		m.filterInput.SetValue("")
		m.applyFilter("")
		m.clampCursor()
		m.details = nil
		m.detailsErr = nil
		m.detailsScroll = 0
		return m, m.scheduleDetails()

	case detailsLoadedMsg:
		if msg.seq != m.detailsSeq {
			return m, nil
		}
		m.detailsLoading = false
		m.detailsErr = msg.err
		m.details = msg.info
		m.detailsScroll = 0
		return m, nil

	case debounceMsg:
		if msg.seq != m.detailsSeq {
			return m, nil
		}
		if len(m.filtered) == 0 || m.cursor < 0 || m.cursor >= len(m.filtered) {
			return m, nil
		}
		name := m.filtered[m.cursor].Name
		m.detailsLoading = true
		return m, loadDetails(m.current(), name, msg.seq)

	case searchDebounceMsg:
		if msg.seq != m.searchSeq {
			return m, nil
		}
		q := strings.TrimSpace(m.searchInput.Value())
		if q == "" {
			m.searchResults = nil
			m.searchLoading = false
			return m, nil
		}
		m.searchLoading = true
		return m, runSearch(m.current(), q)

	case searchDoneMsg:
		if msg.query != strings.TrimSpace(m.searchInput.Value()) {
			return m, nil
		}
		m.searchLoading = false
		m.searchResults = msg.results
		m.searchCursor = 0
		m.searchOffset = 0
		if msg.err != nil {
			m.setStatus(statusError, msg.err.Error())
		}
		return m, nil

	case opDoneMsg:
		m.busyMsg = ""
		switch {
		case msg.err != nil:
			m.setStatus(statusError, fmt.Sprintf("%s %s: %s", msg.action, msg.target, firstLine(msg.err.Error())))
		default:
			extra := ""
			if len(msg.result.Removed) > 0 {
				extra = fmt.Sprintf(" — removed no-longer-needed: %s", strings.Join(msg.result.Removed, ", "))
			}
			m.setStatus(statusSuccess, fmt.Sprintf("%s %q completed%s", msg.action, msg.target, extra))
		}
		return m, loadPackages(m.current())
	}

	return m, nil
}

// scheduleDetails clears the stale details pane and debounces a fresh load.
func (m *model) scheduleDetails() tea.Cmd {
	m.details = nil
	m.detailsErr = nil
	m.detailsLoading = true
	m.detailsScroll = 0
	m.detailsSeq++
	seq := m.detailsSeq
	return tea.Tick(60*time.Millisecond, func(time.Time) tea.Msg {
		return debounceMsg{seq: seq}
	})
}

// handleKey dispatches a key press according to the current mode.
func (m *model) handleKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	// Global quit (not in text-input modes, where typing 'q' must work).
	if m.busyMsg == "" && key.Matches(msg, m.keys.Quit) && m.mode != modeFilter && m.mode != modeSearch {
		return m, tea.Quit
	}

	switch m.mode {
	case modeBrowse:
		return m.handleBrowse(msg)
	case modeFilter:
		return m.handleFilter(msg)
	case modeSearch:
		return m.handleSearch(msg)
	case modeConfirm:
		return m.handleConfirm(msg)
	case modeHelp:
		return m.handleHelp(msg)
	}
	return m, nil
}

func (m *model) handleBrowse(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	if m.busyMsg != "" {
		return m, nil // block input while a mutating operation is running
	}

	if m.focus == focusDetails {
		switch {
		case key.Matches(msg, m.keys.Up), key.Matches(msg, m.keys.Down):
			m.scrollDetails(scrollDelta(msg, m.keys))
			return m, nil
		case key.Matches(msg, m.keys.PageUp):
			m.scrollDetails(-m.detailsRows())
			return m, nil
		case key.Matches(msg, m.keys.PageDown):
			m.scrollDetails(m.detailsRows())
			return m, nil
		case key.Matches(msg, m.keys.Cancel), key.Matches(msg, m.keys.Details):
			m.focus = focusList
			return m, nil
		case key.Matches(msg, m.keys.Help):
			m.enterHelp()
			return m, nil
		}
		return m, nil
	}

	switch {
	case key.Matches(msg, m.keys.PrevManager):
		return m, m.switchManager(-1)
	case key.Matches(msg, m.keys.NextManager):
		return m, m.switchManager(1)
	case key.Matches(msg, m.keys.Up):
		m.moveCursor(-1)
		return m, m.scheduleDetails()
	case key.Matches(msg, m.keys.Down):
		m.moveCursor(1)
		return m, m.scheduleDetails()
	case key.Matches(msg, m.keys.PageUp):
		m.moveCursor(-m.pageSize())
		return m, m.scheduleDetails()
	case key.Matches(msg, m.keys.PageDown):
		m.moveCursor(m.pageSize())
		return m, m.scheduleDetails()
	case key.Matches(msg, m.keys.Top):
		m.cursor = 0
		m.offset = 0
		return m, m.scheduleDetails()
	case key.Matches(msg, m.keys.Bottom):
		m.cursor = maxInt(0, len(m.filtered)-1)
		m.scrollToCursor()
		return m, m.scheduleDetails()
	case key.Matches(msg, m.keys.Filter):
		m.enterFilter()
		return m, nil
	case key.Matches(msg, m.keys.Search), key.Matches(msg, m.keys.Install):
		m.enterSearch()
		return m, nil
	case key.Matches(msg, m.keys.Remove):
		return m.startConfirm("uninstall", m.selectedName())
	case key.Matches(msg, m.keys.Upgrade):
		if m.current().SupportsUpgrade() {
			return m.startConfirm("upgrade", m.selectedName())
		}
		return m, nil
	case key.Matches(msg, m.keys.Refresh):
		m.setStatus(statusInfo, "refreshing…")
		return m, loadPackages(m.current())
	case key.Matches(msg, m.keys.Details):
		m.focus = focusDetails
		m.detailsScroll = 0
		return m, nil
	case key.Matches(msg, m.keys.Help):
		m.enterHelp()
		return m, nil
	}
	return m, nil
}

func (m *model) handleFilter(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch {
	case msg.Type == tea.KeyCtrlC:
		return m, tea.Quit
	case key.Matches(msg, m.keys.Cancel):
		m.filterInput.SetValue("")
		m.applyFilter("")
		m.mode = modeBrowse
		return m, nil
	case key.Matches(msg, m.keys.Details):
		m.mode = modeBrowse
		return m, nil
	}
	var cmd tea.Cmd
	m.filterInput, cmd = m.filterInput.Update(msg)
	m.applyFilter(m.filterInput.Value())
	return m, cmd
}

func (m *model) handleSearch(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch {
	case msg.Type == tea.KeyCtrlC:
		return m, tea.Quit
	case key.Matches(msg, m.keys.Cancel):
		m.mode = modeBrowse
		m.searchResults = nil
		m.searchCursor = 0
		m.searchOffset = 0
		return m, nil
	case key.Matches(msg, m.keys.Up):
		if len(m.searchResults) > 0 {
			m.searchCursor = clamp(m.searchCursor-1, 0, len(m.searchResults)-1)
			m.scrollSearchCursor()
		}
		return m, nil
	case key.Matches(msg, m.keys.Down):
		if len(m.searchResults) > 0 {
			m.searchCursor = clamp(m.searchCursor+1, 0, len(m.searchResults)-1)
			m.scrollSearchCursor()
		}
		return m, nil
	case key.Matches(msg, m.keys.Details):
		if len(m.searchResults) > 0 {
			if m.searchCursor < 0 || m.searchCursor >= len(m.searchResults) {
				m.searchCursor = 0
			}
			return m.startConfirm("install", m.searchResults[m.searchCursor].Name)
		}
		return m, m.triggerSearch()
	}

	old := m.searchInput.Value()
	var cmd tea.Cmd
	m.searchInput, cmd = m.searchInput.Update(msg)
	if m.searchInput.Value() != old {
		m.searchResults = nil
		m.searchCursor = 0
		m.searchOffset = 0
		m.searchLoading = true
		m.searchSeq++
		seq := m.searchSeq
		return m, tea.Batch(cmd, tea.Tick(400*time.Millisecond, func(time.Time) tea.Msg {
			return searchDebounceMsg{seq: seq}
		}))
	}
	return m, cmd
}

func (m *model) handleConfirm(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch {
	case key.Matches(msg, m.keys.Confirm):
		m.busyMsg = busyText(m.confirmAction, m.confirmTarget)
		m.mode = modeBrowse
		m.focus = focusList
		return m, runOp(m.current(), m.confirmAction, m.confirmTarget)
	case key.Matches(msg, m.keys.Cancel):
		m.mode = modeBrowse
		return m, nil
	case key.Matches(msg, m.keys.Quit):
		return m, tea.Quit
	}
	return m, nil
}

func (m *model) handleHelp(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch {
	case key.Matches(msg, m.keys.Up), key.Matches(msg, m.keys.Down):
		m.scrollHelp(scrollDelta(msg, m.keys))
		return m, nil
	case key.Matches(msg, m.keys.PageUp):
		m.scrollHelp(-m.helpRows())
		return m, nil
	case key.Matches(msg, m.keys.PageDown):
		m.scrollHelp(m.helpRows())
		return m, nil
	case key.Matches(msg, m.keys.Help), key.Matches(msg, m.keys.Cancel), key.Matches(msg, m.keys.Quit):
		m.mode = modeBrowse
		return m, nil
	}
	return m, nil
}

// --- helpers ---------------------------------------------------------------

func scrollDelta(msg tea.KeyMsg, k keyMap) int {
	if key.Matches(msg, k.Up) {
		return -1
	}
	if key.Matches(msg, k.Down) {
		return 1
	}
	return 0
}

func (m *model) selectedName() string {
	if m.cursor >= 0 && m.cursor < len(m.filtered) {
		return m.filtered[m.cursor].Name
	}
	return ""
}

func (m *model) switchManager(delta int) tea.Cmd {
	m.currentIndex = (m.currentIndex + delta + 2) % 2
	m.cursor = 0
	m.offset = 0
	m.details = nil
	m.detailsScroll = 0
	m.focus = focusList
	m.mode = modeBrowse
	m.setStatus(statusInfo, "loading "+m.current().Name()+" packages…")
	return loadPackages(m.current())
}

func (m *model) moveCursor(delta int) {
	if len(m.filtered) == 0 {
		return
	}
	m.cursor = clamp(m.cursor+delta, 0, len(m.filtered)-1)
	m.scrollToCursor()
}

func (m *model) clampCursor() {
	if len(m.filtered) == 0 {
		m.cursor = 0
		return
	}
	m.cursor = clamp(m.cursor, 0, len(m.filtered)-1)
	m.scrollToCursor()
}

func (m *model) pageSize() int {
	h := m.listRows()
	if h < 3 {
		return 1
	}
	return h - 1
}

func (m *model) scrollToCursor() {
	h := m.listRows()
	if h <= 0 {
		return
	}
	if m.cursor < m.offset {
		m.offset = m.cursor
	}
	if m.cursor >= m.offset+h {
		m.offset = m.cursor - h + 1
	}
}

func (m *model) scrollSearchCursor() {
	h := m.listRows()
	if h <= 0 {
		return
	}
	if m.searchCursor < m.searchOffset {
		m.searchOffset = m.searchCursor
	}
	if m.searchCursor >= m.searchOffset+h {
		m.searchOffset = m.searchCursor - h + 1
	}
}

func (m *model) scrollDetails(delta int) {
	maxOff := maxInt(0, len(detailsLines(m))-m.detailsRows())
	m.detailsScroll = clamp(m.detailsScroll+delta, 0, maxOff)
}

func (m *model) scrollHelp(delta int) {
	maxOff := maxInt(0, len(helpLines(m))-m.helpRows())
	m.helpScroll = clamp(m.helpScroll+delta, 0, maxOff)
}

func (m *model) enterFilter() {
	m.filterInput.SetValue("")
	m.filterInput.Width = maxInt(10, m.width-6)
	m.filterInput.Focus()
	m.filterInput.CursorEnd()
	m.mode = modeFilter
	m.applyFilter("")
}

func (m *model) enterSearch() {
	m.searchInput.SetValue("")
	m.searchInput.Width = maxInt(10, m.width-28)
	m.searchInput.Focus()
	m.searchInput.CursorEnd()
	m.searchResults = nil
	m.searchCursor = 0
	m.searchOffset = 0
	m.searchLoading = false
	if m.current().Name() == "apt" {
		m.searchInput.Placeholder = "search apt package name/keyword…"
	} else {
		m.searchInput.Placeholder = "search PyPI package name/keyword…"
	}
	m.mode = modeSearch
}

func (m *model) enterHelp() {
	m.helpScroll = 0
	m.mode = modeHelp
}

func (m *model) startConfirm(action, target string) (tea.Model, tea.Cmd) {
	if target == "" {
		return m, nil
	}
	m.confirmAction = action
	m.confirmTarget = target
	m.mode = modeConfirm
	return m, nil
}

func (m *model) applyFilter(query string) {
	q := strings.ToLower(strings.TrimSpace(query))
	if q == "" {
		m.filtered = m.packages
	} else {
		out := make([]pkgs.Package, 0, len(m.packages))
		for _, p := range m.packages {
			if strings.Contains(strings.ToLower(p.Name), q) {
				out = append(out, p)
			}
		}
		m.filtered = out
	}
	if m.cursor >= len(m.filtered) {
		m.cursor = maxInt(0, len(m.filtered)-1)
	}
	if m.cursor < 0 {
		m.cursor = 0
	}
	m.scrollToCursor()
}

func (m *model) triggerSearch() tea.Cmd {
	q := strings.TrimSpace(m.searchInput.Value())
	if q == "" {
		return nil
	}
	m.searchLoading = true
	return runSearch(m.current(), q)
}

func (m *model) setStatus(kind statusKind, msg string) {
	m.statusKind = kind
	m.statusMsg = msg
}

func busyText(action, target string) string {
	switch action {
	case "install":
		return fmt.Sprintf("Installing %s…", target)
	case "uninstall":
		return fmt.Sprintf("Uninstalling %s…", target)
	case "upgrade":
		return fmt.Sprintf("Upgrading %s…", target)
	}
	return "Working…"
}

func firstLine(s string) string {
	if i := strings.IndexByte(s, '\n'); i >= 0 {
		return strings.TrimSpace(s[:i])
	}
	return strings.TrimSpace(s)
}

func clamp(v, lo, hi int) int {
	if hi < lo {
		return lo
	}
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}

func maxInt(a, b int) int {
	if a > b {
		return a
	}
	return b
}
