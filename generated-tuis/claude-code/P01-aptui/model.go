package main

import (
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/help"
	"github.com/charmbracelet/bubbles/spinner"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
)

// focusArea identifies which pane receives navigation keys. Focus never hides
// anything: it only decides where ↑/↓ and the scroll keys apply.
type focusArea int

const (
	focusList focusArea = iota
	focusDetails
	focusLog
)

func (f focusArea) String() string {
	switch f {
	case focusDetails:
		return "details"
	case focusLog:
		return "log"
	default:
		return "list"
	}
}

// next cycles focus in Tab order.
func (f focusArea) next() focusArea {
	return (f + 1) % 3
}

// pendingConfirm is an operation awaiting the user's y/n. It is rendered inline
// in the status line rather than as an overlay, so no information is hidden
// while the prompt is up.
type pendingConfirm struct {
	op      operation
	prompt  string
	warning string
}

// logLine is one line of apt output, tagged with its stream so stderr can be
// distinguished visually.
type logLine struct {
	text   string
	stderr bool
	cmd    bool
}

// logCapacity bounds the retained apt output. A full dist-upgrade emits a few
// thousand lines; keeping this many means the whole of any realistic operation
// stays scrollable without unbounded growth.
const logCapacity = 5000

type model struct {
	keys   keyMap
	styles styles
	help   help.Model

	store *store

	// Terminal geometry and the derived pane sizes.
	width  int
	height int
	layout layout
	ready  bool

	focus       focusArea
	searchFocus bool

	search  textinput.Model
	details viewport.Model
	logView viewport.Model
	spin    spinner.Model

	// cursor is the index into store.visible(); listOffset is the first visible
	// row, so the list scrolls without paginating.
	cursor     int
	listOffset int

	// Details state. detailsGen is bumped on every selection change; responses
	// carrying an older generation are discarded, so holding ↓ does not leave a
	// stale record on screen.
	detailsGen     int
	detailsFor     string
	detailsPkg     *pkgDetails
	detailsErr     error
	detailsLoading bool

	// Operation state.
	runner  *runner
	busy    bool
	current operation
	logs    []logLine

	// logDirty marks the log content as needing a re-render; logFollow records
	// that the pane should jump to the newest output on the next render.
	logDirty  bool
	logFollow bool

	confirm *pendingConfirm

	// Loading state for the initial scans, so the header can say what is
	// happening on a cold start.
	loadingInstalled  bool
	loadingAvailable  bool
	loadingUpgradable bool

	aptListsPresent bool
	aptListsChecked bool

	status      string
	statusLevel statusLevel
	statusToken int

	showHelp bool

	// startupWarning holds a privilege or missing-tool warning, shown until the
	// user does something that replaces the status line.
	startupWarning string
}

func newModel() *model {
	search := textinput.New()
	search.Prompt = ""
	search.Placeholder = "type to filter by name or description"
	search.CharLimit = 128

	sp := spinner.New()
	sp.Spinner = spinner.Dot

	h := help.New()
	h.ShowAll = false

	m := &model{
		keys:   defaultKeyMap(),
		styles: newStyles(),
		help:   h,
		store:  newStore(),
		search: search,
		spin:   sp,
		runner: &runner{},

		loadingInstalled:  true,
		loadingAvailable:  true,
		loadingUpgradable: true,
	}

	m.help.Styles.ShortKey = m.styles.PaneTitle
	m.help.Styles.FullKey = m.styles.PaneTitle
	m.help.Styles.ShortDesc = m.styles.HeaderCount
	m.help.Styles.FullDesc = m.styles.HeaderCount

	if warn := checkPrivileges(); warn != "" {
		m.startupWarning = warn
		m.status = warn
		m.statusLevel = statusWarn
	}
	if missing := missingTools(); len(missing) > 0 {
		m.startupWarning = "missing required tools: " + strings.Join(missing, ", ")
		m.status = m.startupWarning
		m.statusLevel = statusError
	}

	return m
}

func (m *model) Init() tea.Cmd {
	// The installed scan comes first because it is fast and makes the list
	// usable immediately; the 63k-package available index and the upgrade plan
	// stream in behind it.
	return tea.Batch(
		m.spin.Tick,
		probeAptListsCmd(),
		loadInstalledCmd(),
		loadAvailableCmd(),
		loadUpgradableCmd(),
	)
}

// selected returns the package under the cursor, or nil when the list is empty.
func (m *model) selected() *pkg {
	view := m.store.visible()
	if len(view) == 0 {
		return nil
	}
	if m.cursor < 0 || m.cursor >= len(view) {
		return nil
	}
	return view[m.cursor]
}

// listHeight is the number of package rows that fit in the body pane.
func (m *model) listHeight() int {
	// One row of the pane is the column header.
	return max(1, m.layout.BodyHeight-1)
}

// clampCursor keeps the cursor inside the current view and scrolls the window so
// the cursor stays visible.
func (m *model) clampCursor() {
	n := len(m.store.visible())
	if n == 0 {
		m.cursor, m.listOffset = 0, 0
		return
	}

	m.cursor = clamp(m.cursor, 0, n-1)

	h := m.listHeight()
	// Keep the cursor within the window.
	if m.cursor < m.listOffset {
		m.listOffset = m.cursor
	}
	if m.cursor >= m.listOffset+h {
		m.listOffset = m.cursor - h + 1
	}
	// Never leave blank space below when there is content to show above.
	maxOffset := max(0, n-h)
	m.listOffset = clamp(m.listOffset, 0, maxOffset)
}

// moveCursor moves the selection by delta rows and triggers a details fetch when
// the selection actually changes.
func (m *model) moveCursor(delta int) tea.Cmd {
	before := m.selected()
	m.cursor += delta
	m.clampCursor()

	after := m.selected()
	if before == after {
		return nil
	}
	return m.requestDetails()
}

// requestDetails starts a details fetch for the current selection, unless the
// pane already shows it.
func (m *model) requestDetails() tea.Cmd {
	p := m.selected()
	if p == nil {
		m.detailsFor = ""
		m.detailsPkg = nil
		m.detailsErr = nil
		m.detailsLoading = false
		m.details.SetContent("")
		return nil
	}

	if m.detailsFor == p.Name && (m.detailsPkg != nil || m.detailsErr != nil) {
		return nil
	}

	m.detailsGen++
	m.detailsFor = p.Name
	m.detailsPkg = nil
	m.detailsErr = nil
	m.detailsLoading = true
	m.details.GotoTop()

	return loadDetailsCmd(m.detailsGen, p.Name)
}

// refreshDetails forces a re-fetch of the current selection, used after an
// operation changes the package's state.
func (m *model) refreshDetails() tea.Cmd {
	m.detailsFor = ""
	return m.requestDetails()
}

// appendLog adds lines to the apt output pane, trimming to logCapacity. The view
// follows the newest output unless the user has scrolled up to read back, in
// which case their position is left alone.
//
// Rendering is deferred: apt can emit output far faster than the terminal
// redraws, and re-rendering the whole retained log per line would be quadratic in
// the number of lines. The pane is marked dirty here and re-rendered at most once
// per frame, from View.
func (m *model) appendLog(lines ...logLine) {
	m.logFollow = m.logFollow || m.logView.AtBottom()

	m.logs = append(m.logs, lines...)
	if len(m.logs) > logCapacity {
		m.logs = m.logs[len(m.logs)-logCapacity:]
	}
	m.logDirty = true
}

// syncLogView re-renders the log pane's content if it has changed since the last
// frame. SetYOffset re-clamps the scroll position against the new content, which
// matters on resize: an offset valid for the old geometry can point past the end
// of the new one, leaving the pane blank.
func (m *model) syncLogView() {
	if !m.logDirty {
		return
	}
	m.logDirty = false

	offset := m.logView.YOffset
	m.logView.SetContent(m.logContent())

	if m.logFollow {
		m.logFollow = false
		m.logView.GotoBottom()
		return
	}
	m.logView.SetYOffset(offset)
}

// setStatus replaces the status line and schedules its expiry.
func (m *model) setStatus(text string, level statusLevel) tea.Cmd {
	m.status = text
	m.statusLevel = level
	m.statusToken++

	token := m.statusToken
	// Errors persist until something replaces them; transient notices fade so
	// the line returns to showing the startup warning or nothing.
	if level == statusError {
		return nil
	}
	return tea.Tick(6*time.Second, func(time.Time) tea.Msg {
		return statusExpiredMsg{token: token}
	})
}

// statusOrDefault is what the status line shows when there is no transient
// message: the startup warning if there is one, otherwise a hint.
func (m *model) statusOrDefault() (string, statusLevel) {
	if m.status != "" {
		return m.status, m.statusLevel
	}
	if m.startupWarning != "" {
		return m.startupWarning, statusWarn
	}
	if !m.aptListsPresent && m.aptListsChecked {
		return "no package indexes found — press R to run apt-get update", statusWarn
	}
	return "press ? for help", statusInfo
}
