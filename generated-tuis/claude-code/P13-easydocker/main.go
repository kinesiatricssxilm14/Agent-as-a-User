// Command toolm is a terminal user interface for browsing Docker resources:
// containers, images, networks and volumes.
//
// It reads state from a real Docker endpoint — the API socket named by
// DOCKER_HOST, one of the well known local socket paths, or the docker CLI as a
// fallback — and never fabricates data.
package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"strings"

	"toolm/internal/docker"
	"toolm/internal/ui"

	tea "github.com/charmbracelet/bubbletea"
)

// version is the released version of toolm.
const version = "1.0.0"

// usage text shown for -h/--help.
const usageText = `toolm — Docker container management TUI

Usage:
  toolm            start the interactive interface
  toolm --version  print the version and exit
  toolm --help     show this message

Connection:
  toolm talks to the Docker API over the socket named by DOCKER_HOST, or one of
  the standard local paths (/var/run/docker.sock, /run/docker.sock). If no API
  socket answers, it falls back to running the docker CLI.

Keys (a full reference is available with ? inside the interface):
  tab / 1-4      switch between Containers, Images, Networks and Volumes
  up/down, k/j   move the selection
  enter          show details for the selected item
  l              show logs of the selected container
  /              filter the current list
  s / S          change sort column / direction
  r              reload from the Docker endpoint
  ?              full key reference
  q              quit
`

func main() {
	var showVersion, showHelp bool
	flag.BoolVar(&showVersion, "version", false, "print the version and exit")
	flag.BoolVar(&showVersion, "v", false, "print the version and exit")
	flag.BoolVar(&showHelp, "help", false, "show usage information")
	flag.BoolVar(&showHelp, "h", false, "show usage information")
	flag.Usage = func() { fmt.Fprint(os.Stderr, usageText) }
	flag.Parse()

	if showHelp {
		fmt.Print(usageText)
		return
	}
	if showVersion {
		fmt.Printf("toolm %s\n", version)
		return
	}
	if args := flag.Args(); len(args) > 0 {
		fmt.Fprintf(os.Stderr, "toolm: unexpected argument %q\n\n%s", args[0], usageText)
		os.Exit(2)
	}

	client, err := docker.NewClient(context.Background())
	if err != nil {
		fmt.Fprintf(os.Stderr, "toolm: %v\n\n", err)
		fmt.Fprintln(os.Stderr, strings.TrimSpace(`
Set DOCKER_HOST to the API endpoint (for example
unix:///var/run/docker.sock), or make sure the docker CLI can reach a
daemon, then start toolm again.`))
		os.Exit(1)
	}

	program := tea.NewProgram(
		ui.New(client),
		tea.WithAltScreen(),
	)
	if _, err := program.Run(); err != nil {
		fmt.Fprintf(os.Stderr, "toolm: %v\n", err)
		os.Exit(1)
	}
}
