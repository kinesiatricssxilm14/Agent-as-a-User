package ui

import "github.com/charmbracelet/bubbles/key"

// keyMap holds every binding in the application. Each one carries help text so
// the status bar and the help panel are generated from the same source as the
// behaviour: a binding can never drift out of sync with its documentation.
type keyMap struct {
	Up       key.Binding
	Down     key.Binding
	PageUp   key.Binding
	PageDown key.Binding
	Home     key.Binding
	End      key.Binding

	NextTab key.Binding
	PrevTab key.Binding
	PipTab  key.Binding
	AptTab  key.Binding

	Search    key.Binding
	Filter    key.Binding
	Clear     key.Binding
	Installed key.Binding
	Outdated  key.Binding

	Install key.Binding
	Remove  key.Binding
	Upgrade key.Binding
	Mark    key.Binding
	Unmark  key.Binding
	Refresh key.Binding
	Extras  key.Binding

	ScrollDetailsUp   key.Binding
	ScrollDetailsDown key.Binding
	FocusNext         key.Binding
	ToggleLog         key.Binding
	CopyName          key.Binding

	Help    key.Binding
	Confirm key.Binding
	Cancel  key.Binding
	Quit    key.Binding
}

func newKeyMap() keyMap {
	return keyMap{
		Up: key.NewBinding(
			key.WithKeys("up", "k"), key.WithHelp("↑/k", "up")),
		Down: key.NewBinding(
			key.WithKeys("down", "j"), key.WithHelp("↓/j", "down")),
		PageUp: key.NewBinding(
			key.WithKeys("pgup", "ctrl+b"), key.WithHelp("pgup", "page up")),
		PageDown: key.NewBinding(
			key.WithKeys("pgdown", "ctrl+f"), key.WithHelp("pgdn", "page down")),
		Home: key.NewBinding(
			key.WithKeys("home", "g"), key.WithHelp("g/home", "first")),
		End: key.NewBinding(
			key.WithKeys("end", "G"), key.WithHelp("G/end", "last")),

		NextTab: key.NewBinding(
			key.WithKeys("tab"), key.WithHelp("tab", "next manager")),
		PrevTab: key.NewBinding(
			key.WithKeys("shift+tab"), key.WithHelp("shift+tab", "prev manager")),
		PipTab: key.NewBinding(
			key.WithKeys("1"), key.WithHelp("1", "pip view")),
		AptTab: key.NewBinding(
			key.WithKeys("2"), key.WithHelp("2", "apt view")),

		Search: key.NewBinding(
			key.WithKeys("s", "/"), key.WithHelp("s or /", "search index")),
		Filter: key.NewBinding(
			key.WithKeys("f"), key.WithHelp("f", "filter list")),
		Clear: key.NewBinding(
			key.WithKeys("esc"), key.WithHelp("esc", "clear filter/search")),
		Installed: key.NewBinding(
			key.WithKeys("a"), key.WithHelp("a", "all / installed only")),
		Outdated: key.NewBinding(
			key.WithKeys("o"), key.WithHelp("o", "check for updates")),

		Install: key.NewBinding(
			key.WithKeys("i"), key.WithHelp("i", "install…")),
		Remove: key.NewBinding(
			key.WithKeys("d", "delete"), key.WithHelp("d", "uninstall")),
		Upgrade: key.NewBinding(
			key.WithKeys("U"), key.WithHelp("U", "upgrade")),
		Mark: key.NewBinding(
			key.WithKeys(" ", "x"), key.WithHelp("space", "mark/unmark")),
		Unmark: key.NewBinding(
			key.WithKeys("X"), key.WithHelp("X", "clear marks")),
		Refresh: key.NewBinding(
			key.WithKeys("r", "ctrl+r"), key.WithHelp("r", "refresh (rescan)")),
		Extras: key.NewBinding(
			key.WithKeys("m"), key.WithHelp("m", "maintenance menu")),

		ScrollDetailsUp: key.NewBinding(
			key.WithKeys("K", "ctrl+u"), key.WithHelp("K", "details up")),
		ScrollDetailsDown: key.NewBinding(
			key.WithKeys("J", "ctrl+d"), key.WithHelp("J", "details down")),
		FocusNext: key.NewBinding(
			key.WithKeys("right", "left", "l", "h"), key.WithHelp("←/→", "switch pane")),
		ToggleLog: key.NewBinding(
			key.WithKeys("L"), key.WithHelp("L", "toggle command log")),
		CopyName: key.NewBinding(
			key.WithKeys("y"), key.WithHelp("y", "show full name/version")),

		Help: key.NewBinding(
			key.WithKeys("?"), key.WithHelp("?", "help")),
		Confirm: key.NewBinding(
			key.WithKeys("enter"), key.WithHelp("enter", "confirm")),
		Cancel: key.NewBinding(
			key.WithKeys("esc"), key.WithHelp("esc", "cancel")),
		Quit: key.NewBinding(
			key.WithKeys("q", "ctrl+c"), key.WithHelp("q", "quit")),
	}
}

// ShortHelp is the one-line summary shown in the status bar at all times.
func (k keyMap) ShortHelp() []key.Binding {
	return []key.Binding{
		k.Up, k.Down, k.NextTab, k.Search, k.Filter,
		k.Install, k.Remove, k.Upgrade, k.Refresh, k.Help, k.Quit,
	}
}

// FullHelp is the grouped listing shown on the help screen.
func (k keyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{
		{k.Up, k.Down, k.PageUp, k.PageDown, k.Home, k.End},
		{k.NextTab, k.PrevTab, k.PipTab, k.AptTab, k.FocusNext},
		{k.Search, k.Filter, k.Clear, k.Installed, k.Outdated},
		{k.Install, k.Remove, k.Upgrade, k.Mark, k.Unmark},
		{k.Refresh, k.Extras, k.ToggleLog, k.CopyName},
		{k.ScrollDetailsUp, k.ScrollDetailsDown, k.Confirm, k.Cancel},
		{k.Help, k.Quit},
	}
}

// helpSection is one titled group on the help screen, with prose that explains
// what the keys are for rather than just naming them.
type helpSection struct {
	Title string
	Rows  []helpRow
}

type helpRow struct {
	Keys string
	Desc string
}

// helpSections is the long-form documentation rendered by the help panel.
func (k keyMap) helpSections() []helpSection {
	return []helpSection{
		{Title: "Move around", Rows: []helpRow{
			{"↑ / k", "move the cursor up one package"},
			{"↓ / j", "move the cursor down one package"},
			{"PgUp / Ctrl+B", "scroll the list up one screen"},
			{"PgDn / Ctrl+F", "scroll the list down one screen"},
			{"g / Home", "jump to the first package"},
			{"G / End", "jump to the last package"},
			{"← / → / h / l", "move focus between the list and the details pane"},
			{"J / Ctrl+D", "scroll the details pane down"},
			{"K / Ctrl+U", "scroll the details pane up"},
		}},
		{Title: "Choose a package manager", Rows: []helpRow{
			{"Tab", "switch to the next manager (pip → apt → pip)"},
			{"Shift+Tab", "switch to the previous manager"},
			{"1", "jump straight to the pip view"},
			{"2", "jump straight to the apt view"},
		}},
		{Title: "Find packages", Rows: []helpRow{
			{"f", "filter the current list as you type; Enter keeps it, Esc drops it"},
			{"s or /", "search the package index (PyPI or apt-cache) by name or keyword"},
			{"a", "toggle between all search results and installed packages only"},
			{"o", "ask the manager which installed packages have a newer version"},
			{"Esc", "clear the active filter, or leave search results"},
		}},
		{Title: "Change the environment", Rows: []helpRow{
			{"i", "type a package to install; version pins such as flask==3.0.0 work"},
			{"d / Del", "uninstall the package under the cursor, or every marked package"},
			{"U", "upgrade the package under the cursor to the newest version"},
			{"Space / x", "mark or unmark a package so one action can cover several"},
			{"X", "clear every mark"},
			{"m", "open the maintenance menu for the current manager"},
			{"r", "rescan the environment so the list matches reality"},
		}},
		{Title: "Everything else", Rows: []helpRow{
			{"L", "show or hide the command log with the exact commands run"},
			{"y", "print the selected package's full name and version to the status bar"},
			{"?", "open or close this help"},
			{"Enter", "confirm a prompt, a dialog, or a menu choice"},
			{"Esc", "cancel a prompt, dialog, or menu without acting"},
			{"q / Ctrl+C", "quit tooln"},
		}},
	}
}
