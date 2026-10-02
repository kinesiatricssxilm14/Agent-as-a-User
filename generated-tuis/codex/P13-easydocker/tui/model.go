package tui

import (
	"context"
	"fmt"
	"sort"
	"strings"
	"time"

	tea "github.com/charmbracelet/bubbletea"

	"toolm/dockerapi"
)

type resource int

const (
	containers resource = iota
	images
	networks
	volumes
)

type screen int

const (
	listScreen screen = iota
	detailScreen
	logsScreen
	helpScreen
)

type loadMsg struct {
	kind resource
	data any
	err  error
}

type detailMsg struct {
	kind resource
	data any
	err  error
}

type logsMsg struct {
	name string
	text string
	err  error
}

type Model struct {
	client *dockerapi.Client
	width  int
	height int

	active       resource
	screen       screen
	returnScreen screen
	cursor       [4]int
	offset       [4]int

	containerItems []dockerapi.Container
	imageItems     []dockerapi.Image
	networkItems   []dockerapi.Network
	volumeItems    []dockerapi.Volume
	warnings       []string

	containerDetail *dockerapi.ContainerInspect
	imageDetail     *dockerapi.Image
	networkDetail   *dockerapi.Network
	volumeDetail    *dockerapi.Volume
	detailTitle     string
	logsTitle       string
	textLines       []string
	textOffset      int

	loading [4]bool
	lastErr [4]error
	status  string

	filtering bool
	filter    string
}

func New(client *dockerapi.Client) Model {
	m := Model{client: client, active: containers, screen: listScreen}
	for i := range m.loading {
		m.loading[i] = true
	}
	return m
}

func (m Model) Init() tea.Cmd {
	return tea.Batch(m.load(containers), m.load(images), m.load(networks), m.load(volumes))
}

func (m Model) load(kind resource) tea.Cmd {
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
		defer cancel()
		switch kind {
		case containers:
			items, err := m.client.Containers(ctx)
			sort.Slice(items, func(i, j int) bool { return containerName(items[i]) < containerName(items[j]) })
			return loadMsg{kind: kind, data: items, err: err}
		case images:
			items, err := m.client.Images(ctx)
			sort.Slice(items, func(i, j int) bool { return imageRepository(items[i]) < imageRepository(items[j]) })
			return loadMsg{kind: kind, data: items, err: err}
		case networks:
			items, err := m.client.Networks(ctx)
			sort.Slice(items, func(i, j int) bool { return items[i].Name < items[j].Name })
			return loadMsg{kind: kind, data: items, err: err}
		case volumes:
			items, warnings, err := m.client.Volumes(ctx)
			sort.Slice(items, func(i, j int) bool { return items[i].Name < items[j].Name })
			return loadMsg{kind: kind, data: struct {
				Items    []dockerapi.Volume
				Warnings []string
			}{items, warnings}, err: err}
		default:
			return loadMsg{kind: kind, err: fmt.Errorf("unknown resource")}
		}
	}
}

func (m Model) loadDetail() tea.Cmd {
	kind := m.active
	switch kind {
	case containers:
		items := m.filteredContainers()
		if len(items) == 0 {
			return nil
		}
		item := items[clamp(m.cursor[kind], 0, len(items)-1)]
		return func() tea.Msg {
			ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
			defer cancel()
			value, err := m.client.Container(ctx, item.ID)
			return detailMsg{kind: kind, data: value, err: err}
		}
	case images:
		items := m.filteredImages()
		if len(items) == 0 {
			return nil
		}
		item := items[clamp(m.cursor[kind], 0, len(items)-1)]
		return func() tea.Msg { return detailMsg{kind: kind, data: item} }
	case networks:
		items := m.filteredNetworks()
		if len(items) == 0 {
			return nil
		}
		item := items[clamp(m.cursor[kind], 0, len(items)-1)]
		return func() tea.Msg { return detailMsg{kind: kind, data: item} }
	case volumes:
		items := m.filteredVolumes()
		if len(items) == 0 {
			return nil
		}
		item := items[clamp(m.cursor[kind], 0, len(items)-1)]
		return func() tea.Msg {
			ctx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
			defer cancel()
			value, err := m.client.Volume(ctx, item.Name)
			return detailMsg{kind: kind, data: value, err: err}
		}
	}
	return nil
}

func (m Model) loadLogs() tea.Cmd {
	items := m.filteredContainers()
	if len(items) == 0 {
		return nil
	}
	item := items[clamp(m.cursor[containers], 0, len(items)-1)]
	name := containerName(item)
	return func() tea.Msg {
		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		defer cancel()
		text, err := m.client.ContainerLogs(ctx, item.ID)
		return logsMsg{name: name, text: text, err: err}
	}
}

func (m Model) Update(msg tea.Msg) (tea.Model, tea.Cmd) {
	switch msg := msg.(type) {
	case tea.WindowSizeMsg:
		m.width, m.height = msg.Width, msg.Height
		m.keepVisible()
		return m, nil
	case loadMsg:
		m.loading[msg.kind] = false
		m.lastErr[msg.kind] = msg.err
		if msg.err != nil {
			m.status = msg.err.Error()
			return m, nil
		}
		switch msg.kind {
		case containers:
			m.containerItems = msg.data.([]dockerapi.Container)
		case images:
			m.imageItems = msg.data.([]dockerapi.Image)
		case networks:
			m.networkItems = msg.data.([]dockerapi.Network)
		case volumes:
			value := msg.data.(struct {
				Items    []dockerapi.Volume
				Warnings []string
			})
			m.volumeItems, m.warnings = value.Items, value.Warnings
		}
		m.status = fmt.Sprintf("Loaded %d %s", m.itemCount(msg.kind), resourceName(msg.kind))
		m.normalizeCursor(msg.kind)
		return m, nil
	case detailMsg:
		m.loading[msg.kind] = false
		if msg.err != nil {
			m.status = msg.err.Error()
			m.screen = listScreen
			return m, nil
		}
		m.textOffset = 0
		switch msg.kind {
		case containers:
			v := msg.data.(dockerapi.ContainerInspect)
			m.containerDetail = &v
			m.detailTitle = cleanName(v.Name)
		case images:
			v := msg.data.(dockerapi.Image)
			m.imageDetail = &v
			m.detailTitle = imageRepository(v)
		case networks:
			v := msg.data.(dockerapi.Network)
			m.networkDetail = &v
			m.detailTitle = v.Name
		case volumes:
			v := msg.data.(dockerapi.Volume)
			m.volumeDetail = &v
			m.detailTitle = v.Name
		}
		m.screen = detailScreen
		m.status = "Detail loaded"
		return m, nil
	case logsMsg:
		m.loading[containers] = false
		if msg.err != nil {
			m.status = msg.err.Error()
			m.screen = listScreen
			return m, nil
		}
		m.logsTitle = msg.name
		m.textLines = splitLines(msg.text)
		m.textOffset = max(0, len(wrapAll(m.logDisplayLines(), max(10, m.width-2)))-m.contentHeight())
		m.screen = logsScreen
		m.status = fmt.Sprintf("Complete log: %d lines", len(m.textLines))
		return m, nil
	case tea.KeyMsg:
		return m.handleKey(msg)
	}
	return m, nil
}

func (m Model) handleKey(key tea.KeyMsg) (tea.Model, tea.Cmd) {
	k := key.String()
	if m.filtering {
		switch k {
		case "esc":
			m.filtering = false
			m.filter = ""
			m.normalizeCursor(m.active)
		case "enter":
			m.filtering = false
			m.normalizeCursor(m.active)
		case "backspace", "ctrl+h":
			if len(m.filter) > 0 {
				m.filter = m.filter[:len(m.filter)-1]
				m.cursor[m.active] = 0
				m.offset[m.active] = 0
			}
		case "ctrl+u":
			m.filter = ""
			m.cursor[m.active] = 0
			m.offset[m.active] = 0
		default:
			if len(key.Runes) > 0 && !key.Alt {
				m.filter += string(key.Runes)
				m.cursor[m.active] = 0
				m.offset[m.active] = 0
			}
		}
		return m, nil
	}

	if k == "ctrl+c" || (k == "q" && m.screen == listScreen) {
		return m, tea.Quit
	}
	if k == "?" {
		if m.screen == helpScreen {
			m.screen = m.returnScreen
		} else {
			m.returnScreen = m.screen
			m.screen = helpScreen
			m.textOffset = 0
		}
		return m, nil
	}
	if m.screen == helpScreen {
		if k == "esc" || k == "q" || k == "enter" {
			m.screen = m.returnScreen
			m.textOffset = 0
			return m, nil
		}
		m.scrollText(k, len(helpLines()))
		return m, nil
	}
	if m.screen == detailScreen || m.screen == logsScreen {
		if k == "esc" || k == "backspace" || k == "q" {
			m.screen = listScreen
			m.textOffset = 0
			return m, nil
		}
		m.scrollText(k, m.currentTextLength())
		return m, nil
	}

	switch k {
	case "tab", "right":
		m.switchResource(1)
	case "shift+tab", "left":
		m.switchResource(-1)
	case "1":
		m.setResource(containers)
	case "2":
		m.setResource(images)
	case "3":
		m.setResource(networks)
	case "4":
		m.setResource(volumes)
	case "up", "k":
		m.move(-1)
	case "down", "j":
		m.move(1)
	case "pgup", "ctrl+u":
		m.move(-m.listHeight())
	case "pgdown", "ctrl+d":
		m.move(m.listHeight())
	case "home", "g":
		m.cursor[m.active] = 0
		m.keepVisible()
	case "end", "G":
		m.cursor[m.active] = max(0, m.itemCount(m.active)-1)
		m.keepVisible()
	case "/":
		m.filtering = true
		m.filter = ""
		m.cursor[m.active] = 0
		m.offset[m.active] = 0
	case "r":
		m.loading[m.active] = true
		m.lastErr[m.active] = nil
		m.status = "Refreshing…"
		return m, m.load(m.active)
	case "enter", "d":
		if m.itemCount(m.active) > 0 {
			m.loading[m.active] = true
			m.status = "Loading detail…"
			return m, m.loadDetail()
		}
	case "l":
		if m.active == containers && m.itemCount(containers) > 0 {
			m.loading[containers] = true
			m.status = "Loading complete log…"
			return m, m.loadLogs()
		}
	}
	return m, nil
}

func (m *Model) switchResource(delta int) {
	n := (int(m.active) + delta + 4) % 4
	m.setResource(resource(n))
}
func (m *Model) setResource(kind resource) {
	m.active = kind
	m.filter = ""
	m.filtering = false
	m.keepVisible()
}
func (m *Model) move(delta int) {
	count := m.itemCount(m.active)
	if count == 0 {
		return
	}
	m.cursor[m.active] = clamp(m.cursor[m.active]+delta, 0, count-1)
	m.keepVisible()
}
func (m *Model) normalizeCursor(kind resource) {
	m.cursor[kind] = clamp(m.cursor[kind], 0, max(0, m.itemCount(kind)-1))
	m.keepVisible()
}
func (m *Model) keepVisible() {
	h := m.listHeight()
	if m.cursor[m.active] < m.offset[m.active] {
		m.offset[m.active] = m.cursor[m.active]
	}
	if m.cursor[m.active] >= m.offset[m.active]+h {
		m.offset[m.active] = m.cursor[m.active] - h + 1
	}
	m.offset[m.active] = clamp(m.offset[m.active], 0, max(0, m.itemCount(m.active)-h))
}
func (m *Model) scrollText(key string, total int) {
	h := m.contentHeight()
	switch key {
	case "up", "k":
		m.textOffset--
	case "down", "j":
		m.textOffset++
	case "pgup", "ctrl+u":
		m.textOffset -= h
	case "pgdown", "ctrl+d", " ":
		m.textOffset += h
	case "home", "g":
		m.textOffset = 0
	case "end", "G":
		m.textOffset = max(0, total-h)
	}
	m.textOffset = clamp(m.textOffset, 0, max(0, total-h))
}
func (m Model) listHeight() int    { return max(1, m.height-8) }
func (m Model) contentHeight() int { return max(1, m.height-7) }
func (m Model) currentTextLength() int {
	if m.screen == logsScreen {
		return len(wrapAll(m.logDisplayLines(), max(10, m.width-2)))
	}
	return len(wrapAll(m.detailLines(), max(10, m.width-2)))
}
func (m Model) itemCount(kind resource) int {
	switch kind {
	case containers:
		return len(m.filteredContainers())
	case images:
		return len(m.filteredImages())
	case networks:
		return len(m.filteredNetworks())
	case volumes:
		return len(m.filteredVolumes())
	default:
		return 0
	}
}
func (m Model) filteredContainers() []dockerapi.Container {
	if m.filter == "" {
		return m.containerItems
	}
	var out []dockerapi.Container
	q := strings.ToLower(m.filter)
	for _, v := range m.containerItems {
		if strings.Contains(strings.ToLower(containerName(v)+" "+v.Image+" "+v.Status), q) {
			out = append(out, v)
		}
	}
	return out
}
func (m Model) filteredImages() []dockerapi.Image {
	if m.filter == "" {
		return m.imageItems
	}
	var out []dockerapi.Image
	q := strings.ToLower(m.filter)
	for _, v := range m.imageItems {
		if strings.Contains(strings.ToLower(strings.Join(v.RepoTags, " ")+" "+v.ID), q) {
			out = append(out, v)
		}
	}
	return out
}
func (m Model) filteredNetworks() []dockerapi.Network {
	if m.filter == "" {
		return m.networkItems
	}
	var out []dockerapi.Network
	q := strings.ToLower(m.filter)
	for _, v := range m.networkItems {
		if strings.Contains(strings.ToLower(v.Name+" "+v.Driver), q) {
			out = append(out, v)
		}
	}
	return out
}
func (m Model) filteredVolumes() []dockerapi.Volume {
	if m.filter == "" {
		return m.volumeItems
	}
	var out []dockerapi.Volume
	q := strings.ToLower(m.filter)
	for _, v := range m.volumeItems {
		if strings.Contains(strings.ToLower(v.Name+" "+v.Driver+" "+v.Mountpoint), q) {
			out = append(out, v)
		}
	}
	return out
}
