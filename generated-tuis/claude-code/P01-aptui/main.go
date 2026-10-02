package main

import (
	"fmt"
	"os"
	"strings"

	tea "github.com/charmbracelet/bubbletea"
)

// toola is a terminal interface to apt. Every list it shows is read from dpkg or
// apt, and every install, removal and upgrade is a real apt-get invocation whose
// output is streamed into the interface as it runs.
//
// Usage:
//
//	sudo toola
//
// Root is required for install, remove and upgrade because apt writes to
// /var/lib/dpkg. Browsing, searching and inspecting packages work unprivileged.

const usage = `toola — a terminal interface to apt

Usage:
  sudo toola            browse, search, install, remove and upgrade packages
  toola --help          show this message
  toola --version       show the version

toola runs real apt-get and dpkg commands; it never simulates package state.
Root is needed to change packages, so it is normally started with sudo.

Keys are documented inside the interface: the bottom line lists the common ones
and ? expands the full reference.
`

// version is toola's version. It is a constant rather than build-stamped so that
// `go install .` alone produces a complete binary, as the install instructions
// require.
const version = "1.0.0"

func main() {
	for _, arg := range os.Args[1:] {
		switch arg {
		case "-h", "--help", "help":
			fmt.Print(usage)
			return
		case "-v", "--version", "version":
			fmt.Printf("toola %s\n", version)
			return
		default:
			fmt.Fprintf(os.Stderr, "toola: unrecognised argument %q\n\n%s", arg, usage)
			os.Exit(2)
		}
	}

	// Refuse to start without the tools toola drives, rather than failing later
	// on every action with a confusing error.
	if missing := missingTools(); len(missing) > 0 {
		fmt.Fprintf(os.Stderr,
			"toola: required commands not found: %s\ntoola needs apt and dpkg; it is intended for Debian-based systems.\n",
			strings.Join(missing, ", "))
		os.Exit(1)
	}

	// The alternate screen keeps the user's scrollback intact, and the whole
	// interface is one screen by design, so nothing needs to scroll out of it.
	program := tea.NewProgram(
		newModel(),
		tea.WithAltScreen(),
	)

	if _, err := program.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "toola: %v\n", err)
		os.Exit(1)
	}
}
