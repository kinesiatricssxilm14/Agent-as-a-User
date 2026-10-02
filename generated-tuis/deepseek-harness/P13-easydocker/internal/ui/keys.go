package ui

func helpLines() []string {
	return []string{
		"toolm - Docker Container Management TUI",
		"",
		"NAVIGATION",
		"  Up/Down  or  k/j     Move selection",
		"  PgUp / PgDn          Page through the list or scrollable view",
		"  g / G  (Home/End)    Jump to top / bottom",
		"  Left / Right         Switch resource view",
		"  Tab / Shift+Tab      Switch resource view",
		"  1-4                  Containers / Images / Networks / Volumes",
		"  Enter                Show details for the selected item",
		"  Esc                  Back to the list / cancel",
		"  /                    Filter the current list",
		"",
		"CONTAINERS",
		"  l                    View logs for the selected container",
		"  s                    Start the selected container",
		"  x                    Stop the selected container",
		"  r                    Restart the selected container",
		"",
		"RESOURCES",
		"  d                    Remove the selected resource (asks to confirm)",
		"",
		"OTHER",
		"  ctrl+r               Refresh the current view",
		"  ?                    Show / hide this help",
		"  q  or  ctrl+c        Quit",
		"",
		"All data is read live from the Docker Engine API through the local socket.",
	}
}
