// Command toolg is a terminal UI for resolving Git merge conflicts.
//
// It opens a file that Git left in a conflicted state, shows the ours, result
// and theirs sides next to each other, and lets the conflict be resolved with
// single keystrokes. The resolved file is written back to the working tree and
// the merge can be committed from inside the tool. Every read and write of
// repository state goes through the real git command.
//
// Usage:
//
//	toolg [flags] [file]
//
// The default file is conflict.py and the default working directory is
// /bench/data/repo when it exists, otherwise the current directory.
package main

import (
	"errors"
	"flag"
	"fmt"
	"os"
	"path/filepath"

	tea "github.com/charmbracelet/bubbletea"

	"github.com/toolg/toolg/internal/gitx"
	"github.com/toolg/toolg/internal/ui"
)

// defaultWorkDir and defaultFile match the documented launch environment:
// `toolg conflict.py` run inside /bench/data/repo.
const (
	defaultWorkDir = "/bench/data/repo"
	defaultFile    = "conflict.py"
)

// version is reported by -version. It is a plain constant rather than a build
// flag so the binary behaves identically however it is installed.
const version = "1.0.0"

func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, "toolg: "+err.Error())
		os.Exit(1)
	}
}

func run() error {
	var (
		workDir     string
		showVersion bool
		logLimit    int
	)

	fs := flag.NewFlagSet("toolg", flag.ContinueOnError)
	fs.StringVar(&workDir, "C", "", "run as if started in this directory (default /bench/data/repo, or . if absent)")
	fs.StringVar(&workDir, "dir", "", "alias for -C")
	fs.IntVar(&logLimit, "log-limit", 200, "maximum number of commits to load in the history view")
	fs.BoolVar(&showVersion, "version", false, "print the version and exit")
	fs.Usage = func() {
		out := fs.Output()
		fmt.Fprintf(out, `toolg %s — Git merge conflict resolution TUI

Usage:
  toolg [flags] [file]

Arguments:
  file    conflict file to open, relative to the working directory
          (default %q)

Flags:
`, version, defaultFile)
		fs.PrintDefaults()
		fmt.Fprintf(out, `
Examples:
  toolg                      open %s in %s
  toolg src/app.go           open a specific conflicted file
  toolg -C /path/to/repo f.c open f.c in another repository

Keys are documented inside the tool: press ? for the full reference.
`, defaultFile, defaultWorkDir)
	}

	if err := fs.Parse(os.Args[1:]); err != nil {
		// flag already reported the problem, and -h is not a failure.
		if errors.Is(err, flag.ErrHelp) {
			return nil
		}
		return err
	}

	if showVersion {
		fmt.Println("toolg " + version)
		return nil
	}

	if fs.NArg() > 1 {
		return fmt.Errorf("expected at most one file argument, got %d; see toolg -h", fs.NArg())
	}

	file := defaultFile
	if fs.NArg() == 1 {
		file = fs.Arg(0)
	}

	resolvedDir, err := resolveWorkDir(workDir, file)
	if err != nil {
		return err
	}

	// Checking git up front turns an obscure runtime failure into a clear
	// message before the alternate screen is entered.
	if err := gitx.Available(); err != nil {
		return err
	}

	model, err := ui.New(ui.Config{
		WorkDir:  resolvedDir,
		File:     file,
		LogLimit: logLimit,
	})
	if err != nil {
		return err
	}

	p := tea.NewProgram(model, tea.WithAltScreen())
	if _, err := p.Run(); err != nil {
		return err
	}
	return nil
}

// resolveWorkDir decides which directory git commands run in.
//
// An explicit -C always wins. Otherwise the documented default is used when it
// exists, which is the container case; falling back to the current directory
// keeps the tool usable during local development. If the file argument is an
// absolute path, its own directory takes precedence so that `toolg /some/repo/f`
// works without also passing -C.
func resolveWorkDir(flagDir, file string) (string, error) {
	if flagDir != "" {
		if err := mustBeDir(flagDir); err != nil {
			return "", err
		}
		return flagDir, nil
	}

	if filepath.IsAbs(file) {
		return filepath.Dir(file), nil
	}

	if err := mustBeDir(defaultWorkDir); err == nil {
		return defaultWorkDir, nil
	}

	cwd, err := os.Getwd()
	if err != nil {
		return "", fmt.Errorf("cannot determine the current directory: %w", err)
	}
	return cwd, nil
}

func mustBeDir(path string) error {
	st, err := os.Stat(path)
	if err != nil {
		return fmt.Errorf("cannot use %s: %w", path, err)
	}
	if !st.IsDir() {
		return fmt.Errorf("%s is not a directory", path)
	}
	return nil
}
