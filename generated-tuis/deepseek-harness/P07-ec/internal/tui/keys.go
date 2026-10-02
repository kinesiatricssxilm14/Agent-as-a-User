package tui

import (
	tea "github.com/charmbracelet/bubbletea"
)

// keyIn reports whether the pressed key matches any of the given names. Names
// are matched against both the printable representation (e.g. "1", "o", "O")
// and the key-type name (e.g. "up", "enter", "ctrl+s").
func keyIn(msg tea.KeyMsg, keys ...string) bool {
	s := msg.String()
	t := msg.Type.String()
	for _, k := range keys {
		if k == s || k == t {
			return true
		}
	}
	return false
}

// clamp bounds an integer into [lo, hi].
func clamp(v, lo, hi int) int {
	if v < lo {
		return lo
	}
	if v > hi {
		return hi
	}
	return v
}

// binding is a single help entry.
type binding struct {
	keys string
	desc string
}

// helpBindings returns the full key reference shown on the help screen.
func helpBindings() []binding {
	return []binding{
		{"↑ / ↓  or  k / j", "select previous / next conflict"},
		{"Tab / Shift+Tab", "cycle panel focus (ours / result / theirs)"},
		{"[ / ]  or  PgUp / PgDn", "scroll the focused panel"},
		{"Home / End", "jump to top / bottom of focused panel"},
		{"1 or o", "resolve current conflict as OURS (HEAD)"},
		{"2 or t", "resolve current conflict as THEIRS (incoming)"},
		{"3 or b", "resolve current conflict as BOTH (ours then theirs)"},
		{"4 or x", "resolve current conflict as NONE (discard)"},
		{"0 or r", "reset current conflict to unresolved"},
		{"O / T / B", "resolve every conflict as ours / theirs / both"},
		{"s  (Ctrl+S)", "save resolved content back to the file"},
		{"c", "stage and commit the merge"},
		{"f", "show the conflicted files list"},
		{"g", "view git commit history"},
		{"? / h", "show this help"},
		{"Esc", "go back / cancel"},
		{"q  (Ctrl+C)", "quit"},
	}
}

// keyHint returns the single-line key summary shown at the bottom of screens.
func keyHint(s screen) string {
	switch s {
	case screenMerge:
		return "↑/↓ conflict · o ours · t theirs · b both · x none · Tab focus · [ ] scroll · s save · c commit · g history · ? help · q quit"
	case screenFiles:
		return "↑/↓ select · Enter open · r refresh · ? help · q quit"
	case screenCommit:
		return "type message · Enter commit · Esc cancel"
	case screenHistory:
		return "↑/↓ scroll · Esc back · q quit"
	case screenHelp:
		return "? / Esc back · q quit"
	default:
		return ""
	}
}
