package main

import (
	"fmt"
	"os"

	tea "github.com/charmbracelet/bubbletea"

	"toola/internal/apt"
	"toola/internal/ui"
)

func main() {
	if os.Geteuid() != 0 {
		fmt.Fprintln(os.Stderr, "toola must run as root; launch it with: sudo toola")
		os.Exit(1)
	}

	if _, err := os.Stat("/usr/bin/apt-get"); err != nil {
		fmt.Fprintln(os.Stderr, "toola requires apt-get and a Debian-compatible system")
		os.Exit(1)
	}

	program := tea.NewProgram(
		ui.New(apt.NewSystemBackend()),
		tea.WithAltScreen(),
	)
	if _, err := program.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "toola: %v\n", err)
		os.Exit(1)
	}
}
