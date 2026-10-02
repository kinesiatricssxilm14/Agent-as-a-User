// Command toolm is a Docker container management TUI.
//
// It talks to the Docker Engine API through the local socket (or DOCKER_HOST)
// and provides an interactive interface for browsing and operating on
// containers, images, networks, and volumes.
package main

import (
	"fmt"
	"os"

	tea "github.com/charmbracelet/bubbletea"

	"toolm/internal/docker"
	"toolm/internal/ui"
)

func main() {
	client := docker.New()

	p := tea.NewProgram(ui.New(client), tea.WithAltScreen())
	if _, err := p.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "toolm: %v\n", err)
		os.Exit(1)
	}
}
