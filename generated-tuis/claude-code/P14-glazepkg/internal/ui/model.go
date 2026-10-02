package ui

import (
	"fmt"
	"strings"
	"sync"

	"github.com/charmbracelet/bubbles/spinner"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"

	"tooln/internal/pkgmgr"
)

// mode is the input mode: exactly one is active, and it decides how key presses
// are interpreted.
type mode int

const (
	modeBrowse  mode = iota
	modeFilter       // typing into the list filter
	modeSearch       // typing a query for the package index
	modeInstall      // typing a package to install
	modeConfirm      // answering a yes/no question
	modeMenu         // choosing from the maintenance menu
)

// statusLevel tints the status line.
type statusLevel int

const (
	statusInfo statusLevel = iota
	statusGood
	statusWarning
	statusBad
	statusBusy
)

// confirmKind says what a pending confirmation will do once accepted.
type confirmKind int

const (
	confirmRemove confirmKind = iota
	confirmUpgrade
	confirmInstall
	confirmExtra
	confirmQuit
)

// pendingConfirm is a question awaiting a yes/no answer.
type pendingConfirm struct {
	kind    confirmKind
	title   string
	lines   []string
	targets []string
	plan    pkgmgr.Plan
	mgr     pkgmgr.Manager
	op      operation

	// awaiting is true while the orphan preview is still being computed; the
	// dialog stays usable and fills in the extra lines when the answer lands.
	awaiting bool
}

// Model is the root Bubble Tea model.
type Model struct {
	keys   keyMap
	styles styles

	tabs   []*tabState
	active int

	width, height int
	ready         bool

	mode      mode
	input     textinput.Model
	spin      spinner.Model
	confirm   *pendingConfirm
	menuItems []pkgmgr.Extra
	menuIndex int

	focusDetails bool
	showHelp     bool
	helpVP       viewport.Model
	showLog      bool
	logVP        viewport.Model

	// busy counts operations in flight. The label describes the newest one.
	busy      int
	busyLabel string
	// mutating is set while a plan that changes the environment is running, so
	// destructive keys are refused rather than queued behind it.
	mutating bool

	status      string
	statusLevel statusLevel

	// pendingSelect is a package to move the cursor to once the next list
	// arrives, so after an install you land on what you just installed.
	pendingSelect string
	// keepStatus suppresses the routine "N packages installed" message for one
	// refresh, so the outcome of an operation stays on screen.
	keepStatus bool

	logMu sync.Mutex
	logs  []string

	// logCh carries lines from the command goroutines into the update loop.
	logCh chan string
}

const maxLogLines = 4000

// New builds an empty model. Call SetManagers before running it: the managers
// need the model's Logger, so they are attached after construction.
func New() *Model {
	in := textinput.New()
	in.Prompt = ""
	in.CharLimit = 200

	sp := spinner.New()
	sp.Spinner = spinner.Dot

	m := &Model{
		keys:   newKeyMap(),
		styles: newStyles(),
		input:  in,
		spin:   sp,
		logCh:  make(chan string, 4096),
		status: "Starting up: probing package managers…",
	}
	m.statusLevel = statusBusy
	m.spin.Style = m.styles.statusBusy

	m.helpVP = viewport.New(40, 10)
	m.logVP = viewport.New(40, 8)
	m.helpVP.MouseWheelEnabled = false
	m.logVP.MouseWheelEnabled = false
	return m
}

// SetManagers registers the package managers the interface offers, in tab order.
func (m *Model) SetManagers(managers ...pkgmgr.Manager) {
	m.tabs = m.tabs[:0]
	for _, mgr := range managers {
		m.tabs = append(m.tabs, newTabState(mgr))
	}
	m.active = 0
}

// Logger returns the sink the managers write their command trace to. It is safe
// to call from the goroutines the commands run on.
func (m *Model) Logger() pkgmgr.Logger {
	return func(line string) {
		select {
		case m.logCh <- line:
		default: // never block a running command because the UI is behind
		}
	}
}

func (m *Model) Init() tea.Cmd {
	cmds := []tea.Cmd{m.spin.Tick, tickCmd(), m.waitForLog()}
	for _, t := range m.tabs {
		cmds = append(cmds, probeCmd(t.mgr))
		m.busy++
	}
	m.busyLabel = "probing package managers"
	return tea.Batch(cmds...)
}

// waitForLog blocks in a command until a log line is available, turning the
// channel into a stream of messages the update loop can consume.
func (m *Model) waitForLog() tea.Cmd {
	return func() tea.Msg { return logMsg{line: <-m.logCh} }
}

func (m *Model) cur() *tabState {
	if m.active < 0 || m.active >= len(m.tabs) {
		return nil
	}
	return m.tabs[m.active]
}

func (m *Model) tabByID(id string) *tabState {
	for _, t := range m.tabs {
		if t.mgr.ID() == id {
			return t
		}
	}
	return nil
}

// ------------------------------------------------------------------ update ----

func (m *Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		m.ready = true
		m.layout()
		return m, nil

	case tea.KeyMsg:
		return m.handleKey(msg)

	case spinner.TickMsg:
		var cmd tea.Cmd
		m.spin, cmd = m.spin.Update(msg)
		return m, cmd

	case tickMsg:
		// A steady tick keeps elapsed-time indicators moving during long
		// operations without each command having to schedule redraws.
		return m, tickCmd()

	case logMsg:
		m.appendLog(msg.line)
		return m, m.waitForLog()

	case probeDoneMsg:
		return m, m.onProbe(msg)

	case listMsg:
		return m, m.onList(msg)

	case searchMsg:
		return m, m.onSearch(msg)

	case detailsMsg:
		return m, m.onDetails(msg)

	case outdatedMsg:
		return m, m.onOutdated(msg)

	case opDoneMsg:
		return m, m.onOpDone(msg)

	case orphansMsg:
		return m, m.onOrphans(msg)

	case predictMsg:
		m.onPredict(msg)
		return m, nil
	}
	return m, nil
}

func (m *Model) onProbe(msg probeDoneMsg) tea.Cmd {
	m.doneBusy()
	t := m.tabByID(msg.mgr)
	if t == nil {
		return nil
	}
	if msg.err != nil {
		t.probe = probeFailed
		t.probeErr = msg.err.Error()
		m.appendLog(fmt.Sprintf("! %s is unavailable: %v", msg.mgr, msg.err))
		m.retargetActive()
		if m.busy == 0 {
			m.setStatusIfIdle()
		}
		return nil
	}
	t.probe = probeOK
	m.retargetActive()
	// Only narrate the manager the user is actually looking at; the other one
	// loads in the background.
	if t == m.cur() {
		m.setStatus(statusBusy, "Reading installed %s packages…", t.mgr.ID())
	}
	return m.startList(t)
}

// retargetActive moves the active tab off a manager that turned out to be
// unusable. A manager whose probe has not answered yet is left alone: the user
// should land on the first tab, not on whichever manager happened to finish
// probing first.
func (m *Model) retargetActive() {
	t := m.cur()
	if t == nil || t.probe != probeFailed {
		return
	}
	for i, cand := range m.tabs {
		if cand.available() {
			m.active = i
			return
		}
	}
	// Nothing has succeeded yet; prefer a tab that is still being probed over
	// one that has already failed.
	for i, cand := range m.tabs {
		if cand.probe == probePending {
			m.active = i
			return
		}
	}
}

func (m *Model) onList(msg listMsg) tea.Cmd {
	m.doneBusy()
	t := m.tabByID(msg.mgr)
	if t == nil {
		return nil
	}
	t.listing = false
	if msg.err != nil {
		t.loaded = true
		t.loadErr = msg.err.Error()
		m.keepStatus = false
		m.setStatus(statusBad, "Could not list %s packages: %v", msg.mgr, msg.err)
		return nil
	}
	t.setPackages(msg.pkgs, sourceInstalled, "")
	if m.pendingSelect != "" {
		t.selectByName(m.pendingSelect)
		m.pendingSelect = ""
	}
	// A rescan triggered by an install or uninstall has already reported what
	// happened; don't overwrite that with a bare package count.
	if t == m.cur() && !m.keepStatus {
		m.setStatus(statusGood, "%d %s package(s) installed", len(msg.pkgs), msg.mgr)
	}
	m.keepStatus = false
	m.layout()
	return m.ensureDetails()
}

func (m *Model) onSearch(msg searchMsg) tea.Cmd {
	m.doneBusy()
	t := m.tabByID(msg.mgr)
	if t == nil {
		return nil
	}
	t.listing = false
	if msg.err != nil {
		m.keepStatus = false
		m.setStatus(statusWarning, "Search for %q found nothing: %v", msg.query, msg.err)
		return nil
	}
	// Re-running the same query after an operation is a refresh, not a new
	// search: keep the cursor and the filter where the user left them.
	rescan := t.source == sourceSearch && t.query == msg.query
	if !rescan {
		t.filter = ""
		t.installedOnly = false
		t.cursor, t.top = 0, 0
	}
	t.setPackages(msg.pkgs, sourceSearch, msg.query)
	if !rescan {
		t.cursor, t.top = 0, 0
	}
	if m.pendingSelect != "" {
		t.selectByName(m.pendingSelect)
		m.pendingSelect = ""
	}
	if !m.keepStatus {
		installed := t.countInstalled()
		m.setStatus(statusGood, "%d result(s) for %q in %s (%d already installed) — esc returns to the installed list",
			len(msg.pkgs), msg.query, msg.mgr, installed)
	}
	m.keepStatus = false
	m.layout()
	return m.ensureDetails()
}

func (m *Model) onDetails(msg detailsMsg) tea.Cmd {
	t := m.tabByID(msg.mgr)
	if t == nil {
		return nil
	}
	delete(t.pending, msg.name)
	if msg.err != nil {
		t.detailErrs[msg.name] = msg.err.Error()
	} else {
		t.details[msg.name] = msg.details
		delete(t.detailErrs, msg.name)
	}
	if t == m.cur() {
		if cur, ok := t.current(); ok && cur.Name == msg.name {
			t.shownName = "" // force the pane to re-render
			m.refreshDetailViewport()
		}
	}
	return nil
}

func (m *Model) onOutdated(msg outdatedMsg) tea.Cmd {
	m.doneBusy()
	t := m.tabByID(msg.mgr)
	if t == nil {
		return nil
	}
	t.checked = true
	if msg.err != nil {
		t.outdatedErr = msg.err.Error()
		m.setStatus(statusWarning, "Could not check for %s updates: %v", msg.mgr, msg.err)
		return nil
	}
	t.outdated = msg.outdated
	t.outdatedErr = ""
	for i := range t.all {
		t.all[i].Latest = ""
	}
	n := 0
	for i := range t.all {
		if v, ok := lookupOutdated(msg.outdated, t.all[i].Name); ok && v != t.all[i].Version {
			t.all[i].Latest = v
			n++
		}
	}
	if n == 0 {
		m.setStatus(statusGood, "Every installed %s package is already at the newest version", msg.mgr)
	} else {
		m.setStatus(statusWarning, "%d %s package(s) have a newer version — press U to upgrade the selected one", n, msg.mgr)
	}
	return nil
}

// lookupOutdated tolerates the naming differences between managers.
func lookupOutdated(out map[string]string, name string) (string, bool) {
	if v, ok := out[name]; ok {
		return v, true
	}
	target := loosen(name)
	for k, v := range out {
		if loosen(k) == target {
			return v, true
		}
	}
	return "", false
}

func (m *Model) onOpDone(msg opDoneMsg) tea.Cmd {
	m.doneBusy()
	m.mutating = false
	t := m.tabByID(msg.mgr)
	if t == nil {
		return nil
	}
	targets := strings.Join(msg.targets, ", ")
	if msg.err != nil {
		m.setStatus(statusBad, "Failed to %s %s: %v", msg.op.verb(), targets, msg.err)
		m.showLog = true
		m.layout()
		m.logVP.GotoBottom()
		// Still rescan: a failed plan can leave the environment partly changed.
		return m.refreshTab(t)
	}

	t.invalidate()
	switch msg.op {
	case opRemove:
		if msg.snapshot != nil {
			// The unused-dependency pass uninstalls more packages, so it must
			// finish before the list is reread — a rescan running alongside it
			// would show packages that are on their way out. onOrphans does the
			// refresh once the pass is done.
			m.busy++
			m.mutating = true
			m.busyLabel = "removing unused dependencies"
			m.setStatus(statusBusy,
				"Uninstalled %s — checking for dependencies that are no longer needed…", targets)
			for _, name := range msg.targets {
				delete(t.marks, name)
			}
			return orphansCmd(t.mgr, m.Logger(), msg.snapshot, msg.targets)
		}
		m.setStatus(statusGood, "Uninstalled %s in %s", targets, msg.elapsed)
	case opExtra:
		m.setStatus(statusGood, "%s completed in %s", msg.plan.Title, msg.elapsed)
	default:
		m.setStatus(statusGood, "%s %s in %s — the list below has been rescanned",
			capitalize(msg.op.past()), targets, msg.elapsed)
	}
	for _, name := range msg.targets {
		delete(t.marks, name)
	}
	// Land the cursor on what was just installed or upgraded. After a removal
	// there is nothing to land on.
	if msg.op != opRemove {
		m.pendingSelect = firstName(msg.targets)
	}
	return m.refreshTab(t)
}

func (m *Model) onOrphans(msg orphansMsg) tea.Cmd {
	m.doneBusy()
	m.mutating = false
	t := m.tabByID(msg.mgr)
	if t == nil {
		return nil
	}
	switch {
	case msg.err != nil:
		m.setStatus(statusWarning, "Uninstalled %s, but the unused-dependency pass failed: %v",
			strings.Join(msg.removed, ", "), msg.err)
	case len(msg.orphans) == 0:
		m.setStatus(statusGood, "Uninstalled %s — every remaining dependency is still needed by something else",
			strings.Join(msg.removed, ", "))
	default:
		m.setStatus(statusGood, "Uninstalled %s and %d dependency package(s) that nothing needs any more: %s",
			strings.Join(msg.removed, ", "), len(msg.orphans), strings.Join(msg.orphans, ", "))
	}
	t.invalidate()
	return m.refreshTab(t)
}

func (m *Model) onPredict(msg predictMsg) {
	if m.mode != modeConfirm || m.confirm == nil || !m.confirm.awaiting {
		return
	}
	if m.confirm.mgr == nil || m.confirm.mgr.ID() != msg.mgr {
		return
	}
	m.confirm.awaiting = false
	switch {
	case msg.err != nil:
		m.confirm.lines = append(m.confirm.lines,
			"Could not work out which dependencies become unused: "+msg.err.Error(),
			"They will still be checked for after the uninstall.")
	case len(msg.orphans) == 0:
		m.confirm.lines = append(m.confirm.lines,
			"No other package becomes unused: every dependency is still needed elsewhere.")
	default:
		m.confirm.lines = append(m.confirm.lines,
			fmt.Sprintf("These %d dependency package(s) become unused and will be removed too:", len(msg.orphans)),
			"  "+strings.Join(msg.orphans, ", "))
	}
}

// refreshTab rescans a manager's package list from the live environment. It is
// used after an operation, so the status line describing that operation is kept.
func (m *Model) refreshTab(t *tabState) tea.Cmd {
	if t == nil || !t.available() {
		return nil
	}
	m.keepStatus = true
	if t.source == sourceSearch && t.query != "" {
		return m.startSearch(t, t.query)
	}
	return m.startList(t)
}

// startList dispatches a package listing, recording that one is in flight so a
// second one is not queued behind it.
func (m *Model) startList(t *tabState) tea.Cmd {
	if t == nil || !t.available() {
		return nil
	}
	t.listing = true
	m.busy++
	m.busyLabel = "listing " + t.mgr.ID()
	return listCmd(t.mgr)
}

// startSearch dispatches an index search for the given query.
func (m *Model) startSearch(t *tabState, query string) tea.Cmd {
	if t == nil || !t.available() {
		return nil
	}
	t.listing = true
	m.busy++
	m.busyLabel = "searching " + t.mgr.ID()
	return searchCmd(t.mgr, query)
}

// ensureDetails asks for the selected package's details if they are not cached.
func (m *Model) ensureDetails() tea.Cmd {
	t := m.cur()
	if t == nil {
		return nil
	}
	cur, ok := t.current()
	if !ok {
		m.refreshDetailViewport()
		return nil
	}
	m.refreshDetailViewport()
	if _, cached := t.details[cur.Name]; cached {
		return nil
	}
	if _, failed := t.detailErrs[cur.Name]; failed {
		return nil
	}
	if t.pending[cur.Name] {
		return nil
	}
	t.pending[cur.Name] = true
	return detailsCmd(t.mgr, cur)
}

func (m *Model) doneBusy() {
	if m.busy > 0 {
		m.busy--
	}
	if m.busy == 0 {
		m.busyLabel = ""
	}
}

func (m *Model) setStatus(level statusLevel, format string, args ...any) {
	m.status = fmt.Sprintf(format, args...)
	m.statusLevel = level
}

func (m *Model) setStatusIfIdle() {
	t := m.cur()
	if t == nil {
		m.setStatus(statusBad, "No usable package manager was found on this system")
		return
	}
	if !t.available() {
		m.setStatus(statusBad, "%s is unavailable: %s", t.mgr.ID(), t.probeErr)
	}
}

func (m *Model) appendLog(line string) {
	m.logMu.Lock()
	m.logs = append(m.logs, line)
	if len(m.logs) > maxLogLines {
		m.logs = m.logs[len(m.logs)-maxLogLines:]
	}
	m.logMu.Unlock()

	if m.showLog {
		m.renderLog()
		m.logVP.GotoBottom()
	}
}

func (m *Model) logSnapshot() []string {
	m.logMu.Lock()
	defer m.logMu.Unlock()
	out := make([]string, len(m.logs))
	copy(out, m.logs)
	return out
}

func capitalize(s string) string {
	if s == "" {
		return s
	}
	return strings.ToUpper(s[:1]) + s[1:]
}

func firstName(names []string) string {
	for _, n := range names {
		if base := strings.TrimSpace(n); base != "" {
			// Drop any version pin so the cursor lands on the package itself.
			if i := strings.IndexAny(base, "[<>=!~ "); i > 0 {
				base = base[:i]
			}
			return base
		}
	}
	return ""
}
