package ui

import (
	"context"
	"strings"
	"time"

	"github.com/charmbracelet/bubbles/textinput"
	tea "github.com/charmbracelet/bubbletea"

	"toolm/internal/docker"
)

type viewKind int

const (
	viewList viewKind = iota
	viewDetail
	viewLogs
	viewHelp
)

type tabKind int

const (
	tabContainers tabKind = iota
	tabImages
	tabNetworks
	tabVolumes
)

const numTabs = 4

var tabTitles = [numTabs]string{"Containers", "Images", "Networks", "Volumes"}

type removeTarget struct {
	tab  tabKind
	id   string
	name string
}

// Model is the root Bubble Tea model.
type Model struct {
	client *docker.Client

	width  int
	height int
	ready  bool

	tab  tabKind
	view viewKind

	containers []docker.Container
	images     []docker.Image
	networks   []docker.Network
	volumes    []docker.Volume

	loaded [numTabs]bool
	tabErr [numTabs]string

	cursor int
	offset int

	detailID      string
	detailName    string
	detailTitle   string
	detailLines   []string
	detailScroll  int
	detailLoading bool

	logsID         string
	logsName       string
	logs           []string
	logsScroll     int
	logsLoading    bool
	logsFromDetail bool

	helpScroll  int
	preHelpView viewKind

	filter      string
	filterInput textinput.Model
	filtering   bool

	confirm *removeTarget

	message    string
	messageErr bool
	msgGen     int
}

// New constructs the root model.
func New(client *docker.Client) Model {
	ti := textinput.New()
	ti.Placeholder = "type to filter…"
	ti.CharLimit = 64
	ti.Prompt = ""
	return Model{
		client:      client,
		view:        viewList,
		tab:         tabContainers,
		filterInput: ti,
		preHelpView: viewList,
	}
}

func (m Model) Init() tea.Cmd {
	return m.loadTabCmd()
}

func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width = msg.Width
		m.height = msg.Height
		m.ready = true
		m.filterInput.Width = maxInt(10, m.width-24)
		return m, nil

	case messageTickMsg:
		if msg.gen == m.msgGen {
			m.message = ""
			m.messageErr = false
		}
		return m, nil

	case containersMsg:
		m.loaded[tabContainers] = true
		if msg.err != nil {
			m.tabErr[tabContainers] = msg.err.Error()
			return m, nil
		}
		m.tabErr[tabContainers] = ""
		m.containers = msg.items
		m.clampCursor()
		return m, nil

	case imagesMsg:
		m.loaded[tabImages] = true
		if msg.err != nil {
			m.tabErr[tabImages] = msg.err.Error()
			return m, nil
		}
		m.tabErr[tabImages] = ""
		m.images = msg.items
		m.clampCursor()
		return m, nil

	case networksMsg:
		m.loaded[tabNetworks] = true
		if msg.err != nil {
			m.tabErr[tabNetworks] = msg.err.Error()
			return m, nil
		}
		m.tabErr[tabNetworks] = ""
		m.networks = msg.items
		m.clampCursor()
		return m, nil

	case volumesMsg:
		m.loaded[tabVolumes] = true
		if msg.err != nil {
			m.tabErr[tabVolumes] = msg.err.Error()
			return m, nil
		}
		m.tabErr[tabVolumes] = ""
		m.volumes = msg.items
		m.clampCursor()
		return m, nil

	case detailMsg:
		m.detailLoading = false
		if msg.err != nil {
			m.view = viewList
			return m, m.setMessage("Error: "+msg.err.Error(), true)
		}
		m.detailID = msg.id
		m.detailName = msg.name
		m.detailTitle = msg.title
		m.detailLines = msg.lines
		m.detailScroll = 0
		m.view = viewDetail
		return m, nil

	case logsMsg:
		m.logsLoading = false
		if msg.err != nil {
			if m.logsFromDetail {
				m.view = viewDetail
			} else {
				m.view = viewList
			}
			return m, m.setMessage("Error: "+msg.err.Error(), true)
		}
		m.logs = msg.lines
		m.logsID = msg.id
		m.logsName = msg.name
		m.logsScroll = 0
		m.view = viewLogs
		return m, nil

	case actionResultMsg:
		if msg.err != nil {
			return m, m.setMessage("Error: "+msg.err.Error(), true)
		}
		m.view = viewList
		m.cursor = 0
		m.offset = 0
		cmd := m.setMessage(msg.desc, false)
		if msg.tab == m.tab {
			m.loaded[m.tab] = false
			return m, tea.Batch(cmd, m.loadTabCmd())
		}
		return m, cmd

	case tea.KeyMsg:
		return m.handleKey(msg)
	}
	return m, nil
}

func (m Model) handleKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	if msg.String() == "ctrl+c" {
		return m, tea.Quit
	}
	if m.filtering {
		return m.handleFilterKey(msg)
	}
	if m.confirm != nil {
		switch msg.String() {
		case "y", "Y", "enter":
			return m, m.executeRemove()
		case "n", "N", "esc":
			m.confirm = nil
			return m, nil
		}
		return m, nil
	}

	switch msg.String() {
	case "tab":
		return m.switchTo(m.nextTab())
	case "shift+tab":
		return m.switchTo(m.prevTab())
	case "1":
		return m.switchTo(tabContainers)
	case "2":
		return m.switchTo(tabImages)
	case "3":
		return m.switchTo(tabNetworks)
	case "4":
		return m.switchTo(tabVolumes)
	case "?":
		return m.toggleHelp()
	case "q":
		return m, tea.Quit
	}

	switch m.view {
	case viewHelp:
		return m.handleHelpKey(msg)
	case viewLogs:
		return m.handleLogsKey(msg)
	case viewDetail:
		return m.handleDetailKey(msg)
	default:
		return m.handleListKey(msg)
	}
}

func (m Model) handleFilterKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "enter":
		m.filter = m.filterInput.Value()
		m.filtering = false
		m.filterInput.Blur()
		m.cursor = 0
		m.offset = 0
		return m, nil
	case "esc":
		m.filterInput.SetValue(m.filter)
		m.filtering = false
		m.filterInput.Blur()
		return m, nil
	}
	var cmd tea.Cmd
	m.filterInput, cmd = m.filterInput.Update(msg)
	return m, cmd
}

func (m Model) handleListKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	switch msg.String() {
	case "up", "k":
		m.moveCursor(-1)
	case "down", "j":
		m.moveCursor(1)
	case "left":
		return m.switchTo(m.prevTab())
	case "right":
		return m.switchTo(m.nextTab())
	case "pgup":
		m.pageMove(-1)
	case "pgdown":
		m.pageMove(1)
	case "home", "g":
		m.cursor = 0
		m.offset = 0
	case "end", "G":
		m.toBottom()
	case "enter":
		return m.openDetail()
	case "/":
		return m.startFilter()
	case "l":
		if m.tab == tabContainers {
			return m.openLogs()
		}
	case "s":
		if m.tab == tabContainers {
			return m.containerStart()
		}
	case "x":
		if m.tab == tabContainers {
			return m.containerStop()
		}
	case "r":
		if m.tab == tabContainers {
			return m.containerRestart()
		}
	case "d":
		m.promptRemoveCurrent()
	case "ctrl+r":
		return m.reload()
	}
	return m, nil
}

func (m Model) handleDetailKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	viewH := m.contentHeight() - 1
	switch msg.String() {
	case "esc":
		m.view = viewList
		return m, nil
	case "up", "k":
		m.detailScroll--
		clampScroll(&m.detailScroll, len(m.detailLines), viewH)
	case "down", "j":
		m.detailScroll++
		clampScroll(&m.detailScroll, len(m.detailLines), viewH)
	case "pgup":
		m.detailScroll -= viewH
		clampScroll(&m.detailScroll, len(m.detailLines), viewH)
	case "pgdown":
		m.detailScroll += viewH
		clampScroll(&m.detailScroll, len(m.detailLines), viewH)
	case "home", "g":
		m.detailScroll = 0
	case "end", "G":
		m.detailScroll = len(m.detailLines) - viewH
		clampScroll(&m.detailScroll, len(m.detailLines), viewH)
	case "l":
		if m.tab == tabContainers {
			return m.openLogsFromDetail()
		}
	case "s":
		if m.tab == tabContainers {
			return m.containerStartFromDetail()
		}
	case "x":
		if m.tab == tabContainers {
			return m.containerStopFromDetail()
		}
	case "r":
		if m.tab == tabContainers {
			return m.containerRestartFromDetail()
		}
	case "d":
		m.promptRemoveDetail()
	case "ctrl+r":
		return m.reloadDetail()
	}
	return m, nil
}

func (m Model) handleLogsKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	viewH := m.contentHeight() - 1
	switch msg.String() {
	case "esc":
		if m.logsFromDetail {
			m.view = viewDetail
		} else {
			m.view = viewList
		}
		return m, nil
	case "up", "k":
		m.logsScroll--
		clampScroll(&m.logsScroll, len(m.logs), viewH)
	case "down", "j":
		m.logsScroll++
		clampScroll(&m.logsScroll, len(m.logs), viewH)
	case "pgup":
		m.logsScroll -= viewH
		clampScroll(&m.logsScroll, len(m.logs), viewH)
	case "pgdown":
		m.logsScroll += viewH
		clampScroll(&m.logsScroll, len(m.logs), viewH)
	case "home", "g":
		m.logsScroll = 0
	case "end", "G":
		m.logsScroll = len(m.logs) - viewH
		clampScroll(&m.logsScroll, len(m.logs), viewH)
	case "ctrl+r":
		return m.reloadLogs()
	}
	return m, nil
}

func (m Model) handleHelpKey(msg tea.KeyMsg) (tea.Model, tea.Cmd) {
	viewH := m.contentHeight() - 1
	switch msg.String() {
	case "esc":
		m.view = m.preHelpView
		return m, nil
	case "up", "k":
		m.helpScroll--
		clampScroll(&m.helpScroll, len(helpLines()), viewH)
	case "down", "j":
		m.helpScroll++
		clampScroll(&m.helpScroll, len(helpLines()), viewH)
	case "pgup":
		m.helpScroll -= viewH
		clampScroll(&m.helpScroll, len(helpLines()), viewH)
	case "pgdown":
		m.helpScroll += viewH
		clampScroll(&m.helpScroll, len(helpLines()), viewH)
	case "home", "g":
		m.helpScroll = 0
	case "end", "G":
		m.helpScroll = len(helpLines()) - viewH
		clampScroll(&m.helpScroll, len(helpLines()), viewH)
	}
	return m, nil
}

func (m Model) toggleHelp() (tea.Model, tea.Cmd) {
	if m.view == viewHelp {
		m.view = m.preHelpView
	} else {
		m.preHelpView = m.view
		m.view = viewHelp
		m.helpScroll = 0
	}
	return m, nil
}

func (m Model) nextTab() tabKind { return tabKind((int(m.tab) + 1) % numTabs) }
func (m Model) prevTab() tabKind { return tabKind((int(m.tab) + numTabs - 1) % numTabs) }

func (m Model) switchTo(t tabKind) (tea.Model, tea.Cmd) {
	if m.tab == t && m.view == viewList {
		return m, nil
	}
	m.tab = t
	m.view = viewList
	m.cursor = 0
	m.offset = 0
	m.confirm = nil
	m.filter = ""
	m.filterInput.SetValue("")
	m.filtering = false
	m.detailScroll = 0
	if !m.loaded[t] {
		return m, m.loadTabCmd()
	}
	m.clampCursor()
	return m, nil
}

func (m Model) loadTabCmd() tea.Cmd {
	switch m.tab {
	case tabContainers:
		return loadContainersCmd(m.client)
	case tabImages:
		return loadImagesCmd(m.client)
	case tabNetworks:
		return loadNetworksCmd(m.client)
	case tabVolumes:
		return loadVolumesCmd(m.client)
	}
	return nil
}

func (m Model) openDetail() (tea.Model, tea.Cmd) {
	cmd := m.detailLoadCmd()
	if cmd == nil {
		return m, nil
	}
	m.detailLoading = true
	m.view = viewDetail
	return m, cmd
}

func (m Model) detailLoadCmd() tea.Cmd {
	switch m.tab {
	case tabContainers:
		c, ok := m.currentContainer()
		if !ok {
			return nil
		}
		return loadContainerDetailCmd(m.client, c.ID, c.Name())
	case tabImages:
		img, ok := m.currentImage()
		if !ok {
			return nil
		}
		return loadImageDetailCmd(m.client, img.ID, imageDisplayName(img))
	case tabNetworks:
		n, ok := m.currentNetwork()
		if !ok {
			return nil
		}
		return loadNetworkDetailCmd(m.client, n.ID, n.Name)
	case tabVolumes:
		v, ok := m.currentVolume()
		if !ok {
			return nil
		}
		return loadVolumeDetailCmd(m.client, v.Name)
	}
	return nil
}

func (m Model) reloadDetail() (tea.Model, tea.Cmd) {
	cmd := loadDetailCmd(m.client, m.tab, m.detailID, m.detailName)
	if cmd == nil {
		return m, nil
	}
	m.detailLoading = true
	return m, cmd
}

func (m Model) reloadLogs() (tea.Model, tea.Cmd) {
	if m.logsID == "" {
		return m, nil
	}
	m.logsLoading = true
	return m, loadLogsCmd(m.client, m.logsID, m.logsName)
}

func (m Model) reload() (tea.Model, tea.Cmd) {
	m.loaded[m.tab] = false
	m.cursor = 0
	m.offset = 0
	return m, m.loadTabCmd()
}

func (m Model) startFilter() (tea.Model, tea.Cmd) {
	m.filtering = true
	m.filterInput.Focus()
	m.filterInput.SetValue(m.filter)
	m.filterInput.CursorEnd()
	return m, nil
}

func (m Model) openLogs() (tea.Model, tea.Cmd) {
	c, ok := m.currentContainer()
	if !ok {
		return m, nil
	}
	return m.startLogs(c.ID, c.Name(), false)
}

func (m Model) openLogsFromDetail() (tea.Model, tea.Cmd) {
	return m.startLogs(m.detailID, m.detailName, true)
}

func (m Model) startLogs(id, name string, fromDetail bool) (tea.Model, tea.Cmd) {
	m.logsLoading = true
	m.logsID = id
	m.logsName = name
	m.logsScroll = 0
	m.logsFromDetail = fromDetail
	m.view = viewLogs
	return m, loadLogsCmd(m.client, id, name)
}

func (m Model) containerStart() (tea.Model, tea.Cmd) {
	c, ok := m.currentContainer()
	if !ok {
		return m, nil
	}
	return m, runCmd(tabContainers, "Started "+c.Name(), func(ctx context.Context) error {
		return m.client.ContainerStart(ctx, c.ID)
	})
}

func (m Model) containerStop() (tea.Model, tea.Cmd) {
	c, ok := m.currentContainer()
	if !ok {
		return m, nil
	}
	return m, runCmd(tabContainers, "Stopped "+c.Name(), func(ctx context.Context) error {
		return m.client.ContainerStop(ctx, c.ID)
	})
}

func (m Model) containerRestart() (tea.Model, tea.Cmd) {
	c, ok := m.currentContainer()
	if !ok {
		return m, nil
	}
	return m, runCmd(tabContainers, "Restarted "+c.Name(), func(ctx context.Context) error {
		return m.client.ContainerRestart(ctx, c.ID)
	})
}

func (m Model) containerStartFromDetail() (tea.Model, tea.Cmd) {
	return m, runCmd(tabContainers, "Started "+m.detailName, func(ctx context.Context) error {
		return m.client.ContainerStart(ctx, m.detailID)
	})
}

func (m Model) containerStopFromDetail() (tea.Model, tea.Cmd) {
	return m, runCmd(tabContainers, "Stopped "+m.detailName, func(ctx context.Context) error {
		return m.client.ContainerStop(ctx, m.detailID)
	})
}

func (m Model) containerRestartFromDetail() (tea.Model, tea.Cmd) {
	return m, runCmd(tabContainers, "Restarted "+m.detailName, func(ctx context.Context) error {
		return m.client.ContainerRestart(ctx, m.detailID)
	})
}

func (m *Model) promptRemoveCurrent() {
	switch m.tab {
	case tabContainers:
		if c, ok := m.currentContainer(); ok {
			m.confirm = &removeTarget{tabContainers, c.ID, c.Name()}
		}
	case tabImages:
		if img, ok := m.currentImage(); ok {
			m.confirm = &removeTarget{tabImages, img.ID, imageDisplayName(img)}
		}
	case tabNetworks:
		if n, ok := m.currentNetwork(); ok {
			m.confirm = &removeTarget{tabNetworks, n.ID, n.Name}
		}
	case tabVolumes:
		if v, ok := m.currentVolume(); ok {
			m.confirm = &removeTarget{tabVolumes, v.Name, v.Name}
		}
	}
}

func (m *Model) promptRemoveDetail() {
	m.confirm = &removeTarget{m.tab, m.detailID, m.detailName}
}

func (m *Model) executeRemove() tea.Cmd {
	if m.confirm == nil {
		return nil
	}
	t := m.confirm
	m.confirm = nil
	switch t.tab {
	case tabContainers:
		return runCmd(tabContainers, "Removed "+t.name, func(ctx context.Context) error {
			return m.client.ContainerRemove(ctx, t.id, false)
		})
	case tabImages:
		return runCmd(tabImages, "Removed "+t.name, func(ctx context.Context) error {
			return m.client.ImageRemove(ctx, t.id, false)
		})
	case tabNetworks:
		return runCmd(tabNetworks, "Removed "+t.name, func(ctx context.Context) error {
			return m.client.NetworkRemove(ctx, t.id)
		})
	case tabVolumes:
		return runCmd(tabVolumes, "Removed "+t.name, func(ctx context.Context) error {
			return m.client.VolumeRemove(ctx, t.name, false)
		})
	}
	return nil
}

func (m *Model) setMessage(s string, isErr bool) tea.Cmd {
	m.message = s
	m.messageErr = isErr
	m.msgGen++
	gen := m.msgGen
	return tea.Tick(4*time.Second, func(time.Time) tea.Msg {
		return messageTickMsg{gen: gen}
	})
}

// ---- list geometry / selection helpers ----

func (m Model) contentHeight() int {
	reserved := 4 // title, tabs, help bar, status line
	if m.confirm != nil {
		reserved++
	}
	h := m.height - reserved
	if h < 1 {
		h = 1
	}
	return h
}

func (m Model) listBodyHeight() int {
	h := m.contentHeight() - 1
	if h < 1 {
		h = 1
	}
	return h
}

func (m Model) filteredIndices() []int {
	q := strings.ToLower(m.filter)
	var out []int
	switch m.tab {
	case tabContainers:
		for i, c := range m.containers {
			if q == "" || strings.Contains(strings.ToLower(containerMatch(c)), q) {
				out = append(out, i)
			}
		}
	case tabImages:
		for i, img := range m.images {
			if q == "" || strings.Contains(strings.ToLower(imageMatch(img)), q) {
				out = append(out, i)
			}
		}
	case tabNetworks:
		for i, n := range m.networks {
			if q == "" || strings.Contains(strings.ToLower(n.Name+" "+n.Driver+" "+n.Scope+" "+n.ID), q) {
				out = append(out, i)
			}
		}
	case tabVolumes:
		for i, v := range m.volumes {
			if q == "" || strings.Contains(strings.ToLower(v.Name+" "+v.Driver+" "+v.Mountpoint), q) {
				out = append(out, i)
			}
		}
	}
	return out
}

func containerMatch(c docker.Container) string {
	return c.Name() + " " + c.Image + " " + c.Status + " " + c.ID
}

func imageMatch(img docker.Image) string {
	return strings.Join(img.RepoTags, " ") + " " + img.ID
}

func (m Model) selectedIndex() int {
	idx := m.filteredIndices()
	if m.cursor < 0 || m.cursor >= len(idx) {
		return -1
	}
	return idx[m.cursor]
}

func (m Model) currentContainer() (docker.Container, bool) {
	i := m.selectedIndex()
	if i < 0 {
		return docker.Container{}, false
	}
	return m.containers[i], true
}

func (m Model) currentImage() (docker.Image, bool) {
	i := m.selectedIndex()
	if i < 0 {
		return docker.Image{}, false
	}
	return m.images[i], true
}

func (m Model) currentNetwork() (docker.Network, bool) {
	i := m.selectedIndex()
	if i < 0 {
		return docker.Network{}, false
	}
	return m.networks[i], true
}

func (m Model) currentVolume() (docker.Volume, bool) {
	i := m.selectedIndex()
	if i < 0 {
		return docker.Volume{}, false
	}
	return m.volumes[i], true
}

func (m *Model) moveCursor(delta int) {
	n := len(m.filteredIndices())
	if n == 0 {
		return
	}
	m.cursor += delta
	if m.cursor < 0 {
		m.cursor = 0
	}
	if m.cursor >= n {
		m.cursor = n - 1
	}
	bodyH := m.listBodyHeight()
	if m.cursor < m.offset {
		m.offset = m.cursor
	}
	if m.cursor >= m.offset+bodyH {
		m.offset = m.cursor - bodyH + 1
	}
}

func (m *Model) pageMove(dir int) {
	n := len(m.filteredIndices())
	if n == 0 {
		return
	}
	bodyH := m.listBodyHeight()
	m.cursor += dir * bodyH
	if m.cursor < 0 {
		m.cursor = 0
	}
	if m.cursor >= n {
		m.cursor = n - 1
	}
	m.offset = m.cursor
	if m.offset > n-bodyH {
		m.offset = n - bodyH
	}
	if m.offset < 0 {
		m.offset = 0
	}
}

func (m *Model) toBottom() {
	n := len(m.filteredIndices())
	if n == 0 {
		return
	}
	bodyH := m.listBodyHeight()
	m.cursor = n - 1
	m.offset = n - bodyH
	if m.offset < 0 {
		m.offset = 0
	}
}

func (m *Model) clampCursor() {
	n := len(m.filteredIndices())
	if n == 0 {
		m.cursor = 0
		m.offset = 0
		return
	}
	if m.cursor >= n {
		m.cursor = n - 1
	}
	if m.cursor < 0 {
		m.cursor = 0
	}
	bodyH := m.listBodyHeight()
	if m.offset > n-bodyH {
		m.offset = n - bodyH
	}
	if m.offset < 0 {
		m.offset = 0
	}
}

func clampScroll(pos *int, lines, viewH int) {
	if viewH < 1 {
		viewH = 1
	}
	max := lines - viewH
	if max < 0 {
		max = 0
	}
	if *pos < 0 {
		*pos = 0
	}
	if *pos > max {
		*pos = max
	}
}
