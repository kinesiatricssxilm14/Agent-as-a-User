// Command tooln is a keyboard-driven TUI for managing Python (pip) and system
// (apt) packages inside a Debian container.
package main

import (
	"fmt"
	"os"

	tea "github.com/charmbracelet/bubbletea"
	"github.com/tooln/tooln/internal/ui"
)

func main() {
	// Avoid interactive debconf prompts from apt-get when running headlessly.
	_ = os.Setenv("DEBIAN_FRONTEND", "noninteractive")

	p := tea.NewProgram(
		ui.New(),
		tea.WithAltScreen(),
		tea.WithMouseCellMotion(),
	)
	if _, err := p.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "tooln: %v\n", err)
		os.Exit(1)
	}
}
