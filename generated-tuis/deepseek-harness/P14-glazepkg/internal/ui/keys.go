package ui

import "github.com/charmbracelet/bubbles/key"

// keyMap holds every key binding exposed by tooln. Bindings are shown in the
// footer via ShortHelp and in the help page via FullHelp so users can discover
// the shortcuts without external documentation.
type keyMap struct {
	Up       key.Binding
	Down     key.Binding
	PageUp   key.Binding
	PageDown key.Binding
	Top      key.Binding
	Bottom   key.Binding

	PrevManager key.Binding
	NextManager key.Binding

	Filter  key.Binding
	Search  key.Binding
	Install key.Binding
	Remove  key.Binding
	Upgrade key.Binding
	Refresh key.Binding
	Details key.Binding

	Confirm key.Binding
	Cancel  key.Binding

	Help key.Binding
	Quit key.Binding
}

func newKeyMap() keyMap {
	return keyMap{
		Up:       key.NewBinding(key.WithKeys("up", "k"), key.WithHelp("↑/k", "up")),
		Down:     key.NewBinding(key.WithKeys("down", "j"), key.WithHelp("↓/j", "down")),
		PageUp:   key.NewBinding(key.WithKeys("pgup"), key.WithHelp("pgup", "page up")),
		PageDown: key.NewBinding(key.WithKeys("pgdown"), key.WithHelp("pgdn", "page down")),
		Top:      key.NewBinding(key.WithKeys("home", "g"), key.WithHelp("g", "top")),
		Bottom:   key.NewBinding(key.WithKeys("end", "G"), key.WithHelp("G", "bottom")),

		PrevManager: key.NewBinding(key.WithKeys("shift+tab", "left"), key.WithHelp("⇧tab/←", "prev view")),
		NextManager: key.NewBinding(key.WithKeys("tab", "right"), key.WithHelp("tab/→", "next view")),

		Filter:  key.NewBinding(key.WithKeys("/"), key.WithHelp("/", "filter")),
		Search:  key.NewBinding(key.WithKeys("s", "i"), key.WithHelp("s/i", "search")),
		Install: key.NewBinding(key.WithKeys("i"), key.WithHelp("i", "install")),
		Remove:  key.NewBinding(key.WithKeys("d", "x"), key.WithHelp("d/x", "uninstall")),
		Upgrade: key.NewBinding(key.WithKeys("u"), key.WithHelp("u", "upgrade")),
		Refresh: key.NewBinding(key.WithKeys("r"), key.WithHelp("r", "refresh")),
		Details: key.NewBinding(key.WithKeys("enter"), key.WithHelp("enter", "details")),

		Confirm: key.NewBinding(key.WithKeys("enter", "y"), key.WithHelp("enter/y", "confirm")),
		Cancel:  key.NewBinding(key.WithKeys("esc", "n"), key.WithHelp("esc/n", "cancel")),

		Help: key.NewBinding(key.WithKeys("?"), key.WithHelp("?", "help")),
		Quit: key.NewBinding(key.WithKeys("q", "ctrl+c"), key.WithHelp("q", "quit")),
	}
}
