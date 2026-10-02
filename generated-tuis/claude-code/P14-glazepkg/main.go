// Command tooln is a terminal user interface for managing the packages on this
// machine: the Python distributions pip knows about, and the Debian system
// packages dpkg and apt know about.
//
// Everything it reports comes from those tools. Listing runs pip list and
// dpkg-query, details run pip show and apt-cache show, and installing or
// removing runs pip install/uninstall and apt-get. The command log (L) shows
// each invocation and its output as it happens.
//
// Usage:
//
//	tooln              start the interface
//	tooln --help       print this summary
//	tooln --version    print the version
package main

import (
	"flag"
	"fmt"
	"os"

	tea "github.com/charmbracelet/bubbletea"

	"tooln/internal/pkgmgr"
	"tooln/internal/ui"
)

// version is the released version; -ldflags can override it at build time.
var version = "1.0.0"

func main() {
	flag.Usage = usage
	showVersion := flag.Bool("version", false, "print the version and exit")
	flag.BoolVar(showVersion, "v", false, "print the version and exit")
	flag.Parse()

	if *showVersion {
		fmt.Println("tooln", version)
		return
	}
	if flag.NArg() > 0 {
		fmt.Fprintf(os.Stderr, "tooln: unexpected argument %q\n\n", flag.Arg(0))
		usage()
		os.Exit(2)
	}

	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, "tooln:", err)
		os.Exit(1)
	}
}

func run() error {
	model := ui.New()

	// Both managers are always registered. Whether each is usable is decided by
	// its own probe once the interface is up, so an environment without apt
	// still gets a working pip view with an explanation on the apt tab.
	log := model.Logger()
	model.SetManagers(
		pkgmgr.NewPip(log),
		pkgmgr.NewApt(log),
	)

	p := tea.NewProgram(model, tea.WithAltScreen())
	_, err := p.Run()
	return err
}

func usage() {
	fmt.Fprint(os.Stderr, `tooln — a terminal interface for pip and apt packages

Usage:
  tooln              browse and manage installed packages
  tooln --version    print the version
  tooln --help       print this message

Once running, press ? for the full list of keys. The essentials:

  tab            switch between the pip and apt views
  up/down        move through the package list
  f              filter the list as you type
  s or /         search the package index by name or keyword
  i              install a package
  d              uninstall the selected package
  U              upgrade the selected package
  o              check which packages have a newer version
  r              rescan the environment
  L              show the commands tooln is running
  q              quit

tooln changes the environment it runs in. Installing and removing system
packages needs root; run it with sudo if you are not already root.
`)
}
