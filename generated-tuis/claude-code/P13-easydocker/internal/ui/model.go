package ui

import (
	"context"
	"fmt"
	"sort"
	"strings"
	"time"

	"toolm/internal/docker"

	tea "github.com/charmbracelet/bubbletea"
)

// view identifies one of the four resource browsers.
type view int

const (
	viewContainers view = iota
	viewImages
	viewNetworks
	viewVolumes
	viewCount
)

// title returns the tab label of the view.
func (v view) title() string {
	switch v {
	case viewContainers:
		return "Containers"
	case viewImages:
		return "Images"
	case viewNetworks:
		return "Networks"
	case viewVolumes:
		return "Volumes"
	}
	return ""
}

// noun returns the singular resource name of the view.
func (v view) noun() string {
	switch v {
	case viewContainers:
		return "container"
	case viewImages:
		return "image"
	case viewNetworks:
		return "network"
	case viewVolumes:
		return "volume"
	}
	return "item"
}

// mode is the current interaction mode of the application.
type mode int

const (
	modeList mode = iota
	modeDetail
	modeLogs
	modeHelp
)

// level classifies a status message.
type level int

const (
	levelInfo level = iota
	levelOK
	levelWarn
	levelError
)

// listState holds the per-view cursor, scroll offset, filter and sort order.
type listState struct {
	cursor    int
	offset    int
	filter    string
	filtering bool
	sortIdx   int
	sortDesc  bool
}

// sortOption names one sortable column of a view.
type sortOption struct {
	name string
}

// sortOptions lists the sort columns available per view.
var sortOptions = map[view][]sortOption{
	viewContainers: {{"name"}, {"image"}, {"state"}, {"created"}},
	viewImages:     {{"repository"}, {"tag"}, {"size"}, {"created"}},
	viewNetworks:   {{"name"}, {"driver"}, {"scope"}},
	viewVolumes:    {{"name"}, {"driver"}, {"mountpoint"}},
}

// dataSet is the most recent snapshot read from the Docker endpoint.
type dataSet struct {
	containers []docker.Container
	images     []docker.Image
	networks   []docker.Network
	volumes    []docker.Volume

	errs map[view]error
}

// detailState holds the currently inspected object.
type detailState struct {
	view   view
	key    string
	title  string
	rows   []fieldRow
	offset int
	err    error
	reqID  int
}

// logState holds the log viewer contents.
type logState struct {
	containerID   string
	containerName string
	lines         []string
	offset        int
	xOffset       int
	wrap          bool
	err           error
	reqID         int
	empty         bool
}

// Model is the root Bubble Tea model of toolm.
type Model struct {
	client docker.Client
	styles Styles

	width, height int
	ready         bool

	mode mode
	view view

	lists [viewCount]listState
	data  dataSet

	detail detailState
	logs   logState

	helpOffset int

	endpoint    string
	version     string
	loading     int
	spinnerStep int
	lastRefresh time.Time

	statusText  string
	statusLevel level

	reqSeq int

	quitting bool
}

// New builds the initial model for the given Docker client.
func New(client docker.Client) Model {
	m := Model{
		client:   client,
		styles:   NewStyles(),
		endpoint: client.Endpoint(),
		data:     dataSet{errs: map[view]error{}},
	}
	m.statusText = "Loading Docker resources…"
	return m
}

// Init implements tea.Model.
func (m Model) Init() tea.Cmd {
	return tea.Batch(m.refreshCmd(), m.versionCmd(), spinnerTick())
}

// --- messages ---

type dataMsg struct {
	data dataSet
}

type versionMsg struct {
	version string
}

type detailMsg struct {
	reqID int
	view  view
	key   string
	title string
	rows  []fieldRow
	err   error
}

type logsMsg struct {
	reqID int
	id    string
	name  string
	raw   string
	err   error
}

type spinnerMsg struct{}

// spinnerTick schedules the next spinner frame.
func spinnerTick() tea.Cmd {
	return tea.Tick(120*time.Millisecond, func(time.Time) tea.Msg { return spinnerMsg{} })
}

// requestTimeout bounds every Docker API call so a stalled socket cannot hang
// the interface.
const requestTimeout = 20 * time.Second

// refreshCmd reloads every resource list.
func (m Model) refreshCmd() tea.Cmd {
	client := m.client
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
		defer cancel()

		ds := dataSet{errs: map[view]error{}}
		var err error
		if ds.containers, err = client.Containers(ctx); err != nil {
			ds.errs[viewContainers] = err
		}
		if ds.images, err = client.Images(ctx); err != nil {
			ds.errs[viewImages] = err
		}
		if ds.networks, err = client.Networks(ctx); err != nil {
			ds.errs[viewNetworks] = err
		}
		if ds.volumes, err = client.Volumes(ctx); err != nil {
			ds.errs[viewVolumes] = err
		}
		return dataMsg{data: ds}
	}
}

// versionCmd fetches the engine version for the header.
func (m Model) versionCmd() tea.Cmd {
	client := m.client
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		v, err := client.Version(ctx)
		if err != nil {
			return versionMsg{version: ""}
		}
		return versionMsg{version: v}
	}
}

// Update implements tea.Model.
func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		m.ready = true
		m.clampAll()
		return m, nil

	case spinnerMsg:
		m.spinnerStep++
		if m.loading > 0 {
			return m, spinnerTick()
		}
		return m, nil

	case versionMsg:
		m.version = msg.version
		return m, nil

	case dataMsg:
		m.loading--
		if m.loading < 0 {
			m.loading = 0
		}
		m.data = msg.data
		m.lastRefresh = time.Now()
		m.clampAll()
		m.setRefreshStatus()
		return m, nil

	case detailMsg:
		m.loading--
		if m.loading < 0 {
			m.loading = 0
		}
		if msg.reqID != m.detail.reqID {
			return m, nil // a newer request superseded this one
		}
		m.detail.view = msg.view
		m.detail.key = msg.key
		m.detail.title = msg.title
		m.detail.rows = msg.rows
		m.detail.err = msg.err
		m.detail.offset = 0
		m.mode = modeDetail
		if msg.err != nil {
			m.setStatus(levelError, fmt.Sprintf("inspect %s: %v", msg.key, msg.err))
		} else {
			m.setStatus(levelOK, fmt.Sprintf("Details for %s %s", msg.view.noun(), msg.title))
		}
		return m, nil

	case logsMsg:
		m.loading--
		if m.loading < 0 {
			m.loading = 0
		}
		if msg.reqID != m.logs.reqID {
			return m, nil
		}
		m.logs.containerID = msg.id
		m.logs.containerName = msg.name
		m.logs.err = msg.err
		m.logs.lines = docker.SplitLogLines(msg.raw)
		m.logs.empty = len(m.logs.lines) == 0
		m.logs.offset = 0
		m.logs.xOffset = 0
		m.mode = modeLogs
		switch {
		case msg.err != nil:
			m.setStatus(levelError, fmt.Sprintf("logs %s: %v", msg.name, msg.err))
		case m.logs.empty:
			m.setStatus(levelWarn, fmt.Sprintf("Container %s has no log output", msg.name))
		default:
			m.setStatus(levelOK, fmt.Sprintf("%d log lines for %s", len(m.logs.lines), msg.name))
			// Logs are most useful from the tail, matching `docker logs`.
			m.logs.offset = m.maxLogOffset()
		}
		return m, nil

	case tea.KeyMsg:
		return m.handleKey(msg)
	}
	return m, nil
}

// setRefreshStatus summarises the outcome of a reload.
func (m *Model) setRefreshStatus() {
	if len(m.data.errs) > 0 {
		var parts []string
		for v := view(0); v < viewCount; v++ {
			if err, ok := m.data.errs[v]; ok {
				parts = append(parts, fmt.Sprintf("%s (%v)", strings.ToLower(v.title()), err))
			}
		}
		m.setStatus(levelError, "Failed to load "+strings.Join(parts, "; "))
		return
	}
	m.setStatus(levelOK, fmt.Sprintf("Loaded %d containers, %d images, %d networks, %d volumes",
		len(m.data.containers), len(m.data.images), len(m.data.networks), len(m.data.volumes)))
}

// setStatus records the message shown in the status bar.
func (m *Model) setStatus(l level, text string) {
	m.statusLevel = l
	m.statusText = text
}

// handleKey routes a key press to the active mode.
func (m Model) handleKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	key := msg.String()

	// Filter entry captures most keys, so it is handled first.
	if m.mode == modeList && m.list().filtering {
		return m.handleFilterKey(msg)
	}

	switch key {
	case "ctrl+c":
		m.quitting = true
		return m, tea.Quit
	case "q":
		if m.mode == modeList {
			m.quitting = true
			return m, tea.Quit
		}
		// Elsewhere q backs out one level, which is less surprising.
		return m.back(), nil
	case "?":
		if m.mode == modeHelp {
			return m.back(), nil
		}
		m.mode = modeHelp
		m.helpOffset = 0
		m.setStatus(levelInfo, "Key reference — press ? or esc to return")
		return m, nil
	case "r", "ctrl+r", "f5":
		m.loading++
		m.setStatus(levelInfo, "Reloading from "+m.endpoint+"…")
		cmds := []tea.Cmd{m.refreshCmd(), spinnerTick()}
		// Refresh the open detail or log pane too, so a reload is consistent.
		switch m.mode {
		case modeDetail:
			if c := m.reinspectCmd(); c != nil {
				m.loading++
				cmds = append(cmds, c)
			}
		case modeLogs:
			if c := m.relogCmd(); c != nil {
				m.loading++
				cmds = append(cmds, c)
			}
		}
		return m, tea.Batch(cmds...)
	}

	switch m.mode {
	case modeHelp:
		return m.handleHelpKey(key)
	case modeLogs:
		return m.handleLogsKey(key)
	case modeDetail:
		return m.handleDetailKey(key)
	default:
		return m.handleListKey(key)
	}
}

// back leaves the current mode for the list view.
func (m Model) back() Model {
	m.mode = modeList
	m.setStatus(levelInfo, fmt.Sprintf("%s — %d %ss", m.view.title(), m.filteredCount(), m.view.noun()))
	return m
}

// handleFilterKey handles typing in the filter prompt.
func (m Model) handleFilterKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	ls := m.list()
	switch msg.String() {
	case "esc":
		ls.filtering = false
		ls.filter = ""
		ls.cursor, ls.offset = 0, 0
		m.setStatus(levelInfo, "Filter cleared")
		return m, nil
	case "enter":
		ls.filtering = false
		if ls.filter == "" {
			m.setStatus(levelInfo, "Filter cleared")
		} else {
			m.setStatus(levelOK, fmt.Sprintf("Filter %q — %d of %d %ss",
				ls.filter, m.filteredCount(), m.totalCount(), m.view.noun()))
		}
		return m, nil
	case "backspace":
		if r := []rune(ls.filter); len(r) > 0 {
			ls.filter = string(r[:len(r)-1])
		}
	case "ctrl+u":
		ls.filter = ""
	case "up", "down", "ctrl+p", "ctrl+n":
		// Allow moving the cursor without leaving the prompt.
		delta := 1
		if msg.String() == "up" || msg.String() == "ctrl+p" {
			delta = -1
		}
		m.moveCursor(delta)
		return m, nil
	default:
		if msg.Type == tea.KeyRunes {
			ls.filter += string(msg.Runes)
		} else if msg.Type == tea.KeySpace {
			ls.filter += " "
		}
	}
	ls.cursor, ls.offset = 0, 0
	m.clampList()
	return m, nil
}

// handleListKey handles navigation in a resource list.
func (m Model) handleListKey(key string) (tea.Model, tea.Cmd) {
	switch key {
	case "up", "k":
		m.moveCursor(-1)
	case "down", "j":
		m.moveCursor(1)
	case "pgup", "ctrl+b":
		m.moveCursor(-m.listPageSize())
	case "pgdown", "ctrl+f", " ":
		m.moveCursor(m.listPageSize())
	case "home", "g":
		m.setCursor(0)
	case "end", "G":
		m.setCursor(m.filteredCount() - 1)
	case "tab":
		m.switchView(m.view + 1)
	case "shift+tab":
		m.switchView(m.view - 1)
	case "1", "c":
		m.switchView(viewContainers)
	case "2", "i":
		m.switchView(viewImages)
	case "3", "n":
		m.switchView(viewNetworks)
	case "4", "v":
		m.switchView(viewVolumes)
	case "l":
		return m.openLogs()
	case "/":
		m.list().filtering = true
		m.setStatus(levelInfo, "Filter: type to narrow, enter to apply, esc to clear")
	case "esc":
		ls := m.list()
		if ls.filter != "" {
			ls.filter = ""
			ls.cursor, ls.offset = 0, 0
			m.setStatus(levelInfo, "Filter cleared")
		}
	case "s":
		opts := sortOptions[m.view]
		ls := m.list()
		ls.sortIdx = (ls.sortIdx + 1) % len(opts)
		m.clampList()
		m.setStatus(levelInfo, fmt.Sprintf("Sorted by %s (%s)", opts[ls.sortIdx].name, m.sortDirLabel()))
	case "S":
		ls := m.list()
		ls.sortDesc = !ls.sortDesc
		m.clampList()
		m.setStatus(levelInfo, fmt.Sprintf("Sorted by %s (%s)",
			sortOptions[m.view][ls.sortIdx].name, m.sortDirLabel()))
	case "enter":
		return m.openDetail()
	}
	return m, nil
}

// sortDirLabel describes the current sort direction.
func (m Model) sortDirLabel() string {
	if m.listConst().sortDesc {
		return "descending"
	}
	return "ascending"
}

// handleDetailKey scrolls the detail pane.
func (m Model) handleDetailKey(key string) (tea.Model, tea.Cmd) {
	page := m.detailPageSize()
	switch key {
	case "up", "k":
		m.detail.offset--
	case "down", "j":
		m.detail.offset++
	case "pgup", "ctrl+b":
		m.detail.offset -= page
	case "pgdown", "ctrl+f", " ":
		m.detail.offset += page
	case "home", "g":
		m.detail.offset = 0
	case "end", "G":
		m.detail.offset = 1 << 30
	case "esc", "enter", "backspace":
		return m.back(), nil
	case "tab":
		mm := m.back()
		mm.switchView(mm.view + 1)
		return mm, nil
	case "shift+tab":
		mm := m.back()
		mm.switchView(mm.view - 1)
		return mm, nil
	case "l":
		if m.detail.view == viewContainers {
			return m.openLogsFor(m.detail.key, m.detail.title)
		}
	}
	m.clampDetail()
	return m, nil
}

// handleLogsKey scrolls the log viewer.
func (m Model) handleLogsKey(key string) (tea.Model, tea.Cmd) {
	page := m.logPageSize()
	switch key {
	case "up", "k":
		m.logs.offset--
	case "down", "j":
		m.logs.offset++
	case "pgup", "ctrl+b":
		m.logs.offset -= page
	case "pgdown", "ctrl+f", " ":
		m.logs.offset += page
	case "home", "g":
		m.logs.offset = 0
	case "end", "G":
		m.logs.offset = m.maxLogOffset()
	case "left", "h":
		m.logs.xOffset -= 8
	case "right":
		m.logs.xOffset += 8
	case "w":
		m.logs.wrap = !m.logs.wrap
		m.logs.xOffset = 0
		if m.logs.wrap {
			m.setStatus(levelInfo, "Line wrapping on")
		} else {
			m.setStatus(levelInfo, "Line wrapping off — use ←/→ to scroll sideways")
		}
	case "esc", "enter", "backspace":
		return m.back(), nil
	}
	m.clampLogs()
	return m, nil
}

// handleHelpKey scrolls the help screen.
func (m Model) handleHelpKey(key string) (tea.Model, tea.Cmd) {
	switch key {
	case "up", "k":
		m.helpOffset--
	case "down", "j":
		m.helpOffset++
	case "pgup", "ctrl+b":
		m.helpOffset -= m.bodyHeight()
	case "pgdown", "ctrl+f", " ":
		m.helpOffset += m.bodyHeight()
	case "home", "g":
		m.helpOffset = 0
	case "end", "G":
		m.helpOffset = 1 << 30
	case "esc", "enter", "backspace":
		return m.back(), nil
	}
	m.helpOffset = clamp(m.helpOffset, 0, maxOffset(len(m.helpLines()), m.bodyHeight()))
	return m, nil
}

// switchView moves to another resource tab, wrapping around.
func (m *Model) switchView(v view) {
	if v < 0 {
		v = viewCount - 1
	}
	if v >= viewCount {
		v = 0
	}
	m.view = v
	m.mode = modeList
	m.clampList()
	count := m.filteredCount()
	msg := fmt.Sprintf("%s — %d %ss", v.title(), count, v.noun())
	if f := m.listConst().filter; f != "" {
		msg += fmt.Sprintf(" matching %q", f)
	}
	if err, ok := m.data.errs[v]; ok {
		m.setStatus(levelError, fmt.Sprintf("%s: %v", v.title(), err))
		return
	}
	m.setStatus(levelInfo, msg)
}

// list returns a pointer to the active list state.
func (m *Model) list() *listState { return &m.lists[m.view] }

// listConst returns the active list state by value, for read-only use on a
// non-pointer receiver.
func (m Model) listConst() listState { return m.lists[m.view] }

// moveCursor moves the selection by delta rows.
func (m *Model) moveCursor(delta int) { m.setCursor(m.lists[m.view].cursor + delta) }

// setCursor selects row i, keeping it inside the visible window.
func (m *Model) setCursor(i int) {
	n := m.filteredCount()
	ls := &m.lists[m.view]
	if n == 0 {
		ls.cursor, ls.offset = 0, 0
		return
	}
	ls.cursor = clamp(i, 0, n-1)
	page := m.listPageSize()
	if page > 0 {
		if ls.cursor < ls.offset {
			ls.offset = ls.cursor
		}
		if ls.cursor >= ls.offset+page {
			ls.offset = ls.cursor - page + 1
		}
		ls.offset = clamp(ls.offset, 0, maxOffset(n, page))
	}
}

// clampAll re-validates every scroll position after a resize or reload.
func (m *Model) clampAll() {
	saved := m.view
	for v := view(0); v < viewCount; v++ {
		m.view = v
		m.clampList()
	}
	m.view = saved
	m.clampDetail()
	m.clampLogs()
	m.helpOffset = clamp(m.helpOffset, 0, maxOffset(len(m.helpLines()), m.bodyHeight()))
}

// clampList keeps the cursor and offset of the active list in range.
func (m *Model) clampList() {
	n := m.filteredCount()
	ls := &m.lists[m.view]
	if n == 0 {
		ls.cursor, ls.offset = 0, 0
		return
	}
	ls.cursor = clamp(ls.cursor, 0, n-1)
	page := m.listPageSize()
	ls.offset = clamp(ls.offset, 0, maxOffset(n, page))
	if ls.cursor < ls.offset {
		ls.offset = ls.cursor
	}
	if page > 0 && ls.cursor >= ls.offset+page {
		ls.offset = ls.cursor - page + 1
	}
}

// clampDetail keeps the detail scroll offset in range.
func (m *Model) clampDetail() {
	total := len(m.detailLines())
	m.detail.offset = clamp(m.detail.offset, 0, maxOffset(total, m.detailPageSize()))
}

// clampLogs keeps the log scroll offsets in range.
func (m *Model) clampLogs() {
	m.logs.offset = clamp(m.logs.offset, 0, m.maxLogOffset())
	if m.logs.xOffset < 0 {
		m.logs.xOffset = 0
	}
	if m.logs.wrap {
		m.logs.xOffset = 0
	}
}

// maxLogOffset is the largest first-visible-line index for the log viewer.
func (m Model) maxLogOffset() int {
	return maxOffset(len(m.renderedLogLines()), m.logPageSize())
}

// maxOffset returns the largest scroll offset for total items in a window of
// the given size.
func maxOffset(total, window int) int {
	if window <= 0 || total <= window {
		return 0
	}
	return total - window
}

// --- filtering and sorting ---

// matches reports whether haystack contains the filter, case-insensitively.
// An empty filter matches everything, and space separated terms must all match
// so users can narrow progressively (e.g. "nginx running").
func matches(filter string, fields ...string) bool {
	filter = strings.TrimSpace(strings.ToLower(filter))
	if filter == "" {
		return true
	}
	hay := strings.ToLower(strings.Join(fields, "\x00"))
	for _, term := range strings.Fields(filter) {
		if !strings.Contains(hay, term) {
			return false
		}
	}
	return true
}

// filteredContainers returns the containers matching the active filter, sorted.
func (m Model) filteredContainers() []docker.Container {
	ls := m.lists[viewContainers]
	out := make([]docker.Container, 0, len(m.data.containers))
	for _, c := range m.data.containers {
		if matches(ls.filter, c.Name(), c.Image, c.StateLabel(), c.Status, docker.ShortID(c.ID), c.PortsString(), c.Command) {
			out = append(out, c)
		}
	}
	less := func(i, j int) bool {
		a, b := out[i], out[j]
		switch ls.sortIdx {
		case 1:
			if !strings.EqualFold(a.Image, b.Image) {
				return strings.ToLower(a.Image) < strings.ToLower(b.Image)
			}
		case 2:
			if a.StateLabel() != b.StateLabel() {
				// Running containers first when ascending.
				if a.IsRunning() != b.IsRunning() {
					return a.IsRunning()
				}
				return a.StateLabel() < b.StateLabel()
			}
		case 3:
			if a.Created != b.Created {
				return a.Created > b.Created // newest first when ascending
			}
		}
		return strings.ToLower(a.Name()) < strings.ToLower(b.Name())
	}
	sortSlice(len(out), less, ls.sortDesc, func(i, j int) { out[i], out[j] = out[j], out[i] })
	return out
}

// filteredImages returns the images matching the active filter, sorted.
func (m Model) filteredImages() []docker.Image {
	ls := m.lists[viewImages]
	out := make([]docker.Image, 0, len(m.data.images))
	for _, img := range m.data.images {
		repo, tag := imageRepoTag(img)
		if matches(ls.filter, repo, tag, repo+":"+tag, docker.ShortID(img.ID), docker.FormatSizeMB(img.SizeBytes())) {
			out = append(out, img)
		}
	}
	less := func(i, j int) bool {
		a, b := out[i], out[j]
		ra, ta := imageRepoTag(a)
		rb, tb := imageRepoTag(b)
		switch ls.sortIdx {
		case 1:
			if ta != tb {
				return ta < tb
			}
		case 2:
			if a.SizeBytes() != b.SizeBytes() {
				return a.SizeBytes() > b.SizeBytes() // largest first when ascending
			}
		case 3:
			if a.Created != b.Created {
				return a.Created > b.Created
			}
		}
		if !strings.EqualFold(ra, rb) {
			return strings.ToLower(ra) < strings.ToLower(rb)
		}
		return ta < tb
	}
	sortSlice(len(out), less, ls.sortDesc, func(i, j int) { out[i], out[j] = out[j], out[i] })
	return out
}

// filteredNetworks returns the networks matching the active filter, sorted.
func (m Model) filteredNetworks() []docker.Network {
	ls := m.lists[viewNetworks]
	out := make([]docker.Network, 0, len(m.data.networks))
	for _, n := range m.data.networks {
		if matches(ls.filter, n.Name, n.DriverName(), n.Scope, docker.ShortID(n.ID)) {
			out = append(out, n)
		}
	}
	less := func(i, j int) bool {
		a, b := out[i], out[j]
		switch ls.sortIdx {
		case 1:
			if a.DriverName() != b.DriverName() {
				return a.DriverName() < b.DriverName()
			}
		case 2:
			if a.Scope != b.Scope {
				return a.Scope < b.Scope
			}
		}
		return strings.ToLower(a.Name) < strings.ToLower(b.Name)
	}
	sortSlice(len(out), less, ls.sortDesc, func(i, j int) { out[i], out[j] = out[j], out[i] })
	return out
}

// filteredVolumes returns the volumes matching the active filter, sorted.
func (m Model) filteredVolumes() []docker.Volume {
	ls := m.lists[viewVolumes]
	out := make([]docker.Volume, 0, len(m.data.volumes))
	for _, v := range m.data.volumes {
		if matches(ls.filter, v.Name, v.DriverName(), v.Mountpoint, v.Scope) {
			out = append(out, v)
		}
	}
	less := func(i, j int) bool {
		a, b := out[i], out[j]
		switch ls.sortIdx {
		case 1:
			if a.DriverName() != b.DriverName() {
				return a.DriverName() < b.DriverName()
			}
		case 2:
			if a.Mountpoint != b.Mountpoint {
				return a.Mountpoint < b.Mountpoint
			}
		}
		return strings.ToLower(a.Name) < strings.ToLower(b.Name)
	}
	sortSlice(len(out), less, ls.sortDesc, func(i, j int) { out[i], out[j] = out[j], out[i] })
	return out
}

// sortSlice sorts n elements with the given comparison, reversing the result
// when desc is set. A stable sort keeps equal rows in API order.
func sortSlice(n int, less func(i, j int) bool, desc bool, swap func(i, j int)) {
	sort.Stable(sliceAdapter{n: n, less: less, swap: swap})
	if desc {
		for i, j := 0, n-1; i < j; i, j = i+1, j-1 {
			swap(i, j)
		}
	}
}

// sliceAdapter lets sort.SliceStable operate through closures.
type sliceAdapter struct {
	n    int
	less func(i, j int) bool
	swap func(i, j int)
}

func (s sliceAdapter) Len() int           { return s.n }
func (s sliceAdapter) Less(i, j int) bool { return s.less(i, j) }
func (s sliceAdapter) Swap(i, j int)      { s.swap(i, j) }

// imageRepoTag resolves the repository and tag shown for an image. Untagged
// images report "<none>" like the Docker CLI does.
func imageRepoTag(img docker.Image) (string, string) {
	for _, rt := range img.RepoTags {
		if rt != "" && rt != "<none>:<none>" {
			return docker.SplitRepoTag(rt)
		}
	}
	for _, rd := range img.RepoDigests {
		if i := strings.Index(rd, "@"); i > 0 {
			return rd[:i], "<none>"
		}
	}
	return "<none>", "<none>"
}

// filteredCount is the number of rows currently listed in the active view.
func (m Model) filteredCount() int {
	switch m.view {
	case viewContainers:
		return len(m.filteredContainers())
	case viewImages:
		return len(m.filteredImages())
	case viewNetworks:
		return len(m.filteredNetworks())
	case viewVolumes:
		return len(m.filteredVolumes())
	}
	return 0
}

// totalCount is the number of rows before filtering.
func (m Model) totalCount() int {
	switch m.view {
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

// --- opening details and logs ---

// openDetail inspects the selected row and switches to the detail pane.
func (m Model) openDetail() (tea.Model, tea.Cmd) {
	if m.filteredCount() == 0 {
		m.setStatus(levelWarn, "Nothing to inspect in "+m.view.title())
		return m, nil
	}
	cur := m.lists[m.view].cursor
	m.reqSeq++
	id := m.reqSeq
	m.detail.reqID = id
	m.loading++

	client := m.client
	v := m.view

	var key, title string
	switch v {
	case viewContainers:
		c := m.filteredContainers()[cur]
		key, title = c.ID, c.Name()
		fallback := c
		m.setStatus(levelInfo, "Inspecting container "+title+"…")
		return m, tea.Batch(spinnerTick(), func() tea.Msg {
			ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
			defer cancel()
			d, err := client.ContainerInspect(ctx, key)
			if err != nil {
				// Fall back to the list entry so the pane still shows the
				// fields the summary already provided.
				return detailMsg{reqID: id, view: v, key: key, title: title,
					rows: containerRowsFromSummary(fallback), err: err}
			}
			return detailMsg{reqID: id, view: v, key: key, title: nonEmpty(d.CleanName(), title),
				rows: containerRows(d, fallback)}
		})

	case viewImages:
		img := m.filteredImages()[cur]
		repo, tag := imageRepoTag(img)
		key = img.ID
		if repo != "<none>" {
			key = repo + ":" + tag
		}
		title = repo + ":" + tag
		fallback := img
		m.setStatus(levelInfo, "Inspecting image "+title+"…")
		return m, tea.Batch(spinnerTick(), func() tea.Msg {
			ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
			defer cancel()
			d, err := client.ImageInspect(ctx, key)
			if err != nil {
				if d2, err2 := client.ImageInspect(ctx, docker.ShortID(fallback.ID)); err2 == nil {
					return detailMsg{reqID: id, view: v, key: key, title: title, rows: imageRows(d2, fallback)}
				}
				return detailMsg{reqID: id, view: v, key: key, title: title,
					rows: imageRowsFromSummary(fallback), err: err}
			}
			return detailMsg{reqID: id, view: v, key: key, title: title, rows: imageRows(d, fallback)}
		})

	case viewNetworks:
		n := m.filteredNetworks()[cur]
		key, title = nonEmpty(n.ID, n.Name), n.Name
		fallback := n
		m.setStatus(levelInfo, "Inspecting network "+title+"…")
		return m, tea.Batch(spinnerTick(), func() tea.Msg {
			ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
			defer cancel()
			d, err := client.NetworkInspect(ctx, key)
			if err != nil {
				return detailMsg{reqID: id, view: v, key: key, title: title,
					rows: networkRows(&fallback), err: err}
			}
			return detailMsg{reqID: id, view: v, key: key, title: nonEmpty(d.Name, title), rows: networkRows(d)}
		})

	default:
		vol := m.filteredVolumes()[cur]
		key, title = vol.Name, vol.Name
		fallback := vol
		m.setStatus(levelInfo, "Inspecting volume "+title+"…")
		return m, tea.Batch(spinnerTick(), func() tea.Msg {
			ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
			defer cancel()
			d, err := client.VolumeInspect(ctx, key)
			if err != nil {
				return detailMsg{reqID: id, view: v, key: key, title: title,
					rows: volumeRows(&fallback), err: err}
			}
			return detailMsg{reqID: id, view: v, key: key, title: nonEmpty(d.Name, title), rows: volumeRows(d)}
		})
	}
}

// reinspectCmd re-runs the inspection backing the open detail pane.
func (m *Model) reinspectCmd() tea.Cmd {
	if m.detail.key == "" {
		return nil
	}
	m.reqSeq++
	id := m.reqSeq
	m.detail.reqID = id
	client := m.client
	v := m.detail.view
	key := m.detail.key
	title := m.detail.title

	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
		defer cancel()
		switch v {
		case viewContainers:
			d, err := client.ContainerInspect(ctx, key)
			if err != nil {
				return detailMsg{reqID: id, view: v, key: key, title: title, err: err}
			}
			return detailMsg{reqID: id, view: v, key: key, title: nonEmpty(d.CleanName(), title),
				rows: containerRows(d, docker.Container{})}
		case viewImages:
			d, err := client.ImageInspect(ctx, key)
			if err != nil {
				return detailMsg{reqID: id, view: v, key: key, title: title, err: err}
			}
			return detailMsg{reqID: id, view: v, key: key, title: title, rows: imageRows(d, docker.Image{})}
		case viewNetworks:
			d, err := client.NetworkInspect(ctx, key)
			if err != nil {
				return detailMsg{reqID: id, view: v, key: key, title: title, err: err}
			}
			return detailMsg{reqID: id, view: v, key: key, title: nonEmpty(d.Name, title), rows: networkRows(d)}
		default:
			d, err := client.VolumeInspect(ctx, key)
			if err != nil {
				return detailMsg{reqID: id, view: v, key: key, title: title, err: err}
			}
			return detailMsg{reqID: id, view: v, key: key, title: nonEmpty(d.Name, title), rows: volumeRows(d)}
		}
	}
}

// openLogs shows the logs of the selected container.
func (m Model) openLogs() (tea.Model, tea.Cmd) {
	if m.view != viewContainers {
		m.setStatus(levelWarn, "Logs are available in the Containers view (press c)")
		return m, nil
	}
	if m.filteredCount() == 0 {
		m.setStatus(levelWarn, "No container selected")
		return m, nil
	}
	c := m.filteredContainers()[m.lists[viewContainers].cursor]
	return m.openLogsFor(c.ID, c.Name())
}

// openLogsFor fetches the logs of a specific container.
func (m Model) openLogsFor(id, name string) (tea.Model, tea.Cmd) {
	m.reqSeq++
	reqID := m.reqSeq
	m.logs.reqID = reqID
	m.loading++
	m.setStatus(levelInfo, "Fetching logs for "+name+"…")

	client := m.client
	return m, tea.Batch(spinnerTick(), func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
		defer cancel()
		raw, err := client.ContainerLogs(ctx, id)
		return logsMsg{reqID: reqID, id: id, name: name, raw: raw, err: err}
	})
}

// relogCmd re-fetches the logs currently on screen.
func (m *Model) relogCmd() tea.Cmd {
	if m.logs.containerID == "" {
		return nil
	}
	m.reqSeq++
	reqID := m.reqSeq
	m.logs.reqID = reqID
	client := m.client
	id, name := m.logs.containerID, m.logs.containerName
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), requestTimeout)
		defer cancel()
		raw, err := client.ContainerLogs(ctx, id)
		return logsMsg{reqID: reqID, id: id, name: name, raw: raw, err: err}
	}
}

// nonEmpty returns the first non-empty string.
func nonEmpty(vals ...string) string {
	for _, v := range vals {
		if strings.TrimSpace(v) != "" {
			return v
		}
	}
	return ""
}
