package main

import "github.com/charmbracelet/bubbles/key"

// keyMap is the single source of truth for key bindings. The footer and the
// help page both render from it, so a binding can never be documented
// incorrectly: changing the key changes the documentation with it.
type keyMap struct {
	Up       key.Binding
	Down     key.Binding
	PageUp   key.Binding
	PageDown key.Binding
	Home     key.Binding
	End      key.Binding

	Search      key.Binding
	ClearSearch key.Binding
	AcceptInput key.Binding

	Tab    key.Binding
	Filter key.Binding

	FilterAll        key.Binding
	FilterInstalled  key.Binding
	FilterAvailable  key.Binding
	FilterUpgradable key.Binding
	FilterResidual   key.Binding

	Install    key.Binding
	Remove     key.Binding
	Purge      key.Binding
	UpgradeOne key.Binding
	UpgradeAll key.Binding

	Reload key.Binding
	Update key.Binding

	Confirm key.Binding
	Cancel  key.Binding

	Help key.Binding
	Quit key.Binding
}

func defaultKeyMap() keyMap {
	return keyMap{
		Up: key.NewBinding(
			key.WithKeys("up", "k"),
			key.WithHelp("↑/k", "up"),
		),
		Down: key.NewBinding(
			key.WithKeys("down", "j"),
			key.WithHelp("↓/j", "down"),
		),
		PageUp: key.NewBinding(
			key.WithKeys("pgup", "ctrl+b"),
			key.WithHelp("pgup", "page up"),
		),
		PageDown: key.NewBinding(
			key.WithKeys("pgdown", "ctrl+f"),
			key.WithHelp("pgdn", "page down"),
		),
		Home: key.NewBinding(
			key.WithKeys("home", "g"),
			key.WithHelp("home/g", "first"),
		),
		End: key.NewBinding(
			key.WithKeys("end", "G"),
			key.WithHelp("end/G", "last"),
		),

		Search: key.NewBinding(
			key.WithKeys("/"),
			key.WithHelp("/", "search"),
		),
		ClearSearch: key.NewBinding(
			key.WithKeys("esc"),
			key.WithHelp("esc", "leave search / clear"),
		),
		AcceptInput: key.NewBinding(
			key.WithKeys("enter"),
			key.WithHelp("enter", "accept search"),
		),

		Tab: key.NewBinding(
			key.WithKeys("tab"),
			key.WithHelp("tab", "focus list/details/log"),
		),
		Filter: key.NewBinding(
			key.WithKeys("f"),
			key.WithHelp("f", "cycle filter"),
		),

		FilterAll: key.NewBinding(
			key.WithKeys("1"),
			key.WithHelp("1", "all packages"),
		),
		FilterInstalled: key.NewBinding(
			key.WithKeys("2"),
			key.WithHelp("2", "installed only"),
		),
		FilterAvailable: key.NewBinding(
			key.WithKeys("3"),
			key.WithHelp("3", "not installed"),
		),
		FilterUpgradable: key.NewBinding(
			key.WithKeys("4"),
			key.WithHelp("4", "upgradable only"),
		),
		FilterResidual: key.NewBinding(
			key.WithKeys("5"),
			key.WithHelp("5", "residual config"),
		),

		Install: key.NewBinding(
			key.WithKeys("i"),
			key.WithHelp("i", "install"),
		),
		Remove: key.NewBinding(
			key.WithKeys("x"),
			key.WithHelp("x", "remove"),
		),
		Purge: key.NewBinding(
			key.WithKeys("X"),
			key.WithHelp("X", "purge (remove + config)"),
		),
		UpgradeOne: key.NewBinding(
			key.WithKeys("u"),
			key.WithHelp("u", "upgrade selected"),
		),
		UpgradeAll: key.NewBinding(
			key.WithKeys("U"),
			key.WithHelp("U", "upgrade all"),
		),

		Reload: key.NewBinding(
			key.WithKeys("r"),
			key.WithHelp("r", "rescan system"),
		),
		Update: key.NewBinding(
			key.WithKeys("R"),
			key.WithHelp("R", "apt-get update"),
		),

		Confirm: key.NewBinding(
			key.WithKeys("y", "Y"),
			key.WithHelp("y", "confirm"),
		),
		Cancel: key.NewBinding(
			key.WithKeys("n", "N", "esc"),
			key.WithHelp("n/esc", "cancel"),
		),

		Help: key.NewBinding(
			key.WithKeys("?"),
			key.WithHelp("?", "help"),
		),
		Quit: key.NewBinding(
			key.WithKeys("q", "ctrl+c"),
			key.WithHelp("q", "quit"),
		),
	}
}

// ShortHelp is the always-visible footer hint: the keys needed to accomplish
// the core task, nothing more.
func (k keyMap) ShortHelp() []key.Binding {
	return []key.Binding{
		k.Up, k.Down, k.Search, k.Install, k.Remove, k.UpgradeOne, k.Filter, k.Help, k.Quit,
	}
}

// FullHelp is the expanded help, grouped by column. It documents every binding
// so the TUI is self-describing and needs no external documentation.
func (k keyMap) FullHelp() [][]key.Binding {
	return [][]key.Binding{
		{k.Up, k.Down, k.PageUp, k.PageDown, k.Home, k.End},
		{k.Search, k.AcceptInput, k.ClearSearch, k.Tab},
		{k.FilterAll, k.FilterInstalled, k.FilterAvailable, k.FilterUpgradable, k.FilterResidual, k.Filter},
		{k.Install, k.Remove, k.Purge, k.UpgradeOne, k.UpgradeAll},
		{k.Reload, k.Update, k.Confirm, k.Cancel, k.Help, k.Quit},
	}
}

// searchHelp is the footer hint shown while the search box has focus, where
// most global keys are unavailable because they would be typed into the query.
func (k keyMap) searchHelp() []key.Binding {
	return []key.Binding{k.AcceptInput, k.ClearSearch, k.Up, k.Down}
}

// confirmHelp is the footer hint shown while a confirmation is pending.
func (k keyMap) confirmHelp() []key.Binding {
	return []key.Binding{k.Confirm, k.Cancel}
}
