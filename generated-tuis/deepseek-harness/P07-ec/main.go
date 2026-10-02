// Command toolg is a terminal UI for resolving Git merge conflicts.
//
// It shows a three-way merge (ours / result / theirs), lets you choose a
// resolution strategy per conflict, writes the resolved file back to the
// working tree, and can stage and commit the merge — all with the keyboard.
package main

import (
	"flag"
	"fmt"
	"os"

	tea "github.com/charmbracelet/bubbletea"

	"toolg/internal/gitx"
	"toolg/internal/tui"
)

const version = "1.0.0"

func main() {
	var dir string
	var file string
	flag.StringVar(&dir, "dir", "/bench/data/repo", "working directory (git repository)")
	flag.StringVar(&dir, "d", "/bench/data/repo", "working directory (git repository)")
	flag.StringVar(&file, "file", "", "file to open (default: conflict.py)")
	flag.StringVar(&file, "f", "", "file to open (default: conflict.py)")
	listOnly := flag.Bool("list", false, "list conflicted files and exit")
	showVersion := flag.Bool("version", false, "print version and exit")
	flag.Usage = usage
	flag.Parse()

	if *showVersion {
		fmt.Println("toolg", version)
		return
	}

	if *listOnly {
		runList(dir)
		return
	}

	target := file
	if args := flag.Args(); len(args) > 0 {
		target = args[0]
	}
	if target == "" {
		target = "conflict.py"
	}

	model := tui.New(dir, target)
	p := tea.NewProgram(model, tea.WithAltScreen())
	if _, err := p.Run(); err != nil {
		fmt.Fprintln(os.Stderr, "toolg:", err)
		os.Exit(1)
	}
}

// runList prints the currently conflicted files, one per line.
func runList(dir string) {
	repo, err := gitx.FindRoot(dir)
	if err != nil {
		fmt.Fprintln(os.Stderr, "toolg:", err)
		os.Exit(1)
	}
	files, err := gitx.ConflictedFiles(repo)
	if err != nil {
		fmt.Fprintln(os.Stderr, "toolg:", err)
		os.Exit(1)
	}
	for _, f := range files {
		fmt.Println(f)
	}
}

func usage() {
	fmt.Fprintf(os.Stderr, `toolg — resolve Git merge conflicts in the terminal

Usage:
  toolg [FILE]              open FILE (default: conflict.py) in the working dir
  toolg -d DIR [FILE]       use DIR as the working directory

Flags:
`)
	flag.PrintDefaults()
	fmt.Fprintf(os.Stderr, `
Keys (see the in-app help with "?" for the full reference):
  o/1 ours · t/2 theirs · b/3 both · x/4 none · s save · c commit · g history
`)
}
