package ui

import "github.com/tooln/tooln/internal/pkgs"

// mode identifies the top-level interaction state of the TUI.
type mode int

const (
	modeBrowse  mode = iota // browsing the package list
	modeFilter              // filtering the current list
	modeSearch              // searching a repository for a package to install
	modeConfirm             // confirming a mutating action
	modeHelp                // showing the help page
)

// focusArea selects which panel receives navigation keys in browse mode.
type focusArea int

const (
	focusList    focusArea = iota
	focusDetails           // details pane scrolling
)

// packagesLoadedMsg is delivered when a manager's package list finishes loading.
type packagesLoadedMsg struct {
	manager  string
	packages []pkgs.Package
	err      error
}

// detailsLoadedMsg is delivered when package details finish loading.
type detailsLoadedMsg struct {
	seq  int
	info *pkgs.PackageInfo
	err  error
}

// debounceMsg fires after the details debounce delay elapses.
type debounceMsg struct {
	seq int
}

// searchDebounceMsg fires after the search debounce delay elapses.
type searchDebounceMsg struct {
	seq int
}

// searchDoneMsg is delivered when a repository search finishes.
type searchDoneMsg struct {
	query   string
	results []pkgs.SearchResult
	err     error
}

// opDoneMsg is delivered when an install/uninstall/upgrade operation finishes.
type opDoneMsg struct {
	manager string
	action  string
	target  string
	result  pkgs.OpResult
	err     error
}
