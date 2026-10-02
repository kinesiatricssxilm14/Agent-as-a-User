package main

import (
	"bufio"
	"errors"
	"os/exec"
	"time"

	tea "github.com/charmbracelet/bubbletea"
)

// loadResultMsg carries the outcome of a package list load/refresh.
type loadResultMsg struct {
	installed  map[string]Package
	available  map[string]Package
	upgradable map[string]Package
	err        error
}

// loadPackagesCmd performs a full load of installed, available and upgradable
// package data from the real package manager.
func loadPackagesCmd() tea.Cmd {
	return func() tea.Msg {
		installed, e1 := loadInstalled()
		available, e2 := loadAvailable()
		upgradable, e3 := loadUpgradable()
		return loadResultMsg{
			installed:  installed,
			available:  available,
			upgradable: upgradable,
			err:        errors.Join(e1, e2, e3),
		}
	}
}

// refreshPackagesCmd reloads only installed + upgradable data, reusing the
// already-loaded available map (the repository index does not change on
// install/remove/upgrade, so this avoids a slow dumpavail).
func refreshPackagesCmd(available map[string]Package) tea.Cmd {
	return func() tea.Msg {
		installed, e1 := loadInstalled()
		upgradable, e2 := loadUpgradable()
		return loadResultMsg{
			installed:  installed,
			available:  available,
			upgradable: upgradable,
			err:        errors.Join(e1, e2),
		}
	}
}

// detailsDebounceMsg fires after the selection has settled; only the latest
// request id is honoured.
type detailsDebounceMsg struct {
	reqID int
}

func debounceCmd(reqID int, delay time.Duration) tea.Cmd {
	return tea.Tick(delay, func(time.Time) tea.Msg {
		return detailsDebounceMsg{reqID: reqID}
	})
}

type detailsResultMsg struct {
	reqID   int
	details PackageDetails
}

func fetchDetailsCmd(p Package, reqID int) tea.Cmd {
	return func() tea.Msg {
		return detailsResultMsg{reqID: reqID, details: fetchDetails(p)}
	}
}

// runningProcess streams a child process's combined output line by line.
type runningProcess struct {
	cmd     *exec.Cmd
	scanner *bufio.Scanner
}

type startedMsg struct {
	proc *runningProcess
}

type lineMsg string

type doneMsg struct {
	err error
}

func startAptProcess(name string, args ...string) tea.Cmd {
	return func() tea.Msg {
		cmd := exec.Command(name, args...)
		cmd.Env = aptEnv()
		stdout, err := cmd.StdoutPipe()
		if err != nil {
			return doneMsg{err: err}
		}
		cmd.Stderr = cmd.Stdout
		if err := cmd.Start(); err != nil {
			return doneMsg{err: err}
		}
		sc := bufio.NewScanner(stdout)
		sc.Buffer(make([]byte, 64*1024), 1024*1024)
		return startedMsg{proc: &runningProcess{cmd: cmd, scanner: sc}}
	}
}

func (p *runningProcess) nextCmd() tea.Cmd {
	return func() tea.Msg {
		if p.scanner.Scan() {
			return lineMsg(p.scanner.Text())
		}
		return doneMsg{err: p.cmd.Wait()}
	}
}
