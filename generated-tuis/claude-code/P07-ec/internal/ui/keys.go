package ui

import "github.com/charmbracelet/bubbles/key"

// keyMap holds every binding in the application. Help text lives on the
// bindings themselves so the footer hints and the help screen cannot drift out
// of sync with what the keys actually do.
type keyMap struct {
	// navigation
	Up       key.Binding
	Down     key.Binding
	Left     key.Binding
	Right    key.Binding
	PageUp   key.Binding
	PageDown key.Binding
	Home     key.Binding
	End      key.Binding

	// conflict traversal
	NextConflict key.Binding
	PrevConflict key.Binding

	// resolution
	Ours   key.Binding
	Theirs key.Binding
	Both   key.Binding
	None   key.Binding
	Clear  key.Binding

	// bulk resolution
	AllOurs   key.Binding
	AllTheirs key.Binding
	AllBoth   key.Binding

	// panel focus
	NextPanel  key.Binding
	PrevPanel  key.Binding
	ToggleBase key.Binding

	// actions
	Save    key.Binding
	Commit  key.Binding
	Reload  key.Binding
	Edit    key.Binding
	Abort   key.Binding
	History key.Binding
	Files   key.Binding
	Merge   key.Binding

	// generic
	Confirm key.Binding
	Cancel  key.Binding
	Help    key.Binding
	Quit    key.Binding
}

func newKeyMap() keyMap {
	return keyMap{
		Up: key.NewBinding(
			key.WithKeys("up", "k"),
			key.WithHelp("↑/k", "up"),
		),
		Down: key.NewBinding(
			key.WithKeys("down", "j"),
			key.WithHelp("↓/j", "down"),
		),
		Left: key.NewBinding(
			key.WithKeys("left", "h"),
			key.WithHelp("←/h", "scroll left"),
		),
		Right: key.NewBinding(
			key.WithKeys("right", "l"),
			key.WithHelp("→/l", "scroll right"),
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
			key.WithHelp("home/g", "top"),
		),
		End: key.NewBinding(
			key.WithKeys("end", "G"),
			key.WithHelp("end/G", "bottom"),
		),

		NextConflict: key.NewBinding(
			key.WithKeys("n", "tab"),
			key.WithHelp("n/tab", "next conflict"),
		),
		PrevConflict: key.NewBinding(
			key.WithKeys("p", "shift+tab"),
			key.WithHelp("p/S-tab", "prev conflict"),
		),

		Ours: key.NewBinding(
			key.WithKeys("o", "1"),
			key.WithHelp("o/1", "take ours"),
		),
		Theirs: key.NewBinding(
			key.WithKeys("t", "2"),
			key.WithHelp("t/2", "take theirs"),
		),
		Both: key.NewBinding(
			key.WithKeys("b", "3"),
			key.WithHelp("b/3", "keep both"),
		),
		None: key.NewBinding(
			key.WithKeys("d", "4"),
			key.WithHelp("d/4", "discard both"),
		),
		Clear: key.NewBinding(
			key.WithKeys("u", "0"),
			key.WithHelp("u/0", "undo choice"),
		),

		AllOurs: key.NewBinding(
			key.WithKeys("O"),
			key.WithHelp("O", "all ours"),
		),
		AllTheirs: key.NewBinding(
			key.WithKeys("T"),
			key.WithHelp("T", "all theirs"),
		),
		AllBoth: key.NewBinding(
			key.WithKeys("B"),
			key.WithHelp("B", "all both"),
		),

		NextPanel: key.NewBinding(
			key.WithKeys("ctrl+l", "]"),
			key.WithHelp("]", "focus next panel"),
		),
		PrevPanel: key.NewBinding(
			key.WithKeys("ctrl+h", "["),
			key.WithHelp("[", "focus prev panel"),
		),
		ToggleBase: key.NewBinding(
			key.WithKeys("v"),
			key.WithHelp("v", "toggle base panel"),
		),

		Save: key.NewBinding(
			key.WithKeys("s", "ctrl+s"),
			key.WithHelp("s", "save file"),
		),
		Commit: key.NewBinding(
			key.WithKeys("c"),
			key.WithHelp("c", "commit merge"),
		),
		Reload: key.NewBinding(
			key.WithKeys("r", "ctrl+r"),
			key.WithHelp("r", "reload from disk"),
		),
		Edit: key.NewBinding(
			key.WithKeys("e"),
			key.WithHelp("e", "edit result block"),
		),
		Abort: key.NewBinding(
			key.WithKeys("X"),
			key.WithHelp("X", "abort merge"),
		),
		History: key.NewBinding(
			key.WithKeys("L"),
			key.WithHelp("L", "git history"),
		),
		Files: key.NewBinding(
			key.WithKeys("f"),
			key.WithHelp("f", "conflicted files"),
		),
		Merge: key.NewBinding(
			key.WithKeys("m"),
			key.WithHelp("m", "merge view"),
		),

		Confirm: key.NewBinding(
			key.WithKeys("enter"),
			key.WithHelp("enter", "confirm"),
		),
		Cancel: key.NewBinding(
			key.WithKeys("esc"),
			key.WithHelp("esc", "cancel"),
		),
		Help: key.NewBinding(
			key.WithKeys("?", "F1"),
			key.WithHelp("?", "help"),
		),
		Quit: key.NewBinding(
			key.WithKeys("q", "ctrl+c"),
			key.WithHelp("q", "quit"),
		),
	}
}

// helpSection is a titled group of bindings shown on the help screen.
type helpSection struct {
	Title string
	Keys  []key.Binding
}

// sections returns the full key documentation, grouped by task. This is what
// makes every binding discoverable from inside the TUI.
func (k keyMap) sections() []helpSection {
	return []helpSection{
		{"Navigate", []key.Binding{
			k.Up, k.Down, k.Left, k.Right, k.PageUp, k.PageDown, k.Home, k.End,
		}},
		{"Conflicts", []key.Binding{
			k.NextConflict, k.PrevConflict,
		}},
		{"Resolve current conflict", []key.Binding{
			k.Ours, k.Theirs, k.Both, k.None, k.Clear, k.Edit,
		}},
		{"Resolve every conflict", []key.Binding{
			k.AllOurs, k.AllTheirs, k.AllBoth,
		}},
		{"Panels & views", []key.Binding{
			k.NextPanel, k.PrevPanel, k.ToggleBase, k.Merge, k.Files, k.History,
		}},
		{"Git actions", []key.Binding{
			k.Save, k.Commit, k.Reload, k.Abort,
		}},
		{"General", []key.Binding{
			k.Confirm, k.Cancel, k.Help, k.Quit,
		}},
	}
}

// footerHints returns the compact binding list shown in the status bar for a
// given view, so the most relevant keys are always on screen.
func (k keyMap) footerHints(v view) []key.Binding {
	switch v {
	case viewMerge:
		return []key.Binding{
			k.NextConflict, k.Ours, k.Theirs, k.Both, k.None,
			k.Save, k.Commit, k.Help, k.Quit,
		}
	case viewHistory:
		return []key.Binding{k.Up, k.Down, k.Merge, k.Help, k.Quit}
	case viewFiles:
		return []key.Binding{k.Up, k.Down, k.Confirm, k.Merge, k.Help, k.Quit}
	case viewHelp:
		return []key.Binding{k.Up, k.Down, k.Cancel, k.Quit}
	default:
		return []key.Binding{k.Help, k.Quit}
	}
}
