package main

import (
	"fmt"
	"strings"

	"github.com/charmbracelet/bubbles/list"
	"github.com/charmbracelet/bubbles/spinner"
	"github.com/charmbracelet/bubbles/textinput"
	"github.com/charmbracelet/bubbles/viewport"
	tea "github.com/charmbracelet/bubbletea"
)

// ViewMode selects which subset of packages the list shows.
type ViewMode int

const (
	ModeAll ViewMode = iota
	ModeInstalled
	ModeAvailable
	ModeUpgradable
)

func (m ViewMode) String() string {
	switch m {
	case ModeInstalled:
		return "installed"
	case ModeAvailable:
		return "available"
	case ModeUpgradable:
		return "upgradable"
	default:
		return "all"
	}
}

func (m ViewMode) Next() ViewMode { return (m + 1) % 4 }

// Focus selects which pane owns the keyboard.
type Focus int

const (
	FocusList Focus = iota
	FocusDetails
	FocusSearch
)

func (f Focus) Next() Focus { return (f + 1) % 3 }

// AppState is the high-level interaction state.
type AppState int

const (
	StateBrowse AppState = iota
	StateConfirm
	StateRunning
)

// Action describes a pending or running package operation.
type Action struct {
	Kind   string // install, remove, upgrade, upgrade-all, update
	Target string // package name (empty for upgrade-all / update)
}

func (a Action) describe() string {
	switch a.Kind {
	case "install":
		return fmt.Sprintf("install %s", a.Target)
	case "remove":
		return fmt.Sprintf("remove %s", a.Target)
	case "upgrade":
		return fmt.Sprintf("upgrade %s", a.Target)
	case "upgrade-all":
		return "upgrade all packages"
	case "update":
		return "apt-get update"
	}
	return a.Kind
}

func (a Action) command() []string {
	opts := []string{"-o", "Dpkg::Options::=--force-confdef", "-o", "Dpkg::Options::=--force-confold"}
	switch a.Kind {
	case "install":
		return append(opts, "install", "-y", a.Target)
	case "remove":
		return append(opts, "remove", "-y", a.Target)
	case "upgrade":
		return append(opts, "install", "-y", "--only-upgrade", a.Target)
	case "upgrade-all":
		return append(opts, "upgrade", "-y")
	case "update":
		return []string{"update"}
	}
	return nil
}

type model struct {
	width  int
	height int
	ready  bool

	loading bool
	spinner spinner.Model
	help    bool
	proc    *runningProcess
	output  []string

	// data
	allPackages  []Package
	pkgByName    map[string]Package
	filtered     []Package
	installedMap map[string]Package
	availableMap map[string]Package
	upgradable   map[string]Package

	// UI components
	list        list.Model
	searchInput textinput.Model
	details     viewport.Model
	outputView  viewport.Model

	// geometry (computed by applyLayout)
	listW, listH       int
	detailsW, detailsH int
	outputW, outputH   int

	// selection / details
	mode         ViewMode
	focus        Focus
	detailsName  string
	detailsReqID int

	// interaction
	state          AppState
	confirmMsg     string
	pendingAction  Action
	statusMsg      string
	lastActionDesc string
	justOperated   bool
}

func initialModel() model {
	search := textinput.New()
	search.Placeholder = "type to filter packages…"
	search.Prompt = "Search: "
	search.CharLimit = 128

	l := list.New([]list.Item{}, packageDelegate{}, 0, 0)
	l.SetShowStatusBar(false)
	l.SetShowHelp(false)
	l.SetFilteringEnabled(false)
	l.SetShowTitle(false)

	sp := spinner.New()
	sp.Spinner = spinner.Dot
	sp.Style = spinnerStyle

	det := viewport.New(0, 0)
	det.SetContent("Select a package to view details.\n\nUse ↑/↓ to move, / to search, i to install, r to remove.")

	m := model{
		spinner:      sp,
		list:         l,
		searchInput:  search,
		details:      det,
		outputView:   viewport.New(0, 0),
		mode:         ModeAll,
		focus:        FocusList,
		state:        StateBrowse,
		statusMsg:    "loading package data…",
		loading:      true,
		pkgByName:    map[string]Package{},
		installedMap: map[string]Package{},
		availableMap: map[string]Package{},
		upgradable:   map[string]Package{},
	}
	return m
}

func (m *model) Init() tea.Cmd {
	return tea.Batch(loadPackagesCmd(), m.spinner.Tick)
}

// filterPackages applies the current view mode and search query to the full set.
func filterPackages(all []Package, mode ViewMode, query string) []Package {
	q := strings.ToLower(strings.TrimSpace(query))
	out := make([]Package, 0, len(all))
	for _, p := range all {
		if !matchesMode(p, mode) {
			continue
		}
		if q != "" && !matchesQuery(p, q) {
			continue
		}
		out = append(out, p)
	}
	return out
}

func matchesMode(p Package, mode ViewMode) bool {
	switch mode {
	case ModeInstalled:
		return p.Status == StatusInstalled || p.Status == StatusUpgradable
	case ModeAvailable:
		return p.Status == StatusAvailable
	case ModeUpgradable:
		return p.Status == StatusUpgradable
	default:
		return true
	}
}

func matchesQuery(p Package, q string) bool {
	return strings.Contains(strings.ToLower(p.Name), q) ||
		strings.Contains(strings.ToLower(p.Summary), q)
}

func selectedName(l list.Model) string {
	if l.SelectedItem() == nil {
		return ""
	}
	if p, ok := l.SelectedItem().(Package); ok {
		return p.Name
	}
	return ""
}

func focusHint(f Focus) string {
	switch f {
	case FocusSearch:
		return "search focused — type to filter, enter to finish"
	case FocusDetails:
		return "details focused — ↑/↓ PgUp/PgDn to scroll, esc to return"
	default:
		return ""
	}
}

var _ tea.Model = (*model)(nil)
