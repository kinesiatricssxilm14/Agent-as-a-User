package main

import tea "github.com/charmbracelet/bubbletea"

// Every message the model handles. Keeping them in one file makes the state
// machine in update.go easy to follow.

// installedLoadedMsg carries a fresh dpkg-query scan.
type installedLoadedMsg struct {
	pkgs []installedPkg
	err  error
}

// availableLoadedMsg carries a fresh apt-cache dumpavail scan.
type availableLoadedMsg struct {
	pkgs []availablePkg
	err  error
}

// upgradableLoadedMsg carries apt's own upgrade plan.
type upgradableLoadedMsg struct {
	cands []upgradeCandidate
	err   error
}

// policyLoadedMsg carries candidate versions for installed packages.
type policyLoadedMsg struct {
	entries []policyEntry
	err     error
}

// aptListsMsg reports whether any package index is present. A fresh container
// has none, in which case nothing is installable until apt-get update runs.
type aptListsMsg struct {
	present bool
}

// detailsLoadedMsg carries a parsed control record for one package. gen is the
// generation counter at request time: responses for a selection the user has
// already moved away from are discarded.
type detailsLoadedMsg struct {
	gen     int
	name    string
	details *pkgDetails
	err     error
}

// opLineMsg is one line of live output from a running apt operation.
type opLineMsg struct {
	line   string
	stderr bool
}

// opDoneMsg reports that a running operation finished.
type opDoneMsg struct {
	op  operation
	err error
}

// statusMsg sets the transient status line.
type statusMsg struct {
	text  string
	level statusLevel
}

// statusExpiredMsg clears a transient status line if it is still the current
// one. The token guards against an older timer clearing a newer message.
type statusExpiredMsg struct {
	token int
}

type statusLevel int

const (
	statusInfo statusLevel = iota
	statusSuccess
	statusWarn
	statusError
)

// infoStatus is a convenience constructor for a plain status message command.
func infoStatus(text string) tea.Cmd {
	return func() tea.Msg { return statusMsg{text: text, level: statusInfo} }
}

func errStatus(text string) tea.Cmd {
	return func() tea.Msg { return statusMsg{text: text, level: statusError} }
}
