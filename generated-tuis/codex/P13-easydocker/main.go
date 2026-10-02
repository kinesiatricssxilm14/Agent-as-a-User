package main

import (
	"fmt"
	"os"

	tea "github.com/charmbracelet/bubbletea"

	"toolm/dockerapi"
	"toolm/tui"
)

func main() {
	client, err := dockerapi.NewFromEnv()
	if err != nil {
		fmt.Fprintln(os.Stderr, "toolm:", err)
		os.Exit(1)
	}

	program := tea.NewProgram(tui.New(client), tea.WithAltScreen())
	if _, err := program.Run(); err != nil {
		fmt.Fprintln(os.Stderr, "toolm:", err)
		os.Exit(1)
	}
}
