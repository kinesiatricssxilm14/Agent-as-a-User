package ui

// keyHelp is a single documented binding shown in the status bar and help view.
type keyHelp struct {
	keys string
	desc string
}

// helpSection groups related bindings in the full help view.
type helpSection struct {
	title string
	keys  []keyHelp
}

// helpSections is the complete key documentation. It is rendered by the help
// view ("?") so every binding is discoverable inside the TUI itself.
var helpSections = []helpSection{
	{
		title: "Global",
		keys: []keyHelp{
			{"?", "toggle this help screen"},
			{"q / ctrl+c", "quit toolm"},
			{"r / ctrl+r", "reload data from the Docker API"},
			{"tab / shift+tab", "next / previous view"},
			{"1 2 3 4", "jump to Containers / Images / Networks / Volumes"},
			{"c i n v", "jump to Containers / Images / Networks / Volumes"},
			{"esc", "close details, log view, help or filter"},
		},
	},
	{
		title: "List navigation",
		keys: []keyHelp{
			{"↑ / k", "move selection up"},
			{"↓ / j", "move selection down"},
			{"pgup / ctrl+b", "page up"},
			{"pgdn / ctrl+f", "page down"},
			{"home / g", "jump to first item"},
			{"end / G", "jump to last item"},
			{"enter", "open details for the selected item"},
			{"l", "open logs (containers view)"},
			{"s", "cycle sort column"},
			{"S", "reverse sort order"},
		},
	},
	{
		title: "Filtering",
		keys: []keyHelp{
			{"/", "start filtering the current list"},
			{"type", "narrow the list as you type"},
			{"enter", "apply the filter and return to the list"},
			{"esc", "clear the filter"},
			{"backspace", "delete the last character"},
		},
	},
	{
		title: "Details pane",
		keys: []keyHelp{
			{"↑ / ↓ / k / j", "scroll detail fields"},
			{"pgup / pgdn", "scroll a page at a time"},
			{"home / end", "jump to top / bottom"},
			{"enter / esc", "return to the list"},
			{"l", "show logs of the inspected container"},
		},
	},
	{
		title: "Log viewer",
		keys: []keyHelp{
			{"↑ / ↓ / k / j", "scroll one line"},
			{"pgup / pgdn", "scroll one page"},
			{"home / g", "jump to the first log line"},
			{"end / G", "jump to the last log line"},
			{"w", "toggle line wrapping"},
			{"←  / →", "scroll sideways when wrapping is off"},
			{"esc / enter", "back to the container list"},
		},
	},
}

// shortHelpFor returns the compact binding list rendered in the status bar for
// the given mode.
func shortHelpFor(mode mode, view view, filtering bool) []keyHelp {
	if filtering {
		return []keyHelp{
			{"type", "filter"},
			{"enter", "apply"},
			{"esc", "clear"},
			{"?", "help"},
		}
	}
	switch mode {
	case modeHelp:
		return []keyHelp{
			{"↑/↓", "scroll"},
			{"?/esc", "close"},
			{"q", "quit"},
		}
	case modeLogs:
		return []keyHelp{
			{"↑/↓", "scroll"},
			{"pgup/pgdn", "page"},
			{"g/G", "top/bottom"},
			{"w", "wrap"},
			{"esc", "back"},
			{"?", "help"},
		}
	case modeDetail:
		out := []keyHelp{{"↑/↓", "scroll"}}
		if view == viewContainers {
			out = append(out, keyHelp{"l", "logs"})
		}
		return append(out, keyHelp{"esc", "back"}, keyHelp{"r", "reload"}, keyHelp{"?", "help"})
	default:
		out := []keyHelp{
			{"↑/↓", "move"},
			{"enter", "details"},
		}
		if view == viewContainers {
			out = append(out, keyHelp{"l", "logs"})
		}
		return append(out,
			keyHelp{"tab", "view"},
			keyHelp{"/", "filter"},
			keyHelp{"s", "sort"},
			keyHelp{"r", "reload"},
			keyHelp{"?", "help"},
			keyHelp{"q", "quit"},
		)
	}
}
